//! Nonblocking native protocol transport for the windowed client.
//!
//! This module owns bytes and session sequencing only. World snapshots remain
//! owned by the application so the transport cannot accidentally become a
//! second simulation authority.

use super::{encode_client_with_limits, ClientMessage, DisconnectCode, FrameDecoder, ProtocolError, ProtocolLimits, ServerMessage, PROTOCOL_VERSION};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};
use thiserror::Error;

const READ_BUFFER_BYTES: usize = 16 * 1024;
const MAX_OUTBOUND_FRAMES: usize = 128;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("client connection failed: {0}")]
    Io(#[from] io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("server closed the connection")]
    Closed,
    #[error("server disconnected the client ({code:?}): {message}")]
    ServerDisconnected { code: DisconnectCode, message: String },
    #[error("client transport is closing")]
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientPhase {
    AwaitingWelcome,
    Active,
    Closing,
}

pub struct ClientTransport {
    stream: TcpStream,
    decoder: FrameDecoder,
    limits: ProtocolLimits,
    outbox: VecDeque<OutboundFrame>,
    phase: ClientPhase,
    last_activity: Instant,
}

impl ClientTransport {
    pub fn connect(address: SocketAddr, username: String) -> Result<Self, ClientError> {
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        stream.set_nonblocking(true)?;
        let limits = ProtocolLimits::default();
        let now = Instant::now();
        let mut transport = Self {
            stream,
            decoder: FrameDecoder::new(limits),
            limits,
            outbox: VecDeque::new(),
            phase: ClientPhase::AwaitingWelcome,
            last_activity: now,
        };
        transport.queue(ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            username,
        })?;
        transport.flush()?;
        Ok(transport)
    }

    pub fn phase(&self) -> ClientPhase {
        self.phase
    }

    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    pub fn send(&mut self, message: ClientMessage) -> Result<(), ClientError> {
        if self.phase == ClientPhase::Closing {
            return Err(ClientError::Closing);
        }
        self.queue(message)?;
        self.flush()
    }

    /// Polls all currently available server messages without blocking.
    pub fn poll(&mut self) -> Result<Vec<ServerMessage>, ClientError> {
        self.flush()?;
        let mut messages = Vec::new();
        let mut buffer = [0u8; READ_BUFFER_BYTES];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    self.phase = ClientPhase::Closing;
                    return Err(ClientError::Closed);
                }
                Ok(read) => {
                    self.last_activity = Instant::now();
                    for frame in self.decoder.push(&buffer[..read])? {
                        let message = super::decode_server_with_limits(&frame, self.limits)?;
                        self.observe(&message)?;
                        messages.push(message);
                    }
                    if read < buffer.len() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(ClientError::Io(error)),
            }
        }
        Ok(messages)
    }

    pub fn disconnect(&mut self) -> Result<(), ClientError> {
        if self.phase == ClientPhase::Closing {
            return Ok(());
        }
        self.queue(ClientMessage::Disconnect { reason: super::DisconnectReason::ClientQuit })?;
        self.phase = ClientPhase::Closing;
        self.flush()
    }

    fn queue(&mut self, message: ClientMessage) -> Result<(), ClientError> {
        if self.outbox.len() >= MAX_OUTBOUND_FRAMES {
            self.phase = ClientPhase::Closing;
            return Err(ClientError::Closing);
        }
        let bytes = encode_client_with_limits(&message, self.limits)?;
        self.outbox.push_back(OutboundFrame { bytes, offset: 0 });
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ClientError> {
        while let Some(frame) = self.outbox.front_mut() {
            match self.stream.write(&frame.bytes[frame.offset..]) {
                Ok(0) => return Err(ClientError::Closed),
                Ok(written) => {
                    frame.offset += written;
                    self.last_activity = Instant::now();
                    if frame.offset == frame.bytes.len() {
                        self.outbox.pop_front();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(ClientError::Io(error)),
            }
        }
        Ok(())
    }

    fn observe(&mut self, message: &ServerMessage) -> Result<(), ClientError> {
        match message {
            ServerMessage::Welcome { .. } => self.phase = ClientPhase::Active,
            ServerMessage::Disconnect { code, message } => {
                self.phase = ClientPhase::Closing;
                return Err(ClientError::ServerDisconnected { code: code.clone(), message: message.clone() });
            }
            _ => {}
        }
        Ok(())
    }
}

struct OutboundFrame {
    bytes: Vec<u8>,
    offset: usize,
}
