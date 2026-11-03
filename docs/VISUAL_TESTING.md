# Visual regression testing

Framelet uses headless Slint rendering through slint-snapshot, without a physical display server.

## Coverage

- Fixed 760 × 560 logical-pixel main-window screenshot.
- Deterministic software rendering.
- Checked-in PNG reference image and pixel comparison.
- Expected, actual, and diff artifacts when comparison fails.

## Checked-in reference image

The initial 760 × 560 PNG was rendered from the compiled Slint UI and reviewed. It is stored at:

`apps/desktop/tests/snapshots/main-window/default.png`

CI runs the visual test explicitly with `FRAMELET_RUN_VISUAL_TESTS=1` and
`SLINT_SNAPSHOT_MODE=verify`. It never creates or accepts a new baseline automatically.

## Normal verification

```bash
FRAMELET_RUN_VISUAL_TESTS=1 cargo test -p framelet-desktop --test visual_regression
```

Inspect artifacts under target/slint-snapshots/. For intentional UI changes, regenerate explicitly
with SLINT_SNAPSHOT_MODE=accept, review the PNG, and commit the reference.

Keep dimensions, scale factor, renderer, fonts, and embedded resources fixed. Add separate
snapshots for selected modes, camera preview, recording, paused, and error states.

For intentional UI changes, render a candidate, inspect it, then explicitly accept it with
`SLINT_SNAPSHOT_MODE=accept`. Never accept a changed reference without reviewing the PNG.
