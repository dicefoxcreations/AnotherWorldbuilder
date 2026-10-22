// No console window on Windows in the release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    aw_desktop_lib::run()
}
