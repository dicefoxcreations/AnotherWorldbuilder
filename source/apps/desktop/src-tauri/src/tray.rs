//! AW Server on Linux and Windows: the same program, started with
//! `--server` — a symbol in the tray, a small window to set it up, and the
//! dedicated server of `dedicated` running on while the window is closed.
//! `--hidden` starts it without the window, from the login: the server comes
//! back up if it was running when the computer went down.
//!
//! Its settings and its folder are its own, beside the app's but not the
//! app's profile: it has its own identity, as on a NAS.

use super::*;
use crate::dedicated;
use crate::server::ServerOptions;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, WebviewUrl, WebviewWindowBuilder};

const SETTINGS: &str = "server.json";
/// AW Server's own symbol — the app's with a server badge (DiceFox brand/aw-server/v1).
const ICON: &[u8] = include_bytes!("../icons/aw-server.png");
const WINDOW: &str = "server";

/// What is set for it. `running`: it was running when last left — so it
/// comes back up after a restart of the computer.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    admin: String,
    name: String,
    port: u16,
    addresses: Vec<String>,
    running: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { admin: String::new(), name: "My table".into(), port: aw_sync::world::DEFAULT_PORT, addresses: Vec::new(), running: false }
    }
}

/// What its window shows.
#[derive(Serialize)]
pub struct View {
    settings: Settings,
    status: dedicated::Status,
    folder: String,
    autostart: bool,
    /// The ID of the app's profile on this computer — the admin, mostly.
    own: Option<String>,
}

fn config(app: &tauri::AppHandle) -> Answer<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

fn folder(app: &tauri::AppHandle) -> Answer<PathBuf> {
    Ok(app.path().app_data_dir()?.join("server"))
}

fn load(app: &tauri::AppHandle) -> Settings {
    config(app).ok().and_then(|dir| std::fs::read(dir.join(SETTINGS)).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(app: &tauri::AppHandle, s: &Settings) -> Answer<()> {
    let dir = config(app)?;
    std::fs::create_dir_all(&dir)?;
    aw_store::durable::write(&dir.join(SETTINGS), &serde_json::to_vec_pretty(s)?)?;
    Ok(())
}

fn options(app: &tauri::AppHandle, s: &Settings) -> Answer<ServerOptions> {
    let mut o = ServerOptions {
        admin: s.admin.clone(),
        data: folder(app)?,
        port: s.port,
        name: s.name.clone(),
        addresses: s.addresses.clone(),
        // A server is there to be found: on the local network and from outside.
        lan: true,
        internet: true,
    };
    o.validate().map_err(AppError)?;
    Ok(o)
}

fn view(app: &tauri::AppHandle) -> Answer<View> {
    Ok(View { settings: load(app), status: dedicated::status(), folder: folder(app)?.display().to_string(), autostart: autostart::is_on(), own: own_id(app) })
}

/// Who uses the app here, read from its profile — none before it is set up.
fn own_id(app: &tauri::AppHandle) -> Option<String> {
    let at: profile::Locations = serde_json::from_slice(&std::fs::read(profile::config_dir(app).ok()?.join("locations.json")).ok()?).ok()?;
    Some(Identity::load(&at.profile.join("identity.json")).ok()?.public().id.to_string())
}

/// Starts it and remembers that it runs.
fn start(app: &tauri::AppHandle) -> Answer<()> {
    let mut s = load(app);
    dedicated::start(options(app, &s)?).map_err(AppError)?;
    s.running = true;
    save(app, &s)?;
    Ok(())
}

/// Stops it and waits until it has let go of its port and folder.
fn stop(app: &tauri::AppHandle, remember: bool) {
    dedicated::stop();
    while !dedicated::finished() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if remember {
        let mut s = load(app);
        s.running = false;
        let _ = save(app, &s);
    }
    tell(app);
}

/// The tray's menu: its state, start or stop, starting with the computer.
struct TrayMenu {
    state: MenuItem<tauri::Wry>,
    toggle: MenuItem<tauri::Wry>,
    autostart: CheckMenuItem<tauri::Wry>,
}

/// The window and the tray learn of each change of state.
fn tell(app: &tauri::AppHandle) {
    let status = dedicated::status();
    let _ = app.emit("server-status", &status);
    let line = match status.state {
        "running" => "Running",
        "starting" => "Starting…",
        "stopping" => "Stopping…",
        "failed" => "Stopped by an error",
        _ => "Stopped",
    };
    if let Some(tray) = app.tray_by_id("server") {
        let _ = tray.set_tooltip(Some(format!("AW Server — {}", line.to_lowercase())));
    }
    if let Some(menu) = app.try_state::<TrayMenu>() {
        let _ = menu.state.set_text(line);
        let on = matches!(status.state, "running" | "starting");
        let _ = menu.toggle.set_text(if on { "Stop server" } else { "Start server" });
        let _ = menu.toggle.set_enabled(status.state != "stopping" && !load(app).admin.is_empty());
        let _ = menu.autostart.set_checked(autostart::is_on());
    }
}

/// What the tray's menu was asked.
fn on_menu(app: &tauri::AppHandle, id: &str) {
    match id {
        "open" => show(app),
        "toggle" => {
            let app = app.clone();
            // Stopping waits for the port: not on the thread that draws the menu.
            std::thread::spawn(move || {
                if matches!(dedicated::status().state, "running" | "starting") {
                    stop(&app, true);
                } else if let Err(e) = start(&app) {
                    applog::line("server", &e.0);
                    show(&app);
                }
                tell(&app);
            });
        }
        "autostart" => {
            if let Err(e) = autostart::set(!autostart::is_on()) {
                applog::line("server", &e.0);
            }
            tell(app);
        }
        "quit" => {
            stop(app, false);
            app.exit(0);
        }
        _ => {}
    }
}

/// Follows the server for as long as the program runs: each change of its
/// state — also one it comes to by itself, failing — goes to window and tray.
fn watch(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = "";
        loop {
            let state = dedicated::status().state;
            if state != last {
                tell(&app);
                last = state;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });
}

fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window(WINDOW) {
        // Open but behind others: a window may not raise itself under
        // Wayland — shown anew, it comes to the front.
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        }
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html?window=server".into()));
    // Its own symbol in the task bar, apart from the app's.
    let window = match tauri::image::Image::from_bytes(ICON).and_then(|icon| window.icon(icon)) {
        Ok(with) => with,
        Err(_) => WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html?window=server".into())),
    };
    let _ = window
        .title("AW Server")
        .inner_size(560.0, 720.0)
        .min_inner_size(420.0, 420.0)
        .build();
}

// --- What the window asks -------------------------------------------------------

#[tauri::command(async)]
fn server_view(app: tauri::AppHandle) -> Answer<View> {
    view(&app)
}

/// Saves the settings; a running server starts anew with them.
#[tauri::command(async)]
fn server_save(app: tauri::AppHandle, settings: Settings) -> Answer<View> {
    let mut next = settings;
    let was = load(&app);
    next.running = was.running;
    options(&app, &next)?;
    save(&app, &next)?;
    if matches!(dedicated::status().state, "running" | "starting") {
        stop(&app, false);
        start(&app)?;
    }
    tell(&app);
    view(&app)
}

#[tauri::command(async)]
fn server_start(app: tauri::AppHandle) -> Answer<View> {
    start(&app)?;
    view(&app)
}

#[tauri::command(async)]
fn server_stop(app: tauri::AppHandle) -> Answer<View> {
    stop(&app, true);
    view(&app)
}

#[tauri::command(async)]
fn server_worlds(app: tauri::AppHandle) -> Answer<Vec<dedicated::Kept>> {
    dedicated::worlds(&folder(&app)?).map_err(AppError)
}

#[tauri::command(async)]
fn server_folder_open(app: tauri::AppHandle) -> Answer<()> {
    let at = folder(&app)?;
    std::fs::create_dir_all(&at)?;
    profile::reveal(&at)
}

#[tauri::command(async)]
fn server_autostart(app: tauri::AppHandle, on: bool) -> Answer<View> {
    autostart::set(on)?;
    tell(&app);
    view(&app)
}

/// Opens AW Server from the app: its own process — or, if it runs already,
/// its window (one at a time). An AppImage, which brings no menu entry of its
/// own, gets one for AW Server now, so that it is found there next time.
pub fn launch(app: &tauri::AppHandle) -> Answer<()> {
    let exe = autostart::program()?;
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_some() {
        if let Err(e) = menu_entry(app, &exe) {
            applog::line("server", &format!("no menu entry: {}", e.0));
        }
    }
    let _ = app;
    std::process::Command::new(&exe).arg("--server").spawn().map_err(|e| AppError(format!("AW Server did not start: {e}")))?;
    Ok(())
}

/// An AppImage brings no menu entries of its own: each start of the app
/// writes AW Server's — pointing at the AppImage where it lies now — and the
/// app's own where there is none yet.
#[cfg(target_os = "linux")]
pub fn appimage_entries(app: &tauri::AppHandle) {
    if std::env::var_os("APPIMAGE").is_none() {
        return;
    }
    let written = autostart::program().and_then(|exe| {
        menu_entry(app, &exe)?;
        let share = app.path().data_dir()?;
        let entry = share.join("applications/anotherworldbuilder.desktop");
        if !entry.exists() {
            let icon = share.join("icons/anotherworldbuilder.png");
            std::fs::create_dir_all(icon.parent().unwrap())?;
            std::fs::write(&icon, include_bytes!("../icons/icon.png"))?;
            std::fs::write(
                entry,
                format!(
                    "[Desktop Entry]\nType=Application\nName=AnotherWorldbuilder\nComment=Worldbuilding and campaign wiki\nExec=\"{}\"\nIcon={}\nStartupWMClass=aw-desktop\nCategories=Office;\nTerminal=false\n",
                    exe.display(),
                    icon.display()
                ),
            )?;
        }
        Ok(())
    });
    if let Err(e) = written {
        applog::line("app", &format!("no menu entries: {}", e.0));
    }
}

/// `~/.local/share/applications/aw-server.desktop`, with its symbol beside it.
#[cfg(target_os = "linux")]
fn menu_entry(app: &tauri::AppHandle, exe: &Path) -> Answer<()> {
    let share = app.path().data_dir()?;
    let icon = share.join("icons/aw-server.png");
    std::fs::create_dir_all(icon.parent().unwrap())?;
    std::fs::write(&icon, ICON)?;
    let entry = share.join("applications/aw-server.desktop");
    std::fs::create_dir_all(entry.parent().unwrap())?;
    std::fs::write(
        entry,
        format!(
            "[Desktop Entry]\nType=Application\nName=AW Server\nComment=AnotherWorldbuilder's server for your group\nExec=\"{}\" --server\nIcon={}\nCategories=Network;\nTerminal=false\n",
            exe.display(),
            icon.display()
        ),
    )?;
    Ok(())
}

/// Runs AW Server — instead of the app, in a process of its own.
pub fn run(hidden: bool) {
    let mut context = tauri::generate_context!();
    // Not the app's window: its own, made below when it is wanted.
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .setup(move |app| {
            let handle = app.handle().clone();
            applog::set_fallback(folder(&handle).map_err(|e| e.0)?);
            let state = MenuItem::with_id(app, "state", "Stopped", false, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "Start server", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(app, "autostart", "Start with this computer", true, autostart::is_on(), None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let line = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&state, &toggle, &line, &autostart, &open, &PredefinedMenuItem::separator(app)?, &quit])?;
            app.manage(TrayMenu { state, toggle, autostart });
            TrayIconBuilder::with_id("server")
                .icon(tauri::image::Image::from_bytes(ICON)?)
                .tooltip("AW Server")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show(tray.app_handle());
                    }
                })
                .build(app)?;
            watch(&handle);
            // Back up after the computer started, if it was running then.
            if load(&handle).running {
                if let Err(e) = start(&handle) {
                    applog::line("server", &e.0);
                }
            }
            if !hidden || load(&handle).admin.is_empty() {
                show(&handle);
            }
            Ok(())
        })
        // Closing the window leaves the server running, in the tray.
        .on_window_event(|w, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            server_view,
            server_save,
            server_start,
            server_stop,
            server_worlds,
            server_folder_open,
            server_autostart,
            applog::app_log,
        ])
        .run(context)
        .expect("AW Server could not start");
}

/// Starting with the login, without the window.
mod autostart {
    use super::*;

    /// The program to start again: an AppImage by its own file, not the
    /// one unpacked for this run.
    pub fn program() -> Answer<PathBuf> {
        Ok(std::env::var_os("APPIMAGE").map(PathBuf::from).map(Ok).unwrap_or_else(std::env::current_exe)?)
    }

    fn command() -> Answer<String> {
        Ok(format!("\"{}\" --server --hidden", program()?.display()))
    }

    #[cfg(target_os = "linux")]
    fn entry() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(config.join("autostart").join("aw-server.desktop"))
    }

    #[cfg(target_os = "linux")]
    pub fn is_on() -> bool {
        entry().is_some_and(|p| p.exists())
    }

    #[cfg(target_os = "linux")]
    pub fn set(on: bool) -> Answer<()> {
        let path = entry().ok_or_else(|| AppError("No home folder.".into()))?;
        if !on {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        std::fs::create_dir_all(path.parent().unwrap())?;
        let text = format!("[Desktop Entry]\nType=Application\nName=AW Server\nExec={}\nIcon=anotherworldbuilder\nX-GNOME-Autostart-enabled=true\n", command()?);
        std::fs::write(path, text)?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    #[cfg(target_os = "windows")]
    fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        std::process::Command::new("reg").args(args).creation_flags(0x0800_0000).output()
    }

    #[cfg(target_os = "windows")]
    pub fn is_on() -> bool {
        reg(&["query", KEY, "/v", "AW Server"]).is_ok_and(|o| o.status.success())
    }

    #[cfg(target_os = "windows")]
    pub fn set(on: bool) -> Answer<()> {
        let out = if on {
            reg(&["add", KEY, "/v", "AW Server", "/t", "REG_SZ", "/d", &command()?, "/f"])?
        } else {
            reg(&["delete", KEY, "/v", "AW Server", "/f"])?
        };
        if on && !out.status.success() {
            return Err(AppError(String::from_utf8_lossy(&out.stderr).trim().to_string()));
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub fn is_on() -> bool {
        false
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub fn set(_: bool) -> Answer<()> {
        Err(AppError("Not on this system.".into()))
    }
}
