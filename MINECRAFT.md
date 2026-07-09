# Minecraft: Complete Feature & Mechanics Reference

> Compiled from [minecraft.wiki](https://minecraft.wiki/). Covers Java Edition (latest: 26.2) and Bedrock Edition (latest: 26.32). Last updated: July 2026.

---

## Table of Contents

1. [Game Modes & Difficulty](#1-game-modes--difficulty)
2. [Dimensions](#2-dimensions)
3. [World Generation & Biomes](#3-world-generation--biomes)
4. [Blocks](#4-blocks)
5. [Items & Inventory](#5-items--inventory)
6. [Mobs & Entities](#6-mobs--entities)
7. [Player Mechanics](#7-player-mechanics)
8. [Combat](#8-combat)
9. [Enchanting](#9-enchanting)
10. [Brewing & Potions](#10-brewing--potions)
11. [Status Effects](#11-status-effects)
12. [Crafting & Smelting](#12-crafting--smelting)
13. [Trading & Bartering](#13-trading--bartering)
14. [Redstone](#14-redstone)
15. [Transportation](#15-transportation)
16. [Environment & Physics](#16-environment--physics)
17. [Lighting](#17-lighting)
18. [Weather & Daylight Cycle](#18-weather--daylight-cycle)
19. [Explosions](#19-explosions)
20. [Archaeology](#20-archaeology)
21. [Advancements & Achievements](#21-advancements--achievements)
22. [Commands](#22-commands)
23. [Multiplayer](#23-multiplayer)
24. [Game Customization](#24-game-customization)

---

## 1. Game Modes & Difficulty

### Game Modes

| Mode | Type | Description |
|------|------|-------------|
| **Survival** | 0 | Gather resources, craft, fight mobs, manage health/hunger. Death drops items + XP. |
| **Creative** | 1 | Infinite blocks, instant breaking, flight, invulnerability (except void in JE). |
| **Adventure** | 2 | Only break blocks with `can_destroy` tag, only place with `can_place_on` tag. |
| **Spectator** | 3 | Clip through blocks, fly, no interaction. JE: enter mob perspective (left-click). |
| **Hardcore** | — | Survival locked to Hard, one life (death → Spectator or delete). Not a true gamemode. |

### Difficulty Settings

| Level | Hostile Damage | Starvation | Zombie Doors | Villager Conversion | Raid Waves |
|-------|---------------|------------|-------------|-------------------|------------|
| **Peaceful** | None | None | — | — | — |
| **Easy** | 50%+0.5 (capped) | Stops at 5 HP | No | 0% | 3-4 |
| **Normal** | 100% | Stops at 1 HP | No | 50% | 5-6 |
| **Hard** | 150% | Can kill | Yes | 100% | 7-8 |

**Regional Difficulty** (0.00–6.75): Based on world difficulty × chunk inhabited time (cap 50h) × total daytime (cap 63 days) × moon phase. Clamped RD (0.0–1.0) affects mob equipment/enchant quality.

**Moon Phase Effects:** Full=100%, Gibbous=75%, Quarter=50%, Crescent=25%, New=0%.

---

## 2. Dimensions

### Overview

| Dimension | ID | Build Height | Chunk Size | Sea Level | Coordinate Scale |
|-----------|-----|-------------|------------|-----------|-----------------|
| **Overworld** | `overworld` | Y=-64 to 320 (384) | 16×384×16 | Y=63 | 1:1 |
| **Nether** | `the_nether` | Y=0 to 127 (128) | 16×128×16 | Lava at 31 | 8:1 (to OW) |
| **End** | `the_end` | Y=0 to 255 (256) | 16×256×16 | — | 1:1 |

### Dimension-Specific Rules

| Feature | Overworld | Nether | End |
|---------|----------|--------|-----|
| Daylight cycle | Yes | No | No |
| Weather | Rain/snow/thunder | None | None |
| Water | Yes | Evaporates | Yes |
| Bed | Sets spawn / skips night | Explodes (power 5) | Explodes (power 5+) |
| Respawn anchor | Explodes | Works (with charges) | Explodes |
| Compass | Points to spawn | Spins (lodestone works) | Spins |
| Clock | Works | Spins | Spins |
| Fire | Burns out | Burns forever on netherrack | Burns forever on bedrock |
| Sky | Blue, sun/moon/stars | Red/brown fog | Black/purple static |
| Lava flow distance | 4 blocks | 8 blocks | 4 blocks |
| Lava flow speed | 30 ticks/block (1.5s) | 10 ticks/block (0.5s) | 30 ticks/block |

**Nether Coordinate Conversion:** OW→Nether: `(X/8, Y, Z/8)`. Nether→OW: `(X×8, Y, Z×8)`.

**World Border:** JE: X/Z ±29,999,984 (border), ±29,999,999 (hard wall). BE: no fixed limit (~8M breakage).

---

## 3. World Generation & Biomes

### Seed System
- ~18 quintillion (2^64) possible seeds.
- Controls terrain, structures, features in all 3 dimensions.
- Same seed = same world across editions (structure placement may differ).

### Generation Steps (JE)
1. `empty` → 2. `structures_starts` → 3. `structures_references` → 4. `biomes` → 5. `noise` (base terrain + liquids) → 6. `surface` → 7. `carvers` → 8. `features` → 9. `initialize_light` → 10. `light` → 11. `spawn` → 12. `full`

### Noise Generation
- 3D Perlin noise, multiple octaves. Density >0 = solid, <0 = air.
- Splines use continentalness, erosion, and PV (peaks & valleys from weirdness).
- **PV = 1 − |(3×|weirdness|) − 2|** to categorize terrain: Valley, Low, Mid, High, Peak.

### Continentalness Ranges
| Range | Category |
|-------|----------|
| −1.2 to −1.05 | Mushroom fields |
| −1.05 to −0.455 | Deep ocean |
| −0.455 to −0.19 | Ocean |
| −0.19 to −0.11 | Coast |
| −0.11 to 0.03 | Near-inland |
| 0.03 to 0.3 | Mid-inland |
| 0.3 to 1.0 | Far-inland |

### Cave Types
| Type | Method | Description |
|------|--------|-------------|
| **Cheese caves** | 3D noise | Large pocket areas, noise pillars |
| **Spaghetti caves** | 3D noise | Long, narrow winding tunnels |
| **Noodle caves** | 3D noise | 1-5 blocks wide, very squiggly |
| **Carver caves** | Post-terrain | Main room + trunks + branches, can cut through surface |
| **Canyons** | Carver | Exposed to surface |

### Aquifers
- Below Y=-55: Always lava. Above Y=-10: Always water. Between: variable.
- Barriers separate different liquids or liquids from air.
- Deep dark areas (erosion < −0.22, depth > 0.9) always empty.

### Overworld Biomes (55 in JE)

**Offshore:** Ocean, Deep Ocean, Warm Ocean, Lukewarm Ocean, Deep Lukewarm Ocean, Cold Ocean, Deep Cold Ocean, Frozen Ocean, Deep Frozen Ocean, Mushroom Fields

**Highland:** Jagged Peaks, Frozen Peaks, Stony Peaks, Meadow, Cherry Grove, Grove, Snowy Slopes, Windswept Hills, Windswept Gravelly Hills, Windswept Forest, Dappled Forest* (upcoming)

**Woodland:** Forest, Flower Forest, Taiga, Old Growth Pine Taiga, Old Growth Spruce Taiga, Snowy Taiga, Birch Forest, Old Growth Birch Forest, Dark Forest, Pale Garden, Jungle, Sparse Jungle, Bamboo Jungle

**Wetland:** River, Frozen River, Swamp, Mangrove Swamp, Beach, Snowy Beach, Stony Shore

**Flatland:** Plains, Sunflower Plains, Snowy Plains, Ice Spikes

**Arid:** Desert, Savanna, Savanna Plateau, Windswept Savanna, Badlands, Wooded Badlands, Eroded Badlands

**Cave:** Deep Dark, Dripstone Caves, Lush Caves, Sulfur Caves

**Void:** The Void (JE only)

### Nether Biomes (5)

| Biome | Frequency | Mobs |
|-------|-----------|------|
| Nether Wastes | 36.30% | Ghast, zombified piglin, magma cube, piglin, strider, enderman |
| Crimson Forest | 22.22% | Hoglin, piglin, zombified piglin, strider |
| Soul Sand Valley | 17.08% | Ghast, skeleton, enderman, strider; dried ghasts |
| Basalt Deltas | 15.86% | Magma cube (highest rate), ghast |
| Warped Forest | 8.54% | Enderman only (peaceful) |

### The End Biomes (5 in JE): The End, Small End Islands, End Midlands, End Highlands, End Barrens

### World Types
**Default** · **Superflat** · **Single Biome** (JE) · **Amplified** (JE) · **Large Biomes** (JE) · **Debug Mode** (JE) · **Old** (BE) · **Void** (BE Editor)

### Generated Structures

**Overworld:** Village, Desert Pyramid, Jungle Pyramid, Igloo, Swamp Hut, Pillager Outpost, Woodland Mansion, Abandoned Camp* (upcoming), Ocean Monument, Ocean Ruins, Shipwreck, Mineshaft, Stronghold, Ancient City, Trail Ruins, Trial Chambers, Ruined Portal, Buried Treasure

**Nether:** Nether Fortress, Bastion Remnant, Nether Fossil, Ruined Portal

**End:** End City, End Ship, End Spikes (obsidian pillars), Exit Portal, End Gateway

---

## 4. Blocks

### Block Properties
- Arranged in 3D grid of 1-cubic-meter cells. Some occupy partial cells (slabs, stairs, snow layers, etc.).
- **Opaque:** Block light completely. **Transparent:** Varying light effects.
- **Gravity-affected:** Sand, red sand, gravel, anvils, dragon egg, concrete powder, scaffolding, pointed dripstone, suspicious sand/gravel. Fall as entities when support removed.
- **Hardness:** Determines mining time. **Blast resistance:** Determines explosion immunity.
- **Sound/Particles on break** emitted normally; exceptions for falling blocks, anvil destruction, washing away, replacement, support removal, leaf decay.

### Block Categories

**Natural:** Stone, deepslate, granite, diorite, andesite, tuff, calcite, dirt, grass block, podzol, mycelium, mud, sand, red sand, gravel, clay, netherrack, end stone, obsidian, basalt, blackstone.

**Ores (Overworld):** Coal (Y=0–320), Copper (Y=−16–112), Iron (Y=−64–72), Gold (Y=−64–−16, 80–320), Lapis Lazuli (Y=−64–64), Redstone (Y=−64–32), Diamond (Y=−64–16), Emerald (Y=−64–256, mountains only). All have deepslate variants below Y=0 except emerald.

**Nether Ores:** Nether Quartz (Y=0–128), Nether Gold (Y=0–117), Ancient Debris (Y=0–119, highest at Y=15).

**Mineral Blocks:** Block of Iron/Gold/Diamond/Emerald/Copper/Lapis/Redstone/Coal/Netherite/Raw Iron/Raw Gold/Raw Copper/Amethyst/Quartz/Resin/Bamboo.

**Wood Types (per type):** Oak, Spruce, Birch, Jungle, Acacia, Dark Oak, Mangrove, Cherry, Bamboo, Pale Oak, Poplar* (upcoming). Each has: Log, Wood, Stripped Log, Stripped Wood, Planks, Slab, Stairs, Fence, Fence Gate, Door, Trapdoor, Pressure Plate, Button, Sign, Hanging Sign, Boat, Boat with Chest, Shelf.

**Wool & Dyeables (16 colors):** Wool, Carpet, Concrete, Concrete Powder, Terracotta, Glazed Terracotta, Stained Glass, Stained Glass Pane, Shulker Box, Bed, Banner, Candle. Upcoming: Wool Slabs, Wool Stairs, Cushions.

**Functional/Crafting:** Crafting Table, Furnace, Blast Furnace, Smoker, Campfire, Soul Campfire, Chest, Trapped Chest, Ender Chest, Barrel, Copper Chest, Shulker Box, Anvil, Grindstone, Smithing Table, Enchanting Table, Bookshelf, Chiseled Bookshelf, Lectern, Brewing Stand, Cauldron, Composter, Beacon, Conduit, Bell, Stonecutter, Loom, Cartography Table, Fletching Table, Jukebox, Note Block, Daylight Detector, Hopper, Dispenser, Dropper, Observer, Piston, Sticky Piston, Crafter.

**Redstone:** Redstone Wire, Redstone Torch, Repeater, Comparator, Lever, Button (stone+wood), Pressure Plate (wood/stone/light/heavy), Target, Redstone Lamp, Copper Bulb, Sculk Sensor, Calibrated Sculk Sensor, Tripwire Hook, String.

**Light Sources:** Torch, Soul Torch, Lantern, Soul Lantern, Campfire, Soul Campfire, Glowstone, Sea Lantern, Shroomlight, Jack o'Lantern, Redstone Lamp, End Rod, Candle (16), Copper Bulb, Froglight (ochre/verdant/pearlescent), Amethyst Cluster.

**Sculk:** Sculk, Sculk Vein, Sculk Catalyst, Sculk Sensor, Calibrated Sculk Sensor, Sculk Shrieker.

**Spawn/Dev:** Spawner, Command Block (impulse/chain/repeating), Structure Block, Structure Void, Barrier, Light Block, Debug Stick, Jigsaw Block.

**Fluids:** Water, Lava, Powder Snow.

---

## 5. Items & Inventory

### Inventory Layout
- **4 armor slots** (head, chest, legs, feet)
- **27 storage slots**
- **9 hotbar slots** (selectable with 1-9 / mouse wheel)
- **1 off-hand slot** (JE: press F; BE: limited to shields/arrows/rockets/totems/maps/shells)
- **2×2 crafting grid** (not in JE Creative/Spectator)

### Stack Limits
- Most items: **64**
- Snowballs, empty buckets, eggs, signs, ender pearls, etc.: **16**
- Items with durability, filled buckets, potions: **1** (no stack)
- Commands: up to 99 (JE) / 127 (BE)

### Item Behavior
- Dropped items despawn after **5 minutes** in loaded chunks (Nether star never despawns).
- Destroyed by: fire, lava, cactus, explosion, void, `/kill`.
- **Netherite items:** Immune to fire/lava. **Nether stars:** Immune to explosions.
- Submerged items ascend; flowing water pushes them.
- Hopper draws items above if not powered.
- Thrown item physics: Drag-H=0.99, Gravity=−0.04 m/tick², Drag-Y=0.98, Speed=0.6 m/tick.

### External Inventories (GUI-accessible)
| Container | Slots |
|-----------|-------|
| Chest / Barrel / Shulker Box | 27 |
| Large Chest / Copper Chest | 54 |
| Ender Chest | 27 (per-player) |
| Minecart with Chest / Boat with Chest | 27 |
| Dispenser / Dropper | 9 |
| Hopper | 5 |
| Brewing Stand | 4 |
| Furnace / Blast Furnace / Smoker | 3 |
| Crafter | 3×3 |
| Chiseled Bookshelf | 6 book slots |
| Horse inventory | 2 equipment + var. |
| Donkey/Mule with chest | 15 |
| Llama with chest | 3/6/9/12/15 (depends on Strength) |
| Allay | 1 (not player-accessible) |

### Notable Items (non-block)

**Tools:** Pickaxe, Axe, Shovel, Hoe, Shears, Fishing Rod, Brush, Spyglass, Flint and Steel, Fire Charge, Bucket, Bone Meal.

**Weapons:** Sword, Axe (as weapon), Bow, Crossbow, Trident, Spear, Mace, Wind Charge.

**Armor/Special Wear:** Leather/Chainmail/Iron/Gold/Diamond/Netherite armor sets, Turtle Shell (helmet), Elytra (chestplate), Horse Armor, Wolf Armor, Carved Pumpkin, Mob Heads.

**Food:** ~40+ edible items (see Food section).

**Materials:** Coal, Charcoal, Iron/Gold/Copper/Netherite Ingots, Netherite Scrap, Diamond, Emerald, Lapis Lazuli, Nether Quartz, Raw Iron/Gold/Copper, Nuggets, Stick, String, Feather, Flint, Leather, Rabbit Hide, Bone, Ink Sac, Glow Ink Sac, Dyes (16), Slimeball, Honeycomb, Clay Ball, Brick, Nether Brick, Blaze Rod/Powder, Ghast Tear, Magma Cream, Spider Eye, Fermented Spider Eye, Glistering Melon, Golden Carrot, Ender Pearl, Eye of Ender, End Crystal, Prismarine Shard/Crystals, Nautilus Shell, Heart of the Sea, Phantom Membrane, Shulker Shell, Popped Chorus Fruit, Dragon's Breath, Gunpowder, Firework Star, Amethyst Shard, Echo Shard, Disc Fragment 5, Nether Star, Resin Brick.

**Pottery Sherds (23):** Angler, Archer, Arms Up, Blade, Brewer, Burn, Danger, Explorer, Flow, Friend, Guster, Heart, Heartbreak, Howl, Miner, Mourner, Plenty, Prize, Scrape, Sheaf, Shelter, Skull, Snort.

**Music Discs:** 13, Cat, Blocks, Chirp, Far, Mall, Mellohi, Stal, Strad, Ward, 11, Wait, Otherside, Relic, 5, Pigstep, Creator, Creator (Music Box), Precipice.

---

## 6. Mobs & Entities

### Mob Categories

**Passive (never attack):** Allay, Armadillo, Axolotl, Bat, Camel, Camel Husk, Cat, Chicken, Cod, Copper Golem, Cow, Donkey, Frog, Glow Squid, Happy Ghast, Horse, Mooshroom, Mule, Ocelot, Parrot, Pig, Rabbit, Salmon, Sheep, Skeleton Horse, Sniffer, Snow Golem, Squid, Strider, Sulfur Cube, Tadpole, Tropical Fish, Turtle, Villager, Wandering Trader, Zombie Horse.

**Neutral (provoked):** Bee, Cave Spider, Dolphin, Drowned, Enderman, Fox, Goat, Iron Golem, Llama, Nautilus, Panda, Piglin, Polar Bear, Pufferfish, Spider, Trader Llama, Wolf, Zombie Nautilus, Zombified Piglin.

**Hostile (always attack):** Blaze, Bogged, Breeze, Creaking, Creeper, Elder Guardian, Endermite, Evoker, Ghast, Guardian, Hoglin, Husk, Magma Cube, Parched, Phantom, Piglin Brute, Pillager, Ravager, Shulker, Silverfish, Skeleton, Slime, Stray, Vex, Vindicator, Warden, Witch, Wither Skeleton, Zoglin, Zombie, Zombie Villager.

**Bosses:** Ender Dragon (200 HP, 12,000 XP first / 500 respawned), Wither (300 HP JE / 300–600 BE, 50 XP).

### Spawning Rules
- Most passive: spawn on chunk generation. Hostile: spawn based on light level, biome, surroundings.
- Most mobs spawn within 16 blocks (Euclidean) of player and despawn beyond.
- JE: Most passive mobs never despawn; most monsters do. BE: Almost all mobs despawn.
- Prevention: Name tag (both editions), boat (JE only).
- Mobs do NOT spawn on transparent blocks, in water (except aquatic), in lava (except striders), on bedrock, or on blocks less than 1 full block tall.
- **Hostile mobs spawn at block light ≤0** (except slimes in swamps at ≤7, and passive mobs at ≥9).

### Key Mob Stats

**Bosses:** Ender Dragon 200 HP, Wither 300 HP (JE) / 300-600 HP (BE), Warden 500 HP, Iron Golem 100 HP, Ravager 100 HP, Elder Guardian 80 HP.

**Common Hostile:** Creeper/Zombie/Skeleton/Spider/Drowned/Husk/Stray/Zombie Villager/Zombified Piglin: 20 HP. Enderman: 40 HP. Hoglin/Zoglin: 40 HP. Blaze: 20 HP. Witch: 26 HP. Vindicator/Evoker/Pillager: 24 HP.

**Passive:** Player 20 HP, Villager 20 HP, Cow/Pig/Sheep/Chicken/Fox/Frog: 10 HP, Horse: 15-30 HP, Camel: 32 HP, Wolf (tamed): 40 HP, Wolf (wild): 8 HP, Cat: 10 HP.

### Breeding

| Animal | Breeding Item | Notes |
|--------|--------------|-------|
| Horse/Donkey | Golden Apple/Carrot | Tamed; horse+donkey=mule (infertile) |
| Cow/Mooshroom/Goat/Sheep | Wheat | — |
| Pig | Carrot/Potato/Beetroot | — |
| Chicken | Seeds (wheat/pumpkin/melon/beetroot/torchflower/pitcher pod) | Also lays eggs |
| Wolf (tamed) | Raw/cooked meat (any), Rotten Flesh | Must be full health |
| Cat/Ocelot | Raw Cod/Salmon | Tamed, full health |
| Rabbit | Dandelion/Carrot/Golden Carrot | — |
| Turtle | Seagrass | Lays eggs on home beach |
| Panda | Bamboo | Needs bamboo nearby |
| Fox | Sweet Berries/Glow Berries | Baby trusts breeder |
| Bee | Any flower | — |
| Strider | Warped Fungus | — |
| Hoglin | Crimson Fungus | — |
| Frog | Slimeball | Lays frogspawn (10 min to hatch) |
| Camel | Cactus | — |
| Sniffer | Torchflower Seeds | Lays eggs (20 min normal, 10 on moss) |
| Armadillo | Spider Eye | — |
| Nautilus | Any fish | Tamed with pufferfish first |
| Allay | Amethyst Shard (while dancing to jukebox) | Duplication, 5 min cooldown |
| Shulker | — | 25% when hit by bullet at <50% HP, teleport space needed |

### XP from Mobs
- Passive animals: 1–3 XP
- Most hostile (zombie, skeleton, creeper): 5 + 1-3 per equipment
- Blaze/Breeze/Guardian/Elder Guardian/Evoker: 10 XP
- Baby zombie types: 12 + 1-3 per equipment
- Ravager/Piglin Brute: 20 XP
- Wither: 50 XP
- Ender Dragon (first): 12,000 XP / (respawned): 500 XP
- Player (PvP): 7 per level (max 100)

---

## 7. Player Mechanics

### Health
- Default: **20 HP** (×10 hearts). Each half-heart = 1 HP.
- **Natural regen:** Requires hunger ≥18 (×9) and `naturalRegeneration` gamerule = true. Regenerates 1 HP every 4 seconds (80 ticks).
- **Saturation boost (JE only):** At full hunger (20), consumes 1.5 saturation to heal 1 HP every 0.5s.
- Hardcore mode: different heart texture.

### Hunger & Saturation
- **Max hunger:** 20 (10 drumsticks). Max saturation: hidden.
- **Exhaustion** accumulates from actions; at 4.0, reduces saturation by 1, then hunger by 1.
- **Cannot sprint** when hunger ≤6 (×3).
- **Starvation** at hunger=0: 1 HP/4s (Easy: stops at 10 HP, Normal: stops at 1 HP, Hard: can kill).

| Action | Exhaustion |
|--------|-----------|
| Breaking block | 0.005/block |
| Sprinting | 0.1/meter |
| Jumping | 0.05/jump |
| Sprint-jumping | 0.2/jump |
| Attacking entity | 0.1/attack |
| Taking armor-protected damage | 0.1/instance |
| Natural regen (per 1 HP) | 6.0 |

### Experience
- XP from: mob kills (within 5s), mining ores, sculk, smelting, breeding (1-7), fishing (1-6), trading (3-6), Bottle o' Enchanting (3-11).
- **Leveling:** Lv 0-15 = `2×level+7` XP each. Lv 16-30 = `5×level-38`. Lv 31+ = `9×level-158`. Total to reach lv 30: 1,395 XP.
- **Max enchanting level:** 30. **Anvil max work level:** 39.
- **Death penalty:** Drop orbs worth `7×current_level` XP (max 100), rest vanishes. KeepInventory gamerule keeps XP.
- **Orb pickup:** Range 7.25 blocks, 10 orbs/sec, despawn 5 min.

### Mining (Breaking)

**Base time = hardness × multiplier (1.5 for correct tool, 5 for wrong).**

| Tool | Speed |
|------|-------|
| Wooden | 2 |
| Stone | 4 |
| Copper | 5 |
| Iron | 6 |
| Diamond | 8 |
| Netherite | 9 |
| Gold | 12 |
| Shears (wool) | 5 |
| Shears (leaves/cobweb) | 15 |
| Sword (cobweb) | 15 |
| Sword (bamboo) | 30 |

**Efficiency Enchant:** Adds `(level²+1)`. Eff I=+2, Eff II=+5, Eff III=+10, Eff IV=+17, Eff V=+26.

**Haste (JE):** `speed × (0.2 × level + 1)`. **Mining Fatigue (JE):** `speed × 0.3^min(level,4)`.

**Penalties:** Head underwater without Aqua Affinity: 5×. Not on ground: 5×. Both: 25×.

**Instant break:** When tool damage ≥ hardness × 30.

**Breaking range:** JE: 4.5 blocks (survival), 5.2 (creative). BE: 5 (keyboard), 12 (creative touch), 6 (survival touch).

### Best Tool by Category
| Tool | Category | Tier Required |
|------|----------|---------------|
| Pickaxe | Stone/Rock | Wooden+ for stone, Stone+ for iron/copper/lapis, Iron+ for diamond/gold/redstone/emerald, Diamond+ for obsidian/ancient debris |
| Axe | Wood/plants | Any |
| Shovel | Ground/snow | Any |
| Shears | Leaves/cobweb/wool | Required for leaves |
| Sword | Cobweb (→string), Bamboo | Any |
| Hoe | Hay/sculk/etc. | Any |

---

## 8. Combat

### Weapon Damage (JE)

| Weapon | Attack Speed | Damage | DPS |
|--------|-------------|--------|-----|
| Netherite Sword | 1.6 | 8 | 12.8 |
| Diamond Sword | 1.6 | 7 | 11.2 |
| Iron Sword | 1.6 | 6 | 9.6 |
| Trident | 1.1 | 9 | 9.9 |
| Netherite Axe | 1.0 | 10 | 10 |
| Diamond Axe | 1.0 | 9 | 9 |
| Mace | 0.6 | 6 (+fall bonus) | 3.6+ |
| Spear (varies) | ~0.8 | 6-10 | — |
| Bow (full charge) | — | 1-11 | — |
| Crossbow | — | 7-11 (JE) / 9 (BE) | — |

### Special Attacks
- **Critical hit:** Player falling, cooldown ≥84.8% (JE). 150% damage.
- **Sprint-knockback:** Sprinting + attack = extra knockback.
- **Sweep attack (JE only):** Sword, grounded, ≥84.8% cooldown. 1 HP to surrounding entities.
- **Smash attack (Mace):** Bonus damage based on fall distance.
- **Jab/Charge (Spear):** Rapid short-range / sprinting speed-scaled damage.
- **Axe:** 100% chance to disable shield for 5 seconds vs players.
- **BE:** No attack cooldown; weapon damage displayed = +1 over true value; no sweep attack.

### Attack Cooldown (JE Only)
- Formula: `0.2 + ((t+0.5)/T)² × 0.8` (range 0.2–1.0), where `T = 20/attack_speed` ticks.
- At 84.8%: critical hits, sweep, sprint-knockback become possible.

### Shield
- Blocks 100% melee damage/knockback/debuffs. Durability loss only from >2 HP attacks.
- **5 ticks cooldown** after blocking.
- JE: Right-click to block. BE: Sneak to block.
- Axe attack: disables shield 5 seconds (100% chance).

### Damage Reduction

| Type | Formula / Rule |
|------|----------------|
| **Armor** | `damage × (1 − min(20, max(armor/5, armor − damage/(2+toughness/4)))/25)` |
| **Armor cap** | 80% at 20 armor points |
| **Toughness** | Diamond=8, Netherite=12 — reduces effectiveness of high-damage attacks vs armor |
| **Protection enchants** | 4% damage reduction per level (EPF system) |

### Natural Damage Sources
| Source | Damage | Notes |
|--------|--------|-------|
| Drowning | 2 HP/sec | After 15 sec breath |
| Fall | Variable | Feather Falling helps |
| Lava | 4 HP/tick | +15 sec fire after |
| Fire/burning | 1 HP/sec | |
| Cactus | 1 HP/touch | |
| Berry bush | 1 HP/0.5s | Moving through |
| Freezing | Continuous | Powder snow |
| Lightning | 5 HP | Direct hit |
| Starvation | 1 HP/4s | At hunger=0, ignores armor |
| Void | ~4 HP/0.5s | Below Y=-64 |
| Thorns | Variable | Reflected damage |
| Mace crush (anvil) | 2 HP/block fallen (max 40) | Entities under anvil |

---

## 9. Enchanting

### Enchanting Table
- Costs **lapis lazuli + experience levels**.
- Three options appear. Bookshelf arrangement (max 15) determines quality.
- Bookshelves must be **exactly 2 blocks away** (same level or +1), with air/replaceable block in between (JE) or **air only** (BE).
- Base level: `rand(1,8) + floor(bookshelves/2) + rand(0, bookshelves)`
- Slots: Top = `max(base/3, 1)`, Middle = `base×2/3 + 1`, Bottom = `max(base, bookshelves×2)`

### Enchantability (by material)
| Material | Value |
|----------|-------|
| Wood/Leather/Netherite/Mace | 15 |
| Gold | 22 (tools) / 25 (armor) |
| Iron | 14 (tools) / 9 (armor) |
| Diamond | 10 |
| Chainmail | 12 |
| Copper | 13 (tools) / 8 (armor) |
| Stone | 5 |
| Book/Bow/Crossbow/Fishing Rod/Trident | 1 |

### All Enchantments

| Enchantment | Max | Primary | Effect |
|-------------|-----|---------|--------|
| Aqua Affinity | I | Helmet | Removes underwater mining penalty |
| Bane of Arthropods | V | Sword, Spear, Axe(BE) | +DMG to arthropods + Slowness IV |
| Blast Protection | IV | Armor | Reduces explosion damage & knockback |
| Breach | IV | Mace | Reduces armor effectiveness |
| Channeling | I | Trident | Lightning during thunderstorms |
| Curse of Binding | I | Armor, Elytra, Carved Pumpkin, Heads | Cannot remove item |
| Curse of Vanishing | I | All items | Disappears on death |
| Density | V | Mace | +DMG per block fallen (smash) |
| Depth Strider | III | Boots | Reduces water movement penalty |
| Efficiency | V | Pickaxe, Shovel, Axe, Hoe | +Mining speed |
| Feather Falling | IV | Boots | Reduces fall damage |
| Fire Aspect | II | Sword, Spear, Mace | Sets target on fire |
| Fire Protection | IV | Armor | Reduces fire damage & burn time |
| Flame | I | Bow | Arrows set targets on fire |
| Fortune | III | Pickaxe, Shovel, Axe, Hoe | +Block drops |
| Frost Walker | II | Boots | Water → frosted ice; magma/campfire immunity |
| Impaling | V | Trident | JE: aquatic mobs; BE: in water/rain |
| Infinity | I | Bow | Arrows not consumed |
| Knockback | II | Sword, Spear | +Knockback |
| Looting | III | Sword, Spear | +Mob drops |
| Loyalty | III | Trident | Returns on throw |
| Luck of the Sea | III | Fishing Rod | +Treasure rate |
| Lunge | III | Spear | Horizontal launch on jab; costs saturation/durability |
| Lure | III | Fishing Rod | Decreases bite time |
| Mending | I | All tools/weapons/armor | Repairs with XP orbs |
| Multishot | I | Crossbow | Shoots 3 arrows |
| Piercing | IV | Crossbow | Arrows pierce entities |
| Power | V | Bow | +Arrow damage |
| Projectile Protection | IV | Armor | Reduces projectile damage |
| Protection | IV | Armor | Reduces most damage |
| Punch | II | Bow | +Arrow knockback |
| Quick Charge | III | Crossbow | Faster reload |
| Respiration | III | Helmet | Extends underwater breath (+15s/level) |
| Riptide | III | Trident | Launches wielder in water/rain |
| Sharpness | V | Sword, Spear, Axe(BE) | +Melee damage |
| Silk Touch | I | Pickaxe, Shovel, Axe, Hoe | Blocks drop themselves |
| Smite | V | Sword, Spear, Axe(BE), Mace | +DMG to undead |
| Soul Speed | III | Boots | +Speed on soul sand/soil |
| Sweeping Edge (JE) | III | Sword | +Sweep attack damage |
| Swift Sneak | III | Leggings | +Sneaking speed |
| Thorns | III | Chestplate(JE)/All armor(BE) | Damages attacker |
| Unbreaking | III | All tools/weapons/armor | Chance to ignore durability loss |
| Wind Burst | III | Mace | Wind burst on smash; launches attacker upward |

### Incompatible Enchantment Groups
| Group | Conflicting |
|-------|-------------|
| Sword damage | Sharpness ↔ Smite ↔ Bane of Arthropods |
| Mace damage | Density ↔ Breach ↔ Smite ↔ Bane of Arthropods |
| Tool | Fortune ↔ Silk Touch |
| Armor protection | Protection ↔ Fire/Blast/Projectile Protection |
| Boots | Depth Strider ↔ Frost Walker |
| Bow | Infinity ↔ Mending |
| Crossbow | Multishot ↔ Piercing |
| Trident | Loyalty ↔ Riptide, Channeling ↔ Riptide |

### Anvil Mechanics
- **5 functions:** Rename (1 lvl), Repair (1 lvl/unit), Combine items, Combine with enchanted book, Crush entities.
- **12% chance** per operation to degrade (Anvil → Chipped → Damaged → destroyed).
- **Max cost:** 39 levels (beyond: "Too Expensive!").
- **Prior work penalty:** Each anvil use doubles it: `2^c − 1` (c=use count). 0→1→3→7→15→31.
- **Total cost:** `prior_work(target) + prior_work(sacrifice) + rename(1) + repair(2) + enchantment_cost`.

---

## 10. Brewing & Potions

### Equipment
- **Brewing Stand:** Fueled by blaze powder (20 charges). Each operation: 20 seconds (400 ticks).
- **Base:** Glass Bottle + Water → Water Bottle.

### Base Potions
| Potion | Ingredient |
|--------|-----------|
| Awkward Potion | Nether Wart + Water Bottle (precursor to all effect potions) |
| Mundane Potion | Redstone/other + Water Bottle |
| Thick Potion | Glowstone Dust + Water Bottle |

### Modifiers
| Ingredient | Effect |
|------------|--------|
| Nether Wart | Creates Awkward Potion (base for effects) |
| Redstone Dust | Extends duration |
| Glowstone Dust | Increases potency (level II) |
| Gunpowder | Makes Splash Potion |
| Dragon's Breath | Makes Lingering Potion |
| Fermented Spider Eye | Corrupts/reverses effect (e.g., Night Vision → Invisibility) |

### Effect Potions

| Potion | Base Ingredient | Duration | Effect |
|--------|----------------|----------|--------|
| **Fire Resistance** | Magma Cream | 3:00 (ext 8:00) | Fire/lava immunity |
| **Healing** | Glistering Melon | Instant | Restores 4 HP (II: 8 HP) |
| **Harming** | Fermented Spider Eye + Healing | Instant | Inflicts 6 HP (II: 12 HP) |
| **Invisibility** | Fermented Spider Eye + Night Vision | 3:00 (ext 8:00) | Invisible (items still visible) |
| **Leaping** | Rabbit's Foot | 3:00 (ext 8:00, II 1:30) | +50% jump (II: +150%) |
| **Night Vision** | Golden Carrot | 3:00 (ext 8:00) | Full brightness underwater |
| **Poison** | Spider Eye | 0:45 (ext 1:30, II 0:21) | 0.5 HP/1.25s (II: 0.6s); can't kill |
| **Regeneration** | Ghast Tear | 0:45 (ext 1:30, II 0:22) | 0.5 HP/2.5s (II: 1.25s) |
| **Slow Falling** | Phantom Membrane | 1:30 (ext 4:00) | Reduced fall speed, no fall damage |
| **Slowness** | Fermented Spider Eye + Swiftness | 1:30 (ext 4:00, IV 0:20) | −15% speed (IV: −60%) |
| **Strength** | Blaze Powder | 3:00 (ext 8:00, II 1:30) | +3 DMG (II: +6) |
| **Swiftness** | Sugar | 3:00 (ext 8:00, II 1:30) | +20% speed (II: +40%) |
| **Water Breathing** | Pufferfish | 3:00 (ext 8:00) | No drowning |
| **Weakness** | Fermented Spider Eye + Water Bottle | 1:30 (ext 4:00) | −4 DMG |
| **Wind Charged** | Breeze Rod | 3:00 | Wind burst on death |
| **Weaving** | Cobweb | 3:00 | Spreads cobwebs on death |
| **Oozing** | Slime Block | 3:00 | Spawns slimes on death |
| **Infested** | Stone | 3:00 | Spawns silverfish on damage |

### Splash & Lingering
- **Splash Potions:** Same duration as drinkable, thrown to coat area.
- **Lingering Potions:** ¼ drinkable duration, creates area-effect cloud.
- Undead mobs: take damage from Healing, heal from Harming, immune to Poison/Regeneration.

---

## 11. Status Effects

### Positive Effects
| Effect | Source | Effect Description |
|--------|--------|-------------------|
| Absorption | Golden Apple | Temporary yellow hearts |
| Breath of the Nautilus | Nautilus | Freezes oxygen bar |
| Conduit Power | Conduit | Underwater vision + mining speed, no drowning |
| Dolphin's Grace (JE) | Dolphin | +Swim speed |
| Fire Resistance | Potion/Enchanted Gapple | Fire/lava immunity |
| Haste | Beacon | +Mining + attack speed |
| Health Boost | Beacon | +Max HP |
| Hero of the Village | Win raid | Trade discount, gifts |
| Instant Health | Potion | Restores HP (damages undead) |
| Invisibility | Potion | Invisible (items/armor still visible) |
| Jump Boost | Potion/Beacon | +Jump height, −fall damage |
| Luck (JE) | Potion | +Better fishing loot |
| Night Vision | Potion | Full brightness |
| Regeneration | Potion/Golden Apple/Beacon | Restores HP over time |
| Resistance | Potion/Enchanted Gapple | Reduces damage |
| Saturation | Suspicious Stew | Restores hunger + saturation |
| Slow Falling | Potion | Reduced fall speed, no fall damage |
| Speed | Potion/Beacon | +Movement speed |
| Strength | Potion/Beacon | +Melee damage |
| Water Breathing | Potion | Prevents drowning |

### Negative Effects
| Effect | Source | Effect Description |
|--------|--------|-------------------|
| Bad Luck (JE) | — | Worse fishing loot |
| Bad Omen | Raid captain kill | Triggers raid on village |
| Blindness | Suspicious Stew | Black fog, can't sprint/crit |
| Darkness | Warden/Sculk Shrieker | Pulsating dark fog |
| Fatal Poison (BE) | — | Damage over time; can kill |
| Hunger | Rotten Flesh, Raw Chicken | Green hunger bar, depletes faster |
| Infested | Potion | 10% chance spawn silverfish when hurt |
| Instant Damage | Potion | Damages (heals undead) |
| Levitation | Shulker | Floats upward |
| Mining Fatigue | Elder Guardian | −Mining + attack speed |
| Nausea | Pufferfish, Suspicious Stew | Screen wobbles |
| Oozing | Potion | Spawns slimes on death |
| Poison | Potion, Spider Eye, etc. | Damage over time (stops at 1 HP) |
| Raid Omen | Kill raid captain in village | Triggers raid |
| Slowness | Potion | −Movement speed |
| Trial Omen | Ominous trial key | Upgrades trial spawners |
| Weakness | Potion | −Melee damage |
| Weaving | Potion | −Cobweb slowdown, spreads cobwebs on death |
| Wind Charged | Potion | Wind burst on death |
| Wither | Wither | Damage over time (can kill) |

### Effect Removal
- **Death:** Removes all effects.
- **Milk bucket:** Removes ALL effects.
- **Honey bottle:** Clears Poison specifically.
- **Totem of Undying:** Removes all effects on save.

### Effect Immunity
- Ender Dragon/Wither/NPC/Agent: immune to ALL effects.
- All undead: immune to Poison, Regeneration.
- Wither Skeletons: also immune to Wither.
- Spiders/Cave Spiders: immune to Poison.
- Witches: 85% (JE) / 95% (BE) magic damage resistance.

---

## 12. Crafting & Smelting

### Crafting
- **2×2 grid:** In inventory (player inventory screen).
- **3×3 grid:** Crafting Table (any recipe).
- **Crafter:** Automated crafting; requires redstone signal; hoppers can insert; disabled slots block input.
- **Recipe types:** Shaped (position matters), Shapeless (anywhere), Fixed (data pack banners).

### Smelting (Furnace)
- **3 slots:** Input (top-left), Fuel (bottom-left), Output (right).
- **Furnace time:** 10 seconds (200 ticks) per item.
- **Blast Furnace / Smoker:** 5 seconds (100 ticks) per item. Blast furnace: ores only. Smoker: food only.
- **Fuel consumed** immediately; if exhausted mid-smelt, progress reverses at double speed.
- **XP awarded** when player manually removes output. Fractional XP: fractional part = chance of +1.

### Fuel Sources
| Item | Duration | Items Smelted |
|------|----------|--------------|
| Lava Bucket | 1000s | 100 |
| Block of Coal | 800s | 80 |
| Blaze Rod | 120s | 12 |
| Coal/Charcoal | 80s | 8 |
| Dried Kelp Block | 200s | 20 |
| Log/Wood/Planks | 15s | 1.5 |
| Stick | 5s | 0.5 |
| Bamboo | 2.5s | 0.25 |

### Key Smelting Recipes
| Output | Input | XP |
|--------|-------|-----|
| Iron Ingot | Raw Iron / Iron Ore | 0.7 |
| Copper Ingot | Raw Copper / Copper Ore | 0.7 |
| Gold Ingot | Raw Gold / Gold Ore / Nether Gold Ore | 1.0 |
| Netherite Scrap | Ancient Debris | 2.0 |
| Steak | Raw Beef | 0.35 |
| Cooked Porkchop | Raw Porkchop | 0.35 |
| Baked Potato | Potato | 0.35 |
| Stone | Cobblestone | 0.1 |
| Smooth Stone | Stone | 0.1 |
| Glass | Sand | 0.1 |
| Brick | Clay Ball | 0.3 |
| Nether Brick | Netherrack | 0.1 |
| Terracotta | Clay | 0.35 |

### Ore Drops (without Silk Touch)
| Ore | Drops | Quantity | Fortune III Max |
|-----|-------|----------|----------------|
| Coal | Coal | 1 | 4 |
| Iron/Gold | Raw metal | 1 | 4 |
| Copper | Raw Copper | 2-5 | 12 |
| Nether Gold | Gold Nuggets | 2-6 | 24 |
| Redstone | Redstone Dust | 4-5 | 12 |
| Lapis Lazuli | Lapis Lazuli | 4-9 | 32 |
| Diamond | Diamond | 1 | 4 |
| Emerald | Emerald | 1 | 4 |
| Nether Quartz | Nether Quartz | 1 | 4 |
| Ancient Debris | Ancient Debris | 1 | 1 (Fortune doesn't work) |

---

## 13. Trading & Bartering

### Villager Trading
- **5 career levels:** Novice (0 XP), Apprentice (10), Journeyman (70), Expert (150), Master (250).
- Each level unlocks new trades. Max ~10 trades per villager.
- **Restocking:** Up to 2×/day when working at job site block.
- **Price formula factors:** Demand, reputation, Hero of the Village (−30% + 6.25%/level).
- **Curing zombie villagers:** Permanent positive reputation.

### Professions & Job Sites

| Profession | Job Block | Key Trades |
|------------|-----------|------------|
| Armorer | Blast Furnace | Armor, Chainmail, Shield |
| Butcher | Smoker | Cooked meats, Dried Kelp Block |
| Cartographer | Cartography Table | Maps, Explorer Maps (Ocean/Woodland/Trial), Globe Banner |
| Cleric | Brewing Stand | Redstone, Lapis, Ender Pearls, Bottles o' Enchanting |
| Farmer | Composter | Bread, Pumpkin Pie, Cake, Golden Carrot, Suspicious Stew |
| Fisherman | Barrel | Bucket of Cod, Campfire, Enchanted Fishing Rod |
| Fletcher | Fletching Table | Bows, Crossbows, Arrows, Tipped Arrows |
| Leatherworker | Cauldron | Leather armor, Rabbit Hide, Scute |
| Librarian | Lectern | Enchanted Books (all except soulspeed/swiftsneak/windburst), Name Tag, Compass |
| Mason | Stonecutter | Bricks, Quartz/Popular stone blocks, Dripstone |
| Shepherd | Loom | Shears, Colored Wool, Beds, Banners, Paintings |
| Toolsmith | Smithing Table | Enchanted iron/diamond tools, Bell |
| Weaponsmith | Grindstone | Enchanted iron/diamond weapons, Bell |

### Wandering Trader
- Trades 5-8 random items for emeralds: Coral, Dyes, Flowers, Logs, Sand, Packed Ice, Slime Balls, Nautilus Shells, Vines, etc.

### Piglins Bartering
- Drop a random item when given a **gold ingot** (in Nether, while wearing at least one golden armor piece).
- Possible barter items: Obsidian, Fire Charge, Ender Pearl, String, Water Bottle, Soul Sand, Gravel, Nether Bricks, Leather, Potion of Fire Resistance (extended), Iron Nuggets, Quartz, Iron Boots (Soul Speed I-III), Book, Name Tag, Spectral Arrow, Gold Nuggets.

---

## 14. Redstone

### Basic Concepts
- **Power:** ON/OFF. **Signal strength:** 0-15.
- **Redstone tick:** 0.1 seconds (2 game ticks).
- **Block updates:** How redstone changes propagate.

### Power Sources
| Component | Signal | Notes |
|-----------|--------|-------|
| Block of Redstone | 15 | Constant |
| Redstone Torch | 15 | Inverts; burns out if flickered >60 times |
| Lever | 15 | Toggle |
| Button (Wood) | 15 | 1.5s pulse |
| Button (Stone) | 15 | 1s pulse |
| Pressure Plate (Wood) | 15 | All entities |
| Pressure Plate (Stone) | 15 | Players/mobs only |
| Weighted Plate (Light) | 1-15 | Signal = entity count |
| Weighted Plate (Heavy) | 1-15 | Signal = entity count ÷ 10 |
| Daylight Detector | 1-15 | Sunlight intensity (inverted mode for night) |
| Observer | 15 | 1-tick pulse on block state change |
| Trapped Chest | 1-15 | Players with inventory open |
| Target | 1-15 | Signal based on projectile distance from center |
| Lighting Rod | 15 | Single pulse on lightning strike |
| Sculk Sensor | 1-15 | Based on vibration distance (frequency = vibration type) |
| Calibrated Sculk Sensor | 1-15 | Filters specific vibration frequencies |

### Transmission & Components
| Component | Max Distance | Notes |
|-----------|-------------|-------|
| Redstone Wire | 15 blocks | Signal degrades; repeater needed to extend |
| Repeater | ∞ | 1-4 tick delay; locks signal |
| Comparator | ∞ | Compare/subtract modes; reads container states |
| Redstone Torch | — | Acts as NOT gate; vertical transmission |

### Mechanisms
- **Piston:** Pushes up to 12 blocks. **Sticky Piston:** Pushes and pulls.
- **Dispenser:** Shoots items/projectiles. **Dropper:** Ejects items.
- **Hopper:** Transfers items (5 items/20 ticks).
- **Note Block:** Sound depends on block below.
- **Copper Bulb:** Toggleable light; waxable to lock state.
- **Crafter:** Auto-crafts when given redstone pulse.

### Logic Gates
| Gate | Condition |
|------|-----------|
| NOT | Input OFF = Output ON |
| OR | Any ON |
| NOR | All OFF |
| AND | All ON |
| NAND | Any OFF |
| XOR | Inputs differ |
| XNOR | Inputs equal |

### Circuit Types
- **Transmission:** Repeaters, torch towers, redstone ladders, observer towers, bubble columns.
- **Pulse:** Generators, limiters, extenders, multipliers, edge detectors.
- **Clock:** Repeater loop, observer clock, hopper clock, piston clock, comparator clock.
- **Memory:** RS NOR latch, T flip-flop, gated D latch, counter.
- **Piston circuits:** Slime/honey blocks for flying machines.

---

## 15. Transportation

### Movement Speeds

| Method | Speed (m/s) | Notes |
|--------|------------|-------|
| Walking | 4.317 | — |
| Sprinting | 5.612 | — |
| Sprint-jumping | 7.127 | — |
| Sneaking | 1.3 | — |
| Swimming (surface) | 2.20 | — |
| Sprint-swimming (underwater) | 3.918 | — |
| Creative fly | 11.0 (sprint: 22.0) | — |
| Elytra (glide 0°) | 30.0 | — |
| Elytra (52° dive) | 67.3 | — |
| Elytra (firework boost) | 33.5+ | — |
| Riptide III (water/rain) | ~375 | Fastest |

### Mount Speeds

| Mount | Speed (m/s) | Max HP |
|-------|------------|--------|
| Horse (average) | 9.49 | 15-30 |
| Horse (fastest) | 14.57 | 15-30 |
| Donkey / Mule | 7.38 | 15-30 |
| Camel (sprint) | 8.0 | 32 |
| Skeleton Horse | 8.43 | 15 |
| Strider (lava, boosted) | 7.34 | 20 |
| Pig (boosted) | 4.19 | 10 |
| Happy Ghast | Variable | 20 |

### Vehicle Speeds

| Vehicle | Conditions | Speed (m/s) |
|---------|-----------|------------|
| Minecart (powered rail) | Flat | 8.0 |
| Boat | Flat water | 8.0 |
| Boat | Packed ice (straight) | 40.0 |
| Boat | Blue ice (straight) | 72.73 |

### Vertical Transportation
| Method | Speed (m/s) |
|--------|------------|
| Jumping | 2.0 |
| Stairs (up) | 3.2 |
| Ladder/Vine (up) | 2.35 |
| Soul sand bubble column (up) | 14.0 |
| Magma bubble column (down) | 6.0 |
| Falling (terminal) | 78.4 |
| Creative fly (up/down) | 7.49 |

### Rail Types
- **Powered Rail:** Accelerates minecarts when powered; brakes when unpowered.
- **Detector Rail:** Emits redstone when minecart is on it.
- **Activator Rail:** Activates minecart functions (TNT, hopper, command block, furnace).

---

## 16. Environment & Physics

### Water
- **Flow:** 7 blocks horizontal, 5 ticks/block (4 m/s). Flows infinitely down.
- **Source creation:** 2+ adjacent sources + solid block below, or 1 horizontal + 1 vertical above.
- **Breath:** 15 seconds. Respiration: +15s/level.
- **Swimming:** Sprint key enables horizontal mode when fully submerged. Fall damage nullified.
- **Submerged mining:** 5× slower (25× if off ground). Aqua Affinity negates 5× penalty.
- **Water + Lava:** Source+lava=Obsidian. Flowing+lava=Cobblestone. Lava falling onto water=Stone.
- **Bubble columns:** Soul sand ↑ (14 m/s), Magma block ↓ (6 m/s).
- **Sponges:** Absorb water within taxicab distance 7 (max 65 blocks).
- **Irrigation:** Hydrates farmland within 4 blocks horizontally.

### Lava
- **Flow (OW/End):** 4 blocks, 30 ticks/block (1.5s). **Flow (Nether):** 8 blocks, 10 ticks/block (0.5s).
- **Light:** 15 (brightest). **Damage:** 4 HP/tick + 15s of burning.
- **Movement:** Horizontal speed −50%, vertical −20%. Cannot sprint-swim.
- **Fire spread from lava (JE):** Air 3×1×3 above or 5×1×5 two blocks above, if adjacent to flammable.
- **Netherite items:** Not destroyed. All other items destroyed instantly.
- **Dripstone + lava above:** 15/256 (~5.9%) chance per random tick to fill cauldron with lava in OW; automatic in Nether.

### Fire
- **Damage:** 1 HP/tick (soul fire: 2 HP/0.5s).
- **After leaving fire:** Mobs burn 160 ticks (8s). Players: fire value starts at −20 ticks.
- **Burning is NOT a status effect** (milk cannot cure).
- **Eternal fire:** Netherrack, Magma Block, Soul Sand, Soul Soil. Bedrock in End.
- **Spread within:** 1 down, 1 sideways (incl. diagonal), 4 up. Only in player-loaded chunks (8 chunks).
- **Reduced spread in humid biomes** (jungle, swamp) — 50% reduction.

### Powder Snow
- Walking entities sink (unless wearing leather boots).
- Freezing damage after 7 seconds within.
- **Freezing:** Cyan hearts; damage increases over time.
- Leather boots: no sinking, no freezing.

### Ice
- **Frosted Ice:** Created by Frost Walker enchantment; melts quickly.
- **Packed Ice:** Does not melt. **Blue Ice:** Slipperiest, fastest boat speed.

---

## 17. Lighting

### Light Levels
- **0 (minimum) to 15 (maximum).**
- **Client Light = max(sky light, block light).**
- **Block light** from emitting blocks. Decreases by 1 per taxicab distance.
- **Sky light** = 15 for exposed blocks. NOT reduced at night (internal light handles darkness).

### Internal Light
- Formula: `max(internal_sky_light, block_light)`.
- Used for: mob spawning, plant growth, daylight detector.
- Nether/End: always 0.
- Overworld: Noon clear=15, noon rain=12, noon thunder=10, midnight=4.

### Mob Spawning Light
- Most hostile: block light **0**.
- Slime (swamp): block light **≤7**.
- Passive mobs: block light **≥9** (JE) / **≥7** (BE).

### Plant Growth Light
- Crops: uproot at light 0-7 (JE), don't grow at 0-7 (BE).
- Saplings: grow at light ≥9.
- Mushrooms: spread at 4-11, uproot at 12+.
- Grass/Mycelium: spread at light ≥9.

### Light-Filtering Blocks
JE: Water, ice, leaves, cobweb, slime/honey blocks, chorus plant/flower, shulker boxes — reduce sky light by 1.
BE: Beacon (−14), Anvil/Hopper/Etc. (−3), Leaves (−2), Water/Cobweb/Powder Snow/Slabs (−1).

---

## 18. Weather & Daylight Cycle

### Daylight Cycle
- **Full cycle:** 20 minutes real time (24,000 game ticks).
- **Day:** 10 min (ticks 0-12000). **Sunset:** 1.5 min (12000-13000). **Night:** 7 min (13000-23000). **Sunrise:** 1.5 min (23000-24000).
- **Noon:** tick 6000. **Midnight:** tick 18000.
- **/time set:** `day`=1000, `noon`=6000, `night`=13000, `midnight`=18000.

### Weather Types
| Type | Conditions | Effects |
|------|-----------|---------|
| **Clear** | — | Normal |
| **Rain** | Temperature >0.15, not dry | Sky light=12, extinguishes fire, limits fishing |
| **Snow** | Temperature ≤0.15 or high altitude | Snow layers accumulate, cauldrons fill with powder snow |
| **Thunderstorm** | Any | Sky light=10, mobs spawn in daytime, lightning (Charged creepers, Witch, Zombified Piglin, Brown Mooshroom) |

### Weather Duration (JE)
- Rain: 12,000-24,000 ticks (10-20 min). Delay between: 12,000-180,000 ticks.
- Thunder: 3,600-15,600 ticks (3-13 min). Delay between: 12,000-180,000 ticks.

### Lightning
- Deals 5 HP damage (normal). Creates fire (usually extinguished by rain).
- **Transformations:** Creeper→Charged Creeper, Villager→Witch, Pig→Zombified Piglin, Mooshroom→Brown.
- Channeling trident summons lightning during storms.
- Skeleton trap horses from lightning (regional difficulty).

---

## 19. Explosions

### Causes & Power
| Cause | Power | Max Range | Fire |
|-------|-------|-----------|------|
| Ghast fireball | 1 | 2 / 1.5 | Yes |
| Wither skull | 1 | 2 / 1.5 | No |
| Creeper | 3 | 6 / 5.1 | No |
| TNT / TNT minecart | 4 | 8 / 6.9 | No |
| Bed/Respawn Anchor (wrong dim) | 5 | 10 / 8.4 | Yes |
| Charged Creeper | 6 | 12 / 10.2 | No |
| End Crystal (destroyed) | 6 | 12 / 10.2 | No |
| Wither (spawn/kill) | 7 | 14 / 12 | No |

### Entity Damage Formula
```
impact = (1 - distance/(2×power)) × exposure
damage = 7 × power × (impact² + impact) + 1
```
Peaceful: 0. Easy: min(damage/2+1, damage). Normal: damage. Hard: damage×1.5.

### Block Destruction
1. 1352 rays from center (16×16×16 grid).
2. Each ray: intensity = power × random(0.7, 1.3).
3. Steps of 0.3 blocks; reduce by (blast_resistance + 0.3) × 0.3 per step.
4. Max radius ~ 4/3 × power.
5. 1/3 destroyed blocks get fire (for fire-causing explosions).

### Key Blast Resistance Values
| Resistance | Blocks |
|------------|--------|
| 3,600,000 | Bedrock, Barrier, Command Blocks, End Portal/Frame |
| 1,200 | Obsidian, Ancient Debris, Netherite Block, Anvil, Enchanting Table |
| 100 | Water, Lava |
| 9 | Dragon Egg, End Stone |
| 6 | Stone, Deepslate, Bricks, Planks, Ores |
| 0.8 | Wool, Sandstone |
| 0 | Air, Fire, Flowers, Grass |

---

## 20. Archaeology

### Core Mechanic
- Use **Brush** (feather + copper ingot + stick) on **Suspicious Sand** or **Suspicious Gravel**.
- Slowly brush away layers to reveal items. If block falls or breaks, loot is lost.

### Locations
- **Suspicious Sand:** Desert pyramids, desert wells, warm ocean ruins.
- **Suspicious Gravel:** Cold ocean ruins, trail ruins (taigas, jungles, birch forests).

### Loot (varies by location)
- **Desert temple:** Archer/Miner/Prize/Skull sherds, Emerald, Gunpowder, TNT, Diamond.
- **Desert well:** Arms Up/Brewer sherds, Brick, Emerald, Stick, Suspicious Stew.
- **Trail ruins:** Dyes, Candles, wheat, brick, clay, emerald, music disc "Relic", armor trims.
- **Warm ocean ruins:** Sniffer egg, pottery sherds, iron axe, emerald, gold nugget.

### Decorated Pot
- Crafted from 4 pottery sherds or bricks (rhombus shape).
- Side patterns determined by sherd type.
- Has 1 storage slot (1 stack). Breaks into components unless Silk Touch.

### Sniffer Egg
- Only from warm ocean ruins. Hatches into Snifflet (baby sniffer).
- Breed adult sniffers with torchflower seeds.

---

## 21. Advancements & Achievements

### Java Edition Advancements
**126 advancements** across 5 tabs. Types: Normal (green), Goal (green oval), Challenge (purple, XP reward).

**Minecraft Tab (16):** The core progression: Stone Age → Getting an Upgrade → Acquire Hardware → Suit Up → Diamonds! → Enchanter → We Need to Go Deeper → The End?

**Nether Tab (23):** Enter Nether → collect all 5 biomes (Hot Tourist Destinations, 500XP) → full netherite armor (Cover Me in Debris, 100XP) → full power beacon (Beaconator) → all 27+ effects simultaneously (How Did We Get Here?, 1000XP).

**The End Tab (9):** Kill dragon → elytra → levitate 50 blocks (Great View From Here, 50XP).

**Adventure Tab (47):** Kill raid captain → win raid → fall from build limit to bottom (Caves & Cliffs) → fly elytra 1km (Who Needs Rockets?, 100XP) → apply all 40+ armor trims (Smithing with Style).

**Husbandry Tab (31):** Breed all animals (Two by Two) → eat everything edible (A Balanced Diet) → fully use a netherite hoe (Serious Dedication, 1000XP) → tame all cats → tame a wolf → plant seeds.

### Bedrock Edition Achievements
Similar progression with some differences. Completing achievements rewards cosmetics in Character Creator.

---

## 22. Commands

### Usage
- Prefix `/` in chat. Tab completion available. Up/Down arrows for history.
- **Command blocks:** Optional `/` prefix. **Functions (data packs):** No prefix.
- **Cheats:** JE: "Allow Commands" on creation or Open to LAN. BE: toggle in settings (permanently disables achievements).

### Key Commands (JE)

*OP Level 0:* `/help`, `/list`, `/msg`/`/tell`, `/seed`, `/trigger`

*OP Level 2:* `/advancement`, `/attribute`, `/clear`, `/clone`, `/data`, `/difficulty`, `/effect`, `/enchant`, `/execute`, `/fill`, `/gamemode`, `/gamerule`, `/give`, `/kill`, `/locate`, `/particle`, `/playsound`, `/reload`, `/schedule`, `/scoreboard`, `/setblock`, `/setworldspawn`, `/spawnpoint`, `/summon`, `/teleport`, `/time`, `/title`, `/weather`, `/worldborder`, `/xp`, `/function`

*OP Level 3:* `/tick`, `/debug`, `/ban`, `/op`, `/stop`, `/whitelist`, `/save-all`

### Target Selectors
- `@p` — nearest player, `@a` — all players, `@r` — random player, `@e` — all entities, `@s` — self

### BE Exclusive Commands
`/changesetting`, `/setmaxplayers`, `/structure`, `/fog`, `/music`, `/dialogue`

---

## 23. Multiplayer

### Connection Types
- **LAN:** Local network (JE: Open to LAN). BE: Visible to LAN.
- **Servers:** Dedicated server software; external connections (default port: 25565).
- **Realms:** Mojang-hosted subscription (10/40 player max, always online).
- **Friends (BE):** Xbox network invites.
- **Parties (BE):** Up to 15 players, auto-join leader's world.

### Differences from Singleplayer
- Game cannot be paused. JE: `/tick freeze` for operators.
- Player chat with `/msg` for private messages.
- Player reporting system (JE 1.19.1+).
- Ping indicators (Tab): <150ms green, 150-300 yellow, 300-600 orange, 600-1000 red.

### Permissions (BE)
**Visitor:** No interaction. **Member:** Default player. **Operator:** All commands.

---

## 24. Game Customization

### Resource Packs
- Change textures, models, sounds, text, fonts, GUI, lighting (MERS for PBR).
- Can change resolution (power-of-two sizes work best).
- Custom models via block/item models.

### Data Packs (JE)
- Add/modify advancements, recipes, loot tables, predicates, functions, dimension types, world presets.
- Tag system for block/item/entity grouping.
- Predicates (conditions): entity properties, location, weather, time, statistics.

### Add-Ons (BE)
- Behavior packs + resource packs.
- Modify entity behavior, spawn rules, animations.
- JSON-based scripting.

### Mods (JE)
- Full game modification via Fabric, Forge, NeoForge, Quilt.
- Custom blocks, items, entities, mechanics, GUIs.

### Game Rules
- `/gamerule`: `doDaylightCycle`, `doWeatherCycle`, `keepInventory`, `naturalRegeneration`, `mobGriefing`, `fireSpread`, `doMobSpawning`, `doEntityDrops`, `doTileDrops`, `doMobLoot`, `doFireTick`, `commandBlockOutput`, `maxCommandChainLength`, `playersSleepingPercentage`, `freezeDamage`, `fallDamage`, `waterSourceConversion` (JE), `lavaSourceConversion` (JE), `blockExplosionDropDecay` (JE), `mobExplosionDropDecay` (JE), `tntExplosionDropDecay` (JE), `showBorderEffect` (JE), etc.

### Food Values Reference

| Food | Hunger | Saturation | Notes |
|------|--------|-----------|-------|
| Enchanted Golden Apple | 4 | 9.6 | Regen II 20s(JE)/30s(BE), Absorp IV 2min, Resist 5min, Fire Res 5min |
| Golden Apple | 4 | 9.6 | Regen II 5s, Absorp I 2min |
| Rabbit Stew | 10 | 12 | Highest hunger |
| Cooked Porkchop / Steak | 8 | 12.8 | Best non-gold |
| Golden Carrot | 6 | 14.4 | Best sat/hunger ratio |
| Bread | 5 | 6 | Easy farm |
| Baked Potato | 5 | 6 | Good early |
| Cooked Chicken | 6 | 7.2 | — |
| Cooked Mutton / Salmon | 6 | 9.6 | — |
| Cooked Cod | 5 | 6 | — |
| Pumpkin Pie | 8 | 4.8 | — |
| Beetroot Soup | 6 | 7.2 | — |
| Mushroom Stew | 6 | 7.2 | — |
| Suspicious Stew | 6 | 7.2 | +effect (varies by flower) |
| Honey Bottle | 6 | 1.2 | Clears Poison |
| Chorus Fruit | 4 | 2.4 | Random teleport |
| Dried Kelp | 1 | 0.6 | Eats fastest (0.8s) |
| Raw Beef / Porkchop | 3 | 1.8 | — |
| Raw Chicken | 2 | 1.2 | 30% Hunger 30s |
| Rotten Flesh | 4 | 0.8 | 80% Hunger 30s |
| Spider Eye | 2 | 3.2 | Poison 5s |
| Pufferfish | 1 | 0.2 | Hunger III 15s, Nausea 15s, Poison II 60s |
| Poisonous Potato | 2 | 1.2 | 60% Poison 5s |

### Consumption Times
- Default: 32 ticks (1.6s). Dried Kelp: 16 ticks (0.8s). Honey Bottle: 40 ticks (2s).
- Cake must be placed first; consumes per slice.

### Armor Protection Reference

| Armor Set | Total Armor | Toughness | Knockback Res |
|-----------|-------------|-----------|---------------|
| Leather | 7 | 0 | 0 |
| Copper | 12 | 0 | 0 |
| Golden | 11 | 0 | 0 |
| Chainmail | 12 | 0 | 0 |
| Iron | 15 | 0 | 0 |
| Diamond | 20 | 8 | 0 |
| Netherite | 20 | 12 | 0.10 per piece (0.40 full) |

### Entity Height (Block Heights)
- **Player:** 1.8 blocks (can fit in 2-block space with slab). Auto-step height: 0.6 blocks.
- **Zombie/Skeleton/Creeper:** 1.95 blocks. **Spider:** 0.9×1.3. **Enderman:** 2.9 blocks.
- **Horse:** 1.6 blocks. **Camel:** 2.375 blocks. **Warden:** 2.9 blocks (fits 3-block gap).

### Tool Tier Durability
| Tier | Pickaxe | Axe | Shovel | Hoe | Sword | Hoe (netherite) |
|------|---------|-----|--------|-----|-------|-----------------|
| Wooden | 59 | 59 | 59 | 59 | 59 | — |
| Stone | 131 | 131 | 131 | 131 | 131 | — |
| Copper (BE) | 196 | — | — | — | — | — |
| Iron | 250 | 250 | 250 | 250 | 250 | — |
| Diamond | 1561 | 1561 | 1561 | 1561 | 1561 | — |
| Golden | 32 | 32 | 32 | 32 | 32 | — |
| Netherite | 2031 | 2031 | 2031 | 2031 | 2031 | 2031 |

### Armor Durability
| Material | Helmet | Chestplate | Leggings | Boots |
|----------|--------|------------|----------|-------|
| Leather | 55 | 80 | 75 | 65 |
| Golden | 77 | 112 | 105 | 91 |
| Chainmail/Iron | 165 | 240 | 225 | 195 |
| Diamond | 363 | 528 | 495 | 429 |
| Netherite | 407 | 592 | 555 | 481 |

---

*End of reference document. Compiled from [Minecraft Wiki](https://minecraft.wiki/) — see individual pages for full details and revision history.*
