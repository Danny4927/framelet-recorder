#![allow(missing_docs)]

use std::path::Path;

use slint::ComponentHandle;
use slint_snapshot::{
    SnapshotRuntime,
    testing::{SnapshotAssertion, SnapshotMode, SnapshotStore},
};
slint::include_modules!();

#[test]
fn main_window_default_state_matches_reference() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("FRAMELET_RUN_VISUAL_TESTS").is_none() {
        return Ok(());
    }

    let runtime = SnapshotRuntime::new()?;
    let window = MainWindow::new()?;
    runtime.set_size(window.window(), (760, 560), 1.0)?;
    let frame = runtime.render(window.window())?;
    let mode = SnapshotMode::from_env()?.unwrap_or_default();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let store = SnapshotStore::new(
        manifest_dir.join("tests/snapshots"),
        manifest_dir.join("../../target/slint-snapshots"),
    );

    SnapshotAssertion::try_new("main-window/default", frame)?
        .store(store)
        .mode(mode)
        .assert_match();
    Ok(())
}
