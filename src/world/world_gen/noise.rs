//! Port of Minecraft's noise system (ImprovedNoise, PerlinNoise, NormalNoise, SimplexNoise)
//!
//! Corresponding Java classes:
//! - `net.minecraft.world.level.levelgen.synth.ImprovedNoise`
//! - `net.minecraft.world.level.levelgen.synth.PerlinNoise`
//! - `net.minecraft.world.level.levelgen.synth.NormalNoise`
//! - `net.minecraft.world.level.levelgen.synth.SimplexNoise`
//! - `net.minecraft.world.level.levelgen.NoiseSettings`
//! - `net.minecraft.world.level.levelgen.Noises`

#![allow(dead_code)]

// ============================================================================
// Math helpers — corresponding to net.minecraft.util.Mth
// ============================================================================

/// Equivalent to `Mth.floor(double)` — returns the largest int ≤ x.
fn floor(x: f64) -> i32 {
    x.floor() as i32
}

/// Equivalent to `Mth.lfloor(double)` — returns the largest i64 ≤ x.
fn lfloor(x: f64) -> i64 {
    x.floor() as i64
}

/// Equivalent to `Mth.smoothstep(double)` — Ken Perlin's 6t⁵ − 15t⁴ + 10t³.
fn smoothstep(x: f64) -> f64 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// Equivalent to `Mth.smoothstepDerivative(double)` — derivative of smoothstep.
fn smoothstep_derivative(x: f64) -> f64 {
    30.0 * x * x * (1.0 - x) * (1.0 - x)
}

/// Equivalent to `Mth.lerp(double, double, double)` — linear interpolation.
fn lerp(alpha: f64, a: f64, b: f64) -> f64 {
    a + (b - a) * alpha
}

/// Equivalent to `Mth.lerp2(double, double, ...)` — bilinear interpolation.
fn lerp2(a: f64, b: f64, v00: f64, v10: f64, v01: f64, v11: f64) -> f64 {
    let v0 = lerp(a, v00, v10);
    let v1 = lerp(a, v01, v11);
    lerp(b, v0, v1)
}

/// Equivalent to `Mth.lerp3(double, double, double, ...)` — trilinear interpolation
/// over the eight corners of a unit cube.
#[allow(clippy::too_many_arguments)]
fn lerp3(
    a: f64,
    b: f64,
    c: f64,
    v000: f64,
    v100: f64,
    v010: f64,
    v110: f64,
    v001: f64,
    v101: f64,
    v011: f64,
    v111: f64,
) -> f64 {
    let v00 = lerp(a, v000, v100);
    let v10 = lerp(a, v010, v110);
    let v01 = lerp(a, v001, v101);
    let v11 = lerp(a, v011, v111);
    let v0 = lerp(b, v00, v10);
    let v1 = lerp(b, v01, v11);
    lerp(c, v0, v1)
}

// ============================================================================
// NoiseSeed — Xoroshiro128++ with Minecraft's seed semantics
// ============================================================================

/// Deterministic PRNG corresponding to Minecraft's `XoroshiroRandomSource`.
///
/// The single-seed constructor performs the same Stafford13 expansion as
/// `RandomSupport.upgradeSeedTo128bit(seed)`. The internal two-state constructor
/// is used by positional factories, whose states are already fully specified.
pub struct NoiseSeed {
    state_lo: u64,
    state_hi: u64,
    /// Saved initial seed retained for callers of the old accessor.
    initial_seed: u64,
}

impl NoiseSeed {
    pub fn new(seed: u64) -> Self {
        let low = seed ^ 0x6a09e667f3bcc909;
        let high = low.wrapping_add(0x9e3779b97f4a7c15);
        let (state_lo, state_hi) = (mix_stafford13(low), mix_stafford13(high));
        NoiseSeed::from_state(state_lo, state_hi, seed)
    }

    fn from_state(state_lo: u64, state_hi: u64, initial_seed: u64) -> Self {
        let (state_lo, state_hi) = if state_lo == 0 && state_hi == 0 {
            (0x9e3779b97f4a7c15, 0x6a09e667f3bcc909)
        } else {
            (state_lo, state_hi)
        };
        NoiseSeed { state_lo, state_hi, initial_seed }
    }

    /// Return the initial seed (for positional forking).
    pub fn initial_seed(&self) -> u64 {
        self.initial_seed
    }

    /// Advance Xoroshiro128++ and return the top `bits` bits.
    fn next(&mut self, bits: i32) -> i32 {
        (self.next_long() >> (64 - bits)) as i32
    }

    fn next_bits(&mut self, bits: i32) -> u64 {
        self.next_long() >> (64 - bits)
    }

    /// Equivalent to `Xoroshiro128PlusPlus.nextLong()`.
    pub fn next_long(&mut self) -> u64 {
        let s0 = self.state_lo;
        let mut s1 = self.state_hi;
        let result = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);

        s1 ^= s0;
        self.state_lo = s0.rotate_left(49) ^ s1 ^ (s1 << 21);
        self.state_hi = s1.rotate_left(28);
        result
    }

    /// Equivalent to `RandomSource.nextDouble()` for Xoroshiro.
    /// Returns a double in [0.0, 1.0).
    pub fn next_double(&mut self) -> f64 {
        self.next_bits(53) as f64 * 1.1102230246251565e-16
    }

    pub fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * 5.9604645e-8
    }

    pub fn next_boolean(&mut self) -> bool {
        self.next_long() & 1 != 0
    }

    /// Equivalent to `XoroshiroRandomSource.nextInt(int n)`.
    /// Returns a uniformly distributed int in [0, n).
    pub fn next_int(&mut self, n: i32) -> i32 {
        assert!(n > 0, "n must be positive");
        let bound = n as u64;
        loop {
            let random_bits = self.next_long() as u32 as u64;
            let multiplied = random_bits * bound;
            let fractional = multiplied & u32::MAX as u64;
            if fractional < bound {
                let threshold = ((u32::MAX as u64 + 1) - bound) % bound;
                if fractional < threshold {
                    continue;
                }
            }
            return (multiplied >> 32) as i32;
        }
    }

    /// Create a new `NoiseSeed` from the root positional factory for `base_seed`.
    /// This corresponds to `XoroshiroRandomSource(base_seed).forkPositional()
    /// .fromHashOf(name)`.
    pub fn from_hash_of(base_seed: u64, name: &str) -> Self {
        NoiseSeed::new(base_seed).fork_positional().from_hash_of(name)
    }

    /// Split this source into the positional factory used by Java levelgen.
    pub fn fork_positional(&mut self) -> PositionalRandomFactory {
        PositionalRandomFactory {
            seed_lo: self.next_long(),
            seed_hi: self.next_long(),
        }
    }
}

fn mix_stafford13(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

/// The Xoroshiro positional factory used by `PerlinNoise` and `RandomState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositionalRandomFactory {
    seed_lo: u64,
    seed_hi: u64,
}

impl PositionalRandomFactory {
    pub fn from_hash_of(&self, name: &str) -> NoiseSeed {
        let digest = md5_digest(name.as_bytes());
        let hash_lo = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3],
            digest[4], digest[5], digest[6], digest[7],
        ]);
        let hash_hi = u64::from_be_bytes([
            digest[8], digest[9], digest[10], digest[11],
            digest[12], digest[13], digest[14], digest[15],
        ]);
        NoiseSeed::from_state(hash_lo ^ self.seed_lo, hash_hi ^ self.seed_hi, 0)
    }

    /// Equivalent to `XoroshiroPositionalRandomFactory.at(x, y, z)`.
    pub fn at(&self, x: i32, y: i32, z: i32) -> NoiseSeed {
        // Match Mth.getSeed: the X term is evaluated as a Java int before
        // being promoted by the long Z term in the XOR expression.
        let x_term = x.wrapping_mul(3_129_871) as i64;
        let mut positional = x_term
            ^ (z as i64).wrapping_mul(116_129_781)
            ^ y as i64;
        positional = positional
            .wrapping_mul(positional)
            .wrapping_mul(42_317_861)
            .wrapping_add(positional.wrapping_mul(11));
        NoiseSeed::from_state((positional >> 16) as u64 ^ self.seed_lo, self.seed_hi, 0)
    }

    pub fn from_seed(&self, seed: u64) -> NoiseSeed {
        NoiseSeed::from_state(seed ^ self.seed_lo, seed ^ self.seed_hi, seed)
    }
}

fn md5_digest(input: &[u8]) -> [u8; 16] {
    const SHIFT: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const TABLE: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,
        0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
        0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,
        0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,
        0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];

    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((input.len() as u64) * 8).to_le_bytes());

    let mut a0 = 0x67452301_u32;
    let mut b0 = 0xefcdab89_u32;
    let mut c0 = 0x98badcfe_u32;
    let mut d0 = 0x10325476_u32;

    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 16];
        for (word, bytes) in words.iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | ((!b) & d), i)
            } else if i < 32 {
                ((d & b) | ((!d) & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | !d), (7 * i) % 16)
            };
            let next = a
                .wrapping_add(f)
                .wrapping_add(TABLE[i])
                .wrapping_add(words[g])
                .rotate_left(SHIFT[i])
                .wrapping_add(b);
            (a, b, c, d) = (d, next, b, c);
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0_u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    digest
}

// ============================================================================
// SimplexNoise
// ============================================================================

/// Simplex noise (2D and 3D).
///
/// Corresponds to `net.minecraft.world.level.levelgen.synth.SimplexNoise`.
pub struct SimplexNoise {
    /// Permutation table (512 entries, duplicate of a 256-entry shuffled array).
    p: [i32; 512],
    /// Offsets added to input coordinates.
    pub xo: f64,
    pub yo: f64,
    pub zo: f64,
}

impl SimplexNoise {
    /// The gradient vectors for 3D simplex / improved noise (16 entries).
    ///
    /// Entries 12-15 are duplicates of earlier ones so that `& 0xF` indexing
    /// (used by `ImprovedNoise.gradDot`) produces a valid symmetric distribution.
    pub const GRADIENT: [[i32; 3]; 16] = [
        [1, 1, 0],
        [-1, 1, 0],
        [1, -1, 0],
        [-1, -1, 0],
        [1, 0, 1],
        [-1, 0, 1],
        [1, 0, -1],
        [-1, 0, -1],
        [0, 1, 1],
        [0, -1, 1],
        [0, 1, -1],
        [0, -1, -1],
        [1, 1, 0],
        [0, -1, 1],
        [-1, 1, 0],
        [0, -1, -1],
    ];

    /// Create a new `SimplexNoise` seeded from `random`.
    ///
    /// Equivalent to the `SimplexNoise(RandomSource)` constructor.
    pub fn new(random: &mut NoiseSeed) -> Self {
        let xo = random.next_double() * 256.0;
        let yo = random.next_double() * 256.0;
        let zo = random.next_double() * 256.0;

        let mut perm = [0i32; 512];
        for i in 0..256usize {
            perm[i] = i as i32;
        }
        // Fisher-Yates shuffle matching Java's java.util.Random swap pattern
        for i in 0..256usize {
            let offset = random.next_int((256 - i as i32) as i32) as usize;
            let tmp = perm[i];
            perm[i] = perm[i + offset];
            perm[i + offset] = tmp;
        }
        // Duplicate into the second half
        for i in 0..256usize {
            perm[i + 256] = perm[i];
        }

        SimplexNoise {
            p: perm,
            xo,
            yo,
            zo,
        }
    }

    /// Equivalent to `p(int)` — lookup in the permutation table.
    fn p(&self, x: i32) -> i32 {
        self.p[x as usize & 0xFF]
    }

    /// Equivalent to `SimplexNoise.dot(int[], double, double, double)`.
    pub fn dot(g: &[i32; 3], x: f64, y: f64, z: f64) -> f64 {
        g[0] as f64 * x + g[1] as f64 * y + g[2] as f64 * z
    }

    /// Equivalent to `getCornerNoise3D(int, double, double, double, double)`.
    fn corner_noise_3d(&self, index: usize, x: f64, y: f64, z: f64, base: f64) -> f64 {
        let mut t = base - x * x - y * y - z * z;
        if t < 0.0 {
            return 0.0;
        }
        t *= t;
        t * t * Self::dot(&Self::GRADIENT[index], x, y, z)
    }

    /// 2D simplex noise.
    ///
    /// Equivalent to `SimplexNoise.getValue(double, double)`.
    pub fn get_value_2d(&self, xin: f64, yin: f64) -> f64 {
        let sqrt_3 = 1.7320508075688772_f64;
        let f2 = 0.5 * (sqrt_3 - 1.0);
        let g2 = (3.0 - sqrt_3) / 6.0;

        let s = (xin + yin) * f2;
        let i = floor(xin + s);
        let j = floor(yin + s);

        let t = (i + j) as f64 * g2;
        let x0 = xin - (i as f64 - t);
        let y0 = yin - (j as f64 - t);

        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };

        let x1 = x0 - i1 as f64 + g2;
        let y1 = y0 - j1 as f64 + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;

        let ii = i & 0xFF;
        let jj = j & 0xFF;

        let gi0 = (self.p(ii + self.p(jj)) % 12) as usize;
        let gi1 = (self.p(ii + i1 + self.p(jj + j1)) % 12) as usize;
        let gi2 = (self.p(ii + 1 + self.p(jj + 1)) % 12) as usize;

        let n0 = self.corner_noise_3d(gi0, x0, y0, 0.0, 0.5);
        let n1 = self.corner_noise_3d(gi1, x1, y1, 0.0, 0.5);
        let n2 = self.corner_noise_3d(gi2, x2, y2, 0.0, 0.5);

        70.0 * (n0 + n1 + n2)
    }

    /// 3D simplex noise.
    ///
    /// Equivalent to `SimplexNoise.getValue(double, double, double)`.
    pub fn get_value_3d(&self, xin: f64, yin: f64, zin: f64) -> f64 {
        let f3 = 1.0 / 3.0; // 0.3333333333333333
        let g3 = 1.0 / 6.0; // 0.16666666666666666

        let s = (xin + yin + zin) * f3;
        let i = floor(xin + s);
        let j = floor(yin + s);
        let k = floor(zin + s);

        let t = (i + j + k) as f64 * g3;
        let x0 = xin - (i as f64 - t);
        let y0 = yin - (j as f64 - t);
        let z0 = zin - (k as f64 - t);

        // Determine simplex (which skewed tetrahedron we are in)
        let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
            if y0 >= z0 {
                // X Y Z order
                (1, 0, 0, 1, 1, 0)
            } else if x0 >= z0 {
                // X Z Y order
                (1, 0, 0, 1, 0, 1)
            } else {
                // Z X Y order
                (0, 0, 1, 1, 0, 1)
            }
        } else if y0 < z0 {
            // Z Y X order
            (0, 0, 1, 0, 1, 1)
        } else if x0 < z0 {
            // Y Z X order
            (0, 1, 0, 0, 1, 1)
        } else {
            // Y X Z order
            (0, 1, 0, 1, 1, 0)
        };

        let x1 = x0 - i1 as f64 + g3;
        let y1 = y0 - j1 as f64 + g3;
        let z1 = z0 - k1 as f64 + g3;

        let x2 = x0 - i2 as f64 + 2.0 * g3;
        let y2 = y0 - j2 as f64 + 2.0 * g3;
        let z2 = z0 - k2 as f64 + 2.0 * g3;

        let x3 = x0 - 1.0 + 3.0 * g3;
        let y3 = y0 - 1.0 + 3.0 * g3;
        let z3 = z0 - 1.0 + 3.0 * g3;

        let ii = i & 0xFF;
        let jj = j & 0xFF;
        let kk = k & 0xFF;

        let gi0 = (self.p(ii + self.p(jj + self.p(kk))) % 12) as usize;
        let gi1 = (self.p(ii + i1 + self.p(jj + j1 + self.p(kk + k1))) % 12) as usize;
        let gi2 = (self.p(ii + i2 + self.p(jj + j2 + self.p(kk + k2))) % 12) as usize;
        let gi3 = (self.p(ii + 1 + self.p(jj + 1 + self.p(kk + 1))) % 12) as usize;

        let n0 = self.corner_noise_3d(gi0, x0, y0, z0, 0.6);
        let n1 = self.corner_noise_3d(gi1, x1, y1, z1, 0.6);
        let n2 = self.corner_noise_3d(gi2, x2, y2, z2, 0.6);
        let n3 = self.corner_noise_3d(gi3, x3, y3, z3, 0.6);

        32.0 * (n0 + n1 + n2 + n3)
    }
}

// ============================================================================
// ImprovedNoise
// ============================================================================

/// 3D Perlin noise with a permutation table.
///
/// Corresponds to `net.minecraft.world.level.levelgen.synth.ImprovedNoise`.
#[derive(Clone)]
pub struct ImprovedNoise {
    /// Permutation table (256 bytes, stored as u8).
    p: [u8; 256],
    /// Offsets added to input coordinates.
    pub xo: f64,
    pub yo: f64,
    pub zo: f64,
}

impl ImprovedNoise {
    /// Create a new `ImprovedNoise` seeded from `random`.
    ///
    /// Equivalent to the `ImprovedNoise(RandomSource)` constructor.
    pub fn new(random: &mut NoiseSeed) -> Self {
        let xo = random.next_double() * 256.0;
        let yo = random.next_double() * 256.0;
        let zo = random.next_double() * 256.0;

        let mut p = [0u8; 256];
        for i in 0..256usize {
            p[i] = i as u8;
        }
        // Fisher-Yates shuffle matching Java's java.util.Random swap pattern
        for i in 0..256usize {
            let offset = random.next_int((256 - i as i32) as i32) as usize;
            let tmp = p[i];
            p[i] = p[i + offset];
            p[i + offset] = tmp;
        }

        ImprovedNoise { p, xo, yo, zo }
    }

    /// Equivalent to `p(int)` — unsigned byte lookup into the permutation table.
    fn p(&self, x: i32) -> usize {
        self.p[x as usize & 0xFF] as usize
    }

    /// Equivalent to `gradDot(int, double, double, double)`.
    fn grad_dot(hash: i32, x: f64, y: f64, z: f64) -> f64 {
        let g = &SimplexNoise::GRADIENT[hash as usize & 0xF];
        SimplexNoise::dot(g, x, y, z)
    }

    /// 3D Perlin noise at position (_x, _y, _z) with no Y scaling.
    ///
    /// Equivalent to `ImprovedNoise.noise(double, double, double)`.
    pub fn noise(&self, _x: f64, _y: f64, _z: f64) -> f64 {
        self.noise_with_y_scale(_x, _y, _z, 0.0, 0.0)
    }

    /// 3D Perlin noise with optional Y scaling ("y fudge").
    ///
    /// Equivalent to the `@Deprecated` `ImprovedNoise.noise(double, double, double, double, double)`.
    pub fn noise_with_y_scale(&self, _x: f64, _y: f64, _z: f64, y_scale: f64, y_fudge: f64) -> f64 {
        let x = _x + self.xo;
        let y = _y + self.yo;
        let z = _z + self.zo;

        let xf = floor(x);
        let yf = floor(y);
        let zf = floor(z);

        let xr = x - xf as f64;
        let yr = y - yf as f64;
        let zr = z - zf as f64;

        let yr_fudge = if y_scale != 0.0 {
            let fudge_limit = if y_fudge >= 0.0 && y_fudge < yr {
                y_fudge
            } else {
                yr
            };
            floor(fudge_limit / y_scale + 1.0000000116860974e-7) as f64 * y_scale
        } else {
            0.0
        };

        self.sample_and_lerp(xf, yf, zf, xr, yr - yr_fudge, zr, yr)
    }

    /// 3D Perlin noise with derivative computation.
    ///
    /// Equivalent to `ImprovedNoise.noiseWithDerivative(double, double, double, double[])`.
    /// The `derivative_out` slice must have length ≥ 3; the derivatives are *added* to its values.
    pub fn noise_with_derivative(&self, _x: f64, _y: f64, _z: f64, derivative_out: &mut [f64]) -> f64 {
        let x = _x + self.xo;
        let y = _y + self.yo;
        let z = _z + self.zo;

        let xf = floor(x);
        let yf = floor(y);
        let zf = floor(z);

        let xr = x - xf as f64;
        let yr = y - yf as f64;
        let zr = z - zf as f64;

        self.sample_with_derivative(xf, yf, zf, xr, yr, zr, derivative_out)
    }

    /// Equivalent to `sampleAndLerp(int, int, int, double, double, double, double)`.
    fn sample_and_lerp(
        &self,
        x: i32,
        y: i32,
        z: i32,
        xr: f64,
        yr: f64,
        zr: f64,
        yr_original: f64,
    ) -> f64 {
        let x0 = self.p(x);
        let x1 = self.p(x + 1);

        let xy00 = self.p(x0 as i32 + y);
        let xy01 = self.p(x0 as i32 + y + 1);
        let xy10 = self.p(x1 as i32 + y);
        let xy11 = self.p(x1 as i32 + y + 1);

        let d000 = Self::grad_dot(self.p(xy00 as i32 + z) as i32, xr, yr, zr);
        let d100 = Self::grad_dot(self.p(xy10 as i32 + z) as i32, xr - 1.0, yr, zr);
        let d010 = Self::grad_dot(self.p(xy01 as i32 + z) as i32, xr, yr - 1.0, zr);
        let d110 = Self::grad_dot(self.p(xy11 as i32 + z) as i32, xr - 1.0, yr - 1.0, zr);
        let d001 = Self::grad_dot(self.p(xy00 as i32 + z + 1) as i32, xr, yr, zr - 1.0);
        let d101 = Self::grad_dot(self.p(xy10 as i32 + z + 1) as i32, xr - 1.0, yr, zr - 1.0);
        let d011 = Self::grad_dot(self.p(xy01 as i32 + z + 1) as i32, xr, yr - 1.0, zr - 1.0);
        let d111 = Self::grad_dot(self.p(xy11 as i32 + z + 1) as i32, xr - 1.0, yr - 1.0, zr - 1.0);

        let x_alpha = smoothstep(xr);
        let y_alpha = smoothstep(yr_original);
        let z_alpha = smoothstep(zr);

        lerp3(x_alpha, y_alpha, z_alpha, d000, d100, d010, d110, d001, d101, d011, d111)
    }

    /// Equivalent to `sampleWithDerivative(int, int, int, double, double, double, double[])`.
    fn sample_with_derivative(
        &self,
        x: i32,
        y: i32,
        z: i32,
        xr: f64,
        yr: f64,
        zr: f64,
        derivative_out: &mut [f64],
    ) -> f64 {
        let x0 = self.p(x);
        let x1 = self.p(x + 1);

        let xy00 = self.p(x0 as i32 + y);
        let xy01 = self.p(x0 as i32 + y + 1);
        let xy10 = self.p(x1 as i32 + y);
        let xy11 = self.p(x1 as i32 + y + 1);

        let p000 = self.p(xy00 as i32 + z) as i32;
        let p100 = self.p(xy10 as i32 + z) as i32;
        let p010 = self.p(xy01 as i32 + z) as i32;
        let p110 = self.p(xy11 as i32 + z) as i32;
        let p001 = self.p(xy00 as i32 + z + 1) as i32;
        let p101 = self.p(xy10 as i32 + z + 1) as i32;
        let p011 = self.p(xy01 as i32 + z + 1) as i32;
        let p111 = self.p(xy11 as i32 + z + 1) as i32;

        let g000 = &SimplexNoise::GRADIENT[p000 as usize & 0xF];
        let g100 = &SimplexNoise::GRADIENT[p100 as usize & 0xF];
        let g010 = &SimplexNoise::GRADIENT[p010 as usize & 0xF];
        let g110 = &SimplexNoise::GRADIENT[p110 as usize & 0xF];
        let g001 = &SimplexNoise::GRADIENT[p001 as usize & 0xF];
        let g101 = &SimplexNoise::GRADIENT[p101 as usize & 0xF];
        let g011 = &SimplexNoise::GRADIENT[p011 as usize & 0xF];
        let g111 = &SimplexNoise::GRADIENT[p111 as usize & 0xF];

        let d000 = SimplexNoise::dot(g000, xr, yr, zr);
        let d100 = SimplexNoise::dot(g100, xr - 1.0, yr, zr);
        let d010 = SimplexNoise::dot(g010, xr, yr - 1.0, zr);
        let d110 = SimplexNoise::dot(g110, xr - 1.0, yr - 1.0, zr);
        let d001 = SimplexNoise::dot(g001, xr, yr, zr - 1.0);
        let d101 = SimplexNoise::dot(g101, xr - 1.0, yr, zr - 1.0);
        let d011 = SimplexNoise::dot(g011, xr, yr - 1.0, zr - 1.0);
        let d111 = SimplexNoise::dot(g111, xr - 1.0, yr - 1.0, zr - 1.0);

        let x_alpha = smoothstep(xr);
        let y_alpha = smoothstep(yr);
        let z_alpha = smoothstep(zr);

        // Interpolated gradient components
        let d1x = lerp3(
            x_alpha, y_alpha, z_alpha,
            g000[0] as f64, g100[0] as f64, g010[0] as f64, g110[0] as f64,
            g001[0] as f64, g101[0] as f64, g011[0] as f64, g111[0] as f64,
        );
        let d1y = lerp3(
            x_alpha, y_alpha, z_alpha,
            g000[1] as f64, g100[1] as f64, g010[1] as f64, g110[1] as f64,
            g001[1] as f64, g101[1] as f64, g011[1] as f64, g111[1] as f64,
        );
        let d1z = lerp3(
            x_alpha, y_alpha, z_alpha,
            g000[2] as f64, g100[2] as f64, g010[2] as f64, g110[2] as f64,
            g001[2] as f64, g101[2] as f64, g011[2] as f64, g111[2] as f64,
        );

        // Cross-derivatives (differences across lattice edges)
        let d2x = lerp2(y_alpha, z_alpha, d100 - d000, d110 - d010, d101 - d001, d111 - d011);
        let d2y = lerp2(z_alpha, x_alpha, d010 - d000, d011 - d001, d110 - d100, d111 - d101);
        let d2z = lerp2(x_alpha, y_alpha, d001 - d000, d101 - d100, d011 - d010, d111 - d110);

        let x_sd = smoothstep_derivative(xr);
        let y_sd = smoothstep_derivative(yr);
        let z_sd = smoothstep_derivative(zr);

        derivative_out[0] += d1x + x_sd * d2x;
        derivative_out[1] += d1y + y_sd * d2y;
        derivative_out[2] += d1z + z_sd * d2z;

        lerp3(x_alpha, y_alpha, z_alpha, d000, d100, d010, d110, d001, d101, d011, d111)
    }

    /// Equivalent to the visible-for-testing `parityConfigString`.
    pub fn parity_config_string(&self, sb: &mut String) {
        use std::fmt::Write;
        let _ = write!(sb, "xo: {}, yo: {}, zo: {}, p: [", self.xo, self.yo, self.zo);
        for (i, &v) in self.p.iter().enumerate() {
            if i > 0 {
                sb.push_str(", ");
            }
            let _ = write!(sb, "{}", v);
        }
        sb.push(']');
    }
}

// ============================================================================
// PerlinNoise
// ============================================================================

/// Octave-summed Perlin noise.
///
/// Corresponds to `net.minecraft.world.level.levelgen.synth.PerlinNoise`.
#[derive(Clone)]
pub struct PerlinNoise {
    /// Individual noise octave generators (Some if the amplitude is non-zero).
    noise_levels: Vec<Option<ImprovedNoise>>,
    /// Index of the first (lowest-frequency) octave.
    first_octave: i32,
    /// Amplitudes for each octave.
    amplitudes: Vec<f64>,
    /// Input frequency factor for the lowest octave.
    lowest_freq_input_factor: f64,
    /// Value scaling factor for the lowest octave.
    lowest_freq_value_factor: f64,
    /// Maximum possible output value.
    max_value: f64,
}

/// Perlin noise initialization mode.
pub enum PerlinInit {
    /// New initialization using `PositionalRandomFactory.fromHashOf`.
    New,
    /// Legacy initialization using sequential RNG consumption.
    Legacy,
}

impl PerlinNoise {
    // Round-off constant for `wrap`: 2²⁵.
    const ROUND_OFF: f64 = 33_554_432.0;

    /// Number of random values consumed when skipping an octave in legacy mode.
    const SKIP_COUNT: i32 = 262;

    /// Create from a set of octave indices (amplitude = 1 for each, 0 elsewhere).
    ///
    /// Equivalent to `PerlinNoise.create(RandomSource, IntStream)`.
    pub fn create(random: &mut NoiseSeed, octaves: &[i32]) -> Self {
        PerlinNoise::create_with_init(random, octaves, PerlinInit::New)
    }

    /// Sequential octave initialization used internally by Java's
    /// `BlendedNoise`, even when its source random is Xoroshiro.
    pub fn create_legacy_for_blended_noise(
        random: &mut NoiseSeed,
        octaves: &[i32],
    ) -> Self {
        PerlinNoise::create_with_init(random, octaves, PerlinInit::Legacy)
    }

    /// Create from explicit first-octave and amplitude list.
    ///
    /// Equivalent to `PerlinNoise.create(RandomSource, int, DoubleList)` (new init).
    pub fn create_with_amplitudes(random: &mut NoiseSeed, first_octave: i32, amplitudes: Vec<f64>) -> Self {
        PerlinNoise::create_common(random, first_octave, amplitudes, true)
    }

    /// Create using the specified initialization mode.
    fn create_with_init(random: &mut NoiseSeed, octave_set: &[i32], init: PerlinInit) -> Self {
        let (first_octave, amplitudes) = Self::make_amplitudes(octave_set);
        Self::create_common(random, first_octave, amplitudes, matches!(init, PerlinInit::New))
    }

    fn create_common(random: &mut NoiseSeed, first_octave: i32, amplitudes: Vec<f64>, use_new_init: bool) -> Self {
        let octaves = amplitudes.len();
        let zero_octave_index = -first_octave;

        let lowest_freq_input_factor = 2.0_f64.powf(-zero_octave_index as f64);
        let lowest_freq_value_factor = 2.0_f64.powf((octaves - 1) as f64)
            / (2.0_f64.powf(octaves as f64) - 1.0);

        let noise_levels = if use_new_init {
            Self::init_new(random, first_octave, &amplitudes)
        } else {
            Self::init_legacy(random, first_octave, zero_octave_index, &amplitudes)
        };

        let max_value = Self::edge_value_from(&noise_levels, &amplitudes, lowest_freq_value_factor, 2.0);

        PerlinNoise {
            noise_levels,
            first_octave,
            amplitudes,
            lowest_freq_input_factor,
            lowest_freq_value_factor,
            max_value,
        }
    }

    /// Equivalent to `makeAmplitudes(IntSortedSet)`.
    fn make_amplitudes(octave_set: &[i32]) -> (i32, Vec<f64>) {
        assert!(!octave_set.is_empty(), "Need some octaves!");

        let mut sorted = octave_set.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let low_freq_octaves = -sorted[0];
        let high_freq_octaves = sorted[sorted.len() - 1];
        let octaves = (low_freq_octaves + high_freq_octaves + 1) as usize;
        assert!(octaves >= 1, "Total number of octaves needs to be >= 1");

        let mut amplitudes = vec![0.0_f64; octaves];
        for &o in &sorted {
            amplitudes[(o + low_freq_octaves) as usize] = 1.0;
        }

        (-low_freq_octaves, amplitudes)
    }

    /// New initialization path: one positional factory supplies all octaves.
    /// Each octave is then seeded with the factory's MD5 name hash.
    fn init_new(random: &mut NoiseSeed, first_octave: i32, amplitudes: &[f64]) -> Vec<Option<ImprovedNoise>> {
        let positional = random.fork_positional();
        let octaves = amplitudes.len();
        let mut levels = Vec::with_capacity(octaves);

        for i in 0..octaves {
            if amplitudes[i] != 0.0 {
                let octave = first_octave + i as i32;
                let name = format!("octave_{}", octave);
                let mut seed = positional.from_hash_of(&name);
                levels.push(Some(ImprovedNoise::new(&mut seed)));
            } else {
                levels.push(None);
            }
        }

        levels
    }

    /// Legacy initialization path: sequential RNG consumption with skip on zero amplitude.
    fn init_legacy(
        random: &mut NoiseSeed,
        _first_octave: i32,
        zero_octave_index: i32,
        amplitudes: &[f64],
    ) -> Vec<Option<ImprovedNoise>> {
        let octaves = amplitudes.len();
        let mut levels: Vec<Option<ImprovedNoise>> = vec![None; octaves];

        let zero_octave = ImprovedNoise::new(random);
        if zero_octave_index >= 0 && (zero_octave_index as usize) < octaves {
            if amplitudes[zero_octave_index as usize] != 0.0 {
                levels[zero_octave_index as usize] = Some(zero_octave);
            }
        }

        // Fill lower-index slots (higher frequency) by iterating downward.
        for i in (0..zero_octave_index).rev() {
            if (i as usize) < octaves {
                if amplitudes[i as usize] != 0.0 {
                    levels[i as usize] = Some(ImprovedNoise::new(random));
                } else {
                    Self::skip_octave(random);
                }
            } else {
                Self::skip_octave(random);
            }
        }

        // Legacy mode: positive octaves are disabled.
        if (zero_octave_index as usize) < octaves - 1 {
            panic!("Positive octaves are temporarily disabled in legacy mode");
        }

        levels
    }

    /// Equivalent to `skipOctave(RandomSource)`.
    /// Consumes the same number of random values as `ImprovedNoise(RandomSource)`.
    fn skip_octave(random: &mut NoiseSeed) {
        for _ in 0..Self::SKIP_COUNT {
            // Consume one random int per call (matching consumeCount behavior).
            random.next_int(1);
        }
    }

    /// Equivalent to `getOctaveNoise(int)` — returns the ImprovedNoise at reversed index.
    pub fn get_octave_noise(&self, i: i32) -> Option<&ImprovedNoise> {
        let idx = self.noise_levels.len() - 1 - i as usize;
        self.noise_levels.get(idx).and_then(|n| n.as_ref())
    }

    /// Equivalent to `maxValue()`.
    pub fn max_value(&self) -> f64 {
        self.max_value
    }

    /// Equivalent to `firstOctave()`.
    pub fn first_octave(&self) -> i32 {
        self.first_octave
    }

    /// Equivalent to `amplitudes()`.
    pub fn amplitudes(&self) -> &[f64] {
        &self.amplitudes
    }

    /// Equivalent to `PerlinNoise.wrap(double)`.
    ///
    /// Wraps coordinates to prevent blow-up from large values.
    pub fn wrap(x: f64) -> f64 {
        x - lfloor(x / Self::ROUND_OFF + 0.5) as f64 * Self::ROUND_OFF
    }

    /// Equivalent to `getValue(double, double, double)`.
    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        self.get_value_with_y_fudge(x, y, z, 0.0, 0.0)
    }

    /// Equivalent to the `@Deprecated` `getValue(double, double, double, double, double)`.
    pub fn get_value_with_y_fudge(&self, x: f64, y: f64, z: f64, y_scale: f64, y_fudge: f64) -> f64 {
        let mut value = 0.0;
        let mut factor = self.lowest_freq_input_factor;
        let mut value_factor = self.lowest_freq_value_factor;

        for i in 0..self.noise_levels.len() {
            if let Some(noise) = &self.noise_levels[i] {
                let nx = Self::wrap(x * factor);
                let ny = Self::wrap(y * factor);
                let nz = Self::wrap(z * factor);
                let nv = noise.noise_with_y_scale(nx, ny, nz, y_scale * factor, y_fudge * factor);
                value += self.amplitudes[i] * nv * value_factor;
            }
            factor *= 2.0;
            value_factor /= 2.0;
        }

        value
    }

    /// Equivalent to `maxBrokenValue(double)`.
    pub fn max_broken_value(&self, y_scale: f64) -> f64 {
        self.edge_value(y_scale + 2.0)
    }

    /// Equivalent to `edgeValue(double)`.
    fn edge_value(&self, noise_value: f64) -> f64 {
        Self::edge_value_from(&self.noise_levels, &self.amplitudes, self.lowest_freq_value_factor, noise_value)
    }

    fn edge_value_from(
        noise_levels: &[Option<ImprovedNoise>],
        amplitudes: &[f64],
        lowest_freq_value_factor: f64,
        noise_value: f64,
    ) -> f64 {
        let mut value = 0.0;
        let mut value_factor = lowest_freq_value_factor;

        for i in 0..noise_levels.len() {
            if noise_levels[i].is_some() {
                value += amplitudes[i] * noise_value * value_factor;
            }
            value_factor /= 2.0;
        }

        value
    }
}

// ============================================================================
// NormalNoise
// ============================================================================

/// Dual Perlin noise — the primary terrain noise type in Minecraft.
///
/// Corresponds to `net.minecraft.world.level.levelgen.synth.NormalNoise`.
pub struct NormalNoise {
    /// Value factor scaling the combined output.
    value_factor: f64,
    /// First Perlin noise instance.
    first: PerlinNoise,
    /// Second Perlin noise instance (sampled at a scaled coordinate).
    second: PerlinNoise,
    /// Maximum possible output value.
    max_value: f64,
    /// Parameters used to construct this noise.
    parameters: NoiseParameters,
}

/// Parameters for constructing `NormalNoise`.
///
/// Corresponds to `NormalNoise.NoiseParameters`.
#[derive(Clone)]
pub struct NoiseParameters {
    pub first_octave: i32,
    pub amplitudes: Vec<f64>,
}

impl NoiseParameters {
    pub fn new(first_octave: i32, amplitudes: Vec<f64>) -> Self {
        NoiseParameters { first_octave, amplitudes }
    }

    pub fn from_slice(first_octave: i32, amplitudes: &[f64]) -> Self {
        NoiseParameters {
            first_octave,
            amplitudes: amplitudes.to_vec(),
        }
    }

    pub fn from_first(first_octave: i32, first_amplitude: f64, rest: &[f64]) -> Self {
        let mut amplitudes = Vec::with_capacity(rest.len() + 1);
        amplitudes.push(first_amplitude);
        amplitudes.extend_from_slice(rest);
        NoiseParameters { first_octave, amplitudes }
    }
}

/// Seeding mode for `NormalNoise`.
enum NormalInit {
    New,
    Legacy,
}

impl NormalNoise {
    /// Input scaling factor applied to the second noise's coordinates.
    const INPUT_FACTOR: f64 = 1.0181268882175227;

    /// Create `NormalNoise` (new initialization path).
    ///
    /// Equivalent to `NormalNoise.create(RandomSource, NoiseParameters)`.
    pub fn create(random: &mut NoiseSeed, parameters: &NoiseParameters) -> Self {
        NormalNoise::create_with_init(random, parameters, NormalInit::New)
    }

    /// Create with explicit first octave and amplitudes.
    ///
    /// Equivalent to `NormalNoise.create(RandomSource, int, double...)`.
    pub fn create_simple(random: &mut NoiseSeed, first_octave: i32, amplitudes: &[f64]) -> Self {
        let params = NoiseParameters::from_slice(first_octave, amplitudes);
        NormalNoise::create_with_init(random, &params, NormalInit::New)
    }

    /// Create using legacy nether biome initialization.
    #[deprecated(note = "Use create() instead")]
    pub fn create_legacy_nether(random: &mut NoiseSeed, parameters: &NoiseParameters) -> Self {
        NormalNoise::create_with_init(random, parameters, NormalInit::Legacy)
    }

    fn create_with_init(random: &mut NoiseSeed, parameters: &NoiseParameters, init: NormalInit) -> Self {
        let (first, second) = match init {
            NormalInit::New => {
                let first = PerlinNoise::create_with_amplitudes(random, parameters.first_octave, parameters.amplitudes.clone());
                let second = PerlinNoise::create_with_amplitudes(random, parameters.first_octave, parameters.amplitudes.clone());
                (first, second)
            }
            NormalInit::Legacy => {
                // Legacy path uses the sequential RNG
                todo!("Legacy NormalNoise initialization not yet implemented")
            }
        };

        // Compute octave span from non-zero amplitudes
        let mut min_octave = i32::MAX;
        let mut max_octave = i32::MIN;
        for (i, &a) in parameters.amplitudes.iter().enumerate() {
            if a != 0.0 {
                min_octave = min_octave.min(i as i32);
                max_octave = max_octave.max(i as i32);
            }
        }

        let octave_span = max_octave - min_octave;
        let expected_dev = 0.1 * (1.0 + 1.0 / (octave_span + 1) as f64);
        let value_factor = 0.16666666666666666 / expected_dev;

        let max_value = (first.max_value() + second.max_value()) * value_factor;

        NormalNoise {
            value_factor,
            first,
            second,
            max_value,
            parameters: parameters.clone(),
        }
    }

    /// Equivalent to `maxValue()`.
    pub fn max_value(&self) -> f64 {
        self.max_value
    }

    /// Equivalent to `parameters()`.
    pub fn parameters(&self) -> &NoiseParameters {
        &self.parameters
    }

    /// Evaluate the noise at (x, y, z).
    ///
    /// Equivalent to `NormalNoise.getValue(double, double, double)`.
    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        let x2 = x * Self::INPUT_FACTOR;
        let y2 = y * Self::INPUT_FACTOR;
        let z2 = z * Self::INPUT_FACTOR;
        (self.first.get_value(x, y, z) + self.second.get_value(x2, y2, z2)) * self.value_factor
    }
}

// ============================================================================
// NoiseSettings
// ============================================================================

/// Terrain noise grid settings.
///
/// Corresponds to `net.minecraft.world.level.levelgen.NoiseSettings`.
#[derive(Clone, Copy, Debug)]
pub struct NoiseSettings {
    /// Minimum Y coordinate of the dimension.
    pub min_y: i32,
    /// Total height (in blocks) of the dimension.
    pub height: i32,
    /// Cell width in quart blocks (1 quart = ¼ block).
    /// Corresponds to `noiseSizeHorizontal`.
    pub size_horizontal: i32,
    /// Cell height in quart blocks.
    /// Corresponds to `noiseSizeVertical`.
    pub size_vertical: i32,
}

impl NoiseSettings {
    /// Equivalent to `NoiseSettings.create(int, int, int, int)`.
    pub fn create(min_y: i32, height: i32, size_horizontal: i32, size_vertical: i32) -> Self {
        let s = NoiseSettings { min_y, height, size_horizontal, size_vertical };
        // Validate: min_y must be a multiple of 16, height must be a multiple of 16,
        // and min_y + height must not exceed 2032 (MAX_Y + 1 in modern MC).
        assert!(height % 16 == 0, "height has to be a multiple of 16");
        assert!(min_y % 16 == 0, "min_y has to be a multiple of 16");
        assert!(min_y + height <= 2032, "min_y + height cannot be higher than 2032");
        s
    }

    /// Equivalent to `getCellWidth()` — converts from quart blocks to blocks.
    pub fn cell_width(&self) -> i32 {
        self.size_horizontal * 4
    }

    /// Equivalent to `getCellHeight()` — converts from quart blocks to blocks.
    pub fn cell_height(&self) -> i32 {
        self.size_vertical * 4
    }

    /// Equivalent to `clampToHeightAccessor(LevelHeightAccessor)`.
    pub fn clamp_to_height(&self, other_min_y: i32, other_max_y: i32) -> Self {
        let new_min_y = self.min_y.max(other_min_y);
        let new_height = (self.min_y + self.height).min(other_max_y + 1) - new_min_y;
        NoiseSettings {
            min_y: new_min_y,
            height: new_height,
            size_horizontal: self.size_horizontal,
            size_vertical: self.size_vertical,
        }
    }

    /// Overworld noise settings: min_y = -64, height = 384, size_horizontal = 1, size_vertical = 2.
    pub const OVERWORLD: NoiseSettings = NoiseSettings {
        min_y: -64,
        height: 384,
        size_horizontal: 1,
        size_vertical: 2,
    };

    /// Nether noise settings: min_y = 0, height = 128, size_horizontal = 1, size_vertical = 2.
    pub const NETHER: NoiseSettings = NoiseSettings {
        min_y: 0,
        height: 128,
        size_horizontal: 1,
        size_vertical: 2,
    };

    /// End noise settings: min_y = 0, height = 128, size_horizontal = 2, size_vertical = 1.
    pub const END: NoiseSettings = NoiseSettings {
        min_y: 0,
        height: 128,
        size_horizontal: 2,
        size_vertical: 1,
    };

    /// Caves noise settings: min_y = -64, height = 192, size_horizontal = 1, size_vertical = 2.
    pub const CAVES: NoiseSettings = NoiseSettings {
        min_y: -64,
        height: 192,
        size_horizontal: 1,
        size_vertical: 2,
    };

    /// Floating islands noise settings: min_y = 0, height = 256, size_horizontal = 2, size_vertical = 1.
    pub const FLOATING_ISLANDS: NoiseSettings = NoiseSettings {
        min_y: 0,
        height: 256,
        size_horizontal: 2,
        size_vertical: 1,
    };
}

// ============================================================================
// NoiseKey — all noise keys from Noises.java
// ============================================================================

/// All noise key constants from `net.minecraft.world.level.levelgen.Noises`.
///
/// Each variant corresponds to a `ResourceKey<NormalNoise.NoiseParameters>` in the Java source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoiseKey {
    Temperature,
    Vegetation,
    Continentalness,
    Erosion,
    TemperatureLarge,
    VegetationLarge,
    ContinentalnessLarge,
    ErosionLarge,
    Ridge,
    Shift,
    TemperatureNether,
    VegetationNether,
    AquiferBarrier,
    AquiferFluidLevelFloodedness,
    AquiferLava,
    AquiferFluidLevelSpread,
    Pillar,
    PillarRareness,
    PillarThickness,
    Spaghetti2d,
    Spaghetti2dElevation,
    Spaghetti2dModulator,
    Spaghetti2dThickness,
    Spaghetti3d1,
    Spaghetti3d2,
    Spaghetti3dRarity,
    Spaghetti3dThickness,
    SpaghettiRoughness,
    SpaghettiRoughnessModulator,
    CaveEntrance,
    CaveLayer,
    CaveCheese,
    OreVeininess,
    OreVeinA,
    OreVeinB,
    OreGap,
    Noodle,
    NoodleThickness,
    NoodleRidgeA,
    NoodleRidgeB,
    Jagged,
    Surface,
    SurfaceSecondary,
    ClayBandsOffset,
    BadlandsPillar,
    BadlandsPillarRoof,
    BadlandsSurface,
    IcebergPillar,
    IcebergPillarRoof,
    IcebergSurface,
    SulfurCaveGradient,
    Swamp,
    Calcite,
    Gravel,
    PowderSnow,
    PackedIce,
    Ice,
    SoulSandLayer,
    GravelLayer,
    Patch,
    Netherrack,
    NetherWart,
    NetherStateSelector,
}

impl NoiseKey {
    /// Return the string name used as a resource identifier path.
    /// Corresponds to the name argument in `createKey(String)`.
    pub fn name(&self) -> &'static str {
        match self {
            NoiseKey::Temperature => "temperature",
            NoiseKey::Vegetation => "vegetation",
            NoiseKey::Continentalness => "continentalness",
            NoiseKey::Erosion => "erosion",
            NoiseKey::TemperatureLarge => "temperature_large",
            NoiseKey::VegetationLarge => "vegetation_large",
            NoiseKey::ContinentalnessLarge => "continentalness_large",
            NoiseKey::ErosionLarge => "erosion_large",
            NoiseKey::Ridge => "ridge",
            NoiseKey::Shift => "offset",
            NoiseKey::TemperatureNether => "nether/temperature",
            NoiseKey::VegetationNether => "nether/vegetation",
            NoiseKey::AquiferBarrier => "aquifer_barrier",
            NoiseKey::AquiferFluidLevelFloodedness => "aquifer_fluid_level_floodedness",
            NoiseKey::AquiferLava => "aquifer_lava",
            NoiseKey::AquiferFluidLevelSpread => "aquifer_fluid_level_spread",
            NoiseKey::Pillar => "pillar",
            NoiseKey::PillarRareness => "pillar_rareness",
            NoiseKey::PillarThickness => "pillar_thickness",
            NoiseKey::Spaghetti2d => "spaghetti_2d",
            NoiseKey::Spaghetti2dElevation => "spaghetti_2d_elevation",
            NoiseKey::Spaghetti2dModulator => "spaghetti_2d_modulator",
            NoiseKey::Spaghetti2dThickness => "spaghetti_2d_thickness",
            NoiseKey::Spaghetti3d1 => "spaghetti_3d_1",
            NoiseKey::Spaghetti3d2 => "spaghetti_3d_2",
            NoiseKey::Spaghetti3dRarity => "spaghetti_3d_rarity",
            NoiseKey::Spaghetti3dThickness => "spaghetti_3d_thickness",
            NoiseKey::SpaghettiRoughness => "spaghetti_roughness",
            NoiseKey::SpaghettiRoughnessModulator => "spaghetti_roughness_modulator",
            NoiseKey::CaveEntrance => "cave_entrance",
            NoiseKey::CaveLayer => "cave_layer",
            NoiseKey::CaveCheese => "cave_cheese",
            NoiseKey::OreVeininess => "ore_veininess",
            NoiseKey::OreVeinA => "ore_vein_a",
            NoiseKey::OreVeinB => "ore_vein_b",
            NoiseKey::OreGap => "ore_gap",
            NoiseKey::Noodle => "noodle",
            NoiseKey::NoodleThickness => "noodle_thickness",
            NoiseKey::NoodleRidgeA => "noodle_ridge_a",
            NoiseKey::NoodleRidgeB => "noodle_ridge_b",
            NoiseKey::Jagged => "jagged",
            NoiseKey::Surface => "surface",
            NoiseKey::SurfaceSecondary => "surface_secondary",
            NoiseKey::ClayBandsOffset => "clay_bands_offset",
            NoiseKey::BadlandsPillar => "badlands_pillar",
            NoiseKey::BadlandsPillarRoof => "badlands_pillar_roof",
            NoiseKey::BadlandsSurface => "badlands_surface",
            NoiseKey::IcebergPillar => "iceberg_pillar",
            NoiseKey::IcebergPillarRoof => "iceberg_pillar_roof",
            NoiseKey::IcebergSurface => "iceberg_surface",
            NoiseKey::SulfurCaveGradient => "sulfur_cave_gradient",
            NoiseKey::Swamp => "surface_swamp",
            NoiseKey::Calcite => "calcite",
            NoiseKey::Gravel => "gravel",
            NoiseKey::PowderSnow => "powder_snow",
            NoiseKey::PackedIce => "packed_ice",
            NoiseKey::Ice => "ice",
            NoiseKey::SoulSandLayer => "soul_sand_layer",
            NoiseKey::GravelLayer => "gravel_layer",
            NoiseKey::Patch => "patch",
            NoiseKey::Netherrack => "netherrack",
            NoiseKey::NetherWart => "nether_wart",
            NoiseKey::NetherStateSelector => "nether_state_selector",
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothstep_range() {
        assert!((smoothstep(0.0) - 0.0).abs() < 1e-15);
        assert!((smoothstep(1.0) - 1.0).abs() < 1e-15);
        assert!((smoothstep(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_floor() {
        assert_eq!(floor(3.7), 3);
        assert_eq!(floor(-3.7), -4);
        assert_eq!(floor(0.0), 0);
    }

    #[test]
    fn test_lfloor() {
        assert_eq!(lfloor(3.7e10), 37_000_000_000i64);
        assert_eq!(lfloor(-3.7), -4);
    }

    #[test]
    fn test_wrap() {
        let v = PerlinNoise::wrap(100_000_000.0);
        assert!(v >= -PerlinNoise::ROUND_OFF / 2.0);
        assert!(v <= PerlinNoise::ROUND_OFF / 2.0);
    }

    #[test]
    fn test_noise_seed_next_double_range() {
        let mut seed = NoiseSeed::new(42);
        for _ in 0..1000 {
            let d = seed.next_double();
            assert!(d >= 0.0 && d < 1.0, "next_double out of range: {}", d);
        }
    }

    #[test]
    fn test_noise_seed_deterministic() {
        let mut a = NoiseSeed::new(12345);
        let mut b = NoiseSeed::new(12345);
        for _ in 0..100 {
            assert_eq!(a.next_double(), b.next_double());
            assert_eq!(a.next_int(100), b.next_int(100));
        }
    }

    #[test]
    fn test_xoroshiro_reference_vector() {
        // RandomSupport.upgradeSeedTo128bit(0) and Xoroshiro128PlusPlus.
        let mut seed = NoiseSeed::new(0);
        assert_eq!(seed.state_lo, 0x3564b439cd1e1f16);
        assert_eq!(seed.state_hi, 0x63cfc62a2b097592);
        assert_eq!(seed.next_long(), 0x2a2ca488f66f517e);
        assert_eq!(seed.next_long(), 0xccbc22d72e97c372);
        assert_eq!(seed.next_long(), 0x404e64b826f4b9f4);
    }

    #[test]
    fn test_positional_factory_reference_vector() {
        // XoroshiroRandomSource(42).forkPositional().fromHashOf("octave_-2").
        let mut root = NoiseSeed::new(42);
        let factory = root.fork_positional();
        assert_eq!(factory.seed_lo, 0xbed4a3d469c5d91f);
        assert_eq!(factory.seed_hi, 0x65e301cb50e8f4ab);

        let octave = factory.from_hash_of("octave_-2");
        assert_eq!(octave.state_lo, 0x0a76eeaeed22be64);
        assert_eq!(octave.state_hi, 0x67dcf8adde61416f);
        assert_eq!(NoiseSeed::from_hash_of(42, "octave_-2").state_lo, octave.state_lo);
    }

    #[test]
    fn test_positional_coordinate_reference_vector() {
        // Mth.getSeed(12345, 80, -54321), then the Xoroshiro positional xor.
        let mut root = NoiseSeed::new(42);
        let factory = root.fork_positional();
        let positional = factory.at(12345, 80, -54321);
        // Mth.getSeed evaluates the X term with Java int overflow before the
        // long XOR. This is the resulting positional seed for this fixture.
        assert_eq!(positional.state_lo, 0xbed4fb029b1fe01b);
        assert_eq!(positional.state_hi, 0x65e301cb50e8f4ab);
    }

    #[test]
    fn test_normal_noise_reference_vector() {
        // NormalNoise.create(42, -2, [1, 0, 1]) from minecraft-26.2.
        let mut seed = NoiseSeed::new(42);
        let noise = NormalNoise::create_simple(&mut seed, -2, &[1.0, 0.0, 1.0]);
        assert!((noise.max_value() - 25.0 / 7.0).abs() < 1e-15);
        assert!((noise.get_value(1.5, 2.5, 3.5) - 0.04237057610103179).abs() < 1e-14);
        assert_eq!(seed.next_long(), 0x74d89c01aa1097cb);
    }

    #[test]
    fn test_improved_noise_deterministic() {
        let mut seed = NoiseSeed::new(42);
        let noise = ImprovedNoise::new(&mut seed);
        // These values should be reproducible for a given seed
        let v1 = noise.noise(1.5, 2.5, 3.5);
        let v2 = noise.noise(1.5, 2.5, 3.5);
        assert!((v1 - v2).abs() < 1e-15);
    }

    #[test]
    fn test_simplex_noise_deterministic() {
        let mut seed = NoiseSeed::new(42);
        let noise = SimplexNoise::new(&mut seed);
        let v1 = noise.get_value_2d(1.5, 2.5);
        let v2 = noise.get_value_2d(1.5, 2.5);
        assert!((v1 - v2).abs() < 1e-15);
    }

    #[test]
    fn test_noise_settings_defaults() {
        assert_eq!(NoiseSettings::OVERWORLD.min_y, -64);
        assert_eq!(NoiseSettings::OVERWORLD.height, 384);
        assert_eq!(NoiseSettings::NETHER.min_y, 0);
        assert_eq!(NoiseSettings::NETHER.height, 128);
        assert_eq!(NoiseSettings::END.min_y, 0);
        assert_eq!(NoiseSettings::END.height, 128);
    }

    #[test]
    fn test_noise_settings_cell_conversion() {
        let s = NoiseSettings::OVERWORLD;
        assert_eq!(s.cell_width(), 4);
        assert_eq!(s.cell_height(), 8);
    }

    #[test]
    fn test_noise_key_names() {
        assert_eq!(NoiseKey::Temperature.name(), "temperature");
        assert_eq!(NoiseKey::Continentalness.name(), "continentalness");
        assert_eq!(NoiseKey::Shift.name(), "offset");
        assert_eq!(NoiseKey::CaveCheese.name(), "cave_cheese");
    }
}
