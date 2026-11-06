//! The dedicated server, started and stopped inside a program that keeps
//! running — the Android service, the tray on Linux and Windows.
//!
//! It is the host of `aw-server` (`server::run_until`), with its own
//! identity, its own folder and its admin — not the app's own server, which
//! keeps the worlds of the profile. One runs per process: the program that
//! starts it runs nothing else of the app.
//!
//! A worker thread owns the host's runtime. Stop works even while it is still
//! starting; a later start waits until the last one has let go of everything.

use crate::server::{run_until, ServerOptions};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use std::time::Instant;
use tokio::sync::watch;

/// What the server is doing — `state`: starting, running, stopping, stopped
/// or failed; `code`: what members enter, once it runs.
#[derive(Default, Clone, Serialize)]
pub struct Status {
    pub state: &'static str,
    pub code: String,
    pub error: String,
}

struct Running {
    status: Arc<Mutex<Status>>,
    stop: Option<watch::Sender<bool>>,
    worker: std::thread::JoinHandle<()>,
}

fn slot() -> &'static Mutex<Option<Running>> {
    static SLOT: OnceLock<Mutex<Option<Running>>> = OnceLock::new();
    SLOT.get_or_init(Default::default)
}

/// How long a start waits for its port, held by the app on this device.
const PORT_WAIT: Duration = Duration::from_secs(8);

/// Starts the server as `options` say — it runs on until `stop`.
pub fn start(mut options: ServerOptions) -> Result<(), String> {
    options.validate()?;
    let mut slot = slot().lock().map_err(|e| e.to_string())?;
    if slot.as_ref().is_some_and(|r| !r.worker.is_finished()) {
        return Err("The server is running or still stopping.".into());
    }
    if let Some(last) = slot.take() {
        let _ = last.worker.join();
    }
    let status = Arc::new(Mutex::new(Status { state: "starting", ..Default::default() }));
    let told = status.clone();
    let (stop, stopped) = watch::channel(false);
    let worker = std::thread::Builder::new()
        .name("aw-server".into())
        .spawn(move || {
            // The app on this device may still hold the port: it moves aside
            // within a few seconds (`network::keep_off_aw_server`).
            let since = Instant::now();
            let ran = loop {
                let ready = told.clone();
                let mut shutdown = stopped.clone();
                let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_until(
                        options.clone(),
                        async move {
                            let _ = shutdown.wait_for(|stop| *stop).await;
                        },
                        move |code| {
                            if let Ok(mut s) = ready.lock() {
                                s.code = code;
                                if s.state != "stopping" {
                                    s.state = "running";
                                }
                            }
                        },
                    )
                }))
                .unwrap_or_else(|_| Err("The server stopped after an internal error.".into()));
                let taken = ran.as_ref().is_err_and(|e| e.contains(": port "));
                if !taken || *stopped.borrow() || since.elapsed() > PORT_WAIT {
                    break ran;
                }
                std::thread::sleep(Duration::from_millis(500));
            };
            if let Ok(mut s) = told.lock() {
                s.state = if ran.is_ok() { "stopped" } else { "failed" };
                s.error = ran.err().unwrap_or_default();
            }
        })
        .map_err(|e| e.to_string())?;
    *slot = Some(Running { status, stop: Some(stop), worker });
    Ok(())
}

/// Asks the server to stop — `status` says when it has.
pub fn stop() {
    let Ok(mut slot) = slot().lock() else { return };
    let Some(running) = slot.as_mut() else { return };
    if let Some(stop) = running.stop.take() {
        if let Ok(mut s) = running.status.lock() {
            if matches!(s.state, "starting" | "running") {
                s.state = "stopping";
            }
        }
        let _ = stop.send(true);
    }
}

pub fn status() -> Status {
    slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().and_then(|r| r.status.lock().ok().map(|s| s.clone())))
        .unwrap_or_else(|| Status { state: "stopped", ..Default::default() })
}

/// Whether the last server has let go of everything — its port and folder.
pub fn finished() -> bool {
    slot().lock().map(|slot| slot.as_ref().is_none_or(|r| r.worker.is_finished())).unwrap_or(true)
}

/// A world the server keeps, as its own screen lists it.
#[derive(Debug, Serialize)]
pub struct Kept {
    id: String,
    name: String,
    icon: String,
    articles: u64,
    members: u64,
    closed: bool,
    error: String,
}

/// The worlds in the server's folder `data` — read beside the running server,
/// or while it is stopped, and never written: a reader of its own per world,
/// not the server's.
pub fn worlds(data: &Path) -> Result<Vec<Kept>, String> {
    if !data.is_absolute() {
        return Err("The server data folder must be absolute.".into());
    }
    let root = data.join("worlds");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let book: serde_json::Value =
        std::fs::read(data.join("server.json")).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().join("world.sqlite");
        if !path.is_file() {
            continue;
        }
        let read = || -> rusqlite::Result<Kept> {
            let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
            db.busy_timeout(Duration::from_millis(200))?;
            db.query_row(
                "SELECT (SELECT value FROM world WHERE key='id'), (SELECT value FROM world WHERE key='name'),
                        (SELECT value FROM world WHERE key='icon'), (SELECT count(*) FROM documents), (SELECT count(*) FROM members)",
                [],
                |row| {
                    let id: String = row.get(0)?;
                    Ok(Kept {
                        closed: book.get("closed").and_then(|c| c.get(&id)).is_some(),
                        id,
                        name: row.get::<_, Option<String>>(1)?.filter(|n| !n.is_empty()).unwrap_or_else(|| "Unnamed world".into()),
                        icon: row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "🌍".into()),
                        articles: row.get(3)?,
                        members: row.get(4)?,
                        error: String::new(),
                    })
                },
            )
        };
        // One that cannot be read is listed as such, not left out.
        out.push(read().unwrap_or_else(|e| {
            let folder = entry.file_name().to_string_lossy().into_owned();
            Kept { id: folder.clone(), name: folder, icon: "🌍".into(), articles: 0, members: 0, closed: false, error: e.to_string() }
        }));
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_crypto::{Identity, IdentityPublic};
    use aw_sync::world::{Ask, Host, Offer, Order, Reach, Reply, Server, WorldNet};
    use std::time::Instant;

    struct Nobody;
    impl Host for Nobody {
        fn offers(&self) -> Vec<Offer> {
            Vec::new()
        }
        fn answer(&self, _: &str, _: &IdentityPublic, _: Ask) -> Reply {
            Reply::NotOffered
        }
    }

    fn wait(wanted: &str) -> Status {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let s = status();
            if s.state == wanted && (wanted != "stopped" || finished()) {
                return s;
            }
            assert_ne!(s.state, "failed", "{}", s.error);
            assert!(Instant::now() < until, "wanted {wanted}, got {}: {}", s.state, s.error);
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn options(dir: &Path, admin: &Identity, port: u16, name: &str) -> ServerOptions {
        ServerOptions { admin: admin.id().to_string(), data: dir.to_path_buf(), port, name: name.into(), addresses: Vec::new(), lan: false, internet: false }
    }

    #[test]
    fn stopped_while_starting_started_again_renamed_handed_to_another_admin_and_waiting_for_its_port() {
        let dir = tempfile::tempdir().unwrap();
        let admin = Identity::create("Marla").unwrap();
        let port = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        let mut bad = options(dir.path(), &admin, port, "Table");
        bad.admin = "nobody".into();
        assert!(start(bad).is_err(), "no worker for options that do not hold");
        start(options(dir.path(), &admin, port, "Table")).unwrap();
        stop();
        wait("stopped");

        start(options(dir.path(), &admin, port, "Table")).unwrap();
        assert!(start(options(dir.path(), &admin, port, "Table")).is_err(), "never two on the same folder");
        let first = wait("running").code;
        let server = Server::parse(&first).unwrap();
        let allowed = "id:0000000000000001";
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let client = WorldNet::start(Arc::new(Nobody), Reach::default()).await.unwrap();
            let reply = client.ask_server(&server, "", &admin, Ask::Manage { order: Some(Order::Allow { id: allowed.into() }) }).await.unwrap();
            assert!(matches!(reply, Reply::Managed { state } if state.worldbuilders.iter().any(|p| p.id == allowed)));
            let stranger = Identity::create("Tim").unwrap();
            assert_eq!(client.ask_server(&server, "", &stranger, Ask::Manage { order: None }).await.unwrap(), Reply::NotOffered);
            client.shutdown().await;
        });
        drop(runtime);
        stop();
        wait("stopped");
        assert!(std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).is_ok(), "its TCP port is free again");
        assert!(std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).is_ok(), "and its UDP port");

        // Renamed, with another admin: the same server — its code and what it was told stay.
        let other = Identity::create("Olaf").unwrap();
        start(options(dir.path(), &other, port, "Olaf's table")).unwrap();
        assert_eq!(wait("running").code, first);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let client = WorldNet::start(Arc::new(Nobody), Reach::default()).await.unwrap();
            assert!(matches!(client.ask_server(&server, "", &other, Ask::Worlds).await.unwrap(),
                Reply::Worlds { host: Some(host), .. } if host.name == "Olaf's table"));
            assert_eq!(client.ask_server(&server, "", &admin, Ask::Manage { order: None }).await.unwrap(), Reply::NotOffered, "the old admin is out");
            let reply = client.ask_server(&server, "", &other, Ask::Manage { order: None }).await.unwrap();
            assert!(matches!(reply, Reply::Managed { state } if state.worldbuilders.iter().any(|p| p.id == allowed)));
            client.shutdown().await;
        });
        drop(runtime);
        stop();
        wait("stopped");

        // The app holds its port for a moment: it waits, then runs.
        let held = std::net::TcpListener::bind((std::net::Ipv6Addr::UNSPECIFIED, port)).or_else(|_| std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))).unwrap();
        start(options(dir.path(), &other, port, "Olaf's table")).unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(status().state, "starting", "still waiting for its port");
        drop(held);
        assert_eq!(wait("running").code, first);
        stop();
        wait("stopped");
    }

    #[test]
    fn worlds_are_read_beside_a_writer_and_a_broken_one_is_listed() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("worlds/world-test");
        std::fs::create_dir_all(&folder).unwrap();
        let writer = Connection::open(folder.join("world.sqlite")).unwrap();
        writer
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE world(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE documents(id TEXT);
                 CREATE TABLE members(id TEXT);
                 INSERT INTO world VALUES ('id','world:test'),('name','Cinder Coast'),('icon','🌍');
                 INSERT INTO documents VALUES ('one'),('two'); INSERT INTO members VALUES ('gm');",
            )
            .unwrap();
        std::fs::write(root.path().join("server.json"), r#"{"closed":{"world:test":"Cinder Coast"}}"#).unwrap();
        writer.execute_batch("BEGIN; UPDATE world SET value='Uncommitted' WHERE key='name';").unwrap();
        let kept = worlds(root.path()).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name, "Cinder Coast");
        assert_eq!((kept[0].articles, kept[0].members, kept[0].closed), (2, 1, true));
        writer.execute_batch("ROLLBACK;").unwrap();
        std::fs::create_dir_all(root.path().join("worlds/broken")).unwrap();
        std::fs::write(root.path().join("worlds/broken/world.sqlite"), b"not a database").unwrap();
        let kept = worlds(root.path()).unwrap();
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|w| !w.error.is_empty()));
    }
}
