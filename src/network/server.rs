//! Headless authoritative server runtime.
//!
//! This is the headless multiplayer authority: it owns the fixed-rate world
//! and persistence loop, accepts bounded protocol sessions, and never
//! initializes a window or GPU. Clients send intent; movement, chunk
//! replication, block edits, inventory actions, and player persistence remain
//! server-owned.

use super::{
    decode_client_with_limits, encode_chunk_data, encode_server_with_limits, BlockEditAction,
    ClientMessage, DisconnectCode, Face, FrameDecoder, ProtocolError, ProtocolLimits, RejectCode,
    ServerMessage, SessionGuard, WireBlockState, MAX_CHAT_BYTES,
};
use crate::inventory::item::ItemRegistry;
use crate::inventory::progression::mining_outcome;
use crate::inventory::{Inventory, ItemStack, EMPTY_STACK};
use crate::player::{Player, GRAVITY, JUMP_SPEED, SPRINT_MULT, SNEAK_SPEED, WALK_SPEED};
use crate::world::block::{Block, BlockId};
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk_manager::ChunkManager;
use crate::world::persistence::{LevelData, NamedPlayerData, PlayerData, StorageError, WorldStorage};
use crate::world::simulation::{FixedStepClock, ScheduledTickKind, TickScheduler};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:25565";
const DEFAULT_WORLD_DIRECTORY: &str = "server-world";
const DEFAULT_RENDER_DISTANCE: i32 = 6;
const DEFAULT_MAX_PLAYERS: usize = 8;
const DEFAULT_AUTOSAVE_SECONDS: u64 = 30;
const MAX_RENDER_DISTANCE: i32 = 32;
const MIN_RENDER_DISTANCE: i32 = 2;
const MAX_PLAYERS: usize = 64;
const SOCKET_READ_BUFFER_BYTES: usize = 16 * 1024;
const MAX_OUTBOUND_FRAMES: usize = 128;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_TICK_SLEEP: Duration = Duration::from_millis(1);

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub world_dir: PathBuf,
    pub seed: Option<u64>,
    pub render_distance: i32,
    pub max_players: usize,
    pub autosave_interval: Duration,
    pub idle_timeout: Duration,
    pub protocol_limits: ProtocolLimits,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: DEFAULT_BIND_ADDRESS
                .parse()
                .expect("default server bind address is valid"),
            world_dir: PathBuf::from(DEFAULT_WORLD_DIRECTORY),
            seed: None,
            render_distance: DEFAULT_RENDER_DISTANCE,
            max_players: DEFAULT_MAX_PLAYERS,
            autosave_interval: Duration::from_secs(DEFAULT_AUTOSAVE_SECONDS),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            protocol_limits: ProtocolLimits::default(),
        }
    }
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), ServerError> {
        if !(MIN_RENDER_DISTANCE..=MAX_RENDER_DISTANCE).contains(&self.render_distance) {
            return Err(ServerError::Config(format!(
                "render distance must be between {MIN_RENDER_DISTANCE} and {MAX_RENDER_DISTANCE}"
            )));
        }
        if !(1..=MAX_PLAYERS).contains(&self.max_players) {
            return Err(ServerError::Config(format!(
                "max players must be between 1 and {MAX_PLAYERS}"
            )));
        }
        if self.autosave_interval.is_zero() {
            return Err(ServerError::Config(
                "autosave interval must be greater than zero".to_string(),
            ));
        }
        if self.idle_timeout.is_zero() {
            return Err(ServerError::Config(
                "idle timeout must be greater than zero".to_string(),
            ));
        }
        if self.protocol_limits.max_frame_payload == 0
            || self.protocol_limits.max_chunk_payload == 0
            || self.protocol_limits.max_chat_bytes == 0
            || self.protocol_limits.max_username_bytes == 0
            || self.protocol_limits.max_inventory_slots == 0
            || self.protocol_limits.max_messages_per_second == 0
        {
            return Err(ServerError::Config(
                "protocol limits must all be greater than zero".to_string(),
            ));
        }
        Ok(())
    }

    /// Parses server-only options without pulling window or renderer setup
    /// into the headless binary.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, ServerError> {
        let args: Vec<_> = args.into_iter().collect();
        let mut config = Self::default();
        let mut index = 0;
        while index < args.len() {
            let option = &args[index];
            match option.as_str() {
                "--bind" => {
                    config.bind_addr = next_arg(&args, &mut index, option)?
                        .parse()
                        .map_err(|_| ServerError::Config(format!("invalid --bind value")))?;
                }
                "--world-dir" => {
                    config.world_dir = PathBuf::from(next_arg(&args, &mut index, option)?);
                }
                "--seed" => {
                    config.seed = Some(parse_arg(&args, &mut index, option)?);
                }
                "--render-distance" => {
                    config.render_distance = parse_arg(&args, &mut index, option)?;
                }
                "--max-players" => {
                    config.max_players = parse_arg(&args, &mut index, option)?;
                }
                "--autosave-seconds" => {
                    let seconds: u64 = parse_arg(&args, &mut index, option)?;
                    config.autosave_interval = Duration::from_secs(seconds);
                }
                "--help" | "-h" => {
                    return Err(ServerError::HelpRequested);
                }
                value => {
                    return Err(ServerError::Config(format!(
                        "unknown server option `{value}`\n\n{}",
                        server_usage()
                    )));
                }
            }
            index += 1;
        }
        config.validate()?;
        Ok(config)
    }
}

fn next_arg(args: &[String], index: &mut usize, option: &str) -> Result<String, ServerError> {
    *index += 1;
    args.get(*index).cloned().ok_or_else(|| {
        ServerError::Config(format!(
            "missing value for `{option}`\n\n{}",
            server_usage()
        ))
    })
}

fn parse_arg<T: std::str::FromStr>(
    args: &[String],
    index: &mut usize,
    option: &str,
) -> Result<T, ServerError> {
    let value = next_arg(args, index, option)?;
    value
        .parse()
        .map_err(|_| ServerError::Config(format!("invalid {option} value `{value}`")))
}

pub fn server_usage() -> &'static str {
    "Usage: vibecraft-server [--bind IP:PORT] [--world-dir PATH] [--seed U64] [--render-distance 2..32] [--max-players 1..64] [--autosave-seconds N]"
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("help requested")]
    HelpRequested,
    #[error("server I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("world storage failed: {0}")]
    Storage(#[from] StorageError),
    #[error("server configuration invalid: {0}")]
    Config(String),
    #[error("one or more changed chunks could not be saved")]
    ChunkSaveFailed,
}

#[derive(Clone, Debug)]
pub struct ShutdownToken(Arc<AtomicBool>);

impl ShutdownToken {
    pub fn request_shutdown(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub struct HeadlessServer {
    config: ServerConfig,
    listener: TcpListener,
    storage: WorldStorage,
    level: LevelData,
    world: ChunkManager,
    scheduler: TickScheduler,
    item_registry: ItemRegistry,
    sessions: HashMap<u64, ClientConnection>,
    next_session_id: u64,
    shutdown: ShutdownToken,
}

impl HeadlessServer {
    pub fn bind(config: ServerConfig) -> Result<Self, ServerError> {
        config.validate()?;
        let listener = TcpListener::bind(config.bind_addr)?;
        listener.set_nonblocking(true)?;

        let storage = WorldStorage::new(config.world_dir.clone());
        let requested_seed = config.seed.unwrap_or_else(random_seed);
        let level = storage.load_or_create_level(LevelData {
            seed: requested_seed,
            tick: 0,
            game_time: 9_000,
            spawn: [0, 75, 0],
            gamemode: "survival".to_string(),
            difficulty: "normal".to_string(),
            hardcore: false,
            do_daylight_cycle: true,
            keep_inventory: false,
            experience: 0,
            scheduled_ticks: Vec::new(),
            dropped_items: Vec::new(),
            xp_orbs: Vec::new(),
            players: Vec::new(),
        })?;
        if config.seed.is_some_and(|seed| seed != level.seed) {
            log::warn!(
                "ignoring requested server seed because existing world {} uses seed {}",
                config.world_dir.display(),
                level.seed
            );
        }

        let mut world = ChunkManager::new(level.seed, config.render_distance);
        world.set_storage(storage.clone());
        let spawn_chunk = (
            level.spawn[0].div_euclid(CHUNK_SIZE as i32),
            level.spawn[2].div_euclid(CHUNK_SIZE as i32),
        );
        world.update_chunks_async(spawn_chunk.0, spawn_chunk.1);

        Ok(Self {
            config,
            listener,
            storage,
            scheduler: TickScheduler::from_events(level.scheduled_ticks.clone()),
            level,
            world,
            item_registry: ItemRegistry::new(),
            sessions: HashMap::new(),
            next_session_id: 1,
            shutdown: ShutdownToken(Arc::new(AtomicBool::new(false))),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ServerError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn shutdown_token(&self) -> ShutdownToken {
        self.shutdown.clone()
    }

    pub fn server_tick(&self) -> u64 {
        self.level.tick
    }

    pub fn connected_sessions(&self) -> usize {
        self.sessions.len()
    }

    pub fn loaded_chunks(&self) -> usize {
        self.world.stats().loaded
    }

    /// Advances exactly one authoritative 20 TPS step.
    pub fn tick(&mut self) {
        self.level.tick = self.level.tick.wrapping_add(1);
        if self.level.do_daylight_cycle {
            self.level.game_time = self.level.game_time.wrapping_add(1) % (1200 * 20);
        }

        let spawn_chunk = (
            self.level.spawn[0].div_euclid(CHUNK_SIZE as i32),
            self.level.spawn[2].div_euclid(CHUNK_SIZE as i32),
        );
        let mut stream_centers = vec![spawn_chunk];
        stream_centers.extend(self.sessions.values().filter_map(|session| {
            let player = &session.player.as_ref()?.player;
            Some((
                (player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                (player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            ))
        }));
        stream_centers.sort_unstable();
        stream_centers.dedup();
        self.world.update_chunks_async_for_centers(&stream_centers);
        self.world.process_loaded_chunks();
        let _ = self.world.tick_block_entities(&self.item_registry);

        for event in self.scheduler.drain_due(self.level.tick) {
            if self
                .world
                .get_chunk(event.chunk[0], event.chunk[1])
                .is_none()
            {
                self.scheduler
                    .schedule(crate::world::simulation::ScheduledTick {
                        due_tick: self.level.tick + 1,
                        ..event
                    });
                continue;
            }
            match event.kind {
                ScheduledTickKind::Water => {
                    self.world.tick_water(event.chunk[0], event.chunk[1]);
                }
                ScheduledTickKind::Lava => {
                    self.world.tick_lava(event.chunk[0], event.chunk[1]);
                }
                // The random-tick data model exists, but block-specific random
                // behavior is not implemented yet. Keep the event durable.
                ScheduledTickKind::Random => {
                    self.scheduler
                        .schedule(crate::world::simulation::ScheduledTick {
                            due_tick: self.level.tick + 1,
                            ..event
                        })
                }
            }
        }

        self.simulate_players();
        self.stream_loaded_chunks();
    }

    /// Polls connection accept/read/write work without blocking the world
    /// tick. It is useful both to the fixed-rate loop and to deterministic
    /// in-process tests.
    pub fn poll(&mut self) -> Result<usize, ServerError> {
        self.accept_pending()?;
        let now = Instant::now();
        let ids: Vec<_> = self.sessions.keys().copied().collect();
        let mut handled = 0;

        for id in ids {
            if !self.sessions.contains_key(&id) {
                continue;
            }
            let read_result = {
                let session = self
                    .sessions
                    .get_mut(&id)
                    .expect("session ID was collected");
                if now
                    .checked_duration_since(session.last_activity)
                    .unwrap_or_default()
                    >= self.config.idle_timeout
                {
                    session.close_after_flush = true;
                    queue_message(
                        session,
                        ServerMessage::Disconnect {
                            code: DisconnectCode::Timeout,
                            message: "connection timed out".to_string(),
                        },
                        self.config.protocol_limits,
                    );
                    ReadResult::Noop
                } else if session.close_after_flush {
                    ReadResult::Noop
                } else {
                    session.read_frames()
                }
            };

            match read_result {
                ReadResult::Frames(frames) => {
                    for frame in frames {
                        handled += 1;
                        self.process_frame(id, &frame);
                        if self
                            .sessions
                            .get(&id)
                            .is_some_and(|session| session.close_after_flush)
                        {
                            break;
                        }
                    }
                }
                ReadResult::Protocol(error) => {
                    handled += 1;
                    if let Some(session) = self.sessions.get_mut(&id) {
                        let code = if matches!(&error, ProtocolError::UnsupportedVersion(_)) {
                            DisconnectCode::UnsupportedVersion
                        } else {
                            DisconnectCode::MalformedMessage
                        };
                        queue_message(
                            session,
                            ServerMessage::Disconnect {
                                code,
                                message: bounded_message(&error.to_string(), MAX_CHAT_BYTES),
                            },
                            self.config.protocol_limits,
                        );
                        session.close_after_flush = true;
                    }
                }
                ReadResult::Closed => {
                    self.remove_session(id);
                    continue;
                }
                ReadResult::Io(error) => {
                    log::debug!("client session {id} read failed: {error}");
                    self.remove_session(id);
                    continue;
                }
                ReadResult::Noop => {}
            }

            let remove = if let Some(session) = self.sessions.get_mut(&id) {
                if !session.flush() {
                    true
                } else {
                    session.close_after_flush && session.outbox.is_empty()
                }
            } else {
                false
            };
            if remove {
                self.remove_session(id);
            }
        }
        Ok(handled)
    }

    /// Saves changed chunks before level metadata, preserving the existing
    /// atomic persistence contract. A failed chunk write prevents the level
    /// envelope from claiming the save completed.
    pub fn save(&mut self) -> Result<(), ServerError> {
        if !self.world.flush_saved_chunks() {
            return Err(ServerError::ChunkSaveFailed);
        }
        self.persist_active_players();
        self.level.scheduled_ticks = self.scheduler.events();
        self.storage.save_level(&self.level)?;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), ServerError> {
        self.shutdown.request_shutdown();
        self.save()
    }

    /// Runs the server until its shutdown token is requested. Rendering is
    /// absent; the fixed-step clock is the only source of simulation time.
    pub fn run_until_shutdown(&mut self, token: &ShutdownToken) -> Result<(), ServerError> {
        let mut clock = FixedStepClock::new();
        let mut last_frame = Instant::now();
        let mut last_save = last_frame;

        while !token.is_requested() {
            self.poll()?;
            let now = Instant::now();
            let elapsed = now
                .checked_duration_since(last_frame)
                .unwrap_or_default()
                .as_secs_f32();
            last_frame = now;
            for _ in 0..clock.advance(elapsed) {
                self.tick();
            }

            if now.checked_duration_since(last_save).unwrap_or_default()
                >= self.config.autosave_interval
            {
                if let Err(error) = self.save() {
                    log::error!("server autosave failed; keeping process alive: {error}");
                } else {
                    last_save = now;
                }
            }
            thread::sleep(SERVER_TICK_SLEEP);
        }
        self.save()
    }

    fn accept_pending(&mut self) -> Result<(), ServerError> {
        loop {
            match self.listener.accept() {
                Ok((mut stream, address)) => {
                    if self.sessions.len() >= self.config.max_players {
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                        let _ = write_one_shot(
                            &mut stream,
                            ServerMessage::Disconnect {
                                code: DisconnectCode::ServerFull,
                                message: "server is full".to_string(),
                            },
                            self.config.protocol_limits,
                        );
                        continue;
                    }
                    if let Err(error) = stream.set_nonblocking(true) {
                        log::warn!("failed to configure client {address}: {error}");
                        continue;
                    }
                    let id = self.next_session_id;
                    self.next_session_id = self.next_session_id.wrapping_add(1).max(1);
                    log::info!("accepted client {address} as session {id}");
                    self.sessions.insert(
                        id,
                        ClientConnection::new(stream, self.config.protocol_limits, Instant::now()),
                    );
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(ServerError::Io(error)),
            }
        }
    }

    fn process_frame(&mut self, id: u64, frame: &[u8]) {
        let message = match decode_client_with_limits(frame, self.config.protocol_limits) {
            Ok(message) => message,
            Err(error) => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    let code = if matches!(&error, ProtocolError::UnsupportedVersion(_)) {
                        DisconnectCode::UnsupportedVersion
                    } else {
                        DisconnectCode::MalformedMessage
                    };
                    queue_message(
                        session,
                        ServerMessage::Disconnect {
                            code,
                            message: bounded_message(&error.to_string(), MAX_CHAT_BYTES),
                        },
                        self.config.protocol_limits,
                    );
                    session.close_after_flush = true;
                }
                return;
            }
        };

        let request_id = request_id(&message);
        let guard_result = self
            .sessions
            .get_mut(&id)
            .map(|session| session.guard.accept(&message, Instant::now()));
        let Some(guard_result) = guard_result else {
            return;
        };

        if let Err(error) = guard_result {
            if let Some(session) = self.sessions.get_mut(&id) {
                queue_message(
                    session,
                    ServerMessage::Reject {
                        request_id,
                        code: reject_code(&error),
                        message: bounded_message(&error.to_string(), MAX_CHAT_BYTES),
                    },
                    self.config.protocol_limits,
                );
                if matches!(error, ProtocolError::SessionClosing) {
                    session.close_after_flush = true;
                }
            }
            return;
        }

        match message {
            ClientMessage::Hello { username, .. } => self.handle_hello(id, username),
            ClientMessage::KeepAlive { nonce } => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    queue_message(session, ServerMessage::KeepAlive { nonce }, self.config.protocol_limits);
                }
            }
            ClientMessage::Disconnect { .. } => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    queue_message(
                        session,
                        ServerMessage::Disconnect {
                            code: DisconnectCode::ClientQuit,
                            message: "client requested disconnect".to_string(),
                        },
                        self.config.protocol_limits,
                    );
                    session.close_after_flush = true;
                }
            }
            ClientMessage::Chat { message } => self.broadcast_chat(id, message),
            ClientMessage::Input { sequence: _, movement, yaw, pitch, jump, sprint, sneak } => {
                self.handle_input(id, movement, yaw, pitch, jump, sprint, sneak);
            }
            ClientMessage::BlockEditRequest { request_id, position, face, action, expected_revision } => {
                self.handle_block_edit(id, request_id, position, face, action, expected_revision);
            }
            ClientMessage::InventoryActionRequest { request_id, slot, action, expected_revision } => {
                self.handle_inventory_action(id, request_id, slot, action, expected_revision);
            }
        }
    }

    fn unique_username(&self, requested: &str) -> String {
        let active = |candidate: &str| {
            self.sessions
                .values()
                .any(|session| session.username.as_deref() == Some(candidate))
        };
        if !active(requested) {
            return requested.to_string();
        }

        let max_bytes = self.config.protocol_limits.max_username_bytes;
        for suffix_number in 2..1000 {
            let suffix = format!("-{suffix_number}");
            let mut base = requested.to_string();
            while base.len() + suffix.len() > max_bytes {
                if base.pop().is_none() {
                    break;
                }
            }
            let candidate = format!("{base}{suffix}");
            if !active(&candidate) {
                return candidate;
            }
        }

        // max_players is bounded well below this loop limit, so this is only
        // a defensive fallback for an invalid server configuration.
        requested.to_string()
    }

    fn handle_hello(&mut self, id: u64, requested_username: String) {
        let username = self.unique_username(&requested_username);
        let (player, inventory) = match self.level.players.iter().find(|saved| saved.username == username) {
            Some(saved) => match saved.player.clone().into_runtime() {
                Ok(state) => state,
                Err(error) => {
                    log::warn!("could not restore player {username}: {error}");
                    (Player::new(self.level.spawn[0] as f32, self.level.spawn[1] as f32, self.level.spawn[2] as f32), Inventory::new())
                }
            },
            None => {
                let mut inventory = Inventory::new();
                let stone = self.item_registry.item_id_from_block(BlockId::Stone);
                inventory.add_item(stone, 64, &self.item_registry);
                (Player::new(self.level.spawn[0] as f32, self.level.spawn[1] as f32, self.level.spawn[2] as f32), inventory)
            }
        };
        let spawn = [player.x, player.y, player.z];
        let state = PlayerSessionState {
            player,
            inventory,
            input: InputIntent::default(),
            yaw: 0.0,
            pitch: 0.0,
            inventory_revision: 0,
            cursor: EMPTY_STACK,
            sent_chunks: HashSet::new(),
            pending_chunks: VecDeque::new(),
        };
        let existing: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|(&player_id, peer)| {
                if player_id == id { return None; }
                let state = peer.player.as_ref()?;
                Some((player_id, peer.username.clone()?, [state.player.x as f64, state.player.y as f64, state.player.z as f64]))
            })
            .collect();
        let welcome = ServerMessage::Welcome {
            session_id: id,
            username: username.clone(),
            world_seed: self.level.seed,
            spawn: spawn.map(f64::from),
            server_tick: self.level.tick,
            view_distance: self.config.render_distance as u8,
        };
        let own_spawn = ServerMessage::PlayerSpawn {
            player_id: id,
            username: username.clone(),
            position: [spawn[0] as f64, spawn[1] as f64, spawn[2] as f64],
        };
        let limits = self.config.protocol_limits;
        let Some(session) = self.sessions.get_mut(&id) else { return; };
        session.username = Some(username.clone());
        session.player = Some(state);
        queue_message(session, welcome, limits);
        let snapshot = inventory_snapshot(session.player.as_ref().unwrap());
        queue_message(session, snapshot, limits);
        for (player_id, peer_username, position) in existing {
            queue_message(
                session,
                ServerMessage::PlayerSpawn {
                    player_id,
                    username: peer_username,
                    position,
                },
                limits,
            );
        }
        queue_message(session, own_spawn.clone(), limits);
        for (&peer_id, peer) in &mut self.sessions {
            if peer_id != id && peer.player.is_some() {
                queue_message(peer, own_spawn.clone(), limits);
            }
        }
    }

    fn handle_input(&mut self, id: u64, movement: [f32; 3], yaw: f32, pitch: f32, jump: bool, sprint: bool, sneak: bool) {
        if let Some(state) = self.sessions.get_mut(&id).and_then(|session| session.player.as_mut()) {
            state.input = InputIntent { movement, yaw, pitch, jump, sprint, sneak };
        }
    }

    fn simulate_players(&mut self) {
        let mut updates = Vec::new();
        for (&id, session) in &mut self.sessions {
            let Some(state) = session.player.as_mut() else { continue; };
            let input = state.input;
            state.yaw = input.yaw;
            state.pitch = input.pitch;
            state.player.sneaking = input.sneak;
            let dt = 0.05;
            let in_water = state.player.is_in_water(&self.world);
            if in_water {
                state.player.update_water_vertical_velocity(input.jump, input.sneak, dt);
            } else {
                if input.jump && state.player.on_ground {
                    state.player.vy = JUMP_SPEED;
                    state.player.on_ground = false;
                }
                state.player.vy = (state.player.vy + GRAVITY * dt).max(crate::player::TERMINAL_VELOCITY);
            }
            let speed = if in_water {
                crate::player::SWIM_SPEED
            } else if input.sneak {
                SNEAK_SPEED
            } else if input.sprint {
                WALK_SPEED * SPRINT_MULT
            } else {
                WALK_SPEED
            };
            state.player.try_move_with_difficulty(
                input.movement[0] * speed * dt,
                state.player.vy * dt,
                input.movement[2] * speed * dt,
                &self.world,
                difficulty_multiplier(self.level.difficulty.as_str()),
            );
            updates.push((
                id,
                player_update(
                    id,
                    state,
                    self.level.tick,
                    [input.movement[0] * speed, state.player.vy, input.movement[2] * speed],
                ),
            ));
        }
        for update in updates {
            self.broadcast(update.1);
        }
    }

    fn stream_loaded_chunks(&mut self) {
        let loaded = self.world.loaded_chunk_keys();
        let render_distance = self.config.render_distance;
        let ids: Vec<_> = self.sessions.keys().copied().collect();
        for id in ids {
            let (unloads, key) = {
                let Some(state) = self.sessions.get_mut(&id).and_then(|session| session.player.as_mut()) else { continue; };
                let center = (
                    (state.player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                    (state.player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                );
                let unloads: Vec<_> = state
                    .sent_chunks
                    .iter()
                    .copied()
                    .filter(|&(cx, cz)| {
                        (cx - center.0).abs() > render_distance
                            || (cz - center.1).abs() > render_distance
                    })
                    .collect();
                for chunk in &unloads {
                    state.sent_chunks.remove(chunk);
                }
                state.pending_chunks.retain(|&(cx, cz)| {
                    (cx - center.0).abs() <= render_distance
                        && (cz - center.1).abs() <= render_distance
                });
                for key in &loaded {
                    if (key.0 - center.0).abs() <= render_distance
                        && (key.1 - center.1).abs() <= render_distance
                        && !state.sent_chunks.contains(key)
                        && !state.pending_chunks.contains(key)
                    {
                        state.pending_chunks.push_back(*key);
                    }
                }
                (unloads, state.pending_chunks.pop_front())
            };
            if let Some(session) = self.sessions.get_mut(&id) {
                for (cx, cz) in unloads {
                    queue_message(session, ServerMessage::ChunkUnload { cx, cz }, self.config.protocol_limits);
                }
                let Some((cx, cz)) = key else { continue; };
                let Some(data) = self.world.chunk_data(cx, cz) else { continue; };
                let Ok(payload) = encode_chunk_data(&data) else { continue; };
                let revision = self.world.chunk_revision(cx, cz).unwrap_or(0);
                if let Some(state) = session.player.as_mut() { state.sent_chunks.insert((cx, cz)); }
                queue_message(session, ServerMessage::ChunkData { cx, cz, revision, data: payload }, self.config.protocol_limits);
            }
        }
    }

    fn handle_block_edit(&mut self, id: u64, request_id: u64, position: [i32; 3], face: Face, action: BlockEditAction, expected_revision: u64) {
        let Some((player_position, selected_stack, gamemode)) = self.sessions.get(&id).and_then(|session| session.player.as_ref()).map(|state| ([state.player.x, state.player.y + state.player.current_eye_height(), state.player.z], state.inventory.selected_stack().clone(), self.level.gamemode.as_str())) else { return; };
        let selected_item = selected_stack.id;
        if !matches!(gamemode, "survival" | "creative") {
            return self.reject(id, Some(request_id), RejectCode::NotAllowed, "the current game mode cannot edit blocks");
        }
        let target_chunk = (position[0].div_euclid(CHUNK_SIZE as i32), position[2].div_euclid(CHUNK_SIZE as i32));
        let Some(revision) = self.world.chunk_revision(target_chunk.0, target_chunk.1) else {
            return self.reject(id, Some(request_id), RejectCode::OutOfRange, "target chunk is not loaded");
        };
        if revision != expected_revision {
            return self.reject(id, Some(request_id), RejectCode::StaleRevision, "target chunk revision is stale");
        }
        let distance = [position[0] as f32 + 0.5 - player_position[0], position[1] as f32 + 0.5 - player_position[1], position[2] as f32 + 0.5 - player_position[2]];
        if distance.iter().map(|value| value * value).sum::<f32>() > 36.0 {
            return self.reject(id, Some(request_id), RejectCode::OutOfRange, "block is outside interaction reach");
        }
        let old_block = self.world.get_block(position[0], position[1], position[2]);
        let paired_door_position = if old_block.id == BlockId::OakDoor {
            let half = crate::world::block_registry::registry()
                .properties_for_state(old_block.id, old_block.state)
                .and_then(|properties| properties.into_iter().find(|(name, _)| *name == "half").map(|(_, value)| value));
            Some([position[0], position[1] + if half == Some("lower") { 1 } else { -1 }, position[2]])
        } else {
            None
        };
        let (changed_position, new_block, inventory_changed, placed_by_rule) = match action {
            BlockEditAction::Break => {
                if old_block.is_air() {
                    return self.reject(id, Some(request_id), RejectCode::NotAllowed, "target is already air");
                }
                if matches!(old_block.id, BlockId::Bedrock | BlockId::Water | BlockId::Lava) {
                    return self.reject(id, Some(request_id), RejectCode::NotAllowed, "that block cannot be broken");
                }
                if gamemode != "creative" && !mining_outcome(old_block.id, &selected_stack, &self.item_registry).harvestable {
                    return self.reject(id, Some(request_id), RejectCode::NotAllowed, "the held tool cannot harvest that block");
                }
                (position, Block::air(), true, false)
            }
            BlockEditAction::Place { state } => {
                let Some(block_id) = BlockId::from_repr(state.block_id) else { return self.reject(id, Some(request_id), RejectCode::NotAllowed, "unknown block ID"); };
                let place = offset_position(position, face);
                if !self.world.get_block(place[0], place[1], place[2]).is_air() {
                    return self.reject(id, Some(request_id), RejectCode::NotAllowed, "placement position is occupied");
                }
                let expected_item = self.item_registry.item_id_from_block(block_id);
                if gamemode != "creative" && selected_item != expected_item {
                    return self.reject(id, Some(request_id), RejectCode::NotAllowed, "held item does not place this block");
                }
                if !self.world.place_block(place[0], place[1], place[2], block_id) {
                    return self.reject(id, Some(request_id), RejectCode::NotAllowed, "block placement was rejected");
                }
                (place, self.world.get_block(place[0], place[1], place[2]), true, true)
            }
        };
        if !placed_by_rule {
            self.world.set_block(changed_position[0], changed_position[1], changed_position[2], new_block);
        }
        let changed_chunk = (changed_position[0].div_euclid(CHUNK_SIZE as i32), changed_position[2].div_euclid(CHUNK_SIZE as i32));
        let new_revision = self.world.chunk_revision(changed_chunk.0, changed_chunk.1).unwrap_or(revision);
        if inventory_changed {
            if let Some(state) = self.sessions.get_mut(&id).and_then(|session| session.player.as_mut()) {
                if new_block.is_air() {
                    if gamemode != "creative" {
                        let outcome = mining_outcome(old_block.id, &selected_stack, &self.item_registry);
                        let _ = state.inventory.add_stack(outcome.drop, &self.item_registry);
                        if outcome.damages_tool {
                            let held_slot = state.inventory.held_slot;
                            state.inventory.hotbar_slot_mut(held_slot).damage_once(&self.item_registry);
                        }
                    }
                } else if gamemode != "creative" {
                    let held_slot = state.inventory.held_slot;
                    let count = state.inventory.hotbar_slot(held_slot).count.saturating_sub(1);
                    if count == 0 {
                        *state.inventory.hotbar_slot_mut(held_slot) = crate::inventory::EMPTY_STACK;
                    } else {
                        state.inventory.hotbar_slot_mut(held_slot).count = count;
                    }
                }
                state.inventory_revision = state.inventory_revision.wrapping_add(1);
            }
        }
        self.broadcast(ServerMessage::BlockUpdate { position: changed_position, state: wire_state(new_block), revision: new_revision });
        if let Some(paired_position) = paired_door_position {
            self.broadcast(ServerMessage::BlockUpdate { position: paired_position, state: wire_state(self.world.get_block(paired_position[0], paired_position[1], paired_position[2])), revision: new_revision });
        }
        if new_block.id == BlockId::OakDoor {
            let upper_position = [changed_position[0], changed_position[1] + 1, changed_position[2]];
            let upper = self.world.get_block(upper_position[0], upper_position[1], upper_position[2]);
            self.broadcast(ServerMessage::BlockUpdate { position: upper_position, state: wire_state(upper), revision: new_revision });
        }
        self.accept(id, request_id);
        if inventory_changed { self.queue_inventory_snapshot(id); }
    }

    fn broadcast_chat(&mut self, id: u64, message: String) {
        let sender = self.sessions.get(&id).and_then(|session| session.username.clone()).unwrap_or_else(|| "unknown".to_string());
        self.broadcast(ServerMessage::Chat { sender_id: Some(id), sender, message });
    }

    fn handle_inventory_action(
        &mut self,
        id: u64,
        request_id: u64,
        slot: u16,
        action: super::InventoryAction,
        expected_revision: u64,
    ) {
        let Some(state) = self.sessions.get(&id).and_then(|session| session.player.as_ref()) else {
            return;
        };
        if state.inventory_revision != expected_revision {
            self.queue_inventory_snapshot(id);
            self.reject(id, Some(request_id), RejectCode::StaleRevision, "inventory revision is stale");
            return;
        }
        let Some(state) = self.sessions.get_mut(&id).and_then(|session| session.player.as_mut()) else {
            return;
        };
        let accepted = match action {
            super::InventoryAction::SwapHotbar { hotbar_slot } if (hotbar_slot as usize) < crate::inventory::HOTBAR_SLOTS => {
                state.inventory.held_slot = hotbar_slot as usize;
                state.inventory_revision = state.inventory_revision.wrapping_add(1);
                true
            }
            super::InventoryAction::Click { button, mode } if mode == 0 && button <= 1 => {
                if apply_inventory_click(&mut state.inventory, &mut state.cursor, slot as usize, button, &self.item_registry) {
                    state.inventory_revision = state.inventory_revision.wrapping_add(1);
                    true
                } else {
                    false
                }
            }
            super::InventoryAction::Drop { .. } => false,
            _ => false,
        };
        if accepted {
            self.queue_inventory_snapshot(id);
            self.accept(id, request_id);
        } else {
            self.reject(id, Some(request_id), RejectCode::NotAllowed, "inventory action is not enabled yet");
        }
    }

    fn broadcast(&mut self, message: ServerMessage) {
        for session in self.sessions.values_mut() {
            if session.player.is_some() {
                queue_message(session, message.clone(), self.config.protocol_limits);
            }
        }
    }

    fn reject(&mut self, id: u64, request_id: Option<u64>, code: RejectCode, message: &str) {
        if let Some(session) = self.sessions.get_mut(&id) {
            queue_message(session, ServerMessage::Reject { request_id, code, message: message.to_string() }, self.config.protocol_limits);
        }
    }

    fn accept(&mut self, id: u64, request_id: u64) {
        if let Some(session) = self.sessions.get_mut(&id) {
            queue_message(session, ServerMessage::ActionAccepted { request_id, server_tick: self.level.tick }, self.config.protocol_limits);
        }
    }

    fn queue_inventory_snapshot(&mut self, id: u64) {
        let Some(session) = self.sessions.get_mut(&id) else { return; };
        let Some(state) = session.player.as_ref() else { return; };
        queue_message(session, inventory_snapshot(state), self.config.protocol_limits);
    }

    fn remove_session(&mut self, id: u64) {
        let removed = self.sessions.remove(&id);
        if let Some(session) = removed.as_ref() {
            if let (Some(username), Some(state)) = (session.username.as_ref(), session.player.as_ref()) {
                self.persist_player(username, state);
            }
        }
        if removed.and_then(|session| session.player).is_some() {
            self.broadcast(ServerMessage::PlayerDespawn { player_id: id });
            if let Err(error) = self.save() {
                log::warn!("failed to save disconnected player session: {error}");
            }
        }
    }

    fn persist_active_players(&mut self) {
        let active: Vec<_> = self
            .sessions
            .values()
            .filter_map(|session| {
                Some((
                    session.username.as_ref()?.clone(),
                    PlayerData::from_runtime(&session.player.as_ref()?.player, &session.player.as_ref()?.inventory),
                ))
            })
            .collect();
        for (username, player) in active {
            self.persist_player_data(&username, player);
        }
    }

    fn persist_player(&mut self, username: &str, state: &PlayerSessionState) {
        self.persist_player_data(username, PlayerData::from_runtime(&state.player, &state.inventory));
    }

    fn persist_player_data(&mut self, username: &str, player: PlayerData) {
        if let Some(saved) = self.level.players.iter_mut().find(|saved| saved.username == username) {
            saved.player = player;
        } else {
            self.level.players.push(NamedPlayerData { username: username.to_string(), player });
        }
    }
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn request_id(message: &ClientMessage) -> Option<u64> {
    match message {
        ClientMessage::BlockEditRequest { request_id, .. }
        | ClientMessage::InventoryActionRequest { request_id, .. } => Some(*request_id),
        _ => None,
    }
}

fn reject_code(error: &ProtocolError) -> RejectCode {
    match error {
        ProtocolError::UnsupportedVersion(_) => RejectCode::UnsupportedVersion,
        ProtocolError::RateLimited => RejectCode::RateLimited,
        ProtocolError::StaleInputSequence { .. } => RejectCode::InvalidMessage,
        ProtocolError::HandshakeRequired => RejectCode::NotAuthenticated,
        ProtocolError::SessionClosing => RejectCode::InvalidMessage,
        ProtocolError::InvalidMessage(_)
        | ProtocolError::MessageTooLarge { .. }
        | ProtocolError::TruncatedFrame { .. }
        | ProtocolError::TrailingBytes { .. }
        | ProtocolError::InvalidEncoding(_)
        | ProtocolError::InvalidChunkPayload(_)
        | ProtocolError::UnexpectedMessage(_) => RejectCode::InvalidMessage,
    }
}

fn bounded_message(message: &str, max_bytes: usize) -> String {
    if message.len() <= max_bytes {
        return message.to_string();
    }
    let mut end = max_bytes;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

fn write_one_shot(
    stream: &mut TcpStream,
    message: ServerMessage,
    limits: ProtocolLimits,
) -> Result<(), ProtocolError> {
    let frame = encode_server_with_limits(&message, limits)?;
    stream
        .write_all(&frame)
        .map_err(|error| ProtocolError::InvalidEncoding(error.to_string()))
}

struct ClientConnection {
    stream: TcpStream,
    decoder: FrameDecoder,
    guard: SessionGuard,
    username: Option<String>,
    player: Option<PlayerSessionState>,
    outbox: VecDeque<OutboundFrame>,
    close_after_flush: bool,
    last_activity: Instant,
}

impl ClientConnection {
    fn new(stream: TcpStream, limits: ProtocolLimits, now: Instant) -> Self {
        Self {
            stream,
            decoder: FrameDecoder::new(limits),
            guard: SessionGuard::new(now, limits),
            username: None,
            player: None,
            outbox: VecDeque::new(),
            close_after_flush: false,
            last_activity: now,
        }
    }

    fn read_frames(&mut self) -> ReadResult {
        let mut frames = Vec::new();
        let mut buffer = [0u8; SOCKET_READ_BUFFER_BYTES];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => return ReadResult::Closed,
                Ok(read) => {
                    self.last_activity = Instant::now();
                    match self.decoder.push(&buffer[..read]) {
                        Ok(mut decoded) => frames.append(&mut decoded),
                        Err(error) => return ReadResult::Protocol(error),
                    }
                    if read < buffer.len() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return ReadResult::Io(error),
            }
        }
        ReadResult::Frames(frames)
    }

    fn flush(&mut self) -> bool {
        loop {
            let Some(frame) = self.outbox.front_mut() else {
                return true;
            };
            match self.stream.write(&frame.bytes[frame.offset..]) {
                Ok(0) => return false,
                Ok(written) => {
                    frame.offset += written;
                    if frame.offset == frame.bytes.len() {
                        self.outbox.pop_front();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return true,
                Err(_) => return false,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct InputIntent {
    movement: [f32; 3],
    yaw: f32,
    pitch: f32,
    jump: bool,
    sprint: bool,
    sneak: bool,
}

struct PlayerSessionState {
    player: Player,
    inventory: Inventory,
    cursor: ItemStack,
    input: InputIntent,
    yaw: f32,
    pitch: f32,
    inventory_revision: u64,
    sent_chunks: HashSet<(i32, i32)>,
    pending_chunks: VecDeque<(i32, i32)>,
}

fn wire_state(block: Block) -> WireBlockState {
    WireBlockState { block_id: block.id as u16, state: block.state, data: block.data }
}

fn inventory_snapshot(state: &PlayerSessionState) -> ServerMessage {
    ServerMessage::InventorySnapshot {
        revision: state.inventory_revision,
        slots: state.inventory.slots.iter().map(|stack| super::WireItemStack {
            item_id: stack.id,
            count: stack.count,
            damage: stack.damage,
        }).collect(),
        held_slot: state.inventory.held_slot as u8,
        cursor: super::WireItemStack {
            item_id: state.cursor.id,
            count: state.cursor.count,
            damage: state.cursor.damage,
        },
    }
}

fn apply_inventory_click(
    inventory: &mut Inventory,
    cursor: &mut ItemStack,
    slot: usize,
    button: u8,
    items: &ItemRegistry,
) -> bool {
    let Some(target) = inventory.slots.get_mut(slot) else {
        return false;
    };
    if button == 0 {
        if cursor.is_empty() {
            std::mem::swap(cursor, target);
            return !cursor.is_empty() || !target.is_empty();
        }
        if target.is_empty() {
            std::mem::swap(cursor, target);
            return true;
        }
        if target.can_merge_with(cursor) {
            let space = target.max_stack(items) as u16 - target.count;
            let moved = space.min(cursor.count);
            target.count += moved;
            cursor.count -= moved;
            if cursor.count == 0 {
                *cursor = EMPTY_STACK;
            }
            return moved > 0;
        }
        std::mem::swap(cursor, target);
        true
    } else if cursor.is_empty() {
        if target.is_empty() {
            return false;
        }
        let taken = target.count.div_ceil(2);
        *cursor = ItemStack::with_damage(target.id, taken, target.damage);
        target.count -= taken;
        if target.count == 0 {
            *target = EMPTY_STACK;
        }
        true
    } else if target.is_empty() {
        *target = ItemStack::with_damage(cursor.id, 1, cursor.damage);
        cursor.count -= 1;
        if cursor.count == 0 {
            *cursor = EMPTY_STACK;
        }
        true
    } else if target.can_merge_with(cursor) && target.count < target.max_stack(items) as u16 {
        target.count += 1;
        cursor.count -= 1;
        if cursor.count == 0 {
            *cursor = EMPTY_STACK;
        }
        true
    } else {
        false
    }
}

fn player_update(id: u64, state: &PlayerSessionState, server_tick: u64, velocity: [f32; 3]) -> ServerMessage {
    ServerMessage::PlayerUpdate {
        player_id: id,
        server_tick,
        position: [state.player.x as f64, state.player.y as f64, state.player.z as f64],
        velocity,
        yaw: state.yaw,
        pitch: state.pitch,
    }
}

fn difficulty_multiplier(difficulty: &str) -> f32 {
    match difficulty {
        "hard" => 1.5,
        "easy" => 0.5,
        "peaceful" => 0.0,
        _ => 1.0,
    }
}

fn offset_position(position: [i32; 3], face: Face) -> [i32; 3] {
    let mut result = position;
    match face {
        Face::Down => result[1] -= 1,
        Face::Up => result[1] += 1,
        Face::North => result[2] -= 1,
        Face::South => result[2] += 1,
        Face::West => result[0] -= 1,
        Face::East => result[0] += 1,
    }
    result
}

#[derive(Debug)]
struct OutboundFrame {
    bytes: Vec<u8>,
    offset: usize,
}

fn queue_message(session: &mut ClientConnection, message: ServerMessage, limits: ProtocolLimits) {
    if session.outbox.len() >= MAX_OUTBOUND_FRAMES {
        session.close_after_flush = true;
        return;
    }
    match encode_server_with_limits(&message, limits) {
        Ok(bytes) => session.outbox.push_back(OutboundFrame { bytes, offset: 0 }),
        Err(error) => {
            log::warn!("failed to queue server message: {error}");
            session.close_after_flush = true;
        }
    }
}

#[derive(Debug)]
enum ReadResult {
    Frames(Vec<Vec<u8>>),
    Protocol(ProtocolError),
    Closed,
    Io(io::Error),
    Noop,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{
        decode_server, encode_client, BlockEditAction, Face, WireBlockState, PROTOCOL_VERSION,
    };
    use crate::network::client::ClientTransport;
    use std::fs;
    use std::sync::atomic::AtomicU64;

    static TEST_WORLD_ID: AtomicU64 = AtomicU64::new(0);

    fn test_world() -> PathBuf {
        let id = TEST_WORLD_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("vibecraft-server-{id}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn read_message(stream: &mut TcpStream) -> ServerMessage {
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).unwrap();
        let length = u32::from_be_bytes(header) as usize;
        let mut frame = header.to_vec();
        frame.resize(4 + length, 0);
        stream.read_exact(&mut frame[4..]).unwrap();
        decode_server(&frame).unwrap()
    }

    #[test]
    fn server_config_rejects_invalid_limits_and_parses_headless_options() {
        let config = ServerConfig::from_args([
            "--bind".to_string(),
            "127.0.0.1:0".to_string(),
            "--world-dir".to_string(),
            "server-fixture".to_string(),
            "--seed".to_string(),
            "42".to_string(),
            "--max-players".to_string(),
            "4".to_string(),
        ])
        .unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:0".parse().unwrap());
        assert_eq!(config.seed, Some(42));
        assert_eq!(config.max_players, 4);

        let invalid = ServerConfig {
            max_players: 0,
            ..config
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn server_accepts_handshake_echoes_keep_alive_and_saves_on_shutdown() {
        let world_dir = test_world();
        let config = ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            world_dir: world_dir.clone(),
            seed: Some(42),
            autosave_interval: Duration::from_secs(3600),
            ..ServerConfig::default()
        };
        let mut server = HeadlessServer::bind(config).unwrap();
        let mut client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        client
            .write_all(
                &encode_client(&ClientMessage::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    username: "Alex".to_string(),
                })
                .unwrap(),
            )
            .unwrap();

        for _ in 0..100 {
            server.poll().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(server.connected_sessions(), 1);
        assert_eq!(
            read_message(&mut client),
            ServerMessage::Welcome {
                session_id: 1,
                username: "Alex".to_string(),
                world_seed: 42,
                spawn: [0.0, 75.0, 0.0],
                server_tick: 0,
                view_distance: DEFAULT_RENDER_DISTANCE as u8,
            }
        );
        assert!(matches!(read_message(&mut client), ServerMessage::InventorySnapshot { .. }));
        assert!(matches!(read_message(&mut client), ServerMessage::PlayerSpawn { player_id: 1, .. }));

        client
            .write_all(&encode_client(&ClientMessage::KeepAlive { nonce: 9 }).unwrap())
            .unwrap();
        // A connected session does not prove that its newly written frame was
        // accepted and echoed. Poll long enough for the nonblocking server to
        // read and flush this specific request before attempting the read.
        for _ in 0..100 {
            server.poll().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            read_message(&mut client),
            ServerMessage::KeepAlive { nonce: 9 }
        );

        server.tick();
        server.shutdown().unwrap();
        let saved = WorldStorage::new(&world_dir).load_level().unwrap();
        assert_eq!(saved.seed, 42);
        assert_eq!(saved.tick, 1);
        assert_eq!(saved.players.len(), 1);
        assert_eq!(saved.players[0].username, "Alex");
        fs::remove_dir_all(world_dir).unwrap();
    }

    #[test]
    fn server_rejects_unloaded_block_edits_authoritatively() {
        let world_dir = test_world();
        let config = ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            world_dir: world_dir.clone(),
            seed: Some(7),
            ..ServerConfig::default()
        };
        let mut server = HeadlessServer::bind(config).unwrap();
        let mut client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let hello = encode_client(&ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            username: "Builder".to_string(),
        })
        .unwrap();
        let edit = encode_client(&ClientMessage::BlockEditRequest {
            request_id: 3,
            position: [0, 75, 0],
            face: Face::Up,
            action: BlockEditAction::Place {
                state: WireBlockState {
                    block_id: 1,
                    state: 0,
                    data: 0,
                },
            },
            expected_revision: 0,
        })
        .unwrap();
        client.write_all(&hello).unwrap();
        for _ in 0..100 {
            server.poll().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(server.connected_sessions(), 1);
        let _welcome = read_message(&mut client);
        let _inventory = read_message(&mut client);
        let _spawn = read_message(&mut client);
        client.write_all(&edit).unwrap();
        for _ in 0..100 {
            server.poll().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            read_message(&mut client),
            ServerMessage::Reject {
                request_id: Some(3),
                code: RejectCode::OutOfRange,
                message: "target chunk is not loaded".to_string(),
            }
        );
        server.shutdown().unwrap();
        fs::remove_dir_all(world_dir).unwrap();
    }

    #[test]
    fn server_simulates_two_authoritative_players_and_queues_replication() {
        let world_dir = test_world();
        let config = ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            world_dir: world_dir.clone(),
            seed: Some(9),
            ..ServerConfig::default()
        };
        let mut server = HeadlessServer::bind(config).unwrap();
        let mut first = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        let mut second = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        for (stream, username) in [(&mut first, "One"), (&mut second, "Two")] {
            stream
                .write_all(&encode_client(&ClientMessage::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    username: username.to_string(),
                }).unwrap())
                .unwrap();
        }
        for _ in 0..100 {
            server.poll().unwrap();
            if server.connected_sessions() == 2 { break; }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(server.connected_sessions(), 2);
        let before = server.sessions.get(&1).unwrap().player.as_ref().unwrap().player.x;
        first
            .write_all(&encode_client(&ClientMessage::Input {
                sequence: 1,
                movement: [1.0, 0.0, 0.0],
                yaw: 0.0,
                pitch: 0.0,
                jump: false,
                sprint: false,
                sneak: false,
            }).unwrap())
            .unwrap();
        for _ in 0..10 {
            server.poll().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        server.tick();
        let after = server.sessions.get(&1).unwrap().player.as_ref().unwrap().player.x;
        assert!(after > before);
        let second_outbox = &server.sessions.get(&2).unwrap().outbox;
        assert!(second_outbox.iter().any(|frame| {
            matches!(
                decode_server(&frame.bytes),
                Ok(ServerMessage::PlayerUpdate { player_id: 1, velocity, .. }) if velocity[0] > 0.0
            )
        }));
        server.shutdown().unwrap();
        fs::remove_dir_all(world_dir).unwrap();
    }

    #[test]
    fn authoritative_inventory_clicks_use_a_server_cursor() {
        let items = ItemRegistry::new();
        let mut inventory = Inventory::new();
        inventory.slots[0] = ItemStack::new(items.item_id_from_block(BlockId::Stone), 8);
        let mut cursor = EMPTY_STACK;

        assert!(apply_inventory_click(&mut inventory, &mut cursor, 0, 0, &items));
        assert!(inventory.slots[0].is_empty());
        assert_eq!(cursor.count, 8);
        assert!(apply_inventory_click(&mut inventory, &mut cursor, 1, 1, &items));
        assert_eq!(inventory.slots[1].count, 1);
        assert_eq!(cursor.count, 7);
    }

    #[test]
    fn stale_inventory_action_returns_authoritative_snapshot() {
        let world_dir = test_world();
        let config = ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            world_dir: world_dir.clone(),
            seed: Some(11),
            ..ServerConfig::default()
        };
        let mut server = HeadlessServer::bind(config).unwrap();
        let mut client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        client
            .write_all(
                &encode_client(&ClientMessage::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    username: "InventorySync".to_string(),
                })
                .unwrap(),
            )
            .unwrap();
        for _ in 0..100 {
            server.poll().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(server.connected_sessions(), 1);
        let _welcome = read_message(&mut client);
        let _initial_inventory = read_message(&mut client);
        let _spawn = read_message(&mut client);

        server
            .sessions
            .get_mut(&1)
            .unwrap()
            .player
            .as_mut()
            .unwrap()
            .inventory_revision = 3;
        client
            .write_all(
                &encode_client(&ClientMessage::InventoryActionRequest {
                    request_id: 8,
                    slot: 0,
                    action: super::super::InventoryAction::Click { button: 0, mode: 0 },
                    expected_revision: 2,
                })
                .unwrap(),
            )
            .unwrap();
        for _ in 0..100 {
            server.poll().unwrap();
            if server
                .sessions
                .get(&1)
                .is_some_and(|session| session.outbox.len() >= 2)
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert!(matches!(
            read_message(&mut client),
            ServerMessage::InventorySnapshot { revision: 3, .. }
        ));
        assert_eq!(
            read_message(&mut client),
            ServerMessage::Reject {
                request_id: Some(8),
                code: RejectCode::StaleRevision,
                message: "inventory revision is stale".to_string(),
            }
        );
        server.shutdown().unwrap();
        fs::remove_dir_all(world_dir).unwrap();
    }

    #[test]
    fn streams_chunks_to_every_connected_client() {
        let world_dir = test_world();
        let config = ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            world_dir: world_dir.clone(),
            seed: Some(17),
            render_distance: 2,
            ..ServerConfig::default()
        };
        let mut server = HeadlessServer::bind(config).unwrap();
        let address = server.local_addr().unwrap();
        let mut first = ClientTransport::connect(address, "Player".to_string()).unwrap();
        let mut second = ClientTransport::connect(address, "Player".to_string()).unwrap();
        let mut first_chunks = 0;
        let mut second_chunks = 0;

        for _ in 0..300 {
            server.poll().unwrap();
            server.tick();
            if let Ok(messages) = first.poll() {
                first_chunks += messages
                    .iter()
                    .filter(|message| matches!(message, ServerMessage::ChunkData { .. }))
                    .count();
            }
            if let Ok(messages) = second.poll() {
                second_chunks += messages
                    .iter()
                    .filter(|message| matches!(message, ServerMessage::ChunkData { .. }))
                    .count();
            }
            if first_chunks > 0 && second_chunks > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }

        assert!(
            first_chunks > 0,
            "first client received no chunks (first={first_chunks}, second={second_chunks}, sessions={}, loaded={})",
            server.connected_sessions(),
            server.loaded_chunks()
        );
        assert!(
            second_chunks > 0,
            "second client received no chunks (first={first_chunks}, second={second_chunks}, sessions={}, loaded={})",
            server.connected_sessions(),
            server.loaded_chunks()
        );

        for session in server.sessions.values_mut() {
            if let Some(state) = session.player.as_mut() {
                state.player.x += 64.0;
            }
        }
        server.stream_loaded_chunks();
        let queued_unloads = server
            .sessions
            .values()
            .flat_map(|session| session.outbox.iter())
            .filter_map(|frame| decode_server(&frame.bytes).ok())
            .filter(|message| matches!(message, ServerMessage::ChunkUnload { .. }))
            .count();
        assert!(
            queued_unloads > 0,
            "moving outside view distance should queue chunk unloads"
        );

        server.shutdown().unwrap();
        fs::remove_dir_all(world_dir).unwrap();
    }
}
