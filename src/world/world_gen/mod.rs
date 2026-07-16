pub mod caves;
pub mod aquifer;
pub mod biome_source;
pub mod density_fn;
pub mod generator;
pub mod noise;
pub mod noise_router;
pub mod surface;
pub mod terrain;

pub use generator::VanillaWorldGenerator;
pub use biome_source::{OverworldBiomeSource, UnsupportedBiome};

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use ::noise::{NoiseFn, Simplex, SuperSimplex};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::collections::HashMap;

/// Parameters per biome for terrain height computation.
#[derive(Clone, Copy)]
struct BiomeParams {
    base_height: f64,
    amplitude: f64,
    scale: f64,
    surface: BlockId,
    subsurface: BlockId,
    deep: BlockId,
}

impl BiomeParams {
    const fn new(base_height: f64, amplitude: f64, scale: f64, surface: BlockId, subsurface: BlockId, deep: BlockId) -> Self {
        BiomeParams { base_height, amplitude, scale, surface, subsurface, deep }
    }
}

pub struct WorldGenerator {
    // Dedicated noise fields
    height_large: Simplex,       // largest scale continent shape
    height_mid: Simplex,         // medium terrain detail
    height_small: Simplex,       // fine detail
    height_tiny: Simplex,        // micro variation
    continental_noise: SuperSimplex,  // land/ocean separation
    erosion_noise: Simplex,      // terrain ruggedness
    peak_noise: Simplex,         // mountain peaks
    cave_noise: Simplex,
    cave_3d_noise: SuperSimplex,
    temp_noise: Simplex,         // temperature
    humidity_noise: Simplex,     // humidity
    weirdness_noise: Simplex,    // weirdness for biome variation
    river_noise: Simplex,
    biome_offsets: [(f64, f64); 8], // offsets for biome blending
    seed: u64,
    column_cache: Vec<(i32, Biome)>,
    cache_base_x: i64,
    cache_base_z: i64,
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        WorldGenerator {
            height_large: Simplex::new(seed as u32),
            height_mid: Simplex::new((seed + 1) as u32),
            height_small: Simplex::new((seed + 2) as u32),
            height_tiny: Simplex::new((seed + 3) as u32),
            continental_noise: SuperSimplex::new((seed + 4) as u32),
            erosion_noise: Simplex::new((seed + 5) as u32),
            peak_noise: Simplex::new((seed + 6) as u32),
            cave_noise: Simplex::new((seed + 7) as u32),
            cave_3d_noise: SuperSimplex::new((seed + 10) as u32),
            temp_noise: Simplex::new((seed + 8) as u32),
            humidity_noise: Simplex::new((seed + 9) as u32),
            weirdness_noise: Simplex::new((seed + 11) as u32),
            river_noise: Simplex::new((seed + 12) as u32),
            biome_offsets: [
                (8.0, 0.0), (-8.0, 0.0), (0.0, 8.0), (0.0, -8.0),
                (5.7, 5.7), (-5.7, 5.7), (5.7, -5.7), (-5.7, -5.7),
            ],
            seed,
            column_cache: Vec::with_capacity(256),
            cache_base_x: 0,
            cache_base_z: 0,
        }
    }

    const SEA_LEVEL: i32 = 63;

    fn mix_seed(mut value: u64) -> u64 {
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58476d1ce4e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }

    fn chunk_rng(&self, cx: i32, cz: i32, salt: u64) -> StdRng {
        let seed = self.seed
            ^ (cx as i64 as u64).wrapping_mul(0x9e3779b97f4a7c15)
            ^ (cz as i64 as u64).wrapping_mul(0xbf58476d1ce4e5b9)
            ^ salt;
        StdRng::seed_from_u64(Self::mix_seed(seed))
    }

    fn column_chance(&self, wx: i64, wz: i64, salt: u64, chance: f64) -> bool {
        let seed = self.seed
            ^ (wx as u64).wrapping_mul(0x9e3779b97f4a7c15)
            ^ (wz as u64).wrapping_mul(0xbf58476d1ce4e5b9)
            ^ salt;
        (Self::mix_seed(seed) >> 11) as f64 / ((1u64 << 53) as f64) < chance
    }

    /// Get the "climate" noise vector at a point: (temp, humidity, continental, weirdness)
    fn climate_at(&self, wx: f64, wz: f64) -> (f64, f64, f64, f64) {
        let temp = self.temp_noise.get([wx * 0.0015, wz * 0.0015]);
        let humidity = self.humidity_noise.get([wx * 0.002 + 1000.0, wz * 0.002 + 1000.0]);
        let continental = self.continental_noise.get([wx * 0.0008, wz * 0.0008]);
        let weirdness = self.weirdness_noise.get([wx * 0.0025 + 500.0, wz * 0.0025 + 500.0]);
        (temp, humidity, continental, weirdness)
    }

    pub fn get_biome(&self, wx: f64, wz: f64) -> Biome {
        let (temp, humidity, continental, weirdness) = self.climate_at(wx, wz);
        self.biome_from_climate(temp, humidity, continental, weirdness)
    }

    fn biome_at(&self, wx: f64, wz: f64) -> Biome {
        self.get_biome(wx, wz)
    }

    fn biome_from_climate(&self, temp: f64, humidity: f64, continental: f64, weirdness: f64) -> Biome {
        if continental < -0.3 {
            if temp > 0.3 {
                if humidity > 0.0 { Biome::LukewarmOcean } else { Biome::WarmOcean }
            } else if temp > -0.1 { Biome::LukewarmOcean }
            else if temp > -0.4 { Biome::ColdOcean }
            else { Biome::FrozenOcean }
        } else if continental < -0.1 {
            if temp > 0.3 {
                if humidity > 0.0 { Biome::DeepLukewarmOcean } else { Biome::DeepWarmOcean }
            } else if temp > -0.1 { Biome::DeepLukewarmOcean }
            else if temp > -0.4 { Biome::DeepColdOcean }
            else { Biome::DeepFrozenOcean }
        } else if continental < 0.05 {
            Biome::Beach
        } else if continental > 0.6 && weirdness > 0.5 {
            if temp < -0.4 { Biome::JaggedPeaks }
            else if temp < -0.1 { Biome::FrozenPeaks }
            else { Biome::StonyPeaks }
        } else if continental > 0.5 && weirdness > 0.3 {
            if temp > 0.2 {
                if humidity > 0.0 { Biome::WindsweptForest } else { Biome::WindsweptSavanna }
            } else if temp > -0.1 { Biome::WindsweptHills }
            else if humidity > 0.0 { Biome::WindsweptGravellyHills }
            else { Biome::WindsweptHills }
        } else if temp > 0.3 {
            if humidity > 0.3 {
                if weirdness > 0.3 { Biome::BambooJungle } else { Biome::Jungle }
            } else if humidity > -0.2 {
                if continental > 0.0 { Biome::Forest } else { Biome::Savanna }
            } else {
                if weirdness > 0.2 { Biome::Badlands } else { Biome::Desert }
            }
        } else if temp > -0.1 {
            if humidity > 0.5 { Biome::DarkForest }
            else if humidity > 0.2 {
                if continental > 0.2 {
                    if weirdness > 0.1 { Biome::FlowerForest } else { Biome::Forest }
                } else { Biome::Swamp }
            } else if humidity > -0.3 { Biome::Forest }
            else { if weirdness > 0.2 { Biome::SunflowerPlains } else { Biome::Plains } }
        } else if temp > -0.4 {
            if humidity > 0.3 { Biome::DarkForest }
            else if humidity > 0.0 {
                if continental > 0.4 {
                    if weirdness > 0.3 { Biome::OldGrowthPineTaiga } else { Biome::OldGrowthSpruceTaiga }
                } else { Biome::Taiga }
            } else { Biome::Plains }
        } else {
            if humidity > 0.0 {
                if weirdness > 0.4 { Biome::Grove }
                else if weirdness > 0.1 { Biome::SnowySlopes }
                else { Biome::SnowyTundra }
            } else { Biome::Mountains }
        }
    }

    /// Sample biome and params at (wx, wz), then blend with nearby samples
    /// for smooth transitions. Returns blended (height_i, dominant_biome).
    fn get_height(&self, wx: f64, wz: f64) -> (i32, Biome) {
        // Use cached value if available (populated by column loop in generate_chunk)
        let cx = wx as i64 - self.cache_base_x;
        let cz = wz as i64 - self.cache_base_z;
        if !self.column_cache.is_empty()
            && self.column_cache.len() == CHUNK_SIZE * CHUNK_SIZE
            && cx >= 0
            && cx < CHUNK_SIZE as i64
            && cz >= 0
            && cz < CHUNK_SIZE as i64
        {
            return self.column_cache[(cx as usize) * CHUNK_SIZE + cz as usize];
        }

        self.compute_height(wx, wz)
    }

    /// Compute blended biome parameters at (wx, wz) using all 8 offsets
    /// and full 4D climate-space weighting for smooth biome transitions.
    fn blended_params_at(&self, wx: f64, wz: f64) -> (BiomeParams, Biome) {
        let (t, h, c, w) = self.climate_at(wx, wz);
        let main_biome = self.biome_from_climate(t, h, c, w);
        let main_params = self.params_for(main_biome);

        let mut blended_base = main_params.base_height;
        let mut blended_amp = main_params.amplitude;
        let mut blended_scale = main_params.scale;
        let mut total_weight = 1.0;

        for &(dx, dz) in &self.biome_offsets {
            let ox = wx + dx;
            let oz = wz + dz;
            let (nt, nh, nc, nw) = self.climate_at(ox, oz);

            // Euclidean distance in 4D climate space.
            // Small distance => similar climate => higher blend weight.
            let cd = ((t - nt).powi(2) + (h - nh).powi(2)
                     + (c - nc).powi(2) + (w - nw).powi(2))
                     .sqrt();
            let wd = (dx * dx + dz * dz).sqrt();
            // Weight blends climate similarity with distance decay.
            let weight = (1.0 - cd.min(1.0)) * 0.3 / (wd * 0.05 + 1.0);

            if weight > 0.01 {
                let b2 = self.biome_from_climate(nt, nh, nc, nw);
                let p2 = self.params_for(b2);
                blended_base += p2.base_height * weight;
                blended_amp += p2.amplitude * weight;
                blended_scale += p2.scale * weight;
                total_weight += weight;
            }
        }

        blended_base /= total_weight;
        blended_amp /= total_weight;
        blended_scale /= total_weight;

        (BiomeParams::new(blended_base, blended_amp, blended_scale,
                          main_params.surface, main_params.subsurface, main_params.deep),
         main_biome)
    }

    /// Compute blended height without caching.
    fn compute_height(&self, wx: f64, wz: f64) -> (i32, Biome) {
        let (params, main_biome) = self.blended_params_at(wx, wz);

        let river = self.river_noise.get([wx * 0.008, wz * 0.008]);

        // Multi-octave terrain height (4 octaves)
        let scale = params.scale;
        let amp = params.amplitude;
        let h1 = self.height_large.get([wx * scale, wz * scale]) * amp;
        let h2 = self.height_mid.get([wx * scale * 2.0, wz * scale * 2.0]) * (amp * 0.5);
        let h3 = self.height_small.get([wx * scale * 4.0, wz * scale * 4.0]) * (amp * 0.25);
        let h4 = self.height_tiny.get([wx * scale * 8.0, wz * scale * 8.0]) * (amp * 0.125);

        // Height-aware erosion: more rugged at high altitude for jagged peaks
        let erosion = self.erosion_noise.get([wx * 0.005, wz * 0.005]);
        let height_frac = ((params.base_height - 55.0) / 45.0).max(0.0).min(1.0);
        let erosion_factor = 1.0 + erosion * (0.3 + height_frac * 0.4);

        // Ridge noise for natural mountain spine formations
        let ridge_offset = 200.0;
        let ridge = 1.0 - self.peak_noise.get([wx * 0.004 + ridge_offset, wz * 0.004 + ridge_offset]).abs();
        let ridge_boost = ridge.powi(2) * 15.0 * height_frac;

        // Peak boost for dramatic mountain tops
        let peak = self.peak_noise.get([wx * 0.008, wz * 0.008]);
        let peak_boost = (peak * 0.5 + 0.5).powi(3) * 20.0;

        let mut height = params.base_height + (h1 + h2 + h3 + h4) * erosion_factor + peak_boost + ridge_boost;

        // Rivers — wider gentler profile
        let river_strength = (river.abs() * 2.5 - 0.3).min(1.0).max(0.0);
        if river.abs() < 0.28 {
            let river_bed = Self::SEA_LEVEL as f64 - 4.0;
            height = height * (1.0 - river_strength * 0.6) + river_bed * river_strength * 0.6;
        }

        if main_biome == Biome::Beach {
            height = height.max(Self::SEA_LEVEL as f64 - 1.0);
        }
        let is_ocean = matches!(main_biome,
            Biome::Ocean | Biome::DeepOcean | Biome::WarmOcean | Biome::LukewarmOcean
            | Biome::ColdOcean | Biome::FrozenOcean | Biome::DeepWarmOcean
            | Biome::DeepLukewarmOcean | Biome::DeepColdOcean | Biome::DeepFrozenOcean);
        if is_ocean {
            height = (height - 12.0).max(48.0);
        }

        let height_i = (height as i32).max(1).min(CHUNK_HEIGHT as i32 - 1);
        (height_i, main_biome)
    }

    fn params_for(&self, biome: Biome) -> BiomeParams {
        match biome {
            Biome::Plains => BiomeParams::new(66.0, 6.0, 0.015, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Forest => BiomeParams::new(68.0, 12.0, 0.015, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Desert => BiomeParams::new(68.0, 5.0, 0.012, BlockId::Sand, BlockId::Sandstone, BlockId::Sandstone),
            Biome::Savanna => BiomeParams::new(67.0, 8.0, 0.013, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Taiga => BiomeParams::new(69.0, 15.0, 0.018, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::SnowyTundra => BiomeParams::new(70.0, 4.0, 0.01, BlockId::SnowBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Mountains => BiomeParams::new(72.0, 35.0, 0.025, BlockId::Stone, BlockId::Stone, BlockId::Stone),
            Biome::Swamp => BiomeParams::new(64.0, 3.0, 0.01, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Jungle => BiomeParams::new(69.0, 18.0, 0.017, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::DarkForest => BiomeParams::new(68.0, 12.0, 0.015, BlockId::Podzol, BlockId::Dirt, BlockId::Stone),
            Biome::FlowerForest => BiomeParams::new(68.0, 12.0, 0.015, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::SunflowerPlains => BiomeParams::new(66.0, 6.0, 0.015, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::CherryGrove => BiomeParams::new(68.0, 10.0, 0.016, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Meadow => BiomeParams::new(70.0, 8.0, 0.014, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Grove => BiomeParams::new(72.0, 14.0, 0.018, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::SnowySlopes => BiomeParams::new(78.0, 18.0, 0.02, BlockId::SnowBlock, BlockId::Dirt, BlockId::Stone),
            Biome::JaggedPeaks => BiomeParams::new(100.0, 30.0, 0.03, BlockId::Stone, BlockId::Stone, BlockId::Stone),
            Biome::FrozenPeaks => BiomeParams::new(100.0, 28.0, 0.028, BlockId::SnowBlock, BlockId::Stone, BlockId::Stone),
            Biome::StonyPeaks => BiomeParams::new(80.0, 22.0, 0.022, BlockId::Stone, BlockId::Stone, BlockId::Stone),
            Biome::OldGrowthPineTaiga => BiomeParams::new(70.0, 16.0, 0.018, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::OldGrowthSpruceTaiga => BiomeParams::new(70.0, 16.0, 0.018, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::MushroomFields => BiomeParams::new(66.0, 5.0, 0.012, BlockId::Mycelium, BlockId::Dirt, BlockId::Stone),
            Biome::BambooJungle => BiomeParams::new(69.0, 18.0, 0.017, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Badlands => BiomeParams::new(70.0, 8.0, 0.015, BlockId::RedSand, BlockId::RedSand, BlockId::Terracotta),
            Biome::WoodedBadlands => BiomeParams::new(72.0, 10.0, 0.016, BlockId::CoarseDirt, BlockId::RedSand, BlockId::Terracotta),
            Biome::ErodedBadlands => BiomeParams::new(68.0, 12.0, 0.018, BlockId::RedSand, BlockId::RedSand, BlockId::Terracotta),
            Biome::Beach => BiomeParams::new(64.0, 2.0, 0.008, BlockId::Sand, BlockId::Sand, BlockId::Sandstone),
            // SEA_LEVEL is 63. River base_height is 62.0 (1 below sea level) so that river
            // noise carves channels below the water surface. This is intentionally below
            // sea level and is not a bug — the separate river noise function handles the
            // actual channel carving above the noise floor.
            Biome::River => BiomeParams::new(62.0, 1.0, 0.005, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::Ocean | Biome::DeepOcean | Biome::WarmOcean | Biome::LukewarmOcean
            | Biome::ColdOcean | Biome::FrozenOcean | Biome::DeepWarmOcean
            | Biome::DeepLukewarmOcean | Biome::DeepColdOcean | Biome::DeepFrozenOcean => {
                BiomeParams::new(55.0, 6.0, 0.012, BlockId::Water, BlockId::Sand, BlockId::Stone)
            }
            Biome::WindsweptHills => BiomeParams::new(72.0, 25.0, 0.022, BlockId::Stone, BlockId::Stone, BlockId::Stone),
            Biome::WindsweptForest => BiomeParams::new(70.0, 22.0, 0.02, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::WindsweptSavanna => BiomeParams::new(69.0, 20.0, 0.019, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::WindsweptGravellyHills => BiomeParams::new(73.0, 28.0, 0.024, BlockId::Gravel, BlockId::Stone, BlockId::Stone),
            Biome::BirchForest | Biome::PaleGarden => BiomeParams::new(68.0, 12.0, 0.015, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::SavannaPlateau => BiomeParams::new(69.0, 12.0, 0.015, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::SnowyTaiga | Biome::IceSpikes => BiomeParams::new(70.0, 8.0, 0.012, BlockId::SnowBlock, BlockId::Dirt, BlockId::Stone),
            Biome::MangroveSwamp | Biome::StonyShore | Biome::SnowyBeach | Biome::FrozenRiver => BiomeParams::new(63.0, 3.0, 0.01, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::SparseJungle => BiomeParams::new(69.0, 16.0, 0.017, BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
            Biome::DripstoneCaves | Biome::LushCaves | Biome::SulfurCaves | Biome::DeepDark => BiomeParams::new(50.0, 4.0, 0.01, BlockId::Stone, BlockId::Stone, BlockId::Stone),
        }
    }

    /// Get surface blocks for a column, with blending at biome edges.
    /// Uses weighted voting across all 8 offsets so block types transition smoothly.
    fn surface_blocks_for(&self, wx: f64, wz: f64, _height: i32) -> (BlockId, BlockId, BlockId) {
        let mut surface_w: HashMap<BlockId, f64> = HashMap::new();
        let mut subsurface_w: HashMap<BlockId, f64> = HashMap::new();
        let mut deep_w: HashMap<BlockId, f64> = HashMap::new();

        // Center point gets full weight so local biome dominates
        let center = self.params_for(self.biome_at(wx, wz));
        *surface_w.entry(center.surface).or_insert(0.0) += 1.0;
        *subsurface_w.entry(center.subsurface).or_insert(0.0) += 1.0;
        *deep_w.entry(center.deep).or_insert(0.0) += 1.0;

        for &(dx, dz) in &self.biome_offsets {
            let ox = wx + dx;
            let oz = wz + dz;
            let dist = (dx * dx + dz * dz).sqrt();
            let w = 1.0 / (dist + 1.0);
            let p = self.params_for(self.biome_at(ox, oz));
            *surface_w.entry(p.surface).or_insert(0.0) += w;
            *subsurface_w.entry(p.subsurface).or_insert(0.0) += w;
            *deep_w.entry(p.deep).or_insert(0.0) += w;
        }

        let surface = Self::highest_weighted_block(surface_w, BlockId::GrassBlock);
        let subsurface = Self::highest_weighted_block(subsurface_w, BlockId::Dirt);
        let deep = Self::highest_weighted_block(deep_w, BlockId::Stone);

        (surface, subsurface, deep)
    }

    fn highest_weighted_block(weights: HashMap<BlockId, f64>, fallback: BlockId) -> BlockId {
        weights
            .into_iter()
            .max_by(|(a_id, a_weight), (b_id, b_weight)| {
                a_weight
                    .total_cmp(b_weight)
                    .then_with(|| (*a_id as u16).cmp(&(*b_id as u16)))
            })
            .map(|(id, _)| id)
            .unwrap_or(fallback)
    }

    fn surface_water_near(&self, wx: f64, wz: f64, radius: i32) -> bool {
        for distance in 1..=radius {
            for (dx, dz) in [(-distance, 0), (distance, 0), (0, -distance), (0, distance)] {
                let (height, _) = self.compute_height(wx + dx as f64, wz + dz as f64);
                // Terrain fills ocean and river water only above surfaces below
                // sea level; a dry y=63 column is not a shoreline source.
                if height < Self::SEA_LEVEL {
                    return true;
                }
            }
        }
        false
    }

    fn is_cave(&self, wx: f64, wy: f64, wz: f64) -> bool {
        // Legacy per-block cave check using 3D noise (XZ only)
        let cave_scale = 0.025;
        let n = self
            .cave_3d_noise
            .get([wx * cave_scale, wy * cave_scale * 0.6, wz * cave_scale]);
        n > 0.35 && wy > 4.0 && wy < 55.0
    }

    fn is_noise_cave(&self, wx: f64, wy: f64, wz: f64) -> bool {
        // Modern 1.18+ noise caves using 3D SuperSimplex noise.
        // Produces more natural cave shapes with branching tunnels and chambers.
        if wy < 2.0 || wy > 55.0 {
            return false;
        }
        let scale1 = 0.025;
        let scale2 = 0.05;
        let scale3 = 0.012;

        let n1 = self.cave_3d_noise.get([wx * scale1, wy * scale1, wz * scale1]);
        let n2 = self.cave_3d_noise.get([wx * scale2 + 500.0, wy * scale2 + 500.0, wz * scale2 + 500.0]);
        let n3 = self.cave_3d_noise.get([wx * scale3 - 1000.0, wy * scale3 - 1000.0, wz * scale3 - 1000.0]);

        // Combine octaves: main shape + detail + large scale variation
        let combined = n1 * 1.0 + n2 * 0.4 + n3 * 0.6;

        // Carve where combined noise exceeds threshold
        // Use a height-dependent threshold to make caves rarer near surface
        let depth_factor = ((55.0 - wy) / 50.0).min(1.0).max(0.0);
        let threshold = 0.1 - depth_factor * 0.15;

        combined > threshold
    }

    fn carve_cheese_caves(&self, chunk: &mut Chunk, base_x: i64, base_z: i64) {
        // Each 16x16 cell has up to 3 potential rooms, placed by world-space noise.
        // Use cell-aligned coordinates so adjacent chunks agree on room placement.
        let cell_x = (base_x as f64 / 16.0).floor() * 16.0;
        let cell_z = (base_z as f64 / 16.0).floor() * 16.0;
        for i in 0..3 {
            let wx = cell_x + 8.0 + i as f64 * 7.3;
            let wz = cell_z + 8.0 + i as f64 * 11.7;
            let placement = self.cave_3d_noise.get([wx * 0.03, wz * 0.03]);
            if placement < -0.1 {
                continue;
            }
            let pn = placement * 0.5 + 0.5;
            let cx = ((pn * 12.0 + 2.0) as i32).min(CHUNK_SIZE as i32 - 1);
            let cz = (((pn * 12.0 + 2.0) * 3.7) as i32 % CHUNK_SIZE as i32).min(CHUNK_SIZE as i32 - 1);
            let cy = 8 + ((self.cave_noise.get([wx * 0.05 + 100.0, wz * 0.05 + 100.0]) * 0.5 + 0.5) * 38.0) as i32;
            let rn = self.cave_noise.get([wx * 0.03 + 200.0, wz * 0.03 + 200.0]) * 0.5 + 0.5;
            let radius_x = 4.0 + rn * 6.0;
            let radius_y = 3.0 + rn * 5.0;
            let radius_z = 4.0 + rn * 6.0;

            let rx = radius_x.ceil() as i32;
            let ry = radius_y.ceil() as i32;
            let rz = radius_z.ceil() as i32;

            for dx in -rx..=rx {
                for dy in -ry..=ry {
                    for dz in -rz..=rz {
                        let bx = cx + dx;
                        let by = cy + dy;
                        let bz = cz + dz;
                        if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                            continue;
                        }
                        if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 {
                            continue;
                        }
                        let dist = (dx as f64 / radius_x).powi(2)
                            + (dy as f64 / radius_y).powi(2)
                            + (dz as f64 / radius_z).powi(2);
                        if dist > 1.0 {
                            continue;
                        }
                        let block = chunk.get_block(bx as usize, by as usize, bz as usize);
                        if self.is_stone_type(block.id)
                            || block.id == BlockId::Dirt
                            || block.id == BlockId::GrassBlock
                            || block.id == BlockId::Gravel
                            || block.id == BlockId::SoulSand
                        {
                            chunk.set_block(bx as usize, by as usize, bz as usize, Block::air());
                        }
                    }
                }
            }
        }
    }

    fn carve_caves(&self, chunk: &mut Chunk, base_x: i64, base_z: i64) {
        // Use world-space noise to guide tunnel paths.
        let cell_x = (base_x as f64 / 16.0).floor() * 16.0;
        let cell_z = (base_z as f64 / 16.0).floor() * 16.0;
        for i in 0..4 {
            let wx = cell_x + 8.0 + i as f64 * 9.1;
            let wz = cell_z + 8.0 + i as f64 * 13.3;
            let placement = self.cave_3d_noise.get([wx * 0.02 + 300.0, wz * 0.02 + 300.0]);
            if placement < -0.2 {
                continue;
            }
            let start_x = ((placement * 0.5 + 0.5) * 14.0 + 1.0) as f64;
            let start_z = ((placement * 0.5 + 0.5) * 14.0 + 1.0) as f64;
            let start_y = 8.0 + ((self.cave_noise.get([wx * 0.04 + 400.0, wz * 0.04 + 400.0]) * 0.5 + 0.5) * 40.0);

            let mut cx = start_x;
            let mut cy = start_y;
            let mut cz = start_z;
            let length = 10 + ((placement * 0.5 + 0.5) * 30.0) as usize;
            let base_radius = 1.5 + (placement * 0.5 + 0.5) * 2.5;
            let yaw = self.cave_noise.get([wx * 0.02, wz * 0.02]) * std::f64::consts::PI;
            let pitch = self.cave_noise.get([wx * 0.03 + 100.0, wz * 0.03 + 100.0]) * 0.3;

            for step in 0..length {
                let t = step as f64 / length as f64;
                let wx_step = cell_x + 8.0 + i as f64 * 9.1 + t * 20.0;
                let wz_step = cell_z + 8.0 + i as f64 * 13.3 + t * 20.0;
                let noise_angle = self.cave_3d_noise.get([wx_step * 0.03, wz_step * 0.03]);
                let radius = base_radius * (1.0 - t * 0.5);
                let angle = yaw + t * 0.5 + noise_angle * 0.5;
                let vy = pitch + (t - 0.5) * 0.3 + noise_angle * 0.2;
                cx += angle.cos() * 1.5;
                cz += angle.sin() * 1.5;
                cy += vy * 1.0;

                let rad_int = radius.ceil() as i32;
                for dx in -rad_int..=rad_int {
                    for dy in -rad_int..=rad_int {
                        for dz in -rad_int..=rad_int {
                            let bx = cx as i32 + dx;
                            let by = cy as i32 + dy;
                            let bz = cz as i32 + dz;
                            if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                                continue;
                            }
                            if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 {
                                continue;
                            }
                            let dist = (dx as f64).powi(2) + (dy as f64).powi(2) + (dz as f64).powi(2);
                            if dist > radius * radius {
                                continue;
                            }
                            let block = chunk.get_block(bx as usize, by as usize, bz as usize);
                            if block.id == BlockId::Stone
                                || block.id == BlockId::Deepslate
                                || block.id == BlockId::Dirt
                                || block.id == BlockId::GrassBlock
                                || block.id == BlockId::SoulSand
                                || block.id == BlockId::Gravel
                            {
                                chunk.set_block(bx as usize, by as usize, bz as usize, Block::air());
                            }
                        }
                    }
                }
            }
        }
    }

    fn is_stone_type(&self, id: BlockId) -> bool {
        matches!(id, BlockId::Stone | BlockId::Deepslate | BlockId::Granite | BlockId::Diorite | BlockId::Andesite | BlockId::Tuff)
    }

    fn ore_noise_check(&self, wx: f64, wy: f64, wz: f64, scale: f64, offset: f64, threshold: f64) -> bool {
        let n1 = self.cave_3d_noise.get([wx * scale * 0.5 + offset, wy * scale * 0.5, wz * scale * 0.5 + offset]);
        let n2 = self.cave_noise.get([wx * scale + offset * 2.0, wy * scale, wz * scale + offset * 2.0]);
        n1 * 0.6 + n2 * 0.4 > threshold
    }

    pub fn generate_ores(&self, chunk: &mut Chunk, base_x: i64, base_z: i64) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                // No configured ore distribution reaches Y=128. Avoid scanning
                // the remaining two thirds of this 384-block-tall chunk.
                for y in 1..CHUNK_HEIGHT.min(128) {
                    let wy = y as f64;
                    let block_id = chunk.get_block(x, y, z).id;
                    if !self.is_stone_type(block_id) {
                        continue;
                    }
                    let is_deepslate = block_id == BlockId::Deepslate;

                    let placed = if y < 128 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.025, 0.0, 0.28);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateCoalOre } else { BlockId::CoalOre }));
                            true
                        } else {
                            false
                        }
                    } else { false };

                    if placed { continue; }

                    let placed = if y < 63 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.03, 100.0, 0.3);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateIronOre } else { BlockId::IronOre }));
                            true
                        } else {
                            false
                        }
                    } else { false };

                    if placed { continue; }

                    let placed = if y < 96 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.035, 200.0, 0.32);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateCopperOre } else { BlockId::CopperOre }));
                            true
                        } else {
                            false
                        }
                    } else { false };

                    if placed { continue; }

                    let placed = if y < 32 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.04, 300.0, 0.35);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateGoldOre } else { BlockId::GoldOre }));
                            true
                        } else {
                            false
                        }
                    } else { false };

                    if placed { continue; }

                    let placed = if y < 16 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.045, 400.0, 0.35);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateRedstoneOre } else { BlockId::RedstoneOre }));
                            true
                        } else {
                            false
                        }
                    } else { false };

                    if placed { continue; }

                    let placed = if y < 32 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.05, 500.0, 0.38);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateLapisOre } else { BlockId::LapisOre }));
                            true
                        } else {
                            false
                        }
                    } else { false };

                    if placed { continue; }

                    if y < 16 {
                        let n = self.ore_noise_check(wx, wy, wz, 0.05, 600.0, 0.4);
                        if n {
                            chunk.set_block(x, y, z, Block::new(if is_deepslate { BlockId::DeepslateDiamondOre } else { BlockId::DiamondOre }));
                        }
                    }
                }
            }
        }

        // Emeralds: only in Mountains biome
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                if !matches!(self.get_biome(wx, wz), Biome::Mountains) {
                    continue;
                }
                for y in 4..32 {
                    let wy = y as f64;
                    let block_id = chunk.get_block(x, y, z).id;
                    if block_id != BlockId::Stone && block_id != BlockId::Deepslate {
                        continue;
                    }
                    let n = self.ore_noise_check(wx, wy, wz, 0.07, 700.0, 0.45);
                    if n {
                        chunk.set_block(x, y, z, Block::new(if block_id == BlockId::Deepslate { BlockId::DeepslateEmeraldOre } else { BlockId::EmeraldOre }));
                    }
                }
            }
        }
    }

    pub fn generate_large_ore_veins(&self, chunk: &mut Chunk, base_x: i64, base_z: i64) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                for y in 1..32 {
                    let wy = y as f64;
                    let block_id = chunk.get_block(x, y, z).id;
                    if block_id != BlockId::Stone && block_id != BlockId::Deepslate {
                        continue;
                    }
                    // Iron large vein: broad horizontal sheets
                    let iron_n = self.cave_3d_noise.get([wx * 0.008 + 800.0, wy * 0.015, wz * 0.008 + 800.0]);
                    let iron_detail = self.cave_noise.get([wx * 0.03, wy * 0.03, wz * 0.03]);
                    if iron_n > 0.15 && iron_detail > -0.3 {
                        if iron_detail > 0.1 {
                            let ore_block = if y < 16 { BlockId::DeepslateIronOre } else { BlockId::IronOre };
                            chunk.set_block(x, y, z, Block::new(ore_block));
                        } else {
                            chunk.set_block(x, y, z, Block::new(BlockId::Tuff));
                        }
                        continue;
                    }
                    // Copper large vein: medium horizontal sheets, y up to 50
                    if y < 50 {
                        let copper_n = self.cave_3d_noise.get([wx * 0.01 + 900.0, wy * 0.02, wz * 0.01 + 900.0]);
                        if copper_n > 0.2 {
                            let ore_block = if y < 16 { BlockId::DeepslateCopperOre } else { BlockId::CopperOre };
                            chunk.set_block(x, y, z, Block::new(ore_block));
                        }
                    }
                }
            }
        }
    }

    pub fn generate_chunk(&mut self, chunk: &mut Chunk) {
        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;
        self.cache_base_x = base_x;
        self.cache_base_z = base_z;

        self.column_cache.clear();
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.compute_height(wx, wz);
                self.column_cache.push((height, biome));
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                self.generate_terrain_column(chunk, x, z);
            }
        }

        self.carve_caves_and_ores(chunk, base_x, base_z);

        let center_wx = (base_x + 8i64) as f64;
        let center_wz = (base_z + 8i64) as f64;
        let (_center_h, center_biome) = self.get_height(center_wx, center_wz);

        let mut decoration_rng = self.chunk_rng(chunk.cx, chunk.cz, 0x6a09e667f3bcc909);
        self.decorate_biome_features(chunk, &mut decoration_rng, base_x, base_z, center_biome);
        let mut structure_rng = self.chunk_rng(chunk.cx, chunk.cz, 0xbb67ae8584caa73b);
        self.place_structures(chunk, &mut structure_rng, base_x, base_z, center_biome);
        let mut ocean_rng = self.chunk_rng(chunk.cx, chunk.cz, 0x3c6ef372fe94f82b);
        self.generate_ocean_flora(chunk, &mut ocean_rng, base_x, base_z);

        chunk.recount_fluids();
        chunk.is_dirty = true;
    }

    fn generate_terrain_column(&self, chunk: &mut Chunk, x: usize, z: usize) {
        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;
        let wx = (base_x + x as i64) as f64;
        let wz = (base_z + z as i64) as f64;
        let idx = x * CHUNK_SIZE + z;
        let (surface_height, biome) = self.column_cache[idx];

        chunk.set_block(x, 0, z, Block::new(BlockId::Bedrock));

        let (surface_block, subsurface_block, deep_block) = self.surface_blocks_for(wx, wz, surface_height);

        let (surface_block, subsurface_block, deep_block) = if biome == Biome::Beach {
            (BlockId::Sand, BlockId::Sand, BlockId::Sandstone)
        } else if matches!(biome,
            Biome::Ocean | Biome::DeepOcean | Biome::WarmOcean | Biome::LukewarmOcean
            | Biome::ColdOcean | Biome::FrozenOcean | Biome::DeepWarmOcean
            | Biome::DeepLukewarmOcean | Biome::DeepColdOcean | Biome::DeepFrozenOcean)
        {
            let floor = if surface_height > Self::SEA_LEVEL - 5 { BlockId::Sand } else { BlockId::Gravel };
            (BlockId::Water, floor, BlockId::Stone)
        } else {
            (surface_block, subsurface_block, deep_block)
        };

        for y in 1..surface_height {
            let y_usize = y as usize;

            if self.is_noise_cave(wx, y as f64, wz) && y > 3 && y < surface_height - 2 {
                chunk.set_block(x, y_usize, z, Block::air());
                continue;
            }
            if self.is_cave(wx, y as f64, wz) && y > 3 && y < surface_height - 2 {
                chunk.set_block(x, y_usize, z, Block::air());
                continue;
            }

            let stone_type = if deep_block != BlockId::Stone {
                deep_block
            } else if y < 16 {
                let deepslate_chance = 1.0 - (y as f64 / 16.0);
                let deep_noise = self.erosion_noise.get([wx * 0.05 + 100.0, wz * 0.05 + 100.0]);
                if deep_noise < deepslate_chance * 2.0 - 1.0 {
                    BlockId::Deepslate
                } else {
                    let variant_noise = self.erosion_noise.get([wx * 0.03, wz * 0.03]);
                    if variant_noise > 0.4 {
                        BlockId::Granite
                    } else if variant_noise < -0.4 {
                        BlockId::Andesite
                    } else {
                        BlockId::Stone
                    }
                }
            } else {
                let variant_noise = self.erosion_noise.get([wx * 0.03, wz * 0.03]);
                if variant_noise > 0.4 {
                    BlockId::Granite
                } else if variant_noise < -0.4 {
                    BlockId::Andesite
                } else {
                    BlockId::Stone
                }
            };

            let block_to_place = if y == surface_height - 1 {
                Block::new(surface_block)
            } else if y >= surface_height - 4 {
                Block::new(subsurface_block)
            } else {
                Block::new(stone_type)
            };
            chunk.set_block(x, y_usize, z, block_to_place);
        }

        if surface_height < Self::SEA_LEVEL {
            for y in surface_height..Self::SEA_LEVEL {
                if y > 0 && y < CHUNK_HEIGHT as i32 {
                    let existing = chunk.get_block(x, y as usize, z);
                    if existing.is_air() {
                        chunk.set_block(x, y as usize, z, Block::new(BlockId::Water));
                    }
                }
            }
        }

        let surface_idx = (surface_height - 1).max(0) as usize;
        if surface_height <= Self::SEA_LEVEL + 2
            && surface_height >= Self::SEA_LEVEL - 1
            && biome != Biome::Desert
        {
            if chunk.get_block(x, surface_idx, z).id == BlockId::GrassBlock {
                if self.surface_water_near(wx, wz, 2) {
                    chunk.set_block(x, surface_idx, z, Block::new(BlockId::Sand));
                }
            }
        }

        if surface_height >= Self::SEA_LEVEL - 2 && surface_height <= Self::SEA_LEVEL + 3 {
            if self.surface_water_near(wx, wz, 1) {
                for by in (Self::SEA_LEVEL - 2).max(1)..=(surface_height - 1).min(Self::SEA_LEVEL + 1) {
                    let block = chunk.get_block(x, by as usize, z);
                    if block.id == BlockId::Dirt
                        && self.column_chance(wx as i64, wz as i64, by as u64 ^ 0x510e527fade682d1, 0.35)
                    {
                        chunk.set_block(x, by as usize, z, Block::new(BlockId::Sand));
                    }
                }
            }
        }

        if matches!(biome, Biome::SnowyTundra | Biome::Taiga) && surface_height > Self::SEA_LEVEL {
            let snow_block = chunk.get_block(x, surface_idx, z);
            if snow_block.id == BlockId::GrassBlock || snow_block.id == BlockId::Stone {
                chunk.set_block(x, surface_idx, z, Block::new(BlockId::SnowBlock));
                if surface_height < CHUNK_HEIGHT as i32 {
                    let above = chunk.get_block(x, surface_height as usize, z);
                    if above.is_air() {
                        chunk.set_block(x, surface_height as usize, z, Block::new(BlockId::Snow));
                    }
                }
            }
        }
    }

    fn carve_caves_and_ores(&self, chunk: &mut Chunk, base_x: i64, base_z: i64) {
        self.generate_ores(chunk, base_x, base_z);
        self.generate_large_ore_veins(chunk, base_x, base_z);

        // Noise-based aquifer placement (water and lava pools)
        let cell_x = (base_x as f64 / 16.0).floor() * 16.0;
        let cell_z = (base_z as f64 / 16.0).floor() * 16.0;
        for i in 0..5 {
            let wx = cell_x + 8.0 + i as f64 * 7.3;
            let wz = cell_z + 8.0 + i as f64 * 11.7;
            let n = self.cave_3d_noise.get([wx * 0.03, wz * 0.03]);
            if n < -0.2 {
                continue;
            }
            let pn = n * 0.5 + 0.5;
            let ax = ((pn * 12.0 + 2.0) as i32).min(CHUNK_SIZE as i32 - 1);
            let az = (((pn * 12.0 + 2.0) * 3.7) as i32 % CHUNK_SIZE as i32).min(CHUNK_SIZE as i32 - 1);
            let ay = 12 + ((self.cave_noise.get([wx * 0.04 + 500.0, wz * 0.04 + 500.0]) * 0.5 + 0.5) * 38.0) as i32;
            let radius = 1.5 + pn * 3.0;
            let is_lava = ay <= 10 || self.cave_noise.get([wx * 0.05 + 600.0, wz * 0.05 + 600.0]) > 0.2;
            let fill_block = if is_lava { BlockId::Lava } else { BlockId::Water };
            let fill_level = ay + (radius * 0.4) as i32;

            for dx in -(radius as i32)..=radius as i32 {
                for dy in -(radius as i32)..=radius as i32 {
                    for dz in -(radius as i32)..=radius as i32 {
                        let bx = ax + dx;
                        let by = ay + dy;
                        let bz = az + dz;
                        if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                            continue;
                        }
                        if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 {
                            continue;
                        }
                        let dist = (dx as f64).powi(2) + (dy as f64).powi(2) + (dz as f64).powi(2);
                        if dist > radius * radius {
                            continue;
                        }
                        if !self.is_stone_type(chunk.get_block(bx as usize, by as usize, bz as usize).id) {
                            continue;
                        }
                        if by <= fill_level {
                            chunk.set_block(bx as usize, by as usize, bz as usize, Block::new(fill_block));
                        }
                    }
                }
            }
        }

        self.carve_cheese_caves(chunk, base_x, base_z);
        self.carve_caves(chunk, base_x, base_z);

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let river = self.river_noise.get([wx * 0.008, wz * 0.008]);
                if river.abs() >= 0.25 {
                    continue;
                }
                let surface_h = self.column_cache[x * CHUNK_SIZE + z].0;
                if surface_h < 2 {
                    continue;
                }

                let river_strength = 1.0 - river.abs() / 0.25;
                let channel_depth = (river_strength * 3.0).ceil() as i32;

                for dy in 1..=channel_depth.min(surface_h - 1) {
                    chunk.set_block(x, (surface_h - dy) as usize, z, Block::air());
                }

                let channel_bottom = surface_h - channel_depth;
                for y in channel_bottom..Self::SEA_LEVEL {
                    if y >= 1 && y < CHUNK_HEIGHT as i32 {
                        let existing = chunk.get_block(x, y as usize, z);
                        if existing.is_air() {
                            chunk.set_block(x, y as usize, z, Block::new(BlockId::Water));
                        }
                    }
                }
            }
        }
    }

    fn decorate_biome_features(&self, chunk: &mut Chunk, rng: &mut StdRng, base_x: i64, base_z: i64, center_biome: Biome) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let surface_h = height.max(1).min(CHUNK_HEIGHT as i32 - 1);
                let surface_y = (surface_h - 1).max(0) as usize;
                if surface_h < Self::SEA_LEVEL || surface_h >= CHUNK_HEIGHT as i32 - 1 {
                    continue;
                }

                let top_block = chunk.get_block(x, surface_y, z);
                let above_block = chunk.get_block(x, surface_y + 1, z);
                if top_block.id != BlockId::GrassBlock || !above_block.is_air() {
                    continue;
                }

                let flower_chance = match biome {
                    Biome::Plains => 0.12,
                    Biome::Forest => 0.08,
                    Biome::Taiga => 0.03,
                    Biome::Jungle => 0.10,
                    Biome::Swamp => 0.06,
                    Biome::Savanna => 0.05,
                    Biome::DarkForest => 0.02,
                    _ => 0.0,
                };
                let grass_chance = match biome {
                    Biome::Plains => 0.20,
                    Biome::Forest => 0.15,
                    Biome::Savanna => 0.25,
                    Biome::Swamp => 0.10,
                    Biome::Jungle => 0.20,
                    Biome::Taiga => 0.05,
                    Biome::DarkForest => 0.05,
                    _ => 0.0,
                };

                if rng.random_bool(flower_chance) {
                    let flower = match biome {
                        Biome::Plains => {
                            let f = rng.random_range(0..100);
                            if f < 30 {
                                BlockId::Dandelion
                            } else if f < 55 {
                                BlockId::Poppy
                            } else if f < 70 {
                                BlockId::OxeyeDaisy
                            } else if f < 85 {
                                BlockId::Cornflower
                            } else {
                                BlockId::AzureBluet
                            }
                        }
                        Biome::Forest => {
                            if rng.random_bool(0.5) {
                                BlockId::Dandelion
                            } else {
                                BlockId::Poppy
                            }
                        }
                        Biome::Swamp => BlockId::BlueOrchid,
                        Biome::Jungle => BlockId::Dandelion,
                        Biome::Taiga => BlockId::Poppy,
                        Biome::Savanna => BlockId::Dandelion,
                        _ => BlockId::Dandelion,
                    };
                    chunk.set_block(x, surface_y + 1, z, Block::new(flower));
                } else if rng.random_bool(grass_chance) {
                    let is_fern =
                        matches!(biome, Biome::Taiga | Biome::Jungle) && rng.random_bool(0.5);
                    chunk.set_block(x, surface_y + 1, z, Block::new(if is_fern { BlockId::Fern } else { BlockId::Grass }));
                }
            }
        }

        let tree_density = match center_biome {
            Biome::Forest => 0.35,
            Biome::Taiga => 0.30,
            Biome::Jungle => 0.40,
            Biome::Savanna => 0.15,
            Biome::Swamp => 0.20,
            Biome::Plains => 0.05,
            Biome::DarkForest => 0.40,
            _ => 0.0,
        };
        let mut tree_positions: Vec<(i32, i32)> = Vec::new();
        for _ in 0..10 {
            if rng.random_bool(tree_density) {
                let tx = rng.random_range(4..CHUNK_SIZE - 4);
                let tz = rng.random_range(4..CHUNK_SIZE - 4);
                let wx = (base_x + tx as i64) as f64;
                let wz = (base_z + tz as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);

                let surface_is_solid = if height > 0 && height < CHUNK_HEIGHT as i32 {
                    let block = chunk.get_block(tx, height as usize - 1, tz);
                    !block.is_air() && block.id != BlockId::Water
                } else {
                    false
                };

                let too_close = tree_positions
                    .iter()
                    .any(|(ptx, ptz)| (tx as i32 - *ptx).abs() < 5 && (tz as i32 - *ptz).abs() < 5);

                if height > Self::SEA_LEVEL && height < 130 && surface_is_solid && !too_close {
                    let ground = height - 1;
                    tree_positions.push((tx as i32, tz as i32));
                    match biome {
                        Biome::Forest | Biome::Plains | Biome::Swamp => {
                            let tree_roll = rng.random::<f64>();
                            if tree_roll < 0.35 {
                                self.place_birch_tree(chunk, tx, tz, ground, rng);
                            } else {
                                self.place_tree(chunk, tx, tz, ground, BlockId::OakLog, BlockId::OakLeaves, rng);
                            }
                        }
                        Biome::Taiga => {
                            self.place_spruce_tree(chunk, tx, tz, ground, rng);
                        }
                        Biome::Jungle => {
                            self.place_jungle_tree(chunk, tx, tz, ground, rng);
                        }
                        Biome::Savanna => {
                            self.place_tree(chunk, tx, tz, ground, BlockId::AcaciaLog, BlockId::AcaciaLeaves, rng);
                        }
                        Biome::DarkForest => {
                            self.place_dark_oak_tree(chunk, tx, tz, ground, rng);
                        }
                        _ => {}
                    }
                }
            }
        }

        if matches!(center_biome, Biome::Forest | Biome::Taiga | Biome::Jungle | Biome::DarkForest) {
            for _ in 0..2 {
                if !rng.random_bool(0.3) {
                    continue;
                }
                let fx = rng.random_range(2..CHUNK_SIZE - 3);
                let fz = rng.random_range(2..CHUNK_SIZE - 3);
                let wx = (base_x + fx as i64) as f64;
                let wz = (base_z + fz as i64) as f64;
                let (fh, _fb) = self.get_height(wx, wz);
                let ground = fh - 1;
                if ground < 2 || ground >= CHUNK_HEIGHT as i32 - 2 {
                    continue;
                }
                let log_len = 3 + (rng.random::<f64>().abs() * 3.0) as usize;
                let axis = rng.random_bool(0.5);
                let log_id = match center_biome {
                    Biome::Jungle => BlockId::JungleLog,
                    Biome::Taiga => BlockId::SpruceLog,
                    Biome::DarkForest => BlockId::DarkOakLog,
                    _ => BlockId::OakLog,
                };
                for li in 0..log_len {
                    let lx = if axis { fx + li } else { fx };
                    let lz = if axis { fz } else { fz + li };
                    if lx >= CHUNK_SIZE || lz >= CHUNK_SIZE {
                        break;
                    }
                    let above = chunk.get_block(lx, ground as usize + 1, lz);
                    if above.is_air() {
                        chunk.set_block(lx, ground as usize + 1, lz, Block::new(log_id));
                    }
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (_height, v_biome) = self.get_height(wx, wz);
                if !matches!(v_biome, Biome::Jungle | Biome::Swamp | Biome::DarkForest) {
                    continue;
                }
                for y in (2..CHUNK_HEIGHT - 1).rev() {
                    let block = chunk.get_block(x, y, z);
                    if block.id == BlockId::JungleLeaves
                        || block.id == BlockId::OakLeaves
                        || block.id == BlockId::DarkOakLeaves
                    {
                        let has_air_side = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                            .iter()
                            .any(|(dx, dz)| {
                                let nx = x as i32 + dx;
                                let nz = z as i32 + dz;
                                if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                                    chunk.get_block(nx as usize, y, nz as usize).is_air()
                                } else {
                                    false
                                }
                            });
                        if !has_air_side {
                            continue;
                        }
                        let vine_len = 2 + (rng.random::<f64>().abs() * 3.0) as usize;
                        for dy in 1..=vine_len {
                            let vy = y.saturating_sub(dy);
                            if vy < 1 || vy >= CHUNK_HEIGHT {
                                break;
                            }
                            let below = chunk.get_block(x, vy, z);
                            if below.is_air() {
                                chunk.set_block(x, vy, z, Block::new(BlockId::Vine));
                            } else {
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }

        if center_biome == Biome::BambooJungle {
            for x in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (height, biome) = self.get_height(wx, wz);
                    if biome != Biome::BambooJungle
                        || height < Self::SEA_LEVEL
                        || height >= CHUNK_HEIGHT as i32 - 8
                    {
                        continue;
                    }
                    let surface_y = (height - 1).max(0) as usize;
                    if chunk.get_block(x, surface_y, z).id != BlockId::GrassBlock {
                        continue;
                    }
                    if !chunk.get_block(x, surface_y + 1, z).is_air() {
                        continue;
                    }
                    if rng.random_bool(0.15) {
                        let bamboo_height = 3 + (rng.random::<f64>() * 5.0) as usize;
                        for by in 1..=bamboo_height {
                            let pos = surface_y + by;
                            if pos >= CHUNK_HEIGHT {
                                break;
                            }
                            if !chunk.get_block(x, pos, z).is_air() {
                                break;
                            }
                            chunk.set_block(x, pos, z, Block::new(BlockId::Bamboo));
                        }
                    }
                }
            }
        }

        if center_biome == Biome::Taiga {
            for _ in 0..4 {
                let bx = rng.random_range(1..CHUNK_SIZE - 1);
                let bz = rng.random_range(1..CHUNK_SIZE - 1);
                let wx = (base_x + bx as i64) as f64;
                let wz = (base_z + bz as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let surface_y = (height - 1).max(0) as usize;
                if biome != Biome::Taiga
                    || height < Self::SEA_LEVEL
                    || height >= CHUNK_HEIGHT as i32 - 1
                {
                    continue;
                }
                if chunk.get_block(bx, surface_y, bz).id != BlockId::GrassBlock {
                    continue;
                }
                if !chunk.get_block(bx, surface_y + 1, bz).is_air() {
                    continue;
                }
                if rng.random_bool(0.08) {
                    chunk.set_block(bx, surface_y + 1, bz, Block::new(BlockId::SweetBerryBush));
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                if biome != Biome::Desert
                    || height < Self::SEA_LEVEL
                    || height >= CHUNK_HEIGHT as i32 - 3
                {
                    continue;
                }
                let surface_y = (height - 1).max(0) as usize;
                if chunk.get_block(x, surface_y, z).id != BlockId::Sand {
                    continue;
                }
                if !chunk.get_block(x, surface_y + 1, z).is_air() {
                    continue;
                }
                if !rng.random_bool(0.02) {
                    continue;
                }

                let cactus_h = 1 + (rng.random::<f64>().abs() * 2.0) as usize;
                for dy in 1..=cactus_h {
                    let cy = surface_y + dy;
                    if cy >= CHUNK_HEIGHT {
                        break;
                    }
                    let mut blocked = false;
                    for (dx, dz) in &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                            if !chunk.get_block(nx as usize, cy, nz as usize).is_air() {
                                blocked = true;
                                break;
                            }
                        }
                    }
                    if !blocked {
                        chunk.set_block(x, cy, z, Block::new(BlockId::Cactus));
                    }
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let warm = matches!(biome, Biome::Desert | Biome::Savanna | Biome::Jungle | Biome::Plains);
                if !warm || height < Self::SEA_LEVEL || height >= CHUNK_HEIGHT as i32 - 3 {
                    continue;
                }
                let surface_y = (height - 1).max(0) as usize;
                let top = chunk.get_block(x, surface_y, z);
                if top.id != BlockId::Sand && top.id != BlockId::GrassBlock {
                    continue;
                }
                if !chunk.get_block(x, surface_y + 1, z).is_air() {
                    continue;
                }

                let has_water = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                    .iter()
                    .any(|(dx, dz)| {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                            chunk.get_block(nx as usize, surface_y, nz as usize).id == BlockId::Water
                        } else {
                            false
                        }
                    });
                if !has_water || !rng.random_bool(0.08) {
                    continue;
                }

                let cane_h = 1 + (rng.random::<f64>().abs() * 2.0) as usize;
                for dy in 1..=cane_h {
                    let cy = surface_y + dy;
                    if cy >= CHUNK_HEIGHT {
                        break;
                    }
                    if !chunk.get_block(x, cy, z).is_air() {
                        break;
                    }
                    chunk.set_block(x, cy, z, Block::new(BlockId::SugarCane));
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (mh, mb) = self.get_height(wx, wz);
                if !matches!(mb, Biome::Swamp | Biome::DarkForest)
                    || mh < Self::SEA_LEVEL
                    || mh >= CHUNK_HEIGHT as i32 - 1
                {
                    continue;
                }
                let top = chunk.get_block(x, (mh - 1).max(0) as usize, z);
                let is_dark_forest = mb == Biome::DarkForest;
                let surface_allowed = if is_dark_forest {
                    top.id == BlockId::Podzol || top.id == BlockId::GrassBlock || top.id == BlockId::Dirt
                } else {
                    top.id == BlockId::GrassBlock
                };
                if !surface_allowed {
                    continue;
                }
                if !chunk.get_block(x, mh as usize, z).is_air() {
                    continue;
                }
                let mush_chance = if is_dark_forest { 0.15 } else { 0.04 };
                let giant_chance = if is_dark_forest { 0.10 } else { 0.02 };
                if rng.random_bool(mush_chance) {
                    let mush = if rng.random_bool(0.6) {
                        BlockId::BrownMushroom
                    } else {
                        BlockId::RedMushroom
                    };
                    chunk.set_block(x, mh as usize, z, Block::new(mush));
                } else if rng.random_bool(giant_chance) {
                    self.place_giant_mushroom(chunk, x, z, mh, rng.random_bool(0.5), rng);
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                if biome != Biome::Desert
                    || height < Self::SEA_LEVEL
                    || height >= CHUNK_HEIGHT as i32 - 1
                {
                    continue;
                }
                let surface_y = (height - 1).max(0) as usize;
                let top = chunk.get_block(x, surface_y, z);
                if top.id != BlockId::Sand {
                    continue;
                }
                if !chunk.get_block(x, surface_y + 1, z).is_air() {
                    continue;
                }
                if rng.random_bool(0.03) {
                    chunk.set_block(x, surface_y + 1, z, Block::new(BlockId::DeadBush));
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (_height, biome) = self.get_height(wx, wz);
                if biome != Biome::Swamp {
                    continue;
                }
                for y in (1..CHUNK_HEIGHT - 1).rev() {
                    let block = chunk.get_block(x, y, z);
                    if block.id == BlockId::Water && chunk.get_block(x, y + 1, z).is_air() {
                        if rng.random_bool(0.1) {
                            chunk.set_block(x, y + 1, z, Block::new(BlockId::LilyPad));
                        }
                        break;
                    }
                }
            }
        }

        for _ in 0..3 {
            let tx = rng.random_range(1..CHUNK_SIZE - 1);
            let tz = rng.random_range(1..CHUNK_SIZE - 1);
            let wx = (base_x + tx as i64) as f64;
            let wz = (base_z + tz as i64) as f64;
            let (height, biome) = self.get_height(wx, wz);
            let surface_y = (height - 1).max(0) as usize;
            if biome != Biome::Plains
                || height < Self::SEA_LEVEL
                || height >= CHUNK_HEIGHT as i32 - 1
            {
                continue;
            }
            if chunk.get_block(tx, surface_y, tz).id != BlockId::GrassBlock {
                continue;
            }
            if !chunk.get_block(tx, surface_y + 1, tz).is_air() {
                continue;
            }
            let clear = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                .iter()
                .all(|(dx, dz)| {
                    let nx = tx as i32 + dx;
                    let nz = tz as i32 + dz;
                    nx >= 0 && nx < CHUNK_SIZE as i32
                        && nz >= 0 && nz < CHUNK_SIZE as i32
                        && chunk.get_block(nx as usize, surface_y + 1, nz as usize).is_air()
                });
            if clear && rng.random_bool(0.5) {
                chunk.set_block(tx, surface_y + 1, tz, Block::new(BlockId::Pumpkin));
            }
        }

        for _ in 0..3 {
            let tx = rng.random_range(1..CHUNK_SIZE - 1);
            let tz = rng.random_range(1..CHUNK_SIZE - 1);
            let wx = (base_x + tx as i64) as f64;
            let wz = (base_z + tz as i64) as f64;
            let (height, biome) = self.get_height(wx, wz);
            let surface_y = (height - 1).max(0) as usize;
            if biome != Biome::Jungle
                || height < Self::SEA_LEVEL
                || height >= CHUNK_HEIGHT as i32 - 1
            {
                continue;
            }
            if chunk.get_block(tx, surface_y, tz).id != BlockId::GrassBlock {
                continue;
            }
            if !chunk.get_block(tx, surface_y + 1, tz).is_air() {
                continue;
            }
            let clear = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                .iter()
                .all(|(dx, dz)| {
                    let nx = tx as i32 + dx;
                    let nz = tz as i32 + dz;
                    nx >= 0 && nx < CHUNK_SIZE as i32
                        && nz >= 0 && nz < CHUNK_SIZE as i32
                        && chunk.get_block(nx as usize, surface_y + 1, nz as usize).is_air()
                });
            if clear && rng.random_bool(0.5) {
                chunk.set_block(tx, surface_y + 1, tz, Block::new(BlockId::Melon));
            }
        }

        if matches!(center_biome, Biome::Plains | Biome::Forest | Biome::Savanna) && rng.random_bool(0.005) {
            let lx = rng.random_range(2..CHUNK_SIZE - 2);
            let lz = rng.random_range(2..CHUNK_SIZE - 2);
            let wx = (base_x + lx as i64) as f64;
            let wz = (base_z + lz as i64) as f64;
            let (lh, _lb) = self.get_height(wx, wz);
            if lh > Self::SEA_LEVEL + 1 && lh < CHUNK_HEIGHT as i32 - 2 {
                let gy = (lh - 1).max(0) as usize;
                let pool_radius = 1 + (rng.random::<f64>() * 1.5) as usize;
                for dx in -(pool_radius as i32)..=pool_radius as i32 {
                    for dz in -(pool_radius as i32)..=pool_radius as i32 {
                        let bx = lx as i32 + dx;
                        let bz = lz as i32 + dz;
                        if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                            continue;
                        }
                        if dx.abs() == pool_radius as i32 && dz.abs() == pool_radius as i32 && rng.random_bool(0.5) {
                            continue;
                        }
                        let dist = (dx as f64).powi(2) + (dz as f64).powi(2);
                        if dist > (pool_radius as f64 + 0.5).powi(2) {
                            continue;
                        }
                        if chunk.get_block(bx as usize, gy, bz as usize).id == BlockId::GrassBlock {
                            chunk.set_block(bx as usize, gy, bz as usize, Block::new(BlockId::Stone));
                            let is_lava = dx.abs() <= pool_radius as i32 - 1 && dz.abs() <= pool_radius as i32 - 1;
                            if is_lava {
                                chunk.set_block(bx as usize, gy + 1, bz as usize, Block::new(BlockId::Lava));
                            }
                        }
                    }
                }
            }
        }
    }

    fn place_structures(&self, chunk: &mut Chunk, rng: &mut StdRng, base_x: i64, base_z: i64, center_biome: Biome) {
        if center_biome == Biome::Desert && rng.random_bool(0.003) {
            'outer: for x in 2..CHUNK_SIZE - 3 {
                for z in 2..CHUNK_SIZE - 3 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (wh, _wb) = self.get_height(wx, wz);
                    if wh > Self::SEA_LEVEL && wh < CHUNK_HEIGHT as i32 - 3 {
                        self.place_desert_well(chunk, x, z, wh, rng);
                        break 'outer;
                    }
                }
            }
        }

        if matches!(center_biome, Biome::SnowyTundra | Biome::Taiga) && rng.random_bool(0.001) {
            'outer: for x in 3..CHUNK_SIZE - 4 {
                for z in 3..CHUNK_SIZE - 4 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (ih, _ib) = self.get_height(wx, wz);
                    if ih > Self::SEA_LEVEL + 2 && ih < CHUNK_HEIGHT as i32 - 5 {
                        if chunk.get_block(x, (ih - 1).max(0) as usize, z).id == BlockId::SnowBlock
                            || chunk.get_block(x, (ih - 1).max(0) as usize, z).id == BlockId::GrassBlock
                        {
                            self.place_igloo(chunk, x, z, ih, rng);
                            break 'outer;
                        }
                    }
                }
            }
        }

        if center_biome == Biome::Swamp && rng.random_bool(0.002) {
            'outer: for x in 3..CHUNK_SIZE - 4 {
                for z in 3..CHUNK_SIZE - 4 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (sh, _sb) = self.get_height(wx, wz);
                    if sh >= Self::SEA_LEVEL && sh < CHUNK_HEIGHT as i32 - 4 {
                        self.place_swamp_hut(chunk, x, z, sh, rng);
                        break 'outer;
                    }
                }
            }
        }

        if rng.random_bool(0.001) {
            'outer: for x in 2..CHUNK_SIZE - 3 {
                for z in 2..CHUNK_SIZE - 3 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (rh, _rb) = self.get_height(wx, wz);
                    if (rh >= Self::SEA_LEVEL - 3 && rh <= Self::SEA_LEVEL + 2) && rh > 1 {
                        self.place_ocean_ruin(chunk, x, z, rh, rng);
                        break 'outer;
                    }
                }
            }
        }

        if rng.random_bool(0.005) {
            self.place_dungeon(chunk, rng);
        }

        if rng.random_bool(0.002) {
            self.place_ruined_portal(chunk, rng);
        }
    }

    fn generate_ocean_flora(&self, chunk: &mut Chunk, rng: &mut StdRng, base_x: i64, base_z: i64) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let is_ocean = matches!(biome,
                    Biome::Ocean | Biome::DeepOcean | Biome::WarmOcean | Biome::LukewarmOcean
                    | Biome::ColdOcean | Biome::FrozenOcean | Biome::DeepWarmOcean
                    | Biome::DeepLukewarmOcean | Biome::DeepColdOcean | Biome::DeepFrozenOcean);
                if !is_ocean || height >= Self::SEA_LEVEL {
                    continue;
                }
                let mut sea_floor = 0;
                for y in (1..CHUNK_HEIGHT - 1).rev() {
                    if chunk.get_block(x, y, z).id != BlockId::Water {
                        sea_floor = y;
                        break;
                    }
                }
                if sea_floor < 1 || sea_floor >= CHUNK_HEIGHT - 2 {
                    continue;
                }
                let above = chunk.get_block(x, sea_floor + 1, z);
                if above.id != BlockId::Water {
                    continue;
                }
                let water_depth = Self::SEA_LEVEL - sea_floor as i32;
                if water_depth > 2 && water_depth < 20 && rng.random_bool(0.3) {
                    if rng.random_bool(0.3) {
                        let kelp_height = 2 + (rng.random::<f64>() * (water_depth.min(10) as f64 * 0.5)) as usize;
                        for ky in 1..=kelp_height {
                            let by = sea_floor + ky;
                            if by >= CHUNK_HEIGHT {
                                break;
                            }
                            if chunk.get_block(x, by, z).id != BlockId::Water {
                                break;
                            }
                            let kelp_id = if ky == kelp_height { BlockId::Kelp } else { BlockId::KelpPlant };
                            chunk.set_block(x, by, z, Block::new(kelp_id));
                        }
                    } else {
                        chunk.set_block(x, sea_floor + 1, z, Block::new(BlockId::Seagrass));
                    }
                } else if water_depth > 1 && rng.random_bool(0.15) {
                    let tall = rng.random_bool(0.3);
                    if tall && sea_floor + 2 < CHUNK_HEIGHT {
                        let above2 = chunk.get_block(x, sea_floor + 2, z);
                        if above2.id == BlockId::Water {
                            chunk.set_block(x, sea_floor + 1, z, Block::new(BlockId::TallSeagrass));
                            chunk.set_block(x, sea_floor + 2, z, Block::new(BlockId::TallSeagrass));
                        }
                    } else {
                        chunk.set_block(x, sea_floor + 1, z, Block::new(BlockId::Seagrass));
                    }
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let is_warm_ocean = matches!(biome, Biome::WarmOcean | Biome::DeepWarmOcean);
                if !is_warm_ocean || height >= Self::SEA_LEVEL - 2 {
                    continue;
                }
                let mut sea_floor = 0;
                for y in (1..CHUNK_HEIGHT - 1).rev() {
                    if chunk.get_block(x, y, z).id != BlockId::Water {
                        sea_floor = y;
                        break;
                    }
                }
                if sea_floor < 1 || sea_floor >= CHUNK_HEIGHT - 2 {
                    continue;
                }
                let water_depth = Self::SEA_LEVEL - sea_floor as i32;
                if water_depth < 3 || water_depth > 12 {
                    continue;
                }
                if rng.random_bool(0.05) {
                    let coral_types = [
                        BlockId::TubeCoral, BlockId::BrainCoral, BlockId::BubbleCoral,
                        BlockId::FireCoral, BlockId::HornCoral,
                    ];
                    let coral_block_types = [
                        BlockId::TubeCoralBlock, BlockId::BrainCoralBlock, BlockId::BubbleCoralBlock,
                        BlockId::FireCoralBlock, BlockId::HornCoralBlock,
                    ];
                    let coral_idx = rng.random_range(0..5);
                    chunk.set_block(x, sea_floor, z, Block::new(coral_block_types[coral_idx]));
                    let above = chunk.get_block(x, sea_floor + 1, z);
                    if above.id == BlockId::Water {
                        let fan_idx = rng.random_range(0..5);
                        if rng.random_bool(0.4) {
                            chunk.set_block(x, sea_floor + 1, z, Block::new(coral_types[fan_idx]));
                        } else {
                            let coral_fans = [
                                BlockId::TubeCoralFan, BlockId::BrainCoralFan, BlockId::BubbleCoralFan,
                                BlockId::FireCoralFan, BlockId::HornCoralFan,
                            ];
                            chunk.set_block(x, sea_floor + 1, z, Block::new(coral_fans[fan_idx]));
                        }
                    }
                }
            }
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let is_warm = matches!(biome, Biome::WarmOcean | Biome::DeepWarmOcean);
                if !is_warm || height >= Self::SEA_LEVEL - 1 {
                    continue;
                }
                let mut sea_floor = 0;
                for y in (1..CHUNK_HEIGHT - 1).rev() {
                    if chunk.get_block(x, y, z).id != BlockId::Water {
                        sea_floor = y;
                        break;
                    }
                }
                if sea_floor < 1 || sea_floor >= CHUNK_HEIGHT - 2 {
                    continue;
                }
                if sea_floor < Self::SEA_LEVEL as usize - 6 {
                    continue;
                }
                let above = chunk.get_block(x, sea_floor + 1, z);
                if above.id != BlockId::Water {
                    continue;
                }
                if rng.random_bool(0.04) {
                    chunk.set_block(x, sea_floor + 1, z, Block::new(BlockId::SeaPickle));
                }
            }
        }
    }

    fn place_tree(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        log: BlockId,
        leaves: BlockId,
        rng: &mut impl Rng,
    ) {
        let trunk_height = 5 + (rng.random::<f64>().abs() * 2.0) as i32;
        // Trunk (one shorter than leaves so top is capped)
        for ty in 1..=trunk_height.saturating_sub(1) {
            let by = height + ty;
            if by < CHUNK_HEIGHT as i32 {
                chunk.set_block(tx, by as usize, tz, Block::new(log));
            }
        }
        // Leaves: 3 layers of ball canopy
        let leaf_start = height + trunk_height - 2;
        for dy in 0..=2 {
            let radius = if dy == 0 {
                2
            } else if dy == 1 {
                2
            } else {
                1
            };
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let lx = tx as i32 + dx;
                    let lz = tz as i32 + dz;
                    let ly = leaf_start + dy;
                    if lx >= 0
                        && lx < CHUNK_SIZE as i32
                        && lz >= 0
                        && lz < CHUNK_SIZE as i32
                        && ly > 0
                        && ly < CHUNK_HEIGHT as i32
                    {
                        if dx.abs() == radius && dz.abs() == radius && dy > 0 {
                            continue;
                        }
                        if chunk
                            .get_block(lx as usize, ly as usize, lz as usize)
                            .is_air()
                        {
                            chunk.set_block(
                                lx as usize,
                                ly as usize,
                                lz as usize,
                                Block::new(leaves),
                            );
                        }
                    }
                }
            }
        }
    }

    fn place_spruce_tree(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        rng: &mut impl Rng,
    ) {
        let trunk_height = 6 + (rng.random::<f64>().abs() * 4.0) as i32;
        // Trunk one shorter than leaves so top is capped
        for ty in 1..=trunk_height.saturating_sub(1) {
            let by = height + ty;
            if by < CHUNK_HEIGHT as i32 {
                chunk.set_block(tx, by as usize, tz, Block::new(BlockId::SpruceLog));
            }
        }
        // Conical layers: widest at bottom, narrow at top (layer 3 = top, capped over trunk)
        for layer in 0..4 {
            let ly = height + trunk_height - 3 + layer;
            let radius = (3 - layer).max(1);
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let lx = tx as i32 + dx;
                    let lz = tz as i32 + dz;
                    if lx >= 0
                        && lx < CHUNK_SIZE as i32
                        && lz >= 0
                        && lz < CHUNK_SIZE as i32
                        && ly > 0
                        && ly < CHUNK_HEIGHT as i32
                    {
                        if dx.abs() == radius && dz.abs() == radius {
                            continue;
                        }
                        if chunk
                            .get_block(lx as usize, ly as usize, lz as usize)
                            .is_air()
                        {
                            chunk.set_block(
                                lx as usize,
                                ly as usize,
                                lz as usize,
                                Block::new(BlockId::SpruceLeaves),
                            );
                        }
                    }
                }
            }
        }
    }

    fn place_birch_tree(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        rng: &mut impl Rng,
    ) {
        let trunk_height = 5 + (rng.random::<f64>().abs() * 2.0) as i32;
        for ty in 1..trunk_height {
            let by = height + ty;
            if by < CHUNK_HEIGHT as i32 {
                chunk.set_block(tx, by as usize, tz, Block::new(BlockId::BirchLog));
            }
        }

        let leaf_start = height + trunk_height - 2;
        for dx in -2..=2 {
            for dz in -2..=2 {
                for dy in -1..=2 {
                    let lx = tx as i32 + dx;
                    let lz = tz as i32 + dz;
                    let ly = leaf_start + dy;
                    if lx >= 0
                        && lx < CHUNK_SIZE as i32
                        && lz >= 0
                        && lz < CHUNK_SIZE as i32
                        && ly > 0
                        && ly < CHUNK_HEIGHT as i32
                    {
                        let d = dx.abs().max(dz.abs());
                        let in_range = if dy <= 0 { d <= 2 } else { d <= 1 };
                        if in_range
                            && chunk
                                .get_block(lx as usize, ly as usize, lz as usize)
                                .is_air()
                        {
                            chunk.set_block(
                                lx as usize,
                                ly as usize,
                                lz as usize,
                                Block::new(BlockId::BirchLeaves),
                            );
                        }
                    }
                }
            }
        }
    }

    fn place_dungeon(&self, chunk: &mut Chunk, rng: &mut impl Rng) {
        let dx = rng.random_range(2..CHUNK_SIZE - 6);
        let dz = rng.random_range(2..CHUNK_SIZE - 6);
        let dy = 10 + (rng.random::<f64>() * 45.0) as usize;
        if dy >= CHUNK_HEIGHT - 5 {
            return;
        }

        // Check the area is mostly stone (not already carved into cave)
        let mut solid_count = 0;
        for x in dx..=dx + 6 {
            for z in dz..=dz + 6 {
                for y in dy..=dy + 5 {
                    let b = chunk.get_block(x, y, z);
                    if b.id == BlockId::Stone
                        || b.id == BlockId::Deepslate
                        || b.id == BlockId::Dirt
                        || b.id == BlockId::Gravel
                    {
                        solid_count += 1;
                    }
                }
            }
        }
        // Need at least 60% solid blocks to place dungeon here
        if solid_count < 180 {
            return;
        }

        // Carve out room: interior 5x4x5 (x,z,y)
        // Walls are cobblestone/mossy
        for x in dx..=dx + 6 {
            for z in dz..=dz + 6 {
                for y in dy..=dy + 5 {
                    let is_wall =
                        x == dx || x == dx + 6 || z == dz || z == dz + 6 || y == dy || y == dy + 5;
                    if is_wall {
                        let wall_block = if rng.random_bool(0.3) {
                            BlockId::MossyCobblestone
                        } else {
                            BlockId::Cobblestone
                        };
                        chunk.set_block(x, y, z, Block::new(wall_block));
                    } else {
                        chunk.set_block(x, y, z, Block::air());
                    }
                }
            }
        }

        // Spawner in center
        let cx = dx + 3;
        let cz = dz + 3;
        chunk.set_block(cx, dy + 1, cz, Block::new(BlockId::Spawner));

        // 1-2 chests against walls
        let num_chests = 1 + rng.random_range(0..2) as usize;
        for _ in 0..num_chests {
            let (cx2, cz2) = match rng.random_range(0..4) {
                0 => (dx + 1, dz + 1 + rng.random_range(0..4)),
                1 => (dx + 5, dz + 1 + rng.random_range(0..4)),
                2 => (dx + 1 + rng.random_range(0..4), dz + 1),
                _ => (dx + 1 + rng.random_range(0..4), dz + 5),
            };
            let existing = chunk.get_block(cx2, dy + 1, cz2);
            if existing.is_air() {
                chunk.set_block(cx2, dy + 1, cz2, Block::new(BlockId::Chest));
            }
        }
    }

    fn place_ruined_portal(&self, chunk: &mut Chunk, rng: &mut impl Rng) {
        let px = rng.random_range(2..CHUNK_SIZE - 6);
        let pz = rng.random_range(2..CHUNK_SIZE - 6);
        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;
        let wx = (base_x + px as i64) as f64;
        let wz = (base_z + pz as i64) as f64;
        let (height, _biome) = self.get_height(wx, wz);
        if height < Self::SEA_LEVEL - 2 || height >= CHUNK_HEIGHT as i32 - 7 {
            return;
        }
        // Don't place in water
        let ground_block = chunk.get_block(px, height as usize - 1, pz);
        if ground_block.id == BlockId::Water {
            return;
        }

        // Portal frame: 4 wide (x) x 5 tall (y), some blocks missing
        let frame_height = 5;
        let frame_width = 4;
        let gy = (height - 1).max(0) as usize;
        let mut missing = 2 + (rng.random::<f64>() * 4.0) as usize; // 2-5 broken blocks

        for y in 0..frame_height {
            for x in 0..frame_width {
                let bx = px + x;
                let by = gy + y;
                if bx >= CHUNK_SIZE || by >= CHUNK_HEIGHT {
                    continue;
                }
                let is_corner = (x == 0 || x == frame_width - 1) || y == 0 || y == frame_height - 1;
                let is_frame = x == 0 || x == frame_width - 1 || y == 0 || y == frame_height - 1;
                let is_top_corner = (x == 0 || x == frame_width - 1) && y == frame_height - 1;

                // Top corners don't exist in the frame (nether portal is 4W x 5H with open top middle)
                if is_top_corner {
                    continue;
                }

                if !is_frame {
                    continue;
                } // interior is open

                if missing > 0 && !is_corner {
                    missing -= 1;
                    continue; // missing block
                }

                let block = if y == 0 {
                    // Base: mix of stone bricks and obsidian
                    if rng.random_bool(0.3) {
                        BlockId::Obsidian
                    } else {
                        BlockId::StoneBricks
                    }
                } else {
                    // Add some crying obsidian in frame
                    if rng.random_bool(0.1) {
                        BlockId::CryingObsidian
                    } else {
                        BlockId::Obsidian
                    }
                };
                chunk.set_block(bx, by, pz, Block::new(block));
            }
        }

        // Stone bricks around the base
        for dx in -2i32..=5i32 {
            for dz in -2i32..=2i32 {
                let bx = px as i32 + dx;
                let bz = pz as i32 + dz;
                if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                    continue;
                }
                let by = gy;
                let existing = chunk.get_block(bx as usize, by, bz as usize);
                if existing.id == BlockId::Stone
                    || existing.id == BlockId::Dirt
                    || existing.id == BlockId::GrassBlock
                {
                    if dx < 0 || dx > 3 || dz < -1 || dz > 1 {
                        if rng.random_bool(0.3) {
                            chunk.set_block(
                                bx as usize,
                                by,
                                bz as usize,
                                Block::new(BlockId::StoneBricks),
                            );
                        }
                    }
                }
            }
        }
        // Some lichen/vines
        if rng.random_bool(0.4) {
            for _ in 0..3 {
                let bx = px + rng.random_range(0..frame_width);
                let by = gy + (rng.random::<f64>() * frame_height as f64) as usize;
                let bz = pz;
                if bx < CHUNK_SIZE && by < CHUNK_HEIGHT {
                    let existing = chunk.get_block(bx, by, bz);
                    if existing.id == BlockId::Obsidian || existing.id == BlockId::CryingObsidian {
                        for (nx, nz) in &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                            let ax = bx as i32 + nx;
                            let az = pz as i32 + nz;
                            if ax >= 0
                                && ax < CHUNK_SIZE as i32
                                && az >= 0
                                && az < CHUNK_SIZE as i32
                                && chunk.get_block(ax as usize, by, az as usize).is_air()
                            {
                                if rng.random_bool(0.5) {
                                    chunk.set_block(
                                        ax as usize,
                                        by,
                                        az as usize,
                                        Block::new(BlockId::Vine),
                                    );
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn place_giant_mushroom(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        is_red: bool,
        rng: &mut impl Rng,
    ) {
        let stem_height = 2 + (rng.random::<f64>().abs() * 2.0) as i32;
        let cap_id = if is_red {
            BlockId::RedMushroomBlock
        } else {
            BlockId::BrownMushroomBlock
        };

        // Stem
        for dy in 1..=stem_height {
            let by = height + dy;
            if by < CHUNK_HEIGHT as i32 {
                chunk.set_block(tx, by as usize, tz, Block::new(BlockId::MushroomStem));
            }
        }

        let cap_y = height + stem_height + 1;
        // Cap: 3 layers, wider at bottom, narrower at top
        for dx in -2i32..=2i32 {
            for dz in -2i32..=2i32 {
                let lx = tx as i32 + dx;
                let lz = tz as i32 + dz;
                for dy in 0..=2 {
                    let ly = cap_y + dy;
                    if lx < 0 || lx >= CHUNK_SIZE as i32 || lz < 0 || lz >= CHUNK_SIZE as i32 {
                        continue;
                    }
                    if ly < 1 || ly >= CHUNK_HEIGHT as i32 {
                        continue;
                    }
                    let d = dx.abs().max(dz.abs());
                    let in_range = match dy {
                        0 => d <= 2 && d > 0, // bottom ring (don't cover stem top)
                        1 => d <= 2,
                        2 => d <= 1,
                        _ => false,
                    };
                    if in_range
                        && chunk
                            .get_block(lx as usize, ly as usize, lz as usize)
                            .is_air()
                    {
                        chunk.set_block(lx as usize, ly as usize, lz as usize, Block::new(cap_id));
                    }
                }
            }
        }
        // Cap top center
        if cap_y + 2 < CHUNK_HEIGHT as i32 {
            let top = chunk.get_block(tx, (cap_y + 2) as usize, tz);
            if top.is_air() {
                chunk.set_block(tx, (cap_y + 2) as usize, tz, Block::new(cap_id));
            }
        }
    }

    fn place_jungle_tree(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        rng: &mut impl Rng,
    ) {
        let trunk_height = 7 + (rng.random::<f64>().abs() * 5.0) as i32;
        // Single-wide trunk, one shorter than leaves so top is capped
        for ty in 1..=trunk_height.saturating_sub(1) {
            let by = height + ty;
            if by < CHUNK_HEIGHT as i32 {
                chunk.set_block(tx, by as usize, tz, Block::new(BlockId::JungleLog));
            }
        }

        let leaf_start = height + trunk_height - 3;
        for dx in -3..=3 {
            for dz in -3..=3 {
                for dy in -1..=3 {
                    let lx = tx as i32 + dx;
                    let lz = tz as i32 + dz;
                    let ly = leaf_start + dy;
                    if lx >= 0
                        && lx < CHUNK_SIZE as i32
                        && lz >= 0
                        && lz < CHUNK_SIZE as i32
                        && ly > 0
                        && ly < CHUNK_HEIGHT as i32
                    {
                        let d = dx.abs().max(dz.abs());
                        let in_range = if dy <= 0 { d <= 3 } else { d <= 2 };
                        if in_range
                            && chunk
                                .get_block(lx as usize, ly as usize, lz as usize)
                                .is_air()
                        {
                            chunk.set_block(
                                lx as usize,
                                ly as usize,
                                lz as usize,
                                Block::new(BlockId::JungleLeaves),
                            );
                        }
                    }
                }
            }
        }
    }

    fn place_dark_oak_tree(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        rng: &mut impl Rng,
    ) {
        // Dark oak: 2x2 trunk, large rounded canopy
        let trunk_height = 6 + (rng.random::<f64>().abs() * 2.0) as i32;
        // 2x2 trunk
        for ty in 1..=trunk_height {
            for dx in 0..2 {
                for dz in 0..2 {
                    let bx = tx as i32 + dx;
                    let bz = tz as i32 + dz;
                    let by = height + ty;
                    if bx >= 0
                        && bx < CHUNK_SIZE as i32
                        && bz >= 0
                        && bz < CHUNK_SIZE as i32
                        && by < CHUNK_HEIGHT as i32
                    {
                        chunk.set_block(
                            bx as usize,
                            by as usize,
                            bz as usize,
                            Block::new(BlockId::DarkOakLog),
                        );
                    }
                }
            }
        }
        // Large rounded canopy: 4 layers, radius 4 at bottom tapering to 1 at top
        let leaf_start = height + trunk_height - 3;
        for dy in 0..=4 {
            let radius = match dy {
                0 => 4,
                1 => 4,
                2 => 3,
                3 => 2,
                _ => 1,
            };
            for dx in -(radius as i32)..=radius as i32 {
                for dz in -(radius as i32)..=radius as i32 {
                    let lx = (tx as i32 + dx).max(0).min(CHUNK_SIZE as i32 - 1);
                    let lz = (tz as i32 + dz).max(0).min(CHUNK_SIZE as i32 - 1);
                    let ly = leaf_start + dy;
                    if ly > 0 && ly < CHUNK_HEIGHT as i32 {
                        let d = dx.abs().max(dz.abs());
                        if d > radius {
                            continue;
                        }
                        if d == radius && dy > 0 && dy < 4 && rng.random_bool(0.5) {
                            continue;
                        }
                        if chunk
                            .get_block(lx as usize, ly as usize, lz as usize)
                            .is_air()
                        {
                            chunk.set_block(
                                lx as usize,
                                ly as usize,
                                lz as usize,
                                Block::new(BlockId::DarkOakLeaves),
                            );
                        }
                    }
                }
            }
        }
    }

    fn place_desert_well(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        _rng: &mut impl Rng,
    ) {
        let gy = (height - 1).max(0) as usize;
        if chunk.get_block(tx, gy, tz).id != BlockId::Sand {
            return;
        }
        // 2x2 water pool at ground level
        for dx in 0..2 {
            for dz in 0..2 {
                let bx = tx + dx;
                let bz = tz + dz;
                if bx < CHUNK_SIZE && bz < CHUNK_SIZE {
                    chunk.set_block(bx, gy + 1, bz, Block::new(BlockId::Water));
                    // Stone brick rim
                    for (rx, rz) in &[
                        (-1i32, -1i32),
                        (-1, 0),
                        (-1, 1),
                        (-1, 2),
                        (2, -1),
                        (2, 0),
                        (2, 1),
                        (2, 2),
                        (0, -1),
                        (1, -1),
                        (0, 2),
                        (1, 2),
                    ] {
                        let wx = tx as i32 + rx;
                        let wz = tz as i32 + rz;
                        if wx >= 0 && wx < CHUNK_SIZE as i32 && wz >= 0 && wz < CHUNK_SIZE as i32 {
                            let above = chunk.get_block(wx as usize, gy + 1, wz as usize);
                            if above.is_air() || above.id == BlockId::Sand {
                                chunk.set_block(
                                    wx as usize,
                                    gy + 1,
                                    wz as usize,
                                    Block::new(BlockId::StoneBricks),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn place_igloo(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        _rng: &mut impl Rng,
    ) {
        let gy = (height - 1).max(0) as usize;
        if chunk.get_block(tx, gy, tz).id != BlockId::GrassBlock
            && chunk.get_block(tx, gy, tz).id != BlockId::SnowBlock
        {
            return;
        }
        // Snow floor
        let floor_y = gy + 1;
        // Small snow dome: 5x5 base, 3x3 middle, 1x1 top
        for dy in 0..3 {
            let r = match dy {
                0 => 2,
                1 => 1,
                _ => 0,
            };
            for dx in -(r as i32)..=r as i32 {
                for dz in -(r as i32)..=r as i32 {
                    let bx = tx as i32 + dx;
                    let bz = tz as i32 + dz;
                    let by = floor_y + dy;
                    if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                        continue;
                    }
                    if by < CHUNK_HEIGHT {
                        // Interior is air, walls are snow
                        if (dx.abs() == r as i32 || dz.abs() == r as i32 || dy == 2) && r > 0 {
                            chunk.set_block(
                                bx as usize,
                                by,
                                bz as usize,
                                Block::new(BlockId::SnowBlock),
                            );
                        } else if dy == 0 {
                            // Floor
                            if dx == 0 && dz == 0 {
                                chunk.set_block(
                                    bx as usize,
                                    by,
                                    bz as usize,
                                    Block::new(BlockId::RedCarpet),
                                );
                            } else {
                                chunk.set_block(
                                    bx as usize,
                                    by,
                                    bz as usize,
                                    Block::new(BlockId::WhiteCarpet),
                                );
                            }
                        } else {
                            chunk.set_block(bx as usize, by, bz as usize, Block::air());
                        }
                    }
                }
            }
        }
        // Furnace and red wool (bed placeholder) inside
        chunk.set_block(tx, floor_y + 1, tz, Block::new(BlockId::Furnace));
        if tx + 1 < CHUNK_SIZE {
            chunk.set_block(tx + 1, floor_y, tz, Block::new(BlockId::RedWool));
        }
    }

    fn place_swamp_hut(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        _rng: &mut impl Rng,
    ) {
        let gy = (height - 1).max(0) as usize;
        // Stilts (oak plank posts) if over water
        for dx in 0..3 {
            for dz in 0..3 {
                let sx = tx + dx;
                let sz = tz + dz;
                if sx >= CHUNK_SIZE || sz >= CHUNK_SIZE {
                    continue;
                }
                // Check if over water - add stilt posts
                for by in gy..=height as usize {
                    let block = chunk.get_block(sx, by, sz);
                    if block.id == BlockId::Water || block.id == BlockId::LilyPad {
                        chunk.set_block(sx, by, sz, Block::new(BlockId::OakPlanks));
                    }
                }
            }
        }
        let floor_y = (height as usize).max(gy + 2);
        // 3x3 oak plank floor
        for dx in 0..3 {
            for dz in 0..3 {
                let sx = tx + dx;
                let sz = tz + dz;
                if sx >= CHUNK_SIZE || sz >= CHUNK_SIZE {
                    continue;
                }
                chunk.set_block(sx, floor_y, sz, Block::new(BlockId::OakPlanks));
            }
        }
        // Walls: 2 blocks high
        for dy in 1..=2 {
            for dx in 0..3 {
                for dz in 0..3 {
                    let sx = tx + dx;
                    let sz = tz + dz;
                    if sx >= CHUNK_SIZE || sz >= CHUNK_SIZE {
                        continue;
                    }
                    let is_wall = dx == 0 || dx == 2 || dz == 0 || dz == 2;
                    let is_door = (dx == 1 && dz == 0) && dy == 1;
                    if is_wall && !is_door {
                        chunk.set_block(sx, floor_y + dy, sz, Block::new(BlockId::OakPlanks));
                    }
                }
            }
        }
        // Roof: upward slope (spruce slabs)
        for dx in -1..=3 {
            for dz in -1..=3 {
                let sx = tx as i32 + dx;
                let sz = tz as i32 + dz;
                if sx < 0 || sx >= CHUNK_SIZE as i32 || sz < 0 || sz >= CHUNK_SIZE as i32 {
                    continue;
                }
                let d = dx.abs().max(dz.abs());
                if d <= 2 {
                    chunk.set_block(
                        sx as usize,
                        floor_y + 3,
                        sz as usize,
                        Block::new(BlockId::OakPlanks),
                    );
                }
            }
        }
        // Flower pot with mushroom inside
        if tx + 1 < CHUNK_SIZE && tz + 1 < CHUNK_SIZE && floor_y + 2 < CHUNK_HEIGHT {
            chunk.set_block(
                tx + 1,
                floor_y + 1,
                tz + 1,
                Block::new(BlockId::BrownMushroom),
            );
        }
    }

    fn place_ocean_ruin(
        &self,
        chunk: &mut Chunk,
        tx: usize,
        tz: usize,
        height: i32,
        rng: &mut impl Rng,
    ) {
        let gy = (height - 1).max(0) as usize;
        let is_underwater = height < Self::SEA_LEVEL;
        // Small ruin: 3x3 platform of stone bricks, with partial walls
        for dx in -1..=1 {
            for dz in -1..=1 {
                let bx = tx as i32 + dx;
                let bz = tz as i32 + dz;
                if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 {
                    continue;
                }
                // Floor
                let existing = chunk.get_block(bx as usize, gy, bz as usize);
                if is_underwater
                    || existing.id == BlockId::Sand
                    || existing.id == BlockId::Gravel
                    || existing.id == BlockId::Dirt
                {
                    chunk.set_block(
                        bx as usize,
                        gy,
                        bz as usize,
                        Block::new(BlockId::StoneBricks),
                    );
                }
                // Partial corner walls (1 block high)
                if dx.abs() == 1 && dz.abs() == 1 && rng.random_bool(0.6) {
                    if gy + 1 < CHUNK_HEIGHT {
                        let above = chunk.get_block(bx as usize, gy + 1, bz as usize);
                        if above.is_air() || above.id == BlockId::Water {
                            chunk.set_block(
                                bx as usize,
                                gy + 1,
                                bz as usize,
                                Block::new(BlockId::StoneBricks),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_for_a_seed_and_chunk() {
        let mut first = Chunk::new(-3, 5);
        let mut second = Chunk::new(-3, 5);
        WorldGenerator::new(0x5EED).generate_chunk(&mut first);
        WorldGenerator::new(0x5EED).generate_chunk(&mut second);
        assert_eq!(first.blocks, second.blocks);
        assert_eq!(first.water_count, second.water_count);
        assert_eq!(first.lava_count, second.lava_count);
    }

    #[test]
    fn generation_is_independent_of_reused_generator_order() {
        let seed = 0x5EED;
        let mut expected = Chunk::new(7, -11);
        WorldGenerator::new(seed).generate_chunk(&mut expected);

        let mut generator = WorldGenerator::new(seed);
        let mut first = Chunk::new(7, -11);
        generator.generate_chunk(&mut first);
        let mut unrelated = Chunk::new(-24, 19);
        generator.generate_chunk(&mut unrelated);
        let mut repeated = Chunk::new(7, -11);
        generator.generate_chunk(&mut repeated);

        assert_eq!(first.blocks, expected.blocks);
        assert_eq!(repeated.blocks, expected.blocks);
        assert_eq!(repeated.water_count, expected.water_count);
        assert_eq!(repeated.lava_count, expected.lava_count);
    }

    #[test]
    fn surface_blending_is_stable_after_other_chunk_generation() {
        let mut generator = WorldGenerator::new(0x5EED);
        let before = generator.surface_blocks_for(-17.0, 31.0, 64);
        let mut unrelated = Chunk::new(3, -2);
        generator.generate_chunk(&mut unrelated);
        let after = generator.surface_blocks_for(-17.0, 31.0, 64);
        assert_eq!(before, after);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Biome {
    Plains,
    Forest,
    BirchForest,
    PaleGarden,
    Desert,
    Savanna,
    SavannaPlateau,
    Taiga,
    SnowyTaiga,
    SnowyTundra,
    IceSpikes,
    Mountains,
    Swamp,
    MangroveSwamp,
    Jungle,
    SparseJungle,
    DarkForest,
    Ocean,
    DeepOcean,
    WarmOcean,
    LukewarmOcean,
    ColdOcean,
    FrozenOcean,
    StonyShore,
    SnowyBeach,
    DeepWarmOcean,
    DeepLukewarmOcean,
    DeepColdOcean,
    DeepFrozenOcean,
    Beach,
    MushroomFields,
    BambooJungle,
    Badlands,
    WoodedBadlands,
    ErodedBadlands,
    River,
    FrozenRiver,
    FlowerForest,
    SunflowerPlains,
    CherryGrove,
    Meadow,
    Grove,
    SnowySlopes,
    JaggedPeaks,
    FrozenPeaks,
    StonyPeaks,
    OldGrowthPineTaiga,
    OldGrowthSpruceTaiga,
    WindsweptHills,
    WindsweptForest,
    WindsweptSavanna,
    WindsweptGravellyHills,
    DripstoneCaves,
    LushCaves,
    SulfurCaves,
    DeepDark,
}
