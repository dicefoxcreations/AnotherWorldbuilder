//! A made-up player for testing by hand: writes the public card of a fresh
//! identity, as *Players → From file* takes it (see docs/testing/players.md).
//!
//! `player_card <name> <file.awid>` — the secret keys are not kept.

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(name), Some(path)) = (args.next(), args.next()) else {
        eprintln!("player_card <name> <file.awid>");
        std::process::exit(2);
    };
    let identity = aw_crypto::Identity::create(&name).expect("an identity");
    std::fs::write(&path, serde_json::to_vec_pretty(&identity.public()).expect("its card")).expect("written");
    println!("{} {}", identity.id(), path);
}
