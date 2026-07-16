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
use std::f64::consts::PI;

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
    let max_distance = (4 * 2 - 1) * 16;
    let mut random = NoiseSeed::new(seed);

    if random.next_double() > config.base.probability {
        return;
    }

    let bound_a = random.next_int(config.cave_bound).max(1) + 1;
    let bound_b = random.next_int(bound_a).max(1) + 1;
    let cave_count = random.next_int(bound_b).max(1) as usize;

    let chunk_base_x = (chunk.cx * CHUNK_SIZE as i32) as f64;
    let chunk_base_z = (chunk.cz * CHUNK_SIZE as i32) as f64;

    for _ in 0..cave_count {
        let cx = chunk_base_x + random.next_int(16) as f64;
        let cy = config.base.sample_y(&mut random);
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
            carve_room(chunk, aquifer, cx, cy, cz, thickness, y_scale, min_y, height, &skip, &mut random);
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
                0, distance.max(1), 1.0, min_y, height, &skip, &mut random,
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
) {
    let h_radius = 1.5 + (PI / 2.0).sin() * thickness;
    let v_radius = h_radius * y_scale;
    let _ = random;
    carve_ellipsoid(chunk, aquifer, x + 1.0, y, z, h_radius, v_radius, min_y, height, skip);
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
            carve_ellipsoid(chunk, aquifer, x, y, z, h_radius, v_radius, min_y, height, skip);
        }

        if current_step == split_point {
            let branch_hr = tunnel_random.next_double() * 2.0 * PI;
            carve_tunnel(
                chunk, aquifer,
                x, y, z,
                h_mult, v_mult, thickness * tunnel_random.next_double() + tunnel_random.next_double(),
                hr + branch_hr, vr + (tunnel_random.next_double() - 0.5) / 4.0,
                current_step, dist, y_scale, min_y, height, skip, &mut tunnel_random,
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
    let mut random = NoiseSeed::new(seed);

    if random.next_double() > config.base.probability {
        return;
    }

    let max_distance = (4 * 2 - 1) * 16;
    let chunk_base_x = (chunk.cx * CHUNK_SIZE as i32) as f64;
    let chunk_base_z = (chunk.cz * CHUNK_SIZE as i32) as f64;

    let cx = chunk_base_x + random.next_int(16) as f64;
    let cy = config.base.sample_y(&mut random);
    let cz = chunk_base_z + random.next_int(16) as f64;
    let hr = random.next_double() * 2.0 * PI;
    let vr = config.sample_vertical_rotation(&mut random);
    let y_scale = config.base.sample_y_scale(&mut random);
    let thickness = config.sample_thickness(&mut random);
    let distance = (max_distance as f64 * config.sample_distance_factor(&mut random)) as i32;

    do_canyon_carve(
        chunk, config, aquifer,
        cx, cy, cz, thickness, hr, vr,
        0, distance.max(1), y_scale, min_y, height, &mut random,
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
            carve_ellipsoid(chunk, aquifer, x, y, z, h_radius, v_radius, min_y, height, &skip);
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

                let (new_id, apply) = carve_state(aquifer, wy, wx, wz);
                if apply {
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
) -> (crate::world::block::BlockId, bool) {
    match aquifer.compute_substance(world_x, world_y, world_z, 0.0) {
        Some(block) => (block, true),
        None => (BlockId::Air, true),
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
