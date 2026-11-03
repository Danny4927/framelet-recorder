//! Build script that compiles the Framelet Slint user interface.

fn main() {
    slint_build::compile("ui/main.slint").expect("Slint UI compilation failed");
}
