//! Size and position of the window.
//!
//! Sounds like a small thing, but is not: under Wayland WebKitGTK reports a
//! different size than the one that was set. The window decoration is drawn
//! client-side and brings an invisible shadow margin along — measured here
//! at 90 × 138 pixels. Whoever calls `set_size(1200 × 800)` gets back
//! `inner_size()` = 1290 × 938.
//!
//! Nobody notices until someone stores the reported size and sets it again
//! on the next start: then the window grows by exactly this margin on
//! *every* start until it disappears under the taskbar. That is exactly what
//! `tauri-plugin-window-state` does, and exactly why this module handles the
//! size itself — the plugin still takes care of position and state.
//!
//! The margin is not hard-coded but measured on every start: set once, read
//! back once, remember the difference. On systems without this effect —
//! X11, Windows, macOS — the result is zero, and everything below does
//! nothing.

use serde::{Deserialize, Serialize};
#[cfg(feature = "gui")]
use tauri::{Manager, PhysicalSize};

/// Desired size of a new window, in pixels.
///
/// Deliberately modest: it has to fit on a laptop screen minus a panel too.
pub const DEFAULT: (u32, u32) = (1200, 800);

/// A rectangle in screen pixels: top-left corner, then width and height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The size last used — the *visible* one, without the shadow margin.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Remembered {
    width: u32,
    height: u32,
}

#[cfg(feature = "gui")]
fn file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let folder = match crate::profile::profile_root() {
        Some(root) => root.join("window"),
        None => app.path().app_config_dir().ok()?,
    };
    std::fs::create_dir_all(&folder).ok()?;
    Some(folder.join("window-size.json"))
}

#[cfg(feature = "gui")]
fn read_remembered(app: &tauri::AppHandle) -> Option<(u32, u32)> {
    let text = std::fs::read_to_string(file(app)?).ok()?;
    let value: Remembered = serde_json::from_str(&text).ok()?;
    (value.width > 0 && value.height > 0).then_some((value.width, value.height))
}

#[cfg(feature = "gui")]
fn write_remembered(app: &tauri::AppHandle, size: (u32, u32)) {
    let Some(path) = file(app) else { return };
    let content = Remembered {
        width: size.0,
        height: size.1,
    };
    if let Ok(text) = serde_json::to_string_pretty(&content) {
        let _ = std::fs::write(path, text);
    }
}

/// Fits a window into the usable area.
///
/// `area` is the screen without the desktop's panels. What comes back is the
/// geometry that fits inside: first the size is capped, then the window is
/// pushed fully inside.
///
/// A function of its own, because only so can the cases be tested that go
/// wrong in everyday use: a screen smaller than the desired size, and a
/// second screen to the left of the first whose coordinates are negative.
pub fn fitted(area: Rect, window: Rect) -> Rect {
    let width = window.width.clamp(1, area.width.max(1));
    let height = window.height.clamp(1, area.height.max(1));

    // `max` after the computation: were the window wider than the area, the
    // right bound would otherwise lie left of the left one.
    let right = (area.x + area.width as i32 - width as i32).max(area.x);
    let bottom = (area.y + area.height as i32 - height as i32).max(area.y);

    Rect {
        x: window.x.clamp(area.x, right),
        y: window.y.clamp(area.y, bottom),
        width,
        height,
    }
}

#[cfg(feature = "gui")]
/// The usable area of the screen the window is on.
fn usable_area(window: &tauri::WebviewWindow) -> Option<Rect> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    Some(Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    })
}

#[cfg(feature = "gui")]
/// Sets the size and measures the invisible margin while doing so.
///
/// The return value is the difference between what is reported and what was
/// set — that is, the decoration's shadow margin.
fn set_and_measure(window: &tauri::WebviewWindow, size: (u32, u32)) -> (u32, u32) {
    if window
        .set_size(PhysicalSize::new(size.0, size.1))
        .is_err()
    {
        return (0, 0);
    }
    // The window manager does not answer immediately.
    std::thread::sleep(std::time::Duration::from_millis(120));
    match window.inner_size() {
        Ok(reported) => (
            reported.width.saturating_sub(size.0),
            reported.height.saturating_sub(size.1),
        ),
        Err(_) => (0, 0),
    }
}

#[cfg(feature = "gui")]
/// Brings the window to the remembered size and remembers the shadow margin.
///
/// Runs delayed and on a thread of its own: directly in `setup` the window
/// is not up yet and reports 0 × 0.
pub fn on_start(app: tauri::AppHandle, window: tauri::WebviewWindow) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(350));

        if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
            return;
        }
        let Ok(now) = window.inner_size() else { return };
        if now.width == 0 || now.height == 0 {
            return; // The window is not up yet — better do nothing then.
        }

        let wanted = read_remembered(&app).unwrap_or(DEFAULT);
        let position = window
            .outer_position()
            .map(|p| (p.x, p.y))
            .unwrap_or((0, 0));

        let fitting = match usable_area(&window) {
            Some(area) => fitted(
                area,
                Rect {
                    x: position.0,
                    y: position.1,
                    width: wanted.0,
                    height: wanted.1,
                },
            ),
            None => Rect {
                x: position.0,
                y: position.1,
                width: wanted.0,
                height: wanted.1,
            },
        };

        debug_log(&format!(
            "start: reported {}×{}, wanted {}×{}, usable {:?}",
            now.width,
            now.height,
            wanted.0,
            wanted.1,
            usable_area(&window).map(|f| (f.width, f.height)),
        ));

        let margin = set_and_measure(&window, (fitting.width, fitting.height));
        MARGIN.store(
            ((margin.0 as u64) << 32) | margin.1 as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        debug_log(&format!(
            "set {}×{}, measured shadow margin {}×{}",
            fitting.width, fitting.height, margin.0, margin.1
        ));

        if (fitting.x, fitting.y) != position {
            let _ = window.set_position(tauri::PhysicalPosition::new(fitting.x, fitting.y));
        }
    });
}

/// The measured shadow margin, as two u32 in one u64.
static MARGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn margin() -> (u32, u32) {
    let value = MARGIN.load(std::sync::atomic::Ordering::Relaxed);
    ((value >> 32) as u32, value as u32)
}

/// When the size was last written — milliseconds since program start.
static LAST_WRITE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn since_start() -> u64 {
    static BEGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    BEGIN.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

#[cfg(feature = "gui")]
/// Remembers the visible size — without the shadow margin.
///
/// This is exactly where the plugin's bug lay: it stores the *reported*
/// size. When that is set on the next start, the margin is added a second
/// time.
///
/// `immediately` is for closing. While dragging the window edge the events
/// pour in; then the size is written at most every two seconds. It is stored
/// while dragging at all so the size stands even when the application is not
/// shut down cleanly — killed, crashed, logged out.
pub fn remember(app: &tauri::AppHandle, window: &tauri::Window, immediately: bool) {
    if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    // The phone view is a test, not a size one wants back.
    if phone_on() {
        return;
    }
    if !immediately {
        let now = since_start();
        let before = LAST_WRITE.load(std::sync::atomic::Ordering::Relaxed);
        if now.saturating_sub(before) < 2000 {
            return;
        }
        LAST_WRITE.store(now, std::sync::atomic::Ordering::Relaxed);
    }

    let Ok(reported) = window.inner_size() else { return };
    let (margin_width, margin_height) = margin();
    let visible = (
        reported.width.saturating_sub(margin_width),
        reported.height.saturating_sub(margin_height),
    );
    if visible.0 > 0 && visible.1 > 0 {
        debug_log(&format!(
            "remember: reported {}×{}, margin {margin_width}×{margin_height}, remembered {}×{}",
            reported.width, reported.height, visible.0, visible.1
        ));
        write_remembered(app, visible);
    }
}

/// The phone a window stands in for while developing — a common Android
/// screen, in CSS pixels.
const PHONE: (f64, f64) = (412.0, 915.0);

/// The visible size before the phone view, while it is on.
static BEFORE_PHONE: std::sync::Mutex<Option<(u32, u32)>> = std::sync::Mutex::new(None);

#[cfg(feature = "gui")]
/// Shrinks the window to a phone and back — to see the layout a phone gets.
///
/// Only the window changes: the page sees a narrow viewport, exactly as on
/// the phone. The phone size is never remembered; switching back restores
/// the size from before, and with it the configured minimum.
pub fn phone(window: &tauri::WebviewWindow, on: bool, size: Option<(f64, f64)>) -> tauri::Result<()> {
    let (width, tall) = size.unwrap_or(PHONE);
    let mut before = BEFORE_PHONE.lock().unwrap_or_else(|e| e.into_inner());
    if on {
        if before.is_some() {
            return window.set_size(tauri::LogicalSize::new(width, tall));
        }
        if window.is_maximized()? {
            window.unmaximize()?;
        }
        let reported = window.inner_size()?;
        let (margin_width, margin_height) = margin();
        *before = Some((
            reported.width.saturating_sub(margin_width),
            reported.height.saturating_sub(margin_height),
        ));
        window.set_min_size(None::<tauri::Size>)?;
        // A screen lower than the phone gets a shorter phone.
        let scale = window.scale_factor()?;
        let height = match usable_area(window) {
            Some(area) => tall.min(area.height as f64 / scale),
            None => tall,
        };
        window.set_size(tauri::LogicalSize::new(width, height))?;
    } else if let Some((width, height)) = before.take() {
        let minimum = window
            .app_handle()
            .config()
            .app
            .windows
            .first()
            .and_then(|w| w.min_width.zip(w.min_height));
        if let Some((width, height)) = minimum {
            window.set_min_size(Some(tauri::LogicalSize::new(width, height)))?;
        }
        window.set_size(PhysicalSize::new(width, height))?;
    }
    Ok(())
}

fn phone_on() -> bool {
    BEFORE_PHONE.lock().map(|b| b.is_some()).unwrap_or(false)
}

/// Diagnostics when `AW_WINDOW=1` is set.
///
/// The window geometry is a little different on every desktop; without these
/// lines a report about it is hard to check.
fn debug_log(text: &str) {
    if std::env::var_os("AW_WINDOW").is_some() {
        eprintln!("[window_state] {text}");
    }
}

#[cfg(test)]
#[path = "window_state_tests.rs"]
mod tests;
