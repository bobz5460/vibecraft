#![allow(dead_code)]

use crate::world::gen::noise::NormalNoise;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct Context {
    pub block_x: f64,
    pub block_y: f64,
    pub block_z: f64,
    pub min_y: i32,
}

pub trait DensityFunction: Send + Sync {
    fn compute(&self, ctx: &Context) -> f64;
    fn min_value(&self) -> f64;
    fn max_value(&self) -> f64;
}

pub struct Constant(pub f64);

impl DensityFunction for Constant {
    fn compute(&self, _ctx: &Context) -> f64 {
        self.0
    }
    fn min_value(&self) -> f64 {
        self.0
    }
    fn max_value(&self) -> f64 {
        self.0
    }
}

pub struct NoiseHolder {
    pub noise: NormalNoise,
    pub xz_scale: f64,
    pub y_scale: f64,
}

impl DensityFunction for NoiseHolder {
    fn compute(&self, ctx: &Context) -> f64 {
        self.noise.get_value(
            ctx.block_x * self.xz_scale,
            ctx.block_y * self.y_scale,
            ctx.block_z * self.xz_scale,
        )
    }
    fn min_value(&self) -> f64 {
        -f64::MAX
    }
    fn max_value(&self) -> f64 {
        f64::MAX
    }
}

pub struct YClampedGradient {
    pub from_y: i32,
    pub to_y: i32,
    pub from_value: f64,
    pub to_value: f64,
}

impl DensityFunction for YClampedGradient {
    fn compute(&self, ctx: &Context) -> f64 {
        let y = ctx.block_y;
        if y <= self.from_y as f64 {
            self.from_value
        } else if y >= self.to_y as f64 {
            self.to_value
        } else {
            let t = (y - self.from_y as f64) / (self.to_y - self.from_y) as f64;
            self.from_value + t * (self.to_value - self.from_value)
        }
    }
    fn min_value(&self) -> f64 {
        self.from_value.min(self.to_value)
    }
    fn max_value(&self) -> f64 {
        self.from_value.max(self.to_value)
    }
}

macro_rules! binary_op {
    ($name:ident, $op:tt) => {
        pub struct $name(
            pub Box<dyn DensityFunction>,
            pub Box<dyn DensityFunction>,
        );

        impl DensityFunction for $name {
            fn compute(&self, ctx: &Context) -> f64 {
                (self.0).compute(ctx) $op (self.1).compute(ctx)
            }
            fn min_value(&self) -> f64 {
                let a = (self.0).min_value();
                let b = (self.1).min_value();
                a $op b
            }
            fn max_value(&self) -> f64 {
                let a = (self.0).max_value();
                let b = (self.1).max_value();
                a $op b
            }
        }
    };
}

binary_op!(Add, +);
binary_op!(Mul, *);

pub struct Min(
    pub Box<dyn DensityFunction>,
    pub Box<dyn DensityFunction>,
);

impl DensityFunction for Min {
    fn compute(&self, ctx: &Context) -> f64 {
        (self.0).compute(ctx).min((self.1).compute(ctx))
    }
    fn min_value(&self) -> f64 {
        (self.0).min_value().min((self.1).min_value())
    }
    fn max_value(&self) -> f64 {
        (self.0).max_value().min((self.1).max_value())
    }
}

pub struct Max(
    pub Box<dyn DensityFunction>,
    pub Box<dyn DensityFunction>,
);

impl DensityFunction for Max {
    fn compute(&self, ctx: &Context) -> f64 {
        (self.0).compute(ctx).max((self.1).compute(ctx))
    }
    fn min_value(&self) -> f64 {
        (self.0).min_value().max((self.1).min_value())
    }
    fn max_value(&self) -> f64 {
        (self.0).max_value().max((self.1).max_value())
    }
}

pub struct RangeChoice {
    pub input: Box<dyn DensityFunction>,
    pub min_inclusive: f64,
    pub max_exclusive: f64,
    pub when_in_range: Box<dyn DensityFunction>,
    pub when_out_of_range: Box<dyn DensityFunction>,
}

impl DensityFunction for RangeChoice {
    fn compute(&self, ctx: &Context) -> f64 {
        let val = self.input.compute(ctx);
        if val >= self.min_inclusive && val < self.max_exclusive {
            self.when_in_range.compute(ctx)
        } else {
            self.when_out_of_range.compute(ctx)
        }
    }
    fn min_value(&self) -> f64 {
        self.when_in_range
            .min_value()
            .min(self.when_out_of_range.min_value())
    }
    fn max_value(&self) -> f64 {
        self.when_in_range
            .max_value()
            .max(self.when_out_of_range.max_value())
    }
}

pub struct Clamp(pub Box<dyn DensityFunction>, pub f64, pub f64);

impl DensityFunction for Clamp {
    fn compute(&self, ctx: &Context) -> f64 {
        self.0.compute(ctx).clamp(self.1, self.2)
    }
    fn min_value(&self) -> f64 {
        self.1
    }
    fn max_value(&self) -> f64 {
        self.2
    }
}

pub struct Spline {
    pub points: Vec<SplinePoint>,
    pub input: Box<dyn DensityFunction>,
}

pub struct SplinePoint {
    pub location: f64,
    pub value: f64,
    pub derivative: f64,
}

impl DensityFunction for Spline {
    fn compute(&self, ctx: &Context) -> f64 {
        let t = self.input.compute(ctx);

        if self.points.is_empty() {
            return 0.0;
        }
        if t <= self.points[0].location {
            return self.points[0].value;
        }
        if t >= self.points[self.points.len() - 1].location {
            return self.points[self.points.len() - 1].value;
        }

        let idx = match self.points[1..]
            .binary_search_by(|p| p.location.partial_cmp(&t).unwrap())
        {
            Ok(i) => i + 1,
            Err(i) => i,
        };

        let p0 = &self.points[idx.saturating_sub(1)];
        let p1 = &self.points[idx];
        let p2 = if idx + 1 < self.points.len() {
            &self.points[idx + 1]
        } else {
            p1
        };
        let p3 = if idx + 2 < self.points.len() {
            &self.points[idx + 2]
        } else {
            p2
        };

        catmull_rom(p0, p1, p2, p3, t)
    }
    fn min_value(&self) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }
        let mut min = f64::MAX;
        for p in &self.points {
            if p.value < min {
                min = p.value;
            }
        }
        min
    }
    fn max_value(&self) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }
        let mut max = f64::MIN;
        for p in &self.points {
            if p.value > max {
                max = p.value;
            }
        }
        max
    }
}

fn catmull_rom(p0: &SplinePoint, p1: &SplinePoint, p2: &SplinePoint, p3: &SplinePoint, t: f64) -> f64 {
    let t1 = (t - p1.location) / (p2.location - p1.location);
    let t2 = t1 * t1;
    let t3 = t2 * t1;

    let v0 = (p2.location - p0.location) / (p2.location - p1.location);
    let v1 = (p3.location - p1.location) / (p2.location - p1.location);

    let a = 2.0 * t3 - 3.0 * t2 + 1.0;
    let b = t3 - 2.0 * t2 + t1;
    let c = -2.0 * t3 + 3.0 * t2;
    let d = t3 - t2;

    a * p1.value + b * p1.derivative * v0 + c * p2.value + d * p2.derivative * v1
}

macro_rules! unary_op {
    ($name:ident, $body:expr) => {
        pub struct $name(pub Box<dyn DensityFunction>);

        impl DensityFunction for $name {
            fn compute(&self, ctx: &Context) -> f64 {
                let x = (self.0).compute(ctx);
                ($body)(x)
            }
            fn min_value(&self) -> f64 {
                ($body)((self.0).min_value())
            }
            fn max_value(&self) -> f64 {
                ($body)((self.0).max_value())
            }
        }
    };
}

unary_op!(Abs, |x: f64| x.abs());
unary_op!(Square, |x: f64| x * x);

pub struct HalfNegative(pub Box<dyn DensityFunction>);

impl DensityFunction for HalfNegative {
    fn compute(&self, ctx: &Context) -> f64 {
        let x = (self.0).compute(ctx);
        if x >= 0.0 { x } else { x * 0.5 }
    }
    fn min_value(&self) -> f64 {
        let x = (self.0).min_value();
        if x >= 0.0 { x } else { x * 0.5 }
    }
    fn max_value(&self) -> f64 {
        let x = (self.0).max_value();
        if x >= 0.0 { x } else { x * 0.5 }
    }
}

pub struct QuarterNegative(pub Box<dyn DensityFunction>);

impl DensityFunction for QuarterNegative {
    fn compute(&self, ctx: &Context) -> f64 {
        let x = (self.0).compute(ctx);
        if x >= 0.0 { x } else { x * 0.25 }
    }
    fn min_value(&self) -> f64 {
        let x = (self.0).min_value();
        if x >= 0.0 { x } else { x * 0.25 }
    }
    fn max_value(&self) -> f64 {
        let x = (self.0).max_value();
        if x >= 0.0 { x } else { x * 0.25 }
    }
}

pub struct Squeeze(pub Box<dyn DensityFunction>);

impl DensityFunction for Squeeze {
    fn compute(&self, ctx: &Context) -> f64 {
        (self.0).compute(ctx).clamp(-1.0, 1.0)
    }
    fn min_value(&self) -> f64 {
        -1.0
    }
    fn max_value(&self) -> f64 {
        1.0
    }
}

pub fn peaks_and_valleys(weirdness: f64) -> f64 {
    -((weirdness.abs() - 0.6666666666666666).abs() - 0.3333333333333333).abs() * 3.0
}

pub fn slide_overworld(noise: f64, y: f64, min_y: i32, height: i32) -> f64 {
    let top_start = (min_y + height - 80) as f64;
    let top_end = (min_y + height) as f64;
    let bottom_start = (min_y + 24) as f64;
    let bottom_end = (min_y + 64) as f64;
    let mut result = noise;

    if y > top_start {
        let factor = ((y - top_start) / (top_end - top_start)).min(1.0);
        let smooth = factor * factor * (3.0 - 2.0 * factor);
        result = result + (0.117 - result) * smooth;
    }
    if y < bottom_end {
        let factor = ((y - bottom_start) / (bottom_end - bottom_start))
            .max(0.0)
            .min(1.0);
        let smooth = 1.0 - (1.0 - factor) * (1.0 - factor);
        result = result + (-0.078 - result) * smooth;
    }
    result
}

pub fn quarter_negative(x: f64) -> f64 {
    if x >= 0.0 { x } else { x * 0.25 }
}

pub fn half_negative(x: f64) -> f64 {
    if x >= 0.0 { x } else { x * 0.5 }
}

pub struct Cache2D {
    function: Box<dyn DensityFunction>,
    cache: Mutex<HashMap<(i32, i32), f64>>,
}

impl DensityFunction for Cache2D {
    fn compute(&self, ctx: &Context) -> f64 {
        let key = ((ctx.block_x as i32) >> 4, (ctx.block_z as i32) >> 4);
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(&val) = cache.get(&key) {
                return val;
            }
            let val = self.function.compute(ctx);
            cache.insert(key, val);
            val
        } else {
            self.function.compute(ctx)
        }
    }
    fn min_value(&self) -> f64 {
        self.function.min_value()
    }
    fn max_value(&self) -> f64 {
        self.function.max_value()
    }
}

pub fn cache_2d(func: Box<dyn DensityFunction>) -> Cache2D {
    Cache2D {
        function: func,
        cache: Mutex::new(HashMap::new()),
    }
}

struct FlatCacheInner {
    values: Vec<f64>,
    cached_xz: (i32, i32),
}

pub struct FlatCache {
    function: Box<dyn DensityFunction>,
    inner: Mutex<FlatCacheInner>,
}

impl DensityFunction for FlatCache {
    fn compute(&self, ctx: &Context) -> f64 {
        let xz_key = ((ctx.block_x as i32) >> 4, (ctx.block_z as i32) >> 4);

        if let Ok(mut inner) = self.inner.lock() {
            if inner.cached_xz != xz_key {
                let y_count = 384;
                let mut values = vec![0.0_f64; y_count];
                let cx = xz_key.0 << 4;
                let cz = xz_key.1 << 4;
                for y in 0..y_count {
                    let local_ctx = Context {
                        block_x: cx as f64,
                        block_y: y as f64,
                        block_z: cz as f64,
                        min_y: ctx.min_y,
                    };
                    values[y as usize] = self.function.compute(&local_ctx);
                }
                inner.values = values;
                inner.cached_xz = xz_key;
            }

            let y_index = (ctx.block_y as i32).max(0).min(383) as usize;
            inner.values[y_index]
        } else {
            self.function.compute(ctx)
        }
    }
    fn min_value(&self) -> f64 {
        self.function.min_value()
    }
    fn max_value(&self) -> f64 {
        self.function.max_value()
    }
}

pub fn flat_cache(func: Box<dyn DensityFunction>) -> FlatCache {
    FlatCache {
        function: func,
        inner: Mutex::new(FlatCacheInner {
            values: Vec::new(),
            cached_xz: (i32::MAX, i32::MAX),
        }),
    }
}

pub struct Interpolated {
    function: Box<dyn DensityFunction>,
}

impl DensityFunction for Interpolated {
    fn compute(&self, ctx: &Context) -> f64 {
        self.function.compute(ctx)
    }
    fn min_value(&self) -> f64 {
        self.function.min_value()
    }
    fn max_value(&self) -> f64 {
        self.function.max_value()
    }
}

pub fn interpolated(func: Box<dyn DensityFunction>) -> Interpolated {
    Interpolated { function: func }
}

pub struct TrilinearInterpolator {
    pub cells_x: i32,
    pub cells_y: i32,
    pub cells_z: i32,
    pub cell_width: i32,
    pub cell_height: i32,
    pub cell_min_y: i32,
    slices: Vec<Vec<f64>>,
    current_cell_x: i32,
    noise000: f64,
    noise001: f64,
    noise010: f64,
    noise011: f64,
    noise100: f64,
    noise101: f64,
    noise110: f64,
    noise111: f64,
}

impl TrilinearInterpolator {
    pub fn new(
        cell_min_y: i32,
        cell_count_y: i32,
        cell_count_xz: i32,
        cell_width: i32,
        cell_height: i32,
    ) -> Self {
        let capacity = (cell_count_xz + 1) * (cell_count_y + 1) * (cell_count_xz + 1);
        let slices = vec![vec![0.0_f64; capacity as usize]; 2];
        TrilinearInterpolator {
            cells_x: cell_count_xz,
            cells_y: cell_count_y,
            cells_z: cell_count_xz,
            cell_width,
            cell_height,
            cell_min_y,
            slices,
            current_cell_x: -1,
            noise000: 0.0,
            noise001: 0.0,
            noise010: 0.0,
            noise011: 0.0,
            noise100: 0.0,
            noise101: 0.0,
            noise110: 0.0,
            noise111: 0.0,
        }
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        let sx = (self.cells_x + 1) as usize;
        let sy = (self.cells_y + 1) as usize;
        (x as usize) * sy * sx + (y as usize) * sx + (z as usize)
    }

    fn fill_cell_x(&mut self, cell_x_index: i32, density: &dyn DensityFunction) {
        let x = cell_x_index * self.cell_width;
        for cell_y in 0..=self.cells_y {
            let y = self.cell_min_y + cell_y * self.cell_height;
            for cell_z in 0..=self.cells_z {
                let z = cell_z * self.cell_width;
                let ctx = Context {
                    block_x: x as f64,
                    block_y: y as f64,
                    block_z: z as f64,
                    min_y: self.cell_min_y,
                };
                let idx = self.index(cell_x_index, cell_y, cell_z);
                self.slices[0][idx] = density.compute(&ctx);
            }
        }
    }

    pub fn initialize_for_first_cell_x(&mut self, density: &dyn DensityFunction) {
        self.current_cell_x = 0;
        self.fill_cell_x(0, density);
        if self.cells_x > 0 {
            self.fill_cell_x(1, density);
        }
    }

    pub fn advance_cell_x(&mut self, cell_x_index: i32, density: &dyn DensityFunction) {
        self.current_cell_x = cell_x_index;
        let next_x = cell_x_index + 1;
        if next_x <= self.cells_x {
            self.fill_cell_x(next_x, density);
        }
        self.slices.swap(0, 1);
    }

    pub fn select_cell_yz(&mut self, cell_y_index: i32, cell_z_index: i32) {
        let cx = self.current_cell_x;
        let nx = cx + 1;
        let ny = cell_y_index + 1;
        let nz = cell_z_index + 1;

        self.noise000 = self.value(cx, cell_y_index, cell_z_index);
        self.noise001 = self.value(cx, cell_y_index, nz.min(self.cells_z));
        self.noise010 = self.value(cx, ny.min(self.cells_y), cell_z_index);
        self.noise011 = self.value(cx, ny.min(self.cells_y), nz.min(self.cells_z));
        self.noise100 = self.value(nx.min(self.cells_x), cell_y_index, cell_z_index);
        self.noise101 = self.value(nx.min(self.cells_x), cell_y_index, nz.min(self.cells_z));
        self.noise110 = self.value(nx.min(self.cells_x), ny.min(self.cells_y), cell_z_index);
        self.noise111 = self.value(
            nx.min(self.cells_x),
            ny.min(self.cells_y),
            nz.min(self.cells_z),
        );
    }

    fn value(&self, x: i32, y: i32, z: i32) -> f64 {
        let idx = self.index(x, y, z);
        self.slices[0][idx]
    }

    pub fn sample(&self, x_in_cell: f64, y_in_cell: f64, z_in_cell: f64) -> f64 {
        let x0 = self.noise000 + (self.noise100 - self.noise000) * x_in_cell;
        let x1 = self.noise010 + (self.noise110 - self.noise010) * x_in_cell;
        let x2 = self.noise001 + (self.noise101 - self.noise001) * x_in_cell;
        let x3 = self.noise011 + (self.noise111 - self.noise011) * x_in_cell;

        let y0 = x0 + (x1 - x0) * y_in_cell;
        let y1 = x2 + (x3 - x2) * y_in_cell;

        y0 + (y1 - y0) * z_in_cell
    }
}

