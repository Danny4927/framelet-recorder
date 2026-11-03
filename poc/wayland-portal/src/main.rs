#![forbid(unsafe_code)]

//! Opt-in smoke test for the XDG Desktop Portal session lifecycle.
//! This deliberately does not start continuous frame capture yet.

use lamco_wayland::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !std::env::args().any(|argument| argument == "--start") {
        eprintln!("This PoC opens the desktop screen-sharing permission dialog.");
        eprintln!("Run it on a Wayland desktop with: cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start");
        return Ok(());
    }

    let manager = PortalManager::with_default().await?;
    let session = manager
        .create_session("framelet-wayland-poc".to_owned(), None)
        .await?;

    println!("Portal session created successfully.");
    println!("Selected PipeWire streams: {}", session.streams().len());
    println!("No frame data is read or saved by this smoke test.");

    // Dropping the session exercises the library's RAII cleanup path.
    drop(session);
    println!("Portal session released.");

    Ok(())
}
