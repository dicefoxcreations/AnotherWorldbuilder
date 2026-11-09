//! Worlds without a server: mailboxes in a folder a cloud app keeps in step.
//!
//! The world lies with its GM. Beside it, in a folder of the GM's choice,
//! each player has a mailbox the GM shares with them alone — `ein/` holds
//! the world as they see it, `aus/` what they changed:
//!
//! ```text
//! <folder>/alle/media/        pictures every player sees
//! <folder>/<name>/ein/        world.sqlite as <name> sees it, media, done.json
//! <folder>/<name>/aus/        <name>'s changes, one signed file each
//! ```
//!
//! A change is a command of the app, as a member on a server asks for it
//! (`remote::call`); the GM's app does it in the player's name, under the
//! same rules, then delivers each player their view anew. A player works
//! offline on their own copy, and sees what they wrote at once.
//!
//! A place is made for a player by name (`slot:<name>`) and taken by the
//! first identity whose change arrives in it — the cloud app's sharing is
//! who may write there.

use super::*;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};
use store::{Mailbox, Member, WorldDb};

/// Pictures every player sees.
const ALL: &str = "alle";
/// What a player gets.
const IN: &str = "ein";
/// What a player sends.
const OUT: &str = "aus";
/// What the GM's app took of a player's changes, and what it refused.
const DONE: &str = "done.json";
/// What a mailbox's place is called before a player took it.
const SLOT: &str = "slot:";
/// Where the world went, when it went onto a server.
const MOVED: &str = "moved.json";
/// The largest change file taken.
const LARGEST: u64 = 64 * 1024 * 1024;

/// A player's change: a command, in their name.
#[derive(Serialize, Deserialize)]
struct Change {
    world: String,
    card: IdentityPublic,
    at: u64,
    command: String,
    args: serde_json::Value,
    /// The parts' states a save was written against, by layer.
    #[serde(default)]
    states: BTreeMap<String, document::Plain>,
}

/// A change as a file: its body, and the player's signature over it.
#[derive(Serialize, Deserialize)]
struct Signed {
    body: String,
    signature: String,
}

/// What the GM's app did with a player's change files, by file name.
#[derive(Serialize, Deserialize, Default)]
struct Done {
    taken: BTreeSet<String>,
    /// Why not.
    refused: BTreeMap<String, String>,
}

/// A mailbox's folder name for a player's name: letters, digits, `-`, `_`.
fn slot_name(name: &str) -> String {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    s.trim_matches('-').to_string()
}

fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Answer<()> {
    aw_store::durable::write(path, &serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

/// Copies a file where it is missing or differs in size.
fn copy_missing(from: &Path, to: &Path) -> Answer<()> {
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    if size(from).is_some() && size(from) != size(to) {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = to.with_extension("part");
        std::fs::copy(from, &part)?;
        std::fs::rename(&part, to)?;
    }
    Ok(())
}

// --- The GM's side -------------------------------------------------------------

/// The folder of a world's mailboxes — the GM's.
fn root(store: &Store, world: &str) -> Answer<PathBuf> {
    store.load_network()?.posts.get(world).map(PathBuf::from).ok_or_else(|| AppError("This world has no mailboxes.".into()))
}

/// Makes a world one without a server: its mailboxes in `folder`, one for
/// every player it has.
pub(crate) fn setup(store: &Store, world: &str, folder: &Path) -> Answer<()> {
    must_be_gm(store, world, "makes mailboxes")?;
    std::fs::create_dir_all(folder.join(ALL).join(aw_store::db::MEDIA))?;
    let mut settings = store.load_network()?;
    settings.posts.insert(world.to_string(), folder.display().to_string());
    settings.shared.retain(|w| w != world);
    store.save_network(&settings)?;
    let db = store.db(world)?;
    for mut member in db.members()?.into_iter().filter(|m| m.role != Role::Gm) {
        if member.slot.is_empty() {
            member.slot = free_slot(&db.members()?, &member.name);
            db.put_member(&member)?;
        }
        make_box(folder, &member.slot)?;
    }
    Ok(())
}

fn free_slot(members: &[Member], name: &str) -> String {
    let base = match slot_name(name) {
        s if s.is_empty() => "player".to_string(),
        s => s,
    };
    let taken = |s: &str| s == ALL || members.iter().any(|m| m.slot == s);
    let mut slot = base.clone();
    let mut n = 2;
    while taken(&slot) {
        slot = format!("{base}-{n}");
        n += 1;
    }
    slot
}

fn make_box(folder: &Path, slot: &str) -> Answer<()> {
    std::fs::create_dir_all(folder.join(slot).join(IN))?;
    std::fs::create_dir_all(folder.join(slot).join(OUT))?;
    Ok(())
}

/// Places for players, by name: a mailbox each, to share with them.
pub(crate) fn add(store: &Store, world: &str, names: &[String]) -> Answer<WorldInfo> {
    must_be_gm(store, world, "makes places for players")?;
    let folder = root(store, world)?;
    let db = store.db(world)?;
    for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        let slot = free_slot(&db.members()?, name);
        make_box(&folder, &slot)?;
        db.put_member(&Member { id: format!("{SLOT}{slot}"), name: name.to_string(), role: Role::Player, slot, joined: store::now(), ..Default::default() })?;
    }
    world_now(store, world)
}

/// Frees a place again: the next identity whose change arrives takes it.
pub(crate) fn reset(store: &Store, world: &str, member: &str) -> Answer<WorldInfo> {
    must_be_gm(store, world, "frees a place")?;
    let db = store.db(world)?;
    let mut m = db.member(member)?.filter(|m| !m.slot.is_empty()).ok_or_else(|| AppError("There is no such place.".into()))?;
    db.remove_member(member)?;
    m.id = format!("{SLOT}{}", m.slot);
    m.verify_key = String::new();
    db.put_member(&m)?;
    world_now(store, world)
}

/// What the GM's app delivered last, by world and mailbox: how far the
/// world's writes were.
static DELIVERED: OnceLock<Mutex<HashMap<(String, String), u64>>> = OnceLock::new();

fn delivered() -> std::sync::MutexGuard<'static, HashMap<(String, String), u64>> {
    DELIVERED.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// The world went onto a server: each mailbox says so, its players follow,
/// and the mailboxes are served no more.
pub(crate) fn moved(store: &Store, world: &str, slots: &[String], server: &str) -> Answer<()> {
    let folder = root(store, world)?;
    for slot in slots {
        write_json(&folder.join(slot).join(IN).join(MOVED), &serde_json::json!({ "server": server }))?;
    }
    let mut settings = store.load_network()?;
    settings.posts.remove(world);
    Ok(store.save_network(&settings)?)
}

/// One round of the GM's app: takes in what the players sent, then gives
/// each of them the world as they see it, where it changed. Says whether
/// anything came in.
pub(crate) fn round(store: &Store, world: &str) -> Answer<bool> {
    let folder = root(store, world)?;
    let db = store.db(world)?;
    let mut came = false;
    for member in db.members()?.into_iter().filter(|m| !m.slot.is_empty() && m.role != Role::Gm) {
        came |= collect(store, world, &folder, &member)?;
    }
    // What every player sees: asked once for all of them, when one is due.
    let mut everyone = None;
    for member in db.members()?.into_iter().filter(|m| !m.slot.is_empty() && m.role != Role::Gm) {
        let key = (world.to_string(), member.slot.clone());
        if delivered().get(&key) == Some(&db.writes()) {
            continue;
        }
        let writes = db.writes();
        if everyone.is_none() {
            everyone = Some(everyone_sees(store, world)?);
        }
        deliver(store, world, &folder, &member, everyone.as_ref().unwrap_or(&BTreeSet::new()))?;
        delivered().insert(key, writes);
    }
    Ok(came)
}

/// Takes in a player's changes — the first one's identity takes the place.
fn collect(store: &Store, world: &str, folder: &Path, member: &Member) -> Answer<bool> {
    let out = folder.join(&member.slot).join(OUT);
    let done_path = folder.join(&member.slot).join(IN).join(DONE);
    let mut done: Done = read_json(&done_path);
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json") && !done.taken.contains(n) && !done.refused.contains_key(n))
        .collect();
    if names.is_empty() {
        return Ok(false);
    }
    names.sort();
    let mut id = member.id.clone();
    for name in names {
        let path = out.join(&name);
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > LARGEST {
            done.refused.insert(name, "Too large.".into());
            continue;
        }
        // A cloud app may still be writing it: once whole, it is read.
        let Ok(signed) = serde_json::from_slice::<Signed>(&std::fs::read(&path)?) else { continue };
        match take(store, world, &mut id, member, &signed) {
            Ok(()) => {
                done.taken.insert(name);
            }
            Err(e) => {
                crate::applog::line("mailbox", &format!("{world}: {}: {name} refused: {}", member.slot, e.0));
                done.refused.insert(name, e.0);
            }
        }
    }
    // What the player let go of no longer needs naming.
    let there: BTreeSet<String> = std::fs::read_dir(&out).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    done.taken.retain(|n| there.contains(n));
    done.refused.retain(|n, _| there.contains(n));
    write_json(&done_path, &done)?;
    Ok(true)
}

/// One change: checked, and done in the player's name.
fn take(store: &Store, world: &str, id: &mut String, member: &Member, signed: &Signed) -> Answer<()> {
    let change: Change = serde_json::from_str(&signed.body)?;
    let signature: [u8; 64] = hex::decode(&signed.signature).ok().and_then(|s| s.try_into().ok()).ok_or_else(|| AppError("Not signed.".into()))?;
    if change.world != world || !change.card.is_genuine() || change.card.verify(signed.body.as_bytes(), &signature).is_err() {
        return Err(AppError("Not signed by its player.".into()));
    }
    let card = change.card.id.to_string();
    if id.starts_with(SLOT) {
        // The place is taken by whoever comes first.
        let db = store.db(world)?;
        db.remove_member(id)?;
        db.put_member(&Member { id: card.clone(), verify_key: change.card.verify_key.clone(), ..member.clone() })?;
        *id = card.clone();
    }
    if *id != card {
        return Err(AppError("This mailbox is someone else's.".into()));
    }
    if change.command == "hello" {
        return Ok(());
    }
    if let Ok(base) = serde_json::from_value::<document::Bases>(change.args.get("base").cloned().unwrap_or_default()) {
        document::take_states(&base, change.states);
    }
    store::acting_for(id, || remote::call(store, world, &change.command, &change.args)).map(|_| ())
}

/// Gives a player the world as they see it.
fn deliver(store: &Store, world: &str, folder: &Path, member: &Member, everyone: &BTreeSet<String>) -> Answer<()> {
    let db = store.db(world)?;
    let gm = db.members()?.into_iter().find(|m| m.role == Role::Gm).map(|m| m.id).unwrap_or_default();
    let reader = Reader::of(member);
    let images = store::acting_for(&member.id, || store.images_seen(world))?;
    // Pictures every player sees go to all of them, once.
    let temp = store.private_world_dir(world).join("delivering").join(&member.slot);
    let _ = std::fs::remove_dir_all(&temp);
    db.view_copy(&temp, &reader, &[gm, member.id.clone()], &images)?;
    // What the world says of itself, as they may see it: the types and the
    // side bar without what is not for them, and no roles to give.
    {
        let copy = WorldDb::open(&temp)?;
        copy.save_types(&store::types_for(db.types()?, &reader))?;
        if let Some(navigation) = store.navigation_for(world, &reader)? { copy.set_json("navigation", &navigation)? }
        copy.set_json("roles", &Vec::<String>::new())?;
        copy.into_file()?;
    }
    let to = folder.join(&member.slot).join(IN);
    std::fs::create_dir_all(&to)?;
    let media = db.dir().join(aw_store::db::MEDIA);
    for image in db.images()?.into_iter().filter(|i| images.contains(&i.id)) {
        let into = if everyone.contains(&image.id) { folder.join(ALL).join(aw_store::db::MEDIA) } else { to.join(aw_store::db::MEDIA) };
        for file in [&image.file, &image.preview].into_iter().filter(|f| !f.is_empty()) {
            copy_missing(&media.join(file), &into.join(file))?;
        }
    }
    // The database last: once it is there, its pictures are too.
    let part = to.join("world.sqlite.part");
    std::fs::copy(temp.join(aw_store::db::FILE), &part)?;
    std::fs::rename(&part, to.join(aw_store::db::FILE))?;
    let _ = std::fs::remove_dir_all(&temp);
    Ok(())
}

/// The pictures a player with no role at all sees.
fn everyone_sees(store: &Store, world: &str) -> Answer<BTreeSet<String>> {
    let db = store.db(world)?;
    let reader = Reader::holding(&[]);
    let used: BTreeSet<String> = db.documents_for(&reader)?.iter().flat_map(|d| d.layers.iter().flat_map(|l| l.images.clone())).collect();
    Ok(used)
}

// --- The player's side ---------------------------------------------------------

/// A player's mailbox, as this device keeps track of it.
#[derive(Serialize, Deserialize, Default)]
struct Kept {
    /// When the world in `ein/` was last taken: its size and time.
    seen: (u64, u64),
}

fn kept_path(store: &Store, world: &str) -> PathBuf {
    store.private_world_dir(world).join("mailbox.json")
}

/// The id of the world in a mailbox.
fn world_in(folder: &Path) -> Answer<String> {
    let file = folder.join(IN).join(aw_store::db::FILE);
    if !file.exists() {
        return Err(AppError("Nothing there yet — the GM's app delivers it once it runs.".into()));
    }
    WorldDb::id_in(&file).ok().flatten().ok_or_else(|| AppError("This is no world's mailbox.".into()))
}

/// Takes part in a world through the mailbox in `folder`.
pub(crate) fn open(store: &Store, folder: &Path) -> Answer<WorldInfo> {
    let world = world_in(folder)?;
    if store.holds(&world) && !store.load_network()?.mailboxes.contains_key(&world) {
        return Err(AppError("This world lies here already.".into()));
    }
    let mut settings = store.load_network()?;
    settings.mailboxes.insert(world.clone(), Mailbox { folder: folder.display().to_string() });
    // Known from a server before: through the mailbox now.
    settings.remote.remove(&world);
    store.save_network(&settings)?;
    send(store, &world, "hello", serde_json::json!({}), BTreeMap::new())?;
    refresh(store, &world)?;
    world_now(store, &world)
}

/// Leaves a mailbox: this device's copy goes; the mailbox stays.
pub(crate) fn leave(store: &Store, world: &str) -> Answer<()> {
    let mut settings = store.load_network()?;
    settings.mailboxes.remove(world);
    store.save_network(&settings)?;
    if store.holds(world) {
        store.delete_world(world)?;
    }
    Ok(())
}

fn mailbox(store: &Store, world: &str) -> Answer<PathBuf> {
    store.load_network()?.mailboxes.get(world).map(|m| PathBuf::from(&m.folder)).ok_or_else(|| AppError("This world has no mailbox here.".into()))
}

/// Whether a world is taken part in through a mailbox.
pub(crate) fn is_mailbox(store: &Store, world: &str) -> bool {
    store.load_network().is_ok_and(|s| s.mailboxes.contains_key(world))
}

/// Writes a change into the mailbox's `aus/`.
fn send(store: &Store, world: &str, command: &str, args: serde_json::Value, states: BTreeMap<String, document::Plain>) -> Answer<()> {
    let me = store.me()?;
    let at = store::now();
    let change = Change { world: world.to_string(), card: me.public(), at, command: command.to_string(), args, states };
    let body = serde_json::to_string(&change)?;
    let signed = Signed { signature: hex::encode(me.sign(body.as_bytes())), body };
    let out = mailbox(store, world)?.join(OUT);
    std::fs::create_dir_all(&out)?;
    let tag = hex::encode(aw_crypto::random_bytes::<4>().map_err(|e| AppError(e.to_string()))?);
    write_json(&out.join(format!("{at:015}-{tag}.json")), &signed)
}

/// Commands that change nothing — not sent.
const READS: &[&str] = &[
    "world_connect", "world_sync", "world_revision", "view_index", "view_texts", "view_since", "trash_list", "templates_load", "templates_types", "template_read", "template_write",
    "navigation_load", "today_in_world", "texts_written", "document_open", "document_read", "article_versions",
    "version_compare", "version_document", "version_marks", "waiting_versions", "gallery", "gallery_folders", "image_data",
    "join_requests", "presence", "world_presence", "dice_roll", "dice_rolls", "dice_clear",
];

/// A command in a world taken part in through a mailbox: done on this
/// device's copy at once, and — what changes it — sent to the GM.
pub(crate) fn call(store: &Store, world: &str, command: &str, args: serde_json::Value) -> Answer<String> {
    let states = match command {
        "document_save" => serde_json::from_value::<document::Bases>(args.get("base").cloned().unwrap_or_default()).map(|b| document::states_of(&b)).unwrap_or_default(),
        _ => BTreeMap::new(),
    };
    let value = remote::call(store, world, command, &args)?;
    if !READS.contains(&command) {
        send(store, world, command, args, states)?;
    }
    Ok(value)
}

/// Takes the world from `ein/` where it changed — and what this device
/// sent that the GM's app has not taken yet stays as written. Says whether
/// it changed.
pub(crate) fn refresh(store: &Store, world: &str) -> Answer<bool> {
    let folder = mailbox(store, world)?;
    // Gone onto a server: worked in there from now on.
    #[derive(Deserialize, Default)]
    struct Moved {
        server: String,
    }
    let moved: Moved = read_json(&folder.join(IN).join(MOVED));
    if !moved.server.is_empty() {
        let name = store.load_world_meta(world).map(|m| (m.name, m.icon)).unwrap_or_default();
        let mut settings = store.load_network()?;
        settings.mailboxes.remove(world);
        settings.remote.insert(world.to_string(), store::RemoteWorld { name: name.0, icon: name.1, server: moved.server, asked: false });
        store.save_network(&settings)?;
        if store.holds(world) {
            store.delete_world(world)?;
        }
        return Ok(true);
    }
    let file = folder.join(IN).join(aw_store::db::FILE);
    let meta = std::fs::metadata(&file)?;
    let seen = (meta.len(), meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0));
    let mut kept: Kept = read_json(&kept_path(store, world));
    let done: Done = read_json(&folder.join(IN).join(DONE));
    let out = folder.join(OUT);
    // What the GM's app took or refused, this device lets go of.
    let mut pending = Vec::new();
    for entry in std::fs::read_dir(&out).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if done.taken.contains(&name) || done.refused.contains_key(&name) {
            if let Some(why) = done.refused.get(&name) {
                crate::applog::line("mailbox", &format!("{world}: the GM's app refused {name}: {why}"));
            }
            let _ = std::fs::remove_file(entry.path());
        } else if name.ends_with(".json") {
            pending.push(entry.path());
        }
    }
    if kept.seen == seen && store.holds(world) {
        return Ok(false);
    }
    // The copy anew: the world as delivered, its pictures beside it.
    let dir = store.worlds_dir().join(store::dir_name(world));
    store.close(world);
    std::fs::create_dir_all(dir.join(aw_store::db::MEDIA))?;
    for side in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{}{side}", aw_store::db::FILE)));
    }
    let part = dir.join("world.sqlite.part");
    std::fs::copy(&file, &part)?;
    std::fs::rename(&part, dir.join(aw_store::db::FILE))?;
    let all = folder.parent().map(|p| p.join(ALL).join(aw_store::db::MEDIA));
    for from in [Some(folder.join(IN).join(aw_store::db::MEDIA)), all].into_iter().flatten() {
        for entry in std::fs::read_dir(&from).into_iter().flatten().flatten() {
            copy_missing(&entry.path(), &dir.join(aw_store::db::MEDIA).join(entry.file_name()))?;
        }
    }
    // One's own place, as oneself — before the GM's app has seen one.
    let me = store.load_identity()?;
    let db = store.db(world)?;
    if db.member(me.id().as_str())?.is_none() {
        if let Some(slot) = db.members()?.into_iter().find(|m| m.id.starts_with(SLOT)) {
            db.remove_member(&slot.id)?;
            db.put_member(&Member { id: me.id().to_string(), verify_key: me.public().verify_key, ..slot })?;
        }
    }
    // What is on its way stands here as written.
    pending.sort();
    for path in pending {
        let Ok(signed) = serde_json::from_slice::<Signed>(&std::fs::read(&path)?) else { continue };
        let Ok(change) = serde_json::from_str::<Change>(&signed.body) else { continue };
        if change.command == "hello" {
            continue;
        }
        if let Ok(base) = serde_json::from_value::<document::Bases>(change.args.get("base").cloned().unwrap_or_default()) {
            document::take_states(&base, change.states);
        }
        let _ = remote::call(store, world, &change.command, &change.args);
    }
    kept.seen = seen;
    write_json(&kept_path(store, world), &kept)?;
    Ok(true)
}

// --- Commands --------------------------------------------------------------------

#[cfg(feature = "gui")]
/// Makes a world one without a server, its mailboxes in `folder` — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn mailbox_setup(app: tauri::AppHandle, world: String, folder: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    setup(&store, &world, Path::new(&folder))?;
    round(&store, &world)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// Places for players, by name — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn mailbox_add(app: tauri::AppHandle, world: String, names: Vec<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    add(&store, &world, &names)?;
    round(&store, &world)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// Frees a player's place — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn mailbox_reset(app: tauri::AppHandle, world: String, member: String) -> Answer<WorldInfo> {
    reset(&store(&app)?, &world, &member)
}

#[cfg(feature = "gui")]
/// Takes part in a world through one's mailbox.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn mailbox_open(app: tauri::AppHandle, folder: String) -> Answer<WorldInfo> {
    open(&store(&app)?, Path::new(&folder))
}

#[cfg(test)]
#[path = "mailbox_tests.rs"]
mod tests;
