use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE, CHUNK_VOLUME};
use crate::world::block::{Block, BlockId};
use crate::world::mesh::{ChunkMesh, build_chunk_mesh};
use crate::world::world_gen::WorldGenerator;

pub const RENDER_DISTANCE: i32 = 8;

struct ChunkGenTask {
    cx: i32,
    cz: i32,
    seed: u64,
}

pub struct ChunkGenResult {
    pub cx: i32,
    pub cz: i32,
    pub blocks: Box<[Block; CHUNK_VOLUME]>,
}

pub struct ChunkManager {
    pub chunks: HashMap<(i32, i32), Chunk>,
    pub meshes: HashMap<(i32, i32), ChunkMesh>,
    generator: WorldGenerator,
    task_tx: Sender<ChunkGenTask>,
    result_rx: Receiver<ChunkGenResult>,
    pending: HashSet<(i32, i32)>,
    seed: u64,
}

impl ChunkManager {
    pub fn new(seed: u64) -> Self {
        let (task_tx, task_rx) = channel::<ChunkGenTask>();
        let (result_tx, result_rx) = channel::<ChunkGenResult>();
        let task_rx = std::sync::Arc::new(std::sync::Mutex::new(task_rx));
        let num_workers = thread::available_parallelism().map(|n| n.get()).unwrap_or(4);

        for _ in 0..num_workers {
            let rx = std::sync::Arc::clone(&task_rx);
            let tx = result_tx.clone();
            thread::spawn(move || {
                loop {
                    let task = {
                        let lock = rx.lock().unwrap();
                        lock.recv()
                    };
                    match task {
                        Ok(task) => {
                            let generator = WorldGenerator::new(task.seed);
                            let mut chunk = Chunk::new(task.cx, task.cz);
                            generator.generate_chunk(&mut chunk);
                            if tx.send(ChunkGenResult {
                                cx: task.cx,
                                cz: task.cz,
                                blocks: chunk.blocks,
                            }).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        ChunkManager {
            chunks: HashMap::new(),
            meshes: HashMap::new(),
            generator: WorldGenerator::new(seed),
            task_tx,
            result_rx,
            pending: HashSet::new(),
            seed,
        }
    }

    pub fn update_chunks_async(&mut self, player_cx: i32, player_cz: i32) {
        let to_load: Vec<(i32, i32)> = (player_cx - RENDER_DISTANCE..=player_cx + RENDER_DISTANCE)
            .flat_map(|x| (player_cz - RENDER_DISTANCE..=player_cz + RENDER_DISTANCE).map(move |z| (x, z)))
            .collect();

        for &(cx, cz) in &to_load {
            if !self.chunks.contains_key(&(cx, cz)) && !self.pending.contains(&(cx, cz)) {
                self.pending.insert((cx, cz));
                let _ = self.task_tx.send(ChunkGenTask { cx, cz, seed: self.seed });
            }
        }

        self.chunks.retain(|&(cx, cz), _| {
            cx >= player_cx - RENDER_DISTANCE - 1
                && cx <= player_cx + RENDER_DISTANCE + 1
                && cz >= player_cz - RENDER_DISTANCE - 1
                && cz <= player_cz + RENDER_DISTANCE + 1
        });
    }

    pub fn process_loaded_chunks(&mut self) -> usize {
        while let Ok(result) = self.result_rx.try_recv() {
            self.pending.remove(&(result.cx, result.cz));
            if !self.chunks.contains_key(&(result.cx, result.cz)) {
                let mut chunk = Chunk::new(result.cx, result.cz);
                chunk.blocks = result.blocks;
                chunk.is_dirty = true;
                chunk.recount_fluids();
                self.chunks.insert((result.cx, result.cz), chunk);
            }
        }
        self.chunks.len()
    }

    #[allow(dead_code)]
    pub fn get_chunk(&self, cx: i32, cz: i32) -> Option<&Chunk> {
        self.chunks.get(&(cx, cz))
    }

    pub fn get_or_create_chunk(&mut self, cx: i32, cz: i32) -> &mut Chunk {
        let generator = &self.generator;
        self.chunks.entry((cx, cz)).or_insert_with(|| {
            let mut chunk = Chunk::new(cx, cz);
            generator.generate_chunk(&mut chunk);
            chunk
        })
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return Block::air();
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;

        match self.chunks.get(&(cx, cz)) {
            Some(chunk) => chunk.get_block(lx, y as usize, lz),
            None => Block::air(),
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;

        if let Some(chunk) = self.chunks.get_mut(&(cx, cz)) {
            chunk.set_block(lx, y as usize, lz, block);
            chunk.is_dirty = true;
        }
    }

    pub fn update_chunks(&mut self, player_cx: i32, player_cz: i32) {
        let to_load: Vec<(i32, i32)> = (player_cx - RENDER_DISTANCE..=player_cx + RENDER_DISTANCE)
            .flat_map(|x| (player_cz - RENDER_DISTANCE..=player_cz + RENDER_DISTANCE).map(move |z| (x, z)))
            .collect();

        for &(cx, cz) in &to_load {
            self.get_or_create_chunk(cx, cz);
        }

        self.chunks.retain(|&(cx, cz), _| {
            cx >= player_cx - RENDER_DISTANCE - 1
                && cx <= player_cx + RENDER_DISTANCE + 1
                && cz >= player_cz - RENDER_DISTANCE - 1
                && cz <= player_cz + RENDER_DISTANCE + 1
        });
    }

    pub fn rebuild_dirty_meshes(&mut self) {
        let dirty_keys: Vec<(i32, i32)> = self.chunks.iter()
            .filter(|(_, c)| c.is_dirty)
            .map(|(k, _)| *k)
            .collect();

        for key in dirty_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let neighbor_fn = |cx: i32, cz: i32| -> Option<&Chunk> {
                    self.chunks.get(&(cx, cz))
                };
                let mesh = build_chunk_mesh(chunk, &neighbor_fn);
                self.meshes.insert(key, mesh);
            }
            if let Some(chunk) = self.chunks.get_mut(&key) {
                chunk.is_dirty = false;
                chunk.has_mesh = true;
            }
        }
    }

    pub fn get_chunk_mesh(&self, cx: i32, cz: i32) -> Option<&ChunkMesh> {
        self.meshes.get(&(cx, cz))
    }

    pub fn get_biome_name(&self, wx: f64, wz: f64) -> String {
        format!("{:?}", self.generator.get_biome(wx, wz))
    }

    pub fn tick_lava(&mut self) {
        let lava_keys: Vec<(i32, i32)> = self.chunks.iter()
            .filter(|(_, c)| c.has_lava)
            .map(|(&k, _)| k)
            .collect();

        let mut updates: Vec<(i32, i32, i32, u8)> = Vec::new();
        let mut interactions: Vec<(i32, i32, i32)> = Vec::new();

        for &(cx, cz) in &lava_keys {
            let chunk = match self.chunks.get(&(cx, cz)) {
                Some(c) => c,
                None => continue,
            };
            let base_x = cx * CHUNK_SIZE as i32;
            let base_z = cz * CHUNK_SIZE as i32;

            for x in 0..CHUNK_SIZE {
                let wx = base_x + x as i32;
                for y in 1..CHUNK_HEIGHT.min(80) {
                    for z in 0..CHUNK_SIZE {
                        let block = chunk.get_block(x, y, z);
                        if block.id != BlockId::Lava { continue; }
                        let level = block.data;
                        let wz = base_z + z as i32;
                        let wy = y as i32;

                        let below = self.get_block(wx, wy - 1, wz);
                        if below.is_air() || (below.id != BlockId::Lava && !below.id.is_solid()
                            && below.id != BlockId::Water) {
                            updates.push((wx, wy - 1, wz, level));
                            continue;
                        }

                        if level < 2 {
                            for (dx, dz) in &[(1,0), (-1,0), (0,1), (0,-1)] {
                                let nx = wx + dx;
                                let nz = wz + dz;
                                let neighbor = self.get_block(nx, wy, nz);
                                if neighbor.is_air() || (neighbor.id == BlockId::Water) {
                                    if neighbor.id == BlockId::Water {
                                        interactions.push((nx, wy, nz));
                                    } else {
                                        updates.push((nx, wy, nz, level + 1));
                                    }
                                } else if neighbor.id == BlockId::Lava && neighbor.data > level + 1 {
                                    updates.push((nx, wy, nz, level + 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        for (x, y, z) in &interactions {
            self.set_block(*x, *y, *z, Block::new(BlockId::Stone));
        }

        for (x, y, z, data) in updates {
            let existing = self.get_block(x, y, z);
            if existing.is_air() {
                self.set_block(x, y, z, Block { id: BlockId::Lava, data });
            } else if existing.id == BlockId::Lava && existing.data > data {
                let cx = x.div_euclid(CHUNK_SIZE as i32);
                let cz = z.div_euclid(CHUNK_SIZE as i32);
                let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
                let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
                if let Some(ch) = self.chunks.get_mut(&(cx, cz)) {
                    ch.set_block(lx, y as usize, lz, Block { id: BlockId::Lava, data });
                }
            }
        }
    }

    pub fn tick_water(&mut self) {
        let watery_keys: Vec<(i32, i32)> = self.chunks.iter()
            .filter(|(_, c)| c.has_water)
            .map(|(&k, _)| k)
            .collect();

        let mut updates: Vec<(i32, i32, i32, u8)> = Vec::new();
        let mut lava_interactions: Vec<(i32, i32, i32, BlockId)> = Vec::new();

        for &(cx, cz) in &watery_keys {
            let chunk = match self.chunks.get(&(cx, cz)) {
                Some(c) => c,
                None => continue,
            };
            let base_x = cx * CHUNK_SIZE as i32;
            let base_z = cz * CHUNK_SIZE as i32;

            for x in 0..CHUNK_SIZE {
                let wx = base_x + x as i32;
                for y in 1..CHUNK_HEIGHT.min(80) {
                    for z in 0..CHUNK_SIZE {
                        let block = chunk.get_block(x, y, z);
                        if block.id != BlockId::Water { continue; }
                        let level = block.data;
                        let wz = base_z + z as i32;
                        let wy = y as i32;

                        let lava_adjacent = [(1,0), (-1,0), (0,1), (0,-1)].iter().any(|(dx, dz)| {
                            let nx = wx + dx;
                            let nz = wz + dz;
                            let neighbor_below = self.get_block(nx, wy, nz);
                            let neighbor_above = self.get_block(nx, wy + 1, nz);
                            neighbor_below.id == BlockId::Lava || neighbor_above.id == BlockId::Lava
                        }) || self.get_block(wx, wy - 1, wz).id == BlockId::Lava
                           || self.get_block(wx, wy + 1, wz).id == BlockId::Lava;
                        if lava_adjacent {
                            let result = if level == 0 { BlockId::Obsidian } else { BlockId::Cobblestone };
                            lava_interactions.push((wx, wy, wz, result));
                            continue;
                        }

                        let below = self.get_block(wx, wy - 1, wz);
                        if below.is_air() || (below.id != BlockId::Water && !below.id.is_solid()) {
                            updates.push((wx, wy - 1, wz, level));
                            continue;
                        }

                        if level < 7 {
                            for (dx, dz) in &[(1,0), (-1,0), (0,1), (0,-1)] {
                                let nx = wx + dx;
                                let nz = wz + dz;
                                let neighbor = self.get_block(nx, wy, nz);
                                if neighbor.is_air() {
                                    updates.push((nx, wy, nz, level + 1));
                                } else if neighbor.id == BlockId::Water && neighbor.data > level + 1 {
                                    updates.push((nx, wy, nz, level + 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        for (x, y, z, block_id) in &lava_interactions {
            self.set_block(*x, *y, *z, Block::new(*block_id));
        }

        for (x, y, z, data) in updates {
            let existing = self.get_block(x, y, z);
            if existing.is_air() {
                self.set_block(x, y, z, Block { id: BlockId::Water, data });
            } else if existing.id == BlockId::Water && existing.data > data {
                let cx = x.div_euclid(CHUNK_SIZE as i32);
                let cz = z.div_euclid(CHUNK_SIZE as i32);
                let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
                let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
                if let Some(chunk) = self.chunks.get_mut(&(cx, cz)) {
                    chunk.set_block(lx, y as usize, lz, Block { id: BlockId::Water, data });
                }
            }
        }
    }
}
