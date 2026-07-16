# Spec: worldgen-target

Scope: repo

# World Generation Target

- Authoritative reference: `/Users/dac63/Downloads/minecraft-26/net/minecraft/world` and the corresponding `net/minecraft/world/level/levelgen` source beneath it.
- Target scope for this work: Java Edition `minecraft-26` Overworld generation only.
- Required parity boundary: seed-derived noise/random initialization, dimension bounds and sea level, biome source, density/noise router, aquifers, caves/carvers, surface rules, ore/feature placement, structure placement, and deterministic chunk results independent of generation order where the reference is deterministic.
- The Rust engine's native chunk storage and renderer contracts remain in force; Java save/protocol compatibility is not implied.
- Do not claim exact parity for a subsystem until its reference algorithm, seed plumbing, coordinate conventions, and observable fixtures are implemented and verified.
- If the current block registry cannot represent a reference result, record the gap and add the smallest explicit compatibility boundary rather than silently substituting another block.
- Non-goals for this slice: Nether, End, Java Anvil/NBT compatibility, full data-pack loading, and unrelated gameplay systems.