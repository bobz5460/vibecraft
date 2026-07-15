#![allow(dead_code)]

use crate::world::block::BlockId;
use crate::world::gen::noise::{NormalNoise, SimpleRandom};

pub struct CaveSystem {
    pub cave_layer_noise: NormalNoise,
    pub cave_cheese_noise: NormalNoise,
    pub spaghetti_2d_noise: NormalNoise,
    pub spaghetti_3d_noise: NormalNoise,
    pub spaghetti_roughness_noise: NormalNoise,
    pub cave_entrance_noise: NormalNoise,
    pub pillar_noise: NormalNoise,
    pub noodle_noise: NormalNoise,
}

impl CaveSystem {
    pub fn new(seed: u64) -> Self {
        let octave = -15;
        let amplitudes = &[1.0];
        let s = seed as i64;
        CaveSystem {
            cave_layer_noise: NormalNoise::new(s ^ 1, octave, amplitudes),
            cave_cheese_noise: NormalNoise::new(s ^ 2, octave, amplitudes),
            spaghetti_2d_noise: NormalNoise::new(s ^ 3, octave, amplitudes),
            spaghetti_3d_noise: NormalNoise::new(s ^ 4, octave, amplitudes),
            spaghetti_roughness_noise: NormalNoise::new(s ^ 5, octave, amplitudes),
            cave_entrance_noise: NormalNoise::new(s ^ 6, octave, amplitudes),
            pillar_noise: NormalNoise::new(s ^ 7, octave, amplitudes),
            noodle_noise: NormalNoise::new(s ^ 8, octave, amplitudes),
        }
    }

    pub fn compute_cave_density(&self, x: f64, y: f64, z: f64, density: f64) -> f64 {
        if density > 0.0 {
            return density;
        }

        if y > 200.0 || y < 0.0 {
            return density;
        }

        let cave_layer = self.compute_cave_layer(x, y, z);
        let cheese = self.compute_cheese_caves(x, y, z);
        let noodles = self.compute_noodle(x, y, z);

        let base_cave = (cave_layer * 4.0).powi(2) + cheese;

        let entrance = self.compute_entrance(x, y, z);
        let roughness = self.compute_roughness(x, y, z);
        let spaghetti_2d = self.compute_spaghetti_2d(x, z);

        let underground = base_cave.min(entrance) + (spaghetti_2d + roughness).min(entrance);

        let spaghetti_3d = self.compute_spaghetti_3d(x, y, z);

        let full_caves = underground.max(spaghetti_3d);

        density.max(full_caves + noodles)
    }

    fn compute_cave_layer(&self, x: f64, y: f64, z: f64) -> f64 {
        self.cave_layer_noise.get_value(x * 0.01, y * 0.005, z * 0.01)
    }

    fn compute_cheese_caves(&self, x: f64, y: f64, z: f64) -> f64 {
        let n = self.cave_cheese_noise.get_value(x * 0.025, y * 0.025, z * 0.025);
        n * 0.5 + (n * n) * 0.3
    }

    fn compute_spaghetti_2d(&self, x: f64, z: f64) -> f64 {
        let n = self.spaghetti_2d_noise.get_value(x * 0.01, 0.0, z * 0.01);
        (n * 3.0).sin() * 0.5 + 0.5
    }

    fn compute_spaghetti_3d(&self, x: f64, y: f64, z: f64) -> f64 {
        let n = self.spaghetti_3d_noise.get_value(x * 0.015, y * 0.015, z * 0.015);
        n * 0.3
    }

    fn compute_roughness(&self, x: f64, y: f64, z: f64) -> f64 {
        let n = self.spaghetti_roughness_noise.get_value(x * 0.05, y * 0.05, z * 0.05);
        n * 0.15
    }

    fn compute_entrance(&self, x: f64, y: f64, z: f64) -> f64 {
        let n = self.cave_entrance_noise.get_value(x * 0.01, y * 0.01, z * 0.01);
        n.max(0.0) * 0.3
    }

    pub fn compute_pillar(&self, x: f64, y: f64, z: f64) -> f64 {
        let n = self.pillar_noise.get_value(x * 0.02, y * 0.02, z * 0.02);
        -n.max(0.0).min(1.0) * 0.5
    }

    fn compute_noodle(&self, x: f64, y: f64, z: f64) -> f64 {
        let n = self.noodle_noise.get_value(x * 0.08, y * 0.08, z * 0.08);
        if n > 0.4 { n * 0.3 } else { -0.1 }
    }
}

pub const LARGE_VEIN_SEED_OFFSET: u64 = 9;

pub struct OreVeinSystem {
    pub vein_toggle_noise: NormalNoise,
    pub vein_ridged_noise: NormalNoise,
    pub vein_gap_noise: NormalNoise,
}

pub enum VeinResult {
    None,
    CopperVein(BlockId),
    IronVein(BlockId),
}

impl OreVeinSystem {
    pub fn new(seed: u64) -> Self {
        let octave = -15;
        let amplitudes = &[1.0];
        let s = seed as i64;
        OreVeinSystem {
            vein_toggle_noise: NormalNoise::new(s ^ 9, octave, amplitudes),
            vein_ridged_noise: NormalNoise::new(s ^ 10, octave, amplitudes),
            vein_gap_noise: NormalNoise::new(s ^ 11, octave, amplitudes),
        }
    }

    pub fn calculate(&self, x: f64, y: f64, z: f64, rng: &mut SimpleRandom) -> VeinResult {
        if y > 50.0 || y < -64.0 {
            return VeinResult::None;
        }

        let toggle = self.vein_toggle_noise.get_value(x * 0.01, y * 0.01, z * 0.01);
        let is_copper = toggle > 0.0;
        let veininess = toggle.abs();

        let (y_min, y_max) = if is_copper {
            (0.0, 50.0)
        } else {
            (-60.0, -8.0)
        };

        if y < y_min || y > y_max {
            return VeinResult::None;
        }

        let edge_dist = (y - y_min).min(y_max - y);
        let roundoff = if edge_dist < 20.0 {
            (edge_dist / 20.0 - 1.0) * 0.2
        } else {
            0.0
        };

        if veininess + roundoff < 0.4 {
            return VeinResult::None;
        }

        if rng.next_double() > 0.7 {
            return VeinResult::None;
        }

        let ridged = self.vein_ridged_noise.get_value(x * 0.015, y * 0.015, z * 0.015);
        if ridged >= 0.0 {
            return VeinResult::None;
        }

        let richness = if veininess < 0.4 {
            0.1
        } else if veininess > 0.6 {
            0.3
        } else {
            0.1 + (veininess - 0.4) / 0.2 * 0.2
        };

        let gap = self.vein_gap_noise.get_value(x * 0.015, y * 0.015, z * 0.015);
        if rng.next_double() < richness && gap > -0.3 {
            if is_copper {
                if rng.next_double() < 0.3 {
                    VeinResult::CopperVein(BlockId::RawCopperBlock)
                } else {
                    VeinResult::CopperVein(BlockId::CopperOre)
                }
            } else {
                if rng.next_double() < 0.3 {
                    VeinResult::IronVein(BlockId::RawIronBlock)
                } else {
                    VeinResult::IronVein(BlockId::IronOre)
                }
            }
        } else {
            if is_copper {
                VeinResult::CopperVein(BlockId::Granite)
            } else {
                VeinResult::IronVein(BlockId::Tuff)
            }
        }
    }
}
