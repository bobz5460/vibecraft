# Renderer Manual Check

Use this fixed launch scene after terrain, lighting, shader, atlas, or camera changes:

```sh
VIBECRAFT_ASSETS=/tmp/opencode/minecraft-assets cargo run --release -- --seed 1592639710 --world-dir worlds/render-check --render-distance 6 --graphics regular
```

The player starts at `(0, 75, 0)` facing the generated center chunk at noon. Wait for the center terrain and its immediate neighbors to finish streaming, then use `F2` to capture a screenshot. Compare the image with the accepted baseline in `screenshots/render_vanilla_baseline.png` for opaque/cutout/translucent terrain, fog, shadows, GUI text, and block highlight behavior.

Repeat with `--graphics vibrant`, resize the window once, and capture a second screenshot. The world directory is created by startup but world persistence is intentionally deferred to M2.
