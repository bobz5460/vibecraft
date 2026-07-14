# Native Multiplayer Protocol

The demo protocol is version `1` and is native to Vibecraft. It is not the
Java Edition protocol and does not promise Java client/server compatibility.

## Framing

Each message is a UTF-8 JSON envelope prefixed by a four-byte unsigned big-
endian payload length:

```text
u32 payload_length | {"version":1,"message":...}
```

The default payload limit is 1 MiB. Chunk payloads are limited to 768 KiB,
chat and server error text to 256 bytes, usernames to 16 bytes, inventories to
46 slots, and inbound messages to 40 per second per session. Frames with a
truncated body, trailing bytes, an unsupported version, invalid JSON, or a
limit violation are rejected before the message reaches simulation code.

## Handshake And Session

1. The client sends `Hello { protocol_version, username }`.
2. The server validates the version and username, then sends `Welcome` and the
   initial authoritative state.
3. The client may send movement input, block-edit requests, inventory-action
   requests, chat, keep-alives, or a disconnect request.
4. A client input sequence must increase monotonically. A session that has not
   completed `Hello` cannot send gameplay messages. A closing session accepts
   no further messages.

Unsupported versions, malformed requests, stale input, rate limits, and stale
block/inventory revisions map to a `Reject` or `Disconnect` response before
the server changes state. Unsupported versions use the explicit
`DisconnectCode::UnsupportedVersion` path.

## Replicated State

`WireBlockState` carries the raw block ID, compact registry state ordinal, and
legacy data byte. Chunk payloads use the bounded `VCC1` run-length codec rather
than JSON per-cell arrays; block-entity metadata remains JSON inside that
bounded payload. A `ChunkData` revision is the server's mutation revision for
the chunk and is required on block-edit requests.

`ChunkData` adds an authoritative chunk snapshot; `ChunkUnload` removes a
snapshot that is outside the player's current view. Clients must discard the
chunk and its revision on unload so a later re-entry can accept a fresh server
revision. `PlayerSpawn` announces a newly active session and `PlayerDespawn`
removes it. The server sends authoritative `PlayerUpdate` messages after each
fixed tick.
`InventorySnapshot` contains the complete slot array, selected hotbar slot,
revision, and server-owned cursor stack. `InventoryAction::Click` requests a
basic left/right cursor operation. Dropped-item entity replication is not part
of the demo protocol yet, so `InventoryAction::Drop` is rejected rather than
mutating inventory without creating an authoritative world item. Unsupported
container modes are also rejected.

## Authority

Clients send intent only. The server owns player transforms, block states,
chunk revisions, inventories, entity state, game time, and persistence. The
`position` in a block request is a target, not an instruction to teleport; the
server must re-check reach, permissions, expected revision, collision, and
available inventory. Server messages such as `BlockUpdate`, `PlayerUpdate`,
and `InventorySnapshot` are authoritative snapshots and may supersede local
client state.

The protocol module defines and tests the wire contract. The headless server
owns the fixed-tick persistence loop, TCP session, authoritative player state,
movement intent, multi-player-centered loaded-chunk streaming, block-edit
validation, inventory cursor operations, and username-keyed native player
persistence. The windowed executable consumes authoritative snapshots in
`--server` mode and retries disconnected sessions; prediction/reconciliation,
container-specific actions, and external compatibility remain unsupported. A
full server receives `ServerFull` rather than being allowed to mutate state or
wait indefinitely.
