// Times opening a journal: cargo run --release -p aw-store --example open_time -- <world dir> <private dir>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (dir, private) = (std::path::Path::new(&args[1]), std::path::Path::new(&args[2]));
    #[derive(serde::Deserialize)]
    struct Descriptor { public: aw_crypto::WorldPublic }
    let d: Descriptor = serde_json::from_slice(&std::fs::read(dir.join("world.json")).unwrap()).unwrap();
    let t = std::time::Instant::now();
    let j = aw_store::journal::Journal::open(dir, private, &d.public, aw_store::journal::Mode::Shared).unwrap();
    println!("open {:?}, {} records, {} articles", t.elapsed(), j.inventory().len(), j.article_count());
}
