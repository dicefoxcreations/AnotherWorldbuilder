//! aw-server — the keeper of a group's worlds: on a NAS, a small server, in
//! a container. Always on, so players work when the GM's device is off.
//!
//! It knows who is who — everyone proves their identity when they connect —
//! and lets each one in where the world itself says they belong: its GM, and
//! its members. Each command a member asks for it does in their name, and
//! hands them what their view and rights say (see `keeper`).
//!
//! Its admin, a GM named when it starts (`--admin`), manages it from the
//! app: which GMs may bring worlds, which worlds are closed, who is turned
//! away.
//!
//! ```text
//! <data>/profile/      the server's own identity and endpoint key
//! <data>/worlds/       the worlds, as in the app
//! <data>/server.json   GMs, closed worlds, who came, who is turned away
//! <data>/backups/      the worlds' backups
//! <data>/host-code.txt what GMs and players enter
//! ```

use super::*;
use aw_sync::world::{Reach, Server, WorldNet};
use std::path::PathBuf;
use std::sync::Arc;
use std::future::Future;

use crate::keeper::Keeper;

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


#[derive(Clone, Deserialize)]
pub struct ServerOptions {
    pub admin: String,
    pub data: PathBuf,
    pub port: u16,
    pub name: String,
    pub addresses: Vec<String>,
    pub lan: bool,
    /// Router mapping, STUN and IPv6. Disabled in isolated lifecycle tests.
    pub internet: bool,
}

impl ServerOptions {
    pub fn validate(&mut self) -> Result<(), String> {
        self.admin = self.admin.trim().to_ascii_lowercase();
        if !self.admin.starts_with("id:") {
            self.admin = format!("id:{}", self.admin);
        }
        if !is_player_id(&self.admin) {
            return Err("Name the admin by their id — id: and 16 characters, under Profile in the app.".into());
        }
        if self.port == 0 {
            return Err("Choose a port from 1 to 65535.".into());
        }
        if !self.data.is_absolute() {
            return Err("The server data folder must be absolute.".into());
        }
        self.name = self.name.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 120 {
            return Err("Give the server a name of 1 to 120 characters.".into());
        }
        Ok(())
    }
}

fn options() -> Result<ServerOptions, String> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let mut o = ServerOptions {
        admin: env("AW_ADMIN").unwrap_or_default().trim().to_string(),
        data: env("AW_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/data")),
        port: env("AW_PORT").map(|p| p.trim().parse().map_err(|_| "AW_PORT is no port")).transpose()?.unwrap_or(7447),
        name: env("AW_NAME").unwrap_or_else(|| "aw-server".into()),
        addresses: env("AW_ADDRESS").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
        lan: env("AW_LAN").is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes")),
        internet: true,
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
    // Relative CLI paths have always been relative to the caller's folder.
    if !o.data.is_absolute() {
        o.data = std::env::current_dir().map_err(|e| e.to_string())?.join(&o.data);
    }
    o.validate().map_err(|e| format!("{e}\n\n{USAGE}"))?;
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
    let (port, admin) = (o.port, o.admin.clone());
    if let Err(e) = run_until(o, stopped(), |code| {
        println!("aw-server on UDP port {port} — admin {admin}");
        println!("code: {code}");
    }) {
        eprintln!("aw-server: {e}");
        std::process::exit(1);
    }
}

/// Blocking host entry point shared by the CLI and Android's service worker.
/// The caller owns cancellation; no process signals or process exit are used.
/// Returning means networking, backups and blocking writes have finished and
/// storage locks have been released. A later call can start the same host again.
pub fn run_until(
    mut options: ServerOptions,
    shutdown: impl Future<Output = ()>,
    ready: impl FnOnce(String),
) -> Result<(), String> {
    static RUNNING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _running = RUNNING.try_lock().map_err(|_| "A server is already running in this process.".to_string())?;
    options.validate()?;
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    // A dedicated phone should not create one worker for every CPU core.
    #[cfg(target_os = "android")]
    builder.worker_threads(2).max_blocking_threads(4);
    let runtime = builder.build().map_err(|e| e.to_string())?;
    let mut claimed = None;
    let result = runtime.block_on(run(options, shutdown, ready, &mut claimed));
    // Runtime shutdown joins any already running spawn_blocking writes, even
    // when the network was stopped in the middle of a request.
    drop(runtime);
    let cleanup = if let Some(store) = claimed {
        applog::line("server", &result.as_ref().map(|_| "stopped".to_string()).unwrap_or_else(|e| e.0.clone()));
        remote::clear_views(&store);
        let result = store.close_all();
        profile::release_folders(store.profile_dir());
        result.map_err(|e| AppError(e.to_string()))
    } else {
        Ok(())
    };
    result.and(cleanup).map_err(|e| e.0)
}

async fn run(
    o: ServerOptions,
    shutdown: impl Future<Output = ()>,
    ready: impl FnOnce(String),
    claimed: &mut Option<Store>,
) -> Answer<()> {
    tokio::pin!(shutdown);
    let (profile, worlds) = (o.data.join("profile"), o.data.join("worlds"));
    std::fs::create_dir_all(&profile)?;
    std::fs::create_dir_all(&worlds)?;
    let store = profile::use_folders(&profile, &worlds, &o.data.join("locks"))?;
    *claimed = Some(store.clone());
    applog::line("server", &format!("starting on UDP/TCP port {}", o.port));
    // Its own identity: what it writes in members' names, it signs as itself.
    if !store.has_identity() {
        store.save_identity(&Identity::create(&o.name).map_err(|e| AppError(e.to_string()))?)?;
    } else {
        let mut identity = store.load_identity()?;
        if identity.name() != o.name {
            identity.rename(&o.name);
            store.save_identity(&identity)?;
        }
    }
    let keeper = Arc::new(Keeper::open(store.clone(), &o.data, o.admin.clone())?);
    let reach = Reach {
        lan: o.lan,
        internet: o.internet,
        stun: store::NetworkSettings::default().stun,
        // The same endpoint every start: its code stays valid.
        secret: Some(network::endpoint_secret(&store)?),
        port: Some(o.port),
        port_required: true,
        only: home_ip(&o.addresses),
    };
    let net = tokio::select! {
        biased;
        _ = &mut shutdown => return Ok(()),
        result = WorldNet::start_with_code(keeper.clone(), reach) => result.map_err(|e| AppError(format!("The network could not start: {e}")))?,
    };
    let (net, code_listener) = net;

    let mut addresses = o.addresses.clone();
    if addresses.is_empty() {
        addresses.extend(net.reachable().iter().map(|a| a.to_string()));
        addresses.extend(net.home_address().map(|a| a.to_string()).filter(|a| !addresses.contains(a)));
    }
    let code = Server { endpoint: net.id(), addrs: addresses }.code();
    aw_store::durable::write(&o.data.join("host-code.txt"), format!("{code}\n").as_bytes())?;
    // Its address alone is enough then: the app asks for the code on TCP, same port.
    let told = code.clone();
    let port = o.port;
    let code_task = tokio::spawn(async move {
        if let Err(e) = aw_sync::world::tell_code_on(code_listener, told).await {
            eprintln!("TCP port {port}: {e} — the full code is needed then");
        }
    });
    // Cancellable while waiting; a backup already copying is joined, never
    // cut short by our Stop action. No detached, immortal backup threads.
    let (stop_backups, mut backup_stop) = tokio::sync::oneshot::channel::<()>();
    let backups = tokio::spawn(async move {
        let mut delay = 60;
        loop {
            tokio::select! {
                biased;
                _ = &mut backup_stop => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {},
            }
            let backing = keeper.clone();
            let _ = tokio::task::spawn_blocking(move || backing.due()).await;
            delay = 5 * 60;
        }
    });
    applog::line("server", "ready");
    ready(code);
    shutdown.await;
    code_task.abort();
    let _ = code_task.await;
    net.shutdown().await;
    let _ = stop_backups.send(());
    let _ = backups.await;
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
