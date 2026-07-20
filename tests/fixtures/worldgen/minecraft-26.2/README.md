# Minecraft 26.2 worldgen oracle

This directory is reserved for certified Java 26.2 worldgen fixtures. The
official-server bootstrap has been implemented, but it is not yet permitted to
commit a corpus from it: repeated server-lifecycle captures of identical seeds
and chunks produced different decoded block-state hashes even after excluding
save-time, light, and scheduled-work data. That means this boundary is not yet
an oracle for regression use.

The bootstrap freezes the server before force-loading oracle chunks and
advances exactly 200 ticks with `/tick step`; this removes wall-clock timing
from the capture protocol. If repeated captures still differ, the next task is
an in-process Java exporter that invokes each worldgen stage from a controlled
`RandomState`/chunk-status context. Only after two exports produce identical
stage and final-block outputs may those results be committed here.

`tools/worldgen_oracle/run_in_process.sh` is the deterministic replacement
for the first stage: it bootstraps the 26.2 registries and evaluates the
Overworld `RandomState` directly, without a level, network listener, chunk
worker, game tick, or save file. Its density and preliminary-surface output is
the first fixture source; extend this program with biome, aquifer, carver, and
feature probes before accepting corresponding Rust parity claims.

Regenerate only when intentionally updating the pinned reference, with:

```sh
MINECRAFT_26_2_SERVER_JAR=/path/to/server-26.2.jar \
  tools/worldgen_oracle/generate.sh
```

Use `WORLDGEN_ORACLE_SEED` and a semicolon-separated
`WORLDGEN_ORACLE_CHUNKS` list (for example `7 11;8 11`) for a focused local
comparison. The exporter reports final top-height/material summaries alongside
the decoded block-state hash.

The provisional server capture uses seed 42 and positive/negative chunks
outside spawn preparation; it verifies the JAR, seed, and NBT export path but
is deliberately not checked in as a baseline.

The next oracle extension must expose Java stage data (biome quart, density,
aquifer decision, carver mask, and feature/structure placement). Do not treat
these final chunks as sufficient evidence for a stage-level parity claim.
