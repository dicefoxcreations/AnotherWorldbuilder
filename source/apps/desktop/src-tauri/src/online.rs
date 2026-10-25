//! Server worlds have one authority: their server. Decryption and editing
//! happen in a disposable client session; only encrypted records are sent.
//! No command may fall back to an offline world, including the GM's.

use super::*;
use aw_sync::world::{Ask, Mark, Reply, Server, WorldNet};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{collections::{BTreeSet, HashMap, HashSet}, future::Future, pin::Pin,
          sync::{Arc, Mutex, OnceLock}};

/// Where progress is told: the app's windows (`online-progress`). Without
/// a window — the server, tests — nobody is told.
#[cfg(feature = "gui")]
static WINDOWS: OnceLock<tauri::AppHandle> = OnceLock::new();

#[cfg(feature = "gui")]
pub(crate) fn tell_progress_to(app: tauri::AppHandle) {
    let _ = WINDOWS.set(app);
}

/// How far loading from or sending to the server is — `done` of `total`;
/// `total` 0: done.
fn progress(world: &str, what: &str, done: usize, total: usize) {
    #[cfg(feature = "gui")]
    if let Some(app) = WINDOWS.get() {
        use tauri::Emitter;
        let _ = app.emit("online-progress", serde_json::json!({ "world": world, "what": what, "done": done, "total": total }));
    }
    #[cfg(not(feature = "gui"))]
    let _ = (world, what, done, total);
}

pub(crate) const UNAVAILABLE: &str = "[online] The server world is unavailable. No local copy was opened or saved.";
const BUDGET: usize = 4 * 1024 * 1024;
const CACHE_FOLDER: &str = "online-sessions";

pub(crate) trait Transport: Send + Sync {
    fn ask<'a>(&'a self, world: &'a str, ask: Ask) -> Pin<Box<dyn Future<Output = Answer<Reply>> + Send + 'a>>;
}

pub(crate) struct Connection<'a> {
    pub net: &'a WorldNet,
    pub server: &'a Server,
    pub me: &'a Identity,
}

impl Transport for Connection<'_> {
    fn ask<'a>(&'a self, world: &'a str, ask: Ask) -> Pin<Box<dyn Future<Output = Answer<Reply>> + Send + 'a>> {
        Box::pin(async move {
            let quick = matches!(ask, Ask::Digest { .. });
            let limit = std::time::Duration::from_secs(if quick { 6 } else { 120 });
            let reply = patiently(limit, || async {
                tokio::time::timeout(limit, self.net.ask_server(self.server, world, self.me, ask.clone()))
                    .await.map_err(|_| AppError(UNAVAILABLE.into()))?
                    .map_err(|e| AppError(format!("{UNAVAILABLE} {e}")))
            }).await?;
            // The server's own error, said as it is.
            if let Reply::Failed { error } = reply {
                return Err(AppError(format!("The server: {error}")));
            }
            Ok(reply)
        })
    }
}

/// Asks again while the server is busy writing this world for someone
/// else — a moment later, up to `limit`.
pub(crate) async fn patiently<F, Fut>(limit: std::time::Duration, mut ask: F) -> Answer<Reply>
where F: FnMut() -> Fut, Fut: Future<Output = Answer<Reply>> {
    let until = std::time::Instant::now() + limit;
    let mut pause = std::time::Duration::from_millis(100);
    loop {
        let reply = ask().await?;
        if reply != Reply::Busy || std::time::Instant::now() + pause > until {
            return Ok(reply);
        }
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(std::time::Duration::from_secs(1));
    }
}

/// How long what the server last said counts as current for a command.
/// The poll asks every two seconds, writing together more often.
pub(crate) const FRESH: std::time::Duration = std::time::Duration::from_millis(1500);
/// Missed polls in a row before the open world says it is cut off.
const MISSES: u32 = 3;
/// Image parts asked for in one go.
const PARTS_AT_ONCE: usize = 16;
/// Documents asked for in one go, where they are read in the background.
const ARTICLES_AT_ONCE: usize = 200;
/// How long a poll goes on reading what is not here yet — documents, then
/// pictures — a little at a time, the session free between for commands.
const IN_THE_BACKGROUND: std::time::Duration = std::time::Duration::from_millis(3000);
/// How often fetched pictures and documents redraw the open world.
const SHOWN_EVERY: std::time::Duration = std::time::Duration::from_secs(8);

/// What a command needs of the world before it runs: what the frame holds,
/// these documents whole, or all of it.
pub(crate) enum Needs {
    Frame,
    Articles(Vec<String>),
    /// An image shown — its preview, or itself.
    Image(String, bool),
    Everything,
}

impl Needs {
    /// What a command of the app needs: documents it names by id, or —
    /// for what looks at all of them — everything.
    pub(crate) fn of(command: &str, args: &serde_json::Value) -> Self {
        const WHOLE: [&str; 12] = ["world_copy", "world_check", "package_export", "world_tidy", "world_readable_set", "world_encryption_set",
            "template_save", "template_delete", "image_delete", "backup_now", "gm_handover_file", "world_share"];
        if WHOLE.contains(&command) {
            return Needs::Everything;
        }
        if command == "image_data" {
            let image = args.get("image").and_then(|i| i.as_str()).unwrap_or_default().to_string();
            return Needs::Image(image, args.get("preview").and_then(|p| p.as_bool()).unwrap_or(false));
        }
        fn ids(value: &serde_json::Value, out: &mut Vec<String>) {
            match value {
                serde_json::Value::String(s) if s.starts_with("art:") => out.push(s.clone()),
                serde_json::Value::Array(all) => all.iter().for_each(|v| ids(v, out)),
                serde_json::Value::Object(all) => all.values().for_each(|v| ids(v, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        ids(args, &mut out);
        if out.is_empty() { Needs::Frame } else { Needs::Articles(out) }
    }
}

struct Session {
    world: String,
    store: Store,
    _temporary: tempfile::TempDir,
    endpoint: String,
    copies: bool,
    /// When it last matched the server — none: not yet, or not since a miss.
    synced: Option<std::time::Instant>,
    /// What the server has acknowledged: only this survives a failed command.
    confirmed: HashSet<String>,
    /// Polls missed in a row.
    misses: u32,
    /// How far along the server's order of arrivals this session is.
    mark: Option<Mark>,
    /// Members and keys read since the records last changed.
    prepared: bool,
    /// Records a command brought that the open world has not been told of.
    untold: bool,
    /// Who asked to join, as the server told the GM or a supporter — and
    /// whom of them an admin let in — taken in with the next command or poll
    /// (`answer_asking`).
    requests: Vec<aw_crypto::IdentityPublic>,
    approved: Vec<String>,
    removing: Vec<String>,
    /// The server's admins, where they changed — the GM seals its keys for
    /// them.
    admins: Option<Vec<aw_crypto::IdentityPublic>>,
    /// Pictures and documents fetched since the open world last redrew for
    /// them, and when it did.
    unshown: usize,
    shown_at: std::time::Instant,
    /// The server's digest as last read — what a command builds on.
    remote: String,
    /// Whether the frame was read: what shows the world and the names of
    /// its documents.
    framed: bool,
    /// The documents not read whole yet, by id — asked for when a command
    /// needs them, and a batch at each poll.
    pending: Vec<String>,
}

type Slot = Arc<tokio::sync::Mutex<Option<Session>>>;
type Sessions = HashMap<(PathBuf, String), Slot>;
fn sessions() -> &'static Mutex<Sessions> {
    static SESSIONS: OnceLock<Mutex<Sessions>> = OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

fn slot(base: &Store, world: &str) -> Answer<Slot> {
    Ok(sessions().lock().map_err(AppError::from)?.entry((base.profile_dir().to_path_buf(), world.into())).or_default().clone())
}

impl Drop for Session {
    fn drop(&mut self) {
        self.store.close(&self.world);
        crate::forget_world_views(&self.world);
    }
}

fn copy_private(base: &Store, target: &Store, world: &str) -> Answer<()> {
    std::fs::create_dir_all(target.profile_dir())?;
    target.save_identity(&base.load_identity()?)?;
    if base.profile_dir().join("network.json").is_file() {
        std::fs::copy(base.profile_dir().join("network.json"), target.profile_dir().join("network.json"))?;
    }
    let into = target.private_world_dir(world);
    std::fs::create_dir_all(&into)?;
    for name in keys::KEPT.into_iter().chain(["backup.json", "dice.json"]) {
        let from = base.private_world_dir(world).join(name);
        if from.is_file() {
            std::fs::copy(&from, into.join(name))?;
        }
    }
    // An explicit backup belongs beside the real worlds, never in the cache.
    let mut plan: backup::Plan = std::fs::read(into.join("backup.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if plan.folder.is_empty() {
        plan.folder = base.worlds_dir().parent().unwrap_or(base.profile_dir()).join("backups").display().to_string();
        aw_store::durable::write(&into.join("backup.json"), &serde_json::to_vec(&plan)?)?;
    }
    Ok(())
}

impl Session {
    fn new(base: &Store, world: &str, endpoint: &str) -> Answer<Self> {
        let root = base.profile_dir().join(CACHE_FOLDER);
        std::fs::create_dir_all(&root)?;
        let temporary = tempfile::Builder::new().prefix("session-").tempdir_in(root)?;
        let store = Store::online_cache(temporary.path().join("profile"), temporary.path().join("worlds"));
        std::fs::create_dir_all(store.worlds_dir())?;
        copy_private(base, &store, world)?;
        Ok(Self { world: world.into(), store, _temporary: temporary, endpoint: endpoint.into(), copies: false,
                  synced: None, confirmed: HashSet::new(), misses: 0, mark: None, prepared: false, untold: false, requests: Vec::new(), approved: Vec::new(), removing: Vec::new(), admins: None,
                  unshown: 0, shown_at: std::time::Instant::now(), remote: String::new(), framed: false, pending: Vec::new() })
    }

    fn fresh(&self) -> bool {
        self.synced.is_some_and(|at| at.elapsed() < FRESH)
    }

    /// A new session with only what the server acknowledged — what a failed
    /// command wrote here is gone, never uploaded later. Read from this
    /// device, not downloaded again.
    fn without_unsent(&self, base: &Store) -> Answer<Self> {
        let world = self.world.as_str();
        let mut next = Session::new(base, world, &self.endpoint)?;
        let kept: Vec<String> = self.store.inventory(world)?.into_iter().filter(|id| self.confirmed.contains(id)).collect();
        let ordered = self.store.in_order(world, &kept)?;
        for batch in ordered.chunks(2000) {
            next.store.receive(world, &self.store.records(world, batch)?)?;
        }
        let missing = next.store.missing_parts(world)?;
        for batch in missing.chunks(16) {
            next.store.take_parts(world, &self.store.parts(world, batch))?;
        }
        if next.store.holds(world) {
            next.store.refresh_online_members(world).map_err(|e| AppError(format!("[online] {e}")))?;
            next.store.view_keys(world, &next.store.load_identity()?).map_err(|e| AppError(format!("[online] {e}")))?;
        }
        next.copies = self.copies;
        next.confirmed = next.store.inventory(world)?.into_iter().collect();
        // Everything acknowledged is here: the server's order goes on from where it was.
        next.mark = self.mark.clone();
        next.remote = self.remote.clone();
        next.framed = self.framed;
        next.pending = self.pending.clone();
        next.prepared = true;
        if next.store.holds(world) {
            next.store.count_from(world, self.store.control_count(world)?)?;
        }
        Ok(next)
    }

    /// Reads what the server has; true when records came. Pictures come
    /// with the poll, not before a command.
    async fn synchronize(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        self.synchronize_with(world, transport, std::time::Duration::ZERO).await
    }

    async fn synchronize_with(&mut self, world: &str, transport: &impl Transport, pictures: std::time::Duration) -> Answer<bool> {
        let came = self.read_server(world, transport).await?;
        self.fetch_parts(world, transport, pictures).await?;
        self.synced = Some(std::time::Instant::now());
        if came || self.confirmed.is_empty() {
            self.confirmed = self.store.inventory(world)?.into_iter().collect();
        }
        Ok(came)
    }

    /// One digest when nothing changed; then only what came after the mark,
    /// or — the server's order being another one — everything by name.
    async fn read_server(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        let reply = transport.ask(world, network::live_digest(world, self.store.load_identity()?.id().as_str())?).await?;
        network::receive_live(world, &reply);
        let Reply::Digest { digest, requests, approved, removing, admins, no_copy, mark, .. } = reply else {
            return Err(AppError(match reply {
                // Taken out: the world leaves the list, and may be asked for again.
                Reply::NotMember | Reply::NotOffered => {
                    if let Some(base) = profile::store_now().filter(|b| !b.holds_token(world)) {
                        network::drop_online(&base, world)?;
                    }
                    "You are no longer a member of this world."
                }
                Reply::Requested => "Asked to join again. The GM lets you in.",
                _ => UNAVAILABLE,
            }.into()));
        };
        self.copies = !no_copy;
        let mut came = false;
        let same_order = self.mark.as_ref().zip(mark.as_ref()).is_some_and(|(here, there)| here.epoch == there.epoch);
        if !self.framed || (!same_order && !self.pending.is_empty()) {
            // New to this session — or the server's order begun anew before
            // all of it was here: the frame first; documents follow as they
            // are read.
            came |= self.read_frame(world, transport).await?;
        } else if !same_order {
            // All of it here, the server's order another one: what is
            // missing, by name.
            if digest != self.remote {
                came |= self.read_missing(world, transport, &mark).await?;
            }
            self.mark = mark;
        }
        if digest != self.remote || came {
            came |= self.read_since(world, transport).await?;
        }
        if came || !self.prepared {
            self.store.refresh_online_members(world).map_err(|e| AppError(format!("[online] {e}")))?;
            let me = self.store.load_identity()?;
            self.store.view_keys(world, &me).map_err(|e| AppError(format!("[online] {e}")))?;
            self.prepared = true;
        }
        // Taken with what follows, so what they bring is sent with it.
        let gm = self.store.is_gm(world);
        if (!requests.is_empty() || !removing.is_empty()) && (gm || self.store.supports(world)?) {
            self.requests = requests;
            self.approved = approved;
            self.removing = removing;
        }
        if gm && self.store.server_admins(world)? != admins {
            self.admins = Some(admins);
        }
        Ok(came)
    }

    /// The frame, a piece at a time — see `Ask::Frame`. Its mark is the one
    /// it began at: what came meanwhile follows with `read_since`.
    async fn read_frame(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        let started = std::time::Instant::now();
        let mut after = None;
        let mut first = None;
        let mut came = false;
        let mut done = 0;
        loop {
            let Reply::Frame { records, next, mark, seq, total } = transport.ask(world, Ask::Frame { after: after.take() }).await? else {
                return Err(AppError(format!("{UNAVAILABLE} The server needs an update.")));
            };
            first.get_or_insert(mark);
            done += records.len();
            if !records.is_empty() && self.store.receive(world, &records)? > 0 {
                came = true;
            }
            if self.store.holds(world) {
                self.store.count_from(world, seq)?;
            }
            progress(world, "loading", done, total.max(1));
            match next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        progress(world, "loading", 0, 0);
        crate::applog::line("online", &format!("{world}: frame of {done} records read in {:.1} s", started.elapsed().as_secs_f64()));
        self.mark = first.flatten();
        self.framed = true;
        self.pending = if self.store.holds(world) { self.store.article_ids(world)? } else { Vec::new() };
        Ok(came)
    }

    /// What came after the mark, in the order it came.
    async fn read_since(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        let mut came = false;
        let Some(mut at) = self.mark.clone() else { return Ok(false) };
        loop {
            let Reply::Since { records, mark: next, digest } = transport.ask(world, Ask::Since { mark: at.clone() }).await? else {
                return Err(AppError(UNAVAILABLE.into()));
            };
            self.remote = digest;
            // The server's order begun anew meanwhile: read again from the start.
            let Some(next) = next else {
                self.mark = None;
                break;
            };
            if !records.is_empty() && self.store.receive(world, &records)? > 0 {
                came = true;
            }
            let done = records.is_empty() || next.at == at.at;
            at = next;
            self.mark = Some(at.clone());
            if done {
                break;
            }
        }
        Ok(came)
    }

    /// What this session lacks, by name — where all of the world is here and
    /// the server's order is another one.
    async fn read_missing(&mut self, world: &str, transport: &impl Transport, mark: &Option<Mark>) -> Answer<bool> {
        let mut came = false;
        loop {
            let have = self.store.inventory(world)?;
            let Reply::Records { records, wanted, digest } = transport.ask(world, Ask::Swap { have, give: Vec::new() }).await? else {
                return Err(AppError(UNAVAILABLE.into()));
            };
            // A stale or failed command is discarded, never uploaded as sync.
            if !wanted.is_empty() {
                return Err(AppError(format!("{UNAVAILABLE} The session must be read from the server again.")));
            }
            self.remote = digest;
            if records.is_empty() || self.store.receive(world, &records)? == 0 {
                break;
            }
            came = true;
            let total = mark.as_ref().map_or(0, |m| m.at as usize);
            progress(world, "loading", self.store.inventory(world)?.len(), total.max(1));
        }
        progress(world, "loading", 0, 0);
        Ok(came)
    }

    /// The documents named, whole — what the frame left out of them.
    async fn read_articles(&mut self, world: &str, transport: &impl Transport, wanted: &[String]) -> Answer<bool> {
        let mut asked: Vec<String> = wanted.iter().filter(|a| self.pending.contains(a)).cloned().collect();
        asked.dedup();
        let mut came = false;
        while !asked.is_empty() {
            let Reply::Articles { records, done } = transport.ask(world, Ask::Articles { articles: asked.clone() }).await? else {
                return Err(AppError(format!("{UNAVAILABLE} The server needs an update.")));
            };
            if done.is_empty() {
                return Err(AppError(UNAVAILABLE.into()));
            }
            if !records.is_empty() && self.store.receive(world, &records)? > 0 {
                came = true;
            }
            let done: HashSet<String> = done.into_iter().collect();
            self.pending.retain(|a| !done.contains(a));
            asked.retain(|a| !done.contains(a));
        }
        Ok(came)
    }

    /// The parts of an image about to be shown — before all the others.
    async fn read_image(&mut self, world: &str, transport: &impl Transport, image: &str, preview: bool) -> Answer<bool> {
        let want = self.store.image_parts_missing(world, image, preview)?;
        if want.is_empty() {
            return Ok(false);
        }
        let Reply::Parts { parts, .. } = transport.ask(world, Ask::Parts { want, give: Vec::new() }).await? else {
            return Err(AppError(UNAVAILABLE.into()));
        };
        let decoded: Vec<_> = parts.into_iter().map(|(name, encoded)| Ok((name, STANDARD.decode(encoded)?))).collect::<Answer<_>>()?;
        self.store.take_parts(world, &decoded)?;
        Ok(false)
    }

    /// All documents not read yet — for what looks at all of them.
    async fn read_rest(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        let mut came = false;
        while !self.pending.is_empty() {
            came |= self.read_a_batch(world, transport).await?;
        }
        Ok(came)
    }

    /// A batch of the documents not read yet; whether any came.
    async fn read_a_batch(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        let total = self.store.article_count(world)?.max(self.pending.len());
        let batch: Vec<String> = self.pending.iter().take(ARTICLES_AT_ONCE).cloned().collect();
        let came = self.read_articles(world, transport, &batch).await?;
        progress(world, "loading", total - self.pending.len(), total.max(1));
        if self.pending.is_empty() {
            progress(world, "loading", 0, 0);
        }
        Ok(came)
    }

    /// A little of what is not here yet, in the background: a batch of
    /// documents, else a handful of pictures. So lists, search and pictures
    /// fill while the world is open. Whether there was any.
    async fn read_a_little(&mut self, world: &str, transport: &impl Transport) -> Answer<bool> {
        if !self.pending.is_empty() {
            if self.read_a_batch(world, transport).await? {
                self.unshown += 1;
                self.confirmed = self.store.inventory(world)?.into_iter().collect();
                // Built here, not when the open world asks next.
                self.store.load_articles(world)?;
            }
            return Ok(true);
        }
        Ok(!self.fetch_parts(world, transport, std::time::Duration::from_millis(1)).await?)
    }

    /// Image contents follow their signed references, from the server only —
    /// a few at a time for as long as given, so the world opens before all
    /// its pictures are here. True once none is missing.
    async fn fetch_parts(&mut self, world: &str, transport: &impl Transport, time: std::time::Duration) -> Answer<bool> {
        let until = std::time::Instant::now() + time;
        let mut once = false;
        loop {
            let want = self.store.missing_parts(world)?;
            if want.is_empty() {
                return Ok(true);
            }
            // None asked for: no time at all; asked for, at least a handful.
            if time.is_zero() || (once && std::time::Instant::now() >= until) {
                return Ok(false);
            }
            once = true;
            let want: Vec<String> = want.into_iter().take(PARTS_AT_ONCE).collect();
            let Reply::Parts { parts, .. } = transport.ask(world, Ask::Parts { want, give: Vec::new() }).await? else {
                return Err(AppError(UNAVAILABLE.into()));
            };
            let decoded: Vec<_> = parts.into_iter().map(|(name, encoded)| Ok((name, STANDARD.decode(encoded)?))).collect::<Answer<_>>()?;
            if self.store.take_parts(world, &decoded)? == 0 {
                return Err(AppError(format!("{UNAVAILABLE} Images are still missing on the server.")));
            }
            self.unshown += decoded.len();
        }
    }

    fn remember_keys(&self, base: &Store, world: &str) -> Answer<()> {
        let into = base.private_world_dir(world);
        std::fs::create_dir_all(&into)?;
        // Persist newly generated keys BEFORE publishing their ciphertext. If
        // the final acknowledgement is lost, the GM can still open that data.
        for name in ["owner.key", "keyring.json"] {
            let from = self.store.private_world_dir(world).join(name);
            if from.is_file() {
                aw_store::durable::write(&into.join(name), &std::fs::read(from)?)?;
            }
        }
        Ok(())
    }

    fn persist_metadata(&self, base: &Store, world: &str) -> Answer<()> {
        let into = base.private_world_dir(world);
        std::fs::create_dir_all(&into)?;
        for name in keys::KEPT.into_iter().chain(["backup.json", "dice.json"]) {
            let from = self.store.private_world_dir(world).join(name);
            if from.is_file() {
                aw_store::durable::write(&into.join(name), &std::fs::read(from)?)?;
            }
        }
        Ok(())
    }

    fn info(&self, base: &Store, world: &str, server: &Server) -> Answer<serde_json::Value> {
        let me = self.store.load_identity()?;
        let mut info = serde_json::to_value(world_info(&self.store, &self.store.load_world_meta(world)?, me.id().as_str())?)?;
        info["online"] = true.into();
        info["fleeting"] = true.into();
        info["server"] = server.code().into();
        info["server_id"] = server.endpoint.clone().into();
        info["folder"] = "".into();
        info["readable"] = "".into();
        info["token_file"] = base.private_world_dir(world).join("owner.key").display().to_string().into();
        Ok(info)
    }

    /// A world on a server lies there and nowhere else — no copy on this
    /// device, the GM's neither, whatever the world says about copies.
    fn keep_allowed_copy(&self, base: &Store, world: &str) -> Answer<()> {
        discard_working_copy(&base.for_sync(), world)
    }
}

pub(crate) fn is_server_world(store: &Store, world: &str) -> bool {
    store.load_network().is_ok_and(|n| n.homes.contains_key(world))
}

pub(crate) fn requires_online(store: &Store, world: &str) -> bool {
    is_server_world(store, world) || network::online_worlds(store).iter().any(|w| w.world == world && w.info["online"] == true)
}

pub(crate) fn server_for(store: &Store, world: &str) -> Answer<Server> {
    let settings = store.load_network()?;
    let code = settings.home_of(world).ok_or_else(|| AppError(format!("{UNAVAILABLE} Its server is not configured.")))?;
    Server::parse(code).map_err(|e| AppError(e.to_string()))
}

pub(crate) fn discard_working_copy(base: &Store, world: &str) -> Answer<()> {
    if base.holds(world) {
        let dir = base.world_dir(world);
        base.close(world);
        std::fs::remove_dir_all(dir)?;
    }
    // Identity, world token, keyring and private membership metadata stay.
    // Generated plaintext and projections cannot become an offline fallback.
    for name in ["projection.json", "index.json", "readable.json", "text", "drafts", "readable.on"] {
        let p = base.private_world_dir(world).join(name);
        if p.is_dir() { std::fs::remove_dir_all(p)? } else if p.exists() { std::fs::remove_file(p)? }
    }
    Ok(())
}

pub(crate) fn purge_sessions(base: &Store) {
    let _ = std::fs::remove_dir_all(base.profile_dir().join(CACHE_FOLDER));
}

pub(crate) fn clear_sessions(base: &Store) {
    if let Ok(mut all) = sessions().lock() {
        all.retain(|(profile, _), _| profile != base.profile_dir());
    }
    purge_sessions(base);
}

pub(crate) async fn close_session(base: &Store, world: &str) -> Answer<()> {
    let handle = sessions().lock().map_err(AppError::from)?.get(&(base.profile_dir().to_path_buf(), world.to_string())).cloned();
    if let Some(handle) = handle { *handle.lock().await = None }
    Ok(())
}

pub(crate) async fn materialize_for_move(base: &Store, world: &str, server: &Server, transport: &impl Transport) -> Answer<()> {
    let handle = slot(base, world)?;
    let mut held = handle.lock().await;
    if held.as_ref().is_some_and(|s| s.endpoint != server.endpoint) { *held = None }
    if held.is_none() { *held = Some(Session::new(base, world, &server.endpoint)?) }
    let session = held.as_mut().unwrap();
    session.synchronize(world, transport).await?;
    session.read_rest(world, transport).await?;
    session.persist_metadata(base, world)?;
    let stage = base.worlds_dir().join(format!(".moving-from-server-{}", store::dir_name(world)));
    if stage.exists() { std::fs::remove_dir_all(&stage)? }
    copy_cache(&session.store.world_dir(world), &stage)?;
    let target = base.worlds_dir().join(store::dir_name(world));
    base.close(world);
    if target.exists() {
        let old = base.worlds_dir().join(format!(".previous-copy-{}", store::dir_name(world)));
        if old.exists() { std::fs::remove_dir_all(&old)? }
        std::fs::rename(&target, &old)?;
        if let Err(error) = std::fs::rename(&stage, &target) {
            let _ = std::fs::rename(&old, &target);
            return Err(error.into());
        }
        std::fs::remove_dir_all(old)?;
    } else {
        std::fs::rename(stage, target)?;
    }
    Ok(())
}

#[cfg(feature = "gui")]
pub(crate) async fn poll(app: &tauri::AppHandle, base: &Store, net: &WorldNet, me: &Identity) -> Answer<Vec<String>> {
    use tauri::Emitter;
    let current: Vec<_> = sessions().lock().map_err(AppError::from)?.iter()
        .filter(|((profile, _), _)| profile == base.profile_dir())
        .map(|((_, world), handle)| (world.clone(), handle.clone())).collect();
    let mut changed = Vec::new();
    for (world, handle) in current {
        // Busy with a command: that one reads the server itself.
        let Ok(mut held) = handle.try_lock() else { continue };
        let Some(session) = held.as_mut() else { continue };
        let result = async {
            let server = server_for(base, &world)?;
            if session.endpoint != server.endpoint { return Err(AppError(UNAVAILABLE.into())) }
            let transport = Connection { net, server: &server, me };
            // One digest when nothing changed — it carries who is here and the dice too.
            let brought = session.synchronize(&world, &transport).await?;
            if std::mem::take(&mut session.untold) && !brought {
                return Ok(true);
            }
            // Handed to this device's member: theirs now.
            let handed = !session.store.is_gm(&world) && session.store.handing_to(&world)?.as_deref() == Some(me.id().as_str());
            if !brought && session.requests.is_empty() && session.admins.is_none() && !handed {
                // Pictures and documents that came show now and then, and once all are here.
                let all = session.store.missing_parts(&world)?.is_empty() && session.pending.is_empty();
                let due = session.shown_at.elapsed() >= SHOWN_EVERY || all;
                if session.unshown > 0 && due {
                    session.unshown = 0;
                    session.shown_at = std::time::Instant::now();
                    return Ok(true);
                }
                return Ok(false);
            }
            let before = Before::take(&session.store, &world)?;
            if handed {
                network::take_what_is_handed(&session.store, &world, me)?;
            }
            crate::answer_proposals(&session.store, &world)?;
            answer_asking(session, &world)?;
            if before.wrote(&session.store, &world)? {
                let added = publish(session, base, &world, &transport, &before).await?;
                session.confirmed.extend(added);
            }
            // Handed on from this device, or taken over by a supporter: the
            // world names someone else — the keys leave here.
            if session.store.handed_over(&world)? {
                session.store.step_down(&world)?;
                base.step_down(&world)?;
            }
            session.persist_metadata(base, &world)?;
            remember_info(base, &world, &server, session.info(base, &world, &server)?, session.store.journal_version(&world)?)?;
            session.keep_allowed_copy(base, &world)?;
            Ok(true)
        }.await;
        match result {
            Ok(brought) => {
                if session.misses >= MISSES {
                    let _ = app.emit("online-available", serde_json::json!({ "world": world }));
                }
                session.misses = 0;
                if brought { changed.push(world.clone()) }
            }
            Err(error) => {
                // What was answered here but not acknowledged is not kept.
                if session.store.inventory(&world).map_or(true, |i| i.iter().any(|id| !session.confirmed.contains(id))) {
                    match session.without_unsent(base) {
                        Ok(clean) => *session = clean,
                        Err(_) => { *held = None; continue }
                    }
                }
                // Not reached for a moment: the session stays, read again
                // in full before the next command. Only a longer silence
                // stops the open world.
                session.synced = None;
                session.misses += 1;
                crate::applog::line("online", &format!("{world}: not reached ({} in a row): {}", session.misses, error.0));
                if session.misses == MISSES {
                    let message = if error.0.starts_with("[online]") { error.0 } else { format!("{UNAVAILABLE} {}", error.0) };
                    let _ = app.emit("online-unavailable", serde_json::json!({ "world": world, "error": message }));
                }
            }
        }
        drop(held);
        // What is not here yet, a little at a time — the session free
        // between for whatever command waits.
        let Ok(server) = server_for(base, &world) else { continue };
        let transport = Connection { net, server: &server, me };
        let until = std::time::Instant::now() + IN_THE_BACKGROUND;
        while std::time::Instant::now() < until {
            let mut held = handle.lock().await;
            let Some(session) = held.as_mut().filter(|s| s.endpoint == server.endpoint && s.misses == 0 && s.synced.is_some()) else { break };
            if !matches!(session.read_a_little(&world, &transport).await, Ok(true)) {
                break;
            }
        }
    }
    Ok(changed)
}

fn remember_info(base: &Store, world: &str, server: &Server, info: serde_json::Value, version: u64) -> Answer<()> {
    let mut all = network::online_worlds(base);
    all.retain(|w| w.world != world);
    all.push(network::OnlineWorld { world: world.into(), server: Some(server.code()), info, version });
    network::save_online(base, &all)
}

/// Moves a fully acknowledged initial upload into disposable session storage.
/// This is never called while records or images are still being uploaded.
pub(crate) async fn adopt_uploaded(base: &Store, world: &str, server: &Server) -> Answer<()> {
    let base = &base.for_sync();
    let handle = slot(base, world)?;
    let mut held = handle.lock().await;
    let mut session = Session::new(base, world, &server.endpoint)?;
    let target = session.store.worlds_dir().join(store::dir_name(world));
    let source = base.world_dir(world);
    let copies = base.copies(world)?;
    base.close(world);
    // Moved, not copied: what was uploaded lies on the server now, not here.
    if std::fs::rename(&source, &target).is_err() {
        copy_cache(&source, &target)?;
    }
    session.store.refresh_online_members(world)?;
    session.copies = copies;
    session.confirmed = session.store.inventory(world)?.into_iter().collect();
    // All of it is here: what the server holds now is what was sent.
    session.framed = true;
    session.remote = session.store.digest(world)?;
    let version = session.store.journal_version(world)?;
    let info = session.info(base, world, server)?;
    remember_info(base, world, server, info, version)?;
    session.keep_allowed_copy(base, world)?;
    let mut settings = base.load_network()?;
    settings.uploading.retain(|w| w != world);
    base.save_network(&settings)?;
    *held = Some(session);
    Ok(())
}

fn copy_cache(from: &Path, to: &Path) -> Answer<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() { copy_cache(&entry.path(), &target)? }
        else { std::fs::copy(entry.path(), target)?; }
    }
    Ok(())
}

pub(crate) fn remember_offer(base: &Store, world: &str, server: &Server, offer: &aw_sync::world::Offer, no_copy: bool) -> Answer<()> {
    let mut info = network::online_worlds(base).into_iter().find(|w| w.world == world).map(|w| w.info).unwrap_or_else(|| serde_json::json!({
        "id": world, "name": offer.name, "icon": offer.icon, "public_key": "", "token_file": "", "folder": "",
        "role": if base.holds_token(world) { "gm" } else { "player" }, "is_gm": base.holds_token(world),
        "attributes": [], "members": [], "article_count": 0, "home": "", "requests": 0, "invited": [],
        "takeovers": {}, "colours": {}, "moderated": false, "waiting": 0, "version_limit": 50,
        "encrypted": true, "readable": "", "readable_in_world": false, "offered": false, "dice": [],
        "supporters": [], "chosen": [], "admins": [], "supporter": false, "can_take_over": false, "roles": [], "conflicts": 0,
        "calendar": WorldCalendar::default(),
    }));
    info["name"] = offer.name.clone().into();
    info["icon"] = offer.icon.clone().into();
    info["online"] = true.into();
    info["server"] = server.code().into();
    info["server_id"] = server.endpoint.clone().into();
    info["copies"] = (!no_copy).into();
    info["fleeting"] = no_copy.into();
    remember_info(base, world, server, info, 0)
}

pub(crate) fn direct_world_command_allowed(command: &str) -> bool {
    matches!(command, "world_call" | "world_connect" | "world_disconnect" | "world_move" | "network_join" |
        "server_backups" | "window_open" | "dice_log" | "dice_log_add" | "dice_log_clear" | "dice_favorites_set")
}

#[cfg(feature = "gui")]
pub(crate) fn gate_invoke(invoke: &tauri::ipc::Invoke) -> bool {
    let tauri::ipc::InvokeBody::Json(args) = invoke.message.payload() else { return false };
    let Some(world) = args.get("world").and_then(|w| w.as_str()) else { return false };
    if direct_world_command_allowed(invoke.message.command()) { return false }
    let base = match profile::store(invoke.message.webview_ref().app_handle()) {
        Ok(base) => base,
        Err(error) => { invoke.resolver.clone().reject(error.0); return true; }
    };
    let protected = requires_online(&base, world);
    if protected {
        invoke.resolver.clone().reject(format!("{UNAVAILABLE} Server worlds require the online command channel."));
    }
    protected
}

#[cfg(feature = "gui")]
#[tauri::command]
pub(crate) async fn world_call(app: tauri::AppHandle, world: String, command: String, args: serde_json::Value) -> Answer<serde_json::Value> {
    let base = profile::store(&app)?;
    if !is_server_world(&base, &world) {
        return network::remote_call(app, world, command, args).await;
    }
    // No internal escape to the direct, durable world commands is exposed.
    if matches!(command.as_str(), "world_delete" | "world_move" | "remote_call" | "world_call") {
        return Err(AppError("Manage an online world's location or deletion explicitly through its server.".into()));
    }
    if network::online_worlds(&base).iter().any(|w| w.world == world && network::uses_host_rpc(w)) {
        let command = if command == "world_connect" { "changes".into() } else { command };
        return network::remote_call(app, world, command, args).await;
    }
    let server = server_for(&base, &world)?;
    if base.holds(&world) && base.is_gm(&world) && !network::online_worlds(&base).iter().any(|w| w.world == world) {
        return Err(AppError("[online] This world is still being uploaded. It cannot be opened as a server world until the upload is complete.".into()));
    }
    let net = network::running().ok_or_else(|| AppError(UNAVAILABLE.into()))?;
    let me = base.load_identity()?;
    let connection = Connection { net: &net, server: &server, me: &me };
    let name = world.clone();
    let needs = Needs::of(&command, &args);
    let label = command.clone();
    let result = execute(&base, &world, &server, &connection, needs, |cache| async move {
        if command == "world_connect" {
            // The game master's: each player's grant as it should be — a
            // grant written by an older version is brought up to date.
            if cache.is_gm(&name) {
                cache.refresh_grants(&name)?;
            }
            return Ok(serde_json::Value::Null);
        }
        dispatch::run(app.clone(), cache, &name, &command, &args).await
    }).await;
    if let Err(e) = &result {
        crate::applog::line("command", &format!("{world}: {label} failed: {}", e.0));
    }
    result
}

#[cfg(feature = "gui")]
#[tauri::command]
pub(crate) async fn world_connect(app: tauri::AppHandle, world: String) -> Answer<serde_json::Value> {
    let base = profile::store(&app)?;
    // Back in the one left last: it is not left any more.
    if let Ok(mut left) = left().lock() {
        if left.as_deref() == Some(world.as_str()) {
            *left = None;
        }
    }
    if is_server_world(&base, &world) {
        world_call(app, world.clone(), "world_connect".into(), serde_json::json!({})).await?;
        return network::online_worlds(&base).into_iter().find(|w| w.world == world).map(|w| w.info).ok_or_else(|| AppError(UNAVAILABLE.into()));
    }
    if requires_online(&base, &world) || !base.holds(&world) {
        network::remote_call(app, world.clone(), "changes".into(), serde_json::json!({})).await?;
        return network::online_worlds(&base).into_iter().find(|w| w.world == world).map(|w| w.info).ok_or_else(|| AppError(UNAVAILABLE.into()));
    }
    Ok(serde_json::to_value(world_info(&base, &base.load_world_meta(&world)?, base.load_identity()?.id().as_str())?)?)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub(crate) async fn world_disconnect(app: tauri::AppHandle, world: String) -> Answer<()> {
    let base = profile::store(&app)?;
    // Left, it stays read while the app runs — opened again at once. One
    // left behind at most: another one left before goes now.
    let before = left().lock().map_err(AppError::from)?.replace(world.clone());
    if let Some(before) = before.filter(|b| *b != world) {
        close_session(&base, &before).await?;
    }
    Ok(())
}

/// The server world last left, still read — see `world_disconnect`.
#[cfg(feature = "gui")]
fn left() -> &'static Mutex<Option<String>> {
    static LEFT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    LEFT.get_or_init(Default::default)
}

/// Sends what a command wrote; the records sent come back. Built on the
/// server's world as last read: changed meanwhile, a command that writes the
/// world's structure is refused.
async fn publish(session: &mut Session, base: &Store, world: &str, transport: &impl Transport, before: &Before) -> Answer<Vec<String>> {
    let added = before.added(&session.store, world)?;
    let parts: Vec<_> = session.store.wanted_parts(world)?.difference(&before.parts).cloned().collect();
    if added.is_empty() && parts.is_empty() {
        return Ok(added);
    }
    session.remember_keys(base, world)?;
    let transaction = hex::encode(aw_crypto::random_bytes::<32>()?);
    let revision = session.remote.clone();
    let result = async {
        let records = session.store.in_order(world, &added)?;
        let total = records.len() + parts.len();
        let mut at = 0;
        let mut piece = 0;
        while at < records.len() {
            let mut some = Vec::new();
            let mut size = 0;
            for raw in session.store.records(world, &records[at..records.len().min(at + 2000)])? {
                if size + raw.len() > BUDGET && !some.is_empty() { break }
                size += raw.len();
                some.push(raw);
            }
            if some.is_empty() { return Err(AppError("An online record could not be read.".into())) }
            at += some.len();
            // A usual edit fits in one piece: staged and committed in one ask.
            if at == records.len() && parts.is_empty() {
                return finish(transport, world, &transaction, &revision, some, piece).await;
            }
            let reply = transport.ask(world, Ask::Commit { transaction: transaction.clone(), base: revision.clone(), records: some, parts: Vec::new(), finish: false, piece }).await?;
            piece += 1;
            if reply != Reply::Staged { return Err(AppError(format!("{UNAVAILABLE} The server needs an update for online commands."))) }
            progress(world, "sending", at, total);
        }
        // Pictures some megabytes at a time.
        for (n, names) in parts.chunks(PARTS_AT_ONCE).enumerate() {
            let give: Vec<_> = session.store.parts(world, names).into_iter().map(|(name, bytes)| (name, STANDARD.encode(bytes))).collect();
            if give.len() != names.len() { return Err(AppError("An online image could not be read.".into())) }
            let reply = transport.ask(world, Ask::Commit { transaction: transaction.clone(), base: revision.clone(), records: Vec::new(), parts: give, finish: false, piece }).await?;
            piece += 1;
            if reply != Reply::Staged {
                return Err(AppError(UNAVAILABLE.into()));
            }
            progress(world, "sending", records.len() + (n + 1) * PARTS_AT_ONCE, total);
        }
        finish(transport, world, &transaction, &revision, Vec::new(), piece).await
    }.await;
    progress(world, "sending", 0, 0);
    if result.is_err() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), transport.ask(world, Ask::Abort { transaction })).await;
    }
    match &result {
        Ok(_) => crate::applog::line("online", &format!("{world}: saved {} records, {} image parts", added.len(), parts.len())),
        Err(e) => crate::applog::line("online", &format!("{world}: not saved ({} records): {}", added.len(), e.0)),
    }
    session.remote = result?;
    Ok(added)
}

/// Who asked to join: kept for the GM to decide — or, let in ahead of time
/// by their id, taken in at once; their grant goes to the server with what
/// follows.
fn answer_asking(session: &mut Session, world: &str) -> Answer<()> {
    if let Some(admins) = session.admins.take() {
        session.store.set_server_admins(world, admins)?;
    }
    let approved = std::mem::take(&mut session.approved);
    let asking = std::mem::take(&mut session.requests);
    network::take_out(&session.store, world, &std::mem::take(&mut session.removing))?;
    if asking.is_empty() {
        return Ok(());
    }
    network::take_requests(&session.store, world, asking, &approved)
}

/// What a session held before a command — to tell what the command wrote.
struct Before {
    digest: String,
    mark: Option<(String, u64)>,
    parts: BTreeSet<String>,
}

impl Before {
    fn take(store: &Store, world: &str) -> Answer<Self> {
        Ok(Self { digest: store.digest(world)?, mark: store.arrival_mark(world)?, parts: store.wanted_parts(world)? })
    }

    fn wrote(&self, store: &Store, world: &str) -> Answer<bool> {
        Ok(store.digest(world)? != self.digest || store.wanted_parts(world)? != self.parts)
    }

    /// The records written since, in the order they were.
    fn added(&self, store: &Store, world: &str) -> Answer<Vec<String>> {
        let (epoch, at) = self.mark.as_ref().ok_or_else(|| AppError(UNAVAILABLE.into()))?;
        let after = store.since(world, epoch, *at)?
            .ok_or_else(|| AppError(format!("{UNAVAILABLE} The session must be read from the server again.")))?;
        Ok(after.into_iter().map(|(_, id)| id).collect())
    }
}

/// The last piece of a command, and the commit — the server's digest after
/// it. The server keeps the receipt a while for a repeated ask, then lets
/// it go by itself.
async fn finish(transport: &impl Transport, world: &str, transaction: &str, revision: &str, records: Vec<String>, piece: u32) -> Answer<String> {
    let ask = Ask::Commit { transaction: transaction.into(), base: revision.into(), records, parts: Vec::new(), finish: true, piece };
    match transport.ask(world, ask).await? {
        Reply::Committed { digest } => Ok(digest),
        Reply::Conflict => Err(AppError("The server world changed meanwhile. This edit was not saved; please try again.".into())),
        _ => Err(AppError(UNAVAILABLE.into())),
    }
}

pub(crate) async fn execute<T, F, Fut, R>(base: &Store, world: &str, server: &Server, transport: &T, needs: Needs, operation: F) -> Answer<R>
where T: Transport, F: FnOnce(Store) -> Fut, Fut: Future<Output = Answer<R>> {
    let handle = slot(base, world)?;
    let mut held = handle.lock().await;
    if held.as_ref().is_some_and(|s| s.endpoint != server.endpoint) { *held = None }
    if held.is_none() { *held = Some(Session::new(base, world, &server.endpoint)?) }
    let session = held.as_mut().unwrap();
    // What the poll read a moment ago is current enough; a command asks
    // the server only when it has to write, or when that is longer ago.
    let mut changed = false;
    if !session.fresh() {
        match session.synchronize(world, transport).await {
            Ok(brought) => {
                changed = brought;
                // The poll tells the open world — it reads nothing new itself now.
                session.untold |= brought;
            }
            Err(error) => {
                // Never read at all: nothing to keep.
                if session.confirmed.is_empty() { *held = None }
                return Err(error);
            }
        }
    }
    // The documents the command reads, whole.
    let fetched = match needs {
        Needs::Frame => Ok(false),
        Needs::Articles(ids) => session.read_articles(world, transport, &ids).await,
        Needs::Image(image, preview) => session.read_image(world, transport, &image, preview).await,
        Needs::Everything => session.read_rest(world, transport).await,
    };
    match fetched {
        Ok(true) => {
            session.confirmed = session.store.inventory(world)?.into_iter().collect();
            session.untold = true;
        }
        Ok(false) => {}
        Err(error) => return Err(error),
    }
    let before = Before::take(&session.store, world)?;
    // What players asked to be made, and who asked to join, go with this command.
    if changed { crate::answer_proposals(&session.store, world)?; }
    answer_asking(session, world)?;
    let result = operation(session.store.clone()).await;
    let wrote = before.wrote(&session.store, world)?;
    let sent = match (&result, wrote) {
        (_, false) => Ok(false),
        (Ok(_), true) => match publish(session, base, world, transport, &before).await {
            Ok(added) => {
                session.confirmed.extend(added);
                Ok(true)
            }
            Err(error) => Err(error),
        },
        // A command that failed halfway: what it wrote is not sent.
        (Err(_), true) => Err(AppError(String::new())),
    };
    match sent {
        Ok(published) => {
            if published {
                session.copies = session.store.copies(world)?;
            }
            if published || changed {
                session.persist_metadata(base, world)?;
                let info = session.info(base, world, server)?;
                remember_info(base, world, server, info, session.store.journal_version(world)?)?;
                session.keep_allowed_copy(base, world)?;
            }
            result
        }
        Err(error) => {
            // A failed command must never become a background upload later:
            // the session goes on with only what the server acknowledged.
            *held = match held.as_ref().map(|s| s.without_unsent(base)) {
                Some(Ok(clean)) => Some(clean),
                _ => None,
            };
            match result {
                Err(e) => Err(e),
                Ok(_) => Err(error),
            }
        }
    }
}

#[cfg(feature = "gui")]
thread_local! { static COMMAND_STORE: std::cell::RefCell<Option<Store>> = const { std::cell::RefCell::new(None) }; }
#[cfg(feature = "gui")]
tokio::task_local! { static ASYNC_STORE: Store; }

#[cfg(feature = "gui")]
pub(crate) fn command_store() -> Option<Store> {
    COMMAND_STORE.with(|s| s.borrow().clone()).or_else(|| ASYNC_STORE.try_with(Clone::clone).ok())
}

#[cfg(feature = "gui")]
fn with_store<R>(store: Store, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<Store>);
    impl Drop for Restore {
        fn drop(&mut self) { COMMAND_STORE.with(|s| *s.borrow_mut() = self.0.take()); }
    }
    let _restore = Restore(COMMAND_STORE.with(|s| s.replace(Some(store))));
    run()
}

#[cfg(feature = "gui")]
mod dispatch;
