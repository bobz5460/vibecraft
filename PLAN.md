# Vibecraft — Minecraft Replica in Rust

> Feature plan mapped from [MINECRAFT.md](MINECRAFT.md). `[x]` = done, `[~]` = partial, `[ ]` = not started.

## Architecture Overview
- **Engine**: Custom voxel engine built in Rust
- **Rendering**: wgpu (Vulkan/Metal/DX12 backend)
- **Windowing**: winit
- **Textures**: Real Minecraft PNG loading via blockstate/model JSON parsing

## Phase 1: Core Engine Foundation
- [x] Rust project setup with wgpu, winit, nalgebra
- [x] Window management and event loop
- [x] Chunk system (16×384×16 columns)
- [x] Greedy meshing with face culling
- [x] Texture atlas: real Minecraft PNG textures loaded from blockstate + model JSON, built at startup
- [x] Block registry (~400 BlockId variants with properties)
- [x] Camera: free-fly WASD + mouse look (yaw/pitch)
- [ ] Frustum culling (implemented but removed — always renders all loaded chunks)
- [x] DDA raycast for block targeting (max 10 blocks)
- [x] Block highlight wireframe
- [x] Block break (left click) / place selected block (right click)
- [x] Chunk buffer cache (skip GPU re-upload for unchanged chunks)
- [x] Multi-texture blocks: per-face textures for grass, logs, furnace, chest, etc.
- [x] Transparent sorting: transparent chunks back-to-front per frame
- [x] Depth of chunk re-mesh: only re-mesh on actual block changes
- [x] Async chunk generation: thread pool (CPU-count workers), non-blocking polling
- [x] Block selection hotbar (scroll wheel + number keys 1-9)
- [x] F3 debug overlay: FPS, XYZ, block, biome, facing, game time
- [x] Fast movement (Ctrl = 5× speed)

### Game Modes
- [x] **Creative mode**: instant break, F toggles flight, no damage, infinite blocks (default on spawn)
- [x] **Survival mode**: gravity, hold-to-break, health/fall damage, natural regen
- [x] **Adventure mode**: no block breaking/placing, gravity enabled
- [x] **Spectator mode**: noclip (fly through blocks), no block interaction, always flying
- [x] **Switch via command**: `/gamemode <name>` or `/gm <mode>`, numbers supported (0-3)
- [x] Hardcore: survival locked to Hard, permanent death (/hardcore command)
- [x] Difficulty levels: Peaceful, Easy, Normal, Hard with scaling damage, regen control, /difficulty command

## Phase 2: World Generation & Terrain

### Terrain Shape
- [x] Simplex noise height map with biome blending
- [x] Base terrain + detail octave for micro-variation
- [x] 9 biomes: Plains, Forest, Desert, Savanna, Taiga, SnowyTundra, Mountains, Swamp, Jungle
- [x] **River carving**: continuous river noise cuts channels into terrain, water fills to sea level
- [x] **Beach transition**: sand at water's edge where grass meets sea level
- [x] **Cave carvers**: winding spaghetti tunnels carved post-generation, 1-3 branches per chunk, varying radius 1.5-4 blocks
- [x] **Ore vein shapes**: blob-shaped 3D ellipsoid veins (coal, iron, copper, gold, redstone, lapis, diamond, emerald) with deepslate variants
- [x] **Deepslate transition**: smooth gradient from stone→deepslate across y=0..16 (uses noise with depth-based probability)
- [x] **Aquifers**: water source regions underground that carve out lakes when exposed
- [x] **Tree proximity check**: 5-block minimum distance prevents trees from spawning overlapping

### Surface Features
- [x] Trees: Oak, Spruce, Jungle, Acacia with correct log/leaf shapes
- [x] **Fallen trees**: sideways logs on forest floor
- [x] **Birch trees**: 30% chance in forests (instead of oak)
- [x] **Dark oak trees**: 2×2 trunk, large rounded canopy, DarkForest biome with podzol surface, vines, mushrooms
- [x] **Giant mushrooms**: 2% chance in swamps, stem + 3-layer cap (MushroomStem, RedMushroomBlock, BrownMushroomBlock)
- [x] **Cacti**: 2% chance in deserts, 1-3 tall, requires air around
- [x] **Sugar cane**: 5% chance near water in warm biomes, 1-3 tall
- [x] **Pumpkins**: scattered in plains
- [x] **Melons**: scattered in jungles
- [x] **Flowers**: per biome: dandelion, poppy, blue orchid, tulips, daisies, cornflower
- [x] **Tall grass/ferns**: random patches on grass blocks
- [x] **Dead bushes**: 3% chance in deserts on sand
- [x] **Vines**: hanging from trees in jungle/swamp
- [x] **Lily pads**: 10% chance on water surface in swamps
- [ ] **Coral reefs**: in warm ocean biomes (needs ocean biome implementation)
- [x] **Desert wells**: small 2×2 water pool with stone brick rim in deserts (0.3%)
- [x] **Igloos**: small snow dome (5×5 base, 3×3 mid, 1 top) with carpet, furnace in snowy biomes (0.1%)
- [x] **Swamp huts**: stilted 3×3 oak plank hut with mushroom in swamps (0.2%)
- [x] **Ocean ruins**: small stone brick platforms with partial walls near coasts (0.1%)

### Water & Fluids
- [x] Static water fill: below sea level in terrain generation
- [x] **Flowing water**: horizontal spread (7 levels), tick-based flow down and spread, source blocks (level 0) maintained
- [ ] **Bubble columns**: soul sand (upward) / magma block (downward) in water (post-phase-2)
- [x] **Lava flow**: 3-block horizontal spread, tick-based (every 8 frames), flows down, interacts with water → stone
- [x] **Surface lava pools**: small 2-4 block pools in plains/forest/savanna (0.5%), surrounded by stone
- [x] **Water + lava interaction**: water source→obsidian, flowing water→cobblestone, lava+water→stone
- [ ] **Sponges**: water absorption, drying in Nether (post-phase-2)

### Structures
- [ ] **Villages**: buildings (houses, farms, wells, blacksmith) with path generation, per biome style (future)
- [ ] **Desert temples**: with TNT trap and chest room (future)
- [ ] **Jungle temples**: with arrow trap and lever puzzle (future)
- [x] **Igloos**: 5×5 snow dome with carpet/furnace/red wool bed in snowy biomes (0.1%)
- [x] **Swamp huts**: stilted 3×3 oak plank hut with mushroom pot in swamps (0.2%)
- [ ] **Pillager outposts**: watchtower with allay cage (future)
- [x] **Ruined portals**: broken nether portal frames (obsidian, missing blocks, stone bricks base, vines)
- [x] **Debug commands**: press `/` to open console, vanilla syntax: `/<name>`, `/summon <name>`, `/place <name>`
  - Structures: `dungeon`, `portal`, `lava`, `mushroom`, `tree`, `igloo`, `swamp_hut`, `well`, `ruin`
  - Single-char aliases: `/d`, `/p`, `/l`, `/m`, `/t`, `/i`, `/sh`, `/w`, `/r`
  - `/help` lists all commands
- [ ] **Shipwrecks**: in ocean/ocean biomes (future)
- [x] **Ocean ruins**: small stone brick platforms with partial corner walls near coasts (0.1%)
- [ ] **Strongholds**: branching corridors, libraries, portal room with end portal frame (future)
- [ ] **Mineshafts**: branching tunnels with rails, spider webs, minecarts with chests (future)
- [x] **Dungeons**: 7×6×7 cobblestone/mossy rooms with spawner center, 1-2 chests, 0.5% chance per chunk y=10-55
- [x] **Desert wells**: 2×2 water pool with stone brick rim in deserts (0.3%)

## Phase 3: Block & Texture Detail

### Missing Block Types
- [x] **Slabs**: stone + oak, bottom/top via data field, half-height mesh (emit_quad with y-offset), hotbar slots
- [ ] **Stairs**: all wood and stone types with correct shape and face culling
- [ ] **Fences & walls**: with connected post/bar shapes, collision shape
- [ ] **Fence gates**: open/close animation
- [ ] **Doors**: wood and iron, open/close animation, double doors
- [ ] **Trapdoors**: wood and iron, open/close rendering
- [ ] **Pressure plates**: wood (player only) and stone (any entity)
- [ ] **Buttons**: stone and wood, wall-mounted
- [ ] **Levers**: wall-mounted with on/off state
- [ ] **Ladders**: climbable by player
- [ ] **Vines**: climbable, grows down
- [ ] **Snow layers**: 1-8 layers tall, thinner collision
- [ ] **Farms**: tilled soil (hydrated/dry states), crops (wheat, carrots, potatoes, beetroot, nether wart)
- [ ] **Stems**: pumpkin/melon stems, attached vs mature state
- [ ] **Cocoa beans**: on jungle logs, 3 growth stages
- [ ] **Cake**: 6 bite states, collision shape shrinks
- [ ] **Beds**: 16 colors, placed as double-block, set spawn point
- [ ] **Anvils**: 3 damage states, falling block entity, chiseled/corner collision
- [ ] **Grindstone**: wall/floor mounted with attached item frame
- [ ] **Stonecutter**: used for stone cutting recipes
- [ ] **Smithing table**: upgrade diamond→netherite
- [ ] **Cartography table**: map cloning/zooming
- [ ] **Barrel**: chest alternative, easier to open with block above
- [ ] **Composter**: 8 fill levels, produces bone meal
- [ ] **Lectern**: holds books, emits redstone signal
- [ ] **Jukebox**: plays music discs
- [ ] **Bell**: ring animation, glows when struck
- [ ] **Campfire**: with/without food, emits light and smoke particles
- [ ] **Lantern**: hanging light source
- [ ] **Chain**: hanging decoration
- [ ] **Amethyst geode**: budding amethyst, cluster growth stages
- [ ] **Pointed dripstone**: stalactite/stalagmite shapes
- [ ] **Sculk**: sculk catalyst, shrieker, sensor (vibration detection)
- [ ] **Deep dark biome**: ancient cities with sculk

### Lighting
- [ ] Skylight: column-based vertical propagation via per-chunk `sky_map` (replaces heightmap-based)
- [ ] Daylight cycle: sun direction + ambient min uniform, 20-min cycle, sky color transitions
- [ ] **Block light propagation**: flood-fill BFS from light-emitting blocks (torch=14, glowstone=15, lava=15, etc.), stored in per-chunk light_map, combined with skylight in mesh vertex light
- [ ] **Light smoothing**: 3×3×3 box blur pass after flood-fill reduces light banding across faces
- [ ] **Light updates**: propagate on block change (compute_block_light BFS called on break/place)
- [ ] **Internal light**: max(skylight, blocklight) for gameplay (mob spawn, plant growth)
- [ ] **Night-time darkness**: sky light dims at night via `night_factor` uniform, block light unaffected
- [ ] **Ambient occlusion**: per-face AO darkens corners and crevices (checks 3 adjacent blocks for solid neighbors)
- [ ] **Sky color tint**: top faces subtly tinted with sky color at day, warm tones at sunset
- [ ] **Sun/moon billboards**: sun (yellow circle) and moon (pale circle) rendered in sky, follow day/night cycle, hidden when below horizon
- [ ] **Improved shader lighting**: stronger directional contrast, AO multiplier, ambient_min floor, sky tint reflection
- [ ] **Mob spawning light check**: hostile ≤7, passive ≥9, slime ≤7

### Texture System
- [x] **Real Minecraft textures**: loads actual PNGs from minecraft-assets via blockstate/model JSON → face texture paths → atlas
- [x] **Biome tinting (hardcoded)**: greyscale textures tinted at load time (grass green, leaves green, water blue); overlay elements skipped
- [ ] **Animated textures**: water, lava, nether portal, prismarine, etc. (frame-based)
- [ ] **Connected textures**: glass panes, iron bars, stone walls connect to neighbors
- [ ] **Random textures**: grass, dirt get slight rotation/hue offset for variety
- [ ] **CTM (connected textures)**: for glass, bookshelves, etc. where edges blend

## Phase 4: Player & Physics

### Movement
- [x] **Player AABB**: 0.6×1.8×0.6 standing, per-axis collision (slides along walls)
- [x] **Collision detection**: against world blocks on X/Y/Z independently
- [x] **Gravity**: downward acceleration (~25 m/s² in game units)
- [ ] **Walking**: 4.317 blocks/sec on ground
- [x] **Sprinting**: Ctrl = 1.3× speed boost (no hunger cost yet)
- [ ] **Sneaking**: 1.295 blocks/sec, reduces height, prevents falling off edges
- [x] **Auto-jump**: holding space while on ground jumps on every landing
- [ ] **Jumping**: initial velocity 0.42 blocks/tick upward, variable height when held
- [ ] **Falling**: terminal velocity 3.92 blocks/tick
- [x] **Fly mode**: creative flight (F key toggle, Ctrl = 5× speed)
- [ ] **Swimming**: 3D movement in water, slower when not on surface
- [ ] **Dolphin's grace**: increased swim speed near dolphins
- [ ] **Elytra gliding**: forward motion with pitch-based lift
- [ ] **Climbing**: ladders, vines, scaffolding

### Survival
- [x] **Health**: 20 HP (10 hearts), natural regen 0.5 HP/s
- [ ] **Hunger**: 20 food points, depletes from sprinting/jumping/damage
- [ ] **Saturation**: hidden value, determines how quickly hunger depletes
- [ ] **Oxygen**: 15 seconds underwater, bubble meter, damage after depletion
- [x] **Fall damage**: 1 HP per 3 blocks fallen above 3 blocks
- [ ] **Drowning damage**: 2 HP per second after oxygen depletes
- [ ] **Fire/lava damage**: 1 HP per half-second in fire, 4 HP in lava
- [ ] **Suffocation damage**: 1 HP per half-second inside solid blocks
- [ ] **Starvation damage**: 1 HP per 4 seconds at 0 hunger (hard)
- [ ] **Cactus damage**: 1 HP per half-second touching cactus
- [ ] **Berry bush damage**: 0.5 HP per half-second in sweet berry bush
- [x] **Death + respawn**: respawn at (0,100,0), flight toggle on death
- [x] **Experience orbs**: from ores + item pickup, within 2 blocks

### Combat & Weapons
- [ ] **Attack cooldown**: JE-style cooldown bar, 84.8% threshold for crits/sweep
- [ ] **Weapon damage**: sword (7), axe (9-10), trident (9), mace, bow, crossbow
- [ ] **Critical hits**: player falling + cooldown ≥84.8% → 150% damage
- [ ] **Sweep attack** (JE): sword AoE on grounded horizontal targets
- [ ] **Sprint-knockback**: sprint + attack = extra knockback
- [ ] **Shield**: blocks 100% melee damage, durability loss on >2 HP hits
- [ ] **Axe vs shield**: 100% disable shield for 5 seconds
- [ ] **Armor damage reduction**: `damage × (1 − min(20, max(armor/5, ...))/25)`
- [ ] **Armor toughness**: diamond (8), netherite (12) — reduces high-damage effectiveness
- [ ] **Mace smash**: bonus damage based on fall distance
- [ ] **Spear combat**: jab/charge mechanics
- [ ] **Damage sources**: drowning, suffocation, starvation, fire, lava, cactus, berry bush, lightning, void
- [ ] **Natural regen formula**: requires hunger ≥18, 1 HP per 4s
- [ ] **Saturation regen (JE)**: at full hunger, consumes 1.5 sat per 1 HP per 0.5s

### Status Effects
- [ ] **Positive effects**: Speed, Haste, Strength, Jump Boost, Regeneration, Resistance, Fire Resistance, Water Breathing, Night Vision, Invisibility, Absorption, Slow Falling, Conduit Power, Dolphin's Grace, Luck, Health Boost, Saturation, Hero of the Village
- [ ] **Negative effects**: Slowness, Mining Fatigue, Weakness, Poison, Wither, Hunger, Blindness, Nausea, Levitation, Darkness, Bad Omen, Infested, Oozing, Weaving, Wind Charged, Instant Damage
- [ ] **Effect application**: potions, beacons, suspicious stew, mob attacks, environment
- [ ] **Effect removal**: milk bucket (all), honey bottle (poison), death
- [ ] **Undead immunity**: immune to Poison, Regeneration

## Phase 5: Items & Inventory

### Core Inventory
- [ ] **Item ID registry**: all ~1100 items (blocks, tools, food, materials, etc.)
- [x] **Basic hotbar**: 26 block types, scroll wheel + number keys 1-9, right-click places selected block
- [ ] **Inventory model**: 36 main slots + 9 hotbar slots + 4 armor + offhand
- [ ] **Creative inventory**: 12 tabs (building blocks, decoration, redstone, transport, misc, food, tools, combat, materials, spawn eggs, operator utilities, enchanting)
- [ ] **Survival inventory**: only items player has collected
- [ ] **Inventory screen**: render with wgpu (quad-based GUI)
- [ ] **Hotbar HUD**: 9 slots at screen bottom, selected slot highlighted
- [ ] **Item stacking**: up to 64 (16 for eggs, signs, buckets; 1 for tools/armor)
- [ ] **Item durability**: tools/armor degrade with use, break at 0
- [ ] **Off-hand**: shield, map, torch, etc.
- [ ] **Drag-and-drop**: click to pick up, click to place, shift-click to move
- [ ] **Number keys**: 1-9 select hotbar slots, Q drops item
- [ ] **Item cooldown**: visual overlay for weapon/ender pearl

### Dropped Items
- [x] **Item dropping on break**: block drops as physics-enabled `DroppedItem` (gravity, bounce, friction, 300s despawn)
- [x] **Item pickup**: within 2 blocks of player → experience +1, item removed
- [x] **Block break particles**: 6 directional `DroppedItem` cubes with random velocity, 0.5–1.3s lifetime
- [ ] **Item rendering**: held item in first-person view
- [ ] **Item frames**: displayed item on wall

### Tool System
- [ ] **5 tiers**: wood, stone, iron, gold, diamond, netherite
- [ ] **Tool types**: pickaxe, axe, shovel, hoe, sword
- [ ] **Mining speed**: correct base speed per tool (e.g. diamond pick 8.0 vs wood pick 2.0)
- [ ] **Block hardness**: correct values for all blocks (e.g. stone 1.5, obsidian 50)
- [ ] **Tool multiplier**: correct material multiplier (e.g. diamond 3×, wood 2×)
- [ ] **Mining time formula**: `ceil(hardness × 1.5 / speed)` for correct tool, 5× slower for wrong tool
- [ ] **Instant break**: tools with `speed × multiplier >= hardness × 30`
- [ ] **Enchantments**: Efficiency, Fortune, Silk Touch, Unbreaking, etc.
- [ ] **Correct drops**: silk touch returns block itself, fortune multiplies drops
- [ ] **Tool breaking animation**: screen shake/wobble

### Armor System
- [ ] **5 tiers**: leather, chain, iron, gold, diamond, netherite
- [ ] **4 slots**: helmet, chestplate, leggings, boots
- [ ] **Armor points**: protection value per piece (e.g. diamond chestplate 8 points)
- [ ] **Armor toughness**: reduces high-damage effectiveness (diamond/netherite)
- [ ] **Knockback resistance**: from netherite armor
- [ ] **Armor durability**: degrades with damage taken
- [ ] **Enchantments**: Protection, Fire Protection, Blast Protection, Projectile Protection, Feather Falling, Thorns, etc.

### Crafting
- [ ] **2×2 grid** (player inventory): all basic recipes
- [ ] **3×3 grid** (crafting table): advanced recipes
- [ ] **Shaped recipes**: exact pattern match with wildcards
- [ ] **Shapeless recipes**: ingredient set without pattern
- [ ] **Recipe book**: searchable, shows all possible recipes
- [ ] **Vanilla recipe mappings**: all ~350+ recipes
- [ ] **Smelting**: furnace, blast furnace (ores), smoker (food)
- [ ] **Fuel system**: burn times (coal 80s, lava bucket 1000s, etc.)
- [ ] **Experience from smelting**: ores give XP
- [ ] **Cooking**: campfire cooking without fuel

### Food System
- [ ] **All food items**: bread, steak, cooked porkchop, golden apple, etc.
- [ ] **Hunger/food point mapping**: correct restoration per item
- [ ] **Saturation mapping**: correct saturation per item
- [ ] **Eating animation**: crouch, arm raise, bite sounds
- [ ] **Effects**: golden apple (absorption+regeneration), suspicious stew (random effect)
- [ ] **Saturation priority**: saturation consumed before hunger

### Enchanting
- [ ] **Enchanting table**: GUI with 3 slot book, lapis input, enchant buttons
- [ ] **Enchantment registry**: all ~40 enchantments with effects
- [ ] **Level cost**: based on item, enchantment, and random seed
- [ ] **Anvil**: combine items, repair, rename, apply enchanted books
- [ ] **Anvil cost**: prior work penalty (exponential cost per rename)
- [ ] **Grindstone**: disenchant, repair combined items
- [ ] **Experience bar**: 0-139 levels, grows linearly in XP needed per level

### Brewing
- [ ] **Brewing stand**: GUI with ingredient slot, 3 potion slots, blaze powder fuel
- [ ] **Base potions**: awkward potion (nether wart → water bottle)
- [ ] **Effect potions**: 13 primary effects (speed, strength, healing, etc.)
- [ ] **Modifiers**: redstone (extend duration), glowstone (amplify), gunpowder (splash)
- [ ] **Potions rendering**: bottle with colored liquid + particles

## Phase 6: Mobs & Entities

### Entity System
- [ ] **ECS architecture**: position, velocity, health, AI components
- [ ] **Entity types**: mobs, dropped items, XP orbs, projectiles, minecarts, boats
- [ ] **Spatial index**: grid-based entity lookup for collision/AI
- [ ] **Entity networking**: sync state for multiplayer
- [ ] **Hitbox system**: per-entity AABB, attack cooldown
- [ ] **Entity rendering**: textured quads (billboards for some, 3D model for others)
- [ ] **Entity animation**: keyframe-based limb movement

### Passive Mobs
- [ ] **Sheep**: 16 wool colors, shearing, eat grass to regrow wool
- [ ] **Cow**: milk with bucket, drops leather + beef
- [ ] **Pig**: drops porkchop, saddled for riding
- [ ] **Chicken**: lays eggs, drops feather + chicken, falls slowly
- [ ] **Rabbit**: 3 skin variants, drops rabbit hide + foot (rare)
- [ ] **Horse**: 6 colors, 4 patterns, speed/jump stats, tame and ride
- [ ] **Donkey + Mule**: carry chest, can be saddled
- [ ] **Wolf**: tame with bones, attack mobs, follow player
- [ ] **Cat**: ocelot or stray, scare creepers, gift items
- [ ] **Fox**: sleep in shade, attack chickens, hold items in mouth
- [ ] **Axolotl**: fight underwater mobs, play dead
- [ ] **Frog**: eat small slime, produce froglight
- [ ] **Turtle**: lay eggs on beach, hatch into baby turtles
- [ ] **Fish**: cod, salmon, pufferfish, tropical fish (3000+ variants)
- [ ] **Squid + Glow Squid**: ink particles when hit

### Neutral Mobs
- [ ] **Zombified Piglin**: spawn in nether, neutral unless hit, group aggro
- [ ] **Wolf**: neutral until attacked, pack attack
- [ ] **Polar Bear**: neutral unless cubs near by
- [ ] **Spider**: neutral in light, hostile in dark
- [ ] **Enderman**: neutral when stared at, teleports, picks up blocks
- [ ] **Llama**: spit attack, follow caravans
- [ ] **Bee**: pollinate flowers, sting once then dies, honey production

### Hostile Mobs
- [ ] **Zombie**: burns in sunlight, breaks doors, variants (husk, drowned)
- [ ] **Skeleton**: strafes, shoots arrows, burns in sunlight
- [ ] **Creeper**: hisses, explodes, charged (lightning) = stronger
- [ ] **Spider**: climbs walls, jumps, leaves webs in abandoned mineshafts
- [ ] **Witch**: throws splash potions (poison, slowness, weakness, harming, healing)
- [ ] **Slime**: splits into smaller sizes, spawns in swamps/slime chunks
- [ ] **Phantom**: spawns after 3+ days without sleep, diving attack
- [ ] **Guardian + Elder Guardian**: laser attack, guardians in ocean monuments
- [ ] **Ravager**: village raid mob, destroys crops
- [ ] **Pillager + Vindicator + Evoker**: raid mobs, allay stealing
- [ ] **Ghast**: floats in nether, shoots fireballs
- [ ] **Magma Cube**: nether slime, splits, fire resistant
- [ ] **Blaze**: spawns in nether fortresses, shoots fireballs
- [ ] **Wither Skeleton**: drops wither skulls, inflicts wither effect
- [ ] **Hoglin + Zoglin**: nether mobs, hoglin drops porkchop, zoglin attacks everything
- [ ] **Piglin + Brute**: gold armor/tools, bartering system, brute is stronger

### Boss Mobs
- [ ] **Ender Dragon**: 200 HP, perches on portal, destroys blocks, end crystals heal
- [ ] **Wither**: 300 HP (java) / 600 HP (bedrock), wither effect, drops nether star
- [ ] **Elder Guardian**: guardian with mining fatigue, in ocean monuments

### Mob AI Systems
- [ ] **A* pathfinding**: 3D grid-based, with jump nodes
- [ ] **Sensing**: follow range (16 default), retaliate range
- [ ] **Spawn mechanics**: light level ≤ 7 for hostile, biome + block constraints
- [ ] **Despawning**: 128 blocks radius despawn for hostile, 32 for passive
- [ ] **Day/night cycle**: hostile mobs burn in sunlight (except spiders in light)
- [ ] **Equipment**: zombies/skeletons can hold items, wear armor
- [ ] **Riding**: skeletons on spiders, baby zombies on adults, etc.
- [ ] **Village raids**: waves of pillagers, bad omen effect

### Trading & Economy
- [ ] **Villager professions**: 13+ jobs (armorer, butcher, cleric, farmer, etc.)
- [ ] **Career levels**: Novice→Apprentice→Journeyman→Expert→Master (XP-based unlocks)
- [ ] **Trading GUI**: input/output slots, emerald currency, locked/unlocked trades
- [ ] **Restocking**: 2×/day when working at job site block
- [ ] **Pricing**: demand, reputation, Hero of the Village discounts
- [ ] **Wandering Trader**: random trades, llama escorts
- [ ] **Piglin bartering**: gold ingot → random items in Nether
- [ ] **Zombie villager curing**: permanent discount

## Phase 7: Redstone & Technical

### Wire & Gates
- [ ] **Redstone dust**: 15-level power, connects to adjacent components, updates in topological order
- [ ] **Redstone torch**: inverted signal, burn-out on rapid toggle
- [ ] **Repeater**: 1-4 tick delay, diode behavior, lockable by side repeater
- [ ] **Comparator**: subtract/mode compare, read container fullness
- [ ] **Redstone block**: always-powered block
- [ ] **Redstone lamp**: on/off based on power
- [ ] **Target block**: emits signal strength based on projectile hit position
- [ ] **Observer**: detects block state change, emits 1-tick pulse

### Pistons
- [ ] **Piston**: pushes up to 12 blocks, extended/retracted state as block model
- [ ] **Sticky piston**: pushes and pulls, slime-based sticking
- [ ] **Piston physics**: block pushing rules (cannot push certain blocks)
- [ ] **Piston animation**: gradual extension/retraction rendering
- [ ] **Block dropping**: pushed blocks become items if they would break

### Storage & Transport
- [ ] **Chest**: 27 slots, double chest (54 slots) when adjacent
- [ ] **Trapped chest**: emits redstone signal based on open state
- [ ] **Barrel**: 27 slots, easier to open with block above
- [ ] **Shulker box**: 27 slots, retains inventory when broken, colored variants
- [ ] **Hopper**: 5 slots, pulls items from container above, pushes to container below/adjacent
- [ ] **Dropper**: 9 slots, ejects item on pulse
- [ ] **Dispenser**: 9 slots, activates item on pulse (arrows, eggs, water, etc.)
- [ ] **Water + lava**: flow mechanics (spread, source blocks, currents)
- [ ] **Bubble column**: soul sand (upward) / magma block (downward) in water

### Advanced Mechanics
- [ ] **Piston block dropping**: pushed blocks break if they can't be placed (torch, sugar cane, etc.)
- [ ] **Update order**: redstone updates in specific order per tick
- [ ] **0-tick pulses**: pistons can extend/retract in 0 ticks with specific setups
- [ ] **Quasi-connectivity**: pistons can be powered by block above them (Java Edition feature)
- [ ] **Bud switch**: block update detector using quasi-connectivity
- [ ] **Chunk loading**: spawn chunks always loaded, portal chunk loading

### Transportation
- [ ] **Minecart**: on rails, speed 8 m/s on powered rail
- [ ] **Powered rail**: accelerates when powered, brakes when unpowered
- [ ] **Detector rail**: emits redstone when minecart passes
- [ ] **Activator rail**: activates TNT/hopper/command/furnace minecart functions
- [ ] **Boat**: on water (8 m/s), on ice (40+ m/s depending on ice type)
- [ ] **Horse riding**: taming, speed/jump stats, saddles
- [ ] **Pig riding**: saddle + carrot on a stick for steering
- [ ] **Elytra**: gliding with pitch control, firework boost
- [ ] **Soul sand bubble column**: upward in water (14 m/s)
- [ ] **Magma block bubble column**: downward in water (6 m/s)
- [ ] **Ladder/Vine climbing**: 2.35 m/s upward
- [ ] **Stairs**: faster vertical movement than jumping (3.2 m/s)

### Explosions
- [ ] **Explosion causes**: creeper (power 3), TNT (power 4), bed/anchors (power 5), charged creeper (6), wither spawn (7), ghast fireball (1)
- [ ] **Entity damage formula**: `7 × power × (impact² + impact) + 1`
- [ ] **Block destruction**: power × random(0.7, 1.3) intensity, 1352 rays from center
- [ ] **Blast resistance**: applies per block (obsidian 1200, stone 6, etc.)
- [ ] **Fire from explosions**: 1/3 destroyed blocks ignite for fire-causing explosions
- [ ] **Creeper charging**: lightning strike → charged creeper (power 6)
- [ ] **TNT chain reaction**: adjacent TNT ignites from explosion

## Phase 8: Dimensions

### Nether
- [ ] **Nether terrain**: 128-block height, ceiling, lava ocean at y=31
- [ ] **Biomes**: Nether Wastes, Crimson Forest, Warped Forest, Soul Sand Valley, Basalt Deltas
- [ ] **Nether generation**: large cave-like tunnels, exposed lava, glowstone clusters
- [ ] **Nether fossils**: bone block structures in soul sand valleys
- [ ] **Nether fortresses**: bridge/corridor structures with blaze spawners
- [ ] **Bastion remnants**: piglin structures with treasure rooms, housing, bridges, hoglin stables
- [ ] **Portal mechanics**: build 4×5 obsidian frame, light with flint+steel, player teleports to matching coords
- [ ] **Nether scaling**: 1 block in nether = 8 blocks in overworld

### The End
- [ ] **End terrain**: floating islands with end stone, chorus plants
- [ ] **End pillars**: obsidian pillars with end crystals on top (heal dragon)
- [ ] **End gateway**: small exit portal after dragon death
- [ ] **End city**: tower/gateway structures with shulker spawners, elytra
- [ ] **Chorus plant**: grows on end stone, teleports player when broken
- [ ] **End portal**: stronghold portal room, 12 eyes of ender required
- [ ] **Exit portal**: spawns after dragon defeat, returns to overworld spawn

## Phase 9: Multiplayer

### Server Architecture
- [ ] **Headless server**: no rendering, tick-based world simulation
- [ ] **Client-server protocol**: custom protocol over TCP (QUIC later)
- [ ] **Connection handshake**: version check, authentication (Yggdrasil or offline)
- [ ] **Player authentication**: offline mode (local) or Mojang/Microsoft auth
- [ ] **Chunk sync**: send chunk data to clients on load
- [ ] **Entity sync**: player positions, mob positions, updates at 20 Hz
- [ ] **Block updates**: broadcast block changes to all clients
- [ ] **Inventory sync**: player inventory, container contents
- [ ] **Keep-alive**: client-server heartbeat, timeout after 30 seconds

### Server Features
- [ ] **Tick loop**: 20 ticks per second (50ms per tick)
- [ ] **Mob spawning**: server-side spawning, sync to clients
- [x] **Command system**: /gamemode, /time set, /give, /help, /summon (structures)
- [ ] **Operator system**: op levels, permissions
- [ ] **Whitelist/blacklist**: player access control
- [ ] **Server properties**: view distance, max players, motd, etc.
- [ ] **World saving**: region file format (anvil-compatible)
- [ ] **Player data**: inventory, position, stats per player file

### Commands
- [x] **Core commands**: `/gamemode` (with aliases), `/time set <0-1200|day|night>`, `/give <block> [count]`
- [x] **Structure commands**: `/summon dungeon|portal|lava|mushroom|tree|igloo|swamp_hut|well|ruin`
- [ ] **World commands**: `/weather`, `/difficulty`, `/seed`
- [ ] **Entity commands**: `/kill`, `/effect`, `/enchant`
- [ ] **Player commands**: `/xp`, `/spawnpoint`, `/setworldspawn`
- [ ] **Gamerule commands**: `/gamerule` (doDaylightCycle, keepInventory, fallDamage, etc.)
- [ ] **Operator commands**: `/op`, `/deop`, `/ban`, `/whitelist`, `/stop`, `/save-all`
- [ ] **Target selectors**: `@p`, `@a`, `@r`, `@e`, `@s`
- [ ] **Advancement commands**: `/advancement grant/revoke`
- [ ] **Data commands**: `/data get/merge/remove` for block/entity NBT
- [ ] **Scoreboard**: `/scoreboard objectives/list/players` for tracking

## Phase 10: Polish & Optimization

### Audio
- [ ] **Sound engine**: kira or rodio, 3D positional audio
- [ ] **Block sounds**: per-block break, place, step, hit sounds
- [ ] **Ambient sounds**: cave sounds, biome ambience, weather
- [ ] **Music**: background music tracks, biome-specific
- [ ] **Mob sounds**: idle, hurt, death per mob
- [ ] **UI sounds**: click, pop, inventory open/close
- [ ] **Weather sounds**: rain, thunder

### Weather
- [ ] **Rain**: falling particles, sky light = 12, extinguishes fire
- [ ] **Snow**: snow layer accumulation, cauldrons fill with powder snow
- [ ] **Thunderstorm**: sky light = 10, mobs spawn daytime, lightning strikes
- [ ] **Lightning**: 5 HP damage, transformations (creeper→charged, pig→zombified, villager→witch)
- [ ] **Weather duration**: rain 10-20 min, thunder 3-13 min, random delays between
- [ ] **Biome-dependent**: temperature/humidity determine rain vs snow

### Graphics
- [x] **Fog**: distance fog, density=0.002, color matches sky
- [ ] **Sky rendering**: no time-of-day transitions (hardcoded blue sky + fog); no sun/moon entities
- [ ] **Clouds**: 3D cloud layer at y=192 (render as translucent quads)
- [~] **Water rendering**: static translucent surface, no wave animation yet
- [ ] **Lava rendering**: static texture, no emissive glow or animation
- [~] **Block break particles**: reuses DroppedItem system (6 directional cubes, 0.5–1.3s lifetime)
- [ ] **Armor model**: player model with armor overlay
- [ ] **Item rendering**: held item in first-person view
- [ ] **Item frames**: displayed item on wall
- [ ] **Painting**: random painting selection on wall
- [ ] **Nametag rendering**: floating text above entities
- [ ] **Block breaking animation**: crack overlay on block being mined
- [x] **Chunk borders**: F3+G toggle, vertical corner lines + horizontal edges

### UI/UX
- [ ] **Main menu**: title screen, singleplayer, multiplayer, settings, quit
- [ ] **Pause menu**: ESC overlay with save, settings, disconnect
- [ ] **Settings menu**: video (render distance, graphics quality, FOV), audio (volume sliders), controls (key bindings)
- [x] **Debug screen**: F3 overlay with position, FPS, chunk stats, targeted block, biome, facing, game time
- [x] **Command console**: `/` opens inline text input, vanilla syntax, feedback display
- [ ] **Chat**: T to open, text input, command suggestions
- [ ] **Advancements**: progress tracking with toast notifications
- [ ] **Statistics**: per-player tracking of blocks mined, mobs killed, etc.
- [ ] **Recipe book**: searchable, unlockable recipes
- [ ] **Tooltips**: item name, lore, enchantments, attributes
- [ ] **Crosshair**: dynamic, changes when targeting interactive block
- [ ] **Boss bar**: dragon/wither health bar at top of screen
- [ ] **Subtitle system**: accessibility text for sounds

### Performance
- [x] **Multi-threaded chunk generation**: generate terrain on thread pool (CPU-count workers)
- [ ] **Multi-threaded meshing**: build chunk meshes in parallel
- [x] **Incremental meshing**: only re-mesh chunks whose blocks changed (dirty flag)
- [ ] **LOD system**: render distant terrain at lower resolution
- [ ] **Occlusion culling**: frustum culling removed, all loaded chunks rendered
- [ ] **Visible face caching**: skip neighbor face checks for interior faces
- [ ] **Texture atlas mipmaps**: better distant rendering
- [x] **Buffer cache**: reuse GPU buffers across frames for unchanged chunks
- [x] **Cached depth texture**: created once and reused per frame (not re-allocated each frame)
- [ ] **Dynamic render distance**: adjust based on frame time
- [ ] **Entity culling**: skip rendering entities behind camera or far away
- [ ] **Model LOD**: lower-poly entity models at distance

### World Persistence
- [ ] **World save format**: Minecraft anvil format (region files, NBT data)
- [ ] **Player save**: inventory, position, stats in player.dat
- [ ] **Chunk compression**: zlib or Zstd compression
- [ ] **Autosave**: periodic world save
- [ ] **Level.dat**: world settings (seed, time, spawn, gamerules)
- [ ] **Backup system**: incremental backups on save
- [ ] **Resource pack loading**: load real Minecraft textures from .zip
- [ ] **Data pack support**: load advancements, recipes, loot tables, tags

### Game Customization
- [ ] **Resource packs**: load custom textures, models, sounds from .zip
- [ ] **Data packs**: add/modify recipes, loot tables, advancements, functions
- [ ] **Game rules**: `/gamerule` (mobGriefing, doFireTick, doMobSpawning, etc.)
- [ ] **World types**: Default, Superflat, Amplified, Large Biomes

## Legend
- `[x]` = done
- `[ ]` = not started
- `[~]` = partial/in progress
