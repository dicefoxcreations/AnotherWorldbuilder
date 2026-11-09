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
#[path = "dedicated_tests.rs"]
mod tests;
