# Vibecraft

Vibecraft is a native Rust Minecraft-like game prototype targeting Java
Edition 1.21.1 gameplay and assets. It currently provides a playable
singleplayer Overworld with procedural terrain, lighting, block interaction,
inventory, survival values, native persistence, commands, audio, and a wgpu
renderer.

The project is under active development. It is not Java protocol, Anvil/NBT,
resource-pack, or data-pack compatible.

## Requirements

- Rust stable and Cargo
- A GPU and window-system environment supported by wgpu and winit
- Minecraft assets in a directory containing `assets/minecraft`
- An audio device is recommended; audio initialization is best-effort

Vibecraft first looks for the supplied sibling checkout
`../minecraft-26.2-assets` (relative to this repository), then for the legacy
`/tmp/opencode/minecraft-assets` checkout. Set `VIBECRAFT_ASSETS` when using
another location:

```sh
export VIBECRAFT_ASSETS=/path/to/minecraft-assets
```

`VIBECRAFT_ASSETS` may point either at the checkout root containing
`assets/minecraft` or directly at `assets/minecraft`.

## Build And Run

Build the game:

```sh
cargo build
```

Run a release build:

```sh
cargo run --release
```

Run a deterministic world with explicit settings:

```sh
cargo run --release -- \
  --seed 1592639710 \
  --world-dir world/demo \
  --render-distance 6 \
  --graphics regular
```

The game creates the selected world directory and stores native JSON level,
player, and chunk data there. It autosaves during play, saves on window close,
and supports `/save` and `/quit`. Existing saves are validated and migrated by
native data version; they are not Minecraft Java save files. Migrated worlds
retain their legacy generation interpolation profile, while newly created
local and server worlds use the corrected `minecraft26_native_decoration_preview`
profile. Existing `minecraft26_base` worlds remain undecorated.
The preview has bounded oak/spruce trees and rare desert wells, but is not
Java feature-index compatible and does not claim Java world-output parity.

## Native Decoration Preview

`minecraft26_native_decoration_preview` plans candidates from fixed owner
halos, uses isolated undecorated snapshots for bounded support checks, and
projects only cells owned by the generated target chunk. It does not access or
mutate neighboring runtime chunks, so target output is independent of chunk
load or generation order.

The supported tree subset is oak in plains/forest variants and spruce in
taiga/grove variants. Trees use vertical default-data logs and default leaf
state only; horizontal log axes and leaf distance/persistence state are not
implemented. The rare desert well is intentionally incomplete: it uses only
sandstone and water, without sandstone slabs, suspicious sand, loot, or any
block-entity behavior. None of these features uses Java configured/placed
feature indexes or claims Java parity.

## Configuration

Command-line options override the JSON configuration file. By default the game
looks for `vibecraft.json`; use `VIBECRAFT_CONFIG` or `--config` to select a
different file.

```sh
cargo run --release -- --config vibecraft.example.json
VIBECRAFT_CONFIG=config/dev.json cargo run --release
```

Available options:

| Option | Description |
|---|---|
| `--config PATH` | Load a JSON configuration file. |
| `--seed U64` | Use a deterministic world seed. Without it, a time-based seed is generated. |
| `--world-dir PATH` | Select the native save directory. |
| `--render-distance 2..32` | Set the chunk loading radius. |
| `--graphics regular\|vibrant` | Select render quality. |
| `--keybind ACTION=KEY` | Override a supported key binding. Repeatable. |
| `--server IP:PORT` | Store a server endpoint for the networking work in progress. The windowed client does not yet connect to it. |
| `--username NAME` | Set the future network username. |

The complete JSON shape and default key bindings are in
`vibecraft.example.json`. Supported key names include `KeyW`, `KeyA`, `KeyS`,
`KeyD`, `Space`, `ShiftLeft`, `ControlLeft`, `KeyE`, `KeyQ`, `KeyT`, and
`Slash`.

## Controls

Default gameplay controls:

| Key | Action |
|---|---|
| `W`, `A`, `S`, `D` | Move. |
| `Space` | Jump. |
| `Shift` | Sneak. |
| `Control` | Sprint. |
| Mouse movement | Look around. |
| Left mouse button | Break the targeted block. |
| Right mouse button | Place the selected block. |
| `1` through `9` | Select a hotbar slot. |
| `E` | Open or close the inventory. |
| `Q` | Drop the selected item. |
| `T` | Open chat input. |
| `/` | Open command input. |
| `F` | Toggle flight where the current game mode permits it. |
| `Escape` | Exit the game. |

Development and rendering shortcuts:

| Key | Action |
|---|---|
| `F2` | Capture a screenshot. |
| `F3` | Toggle the debug overlay. |
| `F3` + `G` | Toggle chunk borders. |
| `F4` | Toggle Regular/Vibrant graphics. |
| `F5` | Toggle the profiler; disabling it writes profiler output. |

## Commands

Commands are local gameplay/debug commands. They are not Java Edition protocol
commands. Press `/` or the configured command key, then enter a command.

Common commands:

```text
/help
/seed
/gamemode survival|creative|adventure|spectator
/difficulty peaceful|easy|normal|hard
/time set day|noon|night|midnight|<number>
/give <block> [count]
/hotbar <block>
/clearinventory
/heal
/feed
/effect <effect> [duration] [amplifier]
/save
/quit
```

Structure helpers such as `/summon dungeon`, `/place ruined_portal`, and
`/tree` require a loaded block target. See `COMMANDS.md` for the full command,
alias, effect, structure, and argument reference.

Some commands intentionally report requested state without implementing the
full vanilla system. In particular, weather visuals, complete gamerule
enforcement, broad item lookup, and general entity targeting are incomplete.

## Headless Server

The repository also contains a winit/wgpu-free native server binary:

```sh
cargo run --bin vibecraft-server -- \
  --bind 127.0.0.1:25565 \
  --world-dir server-world \
  --seed 42 \
  --render-distance 6 \
  --max-players 8
```

The server owns a fixed 20 TPS simulation loop, native persistence, bounded TCP
sessions, handshake validation, keep-alives, authoritative player sessions,
movement intent, loaded-chunk streaming, and server-side block-edit checks.
Type `quit` or `stop` in the server console to save and shut down. End-of-file
also requests a clean shutdown.

The native protocol is documented in `NETWORK_PROTOCOL.md`. The reusable
`ClientTransport` and protocol messages exist, but the windowed executable is
still a singleplayer client and does not yet join the headless server. A full
two-client playable release therefore remains roadmap work.

## Testing And Verification

Run the library tests, which do not require a GPU or window:

```sh
cargo test --lib
```

Build both application paths:

```sh
cargo build
cargo build --bin vibecraft-server
```

The normal startup smoke test is:

```sh
cargo build && timeout 15 cargo run --release
```

The timeout is expected after a healthy 15-second run. A panic, GPU validation
error, asset-loading error, or freeze is not expected.

For renderer changes, use the fixed scene in `RENDER_CHECK.md`. It specifies a
seed, world directory, asset root, screenshot timing, graphics-quality pass,
resize check, and comparison baseline.

## Repository Guide

| Path | Purpose |
|---|---|
| `src/main.rs` | Windowed application, input, gameplay loop, commands, and rendering coordination. |
| `src/lib.rs` | Reusable library entry point for simulation and tests. |
| `src/engine/` | Camera, input, window, renderer, text, and audio systems. |
| `src/assets/` | Minecraft blockstate/model parsing, textures, language, and GUI asset loading. |
| `src/world/` | Blocks, chunks, world generation, lighting, meshing, entities, fluids, simulation, raycast, and persistence. |
| `src/player/` | Movement, collision, survival values, and status effects. |
| `src/inventory/` | Items, stacks, inventory operations, crafting, and furnace progression. |
| `src/network/` | Native protocol, compact chunk codec, server runtime, and client transport. |
| `src/bin/vibecraft-server.rs` | Headless server executable. |
| `src/shaders/` | WGSL terrain, lighting, sky, GUI, text, highlight, and break shaders. |
| `PLAN.md` | Active roadmap and implementation status. |
| `ISSUES.md` | Known bugs, risks, and investigations. |
| `AGENTS.md` | Architecture and contribution rules for this repository. |

## Current Limitations

- The windowed client is not yet connected to the headless server.
- Multiplayer replication, reconnect, client reconciliation, and graphical
  chat/inventory screens are incomplete.
- The implementation is not Java protocol or save-format compatible.
- Vanilla parity is incomplete across blocks, fluids, structures, mobs,
  dimensions, redstone, weather, sounds, and accessibility.
- Asset loading expects a Minecraft asset checkout and does not silently replace
  missing production assets with arbitrary content.

See `PLAN.md` for the prioritized delivery order.
