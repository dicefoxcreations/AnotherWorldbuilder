//! The app's log: what went wrong, and what a server world's session did —
//! a file a day beside the profile and the worlds (`logs/`), the last
//! fourteen kept. For finding faults, and to send along: it names worlds and
//! commands, never what is written in them, and never a key.

use super::*;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

/// How many days of the log stay.
const KEPT_DAYS: usize = 14;

/// Where the log lies: in the app's data folder, beside the profile in use
/// — or, while none is chosen, beside where the choice is kept.
pub(crate) fn folder() -> Option<PathBuf> {
    match profile::store_now() {
        Some(store) => Some(store.profile_dir().parent()?.join("logs")),
        None => Some(FALLBACK.get()?.join("logs")),
    }
}

static FALLBACK: OnceLock<PathBuf> = OnceLock::new();

/// The app's own folder, for the log before a profile is chosen.
pub(crate) fn set_fallback(dir: PathBuf) {
    let _ = FALLBACK.set(dir);
}

/// Writes a line: when, what kind, what.
pub(crate) fn line(kind: &str, text: &str) {
    static WRITING: OnceLock<Mutex<()>> = OnceLock::new();
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Some(dir) = folder() else { return };
    let Ok(_turn) = WRITING.get_or_init(Default::default).lock() else { return };
    // The first line in the app's folder says which app writes it.
    let first = profile::store_now().is_some() && !STARTED.swap(true, std::sync::atomic::Ordering::Relaxed);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (year, month, day) = civil((secs / 86_400) as i64);
    let path = dir.join(format!("{year:04}-{month:02}-{day:02}.log"));
    let fresh = !path.exists();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else { return };
    let at = format!("{:02}:{:02}:{:02} UTC", secs / 3600 % 24, secs / 60 % 60, secs % 60);
    if first {
        let _ = writeln!(file, "{at}  {:<8} AnotherWorldbuilder {} on {}", "start", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
    }
    let _ = writeln!(file, "{at}  {kind:<8} {}", text.replace('\n', " ⏎ "));
    // A new day: the oldest go.
    if fresh {
        let mut days: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "log")).collect();
        days.sort();
        let gone = days.len().saturating_sub(KEPT_DAYS);
        for old in &days[..gone] {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// The date of a day since 1970 (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
#[test]
fn days_become_dates() {
    assert_eq!(civil(0), (1970, 1, 1));
    assert_eq!(civil(20_386), (2025, 10, 25));
    assert_eq!(civil(11_016), (2000, 2, 29));
}

#[cfg(feature = "gui")]
/// A line from the interface: a command that failed, an error in the page.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn app_log(kind: String, text: String) {
    line(&kind.chars().take(16).collect::<String>(), &text.chars().take(4000).collect::<String>());
}

#[cfg(feature = "gui")]
/// Opens the log's folder in the file manager.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn app_log_open() -> Answer<()> {
    let dir = folder().ok_or_else(|| AppError("No profile in use.".into()))?;
    std::fs::create_dir_all(&dir)?;
    profile::reveal(&dir)
}
