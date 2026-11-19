#![forbid(unsafe_code)]
#![allow(missing_docs)]

slint::include_modules!();

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

    window.run()
}
