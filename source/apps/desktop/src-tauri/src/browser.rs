//! The browser's door: an extension in this device's own browser hands the
//! app pages to take into a world.
//!
//! Closed until the user opens it in the options. Open, the app listens on
//! this device's loopback address only — nothing from the network reaches
//! it — and answers browser extensions alone: a web page cannot get in, nor
//! a site posing under another name for this address. An extension pairs
//! once, with a code the app shows for a few minutes; from then on it
//! carries a token. It sees the worlds this device leads (only a game master
//! takes packages in) and sends packages; the app takes them in as from a
//! folder (`transfer::import`).

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

/// Where extensions knock — fixed, so that they find the app.
pub const PORT: u16 = 47823;
/// A package with its pictures — larger ones the extension sends in parts.
const LARGEST: usize = 256 * 1024 * 1024;
const CODE_FOR: Duration = Duration::from_secs(5 * 60);
/// Wrong guesses before a code is gone — six digits are not to be tried through.
const GUESSES: u8 = 5;
const FILE: &str = "browser.json";

/// What the profile keeps: whether the door is open, and the browsers paired.
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub enabled: bool,
    #[serde(default)]
    pub browsers: Vec<Paired>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Paired {
    pub id: String,
    /// As the extension names itself.
    pub name: String,
    /// Milliseconds since 1970.
    pub since: u64,
    /// The token's hash — the token itself only the extension keeps.
    pub hash: String,
}

/// Open now, and the code shown to pair with.
struct Door {
    listening: Option<Arc<AtomicBool>>,
    /// The code, since when, and the wrong guesses at it.
    code: Option<(String, Instant, u8)>,
    problem: Option<String>,
}

static DOOR: Mutex<Door> = Mutex::new(Door { listening: None, code: None, problem: None });
/// One package at a time.
static TAKING: Mutex<()> = Mutex::new(());

fn load(store: &Store) -> Settings {
    std::fs::read(store.profile_dir().join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(store: &Store, s: &Settings) -> Answer<()> {
    std::fs::create_dir_all(store.profile_dir())?;
    aw_store::durable::write(&store.profile_dir().join(FILE), &serde_json::to_vec_pretty(s)?)?;
    Ok(())
}

fn hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
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
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(listener) => {
            let on = Arc::new(AtomicBool::new(true));
            door.listening = Some(on.clone());
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
        Err(e) => door.problem = Some(format!("Port {PORT} is taken: {e}")),
    }
}

fn close() {
    let mut door = DOOR.lock().unwrap();
    if let Some(on) = door.listening.take() {
        on.store(false, Ordering::SeqCst);
        // The loop waits for a caller: one last knock lets it see it is closed.
        let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_millis(300));
    }
    door.code = None;
}

// --- HTTP, as little of it as a browser needs ---------------------------------

struct Request {
    method: String,
    path: String,
    host: String,
    origin: Option<String>,
    token: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> std::io::Result<Request> {
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or("").to_string();
    let path = words.next().unwrap_or("").to_string();
    let (mut host, mut origin, mut token, mut length) = (String::new(), None, None, 0usize);
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
            "authorization" => token = value.strip_prefix("Bearer ").map(str::to_string),
            "content-length" => length = value.parse().unwrap_or(usize::MAX),
            _ => {}
        }
    }
    if length > LARGEST {
        return Err(std::io::Error::other("too large"));
    }
    let mut body = vec![0; length];
    r.read_exact(&mut body)?;
    Ok(Request { method, path, host, origin, token, body })
}

/// Browser extensions only: their addresses are theirs alone.
fn from_extension(origin: &str) -> bool {
    ["chrome-extension://", "moz-extension://", "safari-web-extension://"].iter().any(|p| origin.starts_with(p))
}

/// Whether a request may come in — and the extension's address to answer, if
/// a browser sent it. Another name for this address (a site rebinding its
/// own) or a web page: no.
fn admitted(r: &Request) -> Option<Option<String>> {
    let local = [format!("127.0.0.1:{PORT}"), format!("localhost:{PORT}")].contains(&r.host);
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
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", text.len());
    if let Some(o) = origin {
        head.push_str(&format!(
            "Access-Control-Allow-Origin: {o}\r\nAccess-Control-Allow-Headers: authorization, content-type\r\nAccess-Control-Allow-Methods: GET, POST\r\nVary: Origin\r\n"
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

fn route(store: &Store, tell: Tell, r: &Request) -> Routed {
    let settings = load(store);
    let paired = r.token.as_deref().map(hash).and_then(|h| settings.browsers.iter().find(|b| b.hash == h).cloned());
    let (path, query) = r.path.split_once('?').unwrap_or((&r.path, ""));
    let world = query.split('&').find_map(|kv| kv.strip_prefix("world=")).unwrap_or("");
    match (r.method.as_str(), path) {
        ("GET", "/v1/hello") => Ok(serde_json::json!({
            "app": "AnotherWorldbuilder",
            "version": env!("CARGO_PKG_VERSION"),
            "paired": paired.is_some(),
        })),
        ("POST", "/v1/pair") => pair(store, tell, settings, &r.body),
        (_, _) if paired.is_none() => Err((401, "Not paired.".into())),
        ("GET", "/v1/worlds") => worlds(store),
        ("GET", "/v1/types") => types(store, world),
        ("POST", "/v1/import") => take(store, tell, paired.as_ref().map(|p| p.name.as_str()).unwrap_or(""), &r.body),
        _ => Err((404, "No such thing.".into())),
    }
}

fn pair(store: &Store, tell: Tell, mut settings: Settings, body: &[u8]) -> Routed {
    #[derive(Deserialize)]
    struct Ask {
        code: String,
        #[serde(default)]
        name: String,
    }
    let ask: Ask = serde_json::from_slice(body).map_err(|e| (400, e.to_string()))?;
    {
        let mut door = DOOR.lock().unwrap();
        let Some((code, _, wrong)) = door.code.as_mut().filter(|(_, since, _)| since.elapsed() < CODE_FOR) else {
            return Err((403, "No code shown — make one in the app.".into()));
        };
        if *code != ask.code.trim() {
            *wrong += 1;
            if *wrong >= GUESSES {
                door.code = None;
            }
            return Err((403, "Wrong code.".into()));
        }
        door.code = None;
    }
    let token = hex::encode(aw_crypto::random_bytes::<32>().map_err(fail)?);
    let name = if ask.name.trim().is_empty() { "Browser".to_string() } else { ask.name.trim().chars().take(60).collect() };
    settings.browsers.push(Paired { id: hex::encode(aw_crypto::random_bytes::<8>().map_err(fail)?), name: name.clone(), since: now_ms(), hash: hash(&token) });
    save(store, &settings).map_err(|e| (500, e.0))?;
    tell("browser-paired", serde_json::json!(name));
    Ok(serde_json::json!({ "token": token }))
}

fn worlds(store: &Store) -> Routed {
    let mut led: Vec<serde_json::Value> = store
        .list_worlds()
        .map_err(fail)?
        .into_iter()
        .filter(|m| store.is_gm(m.public.world_id.as_str()))
        .map(|m| serde_json::json!({ "id": m.public.world_id.to_string(), "name": m.public.name, "icon": m.icon, "members": m.members.len() }))
        .collect();
    for cached in network::online_worlds(store) {
        if store.is_gm(&cached.world) && !led.iter().any(|w| w["id"] == cached.world) {
            led.push(serde_json::json!({ "id": cached.world, "name": cached.info["name"], "icon": cached.info["icon"],
                "members": cached.info["members"].as_array().map_or(0, Vec::len) }));
        }
    }
    Ok(serde_json::Value::Array(led))
}

/// Browser imports use the same acknowledged online command channel as
/// the app. The HTTP listener runs on its own thread, outside Tokio.
fn authorized<R>(store: &Store, world: &str, run: impl FnOnce(&Store) -> Answer<R>) -> Answer<R> {
    if !online::requires_online(store, world) { return run(store) }
    let server = online::server_for(store, world)?;
    if store.holds(world) && !network::online_worlds(store).iter().any(|w| w.world == world) {
        return Err(AppError("[online] This world is still being uploaded.".into()));
    }
    let net = network::running().ok_or_else(|| AppError(online::UNAVAILABLE.into()))?;
    let me = store.load_identity()?;
    let connection = online::Connection { net: &net, server: &server, me: &me };
    tauri::async_runtime::block_on(online::execute(store, world, &server, &connection,
        |cache| std::future::ready(run(&cache))))
}

/// A world's types — so that a package brings only those it lacks, and the
/// world's own stay as its game master made them.
fn types(store: &Store, world: &str) -> Routed {
    if !store.is_gm(world) {
        return Err((403, "Only the GM's worlds.".into()));
    }
    serde_json::to_value(authorized(store, world, |cache| Ok(cache.load_templates(world)?)).map_err(|e| (500, e.0))?).map_err(fail)
}

/// A package from the browser: its `package.json` and its files, the files
/// base64 under the paths the package names.
fn take(store: &Store, tell: Tell, from: &str, body: &[u8]) -> Routed {
    use base64::Engine;
    #[derive(Deserialize)]
    struct Sent {
        world: String,
        package: serde_json::Value,
        #[serde(default)]
        files: BTreeMap<String, String>,
    }
    let sent: Sent = serde_json::from_slice(body).map_err(|e| (400, e.to_string()))?;
    if !store.is_gm(&sent.world) {
        return Err((403, "Only the GM imports.".into()));
    }
    let _one = TAKING.lock().unwrap();
    let folder = store.profile_dir().join("tmp").join(format!("browser-{}", hex::encode(aw_crypto::random_bytes::<6>().map_err(fail)?)));
    let result = (|| -> Routed {
        std::fs::create_dir_all(&folder).map_err(fail)?;
        std::fs::write(folder.join("package.json"), serde_json::to_vec(&sent.package).map_err(fail)?).map_err(fail)?;
        for (name, data) in &sent.files {
            let path = Path::new(name);
            // Inside the package's folder, nowhere else.
            if path.is_absolute() || path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                return Err((400, format!("Not a file of the package: {name}")));
            }
            let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| (400, e.to_string()))?;
            let target = folder.join(path);
            std::fs::create_dir_all(target.parent().unwrap_or(&folder)).map_err(fail)?;
            std::fs::write(target, bytes).map_err(fail)?;
        }
        let (dir, package) = transfer::read(&folder).map_err(|e| (400, e.0))?;
        let me = store.load_identity().map_err(fail)?;
        let progress = |done: usize, total: usize| tell("package-progress", serde_json::json!([done, total]));
        let summary = authorized(store, &sent.world, |cache| transfer::import(cache, &me, &sent.world, &dir, &package, &progress)).map_err(|e| {
            if e.0.contains("[online]") {
                tell("online-unavailable", serde_json::json!({ "world": sent.world, "error": e.0 }));
            }
            (400, e.0)
        })?;
        tell("browser-imported", serde_json::json!({ "world": sent.world, "from": from, "name": summary.name, "documents": summary.documents_new }));
        serde_json::to_value(&summary).map_err(fail)
    })();
    let _ = std::fs::remove_dir_all(&folder);
    result
}

// --- Commands ---------------------------------------------------------------

/// The door as the options show it.
#[derive(Serialize)]
pub struct View {
    pub enabled: bool,
    pub open: bool,
    pub port: u16,
    pub problem: Option<String>,
    /// The code to type into the extension, while it holds.
    pub code: Option<String>,
    pub browsers: Vec<BrowserView>,
}

#[derive(Serialize)]
pub struct BrowserView {
    pub id: String,
    pub name: String,
    pub since: u64,
}

fn view(store: &Store) -> View {
    let s = load(store);
    let door = DOOR.lock().unwrap();
    View {
        enabled: s.enabled,
        open: door.listening.is_some(),
        port: PORT,
        problem: door.problem.clone(),
        code: door.code.as_ref().filter(|(_, since, _)| since.elapsed() < CODE_FOR).map(|(c, _, _)| c.clone()),
        browsers: s.browsers.iter().map(|b| BrowserView { id: b.id.clone(), name: b.name.clone(), since: b.since }).collect(),
    }
}

#[tauri::command]
pub fn browser_settings(app: tauri::AppHandle) -> Answer<View> {
    Ok(view(&store(&app)?))
}

#[tauri::command]
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

/// A new code to pair a browser with — six digits, for five minutes.
#[tauri::command]
pub fn browser_pair(app: tauri::AppHandle) -> Answer<View> {
    let store = store(&app)?;
    let n = u32::from_le_bytes(aw_crypto::random_bytes::<4>()?) % 1_000_000;
    DOOR.lock().unwrap().code = Some((format!("{n:06}"), Instant::now(), 0));
    Ok(view(&store))
}

#[tauri::command]
pub fn browser_forget(app: tauri::AppHandle, id: String) -> Answer<View> {
    let store = store(&app)?;
    let mut s = load(&store);
    s.browsers.retain(|b| b.id != id);
    save(&store, &s)?;
    Ok(view(&store))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn request(method: &str, path: &str, origin: Option<&str>, token: Option<&str>, body: serde_json::Value) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            host: format!("127.0.0.1:{PORT}"),
            origin: origin.map(str::to_string),
            token: token.map(str::to_string),
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
    }

    #[test]
    fn a_browser_pairs_with_the_shown_code_and_then_takes_packages_into_a_world_it_leads() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("gm"));
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let world = new_world(&store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        let told = RefCell::new(Vec::<String>::new());
        let tell = |event: &str, _: serde_json::Value| told.borrow_mut().push(event.to_string());
        let ask = |r: Request| route(&store, &tell, &r);

        // Without a code shown, or with a wrong one: no.
        let code = |c: &str| serde_json::json!({ "code": c, "name": "Chrome" });
        assert_eq!(ask(request("POST", "/v1/pair", None, None, code("123456"))).unwrap_err().0, 403);
        DOOR.lock().unwrap().code = Some(("424242".into(), Instant::now(), 0));
        for _ in 0..GUESSES {
            assert_eq!(ask(request("POST", "/v1/pair", None, None, code("000000"))).unwrap_err().0, 403);
        }
        // Guessed at too often, the code is gone — the right one no longer helps.
        assert_eq!(ask(request("POST", "/v1/pair", None, None, code("424242"))).unwrap_err().0, 403);
        DOOR.lock().unwrap().code = Some(("424242".into(), Instant::now(), 0));
        let token = ask(request("POST", "/v1/pair", None, None, code("424242"))).unwrap()["token"].as_str().unwrap().to_string();
        assert!(DOOR.lock().unwrap().code.is_none(), "a code pairs once");
        assert!(!std::fs::read_to_string(store.profile_dir().join(FILE)).unwrap().contains(&token), "the profile keeps no token");

        // Unpaired, nothing but hello.
        assert_eq!(ask(request("GET", "/v1/worlds", None, None, serde_json::Value::Null)).unwrap_err().0, 401);
        assert_eq!(ask(request("GET", "/v1/worlds", None, Some("feed"), serde_json::Value::Null)).unwrap_err().0, 401);
        assert_eq!(ask(request("GET", "/v1/hello", None, Some(&token), serde_json::Value::Null)).unwrap()["paired"], true);
        let worlds = ask(request("GET", "/v1/worlds", None, Some(&token), serde_json::Value::Null)).unwrap();
        assert_eq!(worlds[0]["id"], world.as_str());
        let types = ask(request("GET", &format!("/v1/types?world={world}"), None, Some(&token), serde_json::Value::Null)).unwrap();
        assert!(types.is_array());
        assert_eq!(ask(request("GET", "/v1/types?world=world:0000000000000000", None, Some(&token), serde_json::Value::Null)).unwrap_err().0, 403);

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
        let summary = ask(request("POST", "/v1/import", None, Some(&token), sent)).unwrap();
        assert_eq!((summary["documents_new"].as_u64(), summary["images_new"].as_u64()), (Some(1), Some(1)));
        assert!(told.borrow().contains(&"browser-imported".to_string()));
        assert!(store.load_articles(&world).unwrap().iter().any(|a| a.id.to_string() == page));
        assert!(!store.profile_dir().join("tmp").read_dir().map(|mut d| d.next().is_some()).unwrap_or(false), "nothing left behind");

        // Files only inside the package.
        let escape = serde_json::json!({ "world": world, "package": package, "files": { "../outside.png": "AAAA" } });
        assert_eq!(ask(request("POST", "/v1/import", None, Some(&token), escape)).unwrap_err().0, 400);
        assert!(!store.profile_dir().join("tmp").join("outside.png").exists());
        // A world it does not lead: no.
        let other = serde_json::json!({ "world": "world:0000000000000000", "package": package, "files": {} });
        assert_eq!(ask(request("POST", "/v1/import", None, Some(&token), other)).unwrap_err().0, 403);
    }
}
