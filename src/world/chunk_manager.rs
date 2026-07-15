use crate::inventory::item::ItemRegistry;
use crate::world::block::{Block, BlockId};
use crate::world::chunk::{BlockEntity, Chunk, CHUNK_HEIGHT, CHUNK_SIZE, CHUNK_VOLUME};
use crate::world::mesh::{build_chunk_mesh, ChunkMesh};
use crate::world::persistence::{ChunkData, StorageError, WorldStorage};
use crate::world::block_registry::registry;
use crate::world::world_gen::WorldGenerator;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

pub const DEFAULT_RENDER_DISTANCE: i32 = 6;
const MAX_LOADED_CHUNKS_PER_FRAME: usize = 32;
const MAX_MESHES_IN_FLIGHT: usize = 8;
const MAX_LIGHT_KEYS_PER_FRAME: usize = 16;

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "worker panicked without a message".to_string()
    }
}

struct ChunkGenTask {
    cx: i32,
    cz: i32,
    saved_chunk: Option<Chunk>,
}

struct ChunkGenResult {
    key: (i32, i32),
    result: Result<Chunk, String>,
}

struct MeshTask {
    key: (i32, i32),
    task_id: u64,
    dependencies: HashMap<(i32, i32), u64>,
    chunk: Arc<Chunk>,
    neighbors: HashMap<(i32, i32), Arc<Chunk>>,
}

struct MeshResult {
    key: (i32, i32),
    task_id: u64,
    dependencies: HashMap<(i32, i32), u64>,
    result: Result<ChunkMesh, String>,
}

struct LightTask {
    epoch: u64,
    dependencies: HashMap<(i32, i32), u64>,
    chunks: HashMap<(i32, i32), Arc<Chunk>>,
    keys: HashSet<(i32, i32)>,
}

struct LightResult {
    epoch: u64,
    dependencies: HashMap<(i32, i32), u64>,
    requested_keys: HashSet<(i32, i32)>,
    keys: HashSet<(i32, i32)>,
    light_arrays: Vec<((i32, i32), Box<[u8; CHUNK_VOLUME]>, Box<[u8; CHUNK_VOLUME]>)>,
    error: Option<String>,
}

pub struct ChunkManager {
    pub chunks: HashMap<(i32, i32), Arc<Chunk>>,
    pub meshes: HashMap<(i32, i32), ChunkMesh>,
    generator: WorldGenerator,
    task_tx: Sender<ChunkGenTask>,
    result_rx: Receiver<ChunkGenResult>,
    mesh_task_tx: Sender<MeshTask>,
    mesh_result_rx: Receiver<MeshResult>,
    light_task_tx: Sender<LightTask>,
    light_result_rx: Receiver<LightResult>,
    meshing: HashMap<(i32, i32), u64>,
    pending: HashSet<(i32, i32)>,
    max_generation_tasks: usize,
    render_distance: i32,
    cached_range: Vec<(i32, i32)>,
    cached_range_center: (i32, i32),
    dirty_keys: HashSet<(i32, i32)>,
    save_dirty_keys: HashSet<(i32, i32)>,
    save_failed_keys: HashSet<(i32, i32)>,
    storage: Option<WorldStorage>,
    light_dirty_keys: HashSet<(i32, i32)>,
    light_in_flight: Option<HashSet<(i32, i32)>>,
    chunk_revisions: HashMap<(i32, i32), u64>,
    next_task_id: u64,
    work_epoch: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkManagerStats {
    pub loaded: usize,
    pub meshed: usize,
    pub pending: usize,
    pub dirty_queue: usize,
    pub light_dirty_queue: usize,
}

impl ChunkManager {
    pub fn new(seed: u64, render_distance: i32) -> Self {
        let (task_tx, task_rx) = channel::<ChunkGenTask>();
        let (result_tx, result_rx) = channel::<ChunkGenResult>();
        let (mesh_task_tx, mesh_task_rx) = channel::<MeshTask>();
        let (mesh_result_tx, mesh_result_rx) = channel::<MeshResult>();
        let (light_task_tx, light_task_rx) = channel::<LightTask>();
        let (light_result_tx, light_result_rx) = channel::<LightResult>();
        let task_rx = std::sync::Arc::new(std::sync::Mutex::new(task_rx));
        let available_workers = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let mesh_workers = (available_workers / 3).max(1);
        let generation_workers = available_workers
            .saturating_sub(mesh_workers + 1)
            .max(1);

        for _ in 0..generation_workers {
            let rx = std::sync::Arc::clone(&task_rx);
            let tx = result_tx.clone();
            thread::spawn(move || {
                let mut generator = WorldGenerator::new(seed);
                loop {
                let task = {
                    let lock = rx.lock().unwrap_or_else(|e| e.into_inner());
                    lock.recv()
                };
                match task {
                    Ok(task) => {
                        let key = (task.cx, task.cz);
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            if let Some(chunk) = task.saved_chunk {
                                chunk
                            } else {
                                let mut chunk = Chunk::new(task.cx, task.cz);
                                generator.generate_chunk(&mut chunk);
                                chunk
                            }
                        }))
                        .map_err(panic_message);
                        if tx.send(ChunkGenResult { key, result }).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                }
            });
        }

        let mesh_task_rx = Arc::new(std::sync::Mutex::new(mesh_task_rx));
        for _ in 0..mesh_workers {
            let rx = Arc::clone(&mesh_task_rx);
            let tx = mesh_result_tx.clone();
            thread::spawn(move || {
                loop {
                    let task = {
                        let lock = rx.lock().unwrap_or_else(|e| e.into_inner());
                        lock.recv()
                    };
                    let Ok(task) = task else { break };
                    let neighbor_fn = |cx: i32, cz: i32| {
                        task.neighbors.get(&(cx, cz)).map(|a| a.as_ref())
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        build_chunk_mesh(&task.chunk, &neighbor_fn)
                    }))
                    .map_err(panic_message);
                    if tx
                        .send(MeshResult {
                            key: task.key,
                            task_id: task.task_id,
                            dependencies: task.dependencies,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }

        thread::spawn(move || {
            while let Ok(task) = light_task_rx.recv() {
                let LightTask { epoch, dependencies, mut chunks, keys } = task;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let changed = recompute_lighting_on_snapshot(&mut chunks, &keys);
                    let light_arrays = changed
                        .iter()
                        .filter_map(|key| chunks.get(key).map(|c| (*key, c.sky_light.clone(), c.block_light.clone())))
                        .collect();
                    (changed, light_arrays)
                }));
                let (keys_changed, light_arrays, error) = match result {
                    Ok((keys_changed, light_arrays)) => (keys_changed, light_arrays, None),
                    Err(payload) => (HashSet::new(), Vec::new(), Some(panic_message(payload))),
                };
                if light_result_tx
                    .send(LightResult {
                        epoch,
                        dependencies,
                        requested_keys: keys,
                        keys: keys_changed,
                        light_arrays,
                        error,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        ChunkManager {
            chunks: HashMap::new(),
            meshes: HashMap::new(),
            generator: WorldGenerator::new(seed),
            task_tx,
            result_rx,
            mesh_task_tx,
            mesh_result_rx,
            light_task_tx,
            light_result_rx,
            meshing: HashMap::new(),
            pending: HashSet::new(),
            max_generation_tasks: (generation_workers * 2).max(4),
            render_distance,
            cached_range: Vec::new(),
            cached_range_center: (0, 0),
            dirty_keys: HashSet::new(),
            save_dirty_keys: HashSet::new(),
            save_failed_keys: HashSet::new(),
            storage: None,
            light_dirty_keys: HashSet::new(),
            light_in_flight: None,
            chunk_revisions: HashMap::new(),
            next_task_id: 0,
            work_epoch: 0,
        }
    }

    /// Enables native chunk persistence. Existing save files are loaded before
    /// generation and changed chunks are written before they leave memory.
    pub fn set_storage(&mut self, storage: WorldStorage) {
        self.storage = Some(storage);
    }

    /// Saves every changed loaded chunk. Returns false when a write failed;
    /// callers should keep the process alive rather than discard that state.
    pub fn flush_saved_chunks(&mut self) -> bool {
        let keys: Vec<_> = self.save_dirty_keys.iter().copied().collect();
        let mut saved_all = true;
        for key in keys {
            if !self.save_chunk(key) {
                self.save_failed_keys.insert(key);
                saved_all = false;
            }
        }
        saved_all
    }

    fn save_chunk(&mut self, key: (i32, i32)) -> bool {
        let Some(storage) = self.storage.as_ref() else {
            return true;
        };
        let Some(chunk) = self.chunks.get(&key) else {
            return true;
        };
        let data = ChunkData::from_chunk(chunk);
        match storage.save_chunk(&data) {
            Ok(()) => {
                self.save_dirty_keys.remove(&key);
                self.save_failed_keys.remove(&key);
                true
            }
            Err(error) => {
                log::error!("failed to save chunk {key:?}: {error}");
                false
            }
        }
    }

    pub fn stats(&self) -> ChunkManagerStats {
        ChunkManagerStats {
            loaded: self.chunks.len(),
            meshed: self.chunks.values().filter(|chunk| chunk.has_mesh).count(),
            pending: self.pending.len(),
            dirty_queue: self.dirty_keys.len(),
            light_dirty_queue: self.light_dirty_keys.len(),
        }
    }

    fn bump_chunk_revision(&mut self, key: (i32, i32)) {
        let revision = self.chunk_revisions.entry(key).or_insert(0);
        *revision = revision.wrapping_add(1);
    }

    fn snapshot_revisions(
        &self,
        keys: impl Iterator<Item = (i32, i32)>,
    ) -> HashMap<(i32, i32), u64> {
        keys.filter_map(|key| self.chunk_revisions.get(&key).map(|&revision| (key, revision)))
            .collect()
    }

    fn revisions_are_current(&self, dependencies: &HashMap<(i32, i32), u64>) -> bool {
        dependencies
            .iter()
            .all(|(key, revision)| self.chunk_revisions.get(key) == Some(revision))
    }

    fn allocate_task_id(&mut self) -> u64 {
        let task_id = self.next_task_id;
        self.next_task_id = self.next_task_id.wrapping_add(1);
        task_id
    }

    fn mesh_neighborhood_ready(&self, key: (i32, i32)) -> bool {
        (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                self.chunks
                    .get(&(key.0 + dx, key.1 + dz))
                    .map_or(true, |chunk| !chunk.light_dirty)
            })
        })
    }

    fn chunk_range(&mut self, player_cx: i32, player_cz: i32) {
        if self.cached_range_center != (player_cx, player_cz) || self.cached_range.is_empty() {
            self.cached_range.clear();
            for x in player_cx - self.render_distance..=player_cx + self.render_distance {
                for z in player_cz - self.render_distance..=player_cz + self.render_distance {
                    self.cached_range.push((x, z));
                }
            }
            self.cached_range_center = (player_cx, player_cz);
        }
    }

    fn retain_chunks(&mut self, player_cx: i32, player_cz: i32) {
        self.retain_chunks_for_centers(&[(player_cx, player_cz)]);
    }

    fn retain_chunks_for_centers(&mut self, centers: &[(i32, i32)]) {
        let render_distance = self.render_distance;
        let keep = |(cx, cz): (i32, i32)| {
            centers.iter().any(|&(center_x, center_z)| {
                cx >= center_x - render_distance - 1
                    && cx <= center_x + render_distance + 1
                    && cz >= center_z - render_distance - 1
                    && cz <= center_z + render_distance + 1
            })
        };
        let unload: Vec<_> = self.chunks.keys().copied().filter(|&key| !keep(key)).collect();
        for key in unload {
            if self.save_dirty_keys.contains(&key) && !self.save_chunk(key) {
                self.save_failed_keys.insert(key);
                continue;
            }
            self.chunks.remove(&key);
            self.meshes.remove(&key);
            self.meshing.remove(&key);
            self.dirty_keys.remove(&key);
            self.light_dirty_keys.remove(&key);
            self.chunk_revisions.remove(&key);
            self.save_dirty_keys.remove(&key);
            self.save_failed_keys.remove(&key);
        }
        self.meshes.retain(|&key, _| keep(key));
    }

    pub fn update_chunks_async(&mut self, player_cx: i32, player_cz: i32) {
        self.update_chunks_async_for_centers(&[(player_cx, player_cz)]);
    }

    pub fn set_render_distance(&mut self, render_distance: i32) {
        self.render_distance = render_distance.clamp(2, 32);
        self.cached_range.clear();
    }

    /// Streams the bounded union around several active players. This is used
    /// by the authoritative server so one player's movement does not unload
    /// terrain needed by another player.
    pub fn update_chunks_async_for_centers(&mut self, centers: &[(i32, i32)]) {
        if centers.is_empty() {
            return;
        }
        self.cached_range.clear();
        let mut desired = HashSet::new();
        for &(center_x, center_z) in centers {
            for cx in center_x - self.render_distance..=center_x + self.render_distance {
                for cz in center_z - self.render_distance..=center_z + self.render_distance {
                    desired.insert((cx, cz));
                }
            }
        }
        self.cached_range.extend(desired.iter().copied());
        self.cached_range_center = centers[0];
        self.pending.retain(|key| desired.contains(key));
        let mut range = self.cached_range.clone();
        range.sort_by_key(|(cx, cz)| {
            centers
                .iter()
                .map(|(center_x, center_z)| (cx - center_x).abs() + (cz - center_z).abs())
                .min()
                .unwrap_or(i32::MAX)
        });
        for &(cx, cz) in &range {
            if self.pending.len() >= self.max_generation_tasks {
                break;
            }
            if !self.chunks.contains_key(&(cx, cz))
                && !self.pending.contains(&(cx, cz))
                && !self.save_failed_keys.contains(&(cx, cz))
            {
                let saved_chunk = match self.storage.as_ref() {
                    Some(storage) => match storage.load_chunk_if_present(cx, cz) {
                        Ok(Some(data)) => match data.into_chunk() {
                            Ok(chunk) => Some(chunk),
                            Err(error) => {
                                log::error!("failed to decode saved chunk ({cx}, {cz}): {error}");
                                self.save_failed_keys.insert((cx, cz));
                                continue;
                            }
                        },
                        Ok(None) => None,
                        Err(error) => {
                            log::error!("failed to load saved chunk ({cx}, {cz}): {error}");
                            self.save_failed_keys.insert((cx, cz));
                            continue;
                        }
                    },
                    None => None,
                };
                self.pending.insert((cx, cz));
                let _ = self.task_tx.send(ChunkGenTask { cx, cz, saved_chunk });
            }
        }
        self.retain_chunks_for_centers(centers);
    }

    pub fn process_loaded_chunks(&mut self) -> usize {
        for _ in 0..MAX_LOADED_CHUNKS_PER_FRAME {
            let Ok(result) = self.result_rx.try_recv() else {
                break;
            };
            let key = result.key;
            self.pending.remove(&key);
            let chunk = match result.result {
                Ok(chunk) => chunk,
                Err(error) => {
                    log::error!("chunk generation worker failed for {key:?}: {error}");
                    continue;
                }
            };
            if !self.cached_range.contains(&key) {
                continue;
            }
            if !self.chunks.contains_key(&key) {
                self.chunks.insert(key, Arc::new(chunk));
                self.bump_chunk_revision(key);
                // Do not publish a bootstrap mesh: it would contain fabricated full
                // skylight. The relight result below schedules the first valid mesh.
                self.light_dirty_keys.insert(key);
                for (dx, dz) in &[(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nk = (key.0 + dx, key.1 + dz);
                    if let Some(chunk) = self.chunks.get_mut(&nk).map(|a| clone_arc(a)) {
                        chunk.light_dirty = true;
                        self.light_dirty_keys.insert(nk);
                        self.bump_chunk_revision(nk);
                    }
                }
            }
        }
        self.chunks.len()
    }

    #[allow(dead_code)]
    pub fn get_chunk(&self, cx: i32, cz: i32) -> Option<&Chunk> {
        self.chunks.get(&(cx, cz)).map(|a| a.as_ref())
    }

    /// Returns loaded chunk coordinates for bounded replication and diagnostics.
    pub fn loaded_chunk_keys(&self) -> Vec<(i32, i32)> {
        self.chunks.keys().copied().collect()
    }

    /// Returns the authoritative mutation revision for a loaded chunk.
    pub fn chunk_revision(&self, cx: i32, cz: i32) -> Option<u64> {
        self.chunk_revisions.get(&(cx, cz)).copied()
    }

    /// Copies the durable block representation used by the native network
    /// codec. Lighting and meshes are deliberately excluded from replication.
    pub fn chunk_data(&self, cx: i32, cz: i32) -> Option<ChunkData> {
        self.chunks.get(&(cx, cz)).map(|chunk| ChunkData::from_chunk(chunk))
    }

    /// Starts a new authoritative network session by discarding snapshots and
    /// worker state owned by the previous server session. Server revisions are
    /// local to a running server, so retaining them across reconnect would let
    /// an older client snapshot reject the new server's initial chunk stream.
    pub fn reset_authoritative_session(&mut self) {
        self.chunks.clear();
        self.meshes.clear();
        self.pending.clear();
        self.meshing.clear();
        self.dirty_keys.clear();
        self.save_dirty_keys.clear();
        self.save_failed_keys.clear();
        self.light_dirty_keys.clear();
        self.chunk_revisions.clear();
        self.cached_range.clear();
        self.light_in_flight = None;
        self.work_epoch = self.work_epoch.wrapping_add(1);
    }

    /// Replaces a chunk with an authoritative snapshot received from a server.
    /// Network snapshots carry no lighting or mesh state, so those are rebuilt
    /// through the normal asynchronous client-side pipeline.
    pub fn apply_chunk_data(&mut self, data: ChunkData, revision: u64) -> Result<bool, StorageError> {
        let key = (data.cx, data.cz);
        if self
            .chunk_revisions
            .get(&key)
            .is_some_and(|current| *current >= revision)
        {
            return Ok(false);
        }
        let mut chunk = data.into_chunk()?;
        chunk.is_dirty = false;
        chunk.light_dirty = true;
        chunk.has_mesh = false;

        self.pending.remove(&key);
        self.meshing.remove(&key);
        self.meshes.remove(&key);
        self.chunks.insert(key, Arc::new(chunk));
        self.chunk_revisions.insert(key, revision);
        self.dirty_keys.insert(key);
        self.light_dirty_keys.insert(key);

        // A replacement can change face culling and ambient-occlusion samples
        // for the four neighboring columns.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let neighbor = (key.0 + dx, key.1 + dz);
            if self.chunks.contains_key(&neighbor) {
                self.dirty_keys.insert(neighbor);
            }
        }
        Ok(true)
    }

    /// Removes a server-authoritative snapshot without starting local
    /// generation or saving it as a singleplayer chunk.
    pub fn unload_authoritative_chunk(&mut self, cx: i32, cz: i32) -> bool {
        let key = (cx, cz);
        let existed = self.chunks.remove(&key).is_some();
        self.meshes.remove(&key);
        self.pending.remove(&key);
        self.meshing.remove(&key);
        self.dirty_keys.remove(&key);
        self.light_dirty_keys.remove(&key);
        self.chunk_revisions.remove(&key);
        self.save_dirty_keys.remove(&key);
        self.save_failed_keys.remove(&key);
        if existed {
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let neighbor = (cx + dx, cz + dz);
                if self.chunks.contains_key(&neighbor) {
                    self.dirty_keys.insert(neighbor);
                }
            }
        }
        existed
    }

    /// Applies a server-authoritative block state without allowing an older
    /// update to overwrite a newer local snapshot.
    pub fn apply_block_state(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block: Block,
        revision: u64,
    ) -> bool {
        let key = (x.div_euclid(CHUNK_SIZE as i32), z.div_euclid(CHUNK_SIZE as i32));
        if !self.chunks.contains_key(&key)
            || self
                .chunk_revisions
                .get(&key)
                .is_some_and(|current| *current > revision)
        {
            return false;
        }
        self.set_block(x, y, z, block);
        self.chunk_revisions.insert(key, revision);
        true
    }

    #[allow(dead_code)]
    pub fn get_or_create_chunk(&mut self, cx: i32, cz: i32) -> &mut Chunk {
        if !self.chunks.contains_key(&(cx, cz)) {
            let mut chunk = Chunk::new(cx, cz);
            self.generator.generate_chunk(&mut chunk);
            self.chunks.insert((cx, cz), Arc::new(chunk));
            self.bump_chunk_revision((cx, cz));
        }
        clone_arc(self.chunks.get_mut(&(cx, cz)).unwrap())
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

    pub fn get_block_entity(&self, x: i32, y: i32, z: i32) -> Option<&BlockEntity> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        self.chunks
            .get(&(cx, cz))
            .and_then(|chunk| chunk.get_block_entity(lx, y as usize, lz))
    }

    /// Persists state changes without invalidating a mesh that cannot observe them.
    pub fn set_block_entity(&mut self, x: i32, y: i32, z: i32, entity: BlockEntity) -> bool {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return false;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|arc| clone_arc(arc)) else {
            return false;
        };
        if !chunk.set_block_entity(lx, y as usize, lz, entity) {
            return false;
        }
        if self.storage.is_some() {
            self.save_dirty_keys.insert((cx, cz));
        }
        true
    }

    /// Advances only loaded furnace block entities. The loaded chunk set is
    /// already bounded by render distance, and chunks are marked for durable
    /// persistence only when processing changes their state.
    pub fn tick_block_entities(&mut self, items: &ItemRegistry) -> u32 {
        let keys: Vec<_> = self.chunks.keys().copied().collect();
        let mut completed = 0;
        for key in keys {
            let Some(chunk) = self.chunks.get_mut(&key).map(|chunk| clone_arc(chunk)) else {
                continue;
            };
            let mut changed = false;
            for entity in chunk.block_entities.values_mut() {
                if let BlockEntity::Furnace { state } = entity {
                    let before = (state.burn_time, state.burn_total, state.cook_time, state.slots.slots.clone());
                    if state.tick(items).is_some() {
                        completed += 1;
                    }
                    changed |= before != (state.burn_time, state.burn_total, state.cook_time, state.slots.slots.clone());
                }
            }
            if changed {
                chunk.is_dirty = true;
                if self.storage.is_some() {
                    self.save_dirty_keys.insert(key);
                }
            }
        }
        completed
    }

    /// Finds a standable spawn in already-loaded terrain near the requested
    /// column. It never generates synchronously, keeping first-world spawn
    /// selection within the normal streaming lifecycle.
    pub fn find_safe_spawn(&self, x: i32, z: i32, radius: i32) -> Option<[i32; 3]> {
        for distance in 0..=radius {
            for dx in -distance..=distance {
                for dz in -distance..=distance {
                    if distance != 0 && dx.abs() != distance && dz.abs() != distance {
                        continue;
                    }
                    let wx = x + dx;
                    let wz = z + dz;
                    let cx = wx.div_euclid(CHUNK_SIZE as i32);
                    let cz = wz.div_euclid(CHUNK_SIZE as i32);
                    if !self.chunks.contains_key(&(cx, cz)) {
                        continue;
                    }
                    for y in (1..CHUNK_HEIGHT as i32 - 2).rev() {
                        let ground = self.get_block(wx, y, wz);
                        if ground.id.is_solid()
                            && self.get_block(wx, y + 1, wz).is_air()
                            && self.get_block(wx, y + 2, wz).is_air()
                        {
                            return Some([wx, y + 1, wz]);
                        }
                    }
                }
            }
        }
        None
    }

    /// Internal light level: max(sky_light, block_light) for gameplay checks.
    /// Returns 0-15, where 0 is dark and 15 is fully lit.
    pub fn get_internal_light(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return 15;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        match self.chunks.get(&(cx, cz)) {
            Some(chunk) => {
                let idx = Chunk::index(lx, y as usize, lz);
                chunk.sky_light[idx].max(chunk.block_light[idx])
            }
            None => 15,
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        let old = self.get_block(x, y, z);
        self.set_block_internal(x, y, z, block, true);
        if old.id == BlockId::OakDoor && block.is_air() {
            let half = registry()
                .properties_for_state(old.id, old.state)
                .and_then(|properties| properties.into_iter().find(|(name, _)| *name == "half").map(|(_, value)| value));
            let other_y = if half == Some("lower") { y + 1 } else { y - 1 };
            if self.get_block(x, other_y, z).id == BlockId::OakDoor {
                self.set_block_internal(x, other_y, z, Block::air(), true);
            }
        }
        self.refresh_connected_states_around(x, y, z);
    }

    /// Applies normal placement rules that need more than one block state.
    /// The caller still owns reach, inventory consumption, and game-mode rules.
    pub fn place_block(&mut self, x: i32, y: i32, z: i32, id: BlockId) -> bool {
        if !self.get_block(x, y, z).is_air() {
            return false;
        }
        if id == BlockId::OakDoor {
            if !self.get_block(x, y + 1, z).is_air() {
                return false;
            }
            let lower = registry().state_for_properties(id, [
                ("facing", "south"), ("half", "lower"), ("hinge", "left"), ("open", "false"), ("powered", "false"),
            ]).unwrap_or(0);
            let upper = registry().state_for_properties(id, [
                ("facing", "south"), ("half", "upper"), ("hinge", "left"), ("open", "false"), ("powered", "false"),
            ]).unwrap_or(0);
            self.set_block_internal(x, y, z, Block { id, state: lower, data: 0 }, true);
            self.set_block_internal(x, y + 1, z, Block { id, state: upper, data: 0 }, true);
        } else {
            self.set_block_internal(x, y, z, Block::new(id), true);
        }
        self.refresh_connected_states_around(x, y, z);
        self.refresh_connected_states_around(x, y + 1, z);
        true
    }

    fn refresh_connected_states_around(&mut self, x: i32, y: i32, z: i32) {
        for (dx, dz) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.refresh_connected_state(x + dx, y, z + dz);
        }
    }

    fn refresh_connected_state(&mut self, x: i32, y: i32, z: i32) {
        let block = self.get_block(x, y, z);
        let directions = [("north", 0, -1), ("east", 1, 0), ("south", 0, 1), ("west", -1, 0)];
        let state = match block.id {
            BlockId::OakFence => {
                let values: Vec<(&str, &str)> = directions.iter().map(|&(name, dx, dz)| {
                    let neighbor = self.get_block(x + dx, y, z + dz);
                    (name, if neighbor.id == BlockId::OakFence || neighbor.id.is_solid() { "true" } else { "false" })
                }).chain(std::iter::once(("waterlogged", "false"))).collect();
                registry().state_for_properties(block.id, values).unwrap_or(block.state)
            }
            BlockId::RedstoneDust => {
                let values: Vec<(&str, &str)> = directions.iter().map(|&(name, dx, dz)| {
                    let neighbor = self.get_block(x + dx, y, z + dz);
                    (name, if neighbor.id == BlockId::RedstoneDust { "side" } else { "none" })
                }).chain(std::iter::once(("power", "0"))).collect();
                registry().state_for_properties(block.id, values).unwrap_or(block.state)
            }
            _ => return,
        };
        if state != block.state {
            self.set_block_internal(x, y, z, Block { state, ..block }, false);
        }
    }

    fn set_block_internal(&mut self, x: i32, y: i32, z: i32, block: Block, update_lighting: bool) {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;

        let mut changed = false;
        let mut sig_changed = false;
        if let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) {
            let old = chunk.get_block(lx, y as usize, lz);
            if old != block {
                chunk.set_block(lx, y as usize, lz, block);
                changed = true;
                chunk.is_dirty = true;
                self.dirty_keys.insert((cx, cz));
                if self.storage.is_some() {
                    self.save_dirty_keys.insert((cx, cz));
                }
                sig_changed = update_lighting && light_signature(old) != light_signature(block);
                if sig_changed {
                    chunk.light_dirty = true;
                    self.light_dirty_keys.insert((cx, cz));
                }
            }
        }

        if changed {
            self.bump_chunk_revision((cx, cz));
        }

        if changed {
            let mut x_offsets = vec![0];
            let mut z_offsets = vec![0];
            if lx == 0 {
                x_offsets.push(-1);
            }
            if lx == CHUNK_SIZE - 1 {
                x_offsets.push(1);
            }
            if lz == 0 {
                z_offsets.push(-1);
            }
            if lz == CHUNK_SIZE - 1 {
                z_offsets.push(1);
            }
            for (dx, dz) in x_offsets
                .into_iter()
                .flat_map(|dx| z_offsets.iter().copied().map(move |dz| (dx, dz)))
                .filter(|&(dx, dz)| dx != 0 || dz != 0)
            {
                if let Some(nc) = self.chunks.get_mut(&(cx + dx, cz + dz)).map(|a| clone_arc(a)) {
                    nc.is_dirty = true;
                    self.dirty_keys.insert((cx + dx, cz + dz));
                    // Lighting crosses cardinal boundaries; diagonal chunks
                    // still need remeshing because vertex AO samples them.
                    if sig_changed && (dx == 0 || dz == 0) {
                        nc.light_dirty = true;
                        self.light_dirty_keys.insert((cx + dx, cz + dz));
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn update_chunks(&mut self, player_cx: i32, player_cz: i32) {
        self.chunk_range(player_cx, player_cz);
        let range = self.cached_range.clone();
        for &(cx, cz) in &range {
            self.get_or_create_chunk(cx, cz);
        }
        self.retain_chunks(player_cx, player_cz);
    }

    pub fn rebuild_dirty_meshes(&mut self) -> Vec<(i32, i32)> {
        let mut rebuilt = Vec::new();

        // 1. Apply completed light results from background worker
        while let Ok(result) = self.light_result_rx.try_recv() {
            self.light_in_flight = None;
            if result.epoch != self.work_epoch {
                continue;
            }
            if let Some(error) = result.error {
                log::error!("lighting worker failed: {error}");
                self.light_dirty_keys.extend(result.requested_keys);
                continue;
            }
            if !self.revisions_are_current(&result.dependencies) {
                // Only discard work whose input chunks changed. Fluid updates
                // elsewhere must not starve initial terrain meshing.
                for key in &result.requested_keys {
                    if self.chunks.contains_key(key) {
                        if let Some(chunk) = self.chunks.get_mut(key).map(|a| clone_arc(a)) {
                            chunk.light_dirty = true;
                        }
                        self.light_dirty_keys.insert(*key);
                    }
                }
                continue;
            }
            let mut changed_keys = HashSet::new();
            for (key, sky, block) in &result.light_arrays {
                let changed = self.chunks.get(key).is_some_and(|chunk| {
                    chunk.sky_light.as_ref() != sky.as_ref()
                        || chunk.block_light.as_ref() != block.as_ref()
                });
                if changed {
                    let Some(chunk) = self.chunks.get_mut(key).map(|a| clone_arc(a)) else {
                        continue;
                    };
                    chunk.sky_light = sky.clone();
                    chunk.block_light = block.clone();
                    chunk.is_dirty = true;
                    changed_keys.insert(*key);
                }
            }
            for key in &result.keys {
                if let Some(chunk) = self.chunks.get_mut(key).map(|a| clone_arc(a)) {
                    chunk.light_dirty = false;
                }
            }
            // Lighting is derived client/server state, not an authoritative
            // block mutation. Keeping it out of the public chunk revision is
            // required for network clients: local lighting must not make a
            // valid server block-edit request appear stale.
            self.dirty_keys.extend(changed_keys);
        }

        // 2. Drain completed mesh results
        while let Ok(MeshResult {
            key,
            task_id,
            dependencies,
            result,
        }) = self.mesh_result_rx.try_recv()
        {
            if self.meshing.get(&key) != Some(&task_id) {
                continue;
            }
            if !self.revisions_are_current(&dependencies) {
                if self.meshing.get(&key) == Some(&task_id) {
                    self.meshing.remove(&key);
                    self.dirty_keys.insert(key);
                }
                continue;
            }
            self.meshing.remove(&key);
            let mesh = match result {
                Ok(mesh) => mesh,
                Err(error) => {
                    log::error!("meshing worker failed for {key:?}: {error}");
                    self.dirty_keys.insert(key);
                    continue;
                }
            };
            if self.chunks.contains_key(&key) {
                self.meshes.insert(key, mesh);
                if let Some(chunk) = self.chunks.get_mut(&key).map(|a| clone_arc(a)) {
                    chunk.has_mesh = true;
                    if chunk.is_dirty {
                        self.dirty_keys.insert(key);
                    }
                }
                rebuilt.push(key);
            }
        }

        if self.dirty_keys.is_empty() && self.light_dirty_keys.is_empty() {
            return rebuilt;
        }

        let mut dirty_keys: Vec<(i32, i32)> = self.dirty_keys.drain().collect();
        let center = self.cached_range_center;
        dirty_keys.sort_by_key(|&(cx, cz)| {
            (cx - center.0).abs() + (cz - center.1).abs()
        });

        // 3. Schedule light recomputation on background worker
        if self.light_in_flight.is_none() && !self.light_dirty_keys.is_empty() {
            let mut all_light_dirty: Vec<(i32, i32)> = self.light_dirty_keys.drain().collect();
            let center = self.cached_range_center;
            all_light_dirty.sort_by_key(|&(cx, cz)| {
                (cx - center.0).abs() + (cz - center.1).abs()
            });
            let light_count = all_light_dirty.len().min(MAX_LIGHT_KEYS_PER_FRAME);
            let process_keys: HashSet<(i32, i32)> =
                all_light_dirty.iter().take(light_count).copied().collect();
            for key in all_light_dirty.iter().skip(light_count) {
                self.light_dirty_keys.insert(*key);
            }

            let scope = lighting_scope_for(&self.chunks, &process_keys);
            let mut chunks: HashMap<(i32, i32), Arc<Chunk>> = HashMap::new();
            // Clone scope chunks (will be zeroed and recomputed)
            for key in &scope {
                if let Some(c) = self.chunks.get(key) {
                    chunks.insert(*key, Arc::clone(c));
                }
            }
            // Clone boundary neighbor chunks (for seeding — read-only)
            for &(cx, cz) in &scope {
                for (dx, dz) in &[(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nk = (cx + dx, cz + dz);
                    if !scope.contains(&nk) && self.chunks.contains_key(&nk) {
                        if let Some(c) = self.chunks.get(&nk) {
                            chunks.entry(nk).or_insert_with(|| Arc::clone(c));
                        }
                    }
                }
            }
            let dependencies = self.snapshot_revisions(chunks.keys().copied());
            if self
                .light_task_tx
                .send(LightTask {
                    epoch: self.work_epoch,
                    dependencies,
                    chunks,
                    keys: process_keys.clone(),
                })
                .is_ok()
            {
                self.light_in_flight = Some(scope);
            } else {
                log::error!("lighting worker channel closed; retrying affected chunks");
                self.light_dirty_keys.extend(process_keys);
            }
        }

        if dirty_keys.is_empty() {
            return rebuilt;
        }

        // 4. Schedule new mesh tasks on background worker
        let available_slots = MAX_MESHES_IN_FLIGHT.saturating_sub(self.meshing.len());
        let mut scheduled = 0;
        let mut overflow = Vec::new();
        for key in dirty_keys {
            if !self.chunks.contains_key(&key) {
                continue;
            }
            // Never mesh a chunk until its current lighting is complete. A
            // chunk dirtied after another light task started may be outside
            // that task's scope, but its light arrays are still stale.
            if !self.mesh_neighborhood_ready(key)
                || self.chunks.get(&key).is_some_and(|chunk| chunk.light_dirty)
            {
                overflow.push(key);
                continue;
            }
            if self.meshing.contains_key(&key) {
                continue;
            }
            if scheduled >= available_slots {
                overflow.push(key);
                continue;
            }

            let Some(chunk) = self.chunks.get(&key).map(Arc::clone) else { continue; };
            let neighbors: HashMap<(i32, i32), Arc<Chunk>> = (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dz| (dx, dz)))
                .filter(|&(dx, dz)| dx != 0 || dz != 0)
                .filter_map(|(dx, dz)| {
                    let neighbor_key = (key.0 + dx, key.1 + dz);
                    self.chunks
                        .get(&neighbor_key)
                        .map(Arc::clone)
                        .map(|chunk| (neighbor_key, chunk))
                })
                .collect();
            let dependencies = self.snapshot_revisions(
                std::iter::once(key).chain(neighbors.keys().copied()),
            );
            let task_id = self.allocate_task_id();
            if self
                .mesh_task_tx
                .send(MeshTask {
                    key,
                    task_id,
                    dependencies,
                    chunk,
                    neighbors,
                })
                .is_ok()
            {
                self.meshing.insert(key, task_id);
                scheduled += 1;
                if let Some(chunk) = self.chunks.get_mut(&key).map(|a| clone_arc(a)) {
                    chunk.is_dirty = false;
                }
            } else {
                overflow.push(key);
            }
        }

        self.dirty_keys.extend(overflow);
        rebuilt
    }

    /// Synchronously recompute lighting for the 3×3 scope around `dirty_keys`,
    /// then rebuild every chunk in that scope. Used when the player breaks or
    /// places a block so the visual update is immediate and uses correct light.
    pub fn rebuild_chunk_now(&mut self, cx: i32, cz: i32) -> Vec<(i32, i32)> {
        self.rebuild_chunks_now(&HashSet::from_iter([(cx, cz)]))
    }

    /// Synchronously recompute lighting for the 3×3 scope around `dirty_keys`,
    /// then rebuild every chunk in that scope.
    pub fn rebuild_chunks_now(
        &mut self,
        dirty_keys: &HashSet<(i32, i32)>,
    ) -> Vec<(i32, i32)> {
        if dirty_keys.is_empty() {
            return Vec::new();
        }
        let scope = lighting_scope_for(&self.chunks, dirty_keys);
        {
            let mut chunks: HashMap<(i32, i32), Arc<Chunk>> = HashMap::new();
            for key in &scope {
                if let Some(c) = self.chunks.get(key) {
                    chunks.insert(*key, Arc::clone(c));
                }
            }
            for &(cx, cz) in &scope {
                for (dx, dz) in &[(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nk = (cx + dx, cz + dz);
                    if !scope.contains(&nk) && self.chunks.contains_key(&nk) {
                        if let Some(c) = self.chunks.get(&nk) {
                            chunks.entry(nk).or_insert_with(|| Arc::clone(c));
                        }
                    }
                }
            }
            let _changed = recompute_lighting_on_snapshot(&mut chunks, dirty_keys);
            for key in &scope {
                if let Some(src) = chunks.get(key) {
                    if let Some(dst) = self.chunks.get_mut(key).map(|a| clone_arc(a)) {
                        dst.sky_light = src.sky_light.clone();
                        dst.block_light = src.block_light.clone();
                    }
                }
            }
        }
        for key in &scope {
            self.bump_chunk_revision(*key);
        }
        // Clear light-dirty flags so the async pipeline doesn't redo this work
        for key in &scope {
            if let Some(chunk) = self.chunks.get_mut(key).map(|a| clone_arc(a)) {
                chunk.light_dirty = false;
            }
            self.light_dirty_keys.remove(key);
        }

        // Rebuild meshes for all scope chunks with correct lighting
        for key in &scope {
            if let Some(chunk) = self.chunks.get(key) {
                let neighbor_fn =
                    |ncx: i32, ncz: i32| -> Option<&Chunk> { self.chunks.get(&(ncx, ncz)).map(|a| a.as_ref()) };
                let mesh = build_chunk_mesh(chunk.as_ref(), &neighbor_fn);
                self.meshes.insert(*key, mesh);
                self.meshing.remove(key);
                if let Some(chunk) = self.chunks.get_mut(key).map(|a| clone_arc(a)) {
                    chunk.is_dirty = false;
                    chunk.has_mesh = true;
                }
            }
            self.dirty_keys.remove(key);
        }
        scope.into_iter().collect()
    }

}

/// Compute the 3×3 chunk scope for a set of dirty-light keys, filtering to
/// only chunks that are present in `chunks`.
fn lighting_scope_for(
    chunks: &HashMap<(i32, i32), Arc<Chunk>>,
    dirty_keys: &HashSet<(i32, i32)>,
) -> HashSet<(i32, i32)> {
    dirty_keys
        .iter()
        .flat_map(|&(cx, cz)| {
            (-1..=1).flat_map(move |dx| (-1..=1).map(move |dz| (cx + dx, cz + dz)))
        })
        .filter(|key| chunks.contains_key(key))
        .collect()
}

/// Standalone version of the recompute-lighting flood fill that operates on a
/// snapshot HashMap of cloned chunks.  Returns the set of all chunk keys whose
/// light arrays were touched (the expanded 3×3 scope).
fn recompute_lighting_on_snapshot(
    chunks: &mut HashMap<(i32, i32), Arc<Chunk>>,
    dirty_keys: &HashSet<(i32, i32)>,
) -> HashSet<(i32, i32)> {
    const DIRS: [(i32, i32, i32); 6] = [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ];

    let relight_keys = lighting_scope_for(chunks, dirty_keys);

    for key in &relight_keys {
        let Some(chunk) = chunks.get_mut(key).map(|a| clone_arc(a)) else {
            continue;
        };
        chunk.sky_light.fill(0);
        chunk.block_light.fill(0);
    }

    let boundary_keys: HashSet<(i32, i32)> = relight_keys
        .iter()
        .flat_map(|&(cx, cz)| [(cx - 1, cz), (cx + 1, cz), (cx, cz - 1), (cx, cz + 1)])
        .filter(|key| {
            if relight_keys.contains(key) {
                return false;
            }
            chunks.get(key).is_some_and(|c| !c.light_dirty)
        })
        .collect();

    let mut sky_queue = VecDeque::new();
    let mut block_queue = VecDeque::new();

    let mut scope_ceiling = 0i32;
    for (cx, cz) in relight_keys.iter().copied() {
        let base_x = cx * CHUNK_SIZE as i32;
        let base_z = cz * CHUNK_SIZE as i32;
        if let Some(chunk) = chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) {
            let max_h = seed_chunk_light(chunk, base_x, base_z, &mut block_queue);
            scope_ceiling = scope_ceiling.max(max_h);
        }
    }

    for key in &boundary_keys {
        if let Some(chunk) = chunks.get(key) {
            seed_boundary_light(
                chunk,
                &relight_keys,
                scope_ceiling,
                &mut sky_queue,
                &mut block_queue,
            );
        }
    }

    for (cx, cz) in relight_keys.iter().copied() {
        let base_x = cx * CHUNK_SIZE as i32;
        let base_z = cz * CHUNK_SIZE as i32;
        if let Some(chunk) = chunks.get(&(cx, cz)) {
            queue_sky_light(chunk, base_x, base_z, scope_ceiling, &mut sky_queue);
        }
    }

    while let Some((x, y, z, light)) = sky_queue.pop_front() {
        for (dx, dy, dz) in DIRS {
            let nx = x + dx;
            let ny = y + dy;
            let nz = z + dz;
            if ny < 0 || ny >= CHUNK_HEIGHT as i32 {
                continue;
            }
            let ncx = nx.div_euclid(CHUNK_SIZE as i32);
            let ncz = nz.div_euclid(CHUNK_SIZE as i32);
            if !relight_keys.contains(&(ncx, ncz)) {
                continue;
            }
            let lx = nx.rem_euclid(CHUNK_SIZE as i32) as usize;
            let lz = nz.rem_euclid(CHUNK_SIZE as i32) as usize;
            let Some(chunk) = chunks.get(&(ncx, ncz)) else {
                continue;
            };
            let idx = Chunk::index(lx, ny as usize, lz);
            let opacity = sky_light_opacity(chunk.blocks[idx]);
            if opacity >= 15 {
                continue;
            }

            let next = if light == 15 && dy == -1 {
                15u8.saturating_sub(opacity)
            } else {
                light.saturating_sub(1 + opacity)
            };

            if next > 0 {
                let Some(chunk) = chunks.get_mut(&(ncx, ncz)).map(|a| clone_arc(a)) else {
                    continue;
                };
                if chunk.sky_light[idx] < next {
                    chunk.sky_light[idx] = next;
                    sky_queue.push_back((nx, ny, nz, next));
                }
            }
        }
    }

    while let Some((x, y, z, light)) = block_queue.pop_front() {
        for (dx, dy, dz) in DIRS {
            let nx = x + dx;
            let ny = y + dy;
            let nz = z + dz;
            if ny < 0 || ny >= CHUNK_HEIGHT as i32 {
                continue;
            }
            let ncx = nx.div_euclid(CHUNK_SIZE as i32);
            let ncz = nz.div_euclid(CHUNK_SIZE as i32);
            if !relight_keys.contains(&(ncx, ncz)) {
                continue;
            }
            let lx = nx.rem_euclid(CHUNK_SIZE as i32) as usize;
            let lz = nz.rem_euclid(CHUNK_SIZE as i32) as usize;
            let Some(chunk) = chunks.get(&(ncx, ncz)) else {
                continue;
            };
            let idx = Chunk::index(lx, ny as usize, lz);
            let opacity = block_light_opacity(chunk.blocks[idx]);
            if opacity >= 15 {
                continue;
            }
            let next = light.saturating_sub(1 + opacity);
            if next > 0 {
                let Some(chunk) = chunks.get_mut(&(ncx, ncz)).map(|a| clone_arc(a)) else {
                    continue;
                };
                if chunk.block_light[idx] < next {
                    chunk.block_light[idx] = next;
                    block_queue.push_back((nx, ny, nz, next));
                }
            }
        }
    }

    relight_keys
}

impl ChunkManager {
    #[allow(dead_code)]
    fn set_sky_light(&mut self, x: i32, y: i32, z: i32, light: u8) -> bool {
        let (cx, cz) = chunk_key(x, z);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) else {
            return false;
        };
        let idx = Chunk::index(lx, y as usize, lz);
        if chunk.sky_light[idx] >= light {
            return false;
        }
        chunk.sky_light[idx] = light;
        true
    }

    #[allow(dead_code)]
    fn set_block_light(&mut self, x: i32, y: i32, z: i32, light: u8) -> bool {
        let (cx, cz) = chunk_key(x, z);
        let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) else {
            return false;
        };
        let idx = Chunk::index(lx, y as usize, lz);
        if chunk.block_light[idx] >= light {
            return false;
        }
        chunk.block_light[idx] = light;
        true
    }

    pub fn get_chunk_mesh(&self, cx: i32, cz: i32) -> Option<&ChunkMesh> {
        self.meshes.get(&(cx, cz))
    }

    pub fn mark_chunk_dirty(&mut self, cx: i32, cz: i32) {
        if let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) {
            chunk.is_dirty = true;
            self.dirty_keys.insert((cx, cz));
        }
    }

    pub fn mark_chunk_light_dirty(&mut self, cx: i32, cz: i32) {
        self.chunk_revisions.entry((cx, cz)).or_insert(0);
        if let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) {
            chunk.light_dirty = true;
            self.light_dirty_keys.insert((cx, cz));
        }
    }

    pub fn absorb_water_sponge(&mut self, x: i32, y: i32, z: i32) -> bool {
        let mut absorbed = false;
        let radius = 2;
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    let bx = x + dx;
                    let by = y + dy;
                    let bz = z + dz;
                    if self.get_block(bx, by, bz).id == BlockId::Water {
                        self.set_block(bx, by, bz, Block::air());
                        absorbed = true;
                    }
                }
            }
        }
        absorbed
    }

    pub fn get_biome_name(&self, wx: f64, wz: f64) -> String {
        format!("{:?}", self.generator.get_biome(wx, wz))
    }

    fn apply_fluid_updates(&mut self, fluid_id: BlockId, updates: &[(i32, i32, i32, u8)]) {
        for &(x, y, z, data) in updates {
            let existing = self.get_block(x, y, z);
            if existing.is_air() {
                self.set_block_internal(
                    x,
                    y,
                    z,
                    Block::with_legacy_data(fluid_id, data),
                    true,
                );
            } else if existing.id == fluid_id && existing.data > data {
                self.set_block_internal(
                    x,
                    y,
                    z,
                    Block::with_legacy_data(fluid_id, data),
                    true,
                );
            }
        }
    }

    pub fn tick_lava(&mut self, player_cx: i32, player_cz: i32) {
        let range = self.render_distance + 2;
        let lava_keys: Vec<(i32, i32)> = self
            .chunks
            .iter()
            .filter(|((cx, cz), c)| {
                c.has_lava && (cx - player_cx).abs() <= range && (cz - player_cz).abs() <= range
            })
            .map(|(&k, _)| k)
            .collect();

        let mut updates: Vec<(i32, i32, i32, u8)> = Vec::new();
        let mut interactions: Vec<(i32, i32, i32)> = Vec::new();

        for &(cx, cz) in &lava_keys {
            // Ensure fluid positions are built
            if let Some(chunk) = self.chunks.get(&(cx, cz)) {
                if chunk.fluid_positions.is_empty() && chunk.has_lava {
                    if let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) {
                        chunk.recount_fluids();
                    }
                }
            }
            let chunk = match self.chunks.get(&(cx, cz)) {
                Some(c) => c,
                None => continue,
            };
            let base_x = cx * CHUNK_SIZE as i32;
            let base_z = cz * CHUNK_SIZE as i32;

            for &(lx, y, lz) in &chunk.fluid_positions {
                if y == 0 {
                    continue;
                }
                let x = lx as usize;
                let y = y as usize;
                let z = lz as usize;
                let idx = Chunk::index(x, y, z);
                let block = chunk.blocks[idx];
                if block.id != BlockId::Lava {
                    continue;
                }
                let level = block.data;
                let wx = base_x + lx as i32;
                let wz = base_z + lz as i32;
                let wy = y as i32;

                let below = chunk.blocks[Chunk::index(x, y - 1, z)];
                if below.is_air()
                    || (below.id != BlockId::Lava
                        && !below.id.is_solid()
                        && below.id != BlockId::Water)
                {
                    updates.push((wx, wy - 1, wz, level));
                    continue;
                }

                if level < 2 {
                    for (dx, dz) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        let neighbor = if nx >= 0
                            && nx < CHUNK_SIZE as i32
                            && nz >= 0
                            && nz < CHUNK_SIZE as i32
                        {
                            chunk.blocks[Chunk::index(nx as usize, y, nz as usize)]
                        } else {
                            self.get_block(wx + dx, wy, wz + dz)
                        };
                        if neighbor.is_air() || (neighbor.id == BlockId::Water) {
                            if neighbor.id == BlockId::Water {
                                interactions.push((wx + dx, wy, wz + dz));
                            } else {
                                updates.push((wx + dx, wy, wz + dz, level + 1));
                            }
                        } else if neighbor.id == BlockId::Lava && neighbor.data > level + 1 {
                            updates.push((wx + dx, wy, wz + dz, level + 1));
                        }
                    }
                }
            }
        }

        let mut rebuild_keys: HashSet<(i32, i32)> = HashSet::new();
        for (x, y, z) in &interactions {
            self.set_block(*x, *y, *z, Block::new(BlockId::Stone));
            rebuild_keys.insert(chunk_key(*x, *z));
        }

        self.apply_fluid_updates(BlockId::Lava, &updates);
        for &(x, _y, z, _) in &updates {
            rebuild_keys.insert(chunk_key(x, z));
        }
        for key in rebuild_keys {
            if let Some(chunk) = self.chunks.get_mut(&key).map(|a| clone_arc(a)) {
                chunk.recount_fluids();
            }
        }
    }

    pub fn tick_water(&mut self, player_cx: i32, player_cz: i32) {
        let range = self.render_distance + 2;
        let watery_keys: Vec<(i32, i32)> = self
            .chunks
            .iter()
            .filter(|((cx, cz), c)| {
                c.has_water && (cx - player_cx).abs() <= range && (cz - player_cz).abs() <= range
            })
            .map(|(&k, _)| k)
            .collect();

        let mut updates: Vec<(i32, i32, i32, u8)> = Vec::new();
        let mut lava_interactions: Vec<(i32, i32, i32, BlockId)> = Vec::new();

        for &(cx, cz) in &watery_keys {
            // Ensure fluid positions are built
            if let Some(chunk) = self.chunks.get(&(cx, cz)) {
                if chunk.fluid_positions.is_empty() && chunk.has_water {
                    if let Some(chunk) = self.chunks.get_mut(&(cx, cz)).map(|a| clone_arc(a)) {
                        chunk.recount_fluids();
                    }
                }
            }
            let chunk = match self.chunks.get(&(cx, cz)) {
                Some(c) => c,
                None => continue,
            };
            let base_x = cx * CHUNK_SIZE as i32;
            let base_z = cz * CHUNK_SIZE as i32;

            for &(lx, y, lz) in &chunk.fluid_positions {
                if y == 0 {
                    continue;
                }
                let x = lx as usize;
                let y = y as usize;
                let z = lz as usize;
                let idx = Chunk::index(x, y, z);
                let block = chunk.blocks[idx];
                if block.id != BlockId::Water {
                    continue;
                }
                let level = block.data;
                let wx = base_x + lx as i32;
                let wz = base_z + lz as i32;
                let wy = y as i32;

                let below_block = chunk.blocks[Chunk::index(x, y - 1, z)];
                let above_block = if y + 1 < CHUNK_HEIGHT {
                    chunk.blocks[Chunk::index(x, y + 1, z)]
                } else {
                    Block::air()
                };

                let lava_adjacent = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
                    let nx = x as i32 + dx;
                    let nz = z as i32 + dz;
                    if nx >= 0 && nx < CHUNK_SIZE as i32 && nz >= 0 && nz < CHUNK_SIZE as i32 {
                        chunk.blocks[Chunk::index(nx as usize, y, nz as usize)].id == BlockId::Lava
                            || (y + 1 < CHUNK_HEIGHT
                                && chunk.blocks[Chunk::index(nx as usize, y + 1, nz as usize)].id
                                    == BlockId::Lava)
                    } else {
                        self.get_block(wx + dx, wy, wz + dz).id == BlockId::Lava
                            || self.get_block(wx + dx, wy + 1, wz + dz).id == BlockId::Lava
                    }
                }) || below_block.id == BlockId::Lava
                    || above_block.id == BlockId::Lava;
                if lava_adjacent {
                    let result = if level == 0 {
                        BlockId::Obsidian
                    } else {
                        BlockId::Cobblestone
                    };
                    lava_interactions.push((wx, wy, wz, result));
                    continue;
                }

                if below_block.is_air()
                    || (below_block.id != BlockId::Water && !below_block.id.is_solid())
                {
                    updates.push((wx, wy - 1, wz, level));
                    continue;
                }

                if level < 7 {
                    for (dx, dz) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        let neighbor = if nx >= 0
                            && nx < CHUNK_SIZE as i32
                            && nz >= 0
                            && nz < CHUNK_SIZE as i32
                        {
                            chunk.blocks[Chunk::index(nx as usize, y, nz as usize)]
                        } else {
                            self.get_block(wx + dx, wy, wz + dz)
                        };
                        if neighbor.is_air() {
                            updates.push((wx + dx, wy, wz + dz, level + 1));
                        } else if neighbor.id == BlockId::Water && neighbor.data > level + 1 {
                            updates.push((wx + dx, wy, wz + dz, level + 1));
                        }
                    }
                }
            }
        }

        let mut rebuild_keys: HashSet<(i32, i32)> = HashSet::new();
        for (x, y, z, block_id) in &lava_interactions {
            self.set_block(*x, *y, *z, Block::new(*block_id));
            rebuild_keys.insert(chunk_key(*x, *z));
        }

        self.apply_fluid_updates(BlockId::Water, &updates);
        for &(x, _y, z, _) in &updates {
            rebuild_keys.insert(chunk_key(x, z));
        }
        for key in rebuild_keys {
            if let Some(chunk) = self.chunks.get_mut(&key).map(|a| clone_arc(a)) {
                chunk.recount_fluids();
            }
        }
    }
}

fn clone_arc<T: Clone>(arc: &mut Arc<T>) -> &mut T {
    if Arc::get_mut(arc).is_none() {
        *arc = Arc::new((**arc).clone());
    }
    Arc::get_mut(arc).unwrap()
}

fn chunk_key(x: i32, z: i32) -> (i32, i32) {
    (
        x.div_euclid(CHUNK_SIZE as i32),
        z.div_euclid(CHUNK_SIZE as i32),
    )
}

fn seed_chunk_light(
    chunk: &mut Chunk,
    base_x: i32,
    base_z: i32,
    block_queue: &mut VecDeque<(i32, i32, i32, u8)>,
) -> i32 {
    let mut max_highest_non_air = 0i32;
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let mut light = 15u8;
            let mut highest_non_air = 0i32;
            for y in (0..CHUNK_HEIGHT).rev() {
                let idx = Chunk::index(lx, y, lz);
                let block = chunk.blocks[idx];

                // Block light: emit inline instead of a separate triple-nested pass
                let bl = block.id.light_level();
                if bl > 0 {
                    chunk.block_light[idx] = bl;
                    block_queue.push_back((base_x + lx as i32, y as i32, base_z + lz as i32, bl));
                }

                if !block.is_air() && highest_non_air == 0 {
                    highest_non_air = y as i32 + 1;
                    max_highest_non_air = max_highest_non_air.max(highest_non_air);
                }

                // Sky light: apply attenuation first, then store. Setting before the
                // opacity check would give light-filtering blocks (leaves, water) sky=15,
                // which queue_sky_light would then enqueue as a full-sky source, causing
                // the BFS to beam 15 through them downward (the light==15 && dy==-1
                // special case) and reinflate the block below.
                let opacity = sky_light_opacity(block);
                if opacity >= 15 {
                    light = 0;
                } else if opacity > 0 {
                    light = light.saturating_sub(opacity);
                }
                chunk.sky_light[idx] = light;
            }
        }
    }
    max_highest_non_air
}

fn queue_sky_light(
    chunk: &Chunk,
    base_x: i32,
    base_z: i32,
    scope_ceiling: i32,
    sky_queue: &mut VecDeque<(i32, i32, i32, u8)>,
) {
    let max_y = (scope_ceiling as usize).min(CHUNK_HEIGHT);
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            for y in 0..max_y {
                let idx = Chunk::index(lx, y, lz);
                let light = chunk.sky_light[idx];
                if light > 0 && light_can_pass(chunk.blocks[idx]) {
                    sky_queue.push_back((base_x + lx as i32, y as i32, base_z + lz as i32, light));
                }
            }
        }
    }
}

fn seed_boundary_light(
    chunk: &Chunk,
    relight_keys: &HashSet<(i32, i32)>,
    scope_ceiling: i32,
    sky_queue: &mut VecDeque<(i32, i32, i32, u8)>,
    block_queue: &mut VecDeque<(i32, i32, i32, u8)>,
) {
    let base_x = chunk.cx * CHUNK_SIZE as i32;
    let base_z = chunk.cz * CHUNK_SIZE as i32;
    let max_y = (scope_ceiling as usize).min(CHUNK_HEIGHT);

    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let wx = base_x + lx as i32;
            let wz = base_z + lz as i32;
            let touches_scope = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dz)| relight_keys.contains(&chunk_key(wx + dx, wz + dz)));
            if !touches_scope {
                continue;
            }

            // Seed skylight up to scope_ceiling (above that is always 15, no-op to queue)
            for y in 0..max_y {
                let idx = Chunk::index(lx, y, lz);
                let sky = chunk.sky_light[idx];
                if sky > 0 {
                    sky_queue.push_back((wx, y as i32, wz, sky));
                }
            }
            // Seed block light at all heights (torches can be anywhere)
            for y in 0..CHUNK_HEIGHT {
                let idx = Chunk::index(lx, y, lz);
                let block = chunk.block_light[idx];
                if block > 0 {
                    block_queue.push_back((wx, y as i32, wz, block));
                }
            }
        }
    }
}

fn light_can_pass(block: Block) -> bool {
    sky_light_opacity(block) < 15
}

fn sky_light_opacity(block: Block) -> u8 {
    match block.id {
        BlockId::Air => 0,
        // Tinted glass is visually transparent but deliberately blocks light.
        BlockId::TintedGlass => 15,
        BlockId::Water | BlockId::Lava | BlockId::Ice => 1,
        BlockId::OakLeaves
        | BlockId::OakLeaves2
        | BlockId::SpruceLeaves
        | BlockId::BirchLeaves
        | BlockId::JungleLeaves
        | BlockId::AcaciaLeaves
        | BlockId::DarkOakLeaves
        | BlockId::CherryLeaves
        | BlockId::MangroveLeaves
        | BlockId::AzaleaLeaves
        | BlockId::FloweringAzaleaLeaves => 1,
        BlockId::StoneSlab | BlockId::OakSlab | BlockId::StoneStairs | BlockId::OakStairs => 1,
        _ if block.id.is_transparent() || !block.id.is_solid() => 0,
        _ => 15,
    }
}

fn block_light_opacity(block: Block) -> u8 {
    match block.id {
        BlockId::Water
        | BlockId::Lava
        | BlockId::Ice
        | BlockId::OakLeaves
        | BlockId::OakLeaves2
        | BlockId::SpruceLeaves
        | BlockId::BirchLeaves
        | BlockId::JungleLeaves
        | BlockId::AcaciaLeaves
        | BlockId::DarkOakLeaves
        | BlockId::CherryLeaves
        | BlockId::MangroveLeaves
        | BlockId::AzaleaLeaves
        | BlockId::FloweringAzaleaLeaves => 0,
        BlockId::StoneSlab | BlockId::OakSlab | BlockId::StoneStairs | BlockId::OakStairs => 1,
        BlockId::Air => 0,
        BlockId::TintedGlass => 15,
        _ if block.id.is_transparent() || !block.id.is_solid() => 0,
        _ => 15,
    }
}

fn light_signature(block: Block) -> (u8, u8, u8) {
    (
        sky_light_opacity(block),
        block_light_opacity(block),
        block.id.light_level(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemStack;
    use crate::world::persistence::WorldStorage;
    use std::fs;

    #[test]
    fn torch_light_spreads_one_level_per_air_block() {
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(8, 20, 8, Block::new(BlockId::Torch));
        let mut chunks = HashMap::from([((0, 0), Arc::new(chunk))]);
        let changed = recompute_lighting_on_snapshot(&mut chunks, &HashSet::from([(0, 0)]));
        assert_eq!(changed, HashSet::from([(0, 0)]));
        let chunk = chunks[&(0, 0)].as_ref();
        assert_eq!(chunk.block_light[Chunk::index(8, 20, 8)], 14);
        assert_eq!(chunk.block_light[Chunk::index(9, 20, 8)], 13);
    }

    #[test]
    fn placement_builds_door_pair_and_connects_fences() {
        let mut manager = ChunkManager::new(7, 1);
        manager.chunks.insert((0, 0), Arc::new(Chunk::new(0, 0)));
        assert!(manager.place_block(2, 40, 2, BlockId::OakDoor));
        assert_eq!(manager.get_block(2, 40, 2).id, BlockId::OakDoor);
        assert_eq!(manager.get_block(2, 41, 2).id, BlockId::OakDoor);
        manager.set_block(2, 40, 2, Block::air());
        assert!(manager.get_block(2, 41, 2).is_air());
        assert!(manager.place_block(5, 40, 5, BlockId::OakFence));
        assert!(manager.place_block(6, 40, 5, BlockId::OakFence));
        let properties = registry().properties_for_state(BlockId::OakFence, manager.get_block(5, 40, 5).state).unwrap();
        assert!(properties.contains(&("east", "true")));
        assert!(manager.place_block(9, 40, 9, BlockId::RedstoneDust));
        assert!(manager.place_block(10, 40, 9, BlockId::RedstoneDust));
        let properties = registry().properties_for_state(BlockId::RedstoneDust, manager.get_block(9, 40, 9).state).unwrap();
        assert!(properties.contains(&("east", "side")));
    }

    #[test]
    fn authoritative_snapshots_reject_stale_revisions() {
        let mut source = Chunk::new(0, 0);
        source.set_block(2, 40, 2, Block::new(BlockId::DiamondBlock));
        let snapshot = ChunkData::from_chunk(&source);
        let mut manager = ChunkManager::new(7, 1);

        assert!(manager.apply_chunk_data(snapshot.clone(), 5).unwrap());
        assert_eq!(manager.get_block(2, 40, 2).id, BlockId::DiamondBlock);
        assert!(!manager.apply_chunk_data(snapshot, 5).unwrap());
        assert!(!manager.apply_block_state(2, 40, 2, Block::air(), 4));
        assert_eq!(manager.get_block(2, 40, 2).id, BlockId::DiamondBlock);
        assert!(manager.apply_block_state(3, 40, 2, Block::new(BlockId::Stone), 5));
        assert!(manager.apply_block_state(2, 40, 2, Block::air(), 6));
        assert!(manager.get_block(2, 40, 2).is_air());
    }

    #[test]
    fn derived_lighting_does_not_change_authoritative_revision() {
        let source = Chunk::new(0, 0);
        let mut manager = ChunkManager::new(7, 1);
        assert!(manager.apply_chunk_data(ChunkData::from_chunk(&source), 7).unwrap());

        for _ in 0..200 {
            manager.rebuild_dirty_meshes();
            if manager.get_chunk(0, 0).is_some_and(|chunk| !chunk.light_dirty) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert_eq!(manager.chunk_revision(0, 0), Some(7));
    }

    #[test]
    fn authoritative_session_reset_accepts_revisions_from_new_server() {
        let mut source = Chunk::new(0, 0);
        source.set_block(2, 40, 2, Block::new(BlockId::DiamondBlock));
        let snapshot = ChunkData::from_chunk(&source);
        let mut manager = ChunkManager::new(7, 1);

        assert!(manager.apply_chunk_data(snapshot.clone(), 5).unwrap());
        assert_eq!(manager.get_block(2, 40, 2).id, BlockId::DiamondBlock);

        manager.reset_authoritative_session();

        assert!(manager.apply_chunk_data(snapshot, 1).unwrap());
        assert_eq!(manager.get_block(2, 40, 2).id, BlockId::DiamondBlock);
    }

    #[test]
    fn authoritative_unload_allows_lower_revision_on_reentry() {
        let mut source = Chunk::new(0, 0);
        source.set_block(2, 40, 2, Block::new(BlockId::DiamondBlock));
        let snapshot = ChunkData::from_chunk(&source);
        let mut manager = ChunkManager::new(7, 1);

        assert!(manager.apply_chunk_data(snapshot.clone(), 9).unwrap());
        assert!(manager.unload_authoritative_chunk(0, 0));
        assert!(manager.get_chunk(0, 0).is_none());
        assert!(manager.apply_chunk_data(snapshot, 1).unwrap());
        assert_eq!(manager.chunk_revision(0, 0), Some(1));
    }

    #[test]
    fn block_entity_api_uses_world_coordinates_and_block_lifecycle() {
        let mut manager = ChunkManager::new(7, 1);
        manager.chunks.insert((-1, -1), Arc::new(Chunk::new(-1, -1)));
        manager.set_block(-1, 30, -1, Block::new(BlockId::Chest));

        let mut chest = manager.get_block_entity(-1, 30, -1).unwrap().clone();
        let BlockEntity::Chest { slots } = &mut chest else { unreachable!() };
        slots.slots[2] = ItemStack::new(1, 8);
        assert!(manager.set_block_entity(-1, 30, -1, chest));
        let BlockEntity::Chest { slots } = manager.get_block_entity(-1, 30, -1).unwrap() else {
            panic!("chest state should be available at negative coordinates");
        };
        assert_eq!(slots.slots[2], ItemStack::new(1, 8));

        manager.set_block(-1, 30, -1, Block::new(BlockId::Stone));
        assert!(manager.get_block_entity(-1, 30, -1).is_none());
    }

    #[test]
    fn changed_chunk_is_saved_before_streaming_unload() {
        let path = std::env::temp_dir().join(format!(
            "vibecraft-chunk-save-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = WorldStorage::new(&path);
        let mut manager = ChunkManager::new(7, 1);
        manager.set_storage(storage.clone());
        manager.chunks.insert((0, 0), Arc::new(Chunk::new(0, 0)));
        manager.set_block(1, 30, 2, Block::new(BlockId::DiamondBlock));

        manager.update_chunks_async(10, 10);

        assert!(!manager.chunks.contains_key(&(0, 0)));
        let saved = storage.load_chunk(0, 0).unwrap().into_chunk().unwrap();
        assert_eq!(saved.get_block(1, 30, 2).id, BlockId::DiamondBlock);
        let _ = fs::remove_dir_all(path);
    }
}
