use std::sync::Arc;

// ---------------------------------------------------------------------------
// FunctionContext — evaluation coordinate
// ---------------------------------------------------------------------------

pub trait FunctionContext {
    fn block_x(&self) -> i32;
    fn block_y(&self) -> i32;
    fn block_z(&self) -> i32;
    fn sample_interpolated(&self, _function: &DenseFn) -> Option<f64> {
        None
    }
}

impl FunctionContext for Box<dyn FunctionContext + '_> {
    fn block_x(&self) -> i32 { (**self).block_x() }
    fn block_y(&self) -> i32 { (**self).block_y() }
    fn block_z(&self) -> i32 { (**self).block_z() }
    fn sample_interpolated(&self, function: &DenseFn) -> Option<f64> {
        (**self).sample_interpolated(function)
    }
}

#[derive(Clone, Debug)]
pub struct SinglePointContext {
    pub block_x: i32,
    pub block_y: i32,
    pub block_z: i32,
}

impl FunctionContext for SinglePointContext {
    fn block_x(&self) -> i32 { self.block_x }
    fn block_y(&self) -> i32 { self.block_y }
    fn block_z(&self) -> i32 { self.block_z }
}

impl<'a> FunctionContext for &'a dyn FunctionContext {
    fn block_x(&self) -> i32 { (**self).block_x() }
    fn block_y(&self) -> i32 { (**self).block_y() }
    fn block_z(&self) -> i32 { (**self).block_z() }
    fn sample_interpolated(&self, function: &DenseFn) -> Option<f64> {
        (**self).sample_interpolated(function)
    }
}

/// A block position inside one noise cell. Only an explicit interpolated
/// marker asks this context for a cell interpolation; all other functions
/// observe the exact block coordinate.
pub struct InterpolatedContext {
    pub block_x: i32,
    pub block_y: i32,
    pub block_z: i32,
    pub cell_width: i32,
    pub cell_height: i32,
}

impl InterpolatedContext {
    pub const fn new(
        block_x: i32,
        block_y: i32,
        block_z: i32,
        cell_width: i32,
        cell_height: i32,
    ) -> Self {
        Self { block_x, block_y, block_z, cell_width, cell_height }
    }
}

impl FunctionContext for InterpolatedContext {
    fn block_x(&self) -> i32 { self.block_x }
    fn block_y(&self) -> i32 { self.block_y }
    fn block_z(&self) -> i32 { self.block_z }

    fn sample_interpolated(&self, function: &DenseFn) -> Option<f64> {
        let x0 = self.block_x.div_euclid(self.cell_width) * self.cell_width;
        let y0 = self.block_y.div_euclid(self.cell_height) * self.cell_height;
        let z0 = self.block_z.div_euclid(self.cell_width) * self.cell_width;
        let x1 = x0 + self.cell_width;
        let y1 = y0 + self.cell_height;
        let z1 = z0 + self.cell_width;
        let sample = |x: i32, y: i32, z: i32| {
            function.compute(&SinglePointContext { block_x: x, block_y: y, block_z: z })
        };
        let v000 = sample(x0, y0, z0);
        let v100 = sample(x1, y0, z0);
        let v010 = sample(x0, y1, z0);
        let v110 = sample(x1, y1, z0);
        let v001 = sample(x0, y0, z1);
        let v101 = sample(x1, y0, z1);
        let v011 = sample(x0, y1, z1);
        let v111 = sample(x1, y1, z1);
        let fx = (self.block_x - x0) as f64 / self.cell_width as f64;
        let fy = (self.block_y - y0) as f64 / self.cell_height as f64;
        let fz = (self.block_z - z0) as f64 / self.cell_width as f64;
        let x00 = v000 + (v100 - v000) * fx;
        let x10 = v010 + (v110 - v010) * fx;
        let x01 = v001 + (v101 - v001) * fx;
        let x11 = v011 + (v111 - v011) * fx;
        let y00 = x00 + (x10 - x00) * fy;
        let y01 = x01 + (x11 - x01) * fy;
        Some(y00 + (y01 - y00) * fz)
    }
}

// ---------------------------------------------------------------------------
// Noise sampling trait & handle
// ---------------------------------------------------------------------------

pub trait NoiseSampler: Send + Sync {
    fn sample(&self, x: f64, y: f64, z: f64) -> f64;
    fn max_value(&self) -> f64;
}

#[derive(Clone)]
pub struct NoiseHandle(pub Arc<dyn NoiseSampler>);

impl NoiseHandle {
    pub fn new(sampler: impl NoiseSampler + 'static) -> Self {
        Self(Arc::new(sampler))
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.0.sample(x, y, z)
    }

    pub fn max_value(&self) -> f64 {
        self.0.max_value()
    }
}

// ---------------------------------------------------------------------------
// 2-D simplex noise (Java-compatible for EndIsland)
// ---------------------------------------------------------------------------

const GRAD_2D: [(f64, f64); 8] = [
    (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
    (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0),
];

fn dot2(g: (f64, f64), x: f64, y: f64) -> f64 { g.0 * x + g.1 * y }

// Java LegacyRandomSource LCG
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as i32
    }
    fn next_f64(&mut self) -> f64 {
        let i = self.next() as i64;
        let i = i & 0x7fff_ffff;
        if i == 0 { 0.0 } else { (i as f64) / (0x7fff_ffffi32 as f64) }
    }
}

#[derive(Clone)]
pub struct SimplexNoise2D {
    perm: [i32; 512],
}

impl SimplexNoise2D {
    pub fn new(seed: u64) -> Self {
        let mut rng = Lcg(seed);
        for _ in 0..17292 { rng.next(); }

        let mut perm = [0i32; 256];
        for i in 0..256 { perm[i] = i as i32; }

        for i in (1..256).rev() {
            let j = (rng.next_f64() * (i as f64 + 1.0)).floor() as usize;
            perm.swap(i, j);
        }

        let mut full = [0i32; 512];
        for i in 0..512 { full[i] = perm[i & 255]; }
        SimplexNoise2D { perm: full }
    }

    pub fn get_value(&self, x: f64, y: f64) -> f64 {
        // 0.5 * (sqrt(3) - 1)  and  (3 - sqrt(3)) / 6
        const F2: f64 = 0.3660254037844386;
        const G2: f64 = 0.21132486540518713;

        let s = (x + y) * F2;
        let i = (x + s).floor();
        let j = (y + s).floor();
        let t = (i + j) * G2;

        let x0 = x - (i - t);
        let y0 = y - (j - t);

        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };

        let x1 = x0 - i1 as f64 + G2;
        let y1 = y0 - j1 as f64 + G2;
        let x2 = x0 - 1.0 + 2.0 * G2;
        let y2 = y0 - 1.0 + 2.0 * G2;

        let ii = i as i32 & 255;
        let jj = j as i32 & 255;

        let gi0 = self.perm[(ii + self.perm[jj as usize]) as usize] as usize & 7;
        let gi1 = self.perm[(ii + i1 + self.perm[(jj + j1) as usize]) as usize] as usize & 7;
        let gi2 = self.perm[(ii + 1 + self.perm[(jj + 1) as usize]) as usize] as usize & 7;

        let mut n0 = 0.5 - x0 * x0 - y0 * y0;
        let mut n1 = 0.5 - x1 * x1 - y1 * y1;
        let mut n2 = 0.5 - x2 * x2 - y2 * y2;

        if n0 < 0.0 { n0 = 0.0; } else { n0 = n0 * n0 * n0 * n0 * dot2(GRAD_2D[gi0], x0, y0); }
        if n1 < 0.0 { n1 = 0.0; } else { n1 = n1 * n1 * n1 * n1 * dot2(GRAD_2D[gi1], x1, y1); }
        if n2 < 0.0 { n2 = 0.0; } else { n2 = n2 * n2 * n2 * n2 * dot2(GRAD_2D[gi2], x2, y2); }

        70.0 * (n0 + n1 + n2)
    }
}

// ---------------------------------------------------------------------------
// DenseFn — cloneable dynamic dispatch wrapper
// ---------------------------------------------------------------------------

pub trait DensityFunction: Send + Sync {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64;
    fn min_value(&self) -> f64;
    fn max_value(&self) -> f64;
    fn fill_array(&self, output: &mut [f64], provider: &dyn ContextProvider) {
        for (i, slot) in output.iter_mut().enumerate() {
            *slot = self.compute(&provider.for_index(i));
        }
    }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn;
    fn clone_dyn(&self) -> DenseFn;
}

pub struct DenseFn(pub Box<dyn DensityFunction>);

impl DenseFn {
    pub fn new(f: impl DensityFunction + 'static) -> Self { DenseFn(Box::new(f)) }
}

impl Clone for DenseFn {
    fn clone(&self) -> Self { self.0.clone_dyn() }
}

impl std::ops::Deref for DenseFn {
    type Target = dyn DensityFunction;
    fn deref(&self) -> &Self::Target { &*self.0 }
}

impl DensityFunction for DenseFn {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { self.0.compute(ctx) }
    fn min_value(&self) -> f64 { self.0.min_value() }
    fn max_value(&self) -> f64 { self.0.max_value() }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { self.0.map_children(visitor) }
    fn clone_dyn(&self) -> DenseFn { self.0.clone_dyn() }
}

pub trait Visitor: Send + Sync {
    fn apply(&self, input: DenseFn) -> DenseFn;
    fn visit_noise(&self, noise: NoiseHandle) -> NoiseHandle { noise }
}

pub trait ContextProvider {
    fn for_index(&self, index: usize) -> Box<dyn FunctionContext + '_>;
    fn fill_all_directly(&self, output: &mut [f64], function: &dyn DensityFunction) {
        for (i, slot) in output.iter_mut().enumerate() {
            *slot = function.compute(&self.for_index(i));
        }
    }
}

// ---------------------------------------------------------------------------
// Constant
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Constant(pub f64);

impl DensityFunction for Constant {
    fn compute(&self, _ctx: &dyn FunctionContext) -> f64 { self.0 }
    fn min_value(&self) -> f64 { self.0 }
    fn max_value(&self) -> f64 { self.0 }
    fn fill_array(&self, output: &mut [f64], _provider: &dyn ContextProvider) {
        output.fill(self.0);
    }
    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn { self.clone_dyn() }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Noise functions
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Noise {
    pub noise: NoiseHandle,
    pub xz_scale: f64,
    pub y_scale: f64,
}

impl DensityFunction for Noise {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        self.noise.sample(
            ctx.block_x() as f64 * self.xz_scale,
            ctx.block_y() as f64 * self.y_scale,
            ctx.block_z() as f64 * self.xz_scale,
        )
    }
    fn min_value(&self) -> f64 { -self.max_value() }
    fn max_value(&self) -> f64 { self.noise.max_value() }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Noise {
            noise: visitor.visit_noise(self.noise.clone()),
            xz_scale: self.xz_scale,
            y_scale: self.y_scale,
        }))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Shift noise wrappers
// ---------------------------------------------------------------------------

trait ShiftNoise: DensityFunction {
    fn offset_noise(&self) -> &NoiseHandle;
    fn noise_compute(&self, x: f64, y: f64, z: f64) -> f64 {
        self.offset_noise().sample(x * 0.25, y * 0.25, z * 0.25) * 4.0
    }
    fn shift_min_value(&self) -> f64 { -self.shift_max_value() }
    fn shift_max_value(&self) -> f64 { self.offset_noise().max_value() * 4.0 }
}

macro_rules! impl_shift {
    ($name:ident, $cx:expr, $cy:expr, $cz:expr) => {
        #[derive(Clone)]
        pub struct $name(pub NoiseHandle);

        impl DensityFunction for $name {
            fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
                self.noise_compute(
                    $cx(ctx.block_x() as f64, ctx.block_y() as f64, ctx.block_z() as f64),
                    $cy(ctx.block_x() as f64, ctx.block_y() as f64, ctx.block_z() as f64),
                    $cz(ctx.block_x() as f64, ctx.block_y() as f64, ctx.block_z() as f64),
                )
            }
            fn min_value(&self) -> f64 { self.shift_min_value() }
            fn max_value(&self) -> f64 { self.shift_max_value() }
            fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
                DenseFn(Box::new($name(visitor.visit_noise(self.0.clone()))))
            }
            fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
        }

        impl ShiftNoise for $name {
            fn offset_noise(&self) -> &NoiseHandle { &self.0 }
        }
    };
}

impl_shift!(ShiftA, |x: f64, _y: f64, _z: f64| x, |_: f64, _: f64, _: f64| 0.0, |_: f64, _: f64, z: f64| z);
impl_shift!(ShiftB, |_: f64, _: f64, z: f64| z, |_: f64, _: f64, _: f64| 0.0, |x: f64, _: f64, _: f64| x);
impl_shift!(Shift,  |x: f64, _y: f64, _z: f64| x, |_: f64, y: f64, _: f64| y, |_: f64, _: f64, z: f64| z);

// ---------------------------------------------------------------------------
// ShiftedNoise — noise with offset functions
// ---------------------------------------------------------------------------

pub struct ShiftedNoise {
    pub shift_x: DenseFn,
    pub shift_y: DenseFn,
    pub shift_z: DenseFn,
    pub xz_scale: f64,
    pub y_scale: f64,
    pub noise: NoiseHandle,
}

impl Clone for ShiftedNoise {
    fn clone(&self) -> Self {
        Self {
            shift_x: self.shift_x.clone(),
            shift_y: self.shift_y.clone(),
            shift_z: self.shift_z.clone(),
            xz_scale: self.xz_scale,
            y_scale: self.y_scale,
            noise: self.noise.clone(),
        }
    }
}

impl DensityFunction for ShiftedNoise {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let x = ctx.block_x() as f64 * self.xz_scale + self.shift_x.compute(ctx);
        let y = ctx.block_y() as f64 * self.y_scale + self.shift_y.compute(ctx);
        let z = ctx.block_z() as f64 * self.xz_scale + self.shift_z.compute(ctx);
        self.noise.sample(x, y, z)
    }
    fn min_value(&self) -> f64 { -self.max_value() }
    fn max_value(&self) -> f64 { self.noise.max_value() }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(ShiftedNoise {
            shift_x: visitor.apply(self.shift_x.clone()),
            shift_y: visitor.apply(self.shift_y.clone()),
            shift_z: visitor.apply(self.shift_z.clone()),
            xz_scale: self.xz_scale,
            y_scale: self.y_scale,
            noise: visitor.visit_noise(self.noise.clone()),
        }))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// EndIslandDensityFunction
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct EndIslandDensityFunction {
    pub island_noise: Arc<SimplexNoise2D>,
}

impl EndIslandDensityFunction {
    pub fn new(seed: u64) -> Self {
        Self { island_noise: Arc::new(SimplexNoise2D::new(seed)) }
    }

    fn height_value(&self, section_x: i32, section_z: i32) -> f64 {
        let chunk_x = section_x / 2;
        let chunk_z = section_z / 2;
        let sub_x = section_x % 2;
        let sub_z = section_z % 2;

        let mut doffs = 100.0 - ((section_x * section_x + section_z * section_z) as f64).sqrt() * 8.0;
        doffs = doffs.clamp(-100.0, 80.0);

        for xo in -12..=12 {
            for zo in -12..=12 {
                let total_cx = (chunk_x + xo) as i64;
                let total_cz = (chunk_z + zo) as i64;
                if total_cx * total_cx + total_cz * total_cz > 4096
                    && self.island_noise.get_value(total_cx as f64, total_cz as f64) < -0.9
                {
                    let island_size = ((total_cx.abs() as f64) * 3439.0
                        + (total_cz.abs() as f64) * 147.0) % 13.0
                        + 9.0;
                    let xd = sub_x as f64 - (xo * 2) as f64;
                    let zd = sub_z as f64 - (zo * 2) as f64;
                    let mut ndoffs = 100.0 - (xd * xd + zd * zd).sqrt() * island_size;
                    ndoffs = ndoffs.clamp(-100.0, 80.0);
                    if ndoffs > doffs { doffs = ndoffs; }
                }
            }
        }

        doffs
    }
}

impl DensityFunction for EndIslandDensityFunction {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        (self.height_value(ctx.block_x() / 8, ctx.block_z() / 8) - 8.0) / 128.0
    }
    fn min_value(&self) -> f64 { -0.84375 }
    fn max_value(&self) -> f64 { 0.5625 }
    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn { self.clone_dyn() }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// YClampedGradient
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct YClampedGradient {
    pub from_y: i32,
    pub to_y: i32,
    pub from_value: f64,
    pub to_value: f64,
}

impl DensityFunction for YClampedGradient {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        clamped_map(ctx.block_y() as f64, self.from_y as f64, self.to_y as f64, self.from_value, self.to_value)
    }
    fn min_value(&self) -> f64 { self.from_value.min(self.to_value) }
    fn max_value(&self) -> f64 { self.from_value.max(self.to_value) }
    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn { self.clone_dyn() }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Arithmetic: two-argument
// ---------------------------------------------------------------------------

pub struct Add(pub DenseFn, pub DenseFn);
impl Clone for Add {
    fn clone(&self) -> Self { Add(self.0.clone(), self.1.clone()) }
}
impl DensityFunction for Add {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { self.0.compute(ctx) + self.1.compute(ctx) }
    fn min_value(&self) -> f64 { self.0.min_value() + self.1.min_value() }
    fn max_value(&self) -> f64 { self.0.max_value() + self.1.max_value() }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Add(visitor.apply(self.0.clone()), visitor.apply(self.1.clone()))))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Mul(pub DenseFn, pub DenseFn);
impl Clone for Mul {
    fn clone(&self) -> Self { Mul(self.0.clone(), self.1.clone()) }
}
impl DensityFunction for Mul {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let a = self.0.compute(ctx);
        if a == 0.0 { return 0.0; }
        a * self.1.compute(ctx)
    }
    fn min_value(&self) -> f64 { compute_mul_min(self.0.min_value(), self.0.max_value(), self.1.min_value(), self.1.max_value()) }
    fn max_value(&self) -> f64 { compute_mul_max(self.0.min_value(), self.0.max_value(), self.1.min_value(), self.1.max_value()) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Mul(visitor.apply(self.0.clone()), visitor.apply(self.1.clone()))))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Min(pub DenseFn, pub DenseFn);
impl Clone for Min {
    fn clone(&self) -> Self { Min(self.0.clone(), self.1.clone()) }
}
impl DensityFunction for Min {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { self.0.compute(ctx).min(self.1.compute(ctx)) }
    fn min_value(&self) -> f64 { self.0.min_value().min(self.1.min_value()) }
    fn max_value(&self) -> f64 { self.0.max_value().min(self.1.max_value()) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Min(visitor.apply(self.0.clone()), visitor.apply(self.1.clone()))))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Max(pub DenseFn, pub DenseFn);
impl Clone for Max {
    fn clone(&self) -> Self { Max(self.0.clone(), self.1.clone()) }
}
impl DensityFunction for Max {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { self.0.compute(ctx).max(self.1.compute(ctx)) }
    fn min_value(&self) -> f64 { self.0.min_value().max(self.1.min_value()) }
    fn max_value(&self) -> f64 { self.0.max_value().max(self.1.max_value()) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Max(visitor.apply(self.0.clone()), visitor.apply(self.1.clone()))))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

fn compute_mul_min(min1: f64, max1: f64, min2: f64, max2: f64) -> f64 {
    (min1 * max2).min(max1 * min2).min(min1 * min2).min(max1 * max2)
}
fn compute_mul_max(min1: f64, max1: f64, min2: f64, max2: f64) -> f64 {
    (min1 * max2).max(max1 * min2).max(min1 * min2).max(max1 * max2)
}

// ---------------------------------------------------------------------------
// Arithmetic: unary (pure)
// ---------------------------------------------------------------------------

pub struct Neg(pub DenseFn);
impl Clone for Neg {
    fn clone(&self) -> Self { Neg(self.0.clone()) }
}
impl DensityFunction for Neg {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { -self.0.compute(ctx) }
    fn min_value(&self) -> f64 { -self.0.max_value() }
    fn max_value(&self) -> f64 { -self.0.min_value() }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(Neg(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Abs(pub DenseFn);
impl Clone for Abs {
    fn clone(&self) -> Self { Abs(self.0.clone()) }
}
impl DensityFunction for Abs {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { self.0.compute(ctx).abs() }
    fn min_value(&self) -> f64 { 0.0_f64.max(self.0.min_value()) }
    fn max_value(&self) -> f64 { self.0.min_value().abs().max(self.0.max_value().abs()) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(Abs(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Square(pub DenseFn);
impl Clone for Square {
    fn clone(&self) -> Self { Square(self.0.clone()) }
}
impl DensityFunction for Square {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { let a = self.0.compute(ctx); a * a }
    fn min_value(&self) -> f64 { 0.0_f64.max(self.0.min_value()) }
    fn max_value(&self) -> f64 { let mi = self.0.min_value().abs(); let ma = self.0.max_value().abs(); mi.max(ma) * mi.max(ma) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(Square(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Cube(pub DenseFn);
impl Clone for Cube {
    fn clone(&self) -> Self { Cube(self.0.clone()) }
}
impl DensityFunction for Cube {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { let a = self.0.compute(ctx); a * a * a }
    fn min_value(&self) -> f64 { let v = self.0.min_value(); let w = self.0.max_value(); v.min(w).min(v * v * v).min(w * w * w) }
    fn max_value(&self) -> f64 { let v = self.0.min_value(); let w = self.0.max_value(); v.max(w).max(v * v * v).max(w * w * w) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(Cube(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct HalfNegative(pub DenseFn);
impl Clone for HalfNegative {
    fn clone(&self) -> Self { HalfNegative(self.0.clone()) }
}
impl DensityFunction for HalfNegative {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let a = self.0.compute(ctx);
        if a < 0.0 { a } else { 0.0 }
    }
    fn min_value(&self) -> f64 { self.0.min_value().min(0.0) }
    fn max_value(&self) -> f64 { 0.0_f64.max(self.0.max_value().min(0.0)) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(HalfNegative(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct QuarterNegative(pub DenseFn);
impl Clone for QuarterNegative {
    fn clone(&self) -> Self { QuarterNegative(self.0.clone()) }
}
impl DensityFunction for QuarterNegative {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let a = self.0.compute(ctx);
        if a < 0.0 { a * a } else { 0.0 }
    }
    fn min_value(&self) -> f64 {
        let mi = self.0.min_value();
        let ma = self.0.max_value();
        if ma <= 0.0 { (mi * mi).min(ma * ma) } else { 0.0 }
    }
    fn max_value(&self) -> f64 {
        let mi = self.0.min_value();
        let ma = self.0.max_value();
        if mi < 0.0 { let a = mi.abs().max(ma.abs()); a * a } else { 0.0 }
    }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(QuarterNegative(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Invert(pub DenseFn);
impl Clone for Invert {
    fn clone(&self) -> Self { Invert(self.0.clone()) }
}
impl DensityFunction for Invert {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 { -self.0.compute(ctx) }
    fn min_value(&self) -> f64 { -self.0.max_value() }
    fn max_value(&self) -> f64 { -self.0.min_value() }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(Invert(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

pub struct Squeeze(pub DenseFn);
impl Clone for Squeeze {
    fn clone(&self) -> Self { Squeeze(self.0.clone()) }
}
impl DensityFunction for Squeeze {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let x = self.0.compute(ctx).clamp(-1.0, 1.0);
        x * 0.5 - x * x * x / 24.0
    }
    fn min_value(&self) -> f64 { 0.0 }
    fn max_value(&self) -> f64 { 1.0 }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn { DenseFn(Box::new(Squeeze(visitor.apply(self.0.clone())))) }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Clamp
// ---------------------------------------------------------------------------

pub struct Clamp {
    pub input: DenseFn,
    pub min: f64,
    pub max: f64,
}
impl Clone for Clamp {
    fn clone(&self) -> Self { Self { input: self.input.clone(), min: self.min, max: self.max } }
}
impl DensityFunction for Clamp {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        self.input.compute(ctx).clamp(self.min, self.max)
    }
    fn min_value(&self) -> f64 { self.min }
    fn max_value(&self) -> f64 { self.max }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Clamp { input: visitor.apply(self.input.clone()), min: self.min, max: self.max }))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// RangeChoice
// ---------------------------------------------------------------------------

pub struct RangeChoice {
    pub input: DenseFn,
    pub min_inclusive: f64,
    pub max_exclusive: f64,
    pub when_in_range: DenseFn,
    pub when_out_of_range: DenseFn,
}
impl Clone for RangeChoice {
    fn clone(&self) -> Self {
        Self {
            input: self.input.clone(),
            min_inclusive: self.min_inclusive,
            max_exclusive: self.max_exclusive,
            when_in_range: self.when_in_range.clone(),
            when_out_of_range: self.when_out_of_range.clone(),
        }
    }
}
impl DensityFunction for RangeChoice {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let v = self.input.compute(ctx);
        if v >= self.min_inclusive && v < self.max_exclusive {
            self.when_in_range.compute(ctx)
        } else {
            self.when_out_of_range.compute(ctx)
        }
    }
    fn fill_array(&self, output: &mut [f64], provider: &dyn ContextProvider) {
        self.input.fill_array(output, provider);
        for i in 0..output.len() {
            let ctx = &provider.for_index(i);
            let v = output[i];
            output[i] = if v >= self.min_inclusive && v < self.max_exclusive {
                self.when_in_range.compute(ctx)
            } else {
                self.when_out_of_range.compute(ctx)
            };
        }
    }
    fn min_value(&self) -> f64 { self.when_in_range.min_value().min(self.when_out_of_range.min_value()) }
    fn max_value(&self) -> f64 { self.when_in_range.max_value().max(self.when_out_of_range.max_value()) }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(RangeChoice {
            input: visitor.apply(self.input.clone()),
            min_inclusive: self.min_inclusive,
            max_exclusive: self.max_exclusive,
            when_in_range: visitor.apply(self.when_in_range.clone()),
            when_out_of_range: visitor.apply(self.when_out_of_range.clone()),
        }))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// IntervalSelect
// ---------------------------------------------------------------------------

pub struct IntervalSelect {
    pub input: DenseFn,
    pub thresholds: Vec<f64>,
    pub functions: Vec<DenseFn>,
}
impl Clone for IntervalSelect {
    fn clone(&self) -> Self {
        Self {
            input: self.input.clone(),
            thresholds: self.thresholds.clone(),
            functions: self.functions.iter().map(|f| f.clone()).collect(),
        }
    }
}
impl IntervalSelect {
    fn select(&self, ctx: &dyn FunctionContext, input: f64) -> f64 {
        for (i, &thresh) in self.thresholds.iter().enumerate() {
            if input < thresh {
                return self.functions[i].compute(ctx);
            }
        }
        self.functions.last().unwrap().compute(ctx)
    }
}
impl DensityFunction for IntervalSelect {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        self.select(ctx, self.input.compute(ctx))
    }
    fn fill_array(&self, output: &mut [f64], provider: &dyn ContextProvider) {
        self.input.fill_array(output, provider);
        for i in 0..output.len() {
            let ctx = &provider.for_index(i);
            output[i] = self.select(ctx, output[i]);
        }
    }
    fn min_value(&self) -> f64 {
        self.functions.iter().map(|f| f.min_value()).fold(f64::MAX, |a, b| a.min(b))
    }
    fn max_value(&self) -> f64 {
        self.functions.iter().map(|f| f.max_value()).fold(f64::NEG_INFINITY, |a, b| a.max(b))
    }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(IntervalSelect {
            input: visitor.apply(self.input.clone()),
            thresholds: self.thresholds.clone(),
            functions: self.functions.iter().map(|f| visitor.apply(f.clone())).collect(),
        }))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Marker / caching wrappers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerType {
    Interpolated,
    FlatCache,
    Cache2D,
    CacheOnce,
    CacheAllInCell,
    BlendDensity,
}

pub struct Marker(pub MarkerType, pub DenseFn);
impl Clone for Marker {
    fn clone(&self) -> Self { Marker(self.0, self.1.clone()) }
}
impl DensityFunction for Marker {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        if self.0 == MarkerType::Interpolated {
            if let Some(value) = ctx.sample_interpolated(&self.1) {
                return value;
            }
        }
        self.1.compute(ctx)
    }
    fn fill_array(&self, output: &mut [f64], provider: &dyn ContextProvider) { self.1.fill_array(output, provider); }
    fn min_value(&self) -> f64 {
        if self.0 == MarkerType::BlendDensity { f64::NEG_INFINITY } else { self.1.min_value() }
    }
    fn max_value(&self) -> f64 {
        if self.0 == MarkerType::BlendDensity { f64::INFINITY } else { self.1.max_value() }
    }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(Marker(self.0, visitor.apply(self.1.clone()))))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Special: BlendAlpha, BlendOffset, BeardifierMarker
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct BlendAlpha;
impl DensityFunction for BlendAlpha {
    fn compute(&self, _ctx: &dyn FunctionContext) -> f64 { 1.0 }
    fn fill_array(&self, output: &mut [f64], _provider: &dyn ContextProvider) { output.fill(1.0); }
    fn min_value(&self) -> f64 { 1.0 }
    fn max_value(&self) -> f64 { 1.0 }
    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn { self.clone_dyn() }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

#[derive(Clone)]
pub struct BlendOffset;
impl DensityFunction for BlendOffset {
    fn compute(&self, _ctx: &dyn FunctionContext) -> f64 { 0.0 }
    fn fill_array(&self, output: &mut [f64], _provider: &dyn ContextProvider) { output.fill(0.0); }
    fn min_value(&self) -> f64 { 0.0 }
    fn max_value(&self) -> f64 { 0.0 }
    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn { self.clone_dyn() }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

#[derive(Clone)]
pub struct BeardifierMarker;
impl DensityFunction for BeardifierMarker {
    fn compute(&self, _ctx: &dyn FunctionContext) -> f64 { 0.0 }
    fn fill_array(&self, output: &mut [f64], _provider: &dyn ContextProvider) { output.fill(0.0); }
    fn min_value(&self) -> f64 { 0.0 }
    fn max_value(&self) -> f64 { 0.0 }
    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn { self.clone_dyn() }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// Spline
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SplinePoint {
    ctx: SinglePointContext,
}

impl SplinePoint {
    pub fn new(ctx: SinglePointContext) -> Self { Self { ctx } }
    pub fn context(&self) -> &dyn FunctionContext { &self.ctx }
}

#[derive(Clone)]
pub struct SplineCoordinate(pub DenseFn);

impl SplineCoordinate {
    pub fn evaluate(&self, point: &SplinePoint) -> f32 {
        self.0.compute(&point.ctx) as f32
    }
    pub fn min_value(&self) -> f32 { self.0.min_value() as f32 }
    pub fn max_value(&self) -> f32 { self.0.max_value() as f32 }
}

#[derive(Clone, Debug)]
pub struct ControlPoint {
    pub location: f32,
    pub value: f32,
    pub derivative: Option<f32>,
}

pub struct Spline {
    pub coordinate: SplineCoordinate,
    pub points: Vec<ControlPoint>,
    min: f64,
    max: f64,
}

impl Clone for Spline {
    fn clone(&self) -> Self {
        Self {
            coordinate: self.coordinate.clone(),
            points: self.points.clone(),
            min: self.min,
            max: self.max,
        }
    }
}

impl Spline {
    pub fn new(coordinate: SplineCoordinate, points: Vec<ControlPoint>) -> Self {
        if points.is_empty() {
            return Self { coordinate, points, min: 0.0, max: 0.0 };
        }
        let min = points.iter().map(|p| p.value as f64).fold(f64::INFINITY, |a, b| a.min(b));
        let max = points.iter().map(|p| p.value as f64).fold(f64::NEG_INFINITY, |a, b| a.max(b));
        Self { coordinate, points, min, max }
    }

    pub fn evaluate(&self, x: f32) -> f32 {
        let pts = &self.points;
        if pts.is_empty() { return 0.0; }
        if x <= pts[0].location { return pts[0].value; }
        if x >= pts[pts.len() - 1].location { return pts[pts.len() - 1].value; }

        let mut lo = 0usize;
        let mut hi = pts.len() - 1;
        while lo < hi - 1 {
            let mid = (lo + hi) / 2;
            if x < pts[mid].location { hi = mid; } else { lo = mid; }
        }

        let p1 = &pts[lo];
        let p2 = &pts[hi];
        let p0 = if lo > 0 { &pts[lo - 1] } else { p1 };
        let p3 = if hi < pts.len() - 1 { &pts[hi + 1] } else { p2 };

        let t = (x - p1.location) / (p2.location - p1.location);
        let dx = p2.location - p1.location;
        cubic_hermite(p0.value, p1.value, p2.value, p3.value, t, p1.derivative, p2.derivative, dx)
    }
}

fn cubic_hermite(_y0: f32, y1: f32, y2: f32, _y3: f32, t: f32, m1: Option<f32>, m2: Option<f32>, dx: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;

    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;

    let slope = if dx != 0.0 { (y2 - y1) / dx } else { 0.0 };

    // Fritsch-Carlson monotone derivative if not specified
    let m1 = m1.unwrap_or(slope);
    let m2 = m2.unwrap_or(slope);

    h00 * y1 + h10 * m1 * dx + h01 * y2 + h11 * m2 * dx
}

impl DensityFunction for Spline {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let point = SplinePoint {
            ctx: SinglePointContext {
                block_x: ctx.block_x(),
                block_y: ctx.block_y(),
                block_z: ctx.block_z(),
            },
        };
        let x = self.coordinate.evaluate(&point);
        self.evaluate(x) as f64
    }
    fn min_value(&self) -> f64 { self.min }
    fn max_value(&self) -> f64 { self.max }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        let new_coord = SplineCoordinate(visitor.apply(self.coordinate.0.clone()));
        DenseFn(Box::new(Self::new(new_coord, self.points.clone())))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// FindTopSurface
// ---------------------------------------------------------------------------

pub struct FindTopSurface {
    pub density: DenseFn,
    pub upper_bound: DenseFn,
    pub lower_bound: i32,
    pub cell_height: i32,
}
impl Clone for FindTopSurface {
    fn clone(&self) -> Self {
        Self {
            density: self.density.clone(),
            upper_bound: self.upper_bound.clone(),
            lower_bound: self.lower_bound,
            cell_height: self.cell_height,
        }
    }
}
impl DensityFunction for FindTopSurface {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let top_y = (self.upper_bound.compute(ctx) / self.cell_height as f64).floor() as i32 * self.cell_height;
        if top_y <= self.lower_bound { return self.lower_bound as f64; }

        let mut y = top_y;
        while y >= self.lower_bound {
            let point = SinglePointContext { block_x: ctx.block_x(), block_y: y, block_z: ctx.block_z() };
            if self.density.compute(&point) > 0.0 {
                return y as f64;
            }
            y -= self.cell_height;
        }
        self.lower_bound as f64
    }
    fn min_value(&self) -> f64 { self.lower_bound as f64 }
    fn max_value(&self) -> f64 { self.lower_bound.max(self.upper_bound.max_value().floor() as i32) as f64 }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(FindTopSurface {
            density: visitor.apply(self.density.clone()),
            upper_bound: visitor.apply(self.upper_bound.clone()),
            lower_bound: self.lower_bound,
            cell_height: self.cell_height,
        }))
    }
    fn clone_dyn(&self) -> DenseFn { DenseFn(Box::new(self.clone())) }
}

// ---------------------------------------------------------------------------
// factory helper functions
// ---------------------------------------------------------------------------

pub fn constant(value: f64) -> DenseFn { DenseFn(Box::new(Constant(value))) }
pub fn zero() -> DenseFn { DenseFn(Box::new(Constant(0.0))) }

pub fn noise(noise: NoiseHandle, xz_scale: f64, y_scale: f64) -> DenseFn {
    DenseFn(Box::new(Noise { noise, xz_scale, y_scale }))
}
pub fn noise_1d(n: NoiseHandle, y_scale: f64) -> DenseFn {
    noise(n, 1.0, y_scale)
}

pub fn shift_a(noise: NoiseHandle) -> DenseFn { DenseFn(Box::new(ShiftA(noise))) }
pub fn shift_b(noise: NoiseHandle) -> DenseFn { DenseFn(Box::new(ShiftB(noise))) }
pub fn shift(noise: NoiseHandle) -> DenseFn { DenseFn(Box::new(Shift(noise))) }

pub fn shifted_noise_2d(shift_x: DenseFn, shift_z: DenseFn, xz_scale: f64, noise: NoiseHandle) -> DenseFn {
    DenseFn(Box::new(ShiftedNoise {
        shift_x,
        shift_y: zero(),
        shift_z,
        xz_scale,
        y_scale: 0.0,
        noise,
    }))
}

pub fn end_islands(seed: u64) -> DenseFn { DenseFn(Box::new(EndIslandDensityFunction::new(seed))) }

pub fn y_clamped_gradient(from_y: i32, to_y: i32, from_value: f64, to_value: f64) -> DenseFn {
    DenseFn(Box::new(YClampedGradient { from_y, to_y, from_value, to_value }))
}

pub fn add(a: DenseFn, b: DenseFn) -> DenseFn { DenseFn(Box::new(Add(a, b))) }
pub fn mul(a: DenseFn, b: DenseFn) -> DenseFn { DenseFn(Box::new(Mul(a, b))) }
pub fn min(a: DenseFn, b: DenseFn) -> DenseFn { DenseFn(Box::new(Min(a, b))) }
pub fn max(a: DenseFn, b: DenseFn) -> DenseFn { DenseFn(Box::new(Max(a, b))) }

pub fn neg(a: DenseFn) -> DenseFn { DenseFn(Box::new(Neg(a))) }
pub fn abs(a: DenseFn) -> DenseFn { DenseFn(Box::new(Abs(a))) }
pub fn square(a: DenseFn) -> DenseFn { DenseFn(Box::new(Square(a))) }
pub fn cube(a: DenseFn) -> DenseFn { DenseFn(Box::new(Cube(a))) }
pub fn half_negative(a: DenseFn) -> DenseFn { DenseFn(Box::new(HalfNegative(a))) }
pub fn quarter_negative(a: DenseFn) -> DenseFn { DenseFn(Box::new(QuarterNegative(a))) }
pub fn invert(a: DenseFn) -> DenseFn { DenseFn(Box::new(Invert(a))) }
pub fn squeeze(a: DenseFn) -> DenseFn { DenseFn(Box::new(Squeeze(a))) }

pub fn clamp(input: DenseFn, min: f64, max: f64) -> DenseFn {
    DenseFn(Box::new(Clamp { input, min, max }))
}

pub fn range_choice(input: DenseFn, min_inclusive: f64, max_exclusive: f64,
                    when_in_range: DenseFn, when_out_of_range: DenseFn) -> DenseFn {
    DenseFn(Box::new(RangeChoice { input, min_inclusive, max_exclusive, when_in_range, when_out_of_range }))
}

pub fn interval_select(input: DenseFn, thresholds: Vec<f64>, functions: Vec<DenseFn>) -> DenseFn {
    DenseFn(Box::new(IntervalSelect { input, thresholds, functions }))
}

pub fn interpolated(function: DenseFn) -> DenseFn { DenseFn(Box::new(Marker(MarkerType::Interpolated, function))) }
pub fn flat_cache(function: DenseFn) -> DenseFn { DenseFn(Box::new(Marker(MarkerType::FlatCache, function))) }
pub fn cache2d(function: DenseFn) -> DenseFn { DenseFn(Box::new(Marker(MarkerType::Cache2D, function))) }
pub fn cache_once(function: DenseFn) -> DenseFn { DenseFn(Box::new(Marker(MarkerType::CacheOnce, function))) }
pub fn cache_all_in_cell(function: DenseFn) -> DenseFn { DenseFn(Box::new(Marker(MarkerType::CacheAllInCell, function))) }
pub fn blend_density(function: DenseFn) -> DenseFn { DenseFn(Box::new(Marker(MarkerType::BlendDensity, function))) }

pub fn blend_alpha() -> DenseFn { DenseFn(Box::new(BlendAlpha)) }
pub fn blend_offset() -> DenseFn { DenseFn(Box::new(BlendOffset)) }
pub fn beardifier() -> DenseFn { DenseFn(Box::new(BeardifierMarker)) }

pub fn find_top_surface(density: DenseFn, upper_bound: DenseFn, lower_bound: i32, cell_height: i32) -> DenseFn {
    DenseFn(Box::new(FindTopSurface { density, upper_bound, lower_bound, cell_height }))
}

pub fn lerp(alpha: DenseFn, first: DenseFn, second: DenseFn) -> DenseFn {
    let alpha_cached = cache_once(alpha);
    let one_minus_alpha = add(mul(alpha_cached.clone(), constant(-1.0)), constant(1.0));
    add(mul(first, one_minus_alpha), mul(second, alpha_cached))
}

pub fn lerp_const(alpha: DenseFn, first_constant: f64, second: DenseFn) -> DenseFn {
    add(mul(alpha, add(second, constant(-first_constant))), constant(first_constant))
}

pub fn spline(coordinate: SplineCoordinate, points: Vec<ControlPoint>) -> DenseFn {
    DenseFn(Box::new(Spline::new(coordinate, points)))
}

pub fn map_from_unit_to(function: DenseFn, min: f64, max: f64) -> DenseFn {
    let middle = (min + max) * 0.5;
    let factor = (max - min) * 0.5;
    add(constant(middle), mul(constant(factor), function))
}

// ---------------------------------------------------------------------------
// Utility: clamped_map (like Mth.clampedMap)
// ---------------------------------------------------------------------------

fn clamped_map(value: f64, from_start: f64, from_end: f64, to_start: f64, to_end: f64) -> f64 {
    let t = ((value - from_start) / (from_end - from_start)).clamp(0.0, 1.0);
    to_start + (to_end - to_start) * t
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant() {
        let c = constant(3.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((c.compute(&ctx) - 3.0).abs() < 1e-12);
        assert!((c.min_value() - 3.0).abs() < 1e-12);
        assert!((c.max_value() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn interpolated_marker_only_interpolates_explicit_functions() {
        let nonlinear = square(y_clamped_gradient(0, 8, 0.0, 8.0));
        let marked = interpolated(nonlinear.clone());
        let point = SinglePointContext { block_x: 1, block_y: 4, block_z: 1 };
        let cell = InterpolatedContext::new(1, 4, 1, 4, 8);

        assert!((nonlinear.compute(&cell) - 16.0).abs() < 1e-12);
        assert!((marked.compute(&point) - 16.0).abs() < 1e-12);
        assert!((marked.compute(&cell) - 32.0).abs() < 1e-12);
    }

    #[test]
    fn test_add() {
        let a = constant(2.0);
        let b = constant(3.0);
        let sum = add(a, b);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((sum.compute(&ctx) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_mul() {
        let a = constant(4.0);
        let b = constant(5.0);
        let prod = mul(a, b);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((prod.compute(&ctx) - 20.0).abs() < 1e-12);
    }

    #[test]
    fn test_min_max() {
        let a = constant(10.0);
        let b = constant(20.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((min(a.clone(), b.clone()).compute(&ctx) - 10.0).abs() < 1e-12);
        assert!((max(a, b).compute(&ctx) - 20.0).abs() < 1e-12);
    }

    #[test]
    fn test_neg_invert() {
        let a = constant(5.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((neg(a.clone()).compute(&ctx) + 5.0).abs() < 1e-12);
        assert!((invert(a).compute(&ctx) + 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_abs_square_cube() {
        let neg_val = constant(-3.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((abs(neg_val.clone()).compute(&ctx) - 3.0).abs() < 1e-12);
        assert!((square(neg_val.clone()).compute(&ctx) - 9.0).abs() < 1e-12);
        assert!((cube(neg_val).compute(&ctx) + 27.0).abs() < 1e-12);
    }

    #[test]
    fn test_half_negative() {
        let pos = constant(5.0);
        let neg_val = constant(-5.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((half_negative(pos).compute(&ctx) - 0.0).abs() < 1e-12);
        assert!((half_negative(neg_val).compute(&ctx) + 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_quarter_negative() {
        let neg_val = constant(-4.0);
        let pos = constant(5.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((quarter_negative(neg_val).compute(&ctx) - 16.0).abs() < 1e-12);
        assert!((quarter_negative(pos).compute(&ctx) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_y_clamped_gradient() {
        let grad = y_clamped_gradient(0, 10, 0.0, 100.0);
        let ctx0 = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        let ctx5 = SinglePointContext { block_x: 0, block_y: 5, block_z: 0 };
        let ctx10 = SinglePointContext { block_x: 0, block_y: 10, block_z: 0 };
        assert!((grad.compute(&ctx0) - 0.0).abs() < 1e-9);
        assert!((grad.compute(&ctx5) - 50.0).abs() < 1e-9);
        assert!((grad.compute(&ctx10) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_range_choice() {
        let input = constant(5.0);
        let in_range = constant(100.0);
        let out = constant(200.0);
        let rc = range_choice(input, 0.0, 10.0, in_range, out);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((rc.compute(&ctx) - 100.0).abs() < 1e-12);
    }

    #[test]
    fn test_clamp() {
        let input = constant(15.0);
        let clamped = clamp(input, 0.0, 10.0);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((clamped.compute(&ctx) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn test_squeeze() {
        let input = constant(0.5);
        let sq = squeeze(input);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        // squeeze(0.5) = 0.5/2 - 0.5^3/24 = 0.25 - 0.125/24 = 0.24479166...
        let expected = 0.5_f64 / 2.0 - 0.5_f64.powi(3) / 24.0;
        assert!((sq.compute(&ctx) - expected).abs() < 1e-12);
    }

    #[test]
    fn test_simplex_noise_symmetry() {
        let noise = SimplexNoise2D::new(42);
        let v = noise.get_value(1.0, 2.0);
        let v_flip = noise.get_value(2.0, 1.0);
        assert!(v.is_finite());
        assert!(v_flip.is_finite());
    }

    #[test]
    fn test_interval_select() {
        let input = constant(0.0);
        let fns = vec![constant(10.0), constant(20.0), constant(30.0)];
        let isel = interval_select(input, vec![-0.5, 0.5], fns);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((isel.compute(&ctx) - 20.0).abs() < 1e-12);
    }

    #[test]
    fn test_spline_simple() {
        let coord = SplineCoordinate(constant(0.5));
        let points = vec![
            ControlPoint { location: 0.0, value: 0.0, derivative: None },
            ControlPoint { location: 1.0, value: 10.0, derivative: None },
        ];
        let spl = spline(coord, points);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        let v = spl.compute(&ctx);
        assert!(v > 0.0 && v < 10.0);
        assert!((v - 5.0).abs() < 1.0);
    }

    #[test]
    fn test_lerp() {
        let factor = constant(0.5);
        let first = constant(0.0);
        let second = constant(10.0);
        let l = lerp(factor, first, second);
        let ctx = SinglePointContext { block_x: 0, block_y: 0, block_z: 0 };
        assert!((l.compute(&ctx) - 5.0).abs() < 1e-9);
    }
}
