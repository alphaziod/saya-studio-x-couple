#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = studio_core::browser_host::try_run() { std::process::exit(code); }
    saya_studio::run();
}
