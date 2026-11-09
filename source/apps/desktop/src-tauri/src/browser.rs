//! The browser's door: an extension in this device's own browser hands the
//! app pages to take into a world.
//!
//! Closed until the user opens it in the options. Open, the app listens on
//! this device's loopback address only — nothing from the network reaches
//! it — and answers browser extensions alone: a web page cannot get in, nor
//! a site posing under another name for this address. Switching it on is
//! the consent: an extension finds the app by itself, on one of a few ports,
//! without pairing. It sees the worlds this device leads (only a game master
//! takes packages in) and sends packages; the app takes them in as from a
//! folder (`transfer::import`).

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;

/// Where extensions knock: the first of these ports that is free — fixed,
/// so that they find the app; more than one, for a second app or profile.
pub const PORT: u16 = 47823;
pub const LAST_PORT: u16 = PORT + 7;
/// What the extension asks for — it says so when the app is older.
const API_VERSION: u8 = 3;
/// A package with its pictures — larger ones the extension sends in parts.
const LARGEST: usize = 256 * 1024 * 1024;
const FILE: &str = "browser.json";

/// What the profile keeps: whether the door is open.
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub enabled: bool,
}

/// Open now, on which port — or why it could not open.
struct Door {
    listening: Option<Arc<AtomicBool>>,
    port: Option<u16>,
    problem: Option<String>,
}

static DOOR: Mutex<Door> = Mutex::new(Door { listening: None, port: None, problem: None });

/// This running app with this profile — not a key, a name to ask for: a
/// world chosen in the extension never lands in another app that took over
/// the port after this one closed.
fn instance_id(store: &Store) -> String {
    static PROCESS: OnceLock<String> = OnceLock::new();
    let process = PROCESS.get_or_init(|| {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        format!("{}-{}", std::process::id(), since.as_nanos())
    });
    let mut hash = blake3::Hasher::new();
    hash.update(process.as_bytes());
    hash.update(store.profile_dir().as_os_str().as_encoded_bytes());
    hash.finalize().to_hex().to_string()
}

fn load(store: &Store) -> Settings {
    std::fs::read(store.profile_dir().join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(store: &Store, s: &Settings) -> Answer<()> {
    std::fs::create_dir_all(store.profile_dir())?;
    aw_store::durable::write(&store.profile_dir().join(FILE), &serde_json::to_vec_pretty(s)?)?;
    Ok(())
}

// --- Listening ------------------------------------------------------------------

/// Opens the door if the profile says so — at start.
pub fn start(app: &tauri::AppHandle) {
    if let Ok(store) = store(app) {
        if load(&store).enabled {
            open(app);
        }
    }
}

fn open(app: &tauri::AppHandle) {
    let mut door = DOOR.lock().unwrap();
    if door.listening.is_some() {
        return;
    }
    let mut listener = None;
    for port in PORT..=LAST_PORT {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(bound) => {
                listener = Some((bound, port));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(e) => {
                door.problem = Some(format!("Could not open: {e}"));
                return;
            }
        }
    }
    let Some((listener, port)) = listener else {
        door.problem = Some(format!("Ports {PORT}–{LAST_PORT} are taken."));
        return;
    };
    let on = Arc::new(AtomicBool::new(true));
    door.listening = Some(on.clone());
    door.port = Some(port);
    door.problem = None;
    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if !on.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = serve(&app, stream);
            });
        }
    });
}

fn close() {
    let mut door = DOOR.lock().unwrap();
    if let Some(on) = door.listening.take() {
        on.store(false, Ordering::SeqCst);
        // The loop waits for a caller: one last knock lets it see it is closed.
        if let Some(port) = door.port {
            let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300));
        }
    }
    door.port = None;
}

// --- HTTP, as little of it as a browser needs ---------------------------------

struct Request {
    method: String,
    path: String,
    host: String,
    origin: Option<String>,
    /// The app it means — see [`instance_id`].
    instance: Option<String>,
    /// Where it came in.
    port: u16,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> std::io::Result<Request> {
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or("").to_string();
    let path = words.next().unwrap_or("").to_string();
    let (mut host, mut origin, mut instance, mut length) = (String::new(), None, None, 0usize);
    loop {
        let mut header = String::new();
        if r.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else { continue };
        let value = value.trim().to_string();
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => host = value,
            "origin" => origin = Some(value),
            "x-dicefox-instance" => instance = Some(value),
            "content-length" => length = value.parse().unwrap_or(usize::MAX),
            _ => {}
        }
    }
    if length > LARGEST {
        return Err(std::io::Error::other("too large"));
    }
    let mut body = vec![0; length];
    r.read_exact(&mut body)?;
    Ok(Request { method, path, host, origin, instance, port: stream.local_addr()?.port(), body })
}

/// Browser extensions only: their addresses are theirs alone.
fn from_extension(origin: &str) -> bool {
    ["chrome-extension://", "moz-extension://", "safari-web-extension://"].iter().any(|p| origin.starts_with(p))
}

/// Whether a request may come in — and the extension's address to answer, if
/// a browser sent it. Another name for this address (a site rebinding its
/// own) or a web page: no.
fn admitted(r: &Request) -> Option<Option<String>> {
    let port = r.port;
    let local = (PORT..=LAST_PORT).contains(&port) && [format!("127.0.0.1:{port}"), format!("localhost:{port}")].contains(&r.host);
    match &r.origin {
        _ if !local => None,
        Some(o) if from_extension(o) => Some(Some(o.clone())),
        Some(_) => None,
        None => Some(None),
    }
}

fn serve(app: &tauri::AppHandle, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let request = match read_request(&stream) {
        Ok(r) => r,
        Err(_) => return reply(&stream, None, 400, &serde_json::json!({ "error": "Bad request." })),
    };
    let Some(origin) = admitted(&request) else {
        return reply(&stream, None, 403, &serde_json::json!({ "error": "Only browser extensions on this device." }));
    };
    if request.method == "OPTIONS" {
        return reply(&stream, origin.as_deref(), 204, &serde_json::Value::Null);
    }
    let tell = |event: &str, payload: serde_json::Value| {
        let _ = app.emit(event, payload);
    };
    let routed = store(app).map_err(|e| (503, e.0)).and_then(|store| route(&store, &tell, &request));
    let (status, body) = match routed {
        Ok(v) => (200, v),
        Err((status, e)) => (status, serde_json::json!({ "error": e })),
    };
    reply(&stream, origin.as_deref(), status, &body)
}

fn reply(mut stream: &TcpStream, origin: Option<&str>, status: u16, body: &serde_json::Value) -> std::io::Result<()> {
    let text = if body.is_null() { String::new() } else { body.to_string() };
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", text.len());
    if let Some(o) = origin {
        head.push_str(&format!(
            "Access-Control-Allow-Origin: {o}\r\nAccess-Control-Allow-Headers: content-type, x-dicefox-instance\r\nAccess-Control-Allow-Methods: GET, POST\r\nVary: Origin\r\n"
        ));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(text.as_bytes())?;
    stream.flush()
}

type Routed = Result<serde_json::Value, (u16, String)>;

fn fail<E: std::fmt::Display>(e: E) -> (u16, String) {
    (500, e.to_string())
}

/// Tells the app's page what happened: an event and its payload.
type Tell<'a> = &'a dyn Fn(&str, serde_json::Value);

/// A value of the query, decoded once — `world%3Aabc` is `world:abc`.
fn query_value(query: &str, name: &str) -> Result<String, (u16, String)> {
    fn decode(text: &str) -> Result<String, (u16, String)> {
        let bad = || (400, "Bad encoding.".to_string());
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'%' => {
                    let digits = bytes.get(i + 1..i + 3).and_then(|d| std::str::from_utf8(d).ok()).ok_or_else(bad)?;
                    out.push(u8::from_str_radix(digits, 16).map_err(|_| bad())?);
                    i += 3;
                    continue;
                }
                b'+' => out.push(b' '),
                byte => out.push(byte),
            }
            i += 1;
        }
        String::from_utf8(out).map_err(|_| bad())
    }
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if decode(key)? == name {
            return decode(value);
        }
    }
    Ok(String::new())
}

fn route(store: &Store, tell: Tell, r: &Request) -> Routed {
    // Closed a moment ago: what the listener took in already gets no further.
    if !load(store).enabled {
        return Err((403, "Switched off in the app.".into()));
    }
    let (path, query) = r.path.split_once('?').unwrap_or((&r.path, ""));
    let instance = instance_id(store);
    if r.method == "GET" && path == "/v1/hello" {
        let name = store.load_identity().map(|me| me.name().to_string()).unwrap_or_else(|_| "AnotherWorldbuilder".into());
        return Ok(serde_json::json!({
            "app": "AnotherWorldbuilder",
            "version": env!("CARGO_PKG_VERSION"),
            "browser_api": API_VERSION,
            "instance": instance,
            "name": name,
        }));
    }
    if r.instance.as_deref() != Some(instance.as_str()) {
        return Err((409, "The app changed — look for it again.".into()));
    }
    match (r.method.as_str(), path) {
        ("GET", "/v1/worlds") => worlds(store),
        ("GET", "/v1/types") => types(store, &query_value(query, "world")?),
        ("POST", "/v1/import") => take(store, tell, r.origin.as_deref().unwrap_or("Local extension"), &r.body),
        ("POST", "/v1/known") => known(store, &r.body),
        _ => Err((404, "No such thing.".into())),
    }
}

/// The worlds this device leads: here, and on their servers — asked there.
fn worlds(store: &Store) -> Routed {
    let mut led: Vec<serde_json::Value> = store
        .list_worlds()
        .map_err(fail)?
        .into_iter()
        .filter(|m| store.is_gm(&m.id))
        .map(|m| serde_json::json!({ "id": m.id, "name": m.name, "icon": m.icon, "members": m.members.len() }))
        .collect();
    let remote: Vec<String> = store.load_network().map_err(fail)?.remote.into_iter().filter(|(_, r)| !r.asked).map(|(w, _)| w).collect();
    for world in remote {
        if let Ok(info) = tauri::async_runtime::block_on(network::connect(store, &world)) {
            if info.is_gm {
                led.push(serde_json::json!({ "id": info.id, "name": info.name, "icon": info.icon, "members": info.members.len() }));
            }
        }
    }
    Ok(serde_json::Value::Array(led))
}

/// Whether this device leads a world: here, or on its server.
fn leads(store: &Store, world: &str) -> bool {
    if store.holds(world) {
        return store.is_gm(world);
    }
    network::remote_of(store, world).is_some() && tauri::async_runtime::block_on(network::connect(store, world)).is_ok_and(|i| i.is_gm)
}

/// A world's types — so that a package brings only those it lacks, and the
/// world's own stay as its game master made them.
fn types(store: &Store, world: &str) -> Routed {
    if !leads(store, world) {
        return Err((403, "Only the GM's worlds.".into()));
    }
    if store.holds(world) {
        return serde_json::to_value(store.load_templates(world).map_err(|e| (500, e.to_string()))?).map_err(fail);
    }
    tauri::async_runtime::block_on(network::call(store, world, "templates_types", serde_json::json!({}))).map_err(|e| (500, e.0))
}

/// Which of these document ids a GM world holds already: what is there need
/// not be read again. Ids only — nothing of what they hold.
fn known(store: &Store, body: &[u8]) -> Routed {
    #[derive(Deserialize)]
    struct Asked {
        world: String,
        ids: Vec<String>,
    }
    let asked: Asked = serde_json::from_slice(body).map_err(|e| (400, e.to_string()))?;
    if !leads(store, &asked.world) {
        return Err((403, "Only the GM's worlds.".into()));
    }
    if store.holds(&asked.world) {
        return serde_json::to_value(crate::documents_known_in(store, &asked.world, asked.ids).map_err(|e| (500, e.0))?).map_err(fail);
    }
    tauri::async_runtime::block_on(network::call(store, &asked.world, "documents_known", serde_json::json!({ "ids": asked.ids }))).map_err(|e| (500, e.0))
}

/// A package from the browser: its `package.json` and its files, the files
/// base64 under the paths the package names. Into a world on a server, it
/// goes there as it came.
fn take(store: &Store, tell: Tell, from: &str, body: &[u8]) -> Routed {
    #[derive(Deserialize)]
    struct Sent {
        world: String,
        package: serde_json::Value,
        #[serde(default)]
        files: BTreeMap<String, String>,
    }
    let sent: Sent = serde_json::from_slice(body).map_err(|e| (400, e.to_string()))?;
    if !leads(store, &sent.world) {
        return Err((403, "Only the GM imports.".into()));
    }
    let summary = if store.holds(&sent.world) {
        let progress = |done: usize, total: usize| tell("package-progress", serde_json::json!([done, total]));
        let summary = transfer::take_in(store, &sent.world, &sent.package, &sent.files, &progress).map_err(|e| (400, e.0))?;
        serde_json::to_value(&summary).map_err(fail)?
    } else {
        let args = serde_json::json!({ "world": sent.world, "package": sent.package, "files": sent.files });
        tauri::async_runtime::block_on(network::call(store, &sent.world, "package_take", args)).map_err(|e| {
            if e.0.contains("[online]") {
                tell("online-unavailable", serde_json::json!({ "world": sent.world, "error": e.0 }));
            }
            (400, e.0)
        })?
    };
    tell("browser-imported", serde_json::json!({ "world": sent.world, "from": from, "name": summary["name"], "documents": summary["documents_new"] }));
    Ok(summary)
}

// --- Commands ---------------------------------------------------------------

/// The door as the options show it.
#[derive(Serialize)]
pub struct View {
    pub enabled: bool,
    pub open: bool,
    pub port: u16,
    pub problem: Option<String>,
}

fn view(store: &Store) -> View {
    let s = load(store);
    let door = DOOR.lock().unwrap();
    View {
        enabled: s.enabled,
        open: door.listening.is_some(),
        port: door.port.unwrap_or(PORT),
        problem: door.problem.clone(),
    }
}

#[tauri::command(async)]
pub fn browser_settings(app: tauri::AppHandle) -> Answer<View> {
    Ok(view(&store(&app)?))
}

#[tauri::command(async)]
pub fn browser_enable(app: tauri::AppHandle, enabled: bool) -> Answer<View> {
    let store = store(&app)?;
    let mut s = load(&store);
    s.enabled = enabled;
    save(&store, &s)?;
    if enabled {
        open(&app);
    } else {
        close();
    }
    Ok(view(&store))
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
