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
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn request(method: &str, path: &str, origin: Option<&str>, instance: Option<&str>, body: serde_json::Value) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            host: format!("127.0.0.1:{PORT}"),
            origin: origin.map(str::to_string),
            instance: instance.map(str::to_string),
            port: PORT,
            body: if body.is_null() { Vec::new() } else { body.to_string().into_bytes() },
        }
    }

    #[test]
    fn only_extensions_on_this_device_come_in() {
        let ext = "chrome-extension://abcdefghijklmnop";
        assert_eq!(admitted(&request("GET", "/v1/hello", Some(ext), None, serde_json::Value::Null)), Some(Some(ext.into())));
        assert_eq!(admitted(&request("GET", "/v1/hello", Some("moz-extension://x"), None, serde_json::Value::Null)), Some(Some("moz-extension://x".into())));
        // A web page, even one served from this device.
        assert_eq!(admitted(&request("GET", "/v1/hello", Some("https://example.org"), None, serde_json::Value::Null)), None);
        assert_eq!(admitted(&request("GET", "/v1/hello", Some("http://127.0.0.1:8000"), None, serde_json::Value::Null)), None);
        // A site that made its own name point here.
        let mut rebound = request("GET", "/v1/hello", Some(ext), None, serde_json::Value::Null);
        rebound.host = format!("evil.example:{PORT}");
        assert_eq!(admitted(&rebound), None);
        // A program on this device, no browser: in, without an address to answer.
        assert_eq!(admitted(&request("GET", "/v1/hello", None, None, serde_json::Value::Null)), Some(None));
        // Any of the app's ports, as long as the name fits it.
        let mut later = request("GET", "/v1/hello", Some(ext), None, serde_json::Value::Null);
        later.port = LAST_PORT;
        later.host = format!("127.0.0.1:{LAST_PORT}");
        assert!(admitted(&later).is_some());
        later.host = format!("localhost:{PORT}");
        assert!(admitted(&later).is_none());
    }

    #[test]
    fn switched_on_the_app_is_found_and_takes_packages_into_a_world_it_leads() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("gm"));
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let world = new_world(&store, &gm, "Eldermere".into()).unwrap().id;
        let told = RefCell::new(Vec::<String>::new());
        let tell = |event: &str, _: serde_json::Value| told.borrow_mut().push(event.to_string());
        let ask = |r: Request| route(&store, &tell, &r);

        // Switched off: nothing, not even hello. Switched on: found, no pairing.
        assert_eq!(ask(request("GET", "/v1/hello", None, None, serde_json::Value::Null)).unwrap_err().0, 403);
        save(&store, &Settings { enabled: true }).unwrap();
        let hello = ask(request("GET", "/v1/hello", None, None, serde_json::Value::Null)).unwrap();
        assert_eq!(hello["browser_api"], 3);
        let target = hello["instance"].as_str().unwrap().to_string();
        // Asking for another app: no.
        assert_eq!(ask(request("GET", "/v1/worlds", None, Some("stale"), serde_json::Value::Null)).unwrap_err().0, 409);
        let worlds = ask(request("GET", "/v1/worlds", None, Some(&target), serde_json::Value::Null)).unwrap();
        assert_eq!(worlds[0]["id"], world.as_str());
        // The world's id as the extension sends it: encoded.
        let encoded = world.replace(':', "%3A");
        let types = ask(request("GET", &format!("/v1/types?world={encoded}"), None, Some(&target), serde_json::Value::Null)).unwrap();
        assert!(types.is_array());
        assert_eq!(ask(request("GET", "/v1/types?world=world%3A0000000000000000", None, Some(&target), serde_json::Value::Null)).unwrap_err().0, 403);

        // A package with a picture.
        let img = image::RgbImage::from_fn(8, 8, |x, _| image::Rgb([x as u8 * 30, 90, 160]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        use base64::Engine;
        let page = "art:01J00000000000000000000003";
        let package = serde_json::json!({
            "format": "aw-package", "version": 1, "name": "From the browser", "written": true,
            "documents": [{ "id": page, "type": "", "slug": "owlbear", "name": "Owlbear", "view": "all", "edit": "gm", "fields": {},
                "parts": [{ "section": "story", "title": "", "text": "Feathers and claws.\n<image src=img:owl>", "fields": {}, "view": "all", "edit": "gm", "level": "open" }] }],
            "images": [{ "id": "img:owl", "name": "Owlbear", "file": "images/owl.png" }],
        });
        let files = serde_json::json!({ "images/owl.png": base64::engine::general_purpose::STANDARD.encode(&png) });
        let sent = serde_json::json!({ "world": world, "package": package, "files": files });
        let summary = ask(request("POST", "/v1/import", None, Some(&target), sent)).unwrap();
        assert_eq!((summary["documents_new"].as_u64(), summary["images_new"].as_u64()), (Some(1), Some(1)));
        assert!(told.borrow().contains(&"browser-imported".to_string()));
        assert!(store.db(&world).unwrap().document(page).unwrap().is_some());
        assert!(!store.profile_dir().join("tmp").read_dir().map(|mut d| d.next().is_some()).unwrap_or(false), "nothing left behind");

        // Files only inside the package.
        let escape = serde_json::json!({ "world": world, "package": package, "files": { "../outside.png": "AAAA" } });
        assert_eq!(ask(request("POST", "/v1/import", None, Some(&target), escape)).unwrap_err().0, 400);
        assert!(!store.profile_dir().join("tmp").join("outside.png").exists());
        // A world it does not lead: no.
        let other = serde_json::json!({ "world": "world:0000000000000000", "package": package, "files": {} });
        assert_eq!(ask(request("POST", "/v1/import", None, Some(&target), other)).unwrap_err().0, 403);
    }

    #[test]
    fn query_values_are_decoded_once_and_bad_ones_refused() {
        assert_eq!(query_value("world=world%3Aabc&other=x", "world").unwrap(), "world:abc");
        assert_eq!(query_value("world=world%253Aabc", "world").unwrap(), "world%3Aabc");
        assert_eq!(query_value("world=A+B%2BC", "world").unwrap(), "A B+C");
        assert_eq!(query_value("world=%E2%9C%93", "world").unwrap(), "✓");
        for value in ["world=%", "world=%0", "world=%zz", "world=%ff"] {
            assert_eq!(query_value(value, "world").unwrap_err().0, 400);
        }
    }

    #[test]
    fn a_player_sees_no_world_and_another_profile_is_another_app() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("local"));
        let me = Identity::create("Tim").unwrap();
        store.save_identity(&me).unwrap();
        let gm = Identity::create("Marla").unwrap();
        let world = store.create_world("Marla's world", &gm).unwrap().id;
        save(&store, &Settings { enabled: true }).unwrap();
        let target = instance_id(&store);
        let tell = |_: &str, _: serde_json::Value| {};
        let ask = |store: &Store, path: &str| route(store, &tell, &request("GET", path, None, Some(&target), serde_json::Value::Null));
        assert!(ask(&store, "/v1/worlds").unwrap().as_array().unwrap().is_empty());
        assert_eq!(ask(&store, &format!("/v1/types?world={}", world.replace(':', "%3A"))).unwrap_err().0, 403);
        let second = Store::new(dir.path().join("second"));
        second.save_identity(&me).unwrap();
        save(&second, &Settings { enabled: true }).unwrap();
        assert_eq!(ask(&second, "/v1/worlds").unwrap_err().0, 409);
    }
}
