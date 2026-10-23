//! Worlds on the local network, for this app.
//!
//! Off until switched on — the network as a whole, and then each world the
//! game master wants to offer, one by one. An offered world is announced with
//! its name and icon only. Whoever finds it and is a member gets the world
//! handed over; whoever is not may ask to join, and the game master decides.
//! The transport itself lives in `aw_sync::world`.
//!
//! While the network is on, players keep their worlds current both ways:
//! every few seconds they ask the host for a digest of what it would give,
//! and only when that or their own journal changed do the two swap records —
//! what the game master wrote goes to the player, what the player wrote to
//! the game master. Nothing is overwritten; records merge.

use super::*;
use crate::store::{JoinRequest, NetworkSettings};
use aw_store::journal::Journal;
use aw_sync::world::{Ask, Host, Offer, Reach, Reply, Server, WorldNet};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};

static NET: OnceLock<Mutex<Option<Arc<WorldNet>>>> = OnceLock::new();
/// Per world, after the last swap: the host's digest and ours. While both
/// stand, there is nothing to swap.
static SWAPPED: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();
/// How many records each world had when last looked at — to tell the
/// interface what changed, whoever brought it.
static SEEN: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
/// Worlds asked to join, waiting for the game master.
static ASKED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
/// One round of keeping up at a time.
static SYNCING: AtomicBool = AtomicBool::new(false);

fn swapped() -> &'static Mutex<HashMap<String, (String, String)>> {
    SWAPPED.get_or_init(Default::default)
}
fn seen() -> &'static Mutex<HashMap<String, usize>> {
    SEEN.get_or_init(Default::default)
}

/// Records this device wrote itself — counted as seen, so only what others
/// bring makes the interface load the world again. Saving while typing
/// would otherwise reload thousands of documents after every pause.
pub(crate) fn wrote_here(world: &str, added: usize) {
    if let Ok(mut seen) = seen().lock() {
        if let Some(count) = seen.get_mut(world) {
            *count += added;
        }
    }
}

/// A whole journal may be big; a swap carries this much at most, the next
/// one the rest.
const SWAP_BUDGET: usize = 32 * 1024 * 1024;
/// Image parts per exchange, in bytes before encoding — a few parts at a time.
const PARTS_BUDGET: usize = 24 * 1024 * 1024;
/// A world brought over is given a little at a time: so much per ask — a
/// slow server takes it in in seconds — and for so long per round; the next
/// round goes on where this one stopped.
const GIVE_BUDGET: usize = 4 * 1024 * 1024;
const GIVE_ROUND: std::time::Duration = std::time::Duration::from_secs(20);
/// Per world and whom it goes to: the records still to be given.
static PENDING: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, Vec<String>>> {
    PENDING.get_or_init(Default::default)
}

/// How far a world has come over to the other side: records and image
/// parts, (there, all). Gone once it is all there.
#[derive(Clone, Default, Serialize)]
pub struct Upload {
    pub records: (usize, usize),
    pub images: (usize, usize),
}

static UPLOADS: OnceLock<Mutex<HashMap<String, Upload>>> = OnceLock::new();

fn uploads() -> &'static Mutex<HashMap<String, Upload>> {
    UPLOADS.get_or_init(Default::default)
}

fn note_upload(world: &str, change: impl FnOnce(&mut Upload)) {
    if let Ok(mut all) = uploads().lock() {
        let upload = all.entry(world.to_string()).or_default();
        change(upload);
        if upload.records.0 >= upload.records.1 && upload.images.0 >= upload.images.1 {
            all.remove(world);
        }
    }
}

/// Per world: the parts the host said it lacks — given with the next round.
static WANTS: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();

fn wants() -> &'static Mutex<HashMap<String, Vec<String>>> {
    WANTS.get_or_init(Default::default)
}

/// The parts named that lie here, read only as far as one exchange carries.
fn parts_within(store: &Store, world: &str, names: &[String]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut size = 0;
    for name in names {
        let Some(part) = store.parts(world, std::slice::from_ref(name)).pop() else { continue };
        size += part.1.len();
        if size > PARTS_BUDGET && !out.is_empty() {
            break;
        }
        out.push(part);
    }
    out
}

/// Parts as they travel: (name, base64), as many as the budget allows.
fn encode_parts(parts: Vec<(String, Vec<u8>)>) -> Vec<(String, String)> {
    use base64::Engine;
    let mut out = Vec::new();
    let mut size = 0;
    for (name, bytes) in parts {
        size += bytes.len();
        if size > PARTS_BUDGET && !out.is_empty() {
            break;
        }
        out.push((name, base64::engine::general_purpose::STANDARD.encode(bytes)));
    }
    out
}

fn decode_parts(parts: Vec<(String, String)>) -> Vec<(String, Vec<u8>)> {
    use base64::Engine;
    parts
        .into_iter()
        .filter_map(|(name, text)| Some((name, base64::engine::general_purpose::STANDARD.decode(text).ok()?)))
        .collect()
}

/// Where someone is writing: in which article, which layer, from where to
/// where — as stable places in the text, which stay right while others
/// write around them. Never stored; gone after a few seconds of silence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Presence {
    pub member: String,
    pub name: String,
    pub article: String,
    /// Empty when the caret stands in no text.
    pub layer: String,
    pub anchor: String,
    pub head: String,
    /// Writing in the article — not only reading it.
    #[serde(default)]
    pub editing: bool,
}

/// How long a presence counts without being renewed.
const PRESENT: std::time::Duration = std::time::Duration::from_secs(5);

/// Per world: who is where, and since when.
type Live = HashMap<String, HashMap<String, (std::time::Instant, Presence)>>;
static LIVE: OnceLock<Mutex<Live>> = OnceLock::new();
/// Per world: where one writes oneself, and since when — for the host, with
/// the next digest.
static MINE: OnceLock<Mutex<HashMap<String, (std::time::Instant, Presence)>>> = OnceLock::new();

fn live() -> &'static Mutex<Live> {
    LIVE.get_or_init(Default::default)
}
fn mine() -> &'static Mutex<HashMap<String, (std::time::Instant, Presence)>> {
    MINE.get_or_init(Default::default)
}

fn note_presence(world: &str, presence: Presence) {
    if let Ok(mut all) = live().lock() {
        all.entry(world.to_string()).or_default().insert(presence.member.clone(), (std::time::Instant::now(), presence));
    }
}

/// A roll of the dice, shown to everyone in the session: who, what for,
/// and what came of it. Never stored; gone after a minute.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Roll {
    pub id: String,
    /// Filled in here — whoever rolled.
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
/// Never synced: each device has its own; the GM can clear them all.
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
#[cfg_attr(feature = "gui", tauri::command)]
pub fn dice_log(app: tauri::AppHandle, world: String) -> Answer<DiceLog> {
    Ok(load_dice(&store(&app)?, &world))
}

#[cfg(feature = "gui")]
/// Keeps a roll in the log — one's own or one that came. A clearing roll
/// empties it, where it comes from the world's GM.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn dice_log_add(app: tauri::AppHandle, world: String, roll: Roll) -> Answer<DiceLog> {
    add_to_log(&store(&app)?, &world, roll)
}

fn add_to_log(store: &Store, world: &str, roll: Roll) -> Answer<DiceLog> {
    let (store, world) = (store, world.to_string());
    let mut log = load_dice(store, &world);
    if roll.mark == CLEAR {
        if store.gm_of(&world)?.as_deref() == Some(roll.member.as_str()) && !log.rolls.is_empty() {
            log.rolls.clear();
            save_dice(store, &world, &log)?;
        }
        return Ok(log);
    }
    if log.rolls.iter().any(|r| r.id == roll.id) {
        return Ok(log);
    }
    log.rolls.push(roll);
    let over = log.rolls.len().saturating_sub(LOG_MAX);
    log.rolls.drain(..over);
    save_dice(store, &world, &log)?;
    Ok(log)
}

#[cfg(feature = "gui")]
/// Clears the log — the GM's: here, and on every device in the session.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn dice_log_clear(app: tauri::AppHandle, world: String) -> Answer<DiceLog> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM clears the log.".into()));
    }
    let me = store.load_identity()?;
    let mut log = load_dice(&store, &world);
    log.rolls.clear();
    save_dice(&store, &world, &log)?;
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    note_roll(
        &world,
        Roll {
            id: format!("clear-{at}"),
            member: me.id().to_string(),
            name: me.name().to_string(),
            document: String::new(),
            label: String::new(),
            formula: String::new(),
            detail: String::new(),
            total: 0.0,
            dice: Vec::new(),
            mark: CLEAR.into(),
            at,
            secret: false,
        },
    );
    Ok(log)
}

#[cfg(feature = "gui")]
/// Sets the favourites — this device's.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn dice_favorites_set(app: tauri::AppHandle, world: String, favorites: Vec<Favorite>) -> Answer<DiceLog> {
    let store = store(&app)?;
    let mut log = load_dice(&store, &world);
    log.favorites = favorites.into_iter().take(40).collect();
    save_dice(&store, &world, &log)?;
    Ok(log)
}

/// How long a roll stays to be seen — and how many per world at most.
const ROLLS_KEPT: std::time::Duration = std::time::Duration::from_secs(60);
const ROLLS_MAX: usize = 30;

/// Per world: the dice rolled lately, by anyone, and when they arrived.
static ROLLS: OnceLock<Mutex<HashMap<String, Vec<(std::time::Instant, Roll)>>>> = OnceLock::new();

fn rolls() -> &'static Mutex<HashMap<String, Vec<(std::time::Instant, Roll)>>> {
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

/// The dice rolled lately in a world that `keep` keeps.
fn recent_rolls(world: &str, keep: impl Fn(&Roll) -> bool) -> Vec<Roll> {
    let Ok(mut all) = rolls().lock() else { return Vec::new() };
    let Some(here) = all.get_mut(world) else { return Vec::new() };
    here.retain(|(at, _)| at.elapsed() < ROLLS_KEPT);
    here.iter().map(|(_, r)| r.clone()).filter(|r| keep(r)).collect()
}

#[cfg(feature = "gui")]
/// Rolls the dice for everyone in the session: kept here, and passed on
/// with the next digest — to the host, or by the host to its players.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn dice_roll(app: tauri::AppHandle, world: String, roll: Roll) -> Answer<()> {
    dice_roll_in(&store(&app)?, world, roll)
}

/// `dice_roll` for whom the store is given — the host, acting for a player.
pub(crate) fn dice_roll_in(store: &Store, world: String, roll: Roll) -> Answer<()> {
    let me = store.clone().load_identity()?;
    // A clearing roll is `dice_log_clear`'s; one's own secret stays here.
    if roll.mark == CLEAR || roll.secret {
        return Ok(());
    }
    let (member, name) = who_am_i(&me);
    note_roll(&world, Roll { member, name, ..roll });
    Ok(())
}

/// The dice rolled lately in a world, by anyone — oldest first.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn dice_rolls(world: String) -> Answer<Vec<Roll>> {
    Ok(recent_rolls(&world, |_| true))
}

/// Who is present in a world right now, but `except`.
fn present(world: &str, except: &str) -> Vec<Presence> {
    let Ok(mut all) = live().lock() else { return Vec::new() };
    let Some(here) = all.get_mut(world) else { return Vec::new() };
    here.retain(|_, (at, _)| at.elapsed() < PRESENT);
    here.values().filter(|(_, p)| p.member != except).map(|(_, p)| p.clone()).collect()
}
fn asked() -> &'static Mutex<HashSet<String>> {
    ASKED.get_or_init(Default::default)
}

pub(crate) fn running() -> Option<Arc<WorldNet>> {
    NET.get_or_init(|| Mutex::new(None)).lock().ok()?.clone()
}

/// The host side — answers from what lies on disk, wherever it lies now.
struct DesktopHost;

impl Host for DesktopHost {
    fn offers(&self) -> Vec<Offer> {
        profile::store_now().and_then(|store| offers(&store).ok()).unwrap_or_default()
    }
    fn serves(&self, world: &str) -> bool {
        // No world in particular: the list of those hosted here.
        world.is_empty()
            || profile::store_now().is_some_and(|store| {
                offers(&store).is_ok_and(|o| o.iter().any(|o| o.world == world)) || on_the_internet(&store, world)
            })
    }
    fn answer(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
        let Some(store) = profile::store_now() else { return Reply::NotOffered };
        // Asked by a host code, as a server is: the worlds offered here.
        if let Ask::Worlds = ask {
            return Reply::Worlds { worlds: offers(&store).unwrap_or_default(), host: None, admin: false, may_bring: false, asked: false };
        }
        // On its way to another folder: asked again shortly.
        if store::moving(world) {
            return Reply::Busy;
        }
        answer(&store, world, who, ask).unwrap_or(Reply::NotOffered)
    }
}

/// A world answered for over the internet: one held here in a shared folder
/// — whose members meet there. The game master's device, and any member's:
/// whoever can be reached carries the others.
fn on_the_internet(store: &Store, world: &str) -> bool {
    store.load_network().is_ok_and(|s| s.folders) && in_shared_folder(store, world)
}

/// Whether a world lies in a location of its own beside the profile's — a
/// cloud drive the system mounts or a folder a sync app keeps, which the
/// group shares. Its members leave their notes there.
fn in_shared_folder(store: &Store, world: &str) -> bool {
    let dir = store.world_dir(world);
    store.holds(world) && store.local_roots().iter().skip(1).any(|root| dir.parent() == Some(root.as_path()))
}

/// How often this device's note is written again while nothing changed.
const NOTE_AGAIN: u64 = 10 * 60;
/// The notes read from shared folders, by file name — each file read once.
static FOLDER_NOTES: OnceLock<Mutex<HashMap<String, HashMap<String, aw_sync::admission::PeerNote>>>> = OnceLock::new();
/// When and with which addresses this device last wrote its note, by world.
static FOLDER_WRITTEN: OnceLock<Mutex<HashMap<String, (Vec<String>, u64)>>> = OnceLock::new();

/// The members' notes in a shared folder's `peers/` — and this device's own,
/// where it can be reached, its older ones taken away. The same signed notes
/// as in a Drive folder; one with a wrong signature is not read.
///
/// A mounted cloud drive may take its time — half a minute, when its
/// provider holds back. The files are read and written without holding the
/// notes already known: those stay at hand meanwhile.
fn folder_notes(store: &Store, world: &str, me: &Identity, reach: Option<(String, Vec<String>)>) -> Answer<Vec<aw_sync::admission::PeerNote>> {
    let dir = store.world_dir(world).join("peers");
    let notes_now = || FOLDER_NOTES.get_or_init(Default::default).lock().map_err(AppError::from);
    let had: HashSet<String> = notes_now()?.get(world).map(|k| k.keys().cloned().collect()).unwrap_or_default();
    let names: Vec<String> = std::fs::read_dir(&dir)
        .map(|it| it.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let mut read = Vec::new();
    for name in names.iter().filter(|n| n.starts_with("peer-") && n.ends_with(".json") && !had.contains(*n)) {
        let raw = std::fs::read(dir.join(name)).ok().filter(|raw| raw.len() <= 8 * 1024);
        if let Some(note) = raw.and_then(|raw| serde_json::from_slice::<aw_sync::admission::PeerNote>(&raw).ok()).filter(|n| n.check(world).is_ok()) {
            read.push((name.clone(), note));
        }
    }
    let mut cache = notes_now()?;
    let known = cache.entry(world.to_string()).or_default();
    known.retain(|name, _| names.contains(name));
    known.extend(read);
    let notes: Vec<_> = known.values().cloned().collect();
    let mine: Vec<String> = known.iter().filter(|(_, n)| n.identity.id == me.public().id).map(|(name, _)| name.clone()).collect();
    drop(cache);
    let Some((endpoint, addrs)) = reach else { return Ok(notes) };
    let at = now();
    let mut written = FOLDER_WRITTEN.get_or_init(Default::default).lock().map_err(AppError::from)?;
    if written.get(world).is_some_and(|(a, when)| a == &addrs && at < when + NOTE_AGAIN) {
        return Ok(notes);
    }
    let note = aw_sync::admission::PeerNote::new(world, &endpoint, addrs.clone(), me, at).map_err(|e| AppError(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("peer-{}-{at}.json", &endpoint[..endpoint.len().min(16)])), serde_json::to_string(&note)?)?;
    // Our older notes go — from earlier starts too, whose endpoints are gone.
    for name in mine {
        let _ = std::fs::remove_file(dir.join(name));
    }
    written.insert(world.to_string(), (addrs, at));
    Ok(notes)
}

/// Worlds whose folder notes are being read right now.
static READING_NOTES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// The notes known of a shared folder, at once — and the folder read anew
/// meanwhile, in the background: a slow drive never holds up the round
/// that carries what is being written.
fn folder_notes_soon(store: &Store, world: &str) -> Vec<aw_sync::admission::PeerNote> {
    let reading = READING_NOTES.get_or_init(Default::default);
    if reading.lock().is_ok_and(|mut r| r.insert(world.to_string())) {
        let (store, world, reach) = (store.clone(), world.to_string(), reach_note());
        std::thread::spawn(move || {
            if let Ok(me) = store.load_identity() {
                let _ = folder_notes(&store, &world, &me, reach);
            }
            if let Ok(mut r) = READING_NOTES.get_or_init(Default::default).lock() {
                r.remove(&world);
            }
        });
    }
    FOLDER_NOTES
        .get_or_init(Default::default)
        .lock()
        .map(|c| c.get(world).map(|k| k.values().cloned().collect()).unwrap_or_default())
        .unwrap_or_default()
}

/// Where this device can be reached from the internet, as its endpoint and
/// addresses — for its note in the shared folder. `None`: not switched on, or
/// nothing reachable.
pub(crate) fn reach_note() -> Option<(String, Vec<String>)> {
    let net = running()?;
    let store = profile::store_now()?;
    if !store.load_network().ok()?.folders {
        return None;
    }
    let mut addrs: Vec<String> = net.reachable().iter().map(|a| a.to_string()).collect();
    // Its address at home too: members in the same network meet by it.
    if let Some(home) = net.home_address().map(|a| a.to_string()).filter(|a| !addrs.contains(a)) {
        addrs.push(home);
    }
    (!addrs.is_empty()).then(|| (net.id(), addrs))
}

/// Notes of how long ago, at most, still count — a device gone longer is
/// gone.
const NOTE_FRESH: u64 = 15 * 60;
/// How often the host connects towards one member at most.
const PUNCH_EVERY: std::time::Duration = std::time::Duration::from_secs(3);
static PUNCHED: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();

/// Puts what the shared folder says of the others to use.
///
/// Whoever can be reached from outside connects towards every member who
/// left a note — that lets their talk through the firewall here. Every
/// device asks one other for the world: the game master's, or — where that
/// does not answer — any member's that can be reached; their records are
/// signed one by one, whoever passes them on. Only the game master's device,
/// reachable itself, asks no one: the others come to it.
fn meet_through_folder(store: &Store, net: &WorldNet, me: &Identity) -> Answer<()> {
    if !store.load_network()?.folders {
        return Ok(());
    }
    let me_id = me.id().to_string();
    let reachable = !net.reachable().is_empty();
    for meta in store.list_worlds()? {
        let world = meta.public.world_id.to_string();
        let gm_here = store.is_gm(&world);
        let gm = store.gm_of(&world)?;
        // Who counts: for the game master the member list; for a player
        // those let in — and the game master, who lets them in.
        let member = |id: &str| if gm_here { meta.member(id).is_some() } else { gm.as_deref() == Some(id) || store.has_grant(&world, id).unwrap_or(false) };
        if !in_shared_folder(store, &world) {
            continue;
        }
        let notes: Vec<_> = folder_notes_soon(store, &world)
            .into_iter()
            .filter(|n| n.at + NOTE_FRESH >= now() && n.identity.id.as_str() != me_id && member(n.identity.id.as_str()))
            .collect();
        let addrs = |n: &aw_sync::admission::PeerNote| n.addrs.iter().filter_map(|a| a.parse().ok()).collect::<Vec<std::net::SocketAddr>>();
        if reachable {
            let mut punched = PUNCHED.get_or_init(Default::default).lock().map_err(AppError::from)?;
            for n in &notes {
                if punched.get(&n.endpoint).is_some_and(|t| t.elapsed() < PUNCH_EVERY) {
                    continue;
                }
                punched.insert(n.endpoint.clone(), std::time::Instant::now());
                let _ = net.punch_to(&n.endpoint, &addrs(n));
            }
        }
        if gm_here && reachable {
            continue;
        }
        // Whom to ask: the game master's device first, then the others, the newest first.
        let failed = FAILED.get_or_init(Default::default).lock().map_err(AppError::from)?.clone();
        let mut candidates: Vec<_> = notes.iter().filter(|n| !n.addrs.is_empty() && !failed.get(&n.endpoint).is_some_and(|t| t.elapsed() < RETRY)).collect();
        candidates.sort_by_key(|n| (gm.as_deref() != Some(n.identity.id.as_str()), std::cmp::Reverse(n.at)));
        match candidates.first() {
            Some(n) => {
                let offer = Offer { world: world.clone(), name: meta.public.name.clone(), icon: meta.icon.clone(), gm: n.identity.name.clone() };
                let _ = net.add_remote(offer, &n.endpoint, &addrs(n));
            }
            None => net.drop_remote(&world),
        }
    }
    Ok(())
}

/// When asking a device last failed, by its endpoint — not asked again for a
/// while, so one that does not answer does not hold every round up, and
/// another one is asked meanwhile.
static FAILED: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
const RETRY: std::time::Duration = std::time::Duration::from_secs(20);

/// The worlds offered: held here as game master, and hosted — on the local
/// network or reachable from the internet.
fn offers(store: &Store) -> Answer<Vec<Offer>> {
    let settings = store.load_network()?;
    if !settings.enabled && !settings.internet {
        return Ok(Vec::new());
    }
    let me = store.load_identity()?;
    Ok(settings
        .shared
        .iter()
        // A world that lives on a server is reached there, not here.
        .filter(|w| store.is_gm(w) && !settings.homes.contains_key(*w))
        .filter_map(|w| store.load_world_meta(w).ok())
        .map(|m| Offer {
            world: m.public.world_id.to_string(),
            name: m.public.name.clone(),
            icon: m.icon.clone(),
            gm: me.name().to_string(),
        })
        .collect())
}

/// Answers someone whose identity is proven.
pub(crate) fn answer(store: &Store, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
    let id = who.id.to_string();
    if !store.holds(world) {
        return Ok(Reply::NotOffered);
    }
    // A member's device answers the others too — over the internet, where
    // the game master's cannot be reached. It knows them by their grant; who
    // asks to join, asks the game master.
    if !store.is_gm(world) {
        if !store.has_grant(world, &id)? {
            return Ok(Reply::NotMember);
        }
        return answer_member(store, world, &id, ask);
    }
    let meta = store.load_world_meta(world)?;
    // Let in ahead of time: now the keys are here that belong to the id.
    if meta.member(&id).is_none() && meta.invited.contains(&id) {
        admit(store, world, who.clone(), Vec::new())?;
    }
    if store.load_world_meta(world)?.member(&id).is_none() {
        return Ok(match ask {
            Ask::Join => {
                let mut requests = store.load_requests(world)?;
                requests.retain(|r| r.identity.id != who.id);
                requests.push(JoinRequest { identity: who.clone(), at: now() });
                store.save_requests(world, &requests)?;
                Reply::Requested
            }
            _ => Reply::NotMember,
        });
    }
    // A world that stays here: its players work on it online.
    if crate::host::online_only(store, world) {
        return Ok(online(store, world, who, ask));
    }
    answer_member(store, world, &id, ask)
}

/// Answers a member of a world that stays with its host: a call, done in
/// their name — anything else, that it stays here.
pub(crate) fn online(store: &Store, world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
    match ask {
        Ask::Call { command, args } => {
            let args: serde_json::Value = serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
            match crate::host::call(store, world, who, &command, &args) {
                Ok(value) => Reply::Called { value: Some(value.to_string()), error: None },
                Err(e) => Reply::Called { value: None, error: Some(e.0) },
            }
        }
        _ => Reply::OnlineOnly,
    }
}

/// Answers a member — whose membership is checked.
pub(crate) fn answer_member(store: &Store, world: &str, id: &str, ask: Ask) -> Answer<Reply> {
    let id = id.to_string();
    Ok(match ask {
        // Only a host answers calls — for a world that stays with it.
        Ask::Call { .. } => Reply::NotOffered,
        Ask::Join | Ask::Digest { .. } => {
            if let Ask::Digest { presence, rolls } = &ask {
                // Only where they write themselves — not in someone else's name.
                if let Some(Ok(p)) = presence.as_deref().map(serde_json::from_str::<Presence>) {
                    if p.member == id {
                        note_presence(world, p);
                    }
                }
                // Their own dice — not someone else's.
                for r in rolls.iter().filter_map(|r| serde_json::from_str::<Roll>(r).ok()) {
                    if r.member == id {
                        note_roll(world, r);
                    }
                }
            }
            // Where others write — in parts this member may have at all.
            let presence = present(world, &id)
                .into_iter()
                .filter(|p| p.layer.is_empty() || p.layer.parse().is_ok_and(|l| store.for_players(world, l).unwrap_or(false)))
                .filter_map(|p| serde_json::to_string(&p).ok())
                .collect();
            let rolls = recent_rolls(world, |r| r.member != id).iter().filter_map(|r| serde_json::to_string(r).ok()).collect();
            Reply::Digest {
                digest: Journal::digest(&store.inventory(world)?),
                presence,
                rolls,
                handover: store.pending_handover(world)?.as_deref() == Some(id.as_str()),
                requests: Vec::new(),
                no_copy: false,
            }
        }
        Ask::Parts { want, give } => {
            // Whatever is not needed or does not match its name is not kept.
            store.take_parts(world, &decode_parts(give))?;
            Reply::Parts { parts: encode_parts(store.parts(world, &want)), want: store.missing_parts(world)? }
        }
        Ask::Handover => match store.pending_handover(world)? {
            Some(to) if to == id => match store.load_world_meta(world)?.member(&id) {
                Some(m) => Reply::Handover {
                    package: serde_json::to_string(&store.handover_pack(world, &m.identity)?).map_err(AppError::from)?,
                },
                None => Reply::NotOffered,
            },
            _ => Reply::NotOffered,
        },
        // A device lists no worlds and has no admin; a server does.
        Ask::Worlds | Ask::Manage { .. } | Ask::HostWorlds => Reply::NotOffered,
        Ask::Give { records } => {
            store.receive(world, &records)?;
            if store.handed_over(world)? {
                store.step_down(world)?;
                republish();
            }
            Reply::Taken { digest: Journal::digest(&store.inventory(world)?) }
        }
        Ask::Swap { have, give } => {
            // Whatever does not check out is simply not taken.
            store.receive(world, &give)?;
            // The one the world was handed to has taken it over: their
            // journal names them game master now, and so does ours.
            if store.handed_over(world)? {
                store.step_down(world)?;
                republish();
            }
            let mine = store.inventory(world)?;
            let known: std::collections::HashSet<&String> = have.iter().collect();
            let missing: Vec<String> = mine.iter().filter(|r| !known.contains(r)).cloned().collect();
            let mut records = Vec::new();
            let mut size = 0;
            for record in store.records(world, &missing)? {
                size += record.len();
                if size > SWAP_BUDGET && !records.is_empty() {
                    break;
                }
                records.push(record);
            }
            let here: std::collections::HashSet<String> = store.inventory(world)?.into_iter().collect();
            let wanted = have.into_iter().filter(|r| !here.contains(r)).collect();
            Reply::Records { records, wanted, digest: Journal::digest(&mine) }
        }
    })
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Starts the network as the settings say — the local network, the
/// internet, both — or stops it. A change of what it reaches starts it anew.
pub(crate) async fn switch(store: &Store, settings: &NetworkSettings) -> Answer<()> {
    let slot = NET.get_or_init(|| Mutex::new(None));
    // Dropping it takes the announcements back and closes the endpoint.
    slot.lock().map_err(AppError::from)?.take();
    if let Some(told) = telling().lock().map_err(AppError::from)?.take() {
        told.abort();
    }
    if !any_network(settings) {
        return Ok(());
    }
    let reach = Reach {
        lan: settings.enabled,
        internet: settings.internet,
        stun: settings.stun.clone(),
        // The same endpoint every start: a host code stays valid.
        secret: Some(endpoint_secret(store)?),
        port: Some(settings.port),
        port_required: false,
        only: None,
    };
    let net = WorldNet::start(Arc::new(DesktopHost), reach)
        .await
        .map_err(|e| AppError(format!("The network could not start: {e}")))?;
    // Its address is enough: the code is told on TCP at the port, as a
    // server does.
    if settings.enabled || settings.internet {
        let port = settings.port;
        let mut addrs: Vec<String> = net.reachable().iter().map(|a| a.to_string()).collect();
        addrs.extend(net.home_address().map(|a| a.to_string()));
        if !addrs.is_empty() {
            let code = Server { endpoint: net.id(), addrs }.code();
            *telling().lock().map_err(AppError::from)? = Some(tokio::spawn(async move {
                let _ = aw_sync::world::tell_code(port, code).await;
            }));
        }
    }
    *slot.lock().map_err(AppError::from)? = Some(Arc::new(net));
    Ok(())
}

/// Telling this device's code on TCP — stopped when the network starts anew.
static TELLING: OnceLock<Mutex<Option<tokio::task::JoinHandle<()>>>> = OnceLock::new();

fn telling() -> &'static Mutex<Option<tokio::task::JoinHandle<()>>> {
    TELLING.get_or_init(Default::default)
}

fn any_network(settings: &NetworkSettings) -> bool {
    settings.enabled || settings.internet || settings.folders || !settings.servers.is_empty()
}

/// This device's endpoint key — made once and kept in the profile, so its
/// id, and with it its host code, stays the same.
pub(crate) fn endpoint_secret(store: &Store) -> Answer<[u8; 32]> {
    let path = store.profile_dir().join("endpoint.key");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(Ok(bytes)) = hex::decode(text.trim()).map(<[u8; 32]>::try_from) {
            return Ok(bytes);
        }
    }
    let bytes = aw_crypto::random_bytes::<32>().map_err(|e| AppError(e.to_string()))?;
    aw_store::durable::write(&path, hex::encode(bytes).as_bytes())?;
    Ok(bytes)
}

#[cfg(feature = "gui")]
/// Called on start, and once a place for the data is chosen: only if
/// switched on. Without a place yet, there is nothing to offer.
pub fn start(app: &tauri::AppHandle) -> Answer<()> {
    let Ok(store) = store(app) else { return Ok(()) };
    let settings = store.load_network()?;
    if !any_network(&settings) {
        return Ok(());
    }
    tauri::async_runtime::spawn(async move {
        let _ = switch(&store, &settings).await;
    });
    Ok(())
}

pub(crate) fn republish() {
    if let Some(net) = running() {
        let _ = net.publish();
    }
}

// --- Commands ---------------------------------------------------------------

#[derive(Serialize)]
pub struct NetworkView {
    pub enabled: bool,
    /// Reachable from the internet — to host.
    pub internet: bool,
    /// Peer to peer through shared folders.
    pub folders: bool,
    /// The port asked for.
    pub port: u16,
    /// What players enter to reach this device as their host — empty while
    /// nothing can be reached.
    pub code: String,
    /// Where others can reach this device from outside — empty: nowhere.
    pub reachable: Vec<String>,
    /// How: `router`, `stun`, `ipv6`.
    pub via: Vec<String>,
    pub stun: Vec<String>,
    /// Whether the network actually runs — it can fail to start.
    pub running: bool,
    /// Servers that keep worlds for the group, by their code.
    pub servers: Vec<String>,
    /// Each of them as it answered last.
    pub server_list: Vec<ServerEntry>,
    /// The worlds this app could offer: those it is game master of.
    pub worlds: Vec<SharedWorld>,
    /// Worlds on their way to a server, by id.
    pub uploads: HashMap<String, Upload>,
}

/// A server as it answered last.
#[derive(Serialize)]
pub struct ServerEntry {
    pub code: String,
    /// What it calls itself — empty while it has not answered yet.
    pub name: String,
    /// This device is its admin.
    pub admin: bool,
    /// A world of this GM waits for its admin.
    pub waiting: bool,
    /// Those worlds, by id.
    pub waiting_worlds: Vec<String>,
    /// This device may bring worlds there.
    pub may_bring: bool,
    /// It asked to.
    pub asked: bool,
    /// It answered last time.
    pub reached: bool,
}

#[derive(Serialize)]
pub struct SharedWorld {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub shared: bool,
    pub encrypted: bool,
    /// Players get a copy of it; without, they work online with this device as host.
    pub copies: bool,
    /// The server it lives on, by code — empty: this device.
    pub server: String,
}

pub(crate) fn view(store: &Store) -> Answer<NetworkView> {
    let settings = store.load_network()?;
    let worlds = store
        .list_worlds()?
        .into_iter()
        .filter(|m| store.is_gm(m.public.world_id.as_str()))
        .map(|m| {
            let id = m.public.world_id.to_string();
            let encrypted = store.is_encrypted(&id).unwrap_or(true);
            let copies = store.copies(&id).unwrap_or(false);
            let server = settings.home_of(&id).cloned().unwrap_or_default();
            SharedWorld { shared: settings.shared.contains(&id), encrypted, copies, server, id, name: m.public.name, icon: m.icon }
        })
        .collect();
    let by = running().map(|n| n.reach_by()).unwrap_or_default();
    let reachable = running().map(|n| n.reachable().iter().map(|a| a.to_string()).collect()).unwrap_or_default();
    let mut via: Vec<String> = by.iter().map(|(k, _)| k.to_string()).collect();
    via.dedup();
    // Its outside addresses, and the one at home for players in the same network.
    let code = running()
        .map(|n| {
            let mut addrs: Vec<String> = n.reachable().iter().map(|a| a.to_string()).collect();
            addrs.extend(n.home_address().map(|a| a.to_string()).filter(|a| !addrs.contains(a)));
            if addrs.is_empty() || !settings.internet {
                return String::new();
            }
            Server { endpoint: n.id(), addrs }.code()
        })
        .unwrap_or_default();
    Ok(NetworkView {
        enabled: settings.enabled,
        internet: settings.internet,
        folders: settings.folders,
        port: settings.port,
        code,
        reachable,
        via,
        stun: settings.stun,
        running: running().is_some(),
        server_list: settings
            .servers
            .iter()
            .map(|code| {
                let s = status(code);
                ServerEntry {
                    code: code.clone(),
                    name: s.name,
                    admin: s.admin,
                    waiting: !s.waiting.is_empty(),
                    waiting_worlds: s.waiting,
                    may_bring: s.may_bring,
                    asked: s.asked,
                    reached: s.reached,
                }
            })
            .collect(),
        servers: settings.servers,
        worlds,
        uploads: uploads().lock().map(|u| u.clone()).unwrap_or_default(),
    })
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command)]
pub fn network_settings(app: tauri::AppHandle) -> Answer<NetworkView> {
    view(&store(&app)?)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_enable(app: tauri::AppHandle, enabled: bool) -> Answer<NetworkView> {
    let store = store(&app)?;
    let mut settings = store.load_network()?;
    settings.enabled = enabled;
    store.save_network(&settings)?;
    switch(&store, &settings).await?;
    view(&store)
}

#[cfg(feature = "gui")]
/// Direct connections over the internet — and whom to ask how this device
/// looks from outside.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_internet(app: tauri::AppHandle, enabled: bool, stun: Vec<String>) -> Answer<NetworkView> {
    let store = store(&app)?;
    let mut settings = store.load_network()?;
    settings.internet = enabled;
    settings.stun = stun.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    store.save_network(&settings)?;
    switch(&store, &settings).await?;
    view(&store)
}

#[cfg(feature = "gui")]
/// Peer to peer through shared folders: devices of a world in a cloud folder meet there.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_folders(app: tauri::AppHandle, enabled: bool) -> Answer<NetworkView> {
    let store = store(&app)?;
    let mut settings = store.load_network()?;
    settings.folders = enabled;
    store.save_network(&settings)?;
    switch(&store, &settings).await?;
    view(&store)
}

#[cfg(feature = "gui")]
/// The port to answer on — for a router's forwarding. None: the usual one.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_port(app: tauri::AppHandle, port: Option<u16>) -> Answer<NetworkView> {
    let store = store(&app)?;
    let mut settings = store.load_network()?;
    settings.port = port.filter(|p| *p > 0).unwrap_or(aw_sync::world::DEFAULT_PORT);
    store.save_network(&settings)?;
    switch(&store, &settings).await?;
    view(&store)
}

#[cfg(feature = "gui")]
/// Whether players of an encrypted world get a copy — or work online, with
/// this device as their host.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn network_copies_set(app: tauri::AppHandle, world: String, copies: bool) -> Answer<NetworkView> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM decides that.".into()));
    }
    if !store.is_encrypted(&world)? {
        return Err(AppError("Only an encrypted world can be copied.".into()));
    }
    // In the world itself: every host keeps to it, a server too.
    store.control(&world, aw_store::journal::Item::Copies { players: copies }, "")?;
    view(&store)
}

#[cfg(feature = "gui")]
/// The servers to keep worlds with, by their code or address.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_servers(app: tauri::AppHandle, servers: Vec<String>) -> Answer<NetworkView> {
    let store = store(&app)?;
    let mut codes = Vec::new();
    // A code, or an address alone — the server tells its code there.
    for entry in servers.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let server = Server::look_up(entry).await.map_err(|e| AppError(e.to_string()))?;
        if !codes.contains(&server.code()) {
            codes.push(server.code());
        }
    }
    let mut settings = store.load_network()?;
    settings.servers = codes;
    store.save_network(&settings)?;
    switch(&store, &settings).await?;
    view(&store)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command)]
pub fn network_share(app: tauri::AppHandle, world: String, shared: bool) -> Answer<NetworkView> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM can offer a world.".into()));
    }
    let mut settings: NetworkSettings = store.load_network()?;
    settings.shared.retain(|w| w != &world);
    if shared {
        settings.shared.push(world);
    }
    store.save_network(&settings)?;
    republish();
    view(&store)
}

/// A world found on the network.
#[derive(Serialize)]
pub struct NetWorld {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub gm: String,
    /// `player` when this app already holds it; nothing otherwise.
    pub local: Option<String>,
    /// The server holding it, by code — empty: found on the local network.
    pub server: String,
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command)]
pub fn network_worlds(app: tauri::AppHandle) -> Answer<Vec<NetWorld>> {
    let store = store(&app)?;
    let Some(net) = running() else { return Ok(Vec::new()) };
    let mut out: Vec<NetWorld> = net
        .found()
        .into_iter()
        // A world held here as game master is one's own, not a find.
        .filter(|o| !store.is_gm(&o.world))
        .map(|o| NetWorld {
            local: store.load_world_meta(&o.world).ok().map(|_| "player".into()),
            id: o.world,
            name: o.name,
            icon: o.icon,
            gm: o.gm,
            server: String::new(),
        })
        .collect();
    // Held by a server.
    for (world, (server, offer)) in server_worlds().lock().map_err(AppError::from)?.iter() {
        if store.is_gm(world) || out.iter().any(|w| &w.id == world) {
            continue;
        }
        out.push(NetWorld {
            local: store.load_world_meta(world).ok().map(|_| "player".into()),
            id: world.clone(),
            name: offer.name.clone(),
            icon: offer.icon.clone(),
            gm: offer.gm.clone(),
            server: server.code(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// What asking a host came to.
#[derive(Serialize)]
pub struct JoinOutcome {
    /// `joined`, `updated`, `current`, `not-member`, `requested`, `not-offered`.
    pub kind: String,
    pub world: Option<WorldInfo>,
}

pub(crate) fn outcome(kind: &str, world: Option<WorldInfo>) -> JoinOutcome {
    JoinOutcome { kind: kind.into(), world }
}

/// Whom a world is swapped with: the device found for it, or a server.
#[derive(Clone)]
enum Via {
    Host,
    Server(Server),
}

impl Via {
    /// What is kept of the last swap, per world and whom it was with.
    fn key(&self, world: &str) -> String {
        match self {
            Via::Host => world.to_string(),
            Via::Server(s) => format!("{world}@{}", s.endpoint),
        }
    }
}

async fn ask(net: &WorldNet, via: &Via, world: &str, me: &Identity, ask: Ask) -> Answer<Reply> {
    let reply = match via {
        Via::Host => net.ask(world, me, ask).await,
        Via::Server(server) => net.ask_server(server, world, me, ask).await,
    }
    .map_err(|e| AppError(e.to_string()))?;
    // Writing the world right now: like no answer — asked again later.
    if reply == Reply::Busy {
        return Err(AppError(BUSY.into()));
    }
    Ok(reply)
}

const BUSY: &str = "The other side is busy with this world.";

/// Asks, and while the other side is busy writing the world, waits and
/// asks again — for a check, which should not end on the usual sync.
#[cfg(feature = "gui")]
async fn ask_waiting(net: &WorldNet, via: &Via, world: &str, me: &Identity, question: Ask) -> Answer<Reply> {
    for _ in 0..120 {
        match ask(net, via, world, me, question.clone()).await {
            Err(e) if e.0 == BUSY => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
            other => return other,
        }
    }
    Err(AppError(BUSY.into()))
}

/// Gives the other side the records it lacks, a little at a time, for one
/// round; what is left the next round gives (`PENDING`). Returns the
/// digest there once all of it is there.
async fn give(store: &Store, net: &WorldNet, via: &Via, me: &Identity, world: &str, wanted: &[String]) -> Answer<Option<String>> {
    let key = via.key(world);
    let order = store.in_order(world, wanted)?;
    let all = store.inventory(world)?.len();
    let started = std::time::Instant::now();
    let mut given = 0;
    let mut digest = None;
    while given < order.len() {
        if started.elapsed() > GIVE_ROUND {
            pending().lock().map_err(AppError::from)?.insert(key, order[given..].to_vec());
            return Ok(None);
        }
        let records = within(store.records(world, &order[given..order.len().min(given + 2000)])?, GIVE_BUDGET);
        let count = records.len();
        match ask(net, via, world, me, Ask::Give { records }).await {
            Ok(Reply::Taken { digest: after }) => digest = Some(after),
            Ok(_) => return Err(AppError("The other side did not take the world.".into())),
            Err(e) => {
                pending().lock().map_err(AppError::from)?.insert(key, order[given..].to_vec());
                return Err(e);
            }
        }
        given += count;
        note_upload(world, |u| u.records = (all - (order.len() - given), all));
    }
    Ok(digest)
}

/// Records up to `budget` bytes — at least one.
fn within(records: Vec<String>, budget: usize) -> Vec<String> {
    let mut size = 0;
    records
        .into_iter()
        .take_while(|r| {
            size += r.len();
            size <= budget || size == r.len()
        })
        .collect()
}

/// Who asked a server to join: kept as if they had asked here.
fn take_requests(store: &Store, world: &str, asking: Vec<IdentityPublic>) -> Answer<()> {
    let meta = store.load_world_meta(world)?;
    let mut requests = store.load_requests(world)?;
    let before = requests.len();
    for who in asking {
        if who.is_genuine() && meta.member(who.id.as_str()).is_none() && !requests.iter().any(|r| r.identity.id == who.id) {
            requests.push(JoinRequest { identity: who, at: now() });
        }
    }
    if requests.len() != before {
        store.save_requests(world, &requests)?;
    }
    Ok(())
}

/// A world worked on online: its host holds it, this device only what it
/// says of the world — kept in the profile (`online.json`), so the world is
/// in the list before the host is asked again.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OnlineWorld {
    pub world: String,
    /// Reached through a server, by its code; none: the host itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// The world as the host shows it to this member (`WorldInfo`).
    pub info: serde_json::Value,
    /// How far the host's world was last time — moved on, the interface loads again.
    #[serde(default)]
    pub version: u64,
}

pub(crate) fn online_worlds(store: &Store) -> Vec<OnlineWorld> {
    std::fs::read(store.profile_dir().join("online.json")).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
}

fn save_online(store: &Store, worlds: &[OnlineWorld]) -> Answer<()> {
    aw_store::durable::write(&store.profile_dir().join("online.json"), &serde_json::to_vec_pretty(worlds)?)?;
    Ok(())
}

/// A world just joined, as the interface lists it — held here, or online.
fn joined_info(store: &Store, world: &str, me: &Identity) -> Answer<serde_json::Value> {
    if store.holds(world) {
        return Ok(serde_json::to_value(world_info(store, &store.load_world_meta(world)?, me.id().as_str())?)?);
    }
    online_worlds(store).into_iter().find(|w| w.world == world).map(|w| w.info).ok_or_else(|| AppError("This world is not here.".into()))
}

/// Forgets a world worked on online.
pub(crate) fn forget_online(store: &Store, world: &str) -> Answer<()> {
    let mut all = online_worlds(store);
    all.retain(|w| w.world != world);
    save_online(store, &all)
}

/// Asks the host to do a command in this member's name.
async fn call_via(net: &WorldNet, me: &Identity, world: &str, via: &Via, command: &str, args: &serde_json::Value) -> Answer<serde_json::Value> {
    match ask(net, via, world, me, Ask::Call { command: command.to_string(), args: args.to_string() }).await? {
        Reply::Called { value: Some(v), .. } => Ok(serde_json::from_str(&v)?),
        Reply::Called { error: Some(e), .. } => Err(AppError(e)),
        Reply::NotMember => Err(AppError("You are no longer a member of this world.".into())),
        _ => Err(AppError("The host does not answer for this world.".into())),
    }
}

fn via_of(entry: &OnlineWorld) -> Via {
    entry.server.as_deref().and_then(|s| Server::parse(s).ok()).map(Via::Server).unwrap_or(Via::Host)
}

/// Takes the world as the host shows it — on joining, and when it moved on.
/// Returns whether it moved on.
async fn refresh_online(store: &Store, net: &WorldNet, me: &Identity, world: &str, via: &Via) -> Answer<bool> {
    let version: u64 = serde_json::from_value(call_via(net, me, world, via, "changes", &serde_json::json!({})).await?)?;
    let mut all = online_worlds(store);
    if all.iter().any(|w| w.world == world && w.version == version) {
        return Ok(false);
    }
    let mut info = call_via(net, me, world, via, "world_info", &serde_json::json!({})).await?;
    info["online"] = true.into();
    let server = match via {
        Via::Server(s) => Some(s.code()),
        Via::Host => None,
    };
    all.retain(|w| w.world != world);
    all.push(OnlineWorld { world: world.to_string(), server, info, version });
    save_online(store, &all)?;
    Ok(true)
}

#[cfg(feature = "gui")]
/// A command for a world worked on online — done by its host, in this
/// member's name.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn remote_call(app: tauri::AppHandle, world: String, command: String, args: serde_json::Value) -> Answer<serde_json::Value> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let net = running().ok_or_else(|| AppError("This world is online — switch the network on.".into()))?;
    let entry = online_worlds(&store).into_iter().find(|w| w.world == world).ok_or_else(|| AppError("This world is not online.".into()))?;
    call_via(&net, &me, &world, &via_of(&entry), &command, &args).await
}

/// The worlds the servers hold, by id: which server, and the world as it is named there.
static SERVER_WORLDS: OnceLock<Mutex<HashMap<String, (Server, Offer)>>> = OnceLock::new();

fn server_worlds() -> &'static Mutex<HashMap<String, (Server, Offer)>> {
    SERVER_WORLDS.get_or_init(Default::default)
}

/// Swaps records with the host of a world — the whole of it the first time.
/// Returns what came of it: `joined`, `updated`, `current`, `not-member`,
/// `not-offered` — and `requested`: a world brought to a server waits for
/// its admin.
async fn swap(store: &Store, net: &WorldNet, via: &Via, me: &Identity, world: &str) -> Answer<String> {
    if store::moving(world) {
        return Ok("current".into());
    }
    let presence = mine()
        .lock()
        .map_err(AppError::from)?
        .get(world)
        .filter(|(at, _)| at.elapsed() < PRESENT)
        .and_then(|(_, p)| serde_json::to_string(p).ok());
    let me_id = me.id().to_string();
    let rolls = recent_rolls(world, |r| r.member == me_id).iter().filter_map(|r| serde_json::to_string(r).ok()).collect();
    let theirs = match ask(net, via, world, me, Ask::Digest { presence, rolls }).await? {
        Reply::Digest { digest, presence, rolls, handover, requests, no_copy } => {
            // Held only while online, or kept — as the world says now.
            if !store.is_gm(world) {
                store.hold_fleeting(world, no_copy)?;
            }
            if !requests.is_empty() && store.is_gm(world) {
                take_requests(store, world, requests)?;
            }
            for p in presence.iter().filter_map(|p| serde_json::from_str::<Presence>(p).ok()) {
                note_presence(world, p);
            }
            for r in rolls.iter().filter_map(|r| serde_json::from_str::<Roll>(r).ok()) {
                note_roll(world, r);
            }
            if handover {
                take_over(store, net, via, me, world).await?;
            }
            digest
        }
        Reply::NotMember => return Ok("not-member".into()),
        Reply::Requested => return Ok("requested".into()),
        // It stays with its host: worked on online from now on.
        Reply::OnlineOnly => {
            let was = online_worlds(store).iter().any(|w| w.world == world);
            let moved = refresh_online(store, net, me, world, via).await?;
            return Ok(if !was { "joined" } else if moved { "updated" } else { "current" }.into());
        }
        _ => return Ok("not-offered".into()),
    };
    // Read only now: taking the world over adds records of our own.
    let held = store.holds(world);
    let key = via.key(world);
    // Brought over several rounds: on where the last one stopped.
    let rest = pending().lock().map_err(AppError::from)?.remove(&key);
    if let Some(rest) = rest {
        if let Some(digest) = give(store, net, via, me, world, &rest).await? {
            let now = Journal::digest(&store.inventory(world)?);
            swapped().lock().map_err(AppError::from)?.insert(key, (digest, now));
            swap_parts(store, net, via, me, world, true).await?;
        }
        return Ok("updated".into());
    }
    let have = store.inventory(world)?;
    let ours = Journal::digest(&have);
    if swapped().lock().map_err(AppError::from)?.get(&key) == Some(&(theirs, ours)) {
        let parts = swap_parts(store, net, via, me, world, false).await?;
        return Ok(if parts > 0 { "updated" } else { "current" }.into());
    }
    let first = Ask::Swap { have: have.clone(), give: Vec::new() };
    let Reply::Records { records, wanted, mut digest } = ask(net, via, world, me, first).await? else {
        return Ok("not-member".into());
    };
    let new = store.receive(world, &records)?;
    if !wanted.is_empty() {
        match give(store, net, via, me, world, &wanted).await? {
            Some(after) => digest = after,
            // A world new to the other side comes over several rounds.
            None => return Ok(if !held { "joined" } else { "updated" }.into()),
        }
    }
    // We handed the world over, and the other side has taken it.
    if store.handed_over(world)? {
        store.step_down(world)?;
        republish();
    }
    let now = Journal::digest(&store.inventory(world)?);
    swapped().lock().map_err(AppError::from)?.insert(key, (digest, now));
    // New gallery entries on either side: the parts follow.
    let parts = swap_parts(store, net, via, me, world, true).await?;
    Ok(if !held { "joined" } else if new > 0 || parts > 0 { "updated" } else { "current" }.into())
}

/// Swaps image parts with the host until neither lacks what the other has —
/// asking at least once after records came or went (`fresh`): the host may
/// lack the parts of an image uploaded here. Returns how many came here.
async fn swap_parts(store: &Store, net: &WorldNet, via: &Via, me: &Identity, world: &str, fresh: bool) -> Answer<usize> {
    if !store.holds(world) {
        return Ok(0);
    }
    let key = via.key(world);
    let mut theirs = wants().lock().map_err(AppError::from)?.remove(&key).unwrap_or_default();
    let mut want = store.missing_parts(world)?;
    let mut new = 0;
    // Bringing one's own world to its server: how far its images are.
    let bringing = matches!(via, Via::Server(_)) && store.is_gm(world);
    let all = if bringing { store.wanted_parts(world)?.len().saturating_sub(want.len()) } else { 0 };
    let started = std::time::Instant::now();
    for round in 0..256 {
        if want.is_empty() && theirs.is_empty() && !(fresh && round == 0) {
            break;
        }
        // The rest the next round brings.
        if round > 0 && started.elapsed() > GIVE_ROUND {
            break;
        }
        let give = encode_parts(parts_within(store, world, &theirs));
        let gave = give.len();
        let ask = Ask::Parts { want: want.clone(), give };
        let Reply::Parts { parts, want: lacking } = self::ask(net, via, world, me, ask).await? else {
            break;
        };
        let got = store.take_parts(world, &decode_parts(parts))?;
        new += got;
        let here = store.missing_parts(world)?;
        // Of what the host lacks, what lies here.
        theirs = lacking.into_iter().filter(|p| !here.contains(p)).collect();
        if bringing {
            note_upload(world, |u| u.images = (all.saturating_sub(theirs.len()), all));
        }
        if got == 0 && gave == 0 && here == want {
            break;
        }
        want = here;
    }
    if !theirs.is_empty() {
        wants().lock().map_err(AppError::from)?.insert(key, theirs);
    }
    Ok(new)
}

/// Takes over the world the host hands to us. The swap that follows brings
/// the host the record naming us game master — and with it, it steps down.
async fn take_over(store: &Store, net: &WorldNet, via: &Via, me: &Identity, world: &str) -> Answer<()> {
    let Reply::Handover { package } = ask(net, via, world, me, Ask::Handover).await? else {
        return Ok(());
    };
    let package: store::HandoverPackage = serde_json::from_str(&package).map_err(AppError::from)?;
    store.handover_take(&package, me)?;
    offer_taken_over(store, world)
}

/// A world taken over stays where its players find it: offered on the
/// network, if the network is on.
pub(crate) fn offer_taken_over(store: &Store, world: &str) -> Answer<()> {
    let mut settings = store.load_network()?;
    if settings.enabled && !settings.shared.iter().any(|w| w == world) {
        settings.shared.push(world.to_string());
        store.save_network(&settings)?;
        republish();
    }
    Ok(())
}

#[cfg(feature = "gui")]
/// Asks the host of a found world for it — or, with `join`, to become a member.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_join(app: tauri::AppHandle, world: String, join: bool) -> Answer<JoinOutcome> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let net = running().ok_or_else(|| AppError("The network is switched off.".into()))?;
    // Found on the network, or held by a server.
    let via = match server_worlds().lock().map_err(AppError::from)?.get(&world) {
        Some((server, _)) if net.host_of(&world).is_none() => Via::Server(server.clone()),
        _ => Via::Host,
    };
    if join {
        return Ok(match ask(&net, &via, &world, &me, Ask::Join).await? {
            Reply::Requested => {
                asked().lock().map_err(AppError::from)?.insert(world);
                outcome("requested", None)
            }
            Reply::NotOffered => outcome("not-offered", None),
            // Already a member — let in ahead of time, say.
            _ => {
                let kind = swap(&store, &net, &via, &me, &world).await?;
                let info = world_info(&store, &store.load_world_meta(&world)?, me.id().as_str()).ok();
                outcome(&kind, info)
            }
        });
    }
    let kind = swap(&store, &net, &via, &me, &world).await?;
    let info = if store.holds(&world) {
        Some(world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())?)
    } else {
        None
    };
    Ok(outcome(&kind, info))
}

/// Someone writing in the same article, as this reader sees it.
#[derive(Serialize)]
pub struct Seen {
    pub id: String,
    pub name: String,
    /// Where their caret stands — none when it stands in no text this
    /// reader has.
    pub caret: Option<document::CaretAt>,
}

#[cfg(feature = "gui")]
/// Tells where one writes — and learns where the others do.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn presence(app: tauri::AppHandle, world: String, article: String, caret: Option<document::Caret>) -> Answer<Vec<Seen>> {
    presence_in(&store(&app)?, world, article, caret)
}

/// `presence` for whom the store is given — the host, acting for a player.
pub(crate) fn presence_in(store: &Store, world: String, article: String, caret: Option<document::Caret>) -> Answer<Vec<Seen>> {
    let store = store.clone();
    let me = store.load_identity()?;
    let (layer, anchor, head) = match caret {
        Some(caret) => match document::caret_in(&store, &me, &world, &article, caret)? {
            Some((layer, start, end)) => (
                layer.to_string(),
                store.cursor(&world, layer, start)?,
                store.cursor(&world, layer, end)?,
            ),
            None => Default::default(),
        },
        None => Default::default(),
    };
    let (me_id, me_name) = who_am_i(&me);
    let own = Presence { member: me_id.clone(), name: me_name, article: article.clone(), layer, anchor, head, editing: true };
    tell_presence(&store, &world, own)?;
    let mut out = Vec::new();
    for p in present(&world, &me_id).into_iter().filter(|p| p.article == article) {
        let caret = match p.layer.parse::<aw_model::LayerId>() {
            Ok(layer) => {
                let at = |c: &str| store.position(&world, layer, c).ok();
                match (at(&p.anchor), at(&p.head)) {
                    (Some(a), Some(h)) => document::caret_at(&store, &me, &world, &article, layer, a, h)?,
                    _ => None,
                }
            }
            Err(_) => None,
        };
        out.push(Seen { id: p.member, name: p.name, caret });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Who one is here: this device — or, on a host acting for a player, them.
fn who_am_i(me: &Identity) -> (String, String) {
    match crate::host::acting() {
        Some(a) => (a.member.id.to_string(), a.member.name.clone()),
        None => (me.id().to_string(), me.name().to_string()),
    }
}

/// Where one is oneself: with the game master's device among the others;
/// with a player's, for the next digest.
fn tell_presence(store: &Store, world: &str, own: Presence) -> Answer<()> {
    // The host keeps it among the others — also what a player tells it online.
    if store.holds_token(world) {
        note_presence(world, own);
    } else {
        mine().lock().map_err(AppError::from)?.insert(world.to_string(), (std::time::Instant::now(), own));
    }
    Ok(())
}

/// Whether one writes somewhere in the world right now — the editor says so
/// more often than the page does.
fn writing_now(store: &Store, world: &str, me: &str) -> bool {
    let fresh = std::time::Duration::from_secs(2);
    if store.is_gm(world) {
        live().lock().is_ok_and(|all| all.get(world).and_then(|w| w.get(me)).is_some_and(|(at, p)| p.editing && at.elapsed() < fresh))
    } else {
        mine().lock().is_ok_and(|m| m.get(world).is_some_and(|(at, p)| p.editing && at.elapsed() < fresh))
    }
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

/// `world_presence` for whom the store is given — the host, acting for a player.
pub(crate) fn world_presence_in(store: &Store, world: String, article: String) -> Answer<Vec<Here>> {
    let store = store.clone();
    let me = store.load_identity()?;
    let (me_id, me_name) = who_am_i(&me);
    if !writing_now(&store, &world, &me_id) {
        let own = Presence { member: me_id.clone(), name: me_name, article, layer: String::new(), anchor: String::new(), head: String::new(), editing: false };
        tell_presence(&store, &world, own)?;
    }
    let mut out: Vec<Here> = present(&world, &me_id).into_iter().map(|p| Here { id: p.member, name: p.name, article: p.article, editing: p.editing }).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// What one round of keeping up brought.
#[derive(Serialize, Default)]
pub struct SyncOutcome {
    /// Worlds that arrived because the GM said yes.
    pub joined: Vec<serde_json::Value>,
    /// Worlds held here that changed — by a swap, or because someone
    /// swapped with us.
    pub updated: Vec<String>,
}

#[cfg(feature = "gui")]
/// Keeps the worlds held here — and those asked for — current, both ways:
/// with the hosts on the local network and those met through shared folders.
/// Asked every few seconds; costs a digest per world unless something changed.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn network_sync(app: tauri::AppHandle) -> Answer<SyncOutcome> {
    let mut out = SyncOutcome::default();
    if SYNCING.swap(true, Ordering::AcqRel) {
        return Ok(out);
    }
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            SYNCING.store(false, Ordering::Release);
        }
    }
    let _done = Done;

    let store = store(&app)?;
    let me = store.load_identity()?;
    if let Some(net) = running() {
        let _ = meet_through_folder(&store, &net, &me);
        let waiting = asked().lock().map_err(AppError::from)?.clone();
        for offer in net.found() {
            let world = offer.world;
            let host = net.host_of(&world).unwrap_or_default();
            if FAILED.get_or_init(Default::default).lock().map_err(AppError::from)?.get(&host).is_some_and(|t| t.elapsed() < RETRY) {
                continue;
            }
            // Held as game master and still offered by someone else: one of
            // the two handed it over, and the swap settles which.
            let held = store.holds(&world);
            if !(held || waiting.contains(&world)) {
                continue;
            }
            // A host that does not answer now may well answer a little later.
            let Ok(kind) = swap(&store, &net, &Via::Host, &me, &world).await else {
                FAILED.get_or_init(Default::default).lock().map_err(AppError::from)?.insert(host, std::time::Instant::now());
                continue;
            };
            if kind == "joined" {
                asked().lock().map_err(AppError::from)?.remove(&world);
                out.joined.push(joined_info(&store, &world, &me)?);
            }
        }
        for world in with_servers(&store, &net, &me, &waiting).await? {
            asked().lock().map_err(AppError::from)?.remove(&world);
            out.joined.push(joined_info(&store, &world, &me)?);
        }
        // Worlds worked on online: has the host's moved on?
        for entry in online_worlds(&store) {
            if let Ok(true) = refresh_online(&store, &net, &me, &entry.world, &via_of(&entry)).await {
                out.updated.push(entry.world);
            }
        }
    }
    // What changed here, whoever brought it.
    for meta in store.list_worlds()? {
        let id = meta.public.world_id.to_string();
        // What players asked to be made, the GM's device makes.
        let _ = crate::answer_proposals(&store, &id);
        // Unencrypted: the readable copy follows, both ways.
        crate::readable::refresh_soon(&store, &id);
        let count = store.inventory(&id)?.len();
        let before = seen().lock().map_err(AppError::from)?.insert(id.clone(), count);
        if before.is_some_and(|b| b != count) {
            out.updated.push(id);
        }
    }
    Ok(out)
}

/// Keeps worlds current with the servers too. A server holds what the GM
/// brings it — only the worlds they offer — and passes it on to members.
/// Returns the worlds that arrived because the GM said yes.
async fn with_servers(store: &Store, net: &WorldNet, me: &Identity, waiting: &HashSet<String>) -> Answer<Vec<String>> {
    let settings = store.load_network()?;
    let mut joined = Vec::new();
    for code in &settings.servers {
        let Ok(server) = Server::parse(code) else { continue };
        let failed = || FAILED.get_or_init(Default::default).lock().map_err(AppError::from);
        if failed()?.get(&server.endpoint).is_some_and(|t| t.elapsed() < RETRY) {
            continue;
        }
        let (held, host, admin, may_bring, asked) = match net.ask_server(&server, "", me, Ask::Worlds).await {
            Ok(Reply::Worlds { worlds, host, admin, may_bring, asked }) => (worlds, host, admin, may_bring, asked),
            // Turned away, or no server at all.
            Ok(_) => {
                server_status().lock().map_err(AppError::from)?.entry(server.code()).or_default().reached = true;
                continue;
            }
            Err(_) => {
                failed()?.insert(server.endpoint.clone(), std::time::Instant::now());
                server_status().lock().map_err(AppError::from)?.entry(server.code()).or_default().reached = false;
                continue;
            }
        };
        let name = host.as_ref().map(|h| h.name.clone()).unwrap_or_default();
        let mut status = ServerStatus { name, admin, waiting: Vec::new(), may_bring, asked, reached: true };
        {
            let mut known = server_worlds().lock().map_err(AppError::from)?;
            known.retain(|_, (s, _)| s.endpoint != server.endpoint);
            for offer in &held {
                known.insert(offer.world.clone(), (server.clone(), offer.clone()));
            }
        }
        let there: HashSet<&str> = held.iter().map(|o| o.world.as_str()).collect();
        let mut worlds: Vec<String> = store.list_worlds()?.into_iter().map(|m| m.public.world_id.to_string()).collect();
        // Asked for, and held only while online — let go when the app last ended.
        for world in waiting.iter().cloned().chain(store.fleeting_worlds()) {
            if there.contains(world.as_str()) && !worlds.contains(&world) {
                worlds.push(world);
            }
        }
        for world in worlds {
            // The GM brings a server only what they offer; a member takes
            // part where the server holds the world.
            let gm = store.is_gm(&world);
            // A GM's world goes to the server it lives on, and nowhere else:
            // one living on this device stays here.
            let home = settings.homes.get(&world);
            let wanted = if gm {
                home.is_some_and(|h| *h == server.endpoint)
            } else {
                there.contains(world.as_str()) && home.is_none_or(|h| *h == server.endpoint)
            };
            if !wanted {
                // Held only while online, and the server has it no more: gone here too.
                if !gm && store.is_fleeting(&world) && server_worlds().lock().map_err(AppError::from)?.get(&world).is_none() {
                    store.let_go(&world, false)?;
                }
                continue;
            }
            // Unencrypted, the server writes in its players' names: the GM vouches for it.
            if let (true, Some(host)) = (gm, &host) {
                if !store.is_encrypted(&world)? {
                    store.add_host(&world, host.id.as_str())?;
                }
            }
            let kind = swap(store, net, &Via::Server(server.clone()), me, &world).await;
            // A member's world comes from this server: it lives there.
            if let (false, None, Ok(k)) = (gm, home, &kind) {
                if !matches!(k.as_str(), "not-member" | "not-offered" | "requested") {
                    live_on_here(store, &world, &server.endpoint)?;
                }
            }
            match kind {
                Ok(kind) if kind == "joined" => joined.push(world),
                Ok(kind) if kind == "requested" => status.waiting.push(world),
                // No member any more: for good. Not offered: until it is again.
                Ok(kind) if kind == "not-member" => store.let_go(&world, true)?,
                Ok(kind) if kind == "not-offered" => store.let_go(&world, false)?,
                Ok(_) => {}
                Err(_) => {
                    failed()?.insert(server.endpoint.clone(), std::time::Instant::now());
                    break;
                }
            }
        }
        server_status().lock().map_err(AppError::from)?.insert(server.code(), status);
    }
    Ok(joined)
}

/// What a server said of this device last time: whether it is its admin,
/// and which of the GM's worlds wait for the admin.
#[derive(Clone, Default)]
struct ServerStatus {
    name: String,
    admin: bool,
    waiting: Vec<String>,
    may_bring: bool,
    asked: bool,
    reached: bool,
}

static SERVER_STATUS: OnceLock<Mutex<HashMap<String, ServerStatus>>> = OnceLock::new();

fn server_status() -> &'static Mutex<HashMap<String, ServerStatus>> {
    SERVER_STATUS.get_or_init(Default::default)
}

/// Notes that a member's world lives on a server, by its endpoint.
fn live_on_here(store: &Store, world: &str, endpoint: &str) -> Answer<()> {
    let mut settings = store.load_network()?;
    if settings.homes.get(world).map(String::as_str) != Some(endpoint) {
        settings.homes.insert(world.to_string(), endpoint.to_string());
        store.save_network(&settings)?;
    }
    Ok(())
}

/// Lets a GM's world live on a server — offered there alone — or on this
/// device again (`None`). The world is offered either way.
pub(crate) fn live_on(store: &Store, world: &str, server: Option<String>) -> Answer<()> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM decides that.".into()));
    }
    let mut settings = store.load_network()?;
    match server {
        Some(code) => {
            let server = Server::parse(&code).map_err(|e| AppError(e.to_string()))?;
            if !settings.servers.contains(&server.code()) {
                return Err(AppError("Add the server first.".into()));
            }
            settings.homes.insert(world.to_string(), server.endpoint);
            if !settings.shared.iter().any(|w| w == world) {
                settings.shared.push(world.to_string());
            }
        }
        None => {
            settings.homes.remove(world);
        }
    }
    store.save_network(&settings)?;
    republish();
    Ok(())
}

/// Moves a GM's world onto a server, or off it (`None`). The server it
/// leaves is asked once more for what players wrote there — not reached, the
/// world stays — and then lets it go.
pub(crate) async fn rehome(store: &Store, world: &str, server: Option<String>) -> Answer<()> {
    let before = store.load_network()?.home_of(world).cloned();
    if before == server {
        return Ok(());
    }
    let old = before.as_deref().map(Server::parse).transpose().map_err(|e| AppError(e.to_string()))?;
    let me = store.load_identity()?;
    let net = running();
    if let Some(old) = &old {
        let accepted = !status(&old.code()).waiting.iter().any(|w| w == world);
        if accepted {
            let net = net.as_ref().ok_or_else(|| AppError("The network is switched off.".into()))?;
            swap(store, net, &Via::Server(old.clone()), &me, world)
                .await
                .map_err(|_| AppError("Its server is not reached — what players wrote there would stay behind.".into()))?;
        }
    }
    live_on(store, world, server)?;
    if let (Some(old), Some(net)) = (old, net) {
        let _ = net.ask_server(&old, "", &me, Ask::Manage { order: Some(aw_sync::world::Order::Delete { world: world.to_string() }) }).await;
    }
    Ok(())
}

/// What a server holds of a GM's world, measured against this device.
#[derive(Serialize, Default)]
pub struct Check {
    /// Entries here.
    pub records: usize,
    /// Of these, not on the server.
    pub missing: usize,
    /// On the server and not here — taken in now.
    pub new: usize,
    /// Image parts the world needs.
    pub parts: usize,
    /// Came back from the server whole: their bytes are what their names say.
    pub intact: usize,
    /// Not on the server, or not whole there — given again.
    pub lacking: usize,
    /// Of these, whole on the server now.
    pub repaired: usize,
}

#[cfg(feature = "gui")]
/// Whether all of a GM's world is on its server. Entries by name — each was
/// checked against its name when the server took it; image parts by their
/// bytes: every part comes back and is checked. Parts missing there, or no
/// longer whole, are given again — the usual sync only asks for missing
/// ones. Entries missing there follow with the next sync. Tells how far it
/// is (`copy-progress`, `checking`).
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn world_check(app: tauri::AppHandle, world: String) -> Answer<Check> {
    use tauri::Emitter;
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM checks a world.".into()));
    }
    let code = store.load_network()?.home_of(&world).cloned().ok_or_else(|| AppError("It lives on no server.".into()))?;
    let via = Via::Server(Server::parse(&code).map_err(|e| AppError(e.to_string()))?);
    let net = running().ok_or_else(|| AppError("The network is switched off.".into()))?;
    let me = store.load_identity()?;
    let mut check = Check::default();

    let have = store.inventory(&world)?;
    check.records = have.len();
    let Reply::Records { records, wanted, .. } = ask_waiting(&net, &via, &world, &me, Ask::Swap { have, give: Vec::new() }).await? else {
        return Err(AppError("The server does not hold it.".into()));
    };
    check.missing = wanted.len();
    check.new = store.receive(&world, &records)?;

    let mut pending: std::collections::VecDeque<String> = store.wanted_parts(&world)?.into_iter().collect();
    let mut lacking = Vec::new();
    check.parts = pending.len();
    while !pending.is_empty() {
        let want: Vec<String> = pending.iter().take(64).cloned().collect();
        let Reply::Parts { parts, .. } = ask_waiting(&net, &via, &world, &me, Ask::Parts { want: want.clone(), give: Vec::new() }).await? else {
            return Err(AppError("The server does not hold it.".into()));
        };
        let got = decode_parts(parts);
        let names: HashSet<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        // In order, up to the last that came: the rest did not fit this time.
        let settled = want.iter().rposition(|n| names.contains(n.as_str())).map_or(want.len(), |i| i + 1);
        for (name, bytes) in &got {
            if aw_store::journal::record_id(bytes) == *name { check.intact += 1 } else { lacking.push(name.clone()) }
        }
        lacking.extend(want[..settled].iter().filter(|n| !names.contains(n.as_str())).cloned());
        pending.drain(..settled);
        let _ = app.emit("copy-progress", ("checking", check.parts - pending.len(), check.parts));
    }
    check.lacking = lacking.len();
    // Given again, a few at a time, then asked for once more.
    for batch in lacking.chunks(4) {
        let give = encode_parts(store.parts(&world, batch));
        ask_waiting(&net, &via, &world, &me, Ask::Parts { want: Vec::new(), give }).await?;
        let Reply::Parts { parts, .. } = ask_waiting(&net, &via, &world, &me, Ask::Parts { want: batch.to_vec(), give: Vec::new() }).await? else {
            continue;
        };
        check.repaired += decode_parts(parts).iter().filter(|(name, bytes)| aw_store::journal::record_id(bytes) == *name).count();
    }
    Ok(check)
}

#[cfg(feature = "gui")]
/// Asks a server's admin to let this device bring worlds there.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn server_host_worlds(app: tauri::AppHandle, server: String) -> Answer<NetworkView> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let net = running().ok_or_else(|| AppError("The network is switched off.".into()))?;
    let server = Server::parse(&server).map_err(|e| AppError(e.to_string()))?;
    ask(&net, &Via::Server(server.clone()), "", &me, Ask::HostWorlds).await?;
    if let Ok(mut all) = server_status().lock() {
        all.entry(server.code()).or_default().asked = true;
    }
    view(&store)
}

fn status(code: &str) -> ServerStatus {
    server_status().lock().ok().and_then(|s| s.get(code).cloned()).unwrap_or_default()
}

#[cfg(feature = "gui")]
/// What a server holds and who uses it — for its admin; with an order,
/// carried out first.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn server_manage(app: tauri::AppHandle, server: String, order: Option<aw_sync::world::Order>) -> Answer<aw_sync::world::ServerState> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let net = running().ok_or_else(|| AppError("The network is switched off.".into()))?;
    let server = Server::parse(&server).map_err(|e| AppError(e.to_string()))?;
    match ask(&net, &Via::Server(server), "", &me, Ask::Manage { order }).await? {
        Reply::Managed { state } => Ok(state),
        _ => Err(AppError("Only the server's admin manages it.".into())),
    }
}

#[derive(Serialize)]
pub struct RequestInfo {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub at: u64,
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command)]
pub fn join_requests(app: tauri::AppHandle, world: String) -> Answer<Vec<RequestInfo>> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Ok(Vec::new());
    }
    Ok(store
        .load_requests(&world)?
        .into_iter()
        .map(|r| RequestInfo {
            id: r.identity.id.to_string(),
            name: r.identity.name.clone(),
            fingerprint: r.identity.fingerprint(),
            at: r.at,
        })
        .collect())
}

#[cfg(feature = "gui")]
/// The game master takes someone in — or turns them away.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn join_answer(app: tauri::AppHandle, world: String, id: String, accept: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM can let someone in.".into()));
    }
    let mut requests = store.load_requests(&world)?;
    let request = requests
        .iter()
        .find(|r| r.identity.id.as_str() == id)
        .cloned()
        .ok_or_else(|| AppError("There is no such request.".into()))?;
    requests.retain(|r| r.identity.id.as_str() != id);
    store.save_requests(&world, &requests)?;
    let meta = if accept {
        admit(&store, &world, request.identity, Vec::new())?
    } else {
        store.load_world_meta(&world)?
    };
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Lets a player in ahead of time, by their id alone. Their keys come when
/// they first show up on the network — and only keys that make that id.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn member_invite(app: tauri::AppHandle, world: String, id: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM can let someone in.".into()));
    }
    let id = id.trim().to_string();
    let valid = id
        .strip_prefix("id:")
        .is_some_and(|body| body.len() == 16 && body.chars().all(|c| c.is_ascii_hexdigit()));
    if !valid {
        return Err(AppError(
            "That is no player id. It is the whole id — id: and 16 characters. The short form is only for reading aloud.".into(),
        ));
    }
    let mut meta = store.load_world_meta(&world)?;
    if meta.member(&id).is_none() && !meta.invited.contains(&id) {
        meta.invited.push(id);
        store.save_world_meta(&meta)?;
    }
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stores() -> (tempfile::TempDir, Store, Identity, Store, Identity) {
        let dir = tempfile::tempdir().unwrap();
        let gm_store = Store::new(dir.path().join("gm"));
        let gm = Identity::create("Marla").unwrap();
        gm_store.save_identity(&gm).unwrap();
        let player_store = Store::new(dir.path().join("bob"));
        let bob = Identity::create("Bob").unwrap();
        player_store.save_identity(&bob).unwrap();
        (dir, gm_store, gm, player_store, bob)
    }

    /// What `swap` does over the network, without it.
    fn swap_here(gm_store: &Store, player_store: &Store, bob: &Identity, world: &str) -> Reply {
        let have = player_store.inventory(world).unwrap();
        let reply = answer(gm_store, world, &bob.public(), Ask::Swap { have, give: Vec::new() }).unwrap();
        let Reply::Records { records, wanted, .. } = &reply else { return reply };
        player_store.receive(world, records).unwrap();
        let give = player_store.records(world, wanted).unwrap();
        let have = player_store.inventory(world).unwrap();
        answer(gm_store, world, &bob.public(), Ask::Swap { have, give }).unwrap()
    }

    fn text(store: &Store, who: &Identity, world: &str, article: &str) -> String {
        document::code_of(store, who, world, article).unwrap().code
    }

    #[test]
    fn a_world_moves_to_another_folder_with_all_it_holds_and_is_written_there_after() {
        let dir = tempfile::tempdir().unwrap();
        let (local, shared) = (dir.path().join("local"), dir.path().join("drive"));
        std::fs::create_dir_all(&local).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        let store = Store::with_roots(dir.path().join("profile"), vec![local.clone(), shared.clone()]);
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let world = new_world(&store.destination(&local).unwrap(), &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        let page = create_page(&store, &world, "notes", "Notes", "The harbour.".into()).unwrap();

        store.move_world(&world, &shared, &|_, _| {}).unwrap();
        assert!(in_shared_folder(&store, &world));
        assert!(!local.join(store::dir_name(&world)).exists(), "nothing stays behind");
        assert!(!store::moving(&world));
        assert_eq!(store.list_worlds().unwrap().len(), 1);
        assert!(store.load_articles(&world).unwrap().iter().any(|a| a.id.to_string() == page));

        // Written to afterwards: in its new place, not in the old.
        create_page(&store, &world, "more", "More", String::new()).unwrap();
        assert!(!local.join(store::dir_name(&world)).exists());
        assert_eq!(store.load_articles(&world).unwrap().len(), 3);

        // Where it lies already, nothing happens; elsewhere than a location, nothing either.
        store.move_world(&world, &shared, &|_, _| {}).unwrap();
        assert!(store.move_world(&world, &dir.path().join("elsewhere"), &|_, _| {}).is_err());
        assert!(in_shared_folder(&store, &world));
    }

    #[test]
    fn folder_notes_take_the_members_genuine_notes_and_replace_their_own() {
        let dir = tempfile::tempdir().unwrap();
        let (local, shared) = (dir.path().join("local"), dir.path().join("drive"));
        std::fs::create_dir_all(&local).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        let store = Store::with_roots(dir.path().join("profile"), vec![local, shared.clone()]);
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let world = new_world(&store.destination(&shared).unwrap(), &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        assert!(in_shared_folder(&store, &world));
        let reach = |addr: &str| Some(("e".repeat(52), vec![addr.to_string()]));
        folder_notes(&store, &world, &gm, reach("1.2.3.4:5")).unwrap();
        let peers = store.world_dir(&world).join("peers");
        assert_eq!(std::fs::read_dir(&peers).unwrap().count(), 1, "the own note lies in the folder");

        // Bob's note counts; Eve's in Bob's name does not.
        let bob = Identity::create("Bob").unwrap();
        let note = aw_sync::admission::PeerNote::new(&world, "b0b0b0b0b0b0b0b0b0", vec!["5.6.7.8:9".into()], &bob, 7).unwrap();
        std::fs::write(peers.join("peer-b0b0b0b0b0b0b0b0-7.json"), serde_json::to_string(&note).unwrap()).unwrap();
        let eve = Identity::create("Eve").unwrap();
        let forged = aw_sync::admission::PeerNote { identity: bob.public(), ..aw_sync::admission::PeerNote::new(&world, "e7e7e7e7e7e7e7e7e7", vec![], &eve, 8).unwrap() };
        std::fs::write(peers.join("peer-e7e7e7e7e7e7e7e7-8.json"), serde_json::to_string(&forged).unwrap()).unwrap();
        let notes = folder_notes(&store, &world, &gm, reach("1.2.3.4:5")).unwrap();
        assert!(notes.iter().any(|n| n.endpoint.starts_with("b0b0")), "the member's note is read");
        assert!(!notes.iter().any(|n| n.endpoint.starts_with("e7e7")), "the forged one is not");

        // At a new address the own note is written anew, the old one goes.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        folder_notes(&store, &world, &gm, reach("9.9.9.9:9")).unwrap();
        let own: Vec<_> = std::fs::read_dir(&peers).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("peer-eeee")).collect();
        assert_eq!(own.len(), 1, "one note of one's own: {own:?}");
    }

    #[test]
    fn who_is_in_the_world_and_who_writes() {
        let (_dir, gm_store, gm, _player_store, bob) = stores();
        let world = new_world(&gm_store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        let at = |article: &str, editing: bool, who: &Identity| Presence {
            member: who.id().to_string(),
            name: who.name().to_string(),
            article: article.into(),
            layer: String::new(),
            anchor: String::new(),
            head: String::new(),
            editing,
        };
        tell_presence(&gm_store, &world, at("art:page", true, &gm)).unwrap();
        assert!(writing_now(&gm_store, &world, gm.id().as_str()), "the editor says so");
        note_presence(&world, at("art:inn", false, &bob));
        let here = present(&world, gm.id().as_str());
        assert_eq!(here.len(), 1);
        assert_eq!((here[0].name.as_str(), here[0].article.as_str(), here[0].editing), ("Bob", "art:inn", false));
    }

    #[test]
    fn a_stranger_asks_the_gm_decides_the_world_arrives_and_what_bob_writes_comes_back() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();

        assert_eq!(answer(&gm_store, &world, &bob.public(), Ask::Digest { presence: None, rolls: Vec::new() }).unwrap(), Reply::NotMember);
        assert_eq!(swap_here(&gm_store, &player_store, &bob, &world), Reply::NotMember, "no records for strangers");
        assert_eq!(answer(&gm_store, &world, &bob.public(), Ask::Join).unwrap(), Reply::Requested);
        let requests = gm_store.load_requests(&world).unwrap();
        assert_eq!(requests.len(), 1, "the request waits for the GM");
        let info = world_info(&gm_store, &gm_store.load_world_meta(&world).unwrap(), gm.id().as_str()).unwrap();
        assert_eq!(info.requests, 1, "and counts for the badge");

        admit(&gm_store, &world, requests[0].identity.clone(), Vec::new()).unwrap();
        assert!(gm_store.load_requests(&world).unwrap().is_empty(), "answered requests are gone");
        swap_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(player_store.load_world_meta(&world).unwrap().public.name, "Eldermere");
        let home = meta.home.clone();
        assert!(text(&player_store, &bob, &world, &home).contains("<name=Eldermere>"));

        // Bob may not write the page — it is the GM's to edit.
        let code = document::code_of(&player_store, &bob, &world, &home).unwrap();
        let mine = code.code.replace("<block>\n", "<block>\nBob was here.\n");
        assert!(document::save(&player_store, &bob, &world, &home, &mine, &code.base, "s-bob").is_err());

        // The GM opens it to players: that arrives, and Bob's writing comes back.
        let code = document::code_of(&gm_store, &gm, &world, &home).unwrap();
        let open = code.code.replace("<block>", "<block view=all edit=players>");
        document::save(&gm_store, &gm, &world, &home, &open, &code.base, "s-gm").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        let code = document::code_of(&player_store, &bob, &world, &home).unwrap();
        let mine = code.code.replace("edit=players>\n", "edit=players>\nBob was here.\n");
        document::save(&player_store, &bob, &world, &home, &mine, &code.base, "s-bob").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(text(&gm_store, &gm, &world, &home).contains("Bob was here."), "{}", text(&gm_store, &gm, &world, &home));

        // Once both have everything, the digests agree.
        let digest = |r: Reply| match r {
            Reply::Digest { digest, .. } => digest,
            other => panic!("{other:?}"),
        };
        let theirs = digest(answer(&gm_store, &world, &bob.public(), Ask::Digest { presence: None, rolls: Vec::new() }).unwrap());
        assert_eq!(theirs, Journal::digest(&player_store.inventory(&world).unwrap()));
    }

    #[test]
    fn dice_reach_the_others_but_only_in_ones_own_name() {
        let (_dir, gm_store, gm, _player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        answer(&gm_store, &world, &bob.public(), Ask::Join).unwrap();
        let requests = gm_store.load_requests(&world).unwrap();
        admit(&gm_store, &world, requests[0].identity.clone(), Vec::new()).unwrap();

        let roll = |id: &str, member: &str| Roll {
            id: id.into(),
            member: member.into(),
            name: "x".into(),
            document: "Goblin".into(),
            label: "Scimitar".into(),
            formula: "1d20 + 4".into(),
            detail: "[17] + 4".into(),
            total: 21.0,
            dice: vec![(20, 17)],
            mark: String::new(),
            at: 0,
            secret: false,
        };
        // The log: kept once; cleared by the GM's clearing roll, not by anyone else's.
        add_to_log(&gm_store, &world, roll("log-1", bob.id().as_str())).unwrap();
        assert_eq!(add_to_log(&gm_store, &world, roll("log-1", bob.id().as_str())).unwrap().rolls.len(), 1);
        let clearing = |member: &str| Roll { mark: CLEAR.into(), ..roll("c", member) };
        assert_eq!(add_to_log(&gm_store, &world, clearing(bob.id().as_str())).unwrap().rolls.len(), 1, "a player cannot clear it");
        assert!(add_to_log(&gm_store, &world, clearing(gm.id().as_str())).unwrap().rolls.is_empty(), "the GM can");
        note_roll(&world, roll("gm-1", gm.id().as_str()));
        let mine = serde_json::to_string(&roll("bob-1", bob.id().as_str())).unwrap();
        let forged = serde_json::to_string(&roll("bob-2", gm.id().as_str())).unwrap();
        let Reply::Digest { rolls, .. } =
            answer(&gm_store, &world, &bob.public(), Ask::Digest { presence: None, rolls: vec![mine.clone(), mine, forged] }).unwrap()
        else {
            panic!("no digest")
        };
        let back: Vec<String> = rolls.iter().map(|r| serde_json::from_str::<Roll>(r).unwrap().id).collect();
        assert_eq!(back, vec!["gm-1".to_string()], "the GM's roll reaches Bob, his own does not come back");
        let kept: Vec<String> = recent_rolls(&world, |_| true).into_iter().map(|r| r.id).collect();
        assert_eq!(kept, vec!["gm-1".to_string(), "bob-1".to_string()], "once each, and none in someone else's name");
    }

    #[test]
    fn what_two_write_at_the_same_time_both_stays() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let open = code.code.replace("<block>\n", "<block view=all edit=players>\nOne.\n\nTwo.\n");
        document::save(&gm_store, &gm, &world, &meta.home, &open, &code.base, "s").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Both open the page, both write, neither sees the other.
        let gm_code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let bob_code = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();
        let gm_text = gm_code.code.replace("One.", "One, said the GM.");
        let bob_text = bob_code.code.replace("Two.", "Two, said Bob.");
        document::save(&gm_store, &gm, &world, &meta.home, &gm_text, &gm_code.base, "s-gm").unwrap();
        document::save(&player_store, &bob, &world, &meta.home, &bob_text, &bob_code.base, "s-bob").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        for (store, who) in [(&gm_store, &gm), (&player_store, &bob)] {
            let now = text(store, who, &world, &meta.home);
            assert!(now.contains("One, said the GM.") && now.contains("Two, said Bob."), "{now}");
        }
    }

    #[test]
    fn a_caret_stays_on_its_word_while_others_write_before_it() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let open = code.code.replace("<block>\n", "<block view=all edit=players>\nOne. Two.\n");
        document::save(&gm_store, &gm, &world, &meta.home, &open, &code.base, "s").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Bob's caret stands before “Two” — in the code as he has it.
        let bobs = document::code_of(&player_store, &bob, &world, &meta.home).unwrap().code;
        let at = bobs.encode_utf16().count() - bobs[bobs.find("Two").unwrap()..].encode_utf16().count();
        let caret = document::Caret { code: Some(bobs), part: None, start: at, end: at };
        let (layer, start, end) = document::caret_in(&player_store, &bob, &world, &meta.home, caret).unwrap().expect("in a part");
        let anchor = player_store.cursor(&world, layer, start).unwrap();
        let head = player_store.cursor(&world, layer, end).unwrap();

        // The GM writes before it.
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let more = code.code.replace("One.", "Once upon a time. One.");
        document::save(&gm_store, &gm, &world, &meta.home, &more, &code.base, "s-gm").unwrap();

        // On the GM's side Bob's caret still stands before “Two”.
        let (a, h) = (gm_store.position(&world, layer, &anchor).unwrap(), gm_store.position(&world, layer, &head).unwrap());
        let seen = document::caret_at(&gm_store, &gm, &world, &meta.home, layer, a, h).unwrap().expect("seen");
        let theirs = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap().code;
        let rest: String = String::from_utf16(&theirs.encode_utf16().collect::<Vec<_>>()[seen.code_start..]).unwrap();
        assert!(rest.starts_with("Two."), "the caret stands before: {rest:?}");
        assert_eq!(seen.code_start, seen.code_end);
    }

    #[test]
    fn what_bob_writes_while_the_gm_changes_access_lands_in_the_text() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let open = code.code.replace("<block>\n", "<block view=all edit=players>\nThe inn.\n");
        document::save(&gm_store, &gm, &world, &meta.home, &open, &code.base, "s").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        let bobs = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();

        // Meanwhile the GM narrows the part to players: a new generation.
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let narrowed = code.code.replace("view=all edit=players", "view=players edit=players");
        document::save(&gm_store, &gm, &world, &meta.home, &narrowed, &code.base, "s-gm").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Bob saves on what he saw — generation one.
        let mine = bobs.code.replace("The inn.", "The inn. It rains.");
        document::save(&player_store, &bob, &world, &meta.home, &mine, &bobs.base, "s-bob").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(text(&gm_store, &gm, &world, &meta.home).contains("The inn. It rains."));
    }

    #[test]
    fn offline_lines_arrive_in_the_text_after_access_changed_meanwhile() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let open = code.code.replace("<block>\n", "<block view=all edit=players>\nThe inn.\n");
        document::save(&gm_store, &gm, &world, &meta.home, &open, &code.base, "s").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Bob writes, apart from the game master.
        let code = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();
        let mine = code.code.replace("The inn.", "The inn. It rains.");
        document::save(&player_store, &bob, &world, &meta.home, &mine, &code.base, "s-bob").unwrap();

        // Later the game master narrows the part and writes too.
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let theirs = code.code.replace("view=all edit=players>\nThe inn.", "view=players edit=players>\nNight. The inn.");
        document::save(&gm_store, &gm, &world, &meta.home, &theirs, &code.base, "s-gm").unwrap();

        // They meet: two rounds, as the network does them.
        swap_here(&gm_store, &player_store, &bob, &world);
        swap_here(&gm_store, &player_store, &bob, &world);
        for (store, who) in [(&gm_store, &gm), (&player_store, &bob)] {
            let now = text(store, who, &world, &meta.home);
            assert!(now.contains("Night. The inn. It rains."), "{now}");
        }
    }

    #[test]
    fn a_document_as_it_stood_with_a_version() {
        let (_dir, gm_store, gm, _player_store, _bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let first = code.code.replace("<block>\n", "<block>\nThe inn is old.\n");
        document::save(&gm_store, &gm, &world, &meta.home, &first, &code.base, "s-1").unwrap();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let second = code.code.replace("<name=Eldermere>", "<name=\"The Mist Lands\">").replace("old", "new");
        document::save(&gm_store, &gm, &world, &meta.home, &second, &code.base, "s-2").unwrap();

        let then = document::document_at(&gm_store, &gm, &world, &meta.home, "s-1").unwrap();
        assert!(then.code.contains("The inn is old.") && then.code.contains("<name=Eldermere>"), "{}", then.code);
        assert_eq!(then.view.name, "Eldermere");
        assert!(then.view.layers.iter().any(|l| l.text == "The inn is old."));
        let now = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap().code;
        assert!(now.contains("The inn is new.") && now.contains("The Mist Lands"), "{now}");
    }

    #[test]
    fn an_id_let_in_ahead_of_time_needs_no_request() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let world = new_world(&gm_store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        let mut meta = gm_store.load_world_meta(&world).unwrap();
        meta.invited.push(bob.id().to_string());
        gm_store.save_world_meta(&meta).unwrap();

        swap_here(&gm_store, &player_store, &bob, &world);
        let meta = gm_store.load_world_meta(&world).unwrap();
        assert!(meta.member(bob.id().as_str()).is_some(), "now a member, with keys");
        assert!(meta.invited.is_empty());
        assert!(player_store.load_world_meta(&world).is_ok(), "the world arrived");

        // Someone else with Bob's id but their own keys is no Bob.
        let eve = Identity::create("Eve").unwrap().public();
        let forged = IdentityPublic { id: bob.public().id, ..eve };
        let (_dir2, other_gm, gm2, _, _) = stores();
        let other = new_world(&other_gm, &gm2, "Other".into()).unwrap().public.world_id.to_string();
        let mut meta = other_gm.load_world_meta(&other).unwrap();
        meta.invited.push(bob.id().to_string());
        other_gm.save_world_meta(&meta).unwrap();
        assert!(answer(&other_gm, &other, &forged, Ask::Digest { presence: None, rolls: Vec::new() }).is_err());
    }

    #[test]
    fn records_of_another_world_are_refused() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let world = new_world(&gm_store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let records = gm_store.all_records(&world).unwrap();
        assert!(player_store.receive("world:0000000000000000", &records).is_err(), "not the world asked for");
        assert!(!player_store.world_dir("world:0000000000000000").exists());
    }
    #[test]
    fn the_gm_hands_the_world_over_and_becomes_a_player() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let open = code.code.replace("<block>\n", "<block view=all>\nThe inn.\n");
        document::save(&gm_store, &gm, &world, &meta.home, &open, &code.base, "s-gm").unwrap();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Nothing happens until Bob shows up.
        gm_store.set_handover(&world, Some(bob.id().as_str())).unwrap();
        let Reply::Digest { handover, .. } = answer(&gm_store, &world, &bob.public(), Ask::Digest { presence: None, rolls: Vec::new() }).unwrap() else {
            panic!("a member gets a digest")
        };
        assert!(handover, "Bob learns the world is his");
        assert!(gm_store.is_gm(&world));

        let Reply::Handover { package } = answer(&gm_store, &world, &bob.public(), Ask::Handover).unwrap() else {
            panic!("the package goes to Bob")
        };
        let package: store::HandoverPackage = serde_json::from_str(&package).unwrap();
        // Sealed for Bob: nobody else opens it.
        let eve = Identity::create("Eve").unwrap();
        assert!(Store::new(_dir.path().join("eve")).handover_take(&package, &eve).is_err());
        player_store.handover_take(&package, &bob).unwrap();
        assert!(player_store.is_gm(&world));

        // The swap brings Marla the record naming Bob — and she steps down.
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(!gm_store.is_gm(&world), "the token left Marla's device");
        let info = world_info(&player_store, &player_store.load_world_meta(&world).unwrap(), bob.id().as_str()).unwrap();
        assert!(info.is_gm);
        let marla = info.members.iter().find(|m| m.id == gm.id().as_str()).unwrap();
        assert_eq!(marla.role, "player");
        assert!(!marla.attributes.contains(&"gm".to_string()));

        // Now Marla is the one who asks — and reads with what Bob grants her.
        swap_here(&player_store, &gm_store, &gm, &world);
        assert!(text(&gm_store, &gm, &world, &meta.home).contains("The inn."));
        let code = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();
        let mine = code.code.replace("The inn.", "The inn, as Bob has it.");
        document::save(&player_store, &bob, &world, &meta.home, &mine, &code.base, "s-bob").unwrap();
        swap_here(&player_store, &gm_store, &gm, &world);
        assert!(text(&gm_store, &gm, &world, &meta.home).contains("as Bob has it"));
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let theirs = code.code.replace("The inn,", "Marla's inn,");
        assert!(document::save(&gm_store, &gm, &world, &meta.home, &theirs, &code.base, "s-marla").is_err(), "the page is the GM's to edit");
    }

    #[test]
    fn a_world_handed_over_as_a_file() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let world = new_world(&gm_store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let package = gm_store.handover_pack(&world, &bob.public()).unwrap();
        gm_store.step_down(&world).unwrap();
        assert!(!gm_store.is_gm(&world));
        // Bob never had the world before — the package brings all of it.
        player_store.handover_take(&package, &bob).unwrap();
        assert!(player_store.is_gm(&world));
        assert_eq!(player_store.load_world_meta(&world).unwrap().public.name, "Eldermere");
    }

    #[test]
    fn who_comes_after_a_removal_reads_what_was_written_before() {
        let (dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let carla = Identity::create("Carla").unwrap();
        admit(&gm_store, &world, carla.public(), Vec::new()).unwrap();
        gm_store.remove_member(&world, carla.id().as_str()).unwrap();
        // Bob comes after the keys turned over: the start page is older.
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(!text(&player_store, &bob, &world, &meta.home).is_empty(), "the page from before the removal opens");
        drop(dir);
    }

    #[test]
    fn a_new_id_takes_a_players_place_with_what_was_theirs() {
        let (dir, gm_store, gm, _old_store, anna) = stores();
        let world = new_world(&gm_store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        admit(&gm_store, &world, anna.public(), vec!["doc:ANNASCHARACTER".into()]).unwrap();
        let secret = crate::NewLayer { title: String::new(), text: "What only Anna knows.".into(), policy: "gm OR doc:ANNASCHARACTER".into(), level: "open".into(), section: "text".into(), edit: "gm".into(), fields: serde_json::json!({}), parent: None };
        let doc = crate::create_document(&gm_store, &world, String::new(), "diary".into(), "Diary".into(), "all".into(), vec![secret]).unwrap();

        // Her device was set up anew: the same Anna, another id — let in ahead of time.
        let new_store = Store::new(dir.path().join("anna-new"));
        let anna_new = Identity::create("Anna").unwrap();
        new_store.save_identity(&anna_new).unwrap();
        crate::replace_member(&gm_store, &world, anna.id().as_str(), anna_new.id().as_str()).unwrap();
        assert!(gm_store.load_world_meta(&world).unwrap().member(anna.id().as_str()).is_some(), "the old id stays until the new one shows up");
        swap_here(&gm_store, &new_store, &anna_new, &world);
        let meta = gm_store.load_world_meta(&world).unwrap();
        assert!(meta.member(anna.id().as_str()).is_none(), "the old id left");
        assert!(meta.member(anna_new.id().as_str()).unwrap().attributes.contains(&"doc:ANNASCHARACTER".to_string()));
        assert!(gm_store.takeovers(&world).unwrap().is_empty());
        swap_here(&gm_store, &new_store, &anna_new, &world);
        assert!(text(&new_store, &anna_new, &world, &doc).contains("What only Anna knows."));

        // Someone let in already takes over at once.
        let bob = Identity::create("Bob").unwrap();
        let bob_new = Identity::create("Bob").unwrap();
        admit(&gm_store, &world, bob.public(), vec!["class:rogue".into()]).unwrap();
        admit(&gm_store, &world, bob_new.public(), Vec::new()).unwrap();
        crate::replace_member(&gm_store, &world, bob.id().as_str(), bob_new.id().as_str()).unwrap();
        let meta = gm_store.load_world_meta(&world).unwrap();
        assert!(meta.member(bob.id().as_str()).is_none());
        assert!(meta.member(bob_new.id().as_str()).unwrap().attributes.contains(&"class:rogue".to_string()));
        assert!(crate::replace_member(&gm_store, &world, gm.id().as_str(), bob.id().as_str()).is_err(), "the GM is no player");
        // Thought better of: the new id taken out again, nobody takes anyone's place.
        let carla = Identity::create("Carla").unwrap();
        crate::replace_member(&gm_store, &world, bob_new.id().as_str(), carla.id().as_str()).unwrap();
        gm_store.remove_member(&world, carla.id().as_str()).unwrap();
        assert!(gm_store.takeovers(&world).unwrap().is_empty());
    }

    #[test]
    fn a_removed_player_gets_nothing_more() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let world = new_world(&gm_store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        let before = gm_store.load_keyring(&world).unwrap().current_epoch(&aw_model::Attribute::Players);
        gm_store.remove_member(&world, bob.id().as_str()).unwrap();
        assert!(gm_store.load_world_meta(&world).unwrap().member(bob.id().as_str()).is_none());
        assert_ne!(gm_store.load_keyring(&world).unwrap().current_epoch(&aw_model::Attribute::Players), before, "his keys turned over");
        assert_eq!(swap_here(&gm_store, &player_store, &bob, &world), Reply::NotMember);
        assert!(gm_store.remove_member(&world, gm.id().as_str()).is_err(), "the GM stays");
    }
    #[test]
    fn a_version_is_marked_by_the_characters_it_changed() {
        let (_dir, gm_store, gm, _player_store, _bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let write = |from: &str, to: &str, session: &str| {
            let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
            let next = code.code.replacen(from, to, 1);
            document::save(&gm_store, &gm, &world, &meta.home, &next, &code.base, session).unwrap();
        };
        write("<block>\n", "<block>\naaaa and more\n", "s-1");
        write("aaaa", "aaabbb", "s-2");
        let marks = document::marks_of(&gm_store, &gm, &world, &meta.home, "s-1").unwrap();
        let cut = |text: &str, ranges: &[aw_store::journal::diff::Range]| -> Vec<String> {
            let u: Vec<u16> = text.encode_utf16().collect();
            ranges.iter().map(|r| String::from_utf16(&u[r.start..r.end]).unwrap()).collect()
        };
        let then = document::document_at(&gm_store, &gm, &world, &meta.home, "s-1").unwrap().code;
        assert_eq!(cut(&then, &marks.then.code), ["a"], "{then}");
        assert_eq!(cut(&marks.now_code, &marks.now.code), ["bbb"], "{}", marks.now_code);
        assert_eq!(marks.now.parts.values().flatten().count(), 1);
    }

    #[test]
    fn under_review_bob_writes_and_the_gm_takes_it_in() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let open = code.code.replace("<block>", "<block view=all edit=players>\nThe inn.");
        document::save(&gm_store, &gm, &world, &meta.home, &open, &code.base, "s-gm").unwrap();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        gm_store.set_moderation(&world, true).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        let code = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();
        let mine = code.code.replace("The inn.", "The inn. It rains.");
        document::save(&player_store, &bob, &world, &meta.home, &mine, &code.base, "s-bob").unwrap();
        assert!(text(&player_store, &bob, &world, &meta.home).contains("It rains."), "Bob sees his own");
        // Leaving edit mode: the session becomes a version.
        player_store.close_session(&world, "s-bob").unwrap();
        assert_eq!(player_store.waiting(&world).unwrap().len(), 1, "and that it waits");
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(!text(&gm_store, &gm, &world, &meta.home).contains("It rains."), "not in the text yet");
        assert_eq!(gm_store.waiting(&world).unwrap().len(), 1);
        // Looked at, it stands as it would be taken in.
        let marks = document::marks_of(&gm_store, &gm, &world, &meta.home, "s-bob").unwrap();
        assert_eq!(marks.review, aw_store::journal::Review::Waiting);
        assert!(!marks.then.code.is_empty());

        let id = gm_store.load_articles(&world).unwrap().iter().find(|a| a.id.to_string() == meta.home).unwrap().id;
        gm_store.verdict(&world, id, "s-bob", true).unwrap();
        assert!(text(&gm_store, &gm, &world, &meta.home).contains("It rains."));
        assert_eq!(gm_store.waiting(&world).unwrap().len(), 0);
        swap_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(player_store.waiting(&world).unwrap().len(), 0, "Bob learns it was taken in");
    }

    /// The version files of a document on a device.
    fn version_files(store: &Store, world: &str, article: &str) -> usize {
        let folder = store.world_dir(world).join("articles").join(aw_store::journal::dir_name(article));
        std::fs::read_dir(folder).map(|e| e.count()).unwrap_or(0)
    }

    #[test]
    fn a_player_asks_and_the_gms_device_makes_the_document() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        // Session logs the players may make; plot notes not.
        let kind = |id: &str, create: &str| -> aw_model::Template {
            serde_json::from_value(serde_json::json!({
                "id": id, "name": id, "create": create,
                "auth": { "view": "players", "edit": "players", "level": "open" },
                "infobox": { "align": "none", "width": "auto", "show": [] },
                "blocks": [{ "key": "notes", "title": "Notes", "parts": [{ "key": "notes", "view": "players", "edit": "players", "level": "open" }] }]
            }))
            .unwrap()
        };
        gm_store.save_templates(&world, &[kind("log", "players"), kind("plot", "")]).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Bob asks — for his log wanting it the GM's alone, with a part the type has not.
        let layer = |section: &str, text: &str| crate::NewLayer { title: String::new(), text: text.into(), policy: "gm".into(), level: "gm-only".into(), section: section.into(), edit: "gm".into(), fields: serde_json::json!({}), parent: None };
        let ask = |kind: &str, name: &str, layers: Vec<crate::NewLayer>| serde_json::to_vec(&crate::Asked { article: String::new(), article_type: kind.into(), slug: name.to_lowercase(), name: name.into(), layers }).unwrap();
        player_store.propose(&world, &bob, &ask("log", "Session 4", vec![layer("notes", "We met a goblin."), layer("nowhere", "Smuggled in.")])).unwrap();
        player_store.propose(&world, &bob, &ask("plot", "Plot", vec![])).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // The GM's device makes the one he may — with the type's access.
        assert_eq!(crate::answer_proposals(&gm_store, &world).unwrap(), 1);
        swap_here(&gm_store, &player_store, &bob, &world);
        let held = player_store.view_keys(&world, &bob).unwrap();
        let view = crate::build_view(&player_store.load_articles(&world).unwrap(), &held);
        let made = view.iter().find(|a| a.name == "Session 4").expect("made, and Bob reads it");
        assert!(made.layers.iter().any(|l| l.text == "We met a goblin." && l.policy == "players"), "{:?}", made.layers.iter().map(|l| (&l.text, &l.policy)).collect::<Vec<_>>());
        assert!(made.layers.iter().all(|l| !l.text.contains("Smuggled")), "a part the type does not know is left out");
        assert!(!view.iter().any(|a| a.name == "Plot"), "not his to make");

        // Answered: gone at the GM's and, with the GM's word, at Bob's.
        assert_eq!(crate::answer_proposals(&gm_store, &world).unwrap(), 0);
        assert!(gm_store.proposals(&world, &gm).unwrap().is_empty());
        assert!(player_store.proposals(&world, &bob).unwrap().is_empty());
    }

    #[test]
    fn what_a_player_asked_for_is_theirs_at_once_and_everyones_once_made() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let kind = |id: &str| -> aw_model::Template {
            serde_json::from_value(serde_json::json!({
                "id": id, "name": id, "create": "players",
                "auth": { "view": "players", "edit": "players", "level": "open" },
                "infobox": { "align": "none", "width": "auto", "show": [] },
                "blocks": [{ "key": "text", "title": "", "parts": [{ "key": "text", "view": "players", "edit": "players", "level": "open" }] }]
            }))
            .unwrap()
        };
        gm_store.save_templates(&world, &[kind("place"), kind("character")]).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        let text = |t: &str| vec![crate::NewLayer { title: String::new(), text: t.into(), policy: "players".into(), level: "open".into(), section: "text".into(), edit: "players".into(), fields: serde_json::json!({}), parent: None }];
        let propose = |id: Option<String>, kind: &str, name: &str, body: &str| crate::propose_document(&player_store, &bob, &world, id, kind.into(), name.to_lowercase(), name.into(), text(body)).unwrap();

        // His home, and his character living there — linked by name, as he writes.
        let home = propose(None, "place", "Kaltwasser", "A harbour.");
        let ilva = propose(None, "character", "Ilva", "Lives in @@place:Kaltwasser@@.");
        let views = |store: &Store, who: &Identity| {
            let held = store.view_keys(&world, who).unwrap();
            let mut v = crate::build_view(&store.load_articles(&world).unwrap(), &held);
            v.extend(crate::pending_views(store, &world, who).unwrap());
            v
        };
        let his = views(&player_store, &bob);
        let mine = his.iter().find(|a| a.id == home).expect("his at once");
        assert!(mine.pending && mine.name == "Kaltwasser");
        let character = his.iter().find(|a| a.id == ilva).unwrap();
        assert!(character.layers[0].text.contains(&home), "the link names the waiting home by its id: {}", character.layers[0].text);
        assert!(views(&gm_store, &gm).iter().all(|a| a.id != home), "nobody else has it yet");

        // Asked again, changed: the newest counts.
        propose(Some(home.clone()), "place", "Kaltwasser", "A harbour in the fog.");
        let his = views(&player_store, &bob);
        assert_eq!(his.iter().filter(|a| a.id == home).count(), 1);
        assert_eq!(his.iter().find(|a| a.id == home).unwrap().layers[0].text, "A harbour in the fog.");

        // The GM's device makes both, under their ids: the link holds for everyone.
        swap_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(crate::answer_proposals(&gm_store, &world).unwrap(), 2);
        let all = views(&gm_store, &gm);
        assert_eq!(all.iter().find(|a| a.id == home).map(|a| a.layers.iter().any(|l| l.text == "A harbour in the fog.")), Some(true));
        assert!(all.iter().find(|a| a.id == ilva).unwrap().layers.iter().any(|l| l.text.contains(&home)));
        swap_here(&gm_store, &player_store, &bob, &world);
        let his = views(&player_store, &bob);
        assert!(his.iter().all(|a| !a.pending), "made: nothing waits any more");
        assert_eq!(his.iter().filter(|a| a.id == home || a.id == ilva).count(), 2);
    }

    #[test]
    fn the_worlds_dice_reach_its_players() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        assert!(gm_store.dice(&world).unwrap().is_empty(), "the usual set until the GM chooses");
        gm_store.set_dice(&world, vec![6]).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(player_store.dice(&world).unwrap(), vec![6]);
    }

    #[test]
    fn the_oldest_versions_go_and_the_document_starts_afresh() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        gm_store.set_version_limit(&world, 2).unwrap();
        let mut line = String::from("The inn.");
        for i in 1..=4 {
            let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
            let next = format!("{line} {i}.");
            let text = if i == 1 { code.code.replace("<block>", &format!("<block view=all edit=players>\n{next}")) } else { code.code.replace(&line, &next) };
            line = next;
            document::save(&gm_store, &gm, &world, &meta.home, &text, &code.base, &format!("s-{i}")).unwrap();
            gm_store.close_session(&world, &format!("s-{i}")).unwrap();
        }
        // Four versions are more than half as many again as two: back to two.
        assert_eq!(version_files(&gm_store, &world, &meta.home), 2);
        assert!(text(&gm_store, &gm, &world, &meta.home).contains("The inn. 1. 2. 3. 4."));
        let id = gm_store.load_articles(&world).unwrap().iter().find(|a| a.id.to_string() == meta.home).unwrap().id;
        let layer = gm_store.load_articles(&world).unwrap().iter().find(|a| a.id == id).unwrap().layers[0].id;
        assert_eq!(gm_store.layer_state(&world, layer).unwrap().unwrap().0, 2, "a fresh start: the next generation");

        // Bob gets the world as it is now.
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(text(&player_store, &bob, &world, &meta.home).contains("The inn. 1. 2. 3. 4."));
        assert_eq!(version_files(&player_store, &world, &meta.home), 2);

        // The GM deletes one more by hand; the newest stays.
        let sessions = gm_store.sessions(&world, id).unwrap();
        let newest = sessions.iter().find(|s| !s.removable && !s.structure).unwrap().session.clone();
        assert!(gm_store.delete_version(&world, id, &newest).is_err());
        let old = sessions.iter().find(|s| s.removable).unwrap().session.clone();
        gm_store.delete_version(&world, id, &old).unwrap();
        assert_eq!(version_files(&gm_store, &world, &meta.home), 1);
        swap_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(version_files(&player_store, &world, &meta.home), 1, "gone on Bob's device too");
        assert!(text(&player_store, &bob, &world, &meta.home).contains("The inn. 1. 2. 3. 4."));
    }

    /// What `swap_parts` does over the network, without it.
    fn parts_here(gm_store: &Store, player_store: &Store, bob: &Identity, world: &str) {
        let mut give = Vec::new();
        for _ in 0..16 {
            let want = player_store.missing_parts(world).unwrap();
            let ask = Ask::Parts { want, give: encode_parts(player_store.parts(world, &give)) };
            let Reply::Parts { parts, want: lacking } = answer(gm_store, world, &bob.public(), ask).unwrap() else { panic!("no parts") };
            player_store.take_parts(world, &decode_parts(parts)).unwrap();
            if lacking.is_empty() && player_store.missing_parts(world).unwrap().is_empty() {
                return;
            }
            give = lacking;
        }
    }

    fn png_with_author(red: u8) -> Vec<u8> {
        let img = image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([red, (x * 6) as u8, (y * 8) as u8]));
        let mut raw = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png).unwrap();
        // A text chunk with a name in it, right after the header.
        let mut chunk = Vec::new();
        let data = b"Author\0Marla Musterfrau";
        chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&[0, 0, 0, 0]);
        let mut out = raw[..33].to_vec();
        out.extend(chunk);
        out.extend_from_slice(&raw[33..]);
        out
    }

    #[test]
    fn a_members_device_carries_the_others_but_lets_no_stranger_in() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let carla = Identity::create("Carla").unwrap();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        admit(&gm_store, &world, carla.public(), Vec::new()).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Carla asks Bob's device — the game master's cannot be reached.
        let Reply::Records { records, .. } = answer(&player_store, &world, &carla.public(), Ask::Swap { have: Vec::new(), give: Vec::new() }).unwrap() else { panic!("no records") };
        assert!(!records.is_empty(), "Bob's device passes the world on");
        let digest = answer(&player_store, &world, &carla.public(), Ask::Digest { presence: None, rolls: Vec::new() }).unwrap();
        assert!(matches!(digest, Reply::Digest { handover: false, .. }));

        // A stranger gets nothing — and asking to join is the game master's to hear.
        let eve = Identity::create("Eve").unwrap();
        assert_eq!(answer(&player_store, &world, &eve.public(), Ask::Swap { have: Vec::new(), give: Vec::new() }).unwrap(), Reply::NotMember);
        assert_eq!(answer(&player_store, &world, &eve.public(), Ask::Join).unwrap(), Reply::NotMember);
        assert!(player_store.load_requests(&world).unwrap_or_default().is_empty());

        // The game master takes Carla out: once Bob's device has heard, it passes nothing on to her.
        gm_store.remove_member(&world, carla.id().as_str()).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(answer(&player_store, &world, &carla.public(), Ask::Swap { have: Vec::new(), give: Vec::new() }).unwrap(), Reply::NotMember);
        assert_eq!(answer(&gm_store, &world, &carla.public(), Ask::Swap { have: Vec::new(), give: Vec::new() }).unwrap(), Reply::NotMember);
        // Let in again: carried again.
        admit(&gm_store, &world, carla.public(), Vec::new()).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(matches!(answer(&player_store, &world, &carla.public(), Ask::Swap { have: Vec::new(), give: Vec::new() }).unwrap(), Reply::Records { .. }));
    }

    #[test]
    fn a_player_puts_their_own_image_into_what_they_ask_for() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let place: aw_model::Template = serde_json::from_value(serde_json::json!({
            "id": "place", "name": "Place", "create": "players",
            "auth": { "view": "players", "edit": "players", "level": "open" },
            "infobox": { "align": "none", "width": "auto", "show": [] },
            "blocks": [{ "key": "text", "title": "", "parts": [{ "key": "text", "view": "players", "edit": "players", "level": "open" }] }]
        }))
        .unwrap();
        gm_store.save_templates(&world, &[place]).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);

        // Bob uploads a picture of his home and puts it into the home he asks for.
        let image = player_store.upload_image(&world, &png_with_author(200), "Kaltwasser", "", &bob).unwrap();
        let layer = crate::NewLayer { title: String::new(), text: format!("A harbour.\n<image src={image} align=right>"), policy: "players".into(), level: "open".into(), section: "text".into(), edit: "players".into(), fields: serde_json::json!({}), parent: None };
        let home = crate::propose_document(&player_store, &bob, &world, None, "place".into(), "kaltwasser".into(), "Kaltwasser".into(), vec![layer]).unwrap();
        assert!(player_store.image_bytes(&world, &image, false).is_ok(), "his at once, the picture too");

        // The GM's device has the picture and the proposal: it makes the home, the picture's key in its part.
        swap_here(&gm_store, &player_store, &bob, &world);
        parts_here(&gm_store, &player_store, &bob, &world);
        assert_eq!(crate::answer_proposals(&gm_store, &world).unwrap(), 1);
        let used = gm_store.gallery(&world, &gm).unwrap().into_iter().find(|i| i.id == image).unwrap().used_in;
        assert_eq!(used.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), vec![home.clone()], "the home uses it");
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(player_store.image_bytes(&world, &image, false).is_ok());
    }

    #[test]
    fn images_follow_the_rights_of_the_part_they_stand_in() {
        let (dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let file = dir.path().join("coast.png");
        std::fs::write(&file, png_with_author(200)).unwrap();
        let coast = gm_store.upload_image(&world, &std::fs::read(&file).unwrap(), "Map of the raven coast", "", &gm).unwrap();
        let traitor = gm_store.upload_image(&world, &png_with_author(10), "Aldric unmasked", "", &gm).unwrap();
        let spare = gm_store.upload_image(&world, &png_with_author(99), "Unused sketch", "", &gm).unwrap();
        let (bytes, mime) = gm_store.image_bytes(&world, &coast, false).unwrap();
        assert_eq!(mime, "image/png");
        assert!(!String::from_utf8_lossy(&bytes).contains("Marla"), "the metadata is gone");

        // The coast in a part for all, the traitor in one for the GM alone.
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let text = code.code.replace(
            "<block>",
            &format!("<block view=all edit=players>\nThe coast.\n<image src={coast} caption=\"The coast\">\n</block>\n<block view=gm edit=gm>\n<image src={traitor} caption=\"A man in the shadows\">"),
        );
        document::save(&gm_store, &gm, &world, &meta.home, &text, &code.base, "s-gm").unwrap();
        gm_store.close_session(&world, "s-gm").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        parts_here(&gm_store, &player_store, &bob, &world);

        assert_eq!(player_store.image_bytes(&world, &coast, false).unwrap().0, bytes, "Bob sees the coast");
        assert!(player_store.image_bytes(&world, &traitor, false).is_err(), "but not the traitor");
        let seen = player_store.gallery(&world, &bob).unwrap();
        assert_eq!(seen.iter().map(|i| i.id.clone()).collect::<Vec<_>>(), vec![coast.clone()], "only what he may see");
        assert!(seen[0].name.is_empty(), "and not its name");
        let all = player_store.missing_parts(&world).unwrap();
        assert!(all.is_empty(), "every device has every part");

        // Bob uploads his own and puts it where he may write.
        let mine = player_store.upload_image(&world, &png_with_author(50), "My sketch", "", &bob).unwrap();
        let code = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();
        let text = code.code.replace("The coast.", &format!("The coast.\n<image src={mine}>"));
        document::save(&player_store, &bob, &world, &meta.home, &text, &code.base, "s-bob").unwrap();
        player_store.close_session(&world, "s-bob").unwrap();
        let seen = player_store.gallery(&world, &bob).unwrap();
        assert!(seen.iter().any(|i| i.id == mine && i.name == "My sketch" && i.own));
        // An image he cannot open he cannot use.
        let code = document::code_of(&player_store, &bob, &world, &meta.home).unwrap();
        let sneaky = code.code.replace("The coast.", &format!("The coast.\n<image src={traitor}>"));
        assert!(document::save(&player_store, &bob, &world, &meta.home, &sneaky, &code.base, "s-bob-2").is_err());
        swap_here(&gm_store, &player_store, &bob, &world);
        parts_here(&gm_store, &player_store, &bob, &world);

        // The GM's gallery: every image, named, with where it is used.
        let gallery = gm_store.gallery(&world, &gm).unwrap();
        let find = |id: &str| gallery.iter().find(|i| i.id == id).unwrap().clone();
        assert_eq!(find(&coast).name, "Map of the raven coast");
        assert_eq!(find(&traitor).used_in.len(), 1);
        assert_eq!(find(&mine).name, "My sketch", "the GM reads Bob's names too");
        assert!(find(&mine).complete, "and has its parts");
        // Folders in folders — the GM's, sealed for them.
        use aw_store::media::FolderNode;
        let folder = |id: &str, name: &str, parent: &str| FolderNode { id: id.into(), name: name.into(), parent: parent.into() };
        gm_store.set_gallery_folders(&world, vec![folder("f1", "Places", ""), folder("f2", "Coast", "f1")]).unwrap();
        assert!(gm_store.set_gallery_folders(&world, vec![folder("f1", "Places", "f2"), folder("f2", "Coast", "f1")]).is_err(), "no folder inside itself");
        gm_store.move_image(&world, &coast, "f2", &gm).unwrap();
        assert_eq!(gm_store.gallery(&world, &gm).unwrap().iter().find(|i| i.id == coast).unwrap().folder, "f2");
        assert_eq!(gm_store.gallery_folders(&world).unwrap().len(), 2);
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(player_store.gallery_folders(&world).unwrap().is_empty(), "Bob does not learn how the GM sorts");
        assert_eq!(find(&traitor).used_in[0].id, meta.home, "where it is used, to open");
        assert!(find(&spare).used_in.is_empty() && !find(&spare).in_versions, "unused");
        gm_store.rename_image(&world, &mine, "Bob's sketch of the inn", &gm).unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(player_store.gallery(&world, &bob).unwrap().iter().any(|i| i.id == mine && i.name == "Bob's sketch of the inn"));
        assert!(player_store.rename_image(&world, &coast, "Mine now", &bob).is_err(), "only uploader and GM rename");

        // Taken out of the text: only old versions name it.
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let text = code.code.replace(&format!("<image src={coast} caption=\"The coast\">\n"), "");
        document::save(&gm_store, &gm, &world, &meta.home, &text, &code.base, "s-gm-2").unwrap();
        gm_store.close_session(&world, "s-gm-2").unwrap();
        let gallery = gm_store.gallery(&world, &gm).unwrap();
        let coast_now = gallery.iter().find(|i| i.id == coast).unwrap();
        assert!(coast_now.used_in.is_empty() && coast_now.in_versions, "{coast_now:?}");

        // Deleted by hand: gone everywhere, parts and all.
        gm_store.delete_images(&world, &[coast.clone(), spare.clone()]).unwrap();
        assert!(gm_store.image_bytes(&world, &spare, false).is_err());
        assert!(!gm_store.gallery(&world, &gm).unwrap().iter().any(|i| i.id == coast || i.id == spare));
        swap_here(&gm_store, &player_store, &bob, &world);
        let parts = aw_store::media::parts_here(&player_store.world_dir(&world)).len();
        assert!(player_store.gallery(&world, &bob).unwrap().iter().all(|i| i.id != coast));
        assert!(player_store.image_bytes(&world, &coast, false).is_err());
        assert_eq!(parts, aw_store::media::parts_here(&gm_store.world_dir(&world)).len(), "the same parts on both");
    }

    #[test]
    fn a_hidden_image_shows_its_stand_in_until_it_is_revealed() {
        let (_dir, gm_store, gm, player_store, bob) = stores();
        let meta = new_world(&gm_store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        admit(&gm_store, &world, bob.public(), Vec::new()).unwrap();
        let aldric = gm_store.upload_image(&world, &png_with_author(10), "Aldric unmasked", "", &gm).unwrap();
        let silhouette = gm_store.upload_image(&world, &png_with_author(200), "Person, unknown", "", &gm).unwrap();
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let text = code.code.replace(
            "<block>",
            &format!("<block view=players edit=gm>\nA man of few words.\n<image src={aldric} caption=\"Aldric, the traitor\" view=gm edit=gm hidden={silhouette} hidden-caption=\"A stranger\">"),
        );
        document::save(&gm_store, &gm, &world, &meta.home, &text, &code.base, "s-gm").unwrap();
        gm_store.close_session(&world, "s-gm").unwrap();
        let gm_code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap().code;
        assert!(gm_code.contains(&format!("hidden={silhouette} hidden-caption=\"A stranger\"")), "one tag for the GM: {gm_code}");
        swap_here(&gm_store, &player_store, &bob, &world);
        parts_here(&gm_store, &player_store, &bob, &world);

        let theirs = document::code_of(&player_store, &bob, &world, &meta.home).unwrap().code;
        assert!(theirs.contains(&format!("<image src={silhouette} caption=\"A stranger\" for=")), "{theirs}");
        assert!(!theirs.contains(&aldric) && !theirs.contains("traitor"), "nothing of the real one: {theirs}");
        assert!(player_store.image_bytes(&world, &silhouette, false).is_ok(), "the stand-in he sees");
        assert!(player_store.image_bytes(&world, &aldric, false).is_err(), "the real one not");

        // Revealed: the GM lets the players see it.
        let code = document::code_of(&gm_store, &gm, &world, &meta.home).unwrap();
        let text = code.code.replace("view=gm edit=gm>", "view=players edit=gm>");
        assert_ne!(text, code.code);
        document::save(&gm_store, &gm, &world, &meta.home, &text, &code.base, "s-reveal").unwrap();
        gm_store.close_session(&world, "s-reveal").unwrap();
        swap_here(&gm_store, &player_store, &bob, &world);
        assert!(player_store.image_bytes(&world, &aldric, false).is_ok(), "now he sees Aldric");
        let theirs = document::code_of(&player_store, &bob, &world, &meta.home).unwrap().code;
        assert!(theirs.contains(&aldric) && !theirs.contains("for="), "one tag for Bob too: {theirs}");
    }
}
