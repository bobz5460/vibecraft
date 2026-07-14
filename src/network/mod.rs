//! The native client/server protocol.
//!
//! The protocol deliberately carries requests from clients rather than
//! authoritative world state. A server validates every request against its
//! current simulation and sends the resulting state back to clients.

pub mod server;
pub mod client;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::world::block::BlockId;
use crate::world::persistence::{BlockCell, ChunkData};
use crate::world::chunk::CHUNK_VOLUME;

pub const PROTOCOL_VERSION: u16 = 1;
pub const FRAME_HEADER_BYTES: usize = 4;
pub const MAX_FRAME_PAYLOAD: usize = 1024 * 1024;
pub const MAX_CHUNK_PAYLOAD: usize = 768 * 1024;
pub const MAX_USERNAME_BYTES: usize = 16;
pub const MAX_CHAT_BYTES: usize = 256;
pub const MAX_INVENTORY_SLOTS: usize = 46;
pub const MAX_MESSAGES_PER_SECOND: u32 = 40;
pub const MAX_WORLD_COORDINATE: i32 = 30_000_000;

/// Limits are kept in one value so a future server can expose a deliberately
/// smaller policy without changing wire types or bypassing validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolLimits {
    pub max_frame_payload: usize,
    pub max_chunk_payload: usize,
    pub max_username_bytes: usize,
    pub max_chat_bytes: usize,
    pub max_inventory_slots: usize,
    pub max_messages_per_second: u32,
}

impl ProtocolLimits {
    pub const fn demo() -> Self {
        Self {
            max_frame_payload: MAX_FRAME_PAYLOAD,
            max_chunk_payload: MAX_CHUNK_PAYLOAD,
            max_username_bytes: MAX_USERNAME_BYTES,
            max_chat_bytes: MAX_CHAT_BYTES,
            max_inventory_slots: MAX_INVENTORY_SLOTS,
            max_messages_per_second: MAX_MESSAGES_PER_SECOND,
        }
    }
}

impl Default for ProtocolLimits {
    fn default() -> Self {
        Self::demo()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WireBlockState {
    pub block_id: u16,
    pub state: u16,
    pub data: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockEditAction {
    Break,
    Place { state: WireBlockState },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryAction {
    Click { button: u8, mode: u8 },
    SwapHotbar { hotbar_slot: u8 },
    Drop { count: u8 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        protocol_version: u16,
        username: String,
    },
    Input {
        sequence: u64,
        movement: [f32; 3],
        yaw: f32,
        pitch: f32,
        jump: bool,
        sprint: bool,
        sneak: bool,
    },
    BlockEditRequest {
        request_id: u64,
        position: [i32; 3],
        face: Face,
        action: BlockEditAction,
        expected_revision: u64,
    },
    InventoryActionRequest {
        request_id: u64,
        slot: u16,
        action: InventoryAction,
        expected_revision: u64,
    },
    Chat {
        message: String,
    },
    KeepAlive {
        nonce: u64,
    },
    Disconnect {
        reason: DisconnectReason,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        session_id: u64,
        username: String,
        world_seed: u64,
        spawn: [f64; 3],
        server_tick: u64,
        view_distance: u8,
    },
    ChunkData {
        cx: i32,
        cz: i32,
        revision: u64,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
    ChunkUnload {
        cx: i32,
        cz: i32,
    },
    PlayerSpawn {
        player_id: u64,
        username: String,
        position: [f64; 3],
    },
    PlayerDespawn {
        player_id: u64,
    },
    PlayerUpdate {
        player_id: u64,
        server_tick: u64,
        position: [f64; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
    },
    BlockUpdate {
        position: [i32; 3],
        state: WireBlockState,
        revision: u64,
    },
    InventorySnapshot {
        revision: u64,
        slots: Vec<WireItemStack>,
        held_slot: u8,
        cursor: WireItemStack,
    },
    Chat {
        sender_id: Option<u64>,
        sender: String,
        message: String,
    },
    KeepAlive {
        nonce: u64,
    },
    ActionAccepted {
        request_id: u64,
        server_tick: u64,
    },
    Reject {
        request_id: Option<u64>,
        code: RejectCode,
        message: String,
    },
    Disconnect {
        code: DisconnectCode,
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WireItemStack {
    pub item_id: u16,
    pub count: u16,
    pub damage: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectCode {
    InvalidMessage,
    UnsupportedVersion,
    NotAuthenticated,
    RateLimited,
    StaleRevision,
    OutOfRange,
    NotAllowed,
    ServerBusy,
    Internal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectCode {
    ProtocolError,
    Kicked,
    ServerFull,
    UnsupportedVersion,
    ServerShutdown,
    Timeout,
    ClientQuit,
    MalformedMessage,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectReason {
    ClientQuit,
    ProtocolError,
    ServerShutdown,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("message validation failed: {0}")]
    InvalidMessage(String),
    #[error("message payload is {actual} bytes, maximum is {max} bytes")]
    MessageTooLarge { actual: usize, max: usize },
    #[error("frame is truncated: expected {expected} bytes, received {actual} bytes")]
    TruncatedFrame { expected: usize, actual: usize },
    #[error("frame contains {extra} trailing bytes")]
    TrailingBytes { extra: usize },
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u16),
    #[error("invalid encoded message: {0}")]
    InvalidEncoding(String),
    #[error("the handshake must be completed first")]
    HandshakeRequired,
    #[error("the session is already closing")]
    SessionClosing,
    #[error("unexpected message for the current session state: {0}")]
    UnexpectedMessage(String),
    #[error("message rate limit exceeded")]
    RateLimited,
    #[error("input sequence {sequence} is not newer than {previous}")]
    StaleInputSequence { sequence: u64, previous: u64 },
    #[error("invalid chunk payload: {0}")]
    InvalidChunkPayload(String),
}

const CHUNK_CODEC_MAGIC: &[u8; 4] = b"VCC1";
const CHUNK_CODEC_VERSION: u8 = 1;

/// Encodes a chunk without JSON's per-cell array overhead. Each run contains
/// one raw block ID, registry state, legacy data byte, and a u16 run length.
/// Runs are split when a single state occupies more than u16::MAX cells.
pub fn encode_chunk_data(data: &ChunkData) -> Result<Vec<u8>, ProtocolError> {
    if data.cells.len() != CHUNK_VOLUME {
        return Err(ProtocolError::InvalidChunkPayload(format!(
            "chunk has {} cells; expected {CHUNK_VOLUME}",
            data.cells.len()
        )));
    }
    let mut runs = Vec::new();
    let mut index = 0;
    while index < data.cells.len() {
        let cell = data.cells[index];
        let mut length = 1usize;
        while index + length < data.cells.len()
            && data.cells[index + length] == cell
            && length < u16::MAX as usize
        {
            length += 1;
        }
        runs.push((cell, length as u16));
        index += length;
    }

    let entities = serde_json::to_vec(&data.block_entities)
        .map_err(|error| ProtocolError::InvalidChunkPayload(error.to_string()))?;
    let mut encoded = Vec::with_capacity(17 + runs.len() * 9 + entities.len());
    encoded.extend_from_slice(CHUNK_CODEC_MAGIC);
    encoded.push(CHUNK_CODEC_VERSION);
    encoded.extend_from_slice(&data.cx.to_le_bytes());
    encoded.extend_from_slice(&data.cz.to_le_bytes());
    encoded.extend_from_slice(&(data.cells.len() as u32).to_le_bytes());
    encoded.extend_from_slice(&(runs.len() as u32).to_le_bytes());
    encoded.extend_from_slice(&(entities.len() as u32).to_le_bytes());
    for (BlockCell(block_id, state, legacy_data), length) in runs {
        encoded.extend_from_slice(&block_id.to_le_bytes());
        encoded.extend_from_slice(&state.to_le_bytes());
        encoded.push(legacy_data);
        encoded.extend_from_slice(&length.to_le_bytes());
    }
    encoded.extend_from_slice(&entities);
    if encoded.len() > MAX_CHUNK_PAYLOAD {
        return Err(ProtocolError::MessageTooLarge {
            actual: encoded.len(),
            max: MAX_CHUNK_PAYLOAD,
        });
    }
    Ok(encoded)
}

/// Decodes and validates a payload produced by [`encode_chunk_data`].
pub fn decode_chunk_data(payload: &[u8]) -> Result<ChunkData, ProtocolError> {
    if payload.len() > MAX_CHUNK_PAYLOAD {
        return Err(ProtocolError::MessageTooLarge {
            actual: payload.len(),
            max: MAX_CHUNK_PAYLOAD,
        });
    }
    let mut cursor = 0usize;
    let magic = read_bytes(payload, &mut cursor, 4)?;
    if magic != CHUNK_CODEC_MAGIC {
        return Err(ProtocolError::InvalidChunkPayload("invalid codec magic".to_string()));
    }
    if read_u8(payload, &mut cursor)? != CHUNK_CODEC_VERSION {
        return Err(ProtocolError::InvalidChunkPayload(
            "unsupported chunk codec version".to_string(),
        ));
    }
    let cx = read_i32(payload, &mut cursor)?;
    let cz = read_i32(payload, &mut cursor)?;
    let cell_count = read_u32(payload, &mut cursor)? as usize;
    let run_count = read_u32(payload, &mut cursor)? as usize;
    let entity_len = read_u32(payload, &mut cursor)? as usize;
    if cell_count != CHUNK_VOLUME {
        return Err(ProtocolError::InvalidChunkPayload(format!(
            "chunk has {cell_count} cells; expected {CHUNK_VOLUME}"
        )));
    }
    let mut cells = Vec::with_capacity(cell_count);
    for _ in 0..run_count {
        let block_id = read_u16(payload, &mut cursor)?;
        let state = read_u16(payload, &mut cursor)?;
        let data = read_u8(payload, &mut cursor)?;
        let length = read_u16(payload, &mut cursor)? as usize;
        if length == 0 || BlockId::from_repr(block_id).is_none() {
            return Err(ProtocolError::InvalidChunkPayload(
                "chunk contains an invalid run".to_string(),
            ));
        }
        cells.extend(std::iter::repeat_n(BlockCell(block_id, state, data), length));
        if cells.len() > cell_count {
            return Err(ProtocolError::InvalidChunkPayload(
                "chunk runs exceed the cell count".to_string(),
            ));
        }
    }
    if cells.len() != cell_count {
        return Err(ProtocolError::InvalidChunkPayload(
            "chunk runs do not fill the cell count".to_string(),
        ));
    }
    let entity_bytes = read_bytes(payload, &mut cursor, entity_len)?;
    if cursor != payload.len() {
        return Err(ProtocolError::InvalidChunkPayload(
            "chunk payload has trailing bytes".to_string(),
        ));
    }
    let block_entities = serde_json::from_slice(entity_bytes)
        .map_err(|error| ProtocolError::InvalidChunkPayload(error.to_string()))?;
    Ok(ChunkData {
        cx,
        cz,
        cells,
        block_entities,
    })
}

fn read_bytes<'a>(payload: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8], ProtocolError> {
    let end = cursor.checked_add(length).ok_or_else(|| {
        ProtocolError::InvalidChunkPayload("chunk cursor overflow".to_string())
    })?;
    let bytes = payload.get(*cursor..end).ok_or_else(|| {
        ProtocolError::InvalidChunkPayload("chunk payload is truncated".to_string())
    })?;
    *cursor = end;
    Ok(bytes)
}

fn read_u8(payload: &[u8], cursor: &mut usize) -> Result<u8, ProtocolError> {
    Ok(read_bytes(payload, cursor, 1)?[0])
}

fn read_u16(payload: &[u8], cursor: &mut usize) -> Result<u16, ProtocolError> {
    Ok(u16::from_le_bytes(read_bytes(payload, cursor, 2)?.try_into().unwrap()))
}

fn read_u32(payload: &[u8], cursor: &mut usize) -> Result<u32, ProtocolError> {
    Ok(u32::from_le_bytes(read_bytes(payload, cursor, 4)?.try_into().unwrap()))
}

fn read_i32(payload: &[u8], cursor: &mut usize) -> Result<i32, ProtocolError> {
    Ok(i32::from_le_bytes(read_bytes(payload, cursor, 4)?.try_into().unwrap()))
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let a = chunk[0] as u32;
            let b = chunk.get(1).copied().unwrap_or(0) as u32;
            let c = chunk.get(2).copied().unwrap_or(0) as u32;
            encoded.push(ALPHABET[((a >> 2) & 0x3f) as usize] as char);
            encoded.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
            encoded.push(if chunk.len() > 1 { ALPHABET[((b & 15) << 2 | (c >> 6)) as usize] as char } else { '=' });
            encoded.push(if chunk.len() > 2 { ALPHABET[(c & 0x3f) as usize] as char } else { '=' });
        }
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() % 4 != 0 {
            return Err(serde::de::Error::custom("base64 payload has invalid length"));
        }
        let mut decoded = Vec::with_capacity(encoded.len() / 4 * 3);
        for chunk in encoded.as_bytes().chunks(4) {
            let a = value(chunk[0]).ok_or_else(|| serde::de::Error::custom("invalid base64 payload"))?;
            let b = value(chunk[1]).ok_or_else(|| serde::de::Error::custom("invalid base64 payload"))?;
            let c = if chunk[2] == b'=' { 0 } else { value(chunk[2]).ok_or_else(|| serde::de::Error::custom("invalid base64 payload"))? };
            let d = if chunk[3] == b'=' { 0 } else { value(chunk[3]).ok_or_else(|| serde::de::Error::custom("invalid base64 payload"))? };
            decoded.push((a << 2) | (b >> 4));
            if chunk[2] != b'=' { decoded.push((b << 4) | (c >> 2)); }
            if chunk[3] != b'=' { decoded.push((c << 6) | d); }
        }
        Ok(decoded)
    }

    fn value(byte: u8) -> Option<u8> {
        ALPHABET.iter().position(|&candidate| candidate == byte).map(|value| value as u8)
    }
}

#[derive(Deserialize, Serialize)]
struct WireEnvelope<T> {
    version: u16,
    message: T,
}

fn encode<T: Serialize>(message: &T, limits: ProtocolLimits) -> Result<Vec<u8>, ProtocolError> {
    let payload = serde_json::to_vec(&WireEnvelope {
        version: PROTOCOL_VERSION,
        message,
    })
    .map_err(|error| ProtocolError::InvalidEncoding(error.to_string()))?;
    if payload.len() > limits.max_frame_payload {
        return Err(ProtocolError::MessageTooLarge {
            actual: payload.len(),
            max: limits.max_frame_payload,
        });
    }
    let payload_len = u32::try_from(payload.len()).map_err(|_| ProtocolError::MessageTooLarge {
        actual: payload.len(),
        max: u32::MAX as usize,
    })?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_BYTES + payload.len());
    frame.extend_from_slice(&payload_len.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn decode<T: DeserializeOwned>(
    frame: &[u8],
    limits: ProtocolLimits,
) -> Result<WireEnvelope<T>, ProtocolError> {
    if frame.len() < FRAME_HEADER_BYTES {
        return Err(ProtocolError::TruncatedFrame {
            expected: FRAME_HEADER_BYTES,
            actual: frame.len(),
        });
    }

    let declared = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if declared > limits.max_frame_payload {
        return Err(ProtocolError::MessageTooLarge {
            actual: declared,
            max: limits.max_frame_payload,
        });
    }

    let expected =
        FRAME_HEADER_BYTES
            .checked_add(declared)
            .ok_or(ProtocolError::MessageTooLarge {
                actual: declared,
                max: limits.max_frame_payload,
            })?;
    if frame.len() < expected {
        return Err(ProtocolError::TruncatedFrame {
            expected,
            actual: frame.len(),
        });
    }
    if frame.len() > expected {
        return Err(ProtocolError::TrailingBytes {
            extra: frame.len() - expected,
        });
    }

    let envelope = serde_json::from_slice::<WireEnvelope<T>>(&frame[FRAME_HEADER_BYTES..])
        .map_err(|error| ProtocolError::InvalidEncoding(error.to_string()))?;
    if envelope.version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(envelope.version));
    }
    Ok(envelope)
}

/// Incremental length-prefixed frame decoder for non-blocking streams.
///
/// The decoder retains only an incomplete frame between calls. Callers should
/// still use a bounded socket read buffer; the protocol limit is enforced
/// before any decoded payload is handed to message deserialization.
#[derive(Debug)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    limits: ProtocolLimits,
}

impl FrameDecoder {
    pub fn new(limits: ProtocolLimits) -> Self {
        Self {
            buffer: Vec::new(),
            limits,
        }
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, ProtocolError> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();

        loop {
            if self.buffer.len() < FRAME_HEADER_BYTES {
                break;
            }
            let declared = u32::from_be_bytes([
                self.buffer[0],
                self.buffer[1],
                self.buffer[2],
                self.buffer[3],
            ]) as usize;
            if declared > self.limits.max_frame_payload {
                return Err(ProtocolError::MessageTooLarge {
                    actual: declared,
                    max: self.limits.max_frame_payload,
                });
            }
            let expected =
                FRAME_HEADER_BYTES
                    .checked_add(declared)
                    .ok_or(ProtocolError::MessageTooLarge {
                        actual: declared,
                        max: self.limits.max_frame_payload,
                    })?;
            if self.buffer.len() < expected {
                break;
            }
            frames.push(self.buffer.drain(..expected).collect());
        }

        let max_buffered = self
            .limits
            .max_frame_payload
            .saturating_add(FRAME_HEADER_BYTES);
        if self.buffer.len() > max_buffered {
            return Err(ProtocolError::MessageTooLarge {
                actual: self.buffer.len(),
                max: max_buffered,
            });
        }
        Ok(frames)
    }
}

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_client_with_limits(message, ProtocolLimits::default())
}

pub fn encode_client_with_limits(
    message: &ClientMessage,
    limits: ProtocolLimits,
) -> Result<Vec<u8>, ProtocolError> {
    message.validate(&limits)?;
    encode(message, limits)
}

pub fn decode_client(frame: &[u8]) -> Result<ClientMessage, ProtocolError> {
    decode_client_with_limits(frame, ProtocolLimits::default())
}

pub fn decode_client_with_limits(
    frame: &[u8],
    limits: ProtocolLimits,
) -> Result<ClientMessage, ProtocolError> {
    let envelope = decode::<ClientMessage>(frame, limits)?;
    envelope.message.validate(&limits)?;
    Ok(envelope.message)
}

pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_server_with_limits(message, ProtocolLimits::default())
}

pub fn encode_server_with_limits(
    message: &ServerMessage,
    limits: ProtocolLimits,
) -> Result<Vec<u8>, ProtocolError> {
    message.validate(&limits)?;
    encode(message, limits)
}

pub fn decode_server(frame: &[u8]) -> Result<ServerMessage, ProtocolError> {
    decode_server_with_limits(frame, ProtocolLimits::default())
}

pub fn decode_server_with_limits(
    frame: &[u8],
    limits: ProtocolLimits,
) -> Result<ServerMessage, ProtocolError> {
    let envelope = decode::<ServerMessage>(frame, limits)?;
    envelope.message.validate(&limits)?;
    Ok(envelope.message)
}

fn validate_text(
    field: &str,
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
) -> Result<(), ProtocolError> {
    if !allow_empty && value.is_empty() {
        return Err(ProtocolError::InvalidMessage(format!(
            "{field} must not be empty"
        )));
    }
    if value.len() > max_bytes {
        return Err(ProtocolError::InvalidMessage(format!(
            "{field} exceeds the {max_bytes}-byte limit"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(ProtocolError::InvalidMessage(format!(
            "{field} contains a control character"
        )));
    }
    Ok(())
}

fn validate_position(position: [i32; 3]) -> Result<(), ProtocolError> {
    if position
        .iter()
        .any(|coordinate| coordinate.unsigned_abs() > MAX_WORLD_COORDINATE as u32)
    {
        return Err(ProtocolError::InvalidMessage(
            "world position is outside the supported world border".to_string(),
        ));
    }
    Ok(())
}

fn validate_finite_f32(field: &str, value: f32) -> Result<(), ProtocolError> {
    if !value.is_finite() {
        return Err(ProtocolError::InvalidMessage(format!(
            "{field} must be finite"
        )));
    }
    Ok(())
}

fn validate_finite_f64(field: &str, values: &[f64]) -> Result<(), ProtocolError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(ProtocolError::InvalidMessage(format!(
            "{field} must contain only finite values"
        )));
    }
    Ok(())
}

impl ClientMessage {
    pub fn validate(&self, limits: &ProtocolLimits) -> Result<(), ProtocolError> {
        match self {
            Self::Hello {
                protocol_version,
                username,
            } => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(ProtocolError::UnsupportedVersion(*protocol_version));
                }
                validate_text("username", username, limits.max_username_bytes, false)
            }
            Self::Input {
                movement,
                yaw,
                pitch,
                ..
            } => {
                for (index, value) in movement.iter().enumerate() {
                    validate_finite_f32(&format!("movement[{index}]"), *value)?;
                    if value.abs() > 1.0 {
                        return Err(ProtocolError::InvalidMessage(
                            "movement components must be between -1 and 1".to_string(),
                        ));
                    }
                }
                validate_finite_f32("yaw", *yaw)?;
                validate_finite_f32("pitch", *pitch)?;
                if !(-90.0..=90.0).contains(pitch) {
                    return Err(ProtocolError::InvalidMessage(
                        "pitch must be between -90 and 90 degrees".to_string(),
                    ));
                }
                Ok(())
            }
            Self::BlockEditRequest { position, .. } => {
                validate_position(*position)?;
                Ok(())
            }
            Self::InventoryActionRequest { slot, action, .. } => {
                if *slot as usize >= limits.max_inventory_slots {
                    return Err(ProtocolError::InvalidMessage(format!(
                        "inventory slot {slot} is outside the supported inventory"
                    )));
                }
                match action {
                    InventoryAction::SwapHotbar { hotbar_slot } if *hotbar_slot >= 9 => {
                        Err(ProtocolError::InvalidMessage(
                            "hotbar slot must be between 0 and 8".to_string(),
                        ))
                    }
                    InventoryAction::Drop { count } if *count == 0 => {
                        Err(ProtocolError::InvalidMessage(
                            "drop count must be greater than zero".to_string(),
                        ))
                    }
                    _ => Ok(()),
                }
            }
            Self::Chat { message } => {
                validate_text("chat message", message, limits.max_chat_bytes, false)
            }
            Self::KeepAlive { .. } | Self::Disconnect { .. } => Ok(()),
        }
    }
}

impl ServerMessage {
    pub fn validate(&self, limits: &ProtocolLimits) -> Result<(), ProtocolError> {
        match self {
            Self::Welcome {
                username, spawn, ..
            } => {
                validate_text("username", username, limits.max_username_bytes, false)?;
                validate_finite_f64("spawn", spawn)
            }
            Self::ChunkData { cx, cz, data, .. } => {
                if cx.unsigned_abs() > MAX_WORLD_COORDINATE as u32 / 16
                    || cz.unsigned_abs() > MAX_WORLD_COORDINATE as u32 / 16
                {
                    return Err(ProtocolError::InvalidMessage(
                        "chunk position is outside the supported world border".to_string(),
                    ));
                }
                if data.len() > limits.max_chunk_payload {
                    return Err(ProtocolError::MessageTooLarge {
                        actual: data.len(),
                        max: limits.max_chunk_payload,
                    });
                }
                Ok(())
            }
            Self::ChunkUnload { cx, cz } => {
                if cx.unsigned_abs() > MAX_WORLD_COORDINATE as u32 / 16
                    || cz.unsigned_abs() > MAX_WORLD_COORDINATE as u32 / 16
                {
                    return Err(ProtocolError::InvalidMessage(
                        "chunk position is outside the supported world border".to_string(),
                    ));
                }
                Ok(())
            }
            Self::PlayerSpawn {
                username, position, ..
            } => {
                validate_text("username", username, limits.max_username_bytes, false)?;
                validate_finite_f64("position", position)
            }
            Self::PlayerDespawn { .. } => Ok(()),
            Self::PlayerUpdate {
                position,
                velocity,
                yaw,
                pitch,
                ..
            } => {
                validate_finite_f64("position", position)?;
                for (index, value) in velocity.iter().enumerate() {
                    validate_finite_f32(&format!("velocity[{index}]"), *value)?;
                }
                validate_finite_f32("yaw", *yaw)?;
                validate_finite_f32("pitch", *pitch)?;
                if !(-90.0..=90.0).contains(pitch) {
                    return Err(ProtocolError::InvalidMessage(
                        "pitch must be between -90 and 90 degrees".to_string(),
                    ));
                }
                Ok(())
            }
            Self::BlockUpdate { position, .. } => validate_position(*position),
            Self::InventorySnapshot {
                slots, held_slot, cursor, ..
            } => {
                if slots.len() > limits.max_inventory_slots {
                    return Err(ProtocolError::InvalidMessage(format!(
                        "inventory contains {} slots, maximum is {}",
                        slots.len(),
                        limits.max_inventory_slots
                    )));
                }
                if *held_slot >= 9 {
                    return Err(ProtocolError::InvalidMessage(
                        "held slot must be between 0 and 8".to_string(),
                    ));
                }
                if slots
                    .iter()
                    .any(|stack| stack.count > 64 || (stack.count == 0 && (stack.item_id != 0 || stack.damage != 0)))
                {
                    return Err(ProtocolError::InvalidMessage(
                        "inventory stacks must be empty or contain between 1 and 64 items".to_string(),
                    ));
                }
                if cursor.count > 64 || (cursor.count == 0 && (cursor.item_id != 0 || cursor.damage != 0)) {
                    return Err(ProtocolError::InvalidMessage(
                        "inventory cursor must be empty or contain between 1 and 64 items".to_string(),
                    ));
                }
                Ok(())
            }
            Self::Chat {
                sender, message, ..
            } => {
                validate_text("sender", sender, limits.max_username_bytes, false)?;
                validate_text("chat message", message, limits.max_chat_bytes, false)
            }
            Self::KeepAlive { .. } | Self::ActionAccepted { .. } => Ok(()),
            Self::Reject { message, .. } | Self::Disconnect { message, .. } => {
                validate_text("server message", message, limits.max_chat_bytes, false)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionPhase {
    AwaitingHello,
    Active,
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    HelloAccepted,
    MessageAccepted,
    DisconnectRequested,
}

/// Small state guard for a connection. It does not mutate world state: each
/// accepted client message is still an intent for the authoritative server.
#[derive(Debug)]
pub struct SessionGuard {
    phase: SessionPhase,
    limits: ProtocolLimits,
    limiter: MessageRateLimiter,
    last_input_sequence: Option<u64>,
}

impl SessionGuard {
    pub fn new(now: Instant, limits: ProtocolLimits) -> Self {
        Self {
            phase: SessionPhase::AwaitingHello,
            limits,
            limiter: MessageRateLimiter::new(now, limits.max_messages_per_second),
            last_input_sequence: None,
        }
    }

    pub fn phase(&self) -> SessionPhase {
        self.phase
    }

    pub fn accept(
        &mut self,
        message: &ClientMessage,
        now: Instant,
    ) -> Result<SessionEvent, ProtocolError> {
        message.validate(&self.limits)?;
        if !self.limiter.allow_at(now) {
            return Err(ProtocolError::RateLimited);
        }

        match self.phase {
            SessionPhase::AwaitingHello => match message {
                ClientMessage::Hello { .. } => {
                    self.phase = SessionPhase::Active;
                    Ok(SessionEvent::HelloAccepted)
                }
                _ => Err(ProtocolError::HandshakeRequired),
            },
            SessionPhase::Active => match message {
                ClientMessage::Hello { .. } => Err(ProtocolError::UnexpectedMessage(
                    "hello was already accepted".to_string(),
                )),
                ClientMessage::Input { sequence, .. } => {
                    if let Some(previous) = self.last_input_sequence {
                        if *sequence <= previous {
                            return Err(ProtocolError::StaleInputSequence {
                                sequence: *sequence,
                                previous,
                            });
                        }
                    }
                    self.last_input_sequence = Some(*sequence);
                    Ok(SessionEvent::MessageAccepted)
                }
                ClientMessage::Disconnect { .. } => {
                    self.phase = SessionPhase::Closing;
                    Ok(SessionEvent::DisconnectRequested)
                }
                _ => Ok(SessionEvent::MessageAccepted),
            },
            SessionPhase::Closing => Err(ProtocolError::SessionClosing),
        }
    }
}

#[derive(Debug)]
struct MessageRateLimiter {
    window_start: Instant,
    count: u32,
    max_messages: u32,
}

impl MessageRateLimiter {
    fn new(now: Instant, max_messages: u32) -> Self {
        Self {
            window_start: now,
            count: 0,
            max_messages,
        }
    }

    fn allow_at(&mut self, now: Instant) -> bool {
        let elapsed = now
            .checked_duration_since(self.window_start)
            .unwrap_or(Duration::ZERO);
        if elapsed >= Duration::from_secs(1) {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= self.max_messages {
            return false;
        }
        self.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> ClientMessage {
        ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            username: "Alex".to_string(),
        }
    }

    #[test]
    fn client_message_round_trips_through_versioned_frame() {
        let message = ClientMessage::BlockEditRequest {
            request_id: 7,
            position: [12, 64, -4],
            face: Face::Up,
            action: BlockEditAction::Place {
                state: WireBlockState {
                    block_id: 5,
                    state: 0,
                    data: 0,
                },
            },
            expected_revision: 19,
        };

        let frame = encode_client(&message).expect("valid message should encode");
        assert_eq!(decode_client(&frame), Ok(message));
        assert_eq!(
            u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize,
            frame.len() - 4
        );
    }

    #[test]
    fn server_chunk_message_round_trips_with_binary_payload() {
        let message = ServerMessage::ChunkData {
            cx: -2,
            cz: 4,
            revision: 11,
            data: vec![0, 1, 2, 255],
        };
        let frame = encode_server(&message).expect("valid message should encode");
        assert_eq!(decode_server(&frame), Ok(message));
    }

    #[test]
    fn server_chunk_unload_message_round_trips() {
        let message = ServerMessage::ChunkUnload { cx: -2, cz: 4 };
        let frame = encode_server(&message).expect("valid message should encode");
        assert_eq!(decode_server(&frame), Ok(message));
    }

    #[test]
    fn compact_chunk_codec_round_trips_and_rejects_trailing_data() {
        let chunk = crate::world::chunk::Chunk::new(2, -3);
        let data = ChunkData::from_chunk(&chunk);
        let encoded = encode_chunk_data(&data).unwrap();
        assert!(encoded.len() < MAX_CHUNK_PAYLOAD);
        assert_eq!(decode_chunk_data(&encoded).unwrap(), data);

        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            decode_chunk_data(&trailing),
            Err(ProtocolError::InvalidChunkPayload(_))
        ));
    }

    #[test]
    fn version_and_frame_size_are_rejected_before_message_use() {
        let mut unsupported = serde_json::to_vec(&WireEnvelope {
            version: PROTOCOL_VERSION + 1,
            message: hello(),
        })
        .unwrap();
        let mut frame = (unsupported.len() as u32).to_be_bytes().to_vec();
        frame.append(&mut unsupported);
        assert_eq!(
            decode_client(&frame),
            Err(ProtocolError::UnsupportedVersion(PROTOCOL_VERSION + 1))
        );

        let oversized = (MAX_FRAME_PAYLOAD as u32 + 1).to_be_bytes();
        assert_eq!(
            decode_client(&oversized),
            Err(ProtocolError::MessageTooLarge {
                actual: MAX_FRAME_PAYLOAD + 1,
                max: MAX_FRAME_PAYLOAD,
            })
        );
    }

    #[test]
    fn truncated_and_trailing_frames_are_rejected() {
        let frame = encode_client(&hello()).unwrap();
        assert!(matches!(
            decode_client(&frame[..frame.len() - 1]),
            Err(ProtocolError::TruncatedFrame { .. })
        ));

        let mut trailing = frame.clone();
        trailing.push(0);
        assert_eq!(
            decode_client(&trailing),
            Err(ProtocolError::TrailingBytes { extra: 1 })
        );

        let invalid_json = [0, 0, 0, 3, b'n', b'o', b'p'];
        assert!(matches!(
            decode_client(&invalid_json),
            Err(ProtocolError::InvalidEncoding(_))
        ));
    }

    #[test]
    fn frame_decoder_handles_partial_and_batched_frames() {
        let first = encode_client(&hello()).unwrap();
        let second = encode_client(&ClientMessage::KeepAlive { nonce: 9 }).unwrap();
        let mut combined = first.clone();
        combined.extend_from_slice(&second);
        let mut decoder = FrameDecoder::new(ProtocolLimits::default());

        assert!(decoder.push(&combined[..2]).unwrap().is_empty());
        let frames = decoder.push(&combined[2..]).unwrap();
        assert_eq!(frames, vec![first, second]);
        assert_eq!(decoder.buffered_bytes(), 0);
    }

    #[test]
    fn invalid_messages_are_rejected_by_direction_specific_codecs() {
        let invalid_input = ClientMessage::Input {
            sequence: 1,
            movement: [f32::NAN, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            jump: false,
            sprint: false,
            sneak: false,
        };
        assert!(matches!(
            encode_client(&invalid_input),
            Err(ProtocolError::InvalidMessage(_))
        ));

        let invalid_server = ServerMessage::InventorySnapshot {
            revision: 1,
            slots: vec![WireItemStack {
                item_id: 1,
                count: 65,
                damage: 0,
            }],
            held_slot: 0,
            cursor: WireItemStack { item_id: 0, count: 0, damage: 0 },
        };
        assert!(matches!(
            encode_server(&invalid_server),
            Err(ProtocolError::InvalidMessage(_))
        ));
    }

    #[test]
    fn session_guard_enforces_handshake_sequence_and_disconnect() {
        let now = Instant::now();
        let mut guard = SessionGuard::new(now, ProtocolLimits::default());
        let input = ClientMessage::Input {
            sequence: 1,
            movement: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            jump: false,
            sprint: false,
            sneak: false,
        };

        assert_eq!(
            guard.accept(&input, now),
            Err(ProtocolError::HandshakeRequired)
        );
        assert_eq!(guard.accept(&hello(), now), Ok(SessionEvent::HelloAccepted));
        assert_eq!(guard.phase(), SessionPhase::Active);
        assert_eq!(guard.accept(&input, now), Ok(SessionEvent::MessageAccepted));
        assert_eq!(
            guard.accept(&input, now),
            Err(ProtocolError::StaleInputSequence {
                sequence: 1,
                previous: 1,
            })
        );
        assert_eq!(
            guard.accept(
                &ClientMessage::Disconnect {
                    reason: DisconnectReason::ClientQuit,
                },
                now,
            ),
            Ok(SessionEvent::DisconnectRequested)
        );
        assert_eq!(guard.phase(), SessionPhase::Closing);
        assert_eq!(
            guard.accept(&hello(), now),
            Err(ProtocolError::SessionClosing)
        );
    }

    #[test]
    fn session_guard_applies_a_windowed_message_rate_limit() {
        let now = Instant::now();
        let limits = ProtocolLimits {
            max_messages_per_second: 2,
            ..ProtocolLimits::default()
        };
        let mut guard = SessionGuard::new(now, limits);
        assert_eq!(guard.accept(&hello(), now), Ok(SessionEvent::HelloAccepted));
        assert_eq!(
            guard.accept(&ClientMessage::KeepAlive { nonce: 1 }, now),
            Ok(SessionEvent::MessageAccepted)
        );
        assert_eq!(
            guard.accept(&ClientMessage::KeepAlive { nonce: 2 }, now),
            Err(ProtocolError::RateLimited)
        );
        assert_eq!(
            guard.accept(
                &ClientMessage::KeepAlive { nonce: 3 },
                now + Duration::from_secs(1),
            ),
            Ok(SessionEvent::MessageAccepted)
        );
    }
}
