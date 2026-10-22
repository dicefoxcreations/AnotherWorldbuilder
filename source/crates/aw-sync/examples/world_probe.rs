//! Tries joining a world across two real network stacks — for checking the
//! way through firewalls by hand (see docs/testing/lan-join.md).
//!
//! `world_probe host`   offers a world and waits.
//! `world_probe closed` is found, but does not answer knocks: with a
//!                      firewall in between the player must time out.
//! `world_probe player` finds the world, asks to join four times, prints the answers.

use aw_crypto::{Identity, IdentityPublic};
use aw_sync::world::{Ask, Host, Offer, Reply, WorldNet};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

struct Probe {
    offering: AtomicBool,
}

impl Host for Probe {
    fn offers(&self) -> Vec<Offer> {
        if !self.offering.load(Ordering::Relaxed) {
            return Vec::new();
        }
        vec![Offer { world: "world:probe".into(), name: "Probe".into(), icon: "🧪".into(), gm: "Probe".into() }]
    }
    fn answer(&self, _: &str, who: &IdentityPublic, _: Ask) -> Reply {
        println!("host: {} asked", who.name);
        Reply::Requested
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let probe = Arc::new(Probe { offering: AtomicBool::new(mode != "player") });
    let net = WorldNet::start(probe.clone(), aw_sync::world::Reach { lan: true, ..Default::default() }).await?;
    match mode.as_str() {
        "host" | "closed" => {
            if mode == "closed" {
                // Announced already; from now on nothing is offered, so knocks go unanswered.
                probe.offering.store(false, Ordering::Relaxed);
            }
            println!("{mode}: ready");
            tokio::time::sleep(Duration::from_secs(40)).await;
        }
        "player" => {
            let found = tokio::time::timeout(Duration::from_secs(20), async {
                while !net.found().iter().any(|o| o.world == "world:probe") {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await;
            if found.is_err() {
                println!("player: not found");
                std::process::exit(2);
            }
            println!("player: found");
            let me = Identity::create("Player")?;
            // The first ask knocks; those shortly after come through without.
            for round in 0..4 {
                if round > 0 {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                match net.ask("world:probe", &me, Ask::Join).await {
                    Ok(reply) => println!("player: {reply:?}"),
                    Err(e) => {
                        println!("player: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        _ => return Err("world_probe host|closed|player".into()),
    }
    Ok(())
}
