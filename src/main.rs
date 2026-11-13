#[cfg(target_os = "macos")]
mod macos;

const APP_NAME: &str = "ADHD，闭嘴！";

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("{APP_NAME} 需要 macOS。");
}
