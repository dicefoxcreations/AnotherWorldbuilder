//! What happens in a world right now: who is where, whose caret stands
//! where, the dice rolled lately — and each device's own dice log.
//!
//! Held in memory by whoever holds the world: this app's own server for its
//! worlds, answering its members over the network the same way. Never
//! stored, gone after a few seconds of silence — but for the dice log, which
//! each device keeps for itself.

use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Where someone is in a world, and where their caret stands.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Presence {
    pub member: String,
    pub name: String,
    pub article: String,
    /// Empty when the caret stands in no text.
    pub layer: String,
    pub start: usize,
    pub end: usize,
    /// Writing in the article — not only reading it.
    #[serde(default)]
    pub editing: bool,
}

/// How long a presence counts without being renewed.
const PRESENT: std::time::Duration = std::time::Duration::from_secs(5);

/// Per world: who is where, and since when.
type Live = HashMap<String, HashMap<String, (std::time::Instant, Presence)>>;

fn live() -> &'static Mutex<Live> {
    static LIVE: OnceLock<Mutex<Live>> = OnceLock::new();
    LIVE.get_or_init(Default::default)
}

fn note_presence(world: &str, presence: Presence) {
    if let Ok(mut all) = live().lock() {
        all.entry(world.to_string()).or_default().insert(presence.member.clone(), (std::time::Instant::now(), presence));
    }
}

/// Who is present in a world right now, but `except`.
fn present(world: &str, except: &str) -> Vec<Presence> {
    let Ok(mut all) = live().lock() else { return Vec::new() };
    let Some(here) = all.get_mut(world) else { return Vec::new() };
    here.retain(|_, (at, _)| at.elapsed() < PRESENT);
    here.values().filter(|(_, p)| p.member != except).map(|(_, p)| p.clone()).collect()
}

/// Whether one writes somewhere in the world right now — the editor says so
/// more often than the page does.
fn writing_now(world: &str, me: &str) -> bool {
    let fresh = std::time::Duration::from_secs(2);
    live().lock().is_ok_and(|all| all.get(world).and_then(|w| w.get(me)).is_some_and(|(at, p)| p.editing && at.elapsed() < fresh))
}

/// Who one is in a world: the member a command runs for, by id and name.
fn who(store: &Store, world: &str) -> Answer<(String, String)> {
    let me = store.actor()?;
    let name = store.db(world)?.member(&me)?.map(|m| m.name).ok_or_else(|| AppError("You are not a member of this world.".into()))?;
    Ok((me, name))
}

/// Someone else writing in the same document, and where their caret stands
/// for this reader.
#[derive(Serialize)]
pub struct Seen {
    pub id: String,
    pub name: String,
    pub caret: Option<document::CaretAt>,
}

#[cfg(feature = "gui")]
/// Tells where one writes in a document — and learns who else writes there,
/// and where their carets stand.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn presence(app: tauri::AppHandle, world: String, article: String, caret: Option<document::Caret>) -> Answer<Vec<Seen>> {
    presence_in(&store(&app)?, world, article, caret)
}

/// `presence` for whom the store is given.
pub(crate) fn presence_in(store: &Store, world: String, article: String, caret: Option<document::Caret>) -> Answer<Vec<Seen>> {
    let (layer, start, end) = match caret {
        Some(caret) => document::caret_in(store, &world, &article, caret)?.unwrap_or_default(),
        None => Default::default(),
    };
    let (me, name) = who(store, &world)?;
    note_presence(&world, Presence { member: me.clone(), name, article: article.clone(), layer, start, end, editing: true });
    let mut out = Vec::new();
    for p in present(&world, &me).into_iter().filter(|p| p.article == article) {
        let caret = if p.layer.is_empty() { None } else { document::caret_at(store, &world, &article, &p.layer, p.start, p.end)? };
        out.push(Seen { id: p.member, name: p.name, caret });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Someone else in the world: on which page, and whether writing there.
#[derive(Serialize)]
pub struct Here {
    pub id: String,
    pub name: String,
    pub article: String,
    pub editing: bool,
}

#[cfg(feature = "gui")]
/// Tells on which page one is — and learns who else is in the world, and where.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn world_presence(app: tauri::AppHandle, world: String, article: String) -> Answer<Vec<Here>> {
    world_presence_in(&store(&app)?, world, article)
}

/// `world_presence` for whom the store is given.
pub(crate) fn world_presence_in(store: &Store, world: String, article: String) -> Answer<Vec<Here>> {
    let (me, name) = who(store, &world)?;
    if !writing_now(&world, &me) {
        note_presence(&world, Presence { member: me.clone(), name, article, layer: String::new(), start: 0, end: 0, editing: false });
    }
    let mut out: Vec<Here> = present(&world, &me).into_iter().map(|p| Here { id: p.member, name: p.name, article: p.article, editing: p.editing }).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

// --- Dice ----------------------------------------------------------------------

/// A roll of the dice, shown to everyone in the world: who, what for, and
/// what came of it. Never stored by the world; gone after a minute.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Roll {
    pub id: String,
    /// Filled in by whoever holds the world — whoever rolled.
    #[serde(default)]
    pub member: String,
    #[serde(default)]
    pub name: String,
    /// The document rolled from.
    pub document: String,
    /// What it was for — `Scimitar`, `Strength`.
    pub label: String,
    /// The formula, names filled in: `1d20 + 4`.
    pub formula: String,
    /// The dice as they fell: `[17] + 4`.
    pub detail: String,
    pub total: f64,
    /// Each die: sides and what it showed.
    pub dice: Vec<(u32, u32)>,
    /// `crit` or `fumble` — as the rules of the document rolled from say.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mark: String,
    /// Milliseconds since the epoch.
    pub at: u64,
    /// Rolled for oneself — never passed on; only in one's own log.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

/// `mark` of the roll that clears everyone's log — the GM's alone.
const CLEAR: &str = "clear";
/// How many rolls a log keeps.
const LOG_MAX: usize = 300;

/// A roll kept to roll again with a click: `Attack` — `1d20 + 5`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Favorite {
    pub name: String,
    pub formula: String,
}

/// What this device keeps of a world's dice: the log, and its favourites.
/// Each device has its own; the GM can clear them all.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiceLog {
    #[serde(default)]
    pub rolls: Vec<Roll>,
    #[serde(default)]
    pub favorites: Vec<Favorite>,
}

fn dice_path(store: &Store, world: &str) -> std::path::PathBuf {
    store.private_world_dir(world).join("dice.json")
}

fn load_dice(store: &Store, world: &str) -> DiceLog {
    std::fs::read(dice_path(store, world)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_dice(store: &Store, world: &str, log: &DiceLog) -> Answer<()> {
    aw_store::durable::write(&dice_path(store, world), &serde_json::to_vec(log)?)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// This device's dice of a world.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn dice_log(app: tauri::AppHandle, world: String) -> Answer<DiceLog> {
    Ok(load_dice(&store(&app)?, &world))
}

#[cfg(feature = "gui")]
/// Keeps a roll in the log — one's own or one that came. A clearing roll
/// empties it: only the world's GM makes one (`dice_clear_in`), and nobody
/// rolls one by hand (`dice_roll_in`).
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn dice_log_add(app: tauri::AppHandle, world: String, roll: Roll) -> Answer<DiceLog> {
    dice_log_add_in(&store(&app)?, world, roll)
}

/// `dice_log_add` for whom the store is given.
pub(crate) fn dice_log_add_in(store: &Store, world: String, roll: Roll) -> Answer<DiceLog> {
    let mut log = load_dice(&store, &world);
    if roll.mark == CLEAR {
        if !log.rolls.is_empty() {
            log.rolls.clear();
            save_dice(&store, &world, &log)?;
        }
        return Ok(log);
    }
    if log.rolls.iter().any(|r| r.id == roll.id) {
        return Ok(log);
    }
    log.rolls.push(roll);
    let over = log.rolls.len().saturating_sub(LOG_MAX);
    log.rolls.drain(..over);
    save_dice(&store, &world, &log)?;
    Ok(log)
}

#[cfg(feature = "gui")]
/// Clears the log — the GM's: here, and on every device in the world.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn dice_log_clear(app: tauri::AppHandle, world: String) -> Answer<DiceLog> {
    dice_log_clear_in(&store(&app)?, world)
}

/// `dice_log_clear` for whom the store is given.
pub(crate) fn dice_log_clear_in(store: &Store, world: String) -> Answer<DiceLog> {
    let mut log = load_dice(&store, &world);
    log.rolls.clear();
    save_dice(&store, &world, &log)?;
    dice_clear_in(&store, &world)?;
    Ok(log)
}

/// The clearing roll, for everyone in the world — the GM's.
pub(crate) fn dice_clear_in(store: &Store, world: &str) -> Answer<()> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM clears the log.".into()));
    }
    let (me, name) = who(store, world)?;
    let at = store::now();
    note_roll(world, Roll { id: format!("clear-{at}"), member: me, name, document: String::new(), label: String::new(), formula: String::new(), detail: String::new(), total: 0.0, dice: Vec::new(), mark: CLEAR.into(), at, secret: false });
    Ok(())
}

#[cfg(feature = "gui")]
/// Sets the favourites — this device's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn dice_favorites_set(app: tauri::AppHandle, world: String, favorites: Vec<Favorite>) -> Answer<DiceLog> {
    dice_favorites_set_in(&store(&app)?, world, favorites)
}

/// `dice_favorites_set` for whom the store is given.
pub(crate) fn dice_favorites_set_in(store: &Store, world: String, favorites: Vec<Favorite>) -> Answer<DiceLog> {
    let mut log = load_dice(&store, &world);
    log.favorites = favorites.into_iter().take(40).collect();
    save_dice(&store, &world, &log)?;
    Ok(log)
}

/// How long a roll stays to be seen — and how many per world at most.
const ROLLS_KEPT: std::time::Duration = std::time::Duration::from_secs(60);
const ROLLS_MAX: usize = 30;

/// Per world: the dice rolled lately, by anyone, and when they arrived.
fn rolls() -> &'static Mutex<HashMap<String, Vec<(std::time::Instant, Roll)>>> {
    static ROLLS: OnceLock<Mutex<HashMap<String, Vec<(std::time::Instant, Roll)>>>> = OnceLock::new();
    ROLLS.get_or_init(Default::default)
}

/// Keeps a roll — once, however often it is passed on. Text is cut short:
/// a roll is a line, not a letter.
fn note_roll(world: &str, mut roll: Roll) {
    let cut = |t: &mut String| {
        if let Some((i, _)) = t.char_indices().nth(120) {
            t.truncate(i);
        }
    };
    for t in [&mut roll.name, &mut roll.document, &mut roll.label, &mut roll.formula, &mut roll.detail] {
        cut(t);
    }
    roll.dice.truncate(200);
    let Ok(mut all) = rolls().lock() else { return };
    let here = all.entry(world.to_string()).or_default();
    here.retain(|(at, _)| at.elapsed() < ROLLS_KEPT);
    if here.iter().any(|(_, r)| r.id == roll.id) {
        return;
    }
    here.push((std::time::Instant::now(), roll));
    let over = here.len().saturating_sub(ROLLS_MAX);
    here.drain(..over);
}

#[cfg(feature = "gui")]
/// Rolls the dice for everyone in the world.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn dice_roll(app: tauri::AppHandle, world: String, roll: Roll) -> Answer<()> {
    dice_roll_in(&store(&app)?, world, roll)
}

/// `dice_roll` for whom the store is given.
pub(crate) fn dice_roll_in(store: &Store, world: String, roll: Roll) -> Answer<()> {
    // A clearing roll is `dice_log_clear`'s; one's own secret stays with oneself.
    if roll.mark == CLEAR || roll.secret {
        return Ok(());
    }
    let (member, name) = who(store, &world)?;
    note_roll(&world, Roll { member, name, ..roll });
    Ok(())
}

#[cfg(feature = "gui")]
/// The dice rolled lately in a world, by anyone — oldest first.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn dice_rolls(app: tauri::AppHandle, world: String) -> Answer<Vec<Roll>> {
    dice_rolls_in(&store(&app)?, world)
}

/// `dice_rolls` for whom the store is given — a member of the world.
pub(crate) fn dice_rolls_in(store: &Store, world: String) -> Answer<Vec<Roll>> {
    store.reader(&world)?;
    let Ok(mut all) = rolls().lock() else { return Ok(Vec::new()) };
    let Some(here) = all.get_mut(&world) else { return Ok(Vec::new()) };
    here.retain(|(at, _)| at.elapsed() < ROLLS_KEPT);
    Ok(here.iter().map(|(_, r)| r.clone()).collect())
}
