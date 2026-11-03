# Visual regression testing

Framelet uses headless Slint rendering through slint-snapshot, without a physical display server.

## Coverage

- Fixed 760 × 560 logical-pixel main-window screenshot.
- Deterministic software rendering.
- Checked-in PNG reference image and pixel comparison.
- Expected, actual, and diff artifacts when comparison fails.

## First reference image

Generate the initial baseline from the compiled UI and review it:

```bash
SLINT_SNAPSHOT_MODE=accept cargo test -p framelet-desktop --test visual_regression
```

Review the PNG under apps/desktop/tests/snapshots/ and commit it. Do not accept references
automatically in CI.

## Normal verification

```bash
cargo test -p framelet-desktop --test visual_regression
```

Inspect artifacts under target/slint-snapshots/. For intentional UI changes, regenerate explicitly
with SLINT_SNAPSHOT_MODE=accept, review the PNG, and commit the reference.

Keep dimensions, scale factor, renderer, fonts, and embedded resources fixed. Add separate
snapshots for selected modes, camera preview, recording, paused, and error states.

The first PNG is intentionally not fabricated; it must be rendered from the compiled UI and reviewed.
