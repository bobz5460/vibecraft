# Vibecraft Commands And Arguments

This document describes the commands currently implemented by the native prototype. Commands are local singleplayer/debug commands, not Java Edition protocol commands.

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
| `--config` | `PATH` | Load JSON configuration from `PATH`. The default path is `vibecraft.json`; `VIBECRAFT_CONFIG` can select a different default. Command-line values override file values. |
| `--seed` | unsigned 64-bit integer | Select the deterministic world-generation seed. Without this option or a JSON `seed`, startup creates a time-based seed. Use `/seed` in game to display it. |
| `--world-dir` | `PATH` | Create and select a native world directory for level, player, and chunk save data. |
| `--server` | `IP:PORT` | Store a native server endpoint for the networking work in progress. The windowed client does not connect yet. |
| `--username` | `NAME` | Set the future network username. |
| `--render-distance` | integer `2` through `32` | Set the chunk radius loaded around the player. |
| `--graphics` | `regular` or `vibrant` | Set the initial render quality. `F4` toggles it while running. |
| `--keybind` | `ACTION=KEY` | Override one core binding. This option can be specified more than once. |

`VIBECRAFT_ASSETS` must point to an asset root containing `assets/minecraft`. It defaults to `/tmp/opencode/minecraft-assets`.

The JSON schema and defaults are shown in `vibecraft.example.json`.

### Keybinding Actions

Supported actions are `forward`, `back`, `left`, `right`, `jump`, `sneak`, `sprint`, `inventory`, `drop_item`, `chat`, and `command`.

Supported key names are `KeyW`, `KeyA`, `KeyS`, `KeyD`, `KeyE`, `KeyQ`, `KeyT`, `Space`, `ShiftLeft`, `ControlLeft`, and `Slash`. Invalid actions or key names stop startup with an error.

Examples:

```sh
cargo run --release -- --seed 42 --render-distance 8 --graphics vibrant
cargo run -- --keybind forward=KeyD --keybind right=KeyW
```

## In-Game Commands

Press the configured command key (`/` by default) to open command input. Press the configured chat key (`T` by default) to open chat input; a message beginning with `/` is executed as a command.

### Player And World State

| Command | Aliases | Arguments | Behavior |
|---|---|---|---|
| `/gamemode [mode]` | `/gm` | `survival`, `creative`, `adventure`, or `spectator` | With no mode, display the current mode. Hardcore prevents changing away from Survival. |
| `/difficulty [difficulty]` | `/d` | `peaceful`, `easy`, `normal`, or `hard` | With no argument, display the current difficulty. Hardcore locks it to Hard. |
| `/hardcore` | `/hc` | none | Enable Hardcore, set Hard difficulty, and set Survival mode. This cannot be undone during the session. |
| `/time set <time>` | none | `day`, `noon`, `night`, `midnight`, or a number | Set time modulo 1200 seconds. `day=300`, `noon=450`, `night=900`, and `midnight=0`. |
| `/seed` | none | none | Display the active generation seed. |
| `/xp [amount]` | none | non-negative integer | With no amount, display total XP. Otherwise add the amount. |
| `/kill [target]` | none | omitted, `@s`, or `player` | Kill the local player. Other targets are rejected because there is no general entity system. |
| `/weather <type>` | none | `clear`, `rain`, `rainy`, `thunder`, or `storm` | Reports the requested weather, but weather simulation and visuals are not implemented. |
| `/gamerule doDaylightCycle <true\|false>` | `/g` | one supported rule and Boolean value | Reports the requested value only. Gamerule storage/enforcement is not implemented. |
| `/save` | none | none | Save the current native world immediately. |
| `/quit` | none | none | Save and exit. A failed save prevents exit so it can be retried. |
| `/help` | `/?`, `/h` | none | Print the compact in-game command list. |

### Inventory And Player Values

| Command | Aliases | Arguments | Behavior |
|---|---|---|---|
| `/give <block> [count]` | none | a supported block name; optional integer | Add up to 64 items to inventory. The count defaults to 1; a full inventory may accept fewer. Item lookup currently supports the block names recognized by `BlockId::from_name`, not every Java Edition item. |
| `/hotbar <block>` | `/hb` | a supported block name | Replace all nine hotbar slots with stacks of 64 of that block. |
| `/clearinventory` | `/ci` | none | Clear all inventory slots. |
| `/heal` | none | none | Restore health, hunger, and saturation; remove absorption health. |
| `/feed` | `/eat` | none | Restore hunger and saturation. |
| `/armor [points] [toughness]` | none | floating-point values | With no values, display current armor and toughness. With `points`, set armor; an optional valid toughness value replaces toughness. |
| `/effect <effect> [duration] [amplifier]` | `/ef` | effect name, seconds, and zero-based amplifier | Apply an effect. Duration defaults to 30 seconds and amplifier defaults to 0. `/effect clear` and `/effect remove_all` remove every active effect. |

Supported effect names:

```text
speed, slowness (slow), haste, mining_fatigue (fatigue), strength (str),
jump_boost (jump), regeneration (regen), resistance (resist),
fire_resistance (fire_resist), water_breathing (water_breath),
night_vision (nv), invisibility (invis), absorption (abs),
slow_falling (slowfall), dolphin_grace (dolphin), weakness, poison, wither,
hunger, nausea, blindness (blind), levitation (levi), darkness (dark),
instant_health (insta_heal), instant_damage (insta_dmg),
health_boost (hp_boost), saturation (sat), fatal_poison (fatal),
bad_omen (omen), hero_of_the_village (hero), wind_charged (wind),
infested, oozing (ooze), weaving (weave)
```

### Structure Placement

These commands require targeting a loaded block. The structure is placed at the targeted block coordinates. They use session-random layout details, so they are not deterministic fixture generators.

| Form | Accepted structure names |
|---|---|
| `/summon <structure>` | all names below |
| `/place <structure>` | all names below |
| `/<structure>` | all names below |

| Structure | Accepted names |
|---|---|
| Dungeon | `dungeon`, `d` |
| Ruined portal | `portal`, `ruined_portal`, `p` |
| Lava pool | `lava`, `lava_pool`, `l` |
| Giant mushroom | `mushroom`, `giant_mushroom`, `m` |
| Oak tree | `tree`, `oak`, `t` |
| Igloo | `igloo`, `i` |
| Swamp hut | `swamp_hut`, `hut`, `sh` |
| Desert well | `well`, `desert_well`, `w` |
| Ocean ruin | `ruin`, `ocean_ruin`, `r` |

Examples:

```text
/summon dungeon
/place ruined_portal
/tree
```

## Runtime Shortcuts

These are not slash commands, but are useful while testing:

| Key | Behavior |
|---|---|
| `F2` | Request a screenshot. |
| `F3` | Toggle debug overlay. |
| `F3` + `G` | Toggle chunk borders. |
| `F4` | Toggle Regular/Vibrant graphics. |
| `F5` | Toggle the profiler; toggling it off saves profiler output. |
| `F` | Toggle flight in a fly-capable game mode. |
| `1` through `9` | Select hotbar slot. |
| `Escape` | Exit the application. |
