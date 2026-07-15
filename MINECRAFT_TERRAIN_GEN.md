# Minecraft Terrain Generation: Complete Reference

> Based on decompiled Minecraft 1.20+ source code (`net.minecraft.world.level.levelgen`).
> This document explains the entire overworld terrain generation pipeline — from world seed to placed ore block.

---

## Table of Contents

1. [Overview: The Generation Pipeline](#1-overview-the-generation-pipeline)
2. [Chunk Status System: The 12-Stage Pipeline](#2-chunk-status-system-the-12-stage-pipeline)
3. [Randomness: How Seeds Control Everything](#3-randomness-how-seeds-control-everything)
4. [Noise Synthesis: The Building Blocks](#4-noise-synthesis-the-building-blocks)
5. [Density Functions: The Computation Model](#5-density-functions-the-computation-model)
6. [NoiseRouter: The 15-Slot Routing System](#6-noiserouter-the-15-slot-routing-system)
7. [How Terrain Is Actually Shaped (NoiseRouterData)](#7-how-terrain-is-actually-shaped-noiserouterdata)
8. [NoiseChunk: The Per-Chunk State Machine](#8-noisechunk-the-per-chunk-state-machine)
9. [NoiseBasedChunkGenerator: Putting It All Together](#9-noisebasedchunkgenerator-putting-it-all-together)
10. [Biome Generation: Climate to Biome Mapping](#10-biome-generation-climate-to-biome-mapping)
11. [Surface Rules: Grass, Sand, Gravel, and More](#11-surface-rules-grass-sand-gravel-and-more)
12. [Cave Carving: After the Terrain Is Built](#12-cave-carving-after-the-terrain-is-built)
13. [Ore Veins: The Veinifier](#13-ore-veins-the-veinifier)
14. [Aquifers: Underground Fluids](#14-aquifers-underground-fluids)
15. [Feature Placement: Trees, Ores, Flowers](#15-feature-placement-trees-ores-flowers)
16. [Structures: Villages, Temples, Strongholds](#16-structures-villages-temples-strongholds)
17. [Blending: Old vs New Chunks](#17-blending-old-vs-new-chunks)
18. [Flat World Generation](#18-flat-world-generation)
19. [Chunk Storage: Paletted Containers & Region Files](#19-chunk-storage-paletted-containers--region-files)
20. [Lighting After Generation](#20-lighting-after-generation)
21. [Biome Colors: How Terrain Gets Its Look](#21-biome-colors-how-terrain-gets-its-look)
22. [The Nether & End Dimensions](#22-the-nether--end-dimensions)
23. [Performance Architecture: Caching & Async](#23-performance-architecture-caching--async)
24. [Configuration: JSON Data-Driven System](#24-configuration-json-data-driven-system)

---

## 1. Overview: The Generation Pipeline

Minecraft's terrain generation is a **multi-stage pipeline** that transforms a 64-bit seed into a fully-decorated 16×384×16 chunk of blocks. The entire system is data-driven: most parameters come from JSON files in data packs rather than hardcoded constants.

### High-Level Flow

```
seed (64-bit long)
  │
  ├──▶ XoroshiroRandomSource (upgraded to 128-bit)
  │      │
  │      ├──▶ forkPositional() → PositionalRandomFactory
  │      │      ├── "aquifer" → aquiferRandom
  │      │      ├── "ore" → oreRandom
  │      │      └── fromHashOf("continentalness") → NormalNoise instance
  │      │
  │      └──▶ NoiseRouterData creates DensityFunction DAG
  │             │
  │             ├── 2D shifted noises (continents, erosion, ridges, temperature, vegetation)
  │             ├── Terrain splines → offset, factor, jaggedness, depth
  │             ├── Sloped cheese → caves → final density
  │             └── Ore vein noises
  │
  ├──▶ ChunkStatus.EMPTY
  │      └── Allocate ProtoChunk with LevelChunkSection[]
  │
  ├──▶ ChunkStatus.STRUCTURE_STARTS
  │      └── ChunkGenerator.createStructures() → StructureStart per structure
  │
  ├──▶ ChunkStatus.STRUCTURE_REFERENCES
  │      └── Scan 17×17 chunk area for cross-references
  │
  ├──▶ ChunkStatus.BIOMES
  │      └── fillBiomesFromNoise() → Climate.Sampler at each 4×4×4 cell
  │
  ├──▶ ChunkStatus.NOISE  ←── THE MAIN EVENT
  │      └── fillFromNoise() (async, 1-2 seconds)
  │             ├── NoiseChunk constructor: wires ALL density functions
  │             ├── Trilinear interpolation over 4×8×4 cells
  │             ├── Aquifer.computeSubstance() per block
  │             ├── OreVeinifier.calculate() per block
  │             └── 16×384×16 blocks placed
  │
  ├──▶ ChunkStatus.SURFACE
  │      └── SurfaceSystem.buildSurface() → replaces top blocks
  │
  ├──▶ ChunkStatus.CARVERS
  │      └── CaveWorldCarver + CanyonWorldCarver carve ellipsoids
  │
  ├──▶ ChunkStatus.FEATURES
  │      └── For each biome's decoration steps:
  │             ├── CountPlacement, HeightRangePlacement, InSquarePlacement
  │             ├── → OreFeature, TreeFeature, LakeFeature, etc.
  │             └── Structure piece placement
  │
  ├──▶ ChunkStatus.INITIALIZE_LIGHT
  │      └── initializeLightSources() → find glowstone, lava, etc.
  │
  ├──▶ ChunkStatus.LIGHT
  │      └── LightEngine.runLightUpdates() → sky + block light propagation
  │
  ├──▶ ChunkStatus.SPAWN
  │      └── spawnOriginalMobs() → animals
  │
  └──▶ ChunkStatus.FULL
         └── ProtoChunk → LevelChunk promotion
```

### Key Files

| File | Purpose |
|------|---------|
| `NoiseBasedChunkGenerator.java` | Main overworld/Nether/End chunk generator |
| `NoiseChunk.java` | Per-chunk noise state with interpolation caching |
| `NoiseRouter.java` | 15 density function slots container |
| `NoiseRouterData.java` | Actual noise DAG construction for each dimension |
| `DensityFunctions.java` | ~25 density function implementations |
| `RandomState.java` | Wiring harness: seeds noises, creates samplers |
| `SurfaceRules.java` | Surface material rule engine |
| `SurfaceSystem.java` | Per-column surface application |
| `Climate.java` | Climate parameter system with R-tree index |
| `OverworldBiomeBuilder.java` | Biome↔climate mapping tables |

### World Presets: What the User Sees

| Preset | Biome Source | Noise Settings | Effect |
|--------|-------------|----------------|--------|
| `NORMAL` | MultiNoise (6D climate) | OVERWORLD | Default world |
| `LARGE_BIOMES` | MultiNoise (lower frequency) | LARGE_BIOMES | Biomes 4× larger |
| `AMPLIFIED` | MultiNoise | AMPLIFIED | Extreme terrain |
| `SINGLE_BIOME_SURFACE` | FixedBiomeSource(plains) | OVERWORLD | Single biome |
| `FLAT` | FixedBiomeSource(plains) | FlatLevelSource | Layers |
| `DEBUG` | FixedBiomeSource(plains) | DebugLevelSource | All block states |

---

## 2. Chunk Status System: The 12-Stage Pipeline

Chunk statuses form a **linear singly-linked list** from `EMPTY` (index 0) to `FULL` (index 11). Each status tracks its parent, its index, what heightmaps to compute, and what `ChunkType` to use.

```java
// ChunkStatus.java
EMPTY             = register("empty",              null,     WORLDGEN_HEIGHTMAPS, ChunkType.PROTOCHUNK);
STRUCTURE_STARTS  = register("structure_starts",   EMPTY,   WORLDGEN_HEIGHTMAPS, ChunkType.PROTOCHUNK);
STRUCTURE_REFERENCES = register("structure_references", STRUCTURE_STARTS, ...);
BIOMES            = register("biomes",             STRUCTURE_REFERENCES, ...);
NOISE             = register("noise",              BIOMES,  ...);
SURFACE           = register("surface",            NOISE,   ...);
CARVERS           = register("carvers",            SURFACE, FINAL_HEIGHTMAPS, ...);
FEATURES          = register("features",           CARVERS, FINAL_HEIGHTMAPS, ...);
INITIALIZE_LIGHT  = register("initialize_light",   FEATURES,...);
LIGHT             = register("light",              INITIALIZE_LIGHT,...);
SPAWN             = register("spawn",              LIGHT,   ...);
FULL              = register("full",               SPAWN,   ..., ChunkType.LEVELCHUNK);
```

### Stage-by-Stage Detail

**EMPTY**: Bare minimum allocation. Creates `LevelChunkSection[]` for the chunk height range, all filled with air.

**STRUCTURE_STARTS**: For each structure set (village, stronghold, etc.), checks if this chunk position is a valid structure chunk. If so, creates `StructureStart` with its pieces. Uses `WorldgenRandom.setLargeFeatureWithSalt()` for deterministic seeding.

**STRUCTURE_REFERENCES**: Scans a 17×17 chunk area centered on this chunk. For each neighbor's structure starts, checks if the structure's bounding box intersects this chunk. If yes, stores a reference (packed `long` chunk position). This is how the game knows that a village in chunk (10, 5) also occupies parts of chunk (10, 6).

**BIOMES**: Evaluates `Climate.Sampler` at each quart-position (4×4×4 cell) to assign biomes. The sampler evaluates 6 density functions (temperature, humidity, continentalness, erosion, depth, weirdness) and queries the `Climate.RTree` for the nearest-matching biome.

**NOISE**: The big one. `fillFromNoise()` generates the entire 3D terrain volume. Detailed in [§9](#9-noisebasedchunkgenerator-putting-it-all-together).

**SURFACE**: `SurfaceSystem.buildSurface()` walks each column top-down, replacing noise-generated stone with biome-appropriate surface blocks (grass, sand, gravel, etc.). Detailed in [§11](#11-surface-rules-grass-sand-gravel-and-more).

**CARVERS**: Applies cave/ravine carvers from a 17×17 chunk area around this chunk. Uses `CarvingMask` to prevent double-carving. Detailed in [§12](#12-cave-carving-after-the-terrain-is-built).

**FEATURES**: Primes final heightmaps, then iterates `GenerationStep.Decoration` steps in order. For each step, places structures and `PlacedFeature`s (ores, trees, flowers, lakes, etc.).

**INITIALIZE_LIGHT**: Scans all blocks for light emission (lava=15, glowstone=15, torches, etc.). Creates initial `DataLayer` arrays for sky and block light.

**LIGHT**: Propagates sky light (from top down) and block light (from emitters outward) until convergence. Runs asynchronously on the light engine thread pool.

**SPAWN**: Places initial passive mob spawns (cows, sheep, pigs, chickens) if the chunk has sufficient light and space.

**FULL**: Promotes `ProtoChunk` → `LevelChunk`, loads block entities, registers tick containers. Must run on main thread.

### Dependency Radii

Each step requires neighbor chunks to have reached a certain status at a certain radius:

| Step | Radius 0 | Radius 1 | Radii 2-7 | Radius 8 |
|------|----------|----------|-----------|----------|
| STRUCTURE_STARTS | EMPTY | — | — | — |
| STRUCTURE_REFERENCES | STRUCTURE_STARTS | — | — | EMPTY |
| BIOMES | STRUCTURE_STARTS | — | — | EMPTY |
| NOISE | STRUCTURE_STARTS | BIOMES | STRUCTURE_STARTS | EMPTY |
| SURFACE | (same as NOISE) | | | |
| CARVERS | (inherited chain) | | | |
| FEATURES | STRUCTURE_STARTS | CARVERS | — | — |
| LIGHT | (inherited chain) | INITIALIZE_LIGHT | — | — |
| SPAWN | (inherited chain) | BIOMES | — | — |

---

## 3. Randomness: How Seeds Control Everything

### The Random Source Hierarchy

```
World Seed (64-bit long)
  │
  ├──▶ RandomSupport.upgradeSeedTo128bit()
  │      │ expands using golden ratio constant + mixStafford13
  │      │ produces two 64-bit values (lo, hi)
  │      │
  │      └──▶ Xoroshiro128PlusPlus (128-bit state)
  │             │
  │             ├──▶ fork() → new XoroshiroRandomSource(nextLong(), nextLong())
  │             │      (used for biome selection, etc.)
  │             │
  │             ├──▶ forkPositional() → XoroshiroPositionalRandomFactory
  │             │      │
  │             │      ├── at(blockX, blockY, blockZ)
  │             │      │    └── seed = Mth.getSeed(x,y,z) ^ seedLo
  │             │      │
  │             │      └── fromHashOf(String name)
  │             │           └── e.g., fromHashOf("continentalness")
  │             │
  │             └──▶ NormalNoise instances created per noise name
  │                    └── Each noise is lazily created via
  │                         ConcurrentHashMap.computeIfAbsent()
```

### Random Source Types

| Class | Algorithm | When Used |
|-------|-----------|-----------|
| `XoroshiroRandomSource` | Xoroshiro128++ | Default modern (1.15+) |
| `LegacyRandomSource` | Java LCG (`seed*25214903917+11`) | Pre-1.15 worlds, Nether legacy biomes |
| `SingleThreadedRandomSource` | Java LCG (no locking) | Single-threaded contexts |
| `ThreadSafeLegacyRandomSource` | Java LCG + CAS retry | Deprecated |

### Seed Derivation for Features

```java
// WorldgenRandom methods:
setDecorationSeed(long seed, int chunkX, int chunkZ)
    // Used for biome decoration features
    // Combines seed + chunk coords with random odd multipliers

setFeatureSeed(long seed, int index, int step)
    // Used for features within decoration steps
    // = seed + index + 10000 * step

setLargeFeatureWithSalt(long seed, int chunkX, int chunkZ, int salt)
    // Used for structure placement
    // = chunkX * 341873128712 + chunkZ * 132897987541 + seed + salt
```

This formula system ensures:
- The same world seed always produces identical terrain
- Different structure types (different salt) produce different layouts
- Different chunk coordinates produce different random sequences

### The `PositionalRandomFactory` Interface

```java
public interface PositionalRandomFactory {
    RandomSource at(BlockPos pos);
    RandomSource at(int x, int y, int z);
    RandomSource fromHashOf(String name);
    RandomSource fromHashOf(Identifier name);
    RandomSource fromSeed(long seed);
}
```

This is the backbone of deterministic randomness in Minecraft. Given a world seed and a position/name, it always produces the same `RandomSource`. The `fromHashOf("aquifer")` call is how each subsystem gets its own independent random stream.

---

## 4. Noise Synthesis: The Building Blocks

### The Noise Class Hierarchy

```
NormalNoise (Gaussian-distributed noise)
  │
  ├── PerlinNoise (octave-combined Perlin, fractal Brownian motion)
  │     └── ImprovedNoise (single octave of 3D Perlin gradient noise)
  │
  └── PerlinNoise (second instance, slightly frequency-shifted)
       └── ImprovedNoise (same structure, independent seed)
```

### ImprovedNoise: The Fundamental Unit

`ImprovedNoise` is Ken Perlin's 2002 "Improved Perlin Noise" adapted for Minecraft. Each instance has:

- **Permutation table** (`p[512]`): Fisher-Yates shuffled from a seed, determines gradient selection at each lattice point
- **Random offsets** (`xo, yo, zo`): Added to input coordinates before evaluation, acts as a per-instance field shift

**The noise computation** (`sampleAndLerp`):

```
Input: (x, y, z)
  │
  ├── Add instance offsets (xo, yo, zo)
  ├── Floor to get integer lattice corner (xf, yf, zf)
  ├── Compute fractional offset (xr, yr, zr)
  ├── For each of 8 cube corners:
  │     ├── Look up gradient index via p[]
  │     └── Dot product of gradient vector with offset from corner
  ├── Apply 6th-order smoothstep to fractional coords: 6t⁵ - 15t⁴ + 10t³
  └── Trilinear interpolation of 8 corner values
```

**Minecraft's Y-Fudge Extension**: `ImprovedNoise.noise(x, y, z, yScale, yFudge)` — a Minecraft-specific addition not in standard Perlin noise. It introduces quantization/staircasing of Y coordinates to stretch vertical features. This is what creates Minecraft's characteristic terrain where horizontal features are larger than vertical ones.

### PerlinNoise: Octave Combination (fBm)

`PerlinNoise` combines multiple `ImprovedNoise` instances at different octaves:

```
Input: (x, y, z)
  │
  value = 0
  factor = lowestFreqInputFactor    // e.g., 2^15
  valueFactor = lowestFreqValueFactor
  │
  for each octave i:
      noise[i].noise(x*factor, y*factor, z*factor, yScale*factor, yFudge*factor)
      value += amplitude[i] * noiseValue * valueFactor
      factor *= 2.0           // double frequency
      valueFactor /= 2.0      // halve amplitude
  │
  return value
```

**Octave configuration**: The `NoiseParameters` record specifies `firstOctave` (e.g., -15) and amplitudes (list of doubles). Octaves are indexed from `firstOctave` upward. The effective input scaling is `2^(-firstOctave)` so the lowest octave is at unit frequency.

**Coordinate wrapping**: `PerlinNoise.wrap(x) = x - floor(x/3.3554432e7 + 0.5) * 3.3554432e7` — prevents precision loss at large coordinates by wrapping to ±16M blocks.

### NormalNoise: Gaussian Distribution

`NormalNoise` wraps **two independent** `PerlinNoise` instances:

```
NormalNoise.getValue(x, y, z):
    x2 = x * 1.0181268882175227   // slight frequency shift for decorrelation
    y2 = y * 1.0181268882175227
    z2 = z * 1.0181268882175227
    return (first.getValue(x, y, z) + second.getValue(x2, y2, z2)) * valueFactor
```

The sum of two Perlin fields approximates a Gaussian distribution (Central Limit Theorem). The `valueFactor = 0.1667 / expectedDeviation(octaveSpan)` normalizes to a standard deviation of approximately 1/3.

### BlendedNoise: The Legacy Terrain Shaper

`BlendedNoise` was the density function for 1.18-era terrain shaping (now replaced by spline-based approach but the pattern is instructive). It blends three noise fields:

```
compute(x, y, z):
  │
  ├── Main noise (8 octaves): blend weight
  │     Used to cross-fade between min and max limit surfaces
  │
  ├── Min limit noise (16 octaves): defines ocean floor / lowlands
  ├── Max limit noise (16 octaves): defines mountain peaks / highlands
  │
  └── factor = clamp(mainNoise / 10 + 1) / 2
      return lerp(factor, minSurface / 512, maxSurface / 512) / 128
```

The `682.412` constant scales block coordinates so terrain features are ~64-128 blocks wide.

---

## 5. Density Functions: The Computation Model

The `DensityFunction` interface is the universal computation primitive:

```java
public interface DensityFunction {
    double compute(FunctionContext context);       // Single point: (blockX, blockY, blockZ)
    void fillArray(double[] output, ContextProvider); // Bulk evaluation
    DensityFunction mapChildren(Visitor visitor);  // Tree transformation
    double minValue();                             // Expected minimum output
    double maxValue();                             // Expected maximum output
    KeyDispatchDataCodec<? extends DensityFunction> codec(); // JSON serialization
}
```

### Function Types (~25 implementations in `DensityFunctions.java`)

**Primitive operations:**
| Type | Purpose |
|------|---------|
| `Noise` | Samples a `NormalNoise` at scaled `(x*xzScale, y*yScale, z*xzScale)` |
| `ShiftedNoise` | Domain warping: adds shift functions to coordinates before sampling |
| `YClampedGradient` | Linear ramp along Y: `clampedMap(blockY, fromY, toY, fromVal, toVal)` |
| `Constant` | Always returns the same value |
| `EndIslandDensityFunction` | Simplex-based island generator for the End |

**Combining operations:**
| Type | Purpose |
|------|---------|
| `Ap2` (ADD, MUL, MIN, MAX) | Binary operations on two sub-functions |
| `MulOrAdd` | Optimized a*x + b |
| `Clamp` | Clamps output to [min, max] |
| `RangeChoice` | If input in range, use whenInRange, else whenOutOfRange |
| `Spline` | Evaluates a cubic spline with coordinate inputs |
| `FindTopSurface` | Scans Y downward for where density > 0 |

**Unary transformers (Mapped):**
| Type | Effect |
|------|--------|
| `ABS` | Absolute value |
| `SQUARE` | `x²` |
| `HALF_NEGATIVE` | If x > 0: x; if x < 0: x * 0.5 |
| `QUARTER_NEGATIVE` | If x > 0: x; if x < 0: x * 0.25 |
| `SQUEEZE` | Clamp to [-1, 1] |

### Caching Markers

Density functions carry marker types that tell `NoiseChunk` how to cache them:

| Marker | Behavior |
|--------|----------|
| `Interpolated` | Sampled at cell corners, trilinearly interpolated per block |
| `FlatCache` | Pre-computed 2D array at quart resolution across chunk |
| `Cache2D` | Single-value cache keyed by (x, z) column |
| `CacheOnce` | Single-value cache reset per interpolation round |
| `CacheAllInCell` | Full cell-sized array filled once per cell |
| `BlendDensity` | Apply old/new chunk blending |

### The Visitor Pattern

`mapChildren(Visitor)` enables the entire function DAG to be transformed recursively. `RandomState` uses this to:
1. Replace `NoiseHolder` references with actual seeded `NormalNoise` instances
2. Flatten `HolderHolder` references (registry lookups)
3. Strip Markers for climate sampler creation

---

## 6. NoiseRouter: The 15-Slot Routing System

`NoiseRouter` is a record with exactly 15 `DensityFunction` slots. Each slot serves a specific purpose in the generation pipeline.

```java
public record NoiseRouter(
    // -- Aquifer/fluid system --
    DensityFunction barrierNoise,                   // "barrier"
    DensityFunction fluidLevelFloodednessNoise,     // "fluid_level_floodedness"
    DensityFunction fluidLevelSpreadNoise,          // "fluid_level_spread"
    DensityFunction lavaNoise,                      // "lava"

    // -- Biome/climate system (all 2D) --
    DensityFunction temperature,                    // "temperature"
    DensityFunction vegetation,                     // "vegetation"
    DensityFunction continents,                     // "continents"
    DensityFunction erosion,                        // "erosion"
    DensityFunction depth,                          // "depth"
    DensityFunction ridges,                         // "ridges"

    // -- Terrain surface --
    DensityFunction preliminarySurfaceLevel,        // "preliminary_surface_level"

    // -- MASTER DENSITY --
    DensityFunction finalDensity,                   // "final_density"

    // -- Ore veins --
    DensityFunction veinToggle,                     // "vein_toggle"
    DensityFunction veinRidged,                     // "vein_ridged"
    DensityFunction veinGap                         // "vein_gap"
) {
    public NoiseRouter mapAll(Visitor visitor) { ... }
        // Creates new NoiseRouter with every slot transformed via visitor
        // This is how NoiseChunk wraps every function in caching layers
}
```

### Slot Roles

| Slot | Dimensionality | Used By | Data Type |
|------|---------------|---------|-----------|
| `barrierNoise` | 3D | Aquifer | PerlinNoise, scale 0.5 |
| `fluidLevelFloodednessNoise` | 3D | Aquifer | PerlinNoise, scale 0.67 |
| `fluidLevelSpreadNoise` | 3D | Aquifer | PerlinNoise, scale 0.714 |
| `lavaNoise` | 3D | Aquifer (deep) | PerlinNoise |
| `temperature` | 2D | Climate/Biome | ShiftedNoise, shifted by SHIFT noise |
| `vegetation` | 2D | Climate/Biome | ShiftedNoise, shifted by SHIFT noise |
| `continents` | 2D | Climate/Biome + Terrain spline | ShiftedNoise, scale 0.25 |
| `erosion` | 2D | Climate/Biome + Terrain spline | ShiftedNoise, scale 0.25 |
| `depth` | Y+continents | Climate/Biome + Terrain | yClampedGradient + offset spline |
| `ridges` | 2D | Climate/Biome + Terrain | ShiftedNoise → `peaksAndValleys()` |
| `preliminarySurfaceLevel` | 2D | Aquifer, SurfaceRules | FindTopSurface of finalDensity |
| `finalDensity` | 3D | Chunk Generator | **The master density DAG** |
| `veinToggle` | Y-limited | OreVeinifier | Noise |
| `veinRidged` | Y-limited | OreVeinifier | Noise |
| `veinGap` | Y-limited | OreVeinifier | Noise |

### The Climate Sampler Construction

The climate sampler doesn't use the router directly. `RandomState` creates a flattened `Climate.Sampler`:

```java
this.sampler = new Climate.Sampler(
    router.temperature().mapAll(noiseFlattener),    // strips Markers, HolderHolders
    router.vegetation().mapAll(noiseFlattener),
    router.continents().mapAll(noiseFlattener),
    router.erosion().mapAll(noiseFlattener),
    router.depth().mapAll(noiseFlattener),
    router.ridges().mapAll(noiseFlattener),
    settings.spawnTarget()
);
```

---

## 7. How Terrain Is Actually Shaped (NoiseRouterData)

`NoiseRouterData.java` (~700 lines) is where the actual terrain shapes are defined. It constructs all the density function DAGs for each dimension.

### Constants

```java
public static final float GLOBAL_OFFSET = -0.50375F;         // Added to all terrain offsets
private static final double SURFACE_DENSITY_THRESHOLD = 1.5625D;  // Branch: surface vs caves
private static final double CHEESE_NOISE_TARGET = -0.703125D;      // Target for cheese caves
public static final double NOISE_ZERO = 0.390625D;               // Noise baseline bias
private static final double BASE_DENSITY_MULTIPLIER = 4.0D;       // Main density scaling
```

### 2D Shifted Noises (Continents, Erosion, Ridges)

All 2D biome/climate noises use domain warping:

```java
// SHIFT_NOISE creates XZ domain warping
shiftX = flatCache(cache2d(shiftA(noise(SHIFT))));
shiftZ = flatCache(cache2d(shiftB(noise(SHIFT))));

// Continents: shifted noise at 0.25 scale
continents = flatCache(shiftedNoise2d(shiftX, shiftZ, 0.25, noise(CONTINENTALNESS)));

// Erosion: same pattern
erosion = flatCache(shiftedNoise2d(shiftX, shiftZ, 0.25, noise(EROSION)));

// Ridge → peaksAndValleys transformation
ridge = flatCache(shiftedNoise2d(shiftX, shiftZ, 0.25, noise(RIDGE)));
ridges_folded = peaksAndValleys(ridge);
```

The **peaksAndValleys** transform creates distinct mountain zones:

```java
private static DensityFunction peaksAndValleys(DensityFunction weirdness) {
    return mul(
        add(
            add(weirdness.abs(), constant(-0.6666666666666666)).abs(),
            constant(-0.3333333333333333)),
        constant(-3.0));
}
```

This maps the raw ridge noise through three folds, creating valley/low/mid/high/peak categories.

### The Terrain Spline System

`registerTerrainNoises()` is the **heart of overworld shaping**. It creates cubic splines that map `(continents, erosion, ridges_folded)` to terrain parameters:

```
offset = spline(continents, erosion, ridges_folded) - 0.50375
         (blended with blendOffset near old chunks)

factor = spline(continents, erosion, weirdness, ridges_folded)
         (blended with 10.0 near old chunks)

depth = yClampedGradient(-64, 320, 1.5, -1.5) + offset

jaggedness = spline(continents, erosion, weirdness, ridges_folded)
             * halfNegative(jaggedNoise)

slopedCheese = 4.0 * quarterNegative((depth + jaggedness) * factor) + base_3d_noise
```

The `quarterNegative()` function is crucial:

```
quarterNegative(x):
    if x >= 0: return x       // Positive density (solid) passes through
    if x < 0:  return x * 0.25 // Negative density (air/cave) is attenuated
```

This means solid ground stays solid, but caves near the surface are less deep.

### Cave System Construction

The `underground()` method combines multiple cave noise sources:

```
baseCaveDensity = CAVE_LAYER * 4, squared  (layer caverns)
                + solidifiedCheese          (large open caverns)

undergroundSubtractions = min(baseCaveDensity, entrances)
                           + min(spaghetti2d + roughness, entrances)

spaghetti3d = min(spaghetti3d_rarity, spaghetti3d_thickness, ...)

fullCaves = max(undergroundSubtractions, pillars)
```

**Cave noise types:**

| Noise | What It Creates |
|-------|----------------|
| `CAVE_LAYER` | Horizontal cavern layers (squared for sharp ceilings/floors) |
| `CAVE_CHEESE` | Large, round open caverns ("swiss cheese" caves) |
| `SPAGHETTI_2D` | Horizontal tunnel networks (2D, quantized by rarity) |
| `SPAGHETTI_3D` | 3D tunnel networks |
| `SPAGHETTI_ROUGHNESS` | Textures cave walls |
| `CAVE_ENTRANCE` | Surface cave entrances |
| `PILLAR` | Column/pillar formations (donut-shaped) |
| `NOODLE` | Thin noodle caves (added at the very end) |

### The Slide System

Smooth transitions at world top and bottom:

```
slide(caves, minY, height, topStart, topEnd, topTarget, bottomStart, bottomEnd, bottomTarget):
  topFactor = yClampedGradient(minY+height-topStart, minY+height-topEnd, 1.0, 0.0)
  noise = lerp(topFactor, topTarget, noise)

  bottomFactor = yClampedGradient(minY+bottomStart, minY+bottomEnd, 0.0, 1.0)
  noise = lerp(bottomFactor, bottomTarget, noise)
  return noise
```

Overworld defaults: top starts sliding 80 blocks below world top, target = 0.117; bottom starts at Y=-64+24, target = -0.078.

### Final Density Assembly

```java
// Branch: surface vs underground
caves = rangeChoice(slopedCheese, -inf, SURFACE_DENSITY_THRESHOLD,
    surface = min(slopedCheese, 5 * entrances),    // Above threshold: surface
    underground(...))                                // Below threshold: caves

// Post-process and add noodle caves
fullNoise = min(postProcess(slideOverworld(caves)), noodle)
```

The post-processing chain:
```
postProcess = blendDensity → interpolated → mul(0.64) → squeeze
```

---

## 8. NoiseChunk: The Per-Chunk State Machine

`NoiseChunk` is the runtime engine that evaluates all density functions across a chunk's volume. It applies caching, trilinear interpolation, blending, and aquifer/ore-vein integration.

### Constructor: Wiring Everything

```
NoiseChunk(ChunkAccess chunk, RandomState randomState, DensityFunction[] samplerFunctions,
           DensityFunction finalDensity, NoiseGeneratorSettings settings, Blender blender,
           Beardifier beardifier)
  │
  ├── 1. Compute cell geometry:
  │       cellWidth = 4 (QuartPos.toBlock(1))
  │       cellHeight = 8 (QuartPos.toBlock(2))
  │       cellCountXZ = 16/4 = 4
  │       cellCountY = 384/8 = 48
  │       cellNoiseMinY = -64/8 = -8
  │
  ├── 2. Initialize Blender caches (if blender non-empty):
  │       blendAlpha FlatCache (pre-filled at quart positions)
  │       blendOffset FlatCache (pre-filled at quart positions)
  │
  ├── 3. Wrap the NoiseRouter via router.mapAll(this::wrap):
  │       Every DensityFunction in the router gets wrapped with
  │       NoiseChunk's caching/interpolation strategy based on
  │       the function's Marker type
  │
  ├── 4. Create Aquifer (if enabled):
  │       NoiseBasedAquifer with aquiferRandom
  │       otherwise DisabledAquifer (always fluid = global)
  │
  ├── 5. Build block state rule chain:
  │       ruleList = MaterialRuleList[
  │           Aquifer.computeSubstance(),   // returns fluid/air or null
  │           OreVeinifier.create()         // replaces stone with ore (optional)
  │       ]
  │
  └── 6. Create canonical density:
        fullNoiseDensity = cacheAllInCell(add(finalDensity, beardifier))
                           .mapAll(this::wrap)
```

### The wrap() Method: Caching Strategy

When `wrap(function)` is called, it selects the appropriate wrapper based on the function's `Marker.Type`:

| Marker Type | Wrapper | Storage |
|-------------|---------|---------|
| `Interpolated` | `NoiseInterpolator` | 2 slices of `[cellCountZ+1][cellCountY+1]` doubles |
| `FlatCache` | `FlatCache` | `(noiseSizeXZ+1)²` doubles, quart-resolution |
| `Cache2D` | `Cache2D` | Single (x,z) → value cache |
| `CacheOnce` | `CacheOnce` | Single value + array, invalidated per round |
| `CacheAllInCell` | `CacheAllInCell` | `cellWidth * cellHeight * cellWidth` doubles |
| `BlendDensity` | BlendAlpha passthrough | Reads from Blender |
| `BeardifierMarker` | Beardifier | Structure terrain adjustment |
| `BlendAlpha.INSTANCE` | blendAlpha FlatCache | Pre-computed |
| `BlendOffset.INSTANCE` | blendOffset FlatCache | Pre-computed |

### NoiseInterpolator: Trilinear Interpolation

The critical class for smooth noise evaluation. It maintains two slices of cell-corner values and lerps between them.

**Lifecycle:**

```
initializeForFirstCellX():
    fillSlice(true, firstCellX)    // Fill slice0 at X=firstCellX

for cellXIndex in 0..3:
    advanceCellX(cellXIndex):
        fillSlice(false, firstCellX + cellXIndex + 1)  // Fill slice1 at next X

    for cellZIndex in 0..3:
        for cellYIndex in 47..0 (top-down):
            selectCellYZ(cellYIndex, cellZIndex):
                // Load 8 corner values from slice0/slice1
                noise000..noise111

            for each block in cell (y in 7..0, x in 0..3, z in 0..3):
                updateForY(factorY) → lerps Y edges
                updateForX(factorX) → lerps X edges
                updateForZ(factorZ) → final lerp → value
                getInterpolatedState()
                    → fullNoiseDensity.compute(context)
                    → aquifer.computeSubstance(density)
                    → oreVeinifier.calculate()
                    → return BlockState or null (keep default)

    swapSlices()  // slice0 = slice1 for next iteration
```

Each `advanceCellX()` call samples all density functions at `cellCountZ * cellCountY` cell corner positions. The `selectCellYZ()` call loads the 8 corners of the current cell for fast trilinear interpolation.

### CacheAllInCell

Stores `cellWidth × cellHeight × cellWidth` values (4×8×4 = 128 per cell). Array is indexed:

```java
index = ((cellHeight - 1 - y) * cellWidth + x) * cellWidth + z
// Y is reversed (top-down iteration)
```

The array is filled in `selectCellYZ()` via the function's `fillArray` method.

---

## 9. NoiseBasedChunkGenerator: Putting It All Together

`NoiseBasedChunkGenerator` is the top-level generator for all vanilla-style dimensions. Its `fillFromNoise()` / `doFill()` methods are the core.

### doFill(): The Block Placement Loop

```java
// 1. Cell count setup
cellWidth = 4, cellHeight = 8
cellCountX = 16/4 = 4, cellCountZ = 16/4 = 4
cellCountY = 384/8 = 48
cellMinY = -64/8 = -8

// 2. For each cell X (0..3):
for cellXIndex:
    noiseChunk.advanceCellX(cellXIndex)

    // 3. For each cell Z (0..3):
    for cellZIndex:
        // 4. For each cell Y (47..0, top-down):
        for cellYIndex:
            noiseChunk.selectCellYZ(cellYIndex, cellZIndex)

            // 5. For each block within the cell:
            for yInCell (7..0):
                noiseChunk.updateForY(posY, factorY)
                for xInCell (0..3):
                    noiseChunk.updateForX(posX, factorX)
                    for zInCell (0..3):
                        noiseChunk.updateForZ(posZ, factorZ)
                        state = noiseChunk.getInterpolatedState()
                        if state != AIR:
                            section.setBlockState(...)
                            heightmap.update(...)
                            if aquifer.shouldScheduleFluidUpdate():
                                markPosForPostProcessing(...)

    noiseChunk.swapSlices()

noiseChunk.stopInterpolation()
```

The iteration order is **Y-innermost, X-middle, Z-outer** with X advancing cells. This matches the double-buffered slice pipeline in `NoiseInterpolator`.

### Other Key Methods

**`buildSurface()`**: Delegates to `randomState.surfaceSystem().buildSurface()` which evaluates `SurfaceRules.RuleSource` against density/biome/context and replaces surface blocks.

**`applyCarvers()`**: Scans a 9×9 chunk area for configured carvers in each biome. For each carver that passes `isStartChunk()`, calls `carver.carve()` with the `CarvingMask`.

**`iterateNoiseColumn()`**: Used for `getBaseHeight()` and `getBaseColumn()`. Creates a temporary 1-cell NoiseChunk and evaluates density at a single (x, z) column.

---

## 10. Biome Generation: Climate to Biome Mapping

### The 6-Dimensional Climate Space

Biomes are determined by 6 noise parameters, each a `DensityFunction` from the `NoiseRouter`:

| Dimension | Range | Source | What It Controls |
|-----------|-------|--------|-----------------|
| Temperature | -1.0 to 1.0 | ShiftedNoise | Hot vs cold |
| Humidity | -1.0 to 1.0 | ShiftedNoise | Wet vs dry |
| Continentalness | -1.0 to 1.2 | ShiftedNoise, 0.25 scale | Deep ocean → far inland |
| Erosion | -1.0 to 1.0 | ShiftedNoise, 0.25 scale | Smooth → rugged |
| Depth | ~-1.5 to 1.5 | yClampedGradient + offset | Below surface → above |
| Weirdness | -1.0 to 1.0 | ShiftedNoise, scaled → peaksAndValleys | Valleys → peaks |

### Parameter Quantization

All float parameters are quantized to `long` by multiplying by 10000:

```java
public static long quantizeCoord(float coord) {
    return (long)(coord * 10000.0F);
}
```

### The R-Tree Spatial Index

`Climate.RTree<T>` is a bounding-volume hierarchy that enables fast nearest-biome queries in 7D space (6 parameters + offset).

**Structure:**
```
RTree<T>
  └── Node<T> (abstract)
       ├── Leaf<T>      ── (ParameterPoint, Holder<Biome>)
       └── SubTree<T>   ── internal node with ≤6 children
```

Each `Node` stores a `Parameter[7]` bounding box that spans all children.

**Building:** Recursive with fanout of 6:
1. If 1 child: leaf
2. If ≤6 children: SubTree
3. If >6 children: pick the dimension that minimizes total bounding box cost, sort along it, bucketize by power-of-6 expected count, recurse

**Searching:**
```
search(target, candidate):
    for each child:
        if child bounding box is closer than current best:
            recurse into child (or return leaf value)
    return candidate
```

Thread-local caching stores the last result as a search hint.

### The `ParameterPoint` Fitness Function

```java
private long fitness(TargetPoint target) {
    return square(temperature.distance(target.temperature))
         + square(humidity.distance(target.humidity))
         + square(continentalness.distance(target.continentalness))
         + square(erosion.distance(target.erosion))
         + square(depth.distance(target.depth))
         + square(weirdness.distance(target.weirdness))
         + square(offset);  // Tiebreaker
}
```

The `distance()` method returns 0 if the target is inside the parameter's interval, otherwise the distance to the nearest endpoint:

```java
public long distance(long target) {
    long above = target - this.max;
    long below = this.min - target;
    if (above > 0L) return above;
    return Math.max(below, 0L);
}
```

### The OverworldBiomeBuilder Tables

Temperature is divided into 5 bands: frozen [-1.0, -0.45], cold [-0.45, -0.15], temperate [-0.15, 0.2], warm [0.2, 0.55], hot [0.55, 1.0].

Humidity is divided into 5 bands: dry through wet.

Erosion is divided into 7 bands (highly eroded to barely eroded).

Continentalness is divided into 7 bands: mushroom fields, deep ocean, ocean, coast, near inland, mid inland, far inland.

**The Middle Biomes table (5×5):**

```
              dry        [1]         [2]          [3]          wet
frozen:   SNOWY_PLAINS SNOWY_PLAINS SNOWY_PLAINS SNOWY_TAIGA  TAIGA
cold:     PLAINS       PLAINS       FOREST       TAIGA        OLD_GROWTH_SPRUCE_TAIGA
temperate:FLOWER_FOREST PLAINS      FOREST       BIRCH_FOREST DARK_FOREST
warm:     SAVANNA      SAVANNA      FOREST       JUNGLE       JUNGLE
hot:      DESERT       DESERT       DESERT       DESERT       DESERT
```

**Plateau biomes replace middle biomes when weirdness produces plateau zones.**

**Weirdness zones** (13 bands, symmetrical about center):
- Center [-0.05, 0.05]: **Valleys** — rivers and shores
- Next outward: Low slices, Mid slices, High slices, **Peaks** (outermost)

Weirdness < 0 → negative valley side (stony shore), weirdness ≥ 0 → positive valley side (rivers).

### The End Biome Distribution

The End uses distance-from-origin plus erosion:

```
if chunkX² + chunkZ² ≤ 4096: THE_END (central island)
else:
    if erosion > 0.25:          END_HIGHLANDS
    if erosion ≥ -0.0625:       END_MIDLANDS
    if erosion < -0.21875:      SMALL_END_ISLANDS
    else:                       END_BARRENS
```

### BiomeManager Jitter (Wobbly Boundaries)

`BiomeManager` adds pseudo-random jitter to biome boundaries using "fiddled distance":

For each position, 8 corners of a 2×2×2 grid in quart-space are computed. Each corner's biome is sampled, but the distances are perturbed by per-corner random offsets from a linear congruential generator seeded with the world seed and corner coordinates. The nearest corner (in fiddled space) wins.

This creates the characteristic wobbly biome boundaries rather than perfect grid-aligned transitions.

---

## 11. Surface Rules: Grass, Sand, Gravel, and More

### Architecture

The surface system is a two-phase pipeline:

```
RuleSource ──apply(Context)──▶ SurfaceRule
ConditionSource ──apply(Context)──▶ Condition

SurfaceRule.tryApply(x, y, z) → BlockState | null
Condition.test() → boolean
```

### The Context Class

`SurfaceRules.Context` is the shared mutable state for a chunk:

```java
final class Context {
    long lastUpdateXZ;      // Version counter for column-level caching
    long lastUpdateY;       // Version counter for Y-level caching

    int blockX, blockZ;     // Current column
    int surfaceDepth;       // From SurfaceSystem.getSurfaceDepth()
    double surfaceSecondary; // From SurfaceSystem.getSurfaceSecondary()

    int blockY;             // Current Y level
    Holder<Biome> biome;    // Lazily fetched
    int waterHeight;        // Top of water column (or MIN_VALUE for none)
    int stoneDepthAbove;    // Blocks of stone from top of column
    int stoneDepthBelow;    // Blocks of stone to cave ceiling
}
```

`updateXZ()` (per column) increments `lastUpdateXZ` which invalidates all `LazyXZCondition` caches (Hole, Steep).

`updateY()` (per block) increments `lastUpdateY` which invalidates `LazyYCondition` caches (Biome, StoneDepth, Water, Y, Temperature, VerticalGradient).

### Lazy Condition Evaluation

Conditions are cached using version counters:

```java
abstract class LazyCondition implements Condition {
    long lastUpdate;
    Boolean result;

    boolean test() {
        if (lastUpdate == getContextLastUpdate())
            return result;               // Cached!
        lastUpdate = getContextLastUpdate();
        result = compute();
        return result;
    }
}

class LazyXZCondition extends LazyCondition {
    long getContextLastUpdate() { return context.lastUpdateXZ; }
}

class LazyYCondition extends LazyCondition {
    long getContextLastUpdate() { return context.lastUpdateY; }
}
```

### Condition Types

| Condition | Type | Rule |
|-----------|------|------|
| `BiomeCondition` | LazyY | Tests if current biome is in target set |
| `StoneDepthCheck` (ON_FLOOR) | LazyY | `stoneDepthAbove <= 1 + offset + surfaceDepth + secondaryDepth` |
| `StoneDepthCheck` (UNDER_FLOOR) | LazyY | Adds `addSurfaceDepth = true` and secondary depth range |
| `YCondition` | LazyY | `blockY + stoneDepthAbove >= anchor.resolveY() + surfaceDepth * multiplier` |
| `WaterCondition` | LazyY | `blockY + stoneDepth >= waterHeight + offset + surfaceDepth * multiplier` |
| `NoiseThresholdCondition` | On-demand | Tests noise value against [min, max] |
| `VerticalGradientCondition` | On-demand | Probabilistic transition zone between two Y anchors |
| `HoleCondition` | LazyXZ | `surfaceDepth <= 0` (surface depression) |
| `SteepCondition` | LazyXZ | Heightmap difference between adjacent columns ≥ 4 |
| `AbovePreliminarySurface` | On-demand | `blockY >= minSurfaceLevel` (bilinearly interpolated) |
| `TemperatureHelperCondition` | LazyY | Delegates to biome's `coldEnoughToSnow()` |

### Rule Types

| Rule | Behavior |
|------|----------|
| `StateRule` | Always returns a fixed `BlockState` |
| `TestRule` | If condition true → delegate to followup; else → null (no match) |
| `SequenceRule` | Tries each sub-rule in order; returns first non-null |
| `Bandlands` | Delegates to `SurfaceSystem.getBand()` for terracotta bands |

### The SurfaceSystem Traversal

```java
public void buildSurface(RuleSource ruleSource):
    rule = ruleSource.apply(context)

    for each column (x, z) in chunk:
        startingHeight = chunk.getHeight(WORLD_SURFACE_WG, x, z) + 1

        context.updateXZ(blockX, blockZ)

        stoneAboveDepth = 0
        waterHeight = MIN_VALUE
        nextCeilingStoneY = MAX_INT

        for y from startingHeight down to minY:
            block = column.getBlock(y)

            if block is air:
                stoneAboveDepth = 0
                waterHeight = MIN_VALUE
            elif block has fluid:
                if waterHeight == MIN_VALUE:
                    waterHeight = y + 1
            elif block is solid:
                // Find cave ceiling for stoneDepthBelow
                if nextCeilingStoneY >= y:
                    scan downward for air → nextCeilingStoneY

                stoneAboveDepth++
                stoneBelowDepth = y - nextCeilingStoneY + 1

                context.updateY(stoneAboveDepth, stoneBelowDepth, waterHeight, y)

                if block is defaultBlock:  // Only replace noise fill
                    newState = rule.tryApply(blockX, y, blockZ)
                    if newState != null:
                        column.setBlock(y, newState)
```

**Key insight**: Surface rules ONLY replace blocks that are `defaultBlock` (the noise fill, typically stone). Already-modified blocks (from previous surface passes or player edits) are left alone.

### Noise-Driven Surface Depth

```java
// SurfaceSystem.java
protected int getSurfaceDepth(int blockX, int blockZ) {
    double noiseValue = this.surfaceNoise.getValue(blockX, 0.0D, blockZ);
    return (int)(noiseValue * 2.75 + 3.0 + smallRandom);
}

protected double getSurfaceSecondary(int blockX, int blockZ) {
    return this.surfaceSecondaryNoise.getValue(blockX, 0.0D, blockZ);
}
```

`getSurfaceDepth()` returns values in [0, 7] typically. This controls how deep surface materials (grass, dirt, sand) extend before hitting stone. The secondary noise adds additional variation mapped to [0, secondaryDepthRange].

### Preliminary Surface Bilinear Interpolation

The "preliminary surface level" (from the aquifer noise) is sampled at 16-block cells and bilinearly interpolated:

```java
int cornerCellX = blockX >> 4;
int cornerCellZ = blockZ >> 4;
// Sample 4 corners
cache[0] = noiseChunk.preliminarySurfaceLevel(cornerCellX << 4, cornerCellZ << 4);
// ... etc for corners (0,1), (1,0), (1,1)
int surface = floor(lerp2(fracX, fracZ, cache[0], cache[1], cache[2], cache[3]));
minSurfaceLevel = surface + surfaceDepth - 8;
```

The `- 8` ensures surface rules start applying 8 blocks below the estimated surface.

### Badlands Pillars (Eroded Badlands)

Creates tall, narrow pillars by adding stone fill above the normal surface. Uses three noise instances:

```java
double pillarBuffer = min(abs(badlandsSurfaceNoise * 8.25),
                          badlandsPillarNoise(x*0.2, z*0.2) * 15.0);
if pillarBuffer > 0:
    extensionTop = 64 + min(pillarBuffer² * 2.5, ceil(pillarFloor * 50) + 24);
    // Fill from extensionTop down through air to terrain surface
```

### Clay Bands (Badlands Terracotta)

A 192-element array of terracotta colors with random band widths:

```
Default: plain terracotta
Every ~5 elements: orange terracotta (1 block wide)
Then bands of: yellow (1), brown (2), red (1), white with light-gray borders
```

The band pattern repeats every 192 blocks, with a noise offset:
```java
int offset = (int)round(clayBandsOffsetNoise(x, z) * 4.0);
return clayBands[(y + offset + 192) % 192];  // Wrap around
```

---

## 12. Cave Carving: After the Terrain Is Built

Carving runs **after** noise-based terrain generation and surface application but **before** features (ores, trees). It operates on a single chunk but considers carvers from a 17×17 chunk area.

### Invocation Order

```java
// NoiseBasedChunkGenerator.applyCarvers()
CarvingMask mask = chunk.getOrCreateCarvingMask();

for dx in -8..8:
    for dz in -8..8:
        sourcePos = (chunk.x + dx, chunk.z + dz)
        for each carver in sourcePos's biome:
            random.setLargeFeatureSeed(seed, sourcePos.x, sourcePos.z)
            if carver.isStartChunk(random):
                carver.carve(context, chunk, ..., random, aquifer, sourcePos, mask)
```

### The `carveEllipsoid()` Primitive

The fundamental carving operation. Both caves and canyons use this:

```java
for each (xIndex, zIndex) in bounding rectangle:
    xd = (worldX + 0.5 - centerX) / hRadius
    zd = (worldZ + 0.5 - centerZ) / hRadius
    if xd² + zd² < 1.0:           // 2D circle test first (narrowing)
        for worldY = maxY down to minY:
            yd = (worldY - 0.5 - centerY) / vRadius
            if !skipChecker.shouldSkip(xd, yd, zd, worldY):
                if !mask.get(xIndex, worldY, zIndex):
                    mask.set(...)
                    carveBlock(...)
```

### Cave Carving (CaveWorldCarver)

```
caveCount = random.nextInt(random.nextInt(random.nextInt(15) + 1) + 1)
           // Heavily skewed: avg ~3.5 caves/chunk

for each cave:
    choose starting point (random XZ within source chunk, Y from config)
    hRadiusMult ~ Uniform[0.7, 1.4]
    vRadiusMult ~ Uniform[0.8, 1.3]
    floorLevel ~ Uniform[-1.0, -0.4]

    if (25% chance):
        createRoom()  // Single ellipsoid at start
        tunnels += random.nextInt(4)  // 1-4 additional branches

    for each tunnel:
        choose heading (hRotation, vRotation)
        thickness ~ random.nextFloat() * 2.0 + random.nextFloat()
            // 10% chance: thickness *= up to 4x
        distance ~ maxDistance (84-112 blocks)

        createTunnel():
            for step in 0..distance:
                hRadius = 1.5 + sin(π * step/distance) * thickness
                        // Sinusoidal: widest at midpoint
                vRadius = hRadius * yScale

                // Move forward
                x += cos(hRotation) * cos(vRotation)
                y += sin(vRotation)
                z += sin(hRotation) * cos(vRotation)

                // Random walk direction
                vRotation *= 0.7 (0.92 if steep)
                hRotation += random walk

                // Branching at splitPoint (mid-to-3/4)
                if step == splitPoint && thickness > 1.0:
                    branch ±90° horizontally, 1/3 vertical rotation

                if random.nextInt(4) != 0:  // 75% carve chance
                    carveEllipsoid(...)
```

**Cave floor leveling**: `yd <= floorLevel` blocks are skipped, creating flat cave floors.

### Canyon/Ravine Carving (CanyonWorldCarver)

```
Only 1% chance per chunk (vs 15% for caves)
Y range: 10-67 (near surface)
yScale = 3.0 (canyons are deep)

Width factors per vertical level:
    for each Y in world height:
        if yIndex == 0 or random.nextInt(widthSmoothness=3) == 0:
            widthFactor = 1.0 + random.nextFloat()²
    // Width changes every ~3 blocks on average

Skip checker:
    (xd² + zd²) * widthFactor + yd²/6 >= 1.0
    // Canyon walls are vertical-stretched (÷√6) and width varies by level
```

**Key differences from caves:**

| Aspect | Caves | Canyons |
|--------|-------|---------|
| Probability | 15%/chunk | 1%/chunk |
| Shape | Tunnels with sinusoidal profile | Gorges with variable-width walls |
| Y range | 8-180 | 10-67 |
| yScale | 0.1-0.9 (flattened) | 3.0 (deep) |
| Floor | Flat (floorLevel skip) | Symmetric (no floor) |
| Branching | 25% chance at midpoint | None |
| Rooms | 25% chance at start | None |

### Nether Caves

- No aquifer (simple lava/air placement)
- Carve through lava AND water
- Thicker (`* 2.0`) and taller (`yScale = 5.0`)
- Fewer caves per chunk (bound = 10 vs 15)

### The CarvingMask

A bitmask of size `256 × height` (16×16 columns × vertical sections):

```java
int getIndex(int x, int y, int z) {
    return (x & 0xF) | ((z & 0xF) << 4) | ((y - minY) << 8);
}
```

Shared across all carvers for the entire chunk. Once a bit is set, no subsequent carver from any origin can modify that block.

### Block Replacement Logic

```java
carveBlock(chunk, pos, aquifer):
    blockState = chunk.getBlockState(pos)

    // Track if carving under grass (for ceiling replacement)
    if blockState is GRASS_BLOCK or MYCELIUM:
        hasGrass = true

    // Only replace blocks in the configured replaceable tag
    if !canReplaceBlock(configuration, blockState):
        return false

    // Ask aquifer what to place
    state = getCarveState(context, config, pos, aquifer)
    // Returns null if density barrier blocks it

    chunk.setBlockState(pos, state)

    // If carving under grass, replace dirt below with biome top
    if hasGrass && chunk.getBlockState(below).is(DIRT):
        chunk.setBlockState(below, context.topMaterial(biome))
```

---

## 13. Ore Veins: The Veinifier

### Vein Types

| Type | Y Range | Ore | Raw Block | Filler |
|------|---------|-----|-----------|--------|
| COPPER | 0-50 | copper_ore | raw_copper_block | granite |
| IRON | -60 to -8 | deepslate_iron_ore | raw_iron_block | tuff |

### Algorithm

For each block, `OreVeinifier.calculate()` evaluates:

```
1. veinToggle.compute()
   > 0 → copper vein
   ≤ 0 → iron vein

2. veininess = |veinToggle|  // Magnitude controls presence

3. Edge roundoff: Y-distance from vein min/max edges
   distance 0→20 → roundoff -0.2→0.0
   Near edges, veins taper off

4. If veininess + roundoff < 0.4: skip (no vein here)

5. Solidness test: random.nextFloat() > 0.7F → skip
   Only 30% of eligible blocks become vein

6. Ridge test: veinRidged.compute() >= 0 → skip
   Ridge noise carves gaps in veins

7. Richness = clampedMap(veininess, 0.4, 0.6, 0.1, 0.3)
   Maps veininess to ore richness fraction

8. If random < richness AND veinGap > -0.3:
     place ore/raw block
   else:
     place filler block (granite/tuff)
```

**Effective probability per eligible block**: ~30% (solidness) × 10-30% (richness) = 3-9% ore placement rate in the vein region.

---

## 14. Aquifers: Underground Fluids

The `NoiseBasedAquifer` is a sophisticated system that creates realistic water/lava pockets.

### Grid Structure

Aquifer cells are arranged on a 3D grid with spacing 16×12×16 (X×Y×Z). Within each cell, the actual sampling point is randomly offset:

```java
static final int X_RANGE = 10;   // random offset within cell: [0..10)
static final int Z_RANGE = 10;
static final int Y_RANGE = 9;
```

### `computeSubstance()` Algorithm

```
computeSubstance(position, density):
  if density > 0: return null (solid)
  if Y > skipSamplingAboveY: return global fluid
  if global fluid is lava: return lava immediately

  1. Find 4 nearest aquifer cell centers
     (2×3×2 neighborhood around pos)

  2. For each cell:
     - Get fluid status (water level, fluid type)
     - Using fluidLevelFloodednessNoise and fluidLevelSpreadNoise

  3. Compute barrier pressure between fluid cells:
     - If one is lava and other is water: pressure = 2.0 (strong barrier)
     - Compute gradient from fluid level difference + vertical position
     - Add barrierNoise (perlin)
     - pressure = 2.0 * (noise + gradient)

  4. If density + barrierPressure > 0.0:
     return null (barrier holds → solid stone wall)
   else:
     return fluid state from nearest cell
```

The barrier system creates the natural-looking stone walls between water and air pockets that give Minecraft's cave systems their characteristic partial flooding.

### Fluid Level Computation

For each aquifer cell:
1. Check 13 nearby surface points (preliminary surface levels from noise)
2. If cell is below surface → use global fluid (sea level water)
3. Otherwise compute randomized fluid surface using `fluidLevelFloodednessNoise` and `fluidLevelSpreadNoise`
4. Deep cells (below Y=-10) may convert water to lava via `lavaNoise`

---

## 15. Feature Placement: Trees, Ores, Flowers

### Architecture

```
BiomeGenerationSettings (per biome: List<HolderSet<PlacedFeature>>)
  │
  ├── PlacedFeature
  │     ├── Holder<ConfiguredFeature<?, ?>>
  │     └── List<PlacementModifier>  ← chain of position transformers
  │
  └── ConfiguredFeature<FC, F extends Feature<FC>>
        ├── F feature (OreFeature, TreeFeature, LakeFeature, etc.)
        └── FC config (OreConfiguration, TreeConfiguration, etc.)
```

### The Placement Pipeline

```java
// PlacedFeature.placeWithContext()
Stream<BlockPos> placements = Stream.of(chunkOrigin);

for PlacementModifier modifier in placement:
    placements = placements.flatMap(p -> modifier.getPositions(context, random, p));

// Now place at each resulting position
for each position:
    feature.place(config, level, generator, random, position);
```

Each modifier is a `Stream<BlockPos> → Stream<BlockPos>` transformer, composed via flatMap.

### Placement Modifier Types

**Count-based (RepeatingPlacement) — multiplies positions:**

| Modifier | Behavior |
|----------|----------|
| `CountPlacement` | Place N times, N from IntProvider |
| `NoiseBasedCountPlacement` | N varies by biome noise |
| `NoiseThresholdCountPlacement` | Two different N, depending on noise threshold |

**Position transformers:**

| Modifier | Behavior |
|----------|----------|
| `InSquarePlacement` | Random offset within 16×16 chunk column |
| `HeightRangePlacement` | Sample Y from HeightProvider (Uniform, Biased, etc.) |
| `HeightmapPlacement` | Place on surface (WORLD_SURFACE_WG or OCEAN_FLOOR_WG) |
| `RandomOffsetPlacement` | ±random X/Y/Z |
| `EnvironmentScanPlacement` | Scan up/down to find target block |

**Filters — reduce positions:**

| Modifier | Behavior |
|----------|----------|
| `RarityFilter` | `1/chance` probability to pass |
| `SurfaceRelativeThresholdFilter` | Y must be within range of surface |
| `SurfaceWaterDepthFilter` | Water depth above ocean floor <= max |
| `BlockPredicateFilter` | Custom block predicate test |
| `BiomeFilter` | **Always last** — verifies biome still has this feature |

### The Decoration Order

Features are organized by `GenerationStep.Decoration`:

1. `RAW_GENERATION` — Bedrock, basic shaping
2. `LAKES` — Lake features
3. `LOCAL_MODIFICATIONS` — Local shaping
4. `UNDERGROUND_STRUCTURES` — Underground structures
5. `SURFACE_STRUCTURES` — Surface structures
6. `STRONGHOLDS` — Strongholds
7. `UNDERGROUND_ORES` — **Ore placement**
8. `UNDERGROUND_DECORATION` — Underground decoration
9. `FLUID_SPRINGS` — Springs
10. `VEGETAL_DECORATION` — **Trees, flowers, grass**
11. `TOP_LAYER_MODIFICATION` — Ice, snow, freeze

### Ore Feature Algorithm

Ores use overlapping blobs:

```java
// OreFeature.place()
// 1. Pick two endpoint positions with random spread
// 2. Generate `size` sphere centers along the line
//    Each sphere radius = f(step_position) * random * size/16
// 3. Remove overlapping spheres (larger absorbs smaller)
// 4. For each sphere, iterate blocks in radius
//    Test against OreConfiguration.TargetBlockState
//    If target block matches (e.g., stone → iron_ore), place ore
// 5. Optional: discard if near air (discardChanceOnAirExposure)
```

### Tree Feature Algorithm

```java
// TreeFeature.place()
// 1. Check height bounds, get free height
// 2. Place roots (optional RootPlacer)
// 3. TrunkPlacer.placeTrunk() → returns foliage attachment points
// 4. FoliagePlacer.createFoliage() at each attachment point
// 5. TreeDecorators: vines, bee nests, cocoa, etc.
// 6. BFS flood-fill: update leaf distances from logs
```

Tree decomposition:

| Component | Types |
|-----------|-------|
| `TrunkPlacer` | Straight, Fancy, Mega, Forking, Bending, Cherry, DarkOak, Giant |
| `FoliagePlacer` | Blob, Spruce, Pine, MegaPine, Cherry, Bush, Palm-like |
| `RootPlacer` | Oak, Mangrove (root system) |
| `TreeDecorator` | Vine, Beehive, Cocoa, AttachedToLogs, AttachedToLeaves |

---

## 16. Structures: Villages, Temples, Strongholds

### Architecture

```
StructureSet (placement + weighted structure list)
  │
  ├── StructurePlacement (how to choose chunk positions)
  │     ├── RandomSpreadStructurePlacement (villages, temples)
  │     └── ConcentricRingsStructurePlacement (strongholds)
  │
  └── Structure (generation logic)
        ├── JigsawStructure (village, trail ruins) → JigsawPlacement
        ├── SinglePieceStructure (simple) → hand-coded piece
        └── Template-based → StructureTemplate from NBT + processors
```

### Structure Placement

**Random spread** (most structures):

```java
isStructureChunk(chunkX, chunkZ):
    // Map to grid cell
    gridX = floorDiv(chunkX, spacing)
    gridZ = floorDiv(chunkZ, spacing)

    // Deterministic random within cell
    random.setLargeFeatureWithSalt(seed, gridX, gridZ, salt)
    limit = spacing - separation
    offsetX = random.nextInt(limit)
    offsetZ = random.nextInt(limit)

    // This chunk is the structure chunk only if:
    return chunkX == gridX * spacing + offsetX
        && chunkZ == gridZ * spacing + offsetZ
```

The `floorDiv` ensures negative coordinates also map to grid cells correctly.

### Jigsaw Placement

Jigsaw structures (villages, trail ruins, etc.) use a BFS expansion algorithm:

```
JigsawPlacement.addPieces(startPool, startJigsaw, maxDepth, position):
  │
  1. Pick random center element from startPool
  2. Find anchor jigsaw block (if specified)
  3. Adjust Y to terrain height (TERRAIN_MATCHING or RIGID)
  4. Create Placer with maxDistance bounds (voxel shape)

  5. BFS expansion:
     for each piece in queue:
       for each jigsaw block on piece:
         targetPool = jigsaw.pool
         targetElement = random from targetPool
         for each rotation:
           Calculate target position (jigsaw alignment)
           Check RIGID vs TERRAIN_MATCHING Y alignment
           if !voxelShape.overlaps(existing pieces):
             Add piece, track junctions, enqueue
```

**Y-alignment logic**:
- RIGID + RIGID: Exact Y offset match
- RIGID + TERRAIN_MATCHING: Child snaps to terrain
- TERRAIN_MATCHING + TERRAIN_MATCHING: Both at terrain, junction at midpoint

### Structure Templates (NBT Files)

`.nbt` files are loaded from `data/minecraft/structures/`. Each template has:

- **Size** (Vec3i)
- **Palette list**: Multiple palettes for random variation
- **Block info list**: `[(pos, BlockState, nbt), ...]`
- **Entity info list**
- **Jigsaw blocks**: Embedded in the palette (detected by being `minecraft:jigsaw`)

### Processor Pipeline

When placing a template, blocks go through this chain:

```
1. BlockIgnoreProcessor(STRUCTURE_BLOCK)
   → Removes structure blocks (they shouldn't appear in world)

2. JigsawReplacementProcessor
   → Converts jigsaw blocks to their final_state (from NBT data)

3. Custom processors (defined in pool element)
   → RuleProcessor: conditional replacement rules
   → BlockAgeProcessor: cracked/mossy stone
   → GravityProcessor: terrain-following (TERRAIN_MATCHING)
   → etc.

4. Projection processors
   → GravityProcessor (for TERRAIN_MATCHING projection)
```

For each block, each processor can:
- Return a modified `StructureBlockInfo` (pass through or replace)
- Return `null` (remove the block)

After all blocks are processed, each processor gets a `finalizeProcessing()` call for post-processing.

### TerrainAdjustment

Structures declare how terrain meets them:

| Type | Effect |
|------|--------|
| `NONE` | No modification |
| `BURY` | Lower structure into terrain |
| `BEARD_THIN` | Stair-step carving around edges |
| `BEARD_BOX` | Box-shaped carving around edges |
| `ENCAPSULATE` | Fully surround in terrain |

The bounding box is inflated by 12 blocks when adaptation != NONE, so the carver knows the affected area.

---

## 17. Blending: Old vs New Chunks

When a world is upgraded from an older version, new terrain must blend seamlessly with existing chunks. The `Blender` class handles this.

### Two Blending Maps

| Map | Range | Data |
|-----|-------|------|
| `heightAndBiomeBlendingData` | ~7 chunks | Height values, biome samples |
| `densityBlendingData` | ~2 chunks | Density values |

### Height Blending

`blendOffsetAndFactor(blockX, blockZ)` returns `(alpha, blendingOffset)`:

```
1. If exact cell match exists in blending data:
     return (alpha=0.0, offset=heightToOffset(height))
   // 0.0 alpha = fully old-chunk

2. Otherwise, inverse-distance-weighted averaging:
     Accumulate weighted heights from all nearby blending data
     alpha = smoothstep(closestDist / (range + 1))
     offset = heightToOffset(averageHeight)
```

The `heightToOffset()` function transforms a height into a noise-density offset, anchoring around Y=128.

### Density Blending

`blendDensity(context, noiseValue)`:

```
If exact cell match exists:
    return that density (scaled by 0.1)
Otherwise:
    alpha = clamp(closestDist / 3.0, 0, 1)
    return lerp(alpha, averageDensity, noiseValue)
```

Only blends within a 3-cell radius of old chunks.

### Biome Blending

`getBiomeResolver(biomeResolver)` returns a wrapper that:
1. Tries to find an old biome at `(quartX, quartY, quartZ)`
2. If found and within range (alpha > 0.5 after shift noise adjustment), overrides the noise biome with the old biome

### Carving Mask Filter

Near old chunks, a `addAroundOldChunksCarvingMaskFilter` prevents carvers from carving through the blending region:

```java
double distanceToCube(x, y, z, radiusX, radiusY, radiusZ):
    return length(max(0, |x|-radiusX), max(0, |y|-radiusY), max(0, |z|-radiusZ));
// Only allow carving where distance < 4.0
```

### BlendingData: Storage

Per-chunk data stored in an L-shaped sampling pattern along chunk borders:

- "Inside" columns along north/west interior border
- "Outside" columns along south/east exterior border
- Heights are persisted; densities and biomes are transient/recalculated

---

## 18. Flat World Generation

### FlatLevelSource

`FlatLevelSource` extends `ChunkGenerator` for superflat worlds.

`fillFromNoise()` simply iterates through the configured layer list placing blocks row-by-row:

```java
for y in minY..minY+height-1:
    for each column:
        set block from layers[y - minY]
```

`buildSurface()` and `applyCarvers()` are no-ops.

### Layer Configuration

A flat world is defined by a list of `FlatLayerInfo(height, Block)`:

```java
// Default "Classic Flat":
Result:
  layer: (1, BEDROCK)    // Y=0
  layer: (2, DIRT)       // Y=1-2
  layer: (1, GRASS_BLOCK) // Y=3
```

10 presets are available:

| Preset | Description | Height | Biomes |
|--------|-------------|--------|--------|
| `CLASSIC_FLAT` | Classic 3-layer | 4 | Plains |
| `TUNNELERS_DREAM` | Full stone layer | 237 | Windswept Hills |
| `WATER_WORLD` | Mostly water | ~165 | Deep Ocean |
| `OVERWORLD` | Normal-like layers | 64 | Plains |
| `SNOWY_KINGDOM` | Snow on top | ~65 | Snowy Plains |
| `BOTTOMLESS_PIT` | Hole to void | 6 | Plains |
| `DESERT` | Sand/Sandstone | 64 | Desert |
| `REDSTONE_READY` | Stone + sandstone | 120 | Desert |
| `THE_VOID` | Nothing (air) | 1 | The Void |

---

## 19. Chunk Storage: Paletted Containers & Region Files

### The PalettedContainer Memory Optimization

Minecraft doesn't store every block as a full 32-bit integer. Instead, it uses a **palette** system that dramatically reduces memory usage.

```
ChunkAccess
  └── LevelChunkSection[] (one per 16-block vertical layer)
        └── PalettedContainer<BlockState> (16×16×16 = 4096 blocks)
              ├── Palette (mapping from ID → actual BlockState)
              └── BitStorage (packed IDs, one per block index)
```

**Palette Types** (chosen automatically based on diversity):

| Palette Type | Bits/Entry | Max Unique Blocks | Implementation |
|-------------|-----------|-------------------|----------------|
| `SingleValuePalette` | 0 | 1 | ZeroBitStorage (uniform section) |
| `LinearPalette` | 1-4 | 2-16 | Array-based, linear scan |
| `HashMapPalette` | 5-8 | 17-256 | Hash-based lookup |
| `GlobalPalette` | 9+ | 257+ | Direct registry ID |

**Palette Resize**: When a palette fills up (`size ≥ 1 << bits`), `onResize()` is called:

```java
Data<T> newData = createOrReuseData(oldData, bits + 1);
newData.copyFrom(oldData.palette, oldData.storage);  // Re-index ALL entries
```

This triggers a complete re-indexing of every entry in the section.

**Index Formula:**
```java
int getIndex(int x, int y, int z) {
    return (y << 4 | z) << 4 | x;   // = (y * 16 + z) * 16 + x
}
```

For biomes (4×4×4 resolution), the same formula uses `bitsPerAxis = 2`.

### DataLayer (Light Storage)

Stores 4-bit-per-value light data (2048 bytes for 4096 blocks):

```java
class DataLayer {
    byte[] data;           // null = uniform (all same value)
    int defaultValue;      // Used when data is null

    int get(int index) {
        if (data == null) return defaultValue;
        int nibble = index & 1;
        return data[index >> 1] >> (4 * nibble) & 0xF;
    }
}
```

The `data == null` optimization saves 2KB for every empty section with no light variation.

### Region File Format (.mca)

Chunks are stored in region files covering 32×32 chunks (512×512 blocks).

```
File structure:
[0x0000]   Sector offset table: 1024 ints (4096 bytes)
           Each int: top 24 bits = sector number (512-byte sectors)
                     bottom 8 bits = sector count
[0x1000]   Timestamp table: 1024 ints (4096 bytes)
           Unix epoch seconds of last write
[0x2000+]  Chunk data (variable, aligned to 4096-byte sectors)

Each chunk's data:
  [4 bytes]   Data length (int)
  [1 byte]    Compression version (1=GZip, 2=Deflate(default), 3=None, 4=LZ4)
  [N bytes]   Compressed NBT data

External chunks (>256 sectors = 1MB):
  Stub in main file: length=1, version | 0x80
  Actual data in c.<x>.<z>.mcc file
```

**NBT Structure of a serialized chunk:**
```nbt
{
  DataVersion: int,
  xPos: int, zPos: int,
  Status: string,
  LastUpdate: long, InhabitedTime: long, isLightOn: boolean,
  sections: [{ Y: byte, block_states: { palette: [...], data: [long...] },
               biomes: { palette: [...], data: [long...] },
               BlockLight: [byte...], SkyLight: [byte...] }...],
  Heightmaps: { MOTION_BLOCKING: [long...], ... },
  structures: { starts: {...}, References: {...} },
  block_entities: [{ id, x, y, z, ...}...],
  entities: [{...}...],
  block_ticks: [...], fluid_ticks: [...],
  PostProcessing: [[short...]...],
  carving_mask: [long...]
}
```

### I/O Pipeline

```
Writing:
  ChunkAccess → SerializableChunkData.copyOf()
    → Process sections, heightmaps, structures, entities
    → Produce CompoundTag with DataVersion
  → IOWorker.store()
    → Queue on consecutive executor → RegionFileStorage.write()
      → RegionFile: allocate sectors, write compressed NBT, update header

Reading:
  IOWorker.loadAsync()
    → RegionFileStorage.read() → RegionFile: read header, decompress
    → DataFixer upgrade if version mismatch
    → SerializableChunkData.parse()
      → Deserialize sections, light data, structure data
      → Create ProtoChunk or LevelChunk (depending on status)
```

### Section Count Optimization

```java
// LevelChunkSection hasOnlyAir():
return nonEmptyBlockCount == 0;
// If true, getBlockState() returns AIR constant without touching palette
```

When a chunk is first generated, most sections are all-air (especially in the sky and deep underground). These sections use `SingleValuePalette` with `ZeroBitStorage` and `DataLayer(data=null)` — effectively zero memory for block storage.

---

## 20. Lighting After Generation

### The Light Engine

Two independent engines: `SkyLightEngine` and `BlockLightEngine`, each managing `DataLayer` arrays per section.

### Initialization

```java
// INITIALIZE_LIGHT stage:
chunk.initializeLightSources()  // Find lava, glowstone, etc.

// LIGHT stage:
lightEngine.lightChunk(chunk, wasLightCorrect)
```

`lightChunk()` runs asynchronously:

1. **Sky light sources**: For each column, `ChunkSkyLightSources` records the lowest Y where sky light enters (the surface). All sections above that Y get skylight = 15.
2. **Propagation**: Both engines run `DynamicGraphMinFixedPoint` — a priority-queue-based solver that processes decrease/increase queues until convergence.
3. **Per-step attenuation**: Light decreases by `getOpacity(block)` per block (minimum 1). Voxel shape occlusion may block light entirely (returning opacity = 16).

### Empty Section Propagation

Sky light passing through empty sections is handled efficiently:

```java
SkyLightEngine.propagateFromEmptySections():
    // Copy sky light level down through all contiguous empty sections at once
    // Avoids iterating 4096 blocks per empty section
```

### Block Light

Any block with `getLightEmission() > 0` (lava=15, glowstone=15, torches=14, etc.) emits block light. The engine propagates outward with the same attenuation rules.

### Convergence

The `DynamicGraphMinFixedPoint` dequeues nodes in priority order (lowest light = highest urgency). Each node checks its neighbors and re-queues them if their computed light level would change. This continues until all queues are empty.

---

## 21. Biome Colors: How Terrain Gets Its Look

### Grass Color

```java
// GrassColor.java
static final int[] pixels = new int[65536];  // 256×256 gradient

static int get(double temperature, double rainfall) {
    return ColorMapColorUtil.get(temperature, rainfall, pixels, MAGENTA_DEFAULT);
}
```

### Foliage Color

```java
// FoliageColor.java
static final int[] pixels = new int[65536];

static int get(double temperature, double rainfall) {
    return ColorMapColorUtil.get(temperature, rainfall, pixels, DEFAULT_GREEN);
}

// Pre-defined colors for specific biomes:
EVERGREEN = -10380959  // Dark green (taiga)
BIRCH = -8345771       // Yellow-green (birch forest)
DEFAULT = -12012264    // Standard green
MANGROVE = -7158200    // Muddy green
```

### The Core Lookup

```java
// ColorMapColorUtil.java
static int get(double temp, double rain, int[] pixels, int defaultColor) {
    rain *= temp;                       // Cold biomes: reduced effective rainfall
    int x = (int)((1.0 - temp) * 255);  // Hot on right
    int y = (int)((1.0 - rain) * 255);  // Wet at top
    int index = y << 8 | x;
    return index < pixels.length ? pixels[index] : defaultColor;
}
```

**Key insight**: `rain *= temp` means cold biomes have muted, desaturated colors even if their downfall is high.

### Grass Color Modifiers

| Modifier | Effect |
|----------|--------|
| `NONE` | Identity (no change) |
| `DARK_FOREST` | Average with dark green: `(color & 0xFEFEFE + 2634762) >> 1` |
| `SWAMP` | Perlin noise at scale 0.0225: above -0.1→dark green, below→pale green |

### Temperature Height Adjustment

Temperature decreases above sea level + 17:

```java
// Biome.java
float getTemperature(BlockPos pos, int seaLevel) {
    float temp = climateSettings.temperature;
    if (pos.getY() > seaLevel + 17) {
        float heightFactor = (float)(noise.getValue(...) * 0.05 + 0.025);
        temp -= heightFactor * (pos.getY() - (seaLevel + 17)) / 150.0f;
    }
    return temp;
}
```

Plus the `FROZEN` modifier uses multi-octave Perlin noise to create ice patches in partially frozen biomes.

---

## 22. The Nether & End Dimensions

### Nether Generation

| Parameter | Value |
|-----------|-------|
| Noise settings | 0 minY, 128 height, 1×2 cells |
| Sea level | 32 |
| Default block | Netherrack |
| Default fluid | Lava |
| Aquifers | Disabled |
| Ore veins | Disabled |
| Noise router | Simplified: netherrack + lava + gravel |

Nether router uses `BASE_3D_NOISE_NETHER` with steeper Y scaling (0.375 vs 0.125). No terrain splines — just simple noise with slides.

Biome source: Nether-specific `MultiNoiseBiomeSourceParameterList` with 5 biomes (Nether Wastes, Soul Sand Valley, Crimson Forest, Warped Forest, Basalt Deltas) mapped by temperature and humidity only.

### End Generation

| Parameter | Value |
|-----------|-------|
| Noise settings | 0 minY, 128 height, 2×1 cells |
| Sea level | 0 |
| Default block | End stone |
| Aquifers | Disabled |
| Ore veins | Disabled |
| Noise router | EndIslandDensityFunction + BASE_3D_NOISE_END |

The End uses `EndIslandDensityFunction` — a Simplex-based system:

```java
// For each 2×2 cell, check island noise < -0.9
// If so, compute island height based on distance from center
float doffs = 100 - sqrt(x² + z²) * 8;  // Base distance falloff
// For each nearby island center:
float islandSize = (|chunkX| * 3439 + |chunkZ| * 147) % 13 + 9;
doffs = max(doffs, 100 - sqrt(xd² + zd²) * islandSize);
```

Biome source: `TheEndBiomeSource` with 5 biomes (The End, End Highlands, End Midlands, End Barrens, Small End Islands) determined by distance from origin and erosion noise.

---

## 23. Performance Architecture: Caching & Async

### NoiseChunk Caching Layers

Each density function is wrapped with an appropriate caching strategy:

```
Function evaluated once per block (16×384×16 = 98,304 evaluations)
  │
  ├── Interpolated: sampled at 5×49×5 = 1,225 cell corners
  │     └── Trilinearly interpolated: 98,304 simple lerps
  │
  ├── FlatCache: pre-computed 2D array (e.g., 17×17 = 289 values)
  │     └── Reused for all 384 Y levels → 289 vs 98,304 evaluations
  │
  ├── CacheOnce: reset per interpolation round
  │     └── Same value used for all blocks in one cell iteration
  │
  └── CacheAllInCell: 128 values per cell
        └── Filled once when cell is selected
```

### Async Pipeline

Several stages run asynchronously:

| Stage | Executor | Reason |
|-------|----------|--------|
| Biomes | Background executor | Noise evaluation, no block writes |
| Noise (`fillFromNoise`) | Background executor | Heavy computation, block writes to ProtoChunk |
| Light | Light engine thread pool | Propagation queues |
| FULL | Main thread | LevelChunk promotion, entity registration |

### The Cell Pipeline

NoiseChunk processes cells in a **double-buffered pipeline**:

```
advanceCellX(0) → fill slice1 at X=firstCellX+1
  │
  ├── selectCellYZ(47, 0) → fill CacheAllInCell for cell (firstCellX+1, 0, 47)
  │     └── Interpolate all 4×8×4 blocks in cell
  │
  ├── selectCellYZ(46, 0) → ...
  ├── ...
  │
  └── selectCellYZ(0, 3) → last cell in this X column

swapSlices() → slice0 = slice1 (previous next becomes current)

advanceCellX(1) → fill slice1 at X=firstCellX+2
  │
  └── process all YZ cells in column X=1
```

This is why the iteration order in `doFill()` is Z-outermost but the advance is X-driven: the interpolator needs all Z cells at a given X before advancing to the next X.

### Weighted Random Cave Count

```java
caveCount = random.nextInt(random.nextInt(random.nextInt(15) + 1) + 1);
```

This triple-nested random produces a heavily skewed distribution toward small values (average ~3.5), preventing every chunk from having maximum cave complexity.

---

## 24. Configuration: JSON Data-Driven System

### The NoiseRouter as JSON

The entire density function pipeline is serialized as JSON in data packs:

```json
{
  "noise_router": {
    "barrier": { "type": "noise", "noise": "minecraft:aquifer_barrier", "xz_scale": 0.5, "y_scale": 0.5 },
    "fluid_level_floodedness": { "type": "noise", "noise": "minecraft:aquifer_fluid_level_floodedness", "xz_scale": 0.67, "y_scale": 0.67 },
    "fluid_level_spread": { "type": "noise", "noise": "minecraft:aquifer_fluid_level_spread", "xz_scale": 0.714, "y_scale": 0.714 },
    "lava": { "type": "noise", "noise": "minecraft:aquifer_lava" },
    "temperature": { "type": "shifted_noise", ... },
    "vegetation": { "type": "shifted_noise", ... },
    "continents": { "type": "shifted_noise", ... },
    "erosion": { "type": "shifted_noise", ... },
    "depth": { "type": "add", "argument1": { "type": "y_clamped_gradient", ... }, "argument2": { "type": "spline", ... } },
    "ridges": { "type": "shifted_noise", ... },
    "preliminary_surface_level": { "type": "find_top_surface", ... },
    "final_density": { "type": "min", ... },  ← complex DAG
    "vein_toggle": { "type": "noise", ... },
    "vein_ridged": { "type": "noise", ... },
    "vein_gap": { "type": "noise", ... }
  }
}
```

### Surface Rules as JSON

```json
{
  "surface_rule": {
    "type": "sequence",
    "sequence": [
      {
        "type": "condition",
        "if_true": {
          "type": "biome",
          "biomes": ["minecraft:plains", "minecraft:forest"]
        },
        "then_run": {
          "type": "block",
          "result_state": "minecraft:grass_block"
        }
      },
      {
        "type": "condition",
        "if_true": {
          "type": "stone_depth",
          "offset": 0,
          "add_surface_depth": true,
          "surface_type": "floor"
        },
        "then_run": {
          "type": "block",
          "result_state": "minecraft:dirt"
        }
      },
      { "type": "block", "result_state": "minecraft:stone" }
    ]
  }
}
```

### Noise Settings as JSON

```json
{
  "type": "minecraft:overworld",
  "generator": {
    "type": "minecraft:noise",
    "settings": "minecraft:overworld",
    "biome_source": {
      "type": "minecraft:multi_noise",
      "preset": "minecraft:overworld"
    }
  }
}
```

### The Codec System

Every density function, surface rule, and configuration class defines a `MapCodec` for JSON serialization:

```java
// Example: DensityFunctions.Constant
public static final MapCodec<Constant> CODEC = Codec.DOUBLE.fieldOf("value")
    .xmap(Constant::new, Constant::value);
```

The dispatch system uses `BuiltInRegistries`:

```java
public static final Codec<DensityFunction> CODEC = BuiltInRegistries.DENSITY_FUNCTION_TYPE
    .byNameCodec().dispatch(type -> type.codec(), Function.identity());
```

This means adding a new density function type is as simple as registering a new type with its codec in the registry.

---

## Summary: The Full Data Flow for One Block

At position `(blockX, blockY, blockZ)` within a chunk being generated:

```
1. NoiseChunk.getInterpolatedState():
   │
   ├── fullNoiseDensity.compute(context)
   │     ├── finalDensity (from NoiseRouter)
   │     │     ├── slopedCheese = 4 * quarterNegative((depth + jaggedness) * factor)
   │     │     │                     + base_3d_noise
   │     │     ├── caves = rangeChoice(slopedCheese, SURFACE_THRESHOLD, ...)
   │     │     │     └── min(postProcess(slide(caves)), noodle)
   │     │     └── beardifier.compute() (structure adjustment)
   │     │
   │     ├── Triliner interpolation from cell corners
   │     │     (pre-sampled at 5×49×5 grid, stored in NoiseInterpolator slices)
   │     │
   │     └── blendDensity() (if near old chunks)
   │
   ├── aquifer.computeSubstance(density)
   │     ├── Find nearest 4 aquifer cell centers
   │     ├── Compute barrier pressure between cells
   │     ├── If density + barrierPressure > 0: return null (solid)
   │     └── Else: return fluid state (air/water/lava)
   │
   ├── oreVeinifier.calculate(density)
   │     ├── veinToggle → copper or iron
   │     ├── veininess, edge check, solidness test, ridge test
   │     └── Return ore/filler block or null
   │
   └── If all returned null: defaultBlock (stone/deepslate)

2. If density > 0: block is solid (default or aquifer-barrier)
   If density ≤ 0: block is fluid (water/lava) or air

3. Later: SurfaceRules replace top layers with grass/sand/dirt
4. Later: Carvers remove blocks to create caves/ravines
5. Later: Features place ores, trees, structures
6. Later: Lighting propagates sky + block light
```

The system is entirely **data-driven** and **composably functional**: every aspect of terrain shape is the result of composing ~25 density function types into a directed acyclic graph, then evaluating that graph through efficient caching and interpolation layers.
