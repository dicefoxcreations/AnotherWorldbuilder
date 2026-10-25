//! aw-server — the door keeper of a group's worlds: on a NAS, a small
//! server, in a container. Always on, so players work when the GM's device
//! is off.
//!
//! It holds no world's keys. It knows who is who — everyone proves their
//! identity when they connect — and lets each one in where the world itself
//! says they belong: its GM, and the members the GM granted. What it passes
//! on of an encrypted world only the members' own keys open; their apps keep
//! it while they are online where the GM keeps players from copies. An
//! unencrypted world it reads, as anyone holding it could, and lets each
//! member see and change what their view and rights say (see `host`) —
//! writing in their name, as itself, which the GM vouches for
//! (`Item::Hosts`).
//!
//! Its admin, a GM named when it starts (`--admin`), manages it from the
//! app: which GMs may bring worlds, which worlds are closed, who is turned
//! away. Of other GMs' worlds the admin sees name, size and form — no more
//! than the server itself can read.
//!
//! ```text
//! <data>/profile/      the server's own identity and endpoint key
//! <data>/worlds/       the worlds, as in the app
//! <data>/server.json   GMs, closed worlds, who came, who is turned away
//! <data>/host-code.txt what GMs and players enter
//! ```

use super::*;
use aw_sync::world::{Reach, Server, WorldNet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::keeper::{is_id, Keeper};

const USAGE: &str = "aw-server — keeps the worlds of AnotherWorldbuilder for groups

  --admin <id>        the GM who manages it: their id, id:…  (AW_ADMIN)
  --data <dir>        where everything lies                  (AW_DATA, /data)
  --port <port>       port to answer on: UDP, and TCP for its code
                                                             (AW_PORT, 7447)
  --name <name>       how it is called                       (AW_NAME, aw-server)
  --address <h:p>     how it is reached, for its code; again for more
                                                             (AW_ADDRESS, comma-separated)
  --lan               be found on the local network          (AW_LAN=1)
  how connections go, for finding faults                     (AW_LOG=1)
";


struct Options {
    admin: String,
    data: PathBuf,
    port: u16,
    name: String,
    addresses: Vec<String>,
    lan: bool,
}

fn options() -> Result<Options, String> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let mut o = Options {
        admin: env("AW_ADMIN").unwrap_or_default().trim().to_string(),
        data: env("AW_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/data")),
        port: env("AW_PORT").map(|p| p.trim().parse().map_err(|_| "AW_PORT is no port")).transpose()?.unwrap_or(7447),
        name: env("AW_NAME").unwrap_or_else(|| "aw-server".into()),
        addresses: env("AW_ADDRESS").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
        lan: env("AW_LAN").is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes")),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--admin" => o.admin = value()?.trim().to_string(),
            "--data" => o.data = PathBuf::from(value()?),
            "--port" => o.port = value()?.parse().map_err(|_| "--port needs a port")?,
            "--name" => o.name = value()?,
            "--address" => o.addresses.push(value()?),
            "--lan" => o.lan = true,
            "-h" | "--help" => return Err(USAGE.into()),
            _ => return Err(format!("unknown: {arg}\n\n{USAGE}")),
        }
    }
    // The 16 characters alone are just as clear.
    if !o.admin.starts_with("id:") {
        o.admin = format!("id:{}", o.admin);
    }
    o.admin = o.admin.to_ascii_lowercase();
    if !is_id(&o.admin) {
        return Err(format!("Name the admin by their id — id: and 16 characters, under Profile in the app.\n\n{USAGE}"));
    }
    Ok(o)
}


pub fn main() {
    let o = match options() {
        Ok(o) => o,
        Err(text) => {
            eprintln!("{text}");
            std::process::exit(2);
        }
    };
    // AW_LOG=1: how connections go; anything else is a filter of its own.
    if let Some(log) = std::env::var("AW_LOG").ok().filter(|v| !v.trim().is_empty() && v != "0") {
        let filter = if log == "1" { "warn,iroh=debug,noq=debug,noq_proto=debug,aw_sync=debug".to_string() } else { log };
        let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::new(filter)).with_ansi(false).try_init();
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("aw-server: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = runtime.block_on(run(o)) {
        eprintln!("aw-server: {}", e.0);
        std::process::exit(1);
    }
}

async fn run(o: Options) -> Answer<()> {
    let (profile, worlds) = (o.data.join("profile"), o.data.join("worlds"));
    store::keep_worlds_alone();
    std::fs::create_dir_all(&profile)?;
    std::fs::create_dir_all(&worlds)?;
    let store = profile::use_folders(&profile, &worlds, &o.data.join("locks"))?;
    // Its own identity: what it writes in members' names, it signs as itself.
    if !store.has_identity() {
        store.save_identity(&Identity::create(&o.name).map_err(|e| AppError(e.to_string()))?)?;
    }
    let keeper = Arc::new(Keeper::open(store.clone(), &o.data, o.admin.clone())?);
    // Every world read once now, in the background: whoever comes first
    // does not wait for it.
    let reading = store.clone();
    std::thread::spawn(move || {
        for world in reading.held_worlds().unwrap_or_default() {
            let _ = reading.digest(&world);
        }
    });
    let reach = Reach {
        lan: o.lan,
        internet: true,
        stun: store::NetworkSettings::default().stun,
        // The same endpoint every start: its code stays valid.
        secret: Some(network::endpoint_secret(&store)?),
        port: Some(o.port),
        port_required: true,
        only: home_ip(&o.addresses),
    };
    // Backups, by themselves: a minute after the start, then every quarter hour.
    let backing = keeper.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        backing.due();
        std::thread::sleep(std::time::Duration::from_secs(14 * 60));
    });
    let net = WorldNet::start(keeper.clone(), reach).await.map_err(|e| AppError(format!("The network could not start: {e}")))?;

    let mut addresses = o.addresses.clone();
    if addresses.is_empty() {
        addresses.extend(net.reachable().iter().map(|a| a.to_string()));
        addresses.extend(net.home_address().map(|a| a.to_string()).filter(|a| !addresses.contains(a)));
    }
    let code = Server { endpoint: net.id(), addrs: addresses }.code();
    aw_store::durable::write(&o.data.join("host-code.txt"), format!("{code}\n").as_bytes())?;
    println!("aw-server on UDP port {} — admin {}", o.port, o.admin);
    println!("code: {code}");
    // Its address alone is enough then: the app asks for the code on TCP, same port.
    let told = code.clone();
    let port = o.port;
    tokio::spawn(async move {
        if let Err(e) = aw_sync::world::tell_code(port, told).await {
            eprintln!("TCP port {port}: {e} — the full code is needed then");
        }
    });
    stopped().await;
    drop(net);
    Ok(())
}
/// The one address it answers at: the first of its own addresses it was
/// told that belongs to this machine, otherwise its home network's.
fn home_ip(addresses: &[String]) -> Option<std::net::Ipv4Addr> {
    let own = |ip: &std::net::Ipv4Addr| std::net::UdpSocket::bind((*ip, 0)).is_ok();
    addresses
        .iter()
        .filter_map(|a| a.parse::<std::net::SocketAddr>().ok())
        .find_map(|a| match a.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() && own(&ip) => Some(ip),
            _ => None,
        })
        .or_else(aw_sync::world::own_ipv4)
}
/// Ctrl+C — or, in a container, the stop it is sent.
async fn stopped() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
