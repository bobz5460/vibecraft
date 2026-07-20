# Vibecraft Commands And Arguments

This document describes commands implemented by the native prototype. They use Java Edition-inspired syntax where the matching simulation behavior exists; they are not Java protocol commands.

## Launch Arguments

```text
vibecraft [--config PATH] [--seed U64] [--world-dir PATH]
          [--server IP:PORT] [--username NAME]
          [--render-distance 2..32] [--graphics regular|vibrant]
          [--keybind ACTION=KEY]
```

| Argument | Value | Behavior |
|---|---|---|
| `--help`, `-h` | none | Print launch usage and exit. |
| `--config` | `PATH` | Load JSON configuration. Command-line values override it. |
| `--seed` | unsigned 64-bit integer | Select the deterministic generation seed. |
| `--world-dir` | `PATH` | Select a native world directory. |
| `--server` | `IP:PORT` | Connect to a native Vibecraft server, not a Java Edition server. |
| `--username` | `NAME` | Set the native multiplayer username. |
| `--render-distance` | integer `2` through `32` | Set the loaded chunk radius. |
| `--graphics` | `regular` or `vibrant` | Set initial render quality. |
| `--keybind` | `ACTION=KEY` | Override a core key binding. |

`VIBECRAFT_ASSETS` may point to an asset checkout containing `assets/minecraft`
or directly to `assets/minecraft`. Without it, Vibecraft first uses the supplied
`../minecraft-26.2-assets` checkout and then `/tmp/opencode/minecraft-assets`.

## Chat

Press the configured command key (`/` by default) to open chat with a slash already entered. Press the configured chat key (`T` by default) to open an empty editor.

The editor supports up to 256 characters, 100 stored messages, viewport wrapping, mouse-wheel scrollback, Up/Down sent-entry recall, cursor editing, and Tab completion for supported commands and arguments. Chat components, links, hover/click events, text filtering, and signing are not implemented.

While connected to a native server, regular messages are sent to the server. Slash commands are rejected locally because the native protocol has no server command-request message and a client must not mutate authority.

## In-Game Commands

All commands below are local singleplayer/debug commands. Item and block IDs accept an optional `minecraft:` namespace when supported by the current registry.

| Command | Supported behavior |
|---|---|
| `/gamemode [mode]` | Query or set `survival`, `creative`, `adventure`, or `spectator`. `/gm` remains a prototype alias. |
| `/difficulty [value]` | Query or set `peaceful`, `easy`, `normal`, or `hard`. `/d` remains a prototype alias. |
| `/hardcore` | Enable hardcore, Hard difficulty, and Survival mode for this session. |
| `/time set <value>` | Set `day`, `noon`, `night`, `midnight`, or a numeric local time. |
| `/time query <value>` | Query `daytime` or `gametime`. |
| `/teleport [@s] <x> <y> <z>` | Teleport the local player. `/tp` is an alias. Absolute values and `~` relative coordinates work. |
| `/setblock <x> <y> <z> <block>` | Replace one loaded block and immediately rebuild affected chunks. |
| `/fill <from> <to> <block> [mode]` | Fill up to 32,768 blocks. `keep` works; `replace` and `destroy` currently both replace because command drops are unsupported. |
| `/setworldspawn [x y z]` | Set persistent world spawn at the player or an explicit position. |
| `/seed` | Display the active generation seed. |
| `/experience add <amount> [points]` | Add experience. `/xp <amount>` remains supported as a compact legacy form. |
| `/give <item> [count]` | Give up to 64 supported items or blocks. |
| `/clear [@s] [item]` | Clear the inventory or matching local item stacks. `/clearinventory` and `/ci` remain prototype aliases. |
| `/effect <effect> [seconds] [amplifier]` | Apply a supported status effect. `/effect clear` clears all active effects. |
| `/gamerule <rule> [value]` | Query or set `doDaylightCycle` and `keepInventory`. |
| `/kill [@s]` | Kill the local player. Other targets need the general entity platform. |
| `/save` | Save the native world immediately. |
| `/quit` | Save then exit; a failed save prevents exit. |
| `/help` | Show the compact supported command list. |

`/weather` accepts `clear`, `rain`, and `thunder`, but weather simulation and visuals do not yet exist. `/armor`, `/heal`, `/feed`, `/hotbar`, and the structure shortcuts below are prototype debugging aids, not Java Edition command compatibility.

## Legacy Structure Shortcuts

These commands require targeting a loaded block and are debug-only. They do not match Java Edition's entity `/summon` or configured-feature `/place` semantics.

```text
/summon dungeon
/place ruined_portal
/tree
```

Supported shortcut names: `dungeon`, `ruined_portal`, `lava_pool`, `giant_mushroom`, `tree`, `igloo`, `swamp_hut`, `desert_well`, and `ocean_ruin`.

## Runtime Shortcuts

| Key | Behavior |
|---|---|
| `F2` | Request a screenshot. |
| `F3` | Toggle debug overlay. |
| `F3` + `G` | Toggle chunk borders. |
| `F4` | Toggle Regular/Vibrant graphics. |
| `F5` | Toggle the profiler; toggling it off saves profiler output. |
| `F` | Toggle flight in a fly-capable game mode. |
| `1` through `9` | Select hotbar slot. |
| `Escape` | Open or close the pause menu. |
