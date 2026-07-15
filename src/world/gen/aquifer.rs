#![allow(dead_code)]

use crate::world::block::BlockId;
use crate::world::gen::noise::NormalNoise;

pub const AQUIFER_X_RANGE: i32 = 10;
pub const AQUIFER_Z_RANGE: i32 = 10;
pub const AQUIFER_Y_RANGE: i32 = 9;
pub const AQUIFER_CELL_SPACING_X: i32 = 16;
pub const AQUIFER_CELL_SPACING_Y: i32 = 12;
pub const AQUIFER_CELL_SPACING_Z: i32 = 16;

pub struct Aquifer {
    pub fluid_level_floodedness_noise: NormalNoise,
    pub fluid_level_spread_noise: NormalNoise,
    pub barrier_noise: NormalNoise,
    pub lava_noise: NormalNoise,
    pub sea_level: i32,
    pub default_fluid: BlockId,
}

pub enum AquiferResult {
    Solid,
    Air,
    Water,
    Lava,
}

impl Aquifer {
    pub fn new(seed: u64, sea_level: i32) -> Self {
        let s = seed as i64;
        Aquifer {
            fluid_level_floodedness_noise: NormalNoise::new(s ^ 0xABCD, -15, &[1.0]),
            fluid_level_spread_noise: NormalNoise::new(s ^ 0xBCDE, -15, &[1.0]),
            barrier_noise: NormalNoise::new(s ^ 0xCDEF, -15, &[1.0]),
            lava_noise: NormalNoise::new(s ^ 0xDEF0, -15, &[1.0]),
            sea_level,
            default_fluid: BlockId::Water,
        }
    }

    pub fn compute_substance(&self, x: f64, y: f64, z: f64, density: f64) -> AquiferResult {
        if density > 0.0 {
            return AquiferResult::Solid;
        }

        if y > self.sea_level as f64 + 2.0 {
            return AquiferResult::Air;
        }

        let _flooded = self
            .fluid_level_floodedness_noise
            .get_value(x * 0.01, y * 0.01, z * 0.01);
        let _spread = self
            .fluid_level_spread_noise
            .get_value(x * 0.01, y * 0.01, z * 0.01);
        let barrier = self
            .barrier_noise
            .get_value(x * 0.02, y * 0.02, z * 0.02);

        let barrier_pressure = 2.0 * (barrier + 0.1);

        if density + barrier_pressure > 0.0 {
            return AquiferResult::Solid;
        }

        if y < -10.0 {
            let lava = self
                .lava_noise
                .get_value(x * 0.01, y * 0.01, z * 0.01);
            if lava > 0.0 {
                return AquiferResult::Lava;
            }
        }

        if y < self.sea_level as f64 {
            AquiferResult::Water
        } else {
            AquiferResult::Air
        }
    }

    pub fn simple_fluid_fill(&self, y: f64, density: f64) -> Option<BlockId> {
        if density > 0.0 {
            return None;
        }
        if y < self.sea_level as f64 {
            if y < -10.0 && self.lava_noise.get_value(0.0, y * 0.01, 0.0) > 0.0 {
                Some(BlockId::Lava)
            } else {
                Some(BlockId::Water)
            }
        } else {
            Some(BlockId::Air)
        }
    }
}
