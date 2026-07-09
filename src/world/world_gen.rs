use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use crate::world::block::{Block, BlockId};
use noise::{NoiseFn, Simplex};
use rand::Rng;

pub struct WorldGenerator {
    height_noise: Simplex,
    detail_noise: Simplex,
    cave_noise: Simplex,
    biome_noise: Simplex,
    river_noise: Simplex,
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        WorldGenerator {
            height_noise: Simplex::new(seed as u32),
            detail_noise: Simplex::new((seed + 1) as u32),
            cave_noise: Simplex::new((seed + 2) as u32),
            biome_noise: Simplex::new((seed + 3) as u32),
            river_noise: Simplex::new((seed + 4) as u32),
        }
    }

    const SEA_LEVEL: i32 = 63;

    pub     fn get_biome(&self, wx: f64, wz: f64) -> Biome {
        let temp = self.biome_noise.get([wx * 0.0015, wz * 0.0015]);
        let humidity = self.biome_noise.get([wx * 0.002 + 1000.0, wz * 0.002 + 1000.0]);
        let continental = self.height_noise.get([wx * 0.001, wz * 0.001]);

        if temp > 0.3 {
            // Hot
            if humidity > 0.3 {
                Biome::Jungle
            } else if humidity > -0.2 {
                if continental > 0.0 { Biome::Forest } else { Biome::Savanna }
            } else {
                Biome::Desert
            }
        } else if temp > -0.1 {
            // Warm / Temperate
            if humidity > 0.5 {
                Biome::DarkForest
            } else if humidity > 0.2 {
                if continental > 0.2 { Biome::Forest } else { Biome::Swamp }
            } else if humidity > -0.3 {
                Biome::Forest
            } else {
                Biome::Plains
            }
        } else if temp > -0.4 {
            // Cool
            if humidity > 0.3 {
                Biome::DarkForest
            } else if humidity > 0.0 {
                Biome::Taiga
            } else {
                Biome::Plains
            }
        } else {
            // Cold
            if humidity > 0.0 {
                Biome::SnowyTundra
            } else {
                Biome::Mountains
            }
        }
    }

    fn get_height(&self, wx: f64, wz: f64) -> (i32, Biome) {
        let biome = self.get_biome(wx, wz);
        let river = self.river_noise.get([wx * 0.008, wz * 0.008]);

        let (base_height, amp, scale) = match biome {
            Biome::Plains => (66.0, 6.0, 0.015),
            Biome::Forest => (68.0, 12.0, 0.015),
            Biome::Desert => (68.0, 5.0, 0.012),
            Biome::Savanna => (67.0, 8.0, 0.013),
            Biome::Taiga => (69.0, 15.0, 0.018),
            Biome::SnowyTundra => (70.0, 4.0, 0.01),
            Biome::Mountains => (72.0, 35.0, 0.025),
            Biome::Swamp => (65.0, 3.0, 0.01),
            Biome::Jungle => (69.0, 18.0, 0.017),
            Biome::DarkForest => (68.0, 12.0, 0.015),
        };

        let h1 = self.height_noise.get([wx * scale, wz * scale]) * amp;
        let h2 = self.detail_noise.get([wx * scale * 2.5, wz * scale * 2.5]) * (amp * 0.3);

        let mut height = base_height + h1 + h2;

        // Rivers cut through terrain
                let river_strength = (river.abs() * 3.0 - 0.5).min(1.0).max(0.0);
        if river.abs() < 0.25 {
            height = height * (1.0 - river_strength) + (Self::SEA_LEVEL as f64 - 5.0) * river_strength;
        }

        let height_i = height as i32;
        (height_i, biome)
    }

    fn is_cave(&self, wx: f64, wy: f64, wz: f64) -> bool {
        // Legacy per-block cave check (kept for compatibility)
        let cave_scale = 0.025;
        let n = self.cave_noise.get([wx * cave_scale, wy * cave_scale * 0.6, wz * cave_scale]);
        (n > 0.40 || n > 0.35) && wy > 4.0 && wy < 55.0
    }

    /// Spaghetti-cave carver: creates winding tunnels through the chunk.
    /// Called once per chunk during generation, replaces stone with air along cave paths.
    pub fn carve_caves(&self, chunk: &mut Chunk, rng: &mut impl Rng) {
        let num_branches = 1 + (rng.gen::<f64>() * 3.0) as usize;
        for _ in 0..num_branches {
            let start_x = (rng.gen::<f64>() * CHUNK_SIZE as f64) as usize;
            let start_z = (rng.gen::<f64>() * CHUNK_SIZE as f64) as usize;
            let start_y = 8 + (rng.gen::<f64>() * 40.0) as usize;

            let mut cx = start_x as f64;
            let mut cy = start_y as f64;
            let mut cz = start_z as f64;
            let length = 10 + (rng.gen::<f64>() * 30.0) as usize;
            let base_radius = 1.5 + rng.gen::<f64>() * 2.5;
            let yaw = rng.gen::<f64>() * std::f64::consts::TAU;
            let pitch = (rng.gen::<f64>() - 0.5) * 0.5;

            for step in 0..length {
                let t = step as f64 / length as f64;
                let radius = base_radius * (1.0 - t * 0.5);
                let angle = yaw + t * 0.5;
                let vy = pitch + (t - 0.5) * 0.3;
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
                            if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                            if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 { continue; }
                            let dist = (dx as f64).powi(2) + (dy as f64).powi(2) + (dz as f64).powi(2);
                            if dist > radius * radius { continue; }
                            let block = chunk.get_block(bx as usize, by as usize, bz as usize);
                            if block.id == BlockId::Stone || block.id == BlockId::Deepslate
                                || block.id == BlockId::Dirt || block.id == BlockId::GrassBlock
                                || block.id == BlockId::SoulSand || block.id == BlockId::Gravel {
                                chunk.set_block(bx as usize, by as usize, bz as usize, Block::air());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Generate ore veins: blob-shaped 3D ellipsoid deposits replacing stone/deepslate.
    /// Called after terrain columns but before water fill so ores appear in correct stone type.
    pub fn generate_ores(&self, chunk: &mut Chunk, rng: &mut impl Rng) {
        let ores: &[(BlockId, BlockId, i32, i32, usize, f64)] = &[
            // (stone_ore, deepslate_ore, y_min, y_max, attempts, vein_radius)
            (BlockId::CoalOre,  BlockId::DeepslateCoalOre,   0, 128, 20, 3.0),
            (BlockId::IronOre,  BlockId::DeepslateIronOre,   0,  63, 15, 3.0),
            (BlockId::CopperOre, BlockId::DeepslateCopperOre, 0,  96, 12, 2.5),
            (BlockId::GoldOre,  BlockId::DeepslateGoldOre,   0,  32,  6, 2.5),
            (BlockId::RedstoneOre, BlockId::DeepslateRedstoneOre, 0, 16, 10, 2.0),
            (BlockId::LapisOre, BlockId::DeepslateLapisOre,  0,  32,  3, 2.0),
            (BlockId::DiamondOre, BlockId::DeepslateDiamondOre, 0, 16,  3, 2.0),
        ];

        for &(stone_ore, deepslate_ore, y_min, y_max, attempts, radius) in ores {
            for _ in 0..attempts {
                let ox = (rng.gen::<f64>() * CHUNK_SIZE as f64) as i32;
                let oz = (rng.gen::<f64>() * CHUNK_SIZE as f64) as i32;
                let oy = y_min + (rng.gen::<f64>() * (y_max - y_min).max(1) as f64) as i32;
                if oy < 1 || oy >= CHUNK_HEIGHT as i32 - 1 { continue; }

                let rx = radius * (0.8 + rng.gen::<f64>() * 0.4);
                let ry = radius * (0.6 + rng.gen::<f64>() * 0.4);
                let rz = radius * (0.8 + rng.gen::<f64>() * 0.4);

                let rx_i = rx.ceil() as i32;
                let ry_i = ry.ceil() as i32;
                let rz_i = rz.ceil() as i32;

                for dx in -rx_i..=rx_i {
                    for dy in -ry_i..=ry_i {
                        for dz in -rz_i..=rz_i {
                            let bx = ox + dx;
                            let by = oy + dy;
                            let bz = oz + dz;
                            if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                            if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 { continue; }
                            let dist = (dx as f64 / rx).powi(2) + (dy as f64 / ry).powi(2) + (dz as f64 / rz).powi(2);
                            if dist > 1.0 { continue; }
                            let block = chunk.get_block(bx as usize, by as usize, bz as usize);
                            if block.id == BlockId::Stone {
                                chunk.set_block(bx as usize, by as usize, bz as usize, Block::new(stone_ore));
                            } else if block.id == BlockId::Deepslate {
                                chunk.set_block(bx as usize, by as usize, bz as usize, Block::new(deepslate_ore));
                            }
                        }
                    }
                }
                // Also replace Granite, Diorite, Andesite with stone ore (not deepslate)
                for dx in -rx_i..=rx_i {
                    for dy in -ry_i..=ry_i {
                        for dz in -rz_i..=rz_i {
                            let bx = ox + dx;
                            let by = oy + dy;
                            let bz = oz + dz;
                            if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                            if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 { continue; }
                            let dist = (dx as f64 / rx).powi(2) + (dy as f64 / ry).powi(2) + (dz as f64 / rz).powi(2);
                            if dist > 1.0 { continue; }
                            let block = chunk.get_block(bx as usize, by as usize, bz as usize);
                            if (block.id == BlockId::Granite || block.id == BlockId::Diorite || block.id == BlockId::Andesite)
                                && stone_ore != BlockId::CoalOre // only coal spawns in granite/diorite/andesite
                            {
                                // Small chance for other ores in stone variants
                                if rng.gen_bool(0.3) {
                                    chunk.set_block(bx as usize, by as usize, bz as usize, Block::new(stone_ore));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Emeralds: only in Mountains biome, single blocks
        let wx = (chunk.cx as i64 * CHUNK_SIZE as i64) as f64 + 8.0;
        let wz = (chunk.cz as i64 * CHUNK_SIZE as i64) as f64 + 8.0;
        if matches!(self.get_biome(wx, wz), Biome::Mountains) {
            for _ in 0..4 {
                let ex = (rng.gen::<f64>() * CHUNK_SIZE as f64) as i32;
                let ez = (rng.gen::<f64>() * CHUNK_SIZE as f64) as i32;
                let ey = 4 + (rng.gen::<f64>() * 28.0) as i32;
                if ey < 1 || ey >= CHUNK_HEIGHT as i32 - 1 { continue; }
                let block = chunk.get_block(ex as usize, ey as usize, ez as usize);
                if block.id == BlockId::Stone {
                    chunk.set_block(ex as usize, ey as usize, ez as usize, Block::new(BlockId::EmeraldOre));
                } else if block.id == BlockId::Deepslate {
                    chunk.set_block(ex as usize, ey as usize, ez as usize, Block::new(BlockId::DeepslateEmeraldOre));
                }
            }
        }
    }

    pub fn generate_chunk(&self, chunk: &mut Chunk) {
        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;

        let mut rng = rand::thread_rng();

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;

                let (height, biome) = self.get_height(wx, wz);
                let surface_height = height.max(1).min(CHUNK_HEIGHT as i32 - 1);

                // Bedrock
                chunk.set_block(x, 0, z, Block::new(BlockId::Bedrock));

                // Determine surface and subsurface blocks based on biome
                let (surface_block, subsurface_block, deep_block) = match biome {
                    Biome::Desert => (BlockId::Sand, BlockId::Sandstone, BlockId::Sandstone),
                    Biome::Plains | Biome::Forest | Biome::Savanna | Biome::Jungle => {
                        (BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone)
                    }
                    Biome::Taiga => (BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
                    Biome::SnowyTundra => (BlockId::SnowBlock, BlockId::Dirt, BlockId::Stone),
                    Biome::Mountains => (BlockId::Stone, BlockId::Stone, BlockId::Stone),
                    Biome::Swamp => (BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
                    Biome::DarkForest => (BlockId::Podzol, BlockId::Dirt, BlockId::Stone),
                };

                // Generate column
        for y in 1..surface_height {
                    let y_usize = y as usize;

                    // Caves
                    if self.is_cave(wx, y as f64, wz) && y > 3 && y < surface_height - 2 {
                        chunk.set_block(x, y_usize, z, Block::air());
                        continue;
                    }

                    // Stone variant: biomes with a specific deep_block (e.g. Desert→Sandstone)
                    // use it instead of the generic noise-based stone.
                    // Smooth deepslate transition: gradual blend from y=0 to y=16
                    let stone_type = if deep_block != BlockId::Stone {
                        deep_block
                    } else if y < 16 {
                        let deepslate_chance = 1.0 - (y as f64 / 16.0);
                        let deep_noise = self.detail_noise.get([wx * 0.05 + 100.0, wz * 0.05 + 100.0]);
                        if deep_noise < deepslate_chance * 2.0 - 1.0 {
                            BlockId::Deepslate
                        } else {
                            let variant_noise = self.detail_noise.get([wx * 0.03, wz * 0.03]);
                            if variant_noise > 0.4 {
                                BlockId::Granite
                            } else if variant_noise < -0.4 {
                                BlockId::Andesite
                            } else {
                                BlockId::Stone
                            }
                        }
                    } else {
                        let variant_noise = self.detail_noise.get([wx * 0.03, wz * 0.03]);
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

                // Water fill below sea level
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

                // Beach sand — grass at shoreline converts to sand
                let surface_idx = (surface_height - 1).max(0) as usize;
                if surface_height <= Self::SEA_LEVEL + 2 && surface_height >= Self::SEA_LEVEL - 1 && biome != Biome::Desert {
                    if chunk.get_block(x, surface_idx, z).id == BlockId::GrassBlock {
                        let near_water = [(-1i32,0i32),(1,0),(0,-1),(0,1),(2,0),(-2,0),(0,2),(0,-2)].iter().any(|(dx, dz)| {
                            let nx = x as i32 + dx;
                            let nz = z as i32 + dz;
                            if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                                for wy in Self::SEA_LEVEL.saturating_sub(2)..=Self::SEA_LEVEL {
                                    let b = chunk.get_block(nx as usize, wy as usize, nz as usize);
                                    if b.id == BlockId::Water { return true; }
                                }
                            }
                            false
                        });
                        if near_water {
                            chunk.set_block(x, surface_idx, z, Block::new(BlockId::Sand));
                        }
                    }
                }
                // Sand/dirt mix just below water level — only within 3 blocks of a water column
                if surface_height >= Self::SEA_LEVEL - 2 && surface_height <= Self::SEA_LEVEL + 3 {
                    let near_water = [(-1i32,0i32),(1,0),(0,-1),(0,1)].iter().any(|(dx, dz)| {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                            for wy in (Self::SEA_LEVEL - 1)..=Self::SEA_LEVEL {
                                let b = chunk.get_block(nx as usize, wy as usize, nz as usize);
                                if b.id == BlockId::Water { return true; }
                            }
                        }
                        false
                    });
                    if near_water {
                        for by in (Self::SEA_LEVEL - 2).max(1)..=(surface_height - 1).min(Self::SEA_LEVEL + 1) {
                            let block = chunk.get_block(x, by as usize, z);
                            if block.id == BlockId::Dirt && rng.gen_bool(0.35) {
                                chunk.set_block(x, by as usize, z, Block::new(BlockId::Sand));
                            }
                        }
                    }
                }

                // Snow on top in cold biomes
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
        }

        // Ore veins: blob-shaped 3D ellipsoid deposits
        self.generate_ores(chunk, &mut rng);

        // Aquifers: replace stone with water in blobs (placed BEFORE caves so caves carve around the water pocket)
        if rng.gen_bool(0.35) {
            let ax = (rng.gen::<f64>() * CHUNK_SIZE as f64) as i32;
            let az = (rng.gen::<f64>() * CHUNK_SIZE as f64) as i32;
            let ay = 12 + (rng.gen::<f64>() * 38.0) as i32;
            let radius = 2.0 + rng.gen::<f64>() * 3.0;
            let water_level = ay + (radius * 0.4) as i32;

            for dx in -(radius as i32)..=radius as i32 {
                for dy in -(radius as i32)..=radius as i32 {
                    for dz in -(radius as i32)..=radius as i32 {
                        let bx = ax + dx;
                        let by = ay + dy;
                        let bz = az + dz;
                        if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                        if by < 1 || by >= CHUNK_HEIGHT as i32 - 1 { continue; }
                        let dist = (dx as f64).powi(2) + (dy as f64).powi(2) + (dz as f64).powi(2);
                        if dist > radius * radius { continue; }
                        let block = chunk.get_block(bx as usize, by as usize, bz as usize);
                        if (block.id == BlockId::Stone || block.id == BlockId::Deepslate)
                            && by <= water_level {
                            chunk.set_block(bx as usize, by as usize, bz as usize, Block::new(BlockId::Water));
                        }
                    }
                }
            }
        }

        // Spaghetti cave carver — runs AFTER aquifers so caves carve around water pockets
        if rng.gen_bool(0.7) {
            self.carve_caves(chunk, &mut rng);
        }

        // River channel carving
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let river = self.river_noise.get([wx * 0.008, wz * 0.008]);
                if river.abs() >= 0.25 { continue; }

                // Find current surface height (after terrain gen)
                let mut surface_h = 0i32;
                for y in (1..CHUNK_HEIGHT).rev() {
                    if !chunk.get_block(x, y, z).is_air() {
                        surface_h = y as i32;
                        break;
                    }
                }
                if surface_h < 2 { continue; }

                let river_strength = 1.0 - river.abs() / 0.25;
                let channel_depth = (river_strength * 3.0).ceil() as i32;

                for dy in 1..=channel_depth.min(surface_h - 1) {
                    chunk.set_block(x, (surface_h - dy) as usize, z, Block::air());
                }

                // Water fill in river channel up to sea level
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

        // Surface decorations (flowers, tall grass)
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let surface_h = height.max(1).min(CHUNK_HEIGHT as i32 - 1);

                if surface_h < Self::SEA_LEVEL || surface_h >= CHUNK_HEIGHT as i32 - 1 {
                    continue;
                }

                let top_block = chunk.get_block(x, surface_h as usize, z);
                let above_block = chunk.get_block(x, surface_h as usize + 1, z);
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

                if rng.gen_bool(flower_chance) {
                    let flower = match biome {
                        Biome::Plains => {
                            let f = rng.gen_range(0..100);
                            if f < 30 { BlockId::Dandelion }
                            else if f < 55 { BlockId::Poppy }
                            else if f < 70 { BlockId::OxeyeDaisy }
                            else if f < 85 { BlockId::Cornflower }
                            else { BlockId::AzureBluet }
                        }
                        Biome::Forest => {
                            if rng.gen_bool(0.5) { BlockId::Dandelion } else { BlockId::Poppy }
                        }
                        Biome::Swamp => BlockId::BlueOrchid,
                        Biome::Jungle => BlockId::Dandelion,
                        Biome::Taiga => BlockId::Poppy,
                        Biome::Savanna => BlockId::Dandelion,
                        _ => BlockId::Dandelion,
                    };
                    chunk.set_block(x, surface_h as usize + 1, z, Block::new(flower));
                } else if rng.gen_bool(grass_chance) {
                    let is_fern = matches!(biome, Biome::Taiga | Biome::Jungle) && rng.gen_bool(0.5);
                    chunk.set_block(x, surface_h as usize + 1, z, Block::new(if is_fern { BlockId::Fern } else { BlockId::Grass }));
                }
            }
        }

        // Trees (biome-specific density)
        let center_wx = (base_x + 8i64) as f64;
        let center_wz = (base_z + 8i64) as f64;
        let (_center_h, center_biome) = self.get_height(center_wx, center_wz);
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
            if rng.gen_bool(tree_density) {
                let tx = rng.gen_range(2..CHUNK_SIZE - 2);
                let tz = rng.gen_range(2..CHUNK_SIZE - 2);
                let wx = (base_x + tx as i64) as f64;
                let wz = (base_z + tz as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);

                // Check surface block is solid (not air/leaves/water)
                let surface_is_solid = if height > 0 && height < CHUNK_HEIGHT as i32 {
                    let block = chunk.get_block(tx, height as usize - 1, tz);
                    !block.is_air() && block.id != BlockId::Water
                } else { false };

                // Don't place trees within 5 blocks of another tree
                let too_close = tree_positions.iter().any(|(ptx, ptz)| {
                    (tx as i32 - *ptx).abs() < 5 && (tz as i32 - *ptz).abs() < 5
                });

                if height > Self::SEA_LEVEL && height < 130 && surface_is_solid && !too_close {
                    let ground = height - 1; // actual surface block y
                    tree_positions.push((tx as i32, tz as i32));
                    match biome {
                        Biome::Forest | Biome::Plains | Biome::Swamp => {
                            let tree_roll = rng.gen::<f64>();
                            if tree_roll < 0.35 {
                                self.place_birch_tree(chunk, tx, tz, ground, &mut rng);
                            } else {
                                self.place_tree(chunk, tx, tz, ground, BlockId::OakLog, BlockId::OakLeaves, &mut rng);
                            }
                        }
                        Biome::Taiga => {
                            self.place_spruce_tree(chunk, tx, tz, ground, &mut rng);
                        }
                        Biome::Jungle => {
                            self.place_jungle_tree(chunk, tx, tz, ground, &mut rng);
                        }
                        Biome::Savanna => {
                            self.place_tree(chunk, tx, tz, ground, BlockId::AcaciaLog, BlockId::AcaciaLeaves, &mut rng);
                        }
                        Biome::DarkForest => {
                            self.place_dark_oak_tree(chunk, tx, tz, ground, &mut rng);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Fallen trees (sideways logs on forest floor)
        if matches!(center_biome, Biome::Forest | Biome::Taiga | Biome::Jungle | Biome::DarkForest) {
            for _ in 0..2 {
                if !rng.gen_bool(0.3) { continue; }
                let fx = rng.gen_range(2..CHUNK_SIZE - 3);
                let fz = rng.gen_range(2..CHUNK_SIZE - 3);
                let wx = (base_x + fx as i64) as f64;
                let wz = (base_z + fz as i64) as f64;
                let (fh, _fb) = self.get_height(wx, wz);
                let ground = fh - 1;
                if ground < 2 || ground >= CHUNK_HEIGHT as i32 - 2 { continue; }
                let log_len = 3 + (rng.gen::<f64>().abs() * 3.0) as usize;
                let axis = rng.gen_bool(0.5); // true = X axis, false = Z axis
                let log_id = match center_biome {
                    Biome::Jungle => BlockId::JungleLog,
                    Biome::Taiga => BlockId::SpruceLog,
                    Biome::DarkForest => BlockId::DarkOakLog,
                    _ => BlockId::OakLog,
                };
                for li in 0..log_len {
                    let lx = if axis { fx + li } else { fx };
                    let lz = if axis { fz } else { fz + li };
                    if lx >= CHUNK_SIZE || lz >= CHUNK_SIZE { break; }
                    let above = chunk.get_block(lx, ground as usize + 1, lz);
                    if above.is_air() {
                        chunk.set_block(lx, ground as usize + 1, lz, Block::new(log_id));
                    }
                }
            }
        }

        // Vines hanging from leaves in jungle and swamp
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (_height, v_biome) = self.get_height(wx, wz);
                if !matches!(v_biome, Biome::Jungle | Biome::Swamp | Biome::DarkForest) { continue; }
                for y in (2..CHUNK_HEIGHT - 1).rev() {
                    let block = chunk.get_block(x, y, z);
                    if block.id == BlockId::JungleLeaves || block.id == BlockId::OakLeaves || block.id == BlockId::DarkOakLeaves {
                        // Find a side that faces air (edge of canopy)
                        let has_air_side = [(-1i32,0i32),(1,0),(0,-1),(0,1)].iter().any(|(dx, dz)| {
                            let nx = x as i32 + dx;
                            let nz = z as i32 + dz;
                            if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                                chunk.get_block(nx as usize, y, nz as usize).is_air()
                            } else { false }
                        });
                        if !has_air_side { continue; }
                        // Place vines below for 2-4 blocks
                        let vine_len = 2 + (rng.gen::<f64>().abs() * 3.0) as usize;
                        for dy in 1..=vine_len {
                            let vy = y.saturating_sub(dy);
                            if vy < 1 || vy >= CHUNK_HEIGHT { break; }
                            let below = chunk.get_block(x, vy, z);
                            if below.is_air() {
                                chunk.set_block(x, vy, z, Block::new(BlockId::Vine));
                            } else { break; }
                        }
                        break;
                    }
                }
            }
        }

        // Cacti in deserts
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                if biome != Biome::Desert || height < Self::SEA_LEVEL || height >= CHUNK_HEIGHT as i32 - 3 {
                    continue;
                }
                if chunk.get_block(x, height as usize, z).id != BlockId::Sand { continue; }
                if !chunk.get_block(x, height as usize + 1, z).is_air() { continue; }
                if !rng.gen_bool(0.02) { continue; }

                let cactus_h = 1 + (rng.gen::<f64>().abs() * 2.0) as usize;
                for dy in 1..=cactus_h {
                    let cy = height as usize + dy;
                    if cy >= CHUNK_HEIGHT { break; }
                    // Check surrounding blocks are air
                    let mut blocked = false;
                    for (dx, dz) in &[(-1i32,0i32),(1,0),(0,-1),(0,1)] {
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

        // Sugar cane directly adjacent to water in warm biomes
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                let warm = matches!(biome, Biome::Desert | Biome::Savanna | Biome::Jungle | Biome::Plains);
                if !warm || height < Self::SEA_LEVEL || height >= CHUNK_HEIGHT as i32 - 3 { continue; }
                let surface_y = (height - 1).max(0) as usize;
                let top = chunk.get_block(x, surface_y, z);
                if top.id != BlockId::Sand && top.id != BlockId::GrassBlock { continue; }
                if !chunk.get_block(x, surface_y + 1, z).is_air() { continue; }

                // Must be directly next to water (immediate neighbor, at water level)
                let has_water = [(-1i32,0i32),(1,0),(0,-1),(0,1)].iter().any(|(dx, dz)| {
                    let nx = x as i32 + dx;
                    let nz = z as i32 + dz;
                    if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                        chunk.get_block(nx as usize, surface_y, nz as usize).id == BlockId::Water
                    } else { false }
                });
                if !has_water || !rng.gen_bool(0.08) { continue; }

                let cane_h = 1 + (rng.gen::<f64>().abs() * 2.0) as usize;
                for dy in 1..=cane_h {
                    let cy = surface_y + dy;
                    if cy >= CHUNK_HEIGHT { break; }
                    if !chunk.get_block(x, cy, z).is_air() { break; }
                    chunk.set_block(x, cy, z, Block::new(BlockId::SugarCane));
                }
            }
        }

        // Mushrooms in swamp and dark forest
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (mh, mb) = self.get_height(wx, wz);
                if !matches!(mb, Biome::Swamp | Biome::DarkForest) || mh < Self::SEA_LEVEL || mh >= CHUNK_HEIGHT as i32 - 1 { continue; }
                let top = chunk.get_block(x, (mh - 1).max(0) as usize, z);
                let is_dark_forest = mb == Biome::DarkForest;
                let surface_allowed = if is_dark_forest {
                    top.id == BlockId::Podzol || top.id == BlockId::GrassBlock || top.id == BlockId::Dirt
                } else {
                    top.id == BlockId::GrassBlock
                };
                if !surface_allowed { continue; }
                if !chunk.get_block(x, mh as usize, z).is_air() { continue; }
                let mush_chance = if is_dark_forest { 0.15 } else { 0.04 };
                let giant_chance = if is_dark_forest { 0.10 } else { 0.02 };
                if rng.gen_bool(mush_chance) {
                    let mush = if rng.gen_bool(0.6) { BlockId::BrownMushroom } else { BlockId::RedMushroom };
                    chunk.set_block(x, mh as usize, z, Block::new(mush));
                } else if rng.gen_bool(giant_chance) {
                    self.place_giant_mushroom(chunk, x, z, mh, rng.gen_bool(0.5), &mut rng);
                }
            }
        }

        // Dead bushes in deserts
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (height, biome) = self.get_height(wx, wz);
                if biome != Biome::Desert || height < Self::SEA_LEVEL || height >= CHUNK_HEIGHT as i32 - 1 { continue; }
                let top = chunk.get_block(x, height as usize, z);
                if top.id != BlockId::Sand { continue; }
                if !chunk.get_block(x, height as usize + 1, z).is_air() { continue; }
                if rng.gen_bool(0.03) {
                    chunk.set_block(x, height as usize + 1, z, Block::new(BlockId::DeadBush));
                }
            }
        }

        // Lily pads in swamp
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (base_x + x as i64) as f64;
                let wz = (base_z + z as i64) as f64;
                let (_height, biome) = self.get_height(wx, wz);
                if biome != Biome::Swamp { continue; }
                // Find water surface
                for y in (1..CHUNK_HEIGHT - 1).rev() {
                    let block = chunk.get_block(x, y, z);
                    if block.id == BlockId::Water && chunk.get_block(x, y + 1, z).is_air() {
                        if rng.gen_bool(0.1) {
                            chunk.set_block(x, y + 1, z, Block::new(BlockId::LilyPad));
                        }
                        break;
                    }
                }
            }
        }

        // Pumpkins in plains
        for _ in 0..3 {
            let tx = rng.gen_range(1..CHUNK_SIZE - 1);
            let tz = rng.gen_range(1..CHUNK_SIZE - 1);
            let wx = (base_x + tx as i64) as f64;
            let wz = (base_z + tz as i64) as f64;
            let (height, biome) = self.get_height(wx, wz);
            if biome != Biome::Plains || height < Self::SEA_LEVEL || height >= CHUNK_HEIGHT as i32 - 1 { continue; }
            if chunk.get_block(tx, height as usize, tz).id != BlockId::GrassBlock { continue; }
            if !chunk.get_block(tx, height as usize + 1, tz).is_air() { continue; }
            // Check neighbors are air
            let clear = [(-1i32,0i32),(1,0),(0,-1),(0,1)].iter().all(|(dx, dz)| {
                let nx = tx as i32 + dx;
                let nz = tz as i32 + dz;
                nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32
                    && chunk.get_block(nx as usize, height as usize + 1, nz as usize).is_air()
            });
            if clear && rng.gen_bool(0.5) {
                chunk.set_block(tx, height as usize + 1, tz, Block::new(BlockId::Pumpkin));
            }
        }

        // Melons in jungle
        for _ in 0..3 {
            let tx = rng.gen_range(1..CHUNK_SIZE - 1);
            let tz = rng.gen_range(1..CHUNK_SIZE - 1);
            let wx = (base_x + tx as i64) as f64;
            let wz = (base_z + tz as i64) as f64;
            let (height, biome) = self.get_height(wx, wz);
            if biome != Biome::Jungle || height < Self::SEA_LEVEL || height >= CHUNK_HEIGHT as i32 - 1 { continue; }
            if chunk.get_block(tx, height as usize, tz).id != BlockId::GrassBlock { continue; }
            if !chunk.get_block(tx, height as usize + 1, tz).is_air() { continue; }
            let clear = [(-1i32,0i32),(1,0),(0,-1),(0,1)].iter().all(|(dx, dz)| {
                let nx = tx as i32 + dx;
                let nz = tz as i32 + dz;
                nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32
                    && chunk.get_block(nx as usize, height as usize + 1, nz as usize).is_air()
            });
            if clear && rng.gen_bool(0.5) {
                chunk.set_block(tx, height as usize + 1, tz, Block::new(BlockId::Melon));
            }
        }

        // Surface lava pools
        if matches!(center_biome, Biome::Plains | Biome::Forest | Biome::Savanna) && rng.gen_bool(0.005) {
            let lx = rng.gen_range(2..CHUNK_SIZE - 2);
            let lz = rng.gen_range(2..CHUNK_SIZE - 2);
            let wx = (base_x + lx as i64) as f64;
            let wz = (base_z + lz as i64) as f64;
            let (lh, _lb) = self.get_height(wx, wz);
            if lh > Self::SEA_LEVEL + 1 && lh < CHUNK_HEIGHT as i32 - 2 {
                let gy = (lh - 1).max(0) as usize;
                let pool_radius = 1 + (rng.gen::<f64>() * 1.5) as usize;
                for dx in -(pool_radius as i32)..=pool_radius as i32 {
                    for dz in -(pool_radius as i32)..=pool_radius as i32 {
                        let bx = lx as i32 + dx;
                        let bz = lz as i32 + dz;
                        if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                        if dx.abs() == pool_radius as i32 && dz.abs() == pool_radius as i32 && rng.gen_bool(0.5) { continue; }
                        let dist = (dx as f64).powi(2) + (dz as f64).powi(2);
                        if dist > (pool_radius as f64 + 0.5).powi(2) { continue; }
                        // Replace surface block with stone/air
                        if chunk.get_block(bx as usize, gy, bz as usize).id == BlockId::GrassBlock {
                            chunk.set_block(bx as usize, gy, bz as usize, Block::new(BlockId::Stone));
                            // Lava at center, stone ring at edges
                            let is_lava = dx.abs() <= pool_radius as i32 - 1 && dz.abs() <= pool_radius as i32 - 1;
                            if is_lava {
                                chunk.set_block(bx as usize, gy + 1, bz as usize, Block::new(BlockId::Lava));
                            }
                        }
                    }
                }
            }
        }

        // Desert wells
        if center_biome == Biome::Desert && rng.gen_bool(0.003) {
            for x in 2..CHUNK_SIZE - 3 {
                for z in 2..CHUNK_SIZE - 3 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (wh, _wb) = self.get_height(wx, wz);
                    if wh > Self::SEA_LEVEL && wh < CHUNK_HEIGHT as i32 - 3 {
                        self.place_desert_well(chunk, x, z, wh, &mut rng);
                        break;
                    }
                }
                break;
            }
        }

        // Igloos in snowy biomes
        if matches!(center_biome, Biome::SnowyTundra | Biome::Taiga) && rng.gen_bool(0.001) {
            for x in 3..CHUNK_SIZE - 4 {
                for z in 3..CHUNK_SIZE - 4 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (ih, _ib) = self.get_height(wx, wz);
                    if ih > Self::SEA_LEVEL + 2 && ih < CHUNK_HEIGHT as i32 - 5 {
                        if chunk.get_block(x, (ih - 1).max(0) as usize, z).id == BlockId::SnowBlock
                            || chunk.get_block(x, (ih - 1).max(0) as usize, z).id == BlockId::GrassBlock {
                            self.place_igloo(chunk, x, z, ih, &mut rng);
                            break;
                        }
                    }
                }
                break;
            }
        }

        // Swamp huts
        if center_biome == Biome::Swamp && rng.gen_bool(0.002) {
            for x in 3..CHUNK_SIZE - 4 {
                for z in 3..CHUNK_SIZE - 4 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (sh, _sb) = self.get_height(wx, wz);
                    if sh >= Self::SEA_LEVEL && sh < CHUNK_HEIGHT as i32 - 4 {
                        self.place_swamp_hut(chunk, x, z, sh, &mut rng);
                        break;
                    }
                }
                break;
            }
        }

        // Ocean ruins (near coast or underwater)
        if rng.gen_bool(0.001) {
            for x in 2..CHUNK_SIZE - 3 {
                for z in 2..CHUNK_SIZE - 3 {
                    let wx = (base_x + x as i64) as f64;
                    let wz = (base_z + z as i64) as f64;
                    let (rh, _rb) = self.get_height(wx, wz);
                    if (rh >= Self::SEA_LEVEL - 3 && rh <= Self::SEA_LEVEL + 2) && rh > 1 {
                        self.place_ocean_ruin(chunk, x, z, rh, &mut rng);
                        break;
                    }
                }
                break;
            }
        }

        // Dungeons: small cobblestone rooms with spawner + chests
        if rng.gen_bool(0.005) {
            self.place_dungeon(chunk, &mut rng);
        }

        // Ruined portals: broken obsidian portal frames on surface
        if rng.gen_bool(0.002) {
            self.place_ruined_portal(chunk, &mut rng);
        }

        chunk.recount_fluids();
        chunk.is_dirty = true;
    }

    fn place_tree(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, log: BlockId, leaves: BlockId, _rng: &mut impl Rng) {
        let trunk_height = 5 + (_rng.gen::<f64>().abs() * 2.0) as i32;
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
            let radius = if dy == 0 { 2 } else if dy == 1 { 2 } else { 1 };
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let lx = tx as i32 + dx;
                    let lz = tz as i32 + dz;
                    let ly = leaf_start + dy;
                    if lx >= 0 && lx < CHUNK_SIZE as i32 && lz >= 0 && lz < CHUNK_SIZE as i32
                        && ly > 0 && ly < CHUNK_HEIGHT as i32
                    {
                        if dx.abs() == radius && dz.abs() == radius && dy > 0 { continue; }
                        if chunk.get_block(lx as usize, ly as usize, lz as usize).is_air() {
                            chunk.set_block(lx as usize, ly as usize, lz as usize, Block::new(leaves));
                        }
                    }
                }
            }
        }
    }

    fn place_spruce_tree(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let trunk_height = 6 + (_rng.gen::<f64>().abs() * 4.0) as i32;
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
                    if lx >= 0 && lx < CHUNK_SIZE as i32 && lz >= 0 && lz < CHUNK_SIZE as i32
                        && ly > 0 && ly < CHUNK_HEIGHT as i32
                    {
                        if dx.abs() == radius && dz.abs() == radius { continue; }
                        if chunk.get_block(lx as usize, ly as usize, lz as usize).is_air() {
                            chunk.set_block(lx as usize, ly as usize, lz as usize, Block::new(BlockId::SpruceLeaves));
                        }
                    }
                }
            }
        }
    }

    fn place_birch_tree(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let trunk_height = 5 + (_rng.gen::<f64>().abs() * 2.0) as i32;
        for ty in 1..=trunk_height {
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
                    if lx >= 0 && lx < CHUNK_SIZE as i32 && lz >= 0 && lz < CHUNK_SIZE as i32
                        && ly > 0 && ly < CHUNK_HEIGHT as i32
                    {
                        let d = dx.abs().max(dz.abs());
                        let in_range = if dy <= 0 { d <= 2 } else { d <= 1 };
                        if in_range && chunk.get_block(lx as usize, ly as usize, lz as usize).is_air() {
                            chunk.set_block(lx as usize, ly as usize, lz as usize, Block::new(BlockId::BirchLeaves));
                        }
                    }
                }
            }
        }
    }

    fn place_dungeon(&self, chunk: &mut Chunk, rng: &mut impl Rng) {
        let dx = rng.gen_range(2..CHUNK_SIZE - 6);
        let dz = rng.gen_range(2..CHUNK_SIZE - 6);
        let dy = 10 + (rng.gen::<f64>() * 45.0) as usize;
        if dy >= CHUNK_HEIGHT - 5 { return; }

        // Check the area is mostly stone (not already carved into cave)
        let mut solid_count = 0;
        for x in dx..=dx + 6 {
            for z in dz..=dz + 6 {
                for y in dy..=dy + 5 {
                    let b = chunk.get_block(x, y, z);
                    if b.id == BlockId::Stone || b.id == BlockId::Deepslate
                        || b.id == BlockId::Dirt || b.id == BlockId::Gravel {
                        solid_count += 1;
                    }
                }
            }
        }
        // Need at least 60% solid blocks to place dungeon here
        if solid_count < 180 { return; }

        // Carve out room: interior 5x4x5 (x,z,y)
        // Walls are cobblestone/mossy
        for x in dx..=dx + 6 {
            for z in dz..=dz + 6 {
                for y in dy..=dy + 5 {
                    let is_wall = x == dx || x == dx + 6 || z == dz || z == dz + 6
                        || y == dy || y == dy + 5;
                    if is_wall {
                        let wall_block = if rng.gen_bool(0.3) {
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
        let num_chests = 1 + (rng.gen::<f64>() * 1.5) as usize;
        for _ in 0..num_chests {
            let (cx2, cz2) = match rng.gen_range(0..4) {
                0 => (dx + 1, dz + 1 + rng.gen_range(0..4)),
                1 => (dx + 5, dz + 1 + rng.gen_range(0..4)),
                2 => (dx + 1 + rng.gen_range(0..4), dz + 1),
                _ => (dx + 1 + rng.gen_range(0..4), dz + 5),
            };
            let existing = chunk.get_block(cx2, dy + 1, cz2);
            if existing.is_air() {
                chunk.set_block(cx2, dy + 1, cz2, Block::new(BlockId::Chest));
            }
        }
    }

    fn place_ruined_portal(&self, chunk: &mut Chunk, rng: &mut impl Rng) {
        let px = rng.gen_range(2..CHUNK_SIZE - 6);
        let pz = rng.gen_range(2..CHUNK_SIZE - 6);
        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;
        let wx = (base_x + px as i64) as f64;
        let wz = (base_z + pz as i64) as f64;
        let (height, _biome) = self.get_height(wx, wz);
        if height < Self::SEA_LEVEL - 2 || height >= CHUNK_HEIGHT as i32 - 7 { return; }
        // Don't place in water
        let ground_block = chunk.get_block(px, height as usize - 1, pz);
        if ground_block.id == BlockId::Water { return; }

        // Portal frame: 4 wide (x) x 5 tall (y), some blocks missing
        let frame_height = 5;
        let frame_width = 4;
        let gy = (height - 1).max(0) as usize;
        let mut missing = 2 + (rng.gen::<f64>() * 4.0) as usize; // 2-5 broken blocks

        for y in 0..frame_height {
            for x in 0..frame_width {
                let bx = px + x;
                let by = gy + y;
                if bx >= CHUNK_SIZE || by >= CHUNK_HEIGHT { continue; }
                let is_corner = (x == 0 || x == frame_width - 1) || y == 0 || y == frame_height - 1;
                let is_frame = x == 0 || x == frame_width - 1 || y == 0 || y == frame_height - 1;
                let is_top_corner = (x == 0 || x == frame_width - 1) && y == frame_height - 1;

                // Top corners don't exist in the frame (nether portal is 4W x 5H with open top middle)
                if is_top_corner { continue; }

                if !is_frame { continue; } // interior is open

                if missing > 0 && !is_corner {
                    missing -= 1;
                    continue; // missing block
                }

                let block = if y == 0 {
                    // Base: mix of stone bricks and obsidian
                    if rng.gen_bool(0.3) { BlockId::Obsidian } else { BlockId::StoneBricks }
                } else {
                    // Add some crying obsidian in frame
                    if rng.gen_bool(0.1) { BlockId::CryingObsidian } else { BlockId::Obsidian }
                };
                chunk.set_block(bx, by, pz, Block::new(block));
            }
        }

        // Stone bricks around the base
        for dx in -2i32..=5i32 {
            for dz in -2i32..=2i32 {
                let bx = px as i32 + dx;
                let bz = pz as i32 + dz;
                if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                let by = gy;
                let existing = chunk.get_block(bx as usize, by, bz as usize);
                if existing.id == BlockId::Stone || existing.id == BlockId::Dirt || existing.id == BlockId::GrassBlock {
                    if dx < 0 || dx > 3 || dz < -1 || dz > 1 {
                        if rng.gen_bool(0.3) {
                            chunk.set_block(bx as usize, by, bz as usize, Block::new(BlockId::StoneBricks));
                        }
                    }
                }
            }
        }
        // Some lichen/vines
        if rng.gen_bool(0.4) {
            for _ in 0..3 {
                let bx = px + rng.gen_range(0..frame_width);
                let by = gy + (rng.gen::<f64>() * frame_height as f64) as usize;
                let bz = pz;
                if bx < CHUNK_SIZE && by < CHUNK_HEIGHT {
                    let existing = chunk.get_block(bx, by, bz);
                    if existing.id == BlockId::Obsidian || existing.id == BlockId::CryingObsidian {
                        for (nx, nz) in &[(-1i32,0i32),(1,0),(0,-1),(0,1)] {
                            let ax = bx as i32 + nx;
                            let az = pz as i32 + nz;
                            if ax >= 0 && ax < CHUNK_SIZE as i32 && az >= 0 && az < CHUNK_SIZE as i32
                                && chunk.get_block(ax as usize, by, az as usize).is_air() {
                                if rng.gen_bool(0.5) {
                                    chunk.set_block(ax as usize, by, az as usize, Block::new(BlockId::Vine));
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn place_giant_mushroom(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, is_red: bool, _rng: &mut impl Rng) {
        let stem_height = 2 + (_rng.gen::<f64>().abs() * 2.0) as i32;
        let cap_id = if is_red { BlockId::RedMushroomBlock } else { BlockId::BrownMushroomBlock };

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
                    if lx < 0 || lx >= CHUNK_SIZE as i32 || lz < 0 || lz >= CHUNK_SIZE as i32 { continue; }
                    if ly < 1 || ly >= CHUNK_HEIGHT as i32 { continue; }
                    let d = dx.abs().max(dz.abs());
                    let in_range = match dy {
                        0 => d <= 2 && d > 0, // bottom ring (don't cover stem top)
                        1 => d <= 2,
                        2 => d <= 1,
                        _ => false,
                    };
                    if in_range && chunk.get_block(lx as usize, ly as usize, lz as usize).is_air() {
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

    fn place_jungle_tree(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let trunk_height = 7 + (_rng.gen::<f64>().abs() * 5.0) as i32;
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
                    if lx >= 0 && lx < CHUNK_SIZE as i32 && lz >= 0 && lz < CHUNK_SIZE as i32
                        && ly > 0 && ly < CHUNK_HEIGHT as i32
                    {
                        let d = dx.abs().max(dz.abs());
                        let in_range = if dy <= 0 { d <= 3 } else { d <= 2 };
                        if in_range && chunk.get_block(lx as usize, ly as usize, lz as usize).is_air() {
                            chunk.set_block(lx as usize, ly as usize, lz as usize, Block::new(BlockId::JungleLeaves));
                        }
                    }
                }
            }
        }
    }

    fn place_dark_oak_tree(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        // Dark oak: 2x2 trunk, large rounded canopy
        let trunk_height = 6 + (_rng.gen::<f64>().abs() * 2.0) as i32;
        // 2x2 trunk
        for ty in 1..=trunk_height {
            for dx in 0..2 {
                for dz in 0..2 {
                    let bx = tx as i32 + dx;
                    let bz = tz as i32 + dz;
                    let by = height + ty;
                    if bx >= 0 && bx < CHUNK_SIZE as i32 && bz >= 0 && bz < CHUNK_SIZE as i32
                        && by < CHUNK_HEIGHT as i32
                    {
                        chunk.set_block(bx as usize, by as usize, bz as usize, Block::new(BlockId::DarkOakLog));
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
                        if d > radius { continue; }
                        if d == radius && dy > 0 && dy < 4 && _rng.gen_bool(0.5) { continue; }
                        if chunk.get_block(lx as usize, ly as usize, lz as usize).is_air() {
                            chunk.set_block(lx as usize, ly as usize, lz as usize, Block::new(BlockId::DarkOakLeaves));
                        }
                    }
                }
            }
        }
    }

    fn place_desert_well(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let gy = (height - 1).max(0) as usize;
        if chunk.get_block(tx, gy, tz).id != BlockId::Sand { return; }
        // 2x2 water pool at ground level
        for dx in 0..2 {
            for dz in 0..2 {
                let bx = tx + dx;
                let bz = tz + dz;
                if bx < CHUNK_SIZE && bz < CHUNK_SIZE {
                    chunk.set_block(bx, gy + 1, bz, Block::new(BlockId::Water));
                    // Stone brick rim
                    for (rx, rz) in &[(-1i32,-1i32),(-1,0),(-1,1),(-1,2),(2,-1),(2,0),(2,1),(2,2),
                                       (0,-1),(1,-1),(0,2),(1,2)] {
                        let wx = tx as i32 + rx;
                        let wz = tz as i32 + rz;
                        if wx >= 0 && wx < CHUNK_SIZE as i32 && wz >= 0 && wz < CHUNK_SIZE as i32 {
                            let above = chunk.get_block(wx as usize, gy + 1, wz as usize);
                            if above.is_air() || above.id == BlockId::Sand {
                                chunk.set_block(wx as usize, gy + 1, wz as usize, Block::new(BlockId::StoneBricks));
                            }
                        }
                    }
                }
            }
        }
    }

    fn place_igloo(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let gy = (height - 1).max(0) as usize;
        if chunk.get_block(tx, gy, tz).id != BlockId::GrassBlock
            && chunk.get_block(tx, gy, tz).id != BlockId::SnowBlock { return; }
        // Snow floor
        let floor_y = gy + 1;
        // Small snow dome: 5x5 base, 3x3 middle, 1x1 top
        for dy in 0..3 {
            let r = match dy { 0 => 2, 1 => 1, _ => 0 };
            for dx in -(r as i32)..=r as i32 {
                for dz in -(r as i32)..=r as i32 {
                    let bx = tx as i32 + dx;
                    let bz = tz as i32 + dz;
                    let by = floor_y + dy;
                    if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                    if by < CHUNK_HEIGHT {
                        // Interior is air, walls are snow
                        if (dx.abs() == r as i32 || dz.abs() == r as i32 || dy == 2) && r > 0 {
                            chunk.set_block(bx as usize, by, bz as usize, Block::new(BlockId::SnowBlock));
                        } else if dy == 0 {
                            // Floor
                            if dx == 0 && dz == 0 {
                                chunk.set_block(bx as usize, by, bz as usize, Block::new(BlockId::RedCarpet));
                            } else {
                                chunk.set_block(bx as usize, by, bz as usize, Block::new(BlockId::WhiteCarpet));
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

    fn place_swamp_hut(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let gy = (height - 1).max(0) as usize;
        // Stilts (oak plank posts) if over water
        for dx in 0..3 {
            for dz in 0..3 {
                let sx = tx + dx;
                let sz = tz + dz;
                if sx >= CHUNK_SIZE || sz >= CHUNK_SIZE { continue; }
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
                if sx >= CHUNK_SIZE || sz >= CHUNK_SIZE { continue; }
                chunk.set_block(sx, floor_y, sz, Block::new(BlockId::OakPlanks));
            }
        }
        // Walls: 2 blocks high
        for dy in 1..=2 {
            for dx in 0..3 {
                for dz in 0..3 {
                    let sx = tx + dx;
                    let sz = tz + dz;
                    if sx >= CHUNK_SIZE || sz >= CHUNK_SIZE { continue; }
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
                if sx < 0 || sx >= CHUNK_SIZE as i32 || sz < 0 || sz >= CHUNK_SIZE as i32 { continue; }
                let d = dx.abs().max(dz.abs() - 1.max(dz.abs()));
                if d <= 2 {
                    chunk.set_block(sx as usize, floor_y + 3, sz as usize, Block::new(BlockId::OakPlanks));
                }
            }
        }
        // Flower pot with mushroom inside
        if tx + 1 < CHUNK_SIZE && tz + 1 < CHUNK_SIZE && floor_y + 2 < CHUNK_HEIGHT {
            chunk.set_block(tx + 1, floor_y + 1, tz + 1, Block::new(BlockId::BrownMushroom));
        }
    }

    fn place_ocean_ruin(&self, chunk: &mut Chunk, tx: usize, tz: usize, height: i32, _rng: &mut impl Rng) {
        let gy = (height - 1).max(0) as usize;
        let is_underwater = height < Self::SEA_LEVEL;
        // Small ruin: 3x3 platform of stone bricks, with partial walls
        for dx in -1..=1 {
            for dz in -1..=1 {
                let bx = tx as i32 + dx;
                let bz = tz as i32 + dz;
                if bx < 0 || bx >= CHUNK_SIZE as i32 || bz < 0 || bz >= CHUNK_SIZE as i32 { continue; }
                // Floor
                let existing = chunk.get_block(bx as usize, gy, bz as usize);
                if is_underwater || existing.id == BlockId::Sand || existing.id == BlockId::Gravel || existing.id == BlockId::Dirt {
                    chunk.set_block(bx as usize, gy, bz as usize, Block::new(BlockId::StoneBricks));
                }
                // Partial corner walls (1 block high)
                if dx.abs() == 1 && dz.abs() == 1 && _rng.gen_bool(0.6) {
                    if gy + 1 < CHUNK_HEIGHT {
                        let above = chunk.get_block(bx as usize, gy + 1, bz as usize);
                        if above.is_air() || above.id == BlockId::Water {
                            chunk.set_block(bx as usize, gy + 1, bz as usize, Block::new(BlockId::StoneBricks));
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Biome {
    Plains,
    Forest,
    Desert,
    Savanna,
    Taiga,
    SnowyTundra,
    Mountains,
    Swamp,
    Jungle,
    DarkForest,
}
