// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = dnd_desktop_lib::run() {
        eprintln!("DndStudio failed to start: {e}");
        std::process::exit(1);
    }
}
