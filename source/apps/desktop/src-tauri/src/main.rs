// No console window on Windows in the release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--server`: AW Server, the dedicated server with its symbol in the tray.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--server") {
        aw_desktop_lib::run_server(args.iter().any(|a| a == "--hidden"));
    } else {
        aw_desktop_lib::run()
    }
}
