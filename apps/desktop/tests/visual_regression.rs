#![allow(missing_docs)]

use slint::ComponentHandle;
use slint_snapshot::{
    SnapshotRuntime,
    testing::{SnapshotAssertion, SnapshotMode},
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
    SnapshotAssertion::try_new("main-window/default", frame)?
        .mode(mode)
        .assert_match();
    Ok(())
}
