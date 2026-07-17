//! Port of Minecraft's WorldCarver, CaveWorldCarver, CanyonWorldCarver, and configurations.
//!
//! Corresponding Java classes:
//! - `net.minecraft.world.level.levelgen.carver.WorldCarver`
//! - `net.minecraft.world.level.levelgen.carver.CaveWorldCarver`
//! - `net.minecraft.world.level.levelgen.carver.CanyonWorldCarver`
//! - `net.minecraft.world.level.levelgen.carver.CarverConfiguration`
//! - `net.minecraft.world.level.levelgen.carver.CaveCarverConfiguration`
//! - `net.minecraft.world.level.levelgen.carver.CanyonCarverConfiguration`

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_SIZE};
use crate::world::world_gen::aquifer::NoiseBasedAquifer;
use crate::world::world_gen::noise::NoiseSeed;
use crate::world::world_gen::structures::JavaLegacyRandom;
use crate::world::world_gen::surface::surface_blocks_for_biome;
use crate::world::world_gen::Biome;
use std::f64::consts::PI;

const JAVA_CARVER_SOURCE_RADIUS: i32 = 8;
const JAVA_CARVER_SOURCE_DIAMETER: usize = 17;
const JAVA_CARVER_SOURCE_COUNT: usize =
    JAVA_CARVER_SOURCE_DIAMETER * JAVA_CARVER_SOURCE_DIAMETER;
const JAVA_CARVERS_PER_SOURCE: usize = 3;
const JAVA_CARVER_MAX_DISTANCE: i32 = (4 * 2 - 1) * 16;

// ============================================================================
// Configurations
// ============================================================================

/// Common carver configuration, matching Java's CarverConfiguration.
pub struct CarverConfig {
    pub probability: f64,
    pub y_min: i32,
    pub y_max: i32,
    pub y_scale_min: f64,
    pub y_scale_max: f64,
    pub lava_level: i32,
}

impl CarverConfig {
    pub fn sample_y(&self, random: &mut NoiseSeed) -> f64 {
        if self.y_min >= self.y_max {
            return self.y_min as f64;
        }
        let range = (self.y_max - self.y_min) as f64;
        self.y_min as f64 + random.next_double() * range
    }

    pub fn sample_y_scale(&self, random: &mut NoiseSeed) -> f64 {
        if self.y_scale_min >= self.y_scale_max {
            return self.y_scale_min;
        }
        self.y_scale_min + (self.y_scale_max - self.y_scale_min) * random.next_double()
    }
}

/// Cave-specific carver configuration (extends CarverConfig).
/// Matches Java's CaveCarverConfiguration.
pub struct CaveCarverConfig {
    pub base: CarverConfig,
    pub horizontal_radius_multiplier_min: f64,
    pub horizontal_radius_multiplier_max: f64,
    pub vertical_radius_multiplier_min: f64,
    pub vertical_radius_multiplier_max: f64,
    pub floor_level_min: f64,
    pub floor_level_max: f64,
    pub cave_bound: i32,
}

impl CaveCarverConfig {
    pub fn sample_h_mult(&self, random: &mut NoiseSeed) -> f64 {
        self.horizontal_radius_multiplier_min
            + (self.horizontal_radius_multiplier_max - self.horizontal_radius_multiplier_min) * random.next_double()
    }

    pub fn sample_v_mult(&self, random: &mut NoiseSeed) -> f64 {
        self.vertical_radius_multiplier_min
            + (self.vertical_radius_multiplier_max - self.vertical_radius_multiplier_min) * random.next_double()
    }

    pub fn sample_floor_level(&self, random: &mut NoiseSeed) -> f64 {
        self.floor_level_min + (self.floor_level_max - self.floor_level_min) * random.next_double()
    }
}

/// Canyon-specific carver configuration (extends CarverConfig).
pub struct CanyonCarverConfig {
    pub base: CarverConfig,
    pub horizontal_radius_factor_min: f64,
    pub horizontal_radius_factor_max: f64,
    pub vertical_rotation_min: f64,
    pub vertical_rotation_max: f64,
    pub thickness_min: f64,
    pub thickness_max: f64,
    pub distance_factor_min: f64,
    pub distance_factor_max: f64,
    pub width_smoothness: i32,
    pub vertical_radius_default_factor: f64,
    pub vertical_radius_center_factor: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CarverSemantics {
    Legacy,
    Java,
}

impl CanyonCarverConfig {
    pub fn sample_h_radius_factor(&self, random: &mut NoiseSeed) -> f64 {
        self.horizontal_radius_factor_min
            + (self.horizontal_radius_factor_max - self.horizontal_radius_factor_min) * random.next_double()
    }

    pub fn sample_vertical_rotation(&self, random: &mut NoiseSeed) -> f64 {
        self.vertical_rotation_min
            + (self.vertical_rotation_max - self.vertical_rotation_min) * random.next_double()
    }

    pub fn sample_thickness(&self, random: &mut NoiseSeed) -> f64 {
        self.thickness_min + (self.thickness_max - self.thickness_min) * random.next_double()
    }

    pub fn sample_distance_factor(&self, random: &mut NoiseSeed) -> f64 {
        self.distance_factor_min
            + (self.distance_factor_max - self.distance_factor_min) * random.next_double()
    }
}

// ============================================================================
// Standard overworld carver configurations (from configured_carver JSON)
// ============================================================================

/// `cave`: probability 0.15, y=8..180, yScale=0.1..0.9, lava=8 above bottom
pub fn cave_config() -> CaveCarverConfig {
    CaveCarverConfig {
        base: CarverConfig {
            probability: 0.15,
            y_min: 8,
            y_max: 180,
            y_scale_min: 0.1,
            y_scale_max: 0.9,
            lava_level: 8,
        },
        horizontal_radius_multiplier_min: 0.7,
        horizontal_radius_multiplier_max: 1.4,
        vertical_radius_multiplier_min: 0.8,
        vertical_radius_multiplier_max: 1.3,
        floor_level_min: -1.0,
        floor_level_max: -0.4,
        cave_bound: 15,
    }
}

/// `cave_extra_underground`: probability 0.07, y=8..47, otherwise same as cave
pub fn cave_extra_underground_config() -> CaveCarverConfig {
    CaveCarverConfig {
        base: CarverConfig {
            probability: 0.07,
            y_min: 8,
            y_max: 47,
            y_scale_min: 0.1,
            y_scale_max: 0.9,
            lava_level: 8,
        },
        horizontal_radius_multiplier_min: 0.7,
        horizontal_radius_multiplier_max: 1.4,
        vertical_radius_multiplier_min: 0.8,
        vertical_radius_multiplier_max: 1.3,
        floor_level_min: -1.0,
        floor_level_max: -0.4,
        cave_bound: 15,
    }
}

/// `canyon`: probability 0.02, y=8..max, yScale sampled per-canyon
pub fn canyon_config() -> CanyonCarverConfig {
    CanyonCarverConfig {
        base: CarverConfig {
            probability: 0.02,
            y_min: 8,
            y_max: 320,
            y_scale_min: 0.1,
            y_scale_max: 0.9,
            lava_level: 8,
        },
        horizontal_radius_factor_min: 0.75,
        horizontal_radius_factor_max: 1.0,
        vertical_rotation_min: -0.5,
        vertical_rotation_max: 0.5,
        thickness_min: 1.0,
        thickness_max: 3.0,
        distance_factor_min: 0.5,
        distance_factor_max: 1.0,
        width_smoothness: 3,
        vertical_radius_default_factor: 0.5,
        vertical_radius_center_factor: 1.0,
    }
}

fn cave_config_reference(min_y: i32) -> CaveCarverConfig {
    let mut config = cave_config();
    config.base.y_min = min_y + 8;
    config.base.lava_level = min_y + 8;
    config
}

fn cave_extra_underground_config_reference(min_y: i32) -> CaveCarverConfig {
    let mut config = cave_extra_underground_config();
    config.base.y_min = min_y + 8;
    config.base.lava_level = min_y + 8;
    config
}

fn canyon_config_reference(min_y: i32) -> CanyonCarverConfig {
    CanyonCarverConfig {
        base: CarverConfig {
            probability: 0.01,
            y_min: 10,
            y_max: 67,
            y_scale_min: 3.0,
            y_scale_max: 3.0,
            lava_level: min_y + 8,
        },
        horizontal_radius_factor_min: 0.75,
        horizontal_radius_factor_max: 1.0,
        vertical_rotation_min: -0.125,
        vertical_rotation_max: 0.125,
        thickness_min: 0.0,
        thickness_max: 6.0,
        distance_factor_min: 0.75,
        distance_factor_max: 1.0,
        width_smoothness: 3,
        vertical_radius_default_factor: 1.0,
        vertical_radius_center_factor: 0.0,
    }
}

// ============================================================================
// CaveWorldCarver
// ============================================================================

/// Carve the overworld cave system using random-walk tunnels and rooms.
/// Port of `CaveWorldCarver.carve()`.
pub fn carve_caves(
    chunk: &mut Chunk,
    config: &CaveCarverConfig,
    seed: u64,
    aquifer: &mut NoiseBasedAquifer,
    min_y: i32,
    height: i32,
) {
    carve_caves_with_semantics(
        chunk,
        config,
        seed,
        aquifer,
        min_y,
        height,
        CarverSemantics::Legacy,
    );
}

fn carve_caves_with_semantics(
    chunk: &mut Chunk,
    config: &CaveCarverConfig,
    seed: u64,
    aquifer: &mut NoiseBasedAquifer,
    min_y: i32,
    height: i32,
    semantics: CarverSemantics,
) {
    let max_distance = (4 * 2 - 1) * 16;
    let mut random = NoiseSeed::new(seed);

    let probability_sample = match semantics {
        CarverSemantics::Legacy => random.next_double(),
        CarverSemantics::Java => random.next_float() as f64,
    };
    if probability_sample > config.base.probability {
        return;
    }

    let bound_a = random.next_int(config.cave_bound).max(1) + 1;
    let bound_b = random.next_int(bound_a).max(1) + 1;
    let cave_count = random.next_int(bound_b).max(1) as usize;

    let chunk_base_x = (chunk.cx * CHUNK_SIZE as i32) as f64;
    let chunk_base_z = (chunk.cz * CHUNK_SIZE as i32) as f64;

    for _ in 0..cave_count {
        let cx = chunk_base_x + random.next_int(16) as f64;
        let cy = match semantics {
            CarverSemantics::Legacy => config.base.sample_y(&mut random),
            CarverSemantics::Java => {
                (config.base.y_min
                    + random.next_int(config.base.y_max - config.base.y_min + 1)) as f64
            }
        };
        let cz = chunk_base_z + random.next_int(16) as f64;
        let h_mult = config.sample_h_mult(&mut random);
        let v_mult = config.sample_v_mult(&mut random);
        let floor_level = config.sample_floor_level(&mut random);

        let skip = |_ctx: &NoiseBasedAquifer, xd: f64, yd: f64, zd: f64, _world_y: i32| -> bool {
            yd <= floor_level || xd * xd + yd * yd + zd * zd >= 1.0
        };

        let mut tunnels = 1usize;
        if random.next_int(4) == 0 {
            let y_scale = config.base.sample_y_scale(&mut random);
            let thickness = 1.0 + random.next_double() * 6.0;
            carve_room(
                chunk, aquifer, cx, cy, cz, thickness, y_scale, min_y, height, &skip,
                &mut random, semantics,
                config.base.lava_level,
            );
            tunnels += random.next_int(4) as usize;
        }

        for _ in 0..tunnels {
            let h_rotation = random.next_double() * 2.0 * PI;
            let v_rotation = (random.next_double() - 0.5) / 4.0;
            let thickness = cave_thickness(&mut random);
            let distance = (max_distance as f64 * (1.0 - random.next_double() * 0.25)) as i32;

            carve_tunnel(
                chunk, aquifer, cx, cy, cz,
                h_mult, v_mult, thickness, h_rotation, v_rotation,
                0, distance.max(1), 1.0, min_y, height, &skip, &mut random, semantics,
                config.base.lava_level,
            );
        }
    }
}

fn cave_thickness(random: &mut NoiseSeed) -> f64 {
    let mut t = random.next_double() * 2.0 + random.next_double();
    if random.next_int(10) == 0 {
        t *= random.next_double() * random.next_double() * 3.0 + 1.0;
    }
    t
}

fn carve_room(
    chunk: &mut Chunk,
    aquifer: &mut NoiseBasedAquifer,
    x: f64, y: f64, z: f64,
    thickness: f64, y_scale: f64,
    min_y: i32, height: i32,
    skip: &dyn Fn(&NoiseBasedAquifer, f64, f64, f64, i32) -> bool,
    random: &mut NoiseSeed,
    semantics: CarverSemantics,
    lava_level: i32,
) {
    let h_radius = 1.5 + (PI / 2.0).sin() * thickness;
    let v_radius = h_radius * y_scale;
    let _ = random;
    carve_ellipsoid(
        chunk, aquifer, x + 1.0, y, z, h_radius, v_radius, min_y, height, skip,
        semantics,
        lava_level,
    );
}

fn carve_tunnel(
    chunk: &mut Chunk,
    aquifer: &mut NoiseBasedAquifer,
    start_x: f64, start_y: f64, start_z: f64,
    h_mult: f64, v_mult: f64, thickness: f64,
    h_rotation: f64, v_rotation: f64,
    step: i32, dist: i32, y_scale: f64,
    min_y: i32, height: i32,
    skip: &dyn Fn(&NoiseBasedAquifer, f64, f64, f64, i32) -> bool,
    random: &mut NoiseSeed,
    semantics: CarverSemantics,
    lava_level: i32,
) {
    let tunnel_seed = random.next_long();
    let mut tunnel_random = NoiseSeed::new(tunnel_seed as u64);

    let split_point = tunnel_random.next_int((dist / 2).max(1)) + dist / 4;
    let steep = tunnel_random.next_int(6) == 0;

    let mut y_rota = 0.0f64;
    let mut x_rota = 0.0f64;
    let mut x = start_x;
    let mut y = start_y;
    let mut z = start_z;
    let mut hr = h_rotation;
    let mut vr = v_rotation;

    for current_step in step..dist {
        x += hr.sin() * vr.cos();
        y += vr.sin();
        z += hr.cos() * vr.cos();

        if steep {
            vr *= 0.92;
        } else {
            let r1 = tunnel_random.next_double();
            let r2 = tunnel_random.next_double();
            x_rota += (r1 - r2) * tunnel_random.next_double() * 4.0;
            y_rota += (r1 - r2) * tunnel_random.next_double() * 4.0;
        }

        hr += x_rota * 0.5;
        vr += y_rota * 0.5;

        let d_vertical = (current_step as f64 / dist as f64) * PI;
        let radius = d_vertical.sin() * thickness;
        let h_radius = h_mult * radius;
        let v_radius = radius * y_scale;

        if can_reach(chunk.cx, chunk.cz, x, z, current_step, dist, thickness as f32) {
            carve_ellipsoid(
                chunk, aquifer, x, y, z, h_radius, v_radius, min_y, height, skip,
                semantics,
                lava_level,
            );
        }

        if current_step == split_point {
            let branch_hr = tunnel_random.next_double() * 2.0 * PI;
            carve_tunnel(
                chunk, aquifer,
                x, y, z,
                h_mult, v_mult, thickness * tunnel_random.next_double() + tunnel_random.next_double(),
                hr + branch_hr, vr + (tunnel_random.next_double() - 0.5) / 4.0,
                current_step, dist, y_scale, min_y, height, skip, &mut tunnel_random,
                semantics,
                lava_level,
            );
        }
    }
}

// ============================================================================
// CanyonWorldCarver
// ============================================================================

/// Carve the overworld canyon/ravine system.
/// Port of `CanyonWorldCarver.carve()`.
pub fn carve_canyons(
    chunk: &mut Chunk,
    config: &CanyonCarverConfig,
    seed: u64,
    aquifer: &mut NoiseBasedAquifer,
    min_y: i32,
    height: i32,
) {
    carve_canyons_with_semantics(
        chunk,
        config,
        seed,
        aquifer,
        min_y,
        height,
        CarverSemantics::Legacy,
    );
}

fn carve_canyons_with_semantics(
    chunk: &mut Chunk,
    config: &CanyonCarverConfig,
    seed: u64,
    aquifer: &mut NoiseBasedAquifer,
    min_y: i32,
    height: i32,
    semantics: CarverSemantics,
) {
    let mut random = NoiseSeed::new(seed);

    let probability_sample = match semantics {
        CarverSemantics::Legacy => random.next_double(),
        CarverSemantics::Java => random.next_float() as f64,
    };
    if probability_sample > config.base.probability {
        return;
    }

    let max_distance = (4 * 2 - 1) * 16;
    let chunk_base_x = (chunk.cx * CHUNK_SIZE as i32) as f64;
    let chunk_base_z = (chunk.cz * CHUNK_SIZE as i32) as f64;

    let cx = chunk_base_x + random.next_int(16) as f64;
    let cy = match semantics {
        CarverSemantics::Legacy => config.base.sample_y(&mut random),
        CarverSemantics::Java => {
            (config.base.y_min + random.next_int(config.base.y_max - config.base.y_min + 1)) as f64
        }
    };
    let cz = chunk_base_z + random.next_int(16) as f64;
    let hr = random.next_double() * 2.0 * PI;
    let vr = config.sample_vertical_rotation(&mut random);
    let y_scale = config.base.sample_y_scale(&mut random);
    let thickness = match semantics {
        CarverSemantics::Legacy => config.sample_thickness(&mut random),
        // TrapezoidFloat(min=0, max=6, plateau=2).
        CarverSemantics::Java => {
            random.next_float() as f64 * 4.0 + random.next_float() as f64 * 2.0
        }
    };
    let distance = (max_distance as f64 * config.sample_distance_factor(&mut random)) as i32;

    do_canyon_carve(
        chunk, config, aquifer,
        cx, cy, cz, thickness, hr, vr,
        0, distance.max(1), y_scale, min_y, height, &mut random, semantics,
        config.base.lava_level,
    );
}

fn do_canyon_carve(
    chunk: &mut Chunk,
    config: &CanyonCarverConfig,
    aquifer: &mut NoiseBasedAquifer,
    start_x: f64, start_y: f64, start_z: f64,
    thickness: f64, h_rotation: f64, v_rotation: f64,
    step: i32, dist: i32, y_scale: f64,
    min_y: i32, height: i32,
    random: &mut NoiseSeed,
    semantics: CarverSemantics,
    lava_level: i32,
) {
    let tunnel_seed = random.next_long();
    let mut tunnel_random = NoiseSeed::new(tunnel_seed as u64);

    let width_factors = init_width_factors(config, height, &mut tunnel_random);

    let mut y_rota = 0.0f64;
    let mut x_rota = 0.0f64;
    let mut x = start_x;
    let mut y = start_y;
    let mut z = start_z;
    let mut hr = h_rotation;
    let mut vr = v_rotation;

    for current_step in step..dist {
        let t_progress = (current_step as f64 / dist as f64) * PI;
        let mut h_radius = 1.5 + t_progress.sin() * thickness;
        let mut v_radius = h_radius * y_scale;

        h_radius *= config.sample_h_radius_factor(&mut tunnel_random);

        let v_mult = 1.0 - (0.5 - current_step as f64 / dist as f64).abs() * 2.0;
        let v_factor = config.vertical_radius_default_factor + config.vertical_radius_center_factor * v_mult;
        v_radius *= v_factor * tunnel_random.next_double().max(0.75) + 0.75;

        x += hr.cos() * vr.cos();
        y += vr.sin();
        z += hr.sin() * vr.cos();

        vr *= 0.7;
        vr += x_rota * 0.05;
        hr += y_rota * 0.05;
        x_rota *= 0.8;
        y_rota *= 0.5;
        x_rota += (tunnel_random.next_double() - tunnel_random.next_double()) * tunnel_random.next_double() * 2.0;
        y_rota += (tunnel_random.next_double() - tunnel_random.next_double()) * tunnel_random.next_double() * 4.0;

        if tunnel_random.next_int(4) != 0 {
            if !can_reach(chunk.cx, chunk.cz, x, z, current_step, dist, thickness as f32) {
                return;
            }
            let skip = |_ctx: &NoiseBasedAquifer, xd: f64, yd: f64, zd: f64, world_y: i32| -> bool {
                let y_index = (world_y - min_y) as usize;
                let wf = width_factors.get(y_index).copied().unwrap_or(1.0);
                (xd * xd + zd * zd) * wf + yd * yd / 6.0 >= 1.0
            };
            carve_ellipsoid(
                chunk, aquifer, x, y, z, h_radius, v_radius, min_y, height, &skip,
                semantics,
                lava_level,
            );
        }
    }
}

fn init_width_factors(config: &CanyonCarverConfig, height: i32, random: &mut NoiseSeed) -> Vec<f64> {
    let depth = height as usize;
    let mut factors = vec![0.0f64; depth];
    let mut width_factor = 1.0f64;
    for y in 0..depth {
        if y == 0 || random.next_int(config.width_smoothness.max(1)) == 0 {
            width_factor = 1.0 + random.next_double() * random.next_double();
        }
        factors[y] = width_factor * width_factor;
    }
    factors
}

// ============================================================================
// Shared carving helpers
// ============================================================================

fn carve_ellipsoid(
    chunk: &mut Chunk,
    aquifer: &mut NoiseBasedAquifer,
    x: f64, y: f64, z: f64,
    h_radius: f64, v_radius: f64,
    min_y: i32, height: i32,
    skip: &dyn Fn(&NoiseBasedAquifer, f64, f64, f64, i32) -> bool,
    semantics: CarverSemantics,
    lava_level: i32,
) {
    let chunk_min_x = chunk.cx * CHUNK_SIZE as i32;
    let chunk_min_z = chunk.cz * CHUNK_SIZE as i32;

    let min_xi = ((x - h_radius).floor() as i32).max(chunk_min_x).min(chunk_min_x + CHUNK_SIZE as i32 - 1);
    let max_xi = ((x + h_radius).floor() as i32).max(chunk_min_x).min(chunk_min_x + CHUNK_SIZE as i32 - 1);
    let min_yi = ((y - v_radius).floor() as i32).max(min_y + 1);
    let max_yi = ((y + v_radius).floor() as i32).min(min_y + height - 1 - 7);
    let min_zi = ((z - h_radius).floor() as i32).max(chunk_min_z).min(chunk_min_z + CHUNK_SIZE as i32 - 1);
    let max_zi = ((z + h_radius).floor() as i32).max(chunk_min_z).min(chunk_min_z + CHUNK_SIZE as i32 - 1);

    if min_xi > max_xi || min_yi > max_yi || min_zi > max_zi {
        return;
    }

    for wx in min_xi..=max_xi {
        let xd = (wx as f64 + 0.5 - x) / h_radius;
        let xd2 = xd * xd;
        if xd2 >= 1.0 { continue; }
        let lx = (wx - chunk_min_x) as usize;

        for wz in min_zi..=max_zi {
            let zd = (wz as f64 + 0.5 - z) / h_radius;
            if xd2 + zd * zd >= 1.0 { continue; }
            let lz = (wz - chunk_min_z) as usize;

            for wy in (min_yi..=max_yi).rev() {
                let yd = (wy as f64 - 0.5 - y) / v_radius;
                if skip(aquifer, xd, yd, zd, wy) {
                    continue;
                }

                let Some(local_y) = world_y_to_local(wy, min_y, height) else { continue };
                let block = chunk.get_block(lx, local_y, lz);
                if block.is_air() {
                    continue;
                }

                if let Some(new_id) =
                    carve_state(aquifer, wx, wy, wz, semantics, lava_level)
                {
                    chunk.set_block(lx, local_y, lz, Block::new(new_id));
                }
            }
        }
    }
}

/// Determine what block to place in a carved position.
/// Uses the aquifer with density=0.0 (Java passes 0.0 for carving).
fn carve_state(
    aquifer: &mut NoiseBasedAquifer,
    world_x: i32,
    world_y: i32,
    world_z: i32,
    semantics: CarverSemantics,
    lava_level: i32,
) -> Option<crate::world::block::BlockId> {
    if semantics == CarverSemantics::Java && world_y <= lava_level {
        return Some(BlockId::Lava);
    }
    let (aquifer_x, aquifer_y, aquifer_z) =
        aquifer_sample_coordinates(semantics, world_x, world_y, world_z);
    carver_replacement(
        semantics,
        aquifer.compute_substance(aquifer_x, aquifer_y, aquifer_z, 0.0),
    )
}

fn aquifer_sample_coordinates(
    semantics: CarverSemantics,
    world_x: i32,
    world_y: i32,
    world_z: i32,
) -> (i32, i32, i32) {
    match semantics {
        CarverSemantics::Legacy => (world_y, world_x, world_z),
        CarverSemantics::Java => (world_x, world_y, world_z),
    }
}

fn carver_replacement(
    semantics: CarverSemantics,
    aquifer_result: Option<BlockId>,
) -> Option<BlockId> {
    match (semantics, aquifer_result) {
        (_, Some(block)) => Some(block),
        (CarverSemantics::Legacy, None) => Some(BlockId::Air),
        (CarverSemantics::Java, None) => None,
    }
}

/// Check whether an ellipsoid position is reachable within chunk range.
fn can_reach(chunk_cx: i32, chunk_cz: i32, x: f64, z: f64, step: i32, total: i32, thickness: f32) -> bool {
    let x_mid = chunk_cx as f64 * CHUNK_SIZE as f64 + 8.0;
    let z_mid = chunk_cz as f64 * CHUNK_SIZE as f64 + 8.0;
    let xd = x - x_mid;
    let zd = z - z_mid;
    let remaining = (total - step) as f64;
    let rr = (thickness + 2.0 + 16.0) as f64;
    xd * xd + zd * zd - remaining * remaining <= rr * rr
}

/// Convert a world Y coordinate to a local storage Y index.
fn world_y_to_local(world_y: i32, min_y: i32, height: i32) -> Option<usize> {
    let local = world_y - min_y;
    if local >= 0 && local < height {
        Some(local as usize)
    } else {
        None
    }
}

// ============================================================================
// Minecraft 26.2 target-projected carvers (Geometry profile only)
// ============================================================================

struct ReferenceCarvingMask {
    bits: Vec<bool>,
}

impl ReferenceCarvingMask {
    fn new(height: i32) -> Self {
        Self {
            bits: vec![false; CHUNK_SIZE * CHUNK_SIZE * height as usize],
        }
    }

    fn claim(&mut self, x: usize, local_y: usize, z: usize) -> bool {
        let index = (local_y * CHUNK_SIZE + z) * CHUNK_SIZE + x;
        if self.bits[index] {
            false
        } else {
            self.bits[index] = true;
            true
        }
    }

    #[cfg(test)]
    fn is_claimed(&self, x: usize, local_y: usize, z: usize) -> bool {
        self.bits[(local_y * CHUNK_SIZE + z) * CHUNK_SIZE + x]
    }
}

enum ReferenceSkip<'a> {
    Cave(f64),
    Canyon(&'a [f32]),
}

impl ReferenceSkip<'_> {
    fn should_skip(&self, xd: f64, yd: f64, zd: f64, world_y: i32, min_y: i32) -> bool {
        match self {
            Self::Cave(floor_level) => {
                yd <= *floor_level || xd * xd + yd * yd + zd * zd >= 1.0
            }
            Self::Canyon(width_factors) => {
                let y_index = (world_y - min_y - 1) as usize;
                (xd * xd + zd * zd) * f64::from(width_factors[y_index]) + yd * yd / 6.0
                    >= 1.0
            }
        }
    }
}

fn java_random_between(random: &mut JavaLegacyRandom, min: f32, max: f32) -> f32 {
    min + random.next_float() * (max - min)
}

fn java_sin(value: f32) -> f32 {
    const SCALE: f64 = 10_430.378_350_470_453;
    let index = ((f64::from(value) * SCALE) as i64 & 65_535) as f64;
    (index / SCALE).sin() as f32
}

fn java_cos(value: f32) -> f32 {
    const SCALE: f64 = 10_430.378_350_470_453;
    let index = ((f64::from(value) * SCALE + 16_384.0) as i64 & 65_535) as f64;
    (index / SCALE).sin() as f32
}

fn reference_carve_caves<F>(
    chunk: &mut Chunk,
    source_x: i32,
    source_z: i32,
    config: &CaveCarverConfig,
    random: &mut JavaLegacyRandom,
    aquifer: &mut NoiseBasedAquifer,
    mask: &mut ReferenceCarvingMask,
    min_y: i32,
    height: i32,
    biome_at: &mut F,
) where
    F: FnMut(i32, i32, i32) -> Biome,
{
    let inner_bound = random.next_int_bound(config.cave_bound) + 1;
    let middle_bound = random.next_int_bound(inner_bound) + 1;
    let cave_count = random.next_int_bound(middle_bound);

    for _ in 0..cave_count {
        let x = f64::from(source_x.wrapping_mul(16).wrapping_add(random.next_int_bound(16)));
        let y = f64::from(
            config.base.y_min
                + random.next_int_bound(config.base.y_max - config.base.y_min + 1),
        );
        let z = f64::from(source_z.wrapping_mul(16).wrapping_add(random.next_int_bound(16)));
        let horizontal_multiplier = f64::from(java_random_between(
            random,
            config.horizontal_radius_multiplier_min as f32,
            config.horizontal_radius_multiplier_max as f32,
        ));
        let vertical_multiplier = f64::from(java_random_between(
            random,
            config.vertical_radius_multiplier_min as f32,
            config.vertical_radius_multiplier_max as f32,
        ));
        let floor_level = f64::from(java_random_between(
            random,
            config.floor_level_min as f32,
            config.floor_level_max as f32,
        ));
        let skip = ReferenceSkip::Cave(floor_level);
        let mut tunnels = 1;

        if random.next_int_bound(4) == 0 {
            let y_scale = f64::from(java_random_between(
                random,
                config.base.y_scale_min as f32,
                config.base.y_scale_max as f32,
            ));
            let thickness = 1.0_f32 + random.next_float() * 6.0_f32;
            let horizontal_radius =
                1.5 + f64::from(java_sin(std::f32::consts::FRAC_PI_2) * thickness);
            reference_carve_ellipsoid(
                chunk,
                aquifer,
                mask,
                x + 1.0,
                y,
                z,
                horizontal_radius,
                horizontal_radius * y_scale,
                min_y,
                height,
                config.base.lava_level,
                &skip,
                biome_at,
            );
            tunnels += random.next_int_bound(4);
        }

        for _ in 0..tunnels {
            let horizontal_rotation = random.next_float() * std::f32::consts::TAU;
            let vertical_rotation = (random.next_float() - 0.5_f32) / 4.0_f32;
            let mut thickness = random.next_float() * 2.0_f32 + random.next_float();
            if random.next_int_bound(10) == 0 {
                thickness *= random.next_float() * random.next_float() * 3.0_f32 + 1.0_f32;
            }
            let distance = JAVA_CARVER_MAX_DISTANCE
                - random.next_int_bound(JAVA_CARVER_MAX_DISTANCE / 4);
            let tunnel_seed = random.next_long();
            reference_carve_cave_tunnel(
                chunk,
                aquifer,
                mask,
                tunnel_seed,
                x,
                y,
                z,
                horizontal_multiplier,
                vertical_multiplier,
                thickness,
                horizontal_rotation,
                vertical_rotation,
                0,
                distance,
                1.0,
                min_y,
                height,
                config.base.lava_level,
                &skip,
                biome_at,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reference_carve_cave_tunnel<F>(
    chunk: &mut Chunk,
    aquifer: &mut NoiseBasedAquifer,
    mask: &mut ReferenceCarvingMask,
    tunnel_seed: i64,
    mut x: f64,
    mut y: f64,
    mut z: f64,
    horizontal_multiplier: f64,
    vertical_multiplier: f64,
    thickness: f32,
    mut horizontal_rotation: f32,
    mut vertical_rotation: f32,
    step: i32,
    distance: i32,
    y_scale: f64,
    min_y: i32,
    height: i32,
    lava_level: i32,
    skip: &ReferenceSkip<'_>,
    biome_at: &mut F,
) where
    F: FnMut(i32, i32, i32) -> Biome,
{
    let mut random = JavaLegacyRandom::new(tunnel_seed);
    let split_point = random.next_int_bound(distance / 2) + distance / 4;
    let steep = random.next_int_bound(6) == 0;
    let mut y_rota = 0.0_f32;
    let mut x_rota = 0.0_f32;

    for current_step in step..distance {
        let horizontal_radius = 1.5
            + f64::from(
                java_sin(std::f32::consts::PI * current_step as f32 / distance as f32)
                    * thickness,
            );
        let vertical_radius = horizontal_radius * y_scale;
        let cos_vertical = java_cos(vertical_rotation);
        x += f64::from(java_cos(horizontal_rotation) * cos_vertical);
        y += f64::from(java_sin(vertical_rotation));
        z += f64::from(java_sin(horizontal_rotation) * cos_vertical);
        vertical_rotation *= if steep { 0.92_f32 } else { 0.7_f32 };
        vertical_rotation += x_rota * 0.1_f32;
        horizontal_rotation += y_rota * 0.1_f32;
        x_rota *= 0.9_f32;
        y_rota *= 0.75_f32;
        x_rota +=
            (random.next_float() - random.next_float()) * random.next_float() * 2.0_f32;
        y_rota +=
            (random.next_float() - random.next_float()) * random.next_float() * 4.0_f32;

        if current_step == split_point && thickness > 1.0_f32 {
            let left_seed = random.next_long();
            let left_thickness = random.next_float() * 0.5_f32 + 0.5_f32;
            reference_carve_cave_tunnel(
                chunk,
                aquifer,
                mask,
                left_seed,
                x,
                y,
                z,
                horizontal_multiplier,
                vertical_multiplier,
                left_thickness,
                horizontal_rotation - std::f32::consts::FRAC_PI_2,
                vertical_rotation / 3.0_f32,
                current_step,
                distance,
                1.0,
                min_y,
                height,
                lava_level,
                skip,
                biome_at,
            );
            let right_seed = random.next_long();
            let right_thickness = random.next_float() * 0.5_f32 + 0.5_f32;
            reference_carve_cave_tunnel(
                chunk,
                aquifer,
                mask,
                right_seed,
                x,
                y,
                z,
                horizontal_multiplier,
                vertical_multiplier,
                right_thickness,
                horizontal_rotation + std::f32::consts::FRAC_PI_2,
                vertical_rotation / 3.0_f32,
                current_step,
                distance,
                1.0,
                min_y,
                height,
                lava_level,
                skip,
                biome_at,
            );
            return;
        }

        if random.next_int_bound(4) != 0 {
            if !can_reach(chunk.cx, chunk.cz, x, z, current_step, distance, thickness) {
                return;
            }
            reference_carve_ellipsoid(
                chunk,
                aquifer,
                mask,
                x,
                y,
                z,
                horizontal_radius * horizontal_multiplier,
                vertical_radius * vertical_multiplier,
                min_y,
                height,
                lava_level,
                skip,
                biome_at,
            );
        }
    }
}

fn reference_carve_canyon<F>(
    chunk: &mut Chunk,
    source_x: i32,
    source_z: i32,
    config: &CanyonCarverConfig,
    random: &mut JavaLegacyRandom,
    aquifer: &mut NoiseBasedAquifer,
    mask: &mut ReferenceCarvingMask,
    min_y: i32,
    height: i32,
    biome_at: &mut F,
) where
    F: FnMut(i32, i32, i32) -> Biome,
{
    let x = f64::from(source_x.wrapping_mul(16).wrapping_add(random.next_int_bound(16)));
    let y = f64::from(
        config.base.y_min + random.next_int_bound(config.base.y_max - config.base.y_min + 1),
    );
    let z = f64::from(source_z.wrapping_mul(16).wrapping_add(random.next_int_bound(16)));
    let horizontal_rotation = random.next_float() * std::f32::consts::TAU;
    let vertical_rotation = java_random_between(
        random,
        config.vertical_rotation_min as f32,
        config.vertical_rotation_max as f32,
    );
    let y_scale = f64::from(config.base.y_scale_min as f32);
    let thickness = random.next_float() * 4.0_f32 + random.next_float() * 2.0_f32;
    let distance_factor = java_random_between(
        random,
        config.distance_factor_min as f32,
        config.distance_factor_max as f32,
    );
    let distance = (JAVA_CARVER_MAX_DISTANCE as f32 * distance_factor) as i32;
    let tunnel_seed = random.next_long();
    reference_carve_canyon_tunnel(
        chunk,
        aquifer,
        mask,
        config,
        tunnel_seed,
        x,
        y,
        z,
        thickness,
        horizontal_rotation,
        vertical_rotation,
        distance,
        y_scale,
        min_y,
        height,
        biome_at,
    );
}

#[allow(clippy::too_many_arguments)]
fn reference_carve_canyon_tunnel<F>(
    chunk: &mut Chunk,
    aquifer: &mut NoiseBasedAquifer,
    mask: &mut ReferenceCarvingMask,
    config: &CanyonCarverConfig,
    tunnel_seed: i64,
    mut x: f64,
    mut y: f64,
    mut z: f64,
    thickness: f32,
    mut horizontal_rotation: f32,
    mut vertical_rotation: f32,
    distance: i32,
    y_scale: f64,
    min_y: i32,
    height: i32,
    biome_at: &mut F,
) where
    F: FnMut(i32, i32, i32) -> Biome,
{
    let mut random = JavaLegacyRandom::new(tunnel_seed);
    let mut width_factors = vec![0.0_f32; height as usize];
    let mut width_factor = 1.0_f32;
    for (index, factor) in width_factors.iter_mut().enumerate() {
        if index == 0 || random.next_int_bound(config.width_smoothness) == 0 {
            width_factor = 1.0_f32 + random.next_float() * random.next_float();
        }
        *factor = width_factor * width_factor;
    }
    let skip = ReferenceSkip::Canyon(&width_factors);
    let mut y_rota = 0.0_f32;
    let mut x_rota = 0.0_f32;

    for current_step in 0..distance {
        let mut horizontal_radius = 1.5
            + f64::from(
                java_sin(current_step as f32 * std::f32::consts::PI / distance as f32)
                    * thickness,
            );
        let mut vertical_radius = horizontal_radius * y_scale;
        horizontal_radius *= f64::from(java_random_between(
            &mut random,
            config.horizontal_radius_factor_min as f32,
            config.horizontal_radius_factor_max as f32,
        ));
        let vertical_multiplier =
            1.0_f32 - (0.5_f32 - current_step as f32 / distance as f32).abs() * 2.0_f32;
        let vertical_factor = config.vertical_radius_default_factor as f32
            + config.vertical_radius_center_factor as f32 * vertical_multiplier;
        vertical_radius *= f64::from(
            vertical_factor * java_random_between(&mut random, 0.75_f32, 1.0_f32),
        );
        let cos_vertical = java_cos(vertical_rotation);
        x += f64::from(java_cos(horizontal_rotation) * cos_vertical);
        y += f64::from(java_sin(vertical_rotation));
        z += f64::from(java_sin(horizontal_rotation) * cos_vertical);
        vertical_rotation *= 0.7_f32;
        vertical_rotation += x_rota * 0.05_f32;
        horizontal_rotation += y_rota * 0.05_f32;
        x_rota *= 0.8_f32;
        y_rota *= 0.5_f32;
        x_rota +=
            (random.next_float() - random.next_float()) * random.next_float() * 2.0_f32;
        y_rota +=
            (random.next_float() - random.next_float()) * random.next_float() * 4.0_f32;

        if random.next_int_bound(4) != 0 {
            if !can_reach(chunk.cx, chunk.cz, x, z, current_step, distance, thickness) {
                return;
            }
            reference_carve_ellipsoid(
                chunk,
                aquifer,
                mask,
                x,
                y,
                z,
                horizontal_radius,
                vertical_radius,
                min_y,
                height,
                config.base.lava_level,
                &skip,
                biome_at,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reference_carve_ellipsoid<F>(
    chunk: &mut Chunk,
    aquifer: &mut NoiseBasedAquifer,
    mask: &mut ReferenceCarvingMask,
    x: f64,
    y: f64,
    z: f64,
    horizontal_radius: f64,
    vertical_radius: f64,
    min_y: i32,
    height: i32,
    lava_level: i32,
    skip: &ReferenceSkip<'_>,
    biome_at: &mut F,
) -> bool
where
    F: FnMut(i32, i32, i32) -> Biome,
{
    let chunk_min_x = chunk.cx.wrapping_mul(CHUNK_SIZE as i32);
    let chunk_min_z = chunk.cz.wrapping_mul(CHUNK_SIZE as i32);
    let center_x = f64::from(chunk_min_x) + 8.0;
    let center_z = f64::from(chunk_min_z) + 8.0;
    let max_delta = 16.0 + horizontal_radius * 2.0;
    if (x - center_x).abs() > max_delta || (z - center_z).abs() > max_delta {
        return false;
    }

    let min_x = ((x - horizontal_radius).floor() as i32 - chunk_min_x - 1).max(0);
    let max_x = ((x + horizontal_radius).floor() as i32 - chunk_min_x).min(15);
    let lower_y = ((y - vertical_radius).floor() as i32 - 1).max(min_y + 1);
    let max_y = ((y + vertical_radius).floor() as i32 + 1).min(min_y + height - 8);
    let min_z = ((z - horizontal_radius).floor() as i32 - chunk_min_z - 1).max(0);
    let max_z = ((z + horizontal_radius).floor() as i32 - chunk_min_z).min(15);
    let mut carved = false;

    for local_x in min_x..=max_x {
        let world_x = chunk_min_x + local_x;
        let xd = (f64::from(world_x) + 0.5 - x) / horizontal_radius;
        for local_z in min_z..=max_z {
            let world_z = chunk_min_z + local_z;
            let zd = (f64::from(world_z) + 0.5 - z) / horizontal_radius;
            if xd * xd + zd * zd >= 1.0 {
                continue;
            }
            let mut has_grass = false;
            for world_y in (lower_y + 1..=max_y).rev() {
                let yd = (f64::from(world_y) - 0.5 - y) / vertical_radius;
                if skip.should_skip(xd, yd, zd, world_y, min_y) {
                    continue;
                }
                let local_y = (world_y - min_y) as usize;
                if !mask.claim(local_x as usize, local_y, local_z as usize) {
                    continue;
                }
                let current = chunk.get_block(local_x as usize, local_y, local_z as usize).id;
                if matches!(current, BlockId::GrassBlock | BlockId::Mycelium) {
                    has_grass = true;
                }
                if !reference_carver_replaceable(current) {
                    continue;
                }
                let Some(replacement) = carve_state(
                    aquifer,
                    world_x,
                    world_y,
                    world_z,
                    CarverSemantics::Java,
                    lava_level,
                ) else {
                    continue;
                };
                chunk.set_block(
                    local_x as usize,
                    local_y,
                    local_z as usize,
                    Block::new(replacement),
                );
                carved = true;

                if has_grass && local_y > 0 {
                    let below_y = local_y - 1;
                    if chunk.get_block(local_x as usize, below_y, local_z as usize).id
                        == BlockId::Dirt
                    {
                        let (top, subsurface, _) =
                            surface_blocks_for_biome(biome_at(world_x, world_y - 1, world_z));
                        let repaired = if matches!(replacement, BlockId::Water | BlockId::Lava) {
                            subsurface
                        } else {
                            top
                        };
                        chunk.set_block(
                            local_x as usize,
                            below_y,
                            local_z as usize,
                            Block::new(repaired),
                        );
                    }
                }
            }
        }
    }
    carved
}

fn reference_carver_replaceable(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Stone
            | BlockId::Granite
            | BlockId::Diorite
            | BlockId::Andesite
            | BlockId::Tuff
            | BlockId::Deepslate
            | BlockId::Dirt
            | BlockId::CoarseDirt
            | BlockId::RootedDirt
            | BlockId::Mud
            | BlockId::MuddyMangroveRoots
            | BlockId::MossBlock
            | BlockId::GrassBlock
            | BlockId::Podzol
            | BlockId::Mycelium
            | BlockId::Sand
            | BlockId::RedSand
            | BlockId::Terracotta
            | BlockId::WhiteTerracotta
            | BlockId::OrangeTerracotta
            | BlockId::MagentaTerracotta
            | BlockId::LightBlueTerracotta
            | BlockId::YellowTerracotta
            | BlockId::LimeTerracotta
            | BlockId::PinkTerracotta
            | BlockId::GrayTerracotta
            | BlockId::LightGrayTerracotta
            | BlockId::CyanTerracotta
            | BlockId::PurpleTerracotta
            | BlockId::BlueTerracotta
            | BlockId::BrownTerracotta
            | BlockId::GreenTerracotta
            | BlockId::RedTerracotta
            | BlockId::BlackTerracotta
            | BlockId::IronOre
            | BlockId::DeepslateIronOre
            | BlockId::CopperOre
            | BlockId::DeepslateCopperOre
            | BlockId::Snow
            | BlockId::Snow2
            | BlockId::SnowBlock
            | BlockId::PowderSnow
            | BlockId::Water
            | BlockId::Gravel
            | BlockId::Sandstone
            | BlockId::Calcite
            | BlockId::PackedIce
            | BlockId::RawIronBlock
            | BlockId::RawCopperBlock
    )
}

fn reference_source_chunks(
    target_x: i32,
    target_z: i32,
) -> impl Iterator<Item = (i32, i32)> {
    (-JAVA_CARVER_SOURCE_RADIUS..=JAVA_CARVER_SOURCE_RADIUS).flat_map(move |dx| {
        (-JAVA_CARVER_SOURCE_RADIUS..=JAVA_CARVER_SOURCE_RADIUS).map(move |dz| {
            (target_x.wrapping_add(dx), target_z.wrapping_add(dz))
        })
    })
}

// ============================================================================
// Carve pass — run all configured overworld carvers for a chunk
// ============================================================================

/// Run all standard overworld carvers for a chunk, in Java's order.
pub fn carve_overworld_chunk(
    chunk: &mut Chunk,
    world_seed: u64,
    aquifer: &mut NoiseBasedAquifer,
    min_y: i32,
    height: i32,
) {
    let chunk_base_x = chunk.cx * CHUNK_SIZE as i32;
    let chunk_base_z = chunk.cz * CHUNK_SIZE as i32;

    // Use the decoration seed as the base carver seed (Java uses
    // setLargeFeatureSeed with the same pattern).
    let decoration_seed = {
        let mut random = NoiseSeed::new(world_seed);
        let xm = (random.next_long() | 1) as i64;
        let zm = (random.next_long() | 1) as i64;
        let mixed = (chunk_base_x as i64)
            .wrapping_mul(xm)
            .wrapping_add((chunk_base_z as i64).wrapping_mul(zm)) as u64 ^ world_seed;
        mixed
    };

    // Caves (step 0)
    let cave_seed = decoration_seed.wrapping_add(0);
    carve_caves(chunk, &cave_config(), cave_seed, aquifer, min_y, height);

    // Extra underground caves (step 1)
    let extra_seed = decoration_seed.wrapping_add(1);
    carve_caves(chunk, &cave_extra_underground_config(), extra_seed, aquifer, min_y, height);

    // Canyons (step 2)
    let canyon_seed = decoration_seed.wrapping_add(2);
    carve_canyons(chunk, &canyon_config(), canyon_seed, aquifer, min_y, height);
}

/// Geometry-only Minecraft 26.2 carver pass. Every Java source chunk owns its
/// RNG and attempts, while all writes are clipped to this target chunk.
pub fn carve_overworld_chunk_reference<F>(
    chunk: &mut Chunk,
    world_seed: u64,
    aquifer: &mut NoiseBasedAquifer,
    min_y: i32,
    height: i32,
    biome_at: F,
) where
    F: FnMut(i32, i32, i32) -> Biome,
{
    let cave = cave_config_reference(min_y);
    let extra = cave_extra_underground_config_reference(min_y);
    let canyon = canyon_config_reference(min_y);
    let target_x = chunk.cx;
    let target_z = chunk.cz;
    let mut random = JavaLegacyRandom::new(0);
    let mut mask = ReferenceCarvingMask::new(height);
    let mut biome_at = biome_at;
    let mut source_count = 0;

    for (source_x, source_z) in reference_source_chunks(target_x, target_z) {
        source_count += 1;

        random.set_large_feature_seed(world_seed as i64, source_x, source_z);
        if random.next_float() <= cave.base.probability as f32 {
            reference_carve_caves(
                chunk,
                source_x,
                source_z,
                &cave,
                &mut random,
                aquifer,
                &mut mask,
                min_y,
                height,
                &mut biome_at,
            );
        }

        random.set_large_feature_seed(
            (world_seed as i64).wrapping_add(1),
            source_x,
            source_z,
        );
        if random.next_float() <= extra.base.probability as f32 {
            reference_carve_caves(
                chunk,
                source_x,
                source_z,
                &extra,
                &mut random,
                aquifer,
                &mut mask,
                min_y,
                height,
                &mut biome_at,
            );
        }

        random.set_large_feature_seed(
            (world_seed as i64).wrapping_add(2),
            source_x,
            source_z,
        );
        if random.next_float() <= canyon.base.probability as f32 {
            reference_carve_canyon(
                chunk,
                source_x,
                source_z,
                &canyon,
                &mut random,
                aquifer,
                &mut mask,
                min_y,
                height,
                &mut biome_at,
            );
        }
    }

    debug_assert_eq!(source_count, JAVA_CARVER_SOURCE_COUNT);
    debug_assert_eq!(source_count * JAVA_CARVERS_PER_SOURCE, 867);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::world_gen::noise_router::NoiseRouterData;
    use std::sync::Arc;

    #[test]
    fn reference_carver_uses_xyz_aquifer_coordinates_and_java_null_semantics() {
        assert_eq!(
            aquifer_sample_coordinates(CarverSemantics::Java, 11, -37, 29),
            (11, -37, 29)
        );
        assert_eq!(
            aquifer_sample_coordinates(CarverSemantics::Legacy, 11, -37, 29),
            (-37, 11, 29)
        );
        assert_eq!(carver_replacement(CarverSemantics::Java, None), None);
        assert_eq!(
            carver_replacement(CarverSemantics::Legacy, None),
            Some(BlockId::Air)
        );
    }

    #[test]
    fn reference_carver_configs_resolve_overworld_anchors_and_26_2_ranges() {
        let cave = cave_config_reference(-64);
        let extra = cave_extra_underground_config_reference(-64);
        let canyon = canyon_config_reference(-64);
        assert_eq!((cave.base.y_min, cave.base.y_max), (-56, 180));
        assert_eq!((extra.base.y_min, extra.base.y_max), (-56, 47));
        assert_eq!((canyon.base.y_min, canyon.base.y_max), (10, 67));
        assert_eq!(
            (cave.base.lava_level, extra.base.lava_level, canyon.base.lava_level),
            (-56, -56, -56)
        );
        assert_eq!(canyon.base.probability, 0.01);
        assert_eq!((canyon.base.y_scale_min, canyon.base.y_scale_max), (3.0, 3.0));
        assert_eq!(
            (canyon.thickness_min, canyon.thickness_max),
            (0.0, 6.0)
        );
    }

    #[test]
    fn reference_source_scan_is_exact_and_negative_coordinate_safe() {
        let sources = reference_source_chunks(-9, -12).collect::<Vec<_>>();
        assert_eq!(sources.len(), JAVA_CARVER_SOURCE_COUNT);
        assert_eq!(sources.first(), Some(&(-17, -20)));
        assert_eq!(sources.last(), Some(&(-1, -4)));
        assert_eq!(sources[1], (-17, -19));
        assert_eq!(sources[JAVA_CARVER_SOURCE_DIAMETER], (-16, -20));
        assert_eq!(sources.len() * JAVA_CARVERS_PER_SOURCE, 867);
    }

    #[test]
    fn reference_mask_claims_each_target_block_once() {
        let mut mask = ReferenceCarvingMask::new(384);
        assert!(mask.claim(15, 7, 0));
        assert!(!mask.claim(15, 7, 0));
        assert!(mask.is_claimed(15, 7, 0));
        assert!(mask.claim(0, 7, 0));
    }

    #[test]
    fn source_owned_cave_projection_crosses_chunk_edge_without_mask_seam() {
        let seed = 0x5EED_u64;
        let router = Arc::new(NoiseRouterData::create_overworld_router_reference(
            seed, false, false,
        ));
        let mut left = Chunk::new(0, 0);
        let mut right = Chunk::new(1, 0);
        left.blocks.fill(Block::new(BlockId::Stone));
        right.blocks.fill(Block::new(BlockId::Stone));
        let mut left_aquifer = NoiseBasedAquifer::overworld(0, 0, router.clone(), seed);
        let mut right_aquifer = NoiseBasedAquifer::overworld(1, 0, router, seed);
        let mut left_mask = ReferenceCarvingMask::new(384);
        let mut right_mask = ReferenceCarvingMask::new(384);
        let skip = ReferenceSkip::Cave(-1.0);

        reference_carve_ellipsoid(
            &mut left,
            &mut left_aquifer,
            &mut left_mask,
            16.0,
            -56.0,
            8.0,
            4.0,
            4.0,
            -64,
            384,
            -56,
            &skip,
            &mut |_, _, _| Biome::Plains,
        );
        reference_carve_ellipsoid(
            &mut right,
            &mut right_aquifer,
            &mut right_mask,
            16.0,
            -56.0,
            8.0,
            4.0,
            4.0,
            -64,
            384,
            -56,
            &skip,
            &mut |_, _, _| Biome::Plains,
        );

        let mut claimed_edge_blocks = 0;
        for local_y in 0..384 {
            for z in 0..CHUNK_SIZE {
                let left_claimed = left_mask.is_claimed(15, local_y, z);
                let right_claimed = right_mask.is_claimed(0, local_y, z);
                assert_eq!(left_claimed, right_claimed, "mask seam at y={local_y}, z={z}");
                claimed_edge_blocks += usize::from(left_claimed);
            }
        }
        assert!(claimed_edge_blocks > 0);
        assert_eq!(left.get_block(15, 6, 8).id, BlockId::Lava);
        assert_eq!(right.get_block(0, 6, 8).id, BlockId::Lava);
    }
}
