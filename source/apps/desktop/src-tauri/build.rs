fn main() {
    // The interface is built into the binary. Without this a binary built
    // after a new interface would keep the old one — the frontend is no Rust
    // source, so cargo would not notice.
    println!("cargo:rerun-if-changed=../../../build/frontend");
    // The server is built without the app's window: nothing of Tauri's to prepare.
    if std::env::var_os("CARGO_FEATURE_GUI").is_some() {
        tauri_build::build()
    }
}
