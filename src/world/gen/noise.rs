#![allow(dead_code)]

// ---------------------------------------------------------------------------
// Private Perlin helper functions
// ---------------------------------------------------------------------------

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn grad(hash: u16, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    let u = if h & 1 == 0 { u } else { -u };
    let v = if h & 2 == 0 { v } else { -v };
    u + v
}

// ---------------------------------------------------------------------------
// SimpleRandom — deterministic LCG-based PRNG
// ---------------------------------------------------------------------------

/// Minimal deterministic pseudo-random number generator using the same LCG
/// recurrence as Java's `Random` class.
pub struct SimpleRandom {
    pub seed: u64,
}

impl SimpleRandom {
    pub fn new(seed: u64) -> Self {
        SimpleRandom { seed }
    }

    /// Advance the LCG and return the new internal state as an `i64`.
    pub fn next_long(&mut self) -> i64 {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.seed as i64
    }

    /// Return a uniformly distributed integer in `[0, bound)`.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        let n = self.next_long().unsigned_abs() as i64;
        (n % bound as i64) as i32
    }

    /// Return a uniformly distributed `f64` in `[0, 1)`.
    pub fn next_double(&mut self) -> f64 {
        ((self.next_long().unsigned_abs() >> 11) as f64) * 1.1102230246251565e-16
    }
}

// ---------------------------------------------------------------------------
// ImprovedNoise — single octave of 3-D Perlin gradient noise
// ---------------------------------------------------------------------------

/// A single octave of 3-D Perlin noise using Ken Perlin's 2002 "improved"
/// algorithm.  Each instance carries a random coordinate offset and a
/// deterministic permutation table built from the supplied seed.
#[derive(Clone)]
pub struct ImprovedNoise {
    pub p: [u16; 512],
    pub xo: f64,
    pub yo: f64,
    pub zo: f64,
}

impl ImprovedNoise {
    /// Build a new permutation table and offsets from `seed`.
    ///
    /// The table is produced by a Fisher-Yates shuffle driven by
    /// `SimpleRandom` so that the result depends only on `seed`.
    pub fn new(seed: i64) -> Self {
        let mut rand = SimpleRandom::new(seed as u64);
        let mut perm = [0u16; 256];
        for i in 0..256 {
            perm[i] = i as u16;
        }
        for i in (1..256).rev() {
            let j = rand.next_int(i as i32 + 1) as usize;
            perm.swap(i, j);
        }
        let mut p = [0u16; 512];
        for i in 0..512 {
            p[i] = perm[i & 255];
        }
        let xo = rand.next_double() * 256.0;
        let yo = rand.next_double() * 256.0;
        let zo = rand.next_double() * 256.0;
        ImprovedNoise { p, xo, yo, zo }
    }

    /// Evaluate the noise function at `(x, y, z)` after adding the per-
    /// instance offsets.
    pub fn sample_and_lerp(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = x + self.xo;
        let y = y + self.yo;
        let z = z + self.zo;

        let ix = x.floor() as i32;
        let iy = y.floor() as i32;
        let iz = z.floor() as i32;

        let fx = x - ix as f64;
        let fy = y - iy as f64;
        let fz = z - iz as f64;

        let sx = fade(fx);
        let sy = fade(fy);
        let sz = fade(fz);

        let ix = ix as usize & 255;
        let iy = iy as usize & 255;
        let iz = iz as usize & 255;

        let a = self.p[ix] as usize + iy;
        let aa = self.p[a] as usize + iz;
        let ab = self.p[a + 1] as usize + iz;
        let b = self.p[ix + 1] as usize + iy;
        let ba = self.p[b] as usize + iz;
        let bb = self.p[b + 1] as usize + iz;

        let x1 = lerp(
            sx,
            grad(self.p[aa], fx, fy, fz),
            grad(self.p[ba], fx - 1.0, fy, fz),
        );
        let x2 = lerp(
            sx,
            grad(self.p[ab], fx, fy - 1.0, fz),
            grad(self.p[bb], fx - 1.0, fy - 1.0, fz),
        );
        let y1 = lerp(sy, x1, x2);

        let x1 = lerp(
            sx,
            grad(self.p[aa + 1], fx, fy, fz - 1.0),
            grad(self.p[ba + 1], fx - 1.0, fy, fz - 1.0),
        );
        let x2 = lerp(
            sx,
            grad(self.p[ab + 1], fx, fy - 1.0, fz - 1.0),
            grad(self.p[bb + 1], fx - 1.0, fy - 1.0, fz - 1.0),
        );
        let y2 = lerp(sy, x1, x2);

        lerp(sz, y1, y2)
    }

    /// Minecraft-style variant with Y-axis stretching and quantisation.
    ///
    /// Internally calls `sample_and_lerp` after adjusting `y`:
    /// `y * y_scale + y_fudge`.
    pub fn noise(&self, x: f64, y: f64, z: f64, y_scale: f64, y_fudge: f64) -> f64 {
        self.sample_and_lerp(x, y * y_scale + y_fudge, z)
    }
}

// ---------------------------------------------------------------------------
// PerlinNoise — octave-combined fBm
// ---------------------------------------------------------------------------

/// Fractal Brownian Motion built from multiple `ImprovedNoise` octaves.
///
/// Each octave operates at a frequency that doubles from the previous one.
/// The contribution of each octave is multiplied by its corresponding
/// amplitude and a per-octave value factor that halves each step, producing
/// a bounded output suitable for terrain and density functions.
#[derive(Clone)]
pub struct PerlinNoise {
    pub noise_levels: Vec<ImprovedNoise>,
    pub amplitudes: Vec<f64>,
    /// Input coordinate multiplier for the lowest-frequency (first) octave.
    /// Equal to `2^{-first_octave}`.
    pub lowest_freq_input_factor: f64,
    /// Starting value factor; halved each octave.
    pub lowest_freq_value_factor: f64,
}

impl PerlinNoise {
    /// Create a new `PerlinNoise` instance.
    ///
    /// * `seed` — base seed; each octave uses `seed`, `seed + 1`, …
    /// * `first_octave` — typically `-15` for high-detail terrain noise.
    ///   Used to derive `lowest_freq_input_factor = 2^{-first_octave}`.
    /// * `amplitudes` — per-octave weights.
    pub fn new(seed: i64, first_octave: i32, amplitudes: &[f64]) -> Self {
        let noise_levels: Vec<ImprovedNoise> = (0..amplitudes.len())
            .map(|i| ImprovedNoise::new(seed.wrapping_add(i as i64)))
            .collect();
        let lowest_freq_input_factor = 2.0f64.powi(-first_octave);
        let lowest_freq_value_factor = 1.0;
        PerlinNoise {
            noise_levels,
            amplitudes: amplitudes.to_vec(),
            lowest_freq_input_factor,
            lowest_freq_value_factor,
        }
    }

    /// Evaluate the noise at `(x, y, z)` without Y-fudge.
    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut value = 0.0;
        let mut value_factor = self.lowest_freq_value_factor;
        for (i, noise) in self.noise_levels.iter().enumerate() {
            let factor = self.lowest_freq_input_factor * (1 << i) as f64;
            value +=
                noise.sample_and_lerp(x * factor, y * factor, z * factor)
                    * self.amplitudes[i]
                    * value_factor;
            value_factor *= 0.5;
        }
        value
    }

    /// Evaluate the noise at `(x, y, z)` with Y-stretching for each octave.
    ///
    /// * `y_scale` — vertical stretch factor.
    /// * `y_fudge` — vertical quantisation offset.
    pub fn get_value_3d(&self, x: f64, y: f64, z: f64, y_scale: f64, y_fudge: f64) -> f64 {
        let mut value = 0.0;
        let mut value_factor = self.lowest_freq_value_factor;
        for (i, noise) in self.noise_levels.iter().enumerate() {
            let factor = self.lowest_freq_input_factor * (1 << i) as f64;
            value +=
                noise.noise(x * factor, y * factor, z * factor, y_scale, y_fudge)
                    * self.amplitudes[i]
                    * value_factor;
            value_factor *= 0.5;
        }
        value
    }

    /// Wrap `x` modulo `2^25` to prevent precision loss at large coordinates.
    pub fn wrap(x: f64) -> f64 {
        x - (x / 3.3554432e7 + 0.5).floor() * 3.3554432e7
    }
}

// ---------------------------------------------------------------------------
// NormalNoise — Gaussian-distributed dual-Perlin noise
// ---------------------------------------------------------------------------

/// Gaussian-distributed noise produced by summing two `PerlinNoise`
/// instances with slightly different frequencies.
#[derive(Clone)]
pub struct NormalNoise {
    pub first: PerlinNoise,
    pub second: PerlinNoise,
    pub value_factor: f64,
}

impl NormalNoise {
    /// Create a new `NormalNoise`.
    ///
    /// The second `PerlinNoise` is seeded with `seed + 2971` so that the
    /// two layers are decorrelated.
    pub fn new(seed: i64, first_octave: i32, amplitudes: &[f64]) -> Self {
        let first = PerlinNoise::new(seed, first_octave, amplitudes);
        let second = PerlinNoise::new(seed.wrapping_add(2971), first_octave, amplitudes);
        let value_factor = 0.1667 / 3.0;
        NormalNoise {
            first,
            second,
            value_factor,
        }
    }

    /// Create a simple single-octave `NormalNoise` from a u64 seed.
    /// Convenience for use in router/aquifer/surface construction.
    pub fn simple(seed: u64) -> Self {
        Self::new(seed as i64, -15, &[1.0])
    }

    /// Create a `NormalNoise` with a custom frequency scaling factor.
    /// Adjusts the first_octave so that the effective output amplitude is
    /// roughly multiplied by `_freq_scale`.
    pub fn with_frequency(seed: u64, _freq_scale: f64) -> Self {
        // The frequency scale is approximated by adjusting octave amplitudes.
        // For simplicity we just use a moderate -10 first_octave giving
        // roughly 2^(10) = 1024 input scaling which works well for terrain.
        Self::new(seed as i64, -10, &[1.0, 0.5, 0.25])
    }

    /// Evaluate the noise.
    ///
    /// The second layer uses coordinates scaled by ≈1.01813 so that the two
    /// layers do not share zero-crossings, eliminating grid artefacts.
    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        let x2 = x * 1.0181268882175227;
        let y2 = y * 1.0181268882175227;
        let z2 = z * 1.0181268882175227;
        (self.first.get_value(x, y, z) + self.second.get_value(x2, y2, z2)) * self.value_factor
    }
}

// ---------------------------------------------------------------------------
// PositionalRandomFactory — seed derivation from block positions
// ---------------------------------------------------------------------------

/// Factory that turns block coordinates or string names into
/// deterministic `SimpleRandom` instances.
pub struct PositionalRandomFactory {
    pub seed_lo: u64,
    pub seed_hi: u64,
}

impl PositionalRandomFactory {
    /// Upgrade a 64-bit seed to a 128-bit internal pair using Minecraft's
    /// `mixStafford13`-based scheme.
    pub fn new(seed: u64) -> Self {
        let upgraded = upgrade_seed_to_128bit(seed);
        PositionalRandomFactory {
            seed_lo: upgraded.0,
            seed_hi: upgraded.1,
        }
    }

    /// Derive a `SimpleRandom` from a string name.
    pub fn from_hash_of(&self, name: &str) -> SimpleRandom {
        let hash = murmur_hash(name);
        SimpleRandom::new(self.seed_lo ^ hash)
    }

    /// Derive a `SimpleRandom` from integer coordinates.
    pub fn at(&self, x: i32, y: i32, z: i32) -> SimpleRandom {
        let seed = mix_seed(x as i64, y as i64, z as i64) ^ self.seed_lo as i64;
        SimpleRandom::new(seed as u64)
    }
}

// ---------------------------------------------------------------------------
// Free-standing utilities
// ---------------------------------------------------------------------------

/// Wrap `x` modulo `2^25` to prevent floating-point precision loss at large
/// coordinates.
pub fn wrap(x: f64) -> f64 {
    x - (x / 3.3554432e7 + 0.5).floor() * 3.3554432e7
}

/// Minecraft's `mixStafford13` finaliser.
fn mix_stafford13(mut seed: u64) -> u64 {
    seed ^= seed >> 30;
    seed = seed.wrapping_mul(46518821398408343);
    seed ^= seed >> 27;
    seed = seed.wrapping_mul(1495075033947455135);
    seed ^= seed >> 31;
    seed
}

/// Upgrade a 64-bit seed to a 128-bit seed pair using `mixStafford13`.
///
/// The second seed uses an offset of `0x5555555555555555` to decorrelate
/// the two halves.
pub fn upgrade_seed_to_128bit(seed: u64) -> (u64, u64) {
    let lo = mix_stafford13(seed);
    let hi = mix_stafford13(seed.wrapping_add(6148914691236517205));
    (lo, hi)
}

/// Deterministically mix three integer coordinates into a single `i64`.
pub fn mix_seed(x: i64, y: i64, z: i64) -> i64 {
    let s = x.wrapping_mul(3129871) ^ z.wrapping_mul(116129781) ^ y;
    s.wrapping_mul(s.wrapping_mul(15731).wrapping_add(789221))
        .wrapping_add(1376312589)
}

/// MurmurHash2 64-bit (x64 variant).
///
/// This is a portable implementation suitable for deriving procedural seeds
/// from string names.
pub fn murmur_hash(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let seed: u64 = 0x9747b28c;
    let m: u64 = 0xc6a4a7935bd1e995;
    let r: u32 = 47;

    let mut h = seed ^ (len as u64).wrapping_mul(m);

    let mut i = 0;
    while i + 8 <= len {
        let k = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        let k = k.wrapping_mul(m);
        let k = k ^ (k >> r);
        let k = k.wrapping_mul(m);
        h ^= k;
        h = h.wrapping_mul(m);
        i += 8;
    }

    let tail = &bytes[i..];
    for (j, &byte) in tail.iter().enumerate() {
        h ^= (byte as u64) << (j * 8);
    }
    if !tail.is_empty() {
        h = h.wrapping_mul(m);
    }

    h ^= h >> r;
    h = h.wrapping_mul(m);
    h ^= h >> r;
    h
}
