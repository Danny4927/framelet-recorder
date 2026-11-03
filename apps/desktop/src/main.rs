#![deny(unsafe_code)]
#![allow(missing_docs)]

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    window.run()
}
