// Whether a world's first upload piece names the world:
// cargo run --release -p aw-store --example first_piece -- <world dir> <private dir>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (dir, private) = (std::path::Path::new(&args[1]), std::path::Path::new(&args[2]));
    #[derive(serde::Deserialize)]
    struct Descriptor { public: aw_crypto::WorldPublic }
    let d: Descriptor = serde_json::from_slice(&std::fs::read(dir.join("world.json")).unwrap()).unwrap();
    let j = aw_store::journal::Journal::open(dir, private, &d.public).unwrap();
    let order = j.in_order(&j.inventory());
    let first: Vec<String> = order.iter().take(2000).filter_map(|id| j.raw(id)).map(|r| String::from_utf8_lossy(&r).to_string()).collect();
    println!("{} records; first piece names the world: {:?}", order.len(), aw_store::journal::world_of(&first, d.public.world_id.as_str()).map(|_| "yes"));
}
