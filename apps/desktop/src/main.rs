#![forbid(unsafe_code)]
slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    window.run()
}
