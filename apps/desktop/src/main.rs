#![deny(unsafe_code)]
#![allow(missing_docs)]

slint::include_modules!();

use std::path::PathBuf;

use framelet_storage::FileSystemRecordingStorage;

const DEFAULT_OUTPUT_DIRECTORY: &str = "Framelet";

fn default_output_directory() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_VIDEOS_DIR").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Videos")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(DEFAULT_OUTPUT_DIRECTORY)
}

fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(windows)]
fn apply_native_title_bar(window: &MainWindow) {
    use std::time::Duration;

    use slint::winit_030::{WinitWindowAccessor, winit};
    use winit::platform::windows::{Color, WindowExtWindows};

    let window_weak = window.as_weak();

    // Winit exposes the native window only while its event loop is active.
    // Schedule the styling after the first window has been created.
    slint::Timer::single_shot(Duration::from_millis(100), move || {
        if let Some(window) = window_weak.upgrade() {
            let _ = window.window().with_winit_window(|native_window| {
                native_window.set_title_background_color(Some(Color::from_rgb(16, 20, 28)));
                native_window.set_title_text_color(Color::from_rgb(243, 244, 246));
                native_window.set_border_color(Some(Color::from_rgb(16, 20, 28)));
            });
        }
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;

    #[cfg(windows)]
    apply_native_title_bar(&window);

    let output_directory = default_output_directory();
    match FileSystemRecordingStorage::new(output_directory.clone()) {
        Ok(storage) => {
            window.set_output_directory(display_path(&output_directory).into());
            let count = storage.find_orphaned_temporary_files().map(|files| files.len()).unwrap_or(0);
            window.set_recovery_message(format!("{count} temporäre Datei(en) gefunden").into());
            if count > 0 {
                window.set_status_message("Unterbrochene Dateien werden nicht automatisch gelöscht.".into());
            } else {
                window.set_status_message("Speicherort bereit; Aufnahme-Backend noch nicht verfügbar.".into());
            }
        }
        Err(error) => {
            window.set_status_message(format!("Speicherort nicht verfügbar: {error}").into());
            window.set_recovery_message("Prüfung nicht möglich".into());
        }
    }

    {
        let directory = output_directory.clone();
        let weak = window.as_weak();
        window.on_scan_recovery(move || {
            if let Some(window) = weak.upgrade() {
                match FileSystemRecordingStorage::new(directory.clone())
                    .and_then(|storage| storage.find_orphaned_temporary_files())
                {
                    Ok(files) => {
                        window.set_recovery_message(format!("{} temporäre Datei(en) gefunden", files.len()).into());
                        window.set_status_message(if files.is_empty() {
                            "Keine verwaisten temporären Dateien gefunden.".into()
                        } else {
                            "Dateien bleiben unverändert; keine automatische Löschung.".into()
                        });
                    }
                    Err(error) => {
                        window.set_recovery_message("Prüfung fehlgeschlagen".into());
                        window.set_status_message(format!("Recovery-Prüfung fehlgeschlagen: {error}").into());
                    }
                }
            }
        });
    }

    window.run()
}
