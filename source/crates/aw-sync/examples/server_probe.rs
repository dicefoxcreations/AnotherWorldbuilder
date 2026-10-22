//! Asks a server for its worlds, as the app does — for checking by hand
//! whether a server is reached (see docs/testing/server.md).
//!
//! `server_probe <address or code> [times]` looks the server up, then asks
//! it `times` times (default 3) and prints how each went. `AW_LOG=1`: how
//! the connection goes.

use aw_crypto::Identity;
use aw_sync::world::{Ask, Server, WorldNet};
use std::{sync::Arc, time::Instant};

struct Nothing;

impl aw_sync::world::Host for Nothing {
    fn offers(&self) -> Vec<aw_sync::world::Offer> {
        Vec::new()
    }
    fn answer(&self, _: &str, _: &aw_crypto::IdentityPublic, _: Ask) -> aw_sync::world::Reply {
        aw_sync::world::Reply::Requested
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if std::env::var("AW_LOG").is_ok_and(|v| v == "1") {
        let filter = tracing_subscriber::EnvFilter::new(&std::env::var("AW_LOG_FILTER").unwrap_or_else(|_| "warn,iroh=debug,noq=debug,noq_proto=debug,aw_sync=debug".into()));
        let _ = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).try_init();
    }
    let mut args = std::env::args().skip(1);
    let Some(address) = args.next() else { return Err("server_probe <address or code> [times]".into()) };
    let times: u32 = args.next().and_then(|t| t.parse().ok()).unwrap_or(3);
    let server = Server::look_up(&address).await?;
    println!("code: {}", server.code());
    let net = WorldNet::start(Arc::new(Nothing), Default::default()).await?;
    let me = Identity::create("Probe")?;
    for round in 1..=times {
        let at = Instant::now();
        match net.ask_server(&server, "", &me, Ask::Worlds).await {
            Ok(reply) => println!("{round}: answered in {:?}: {reply:?}", at.elapsed()),
            Err(e) => println!("{round}: no answer after {:?}: {e}", at.elapsed()),
        }
    }
    Ok(())
}
