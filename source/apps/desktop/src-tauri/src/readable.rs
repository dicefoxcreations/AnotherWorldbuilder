//! The readable copy of an unencrypted world: every document as a text file,
//! every image as an image file — in the world's folder, for any editor,
//! search or backup, with or without this app.
//!
//! ```text
//! <world>/text/<type>/<document>.md     the document as code, as the code editor shows it
//! <world>/text/images/<image>.<ext>     each image, whole
//! ```
//!
//! The journal stays what counts — versions, writing together, swapping
//! records. The copy follows it, and the other way round: a file changed
//! elsewhere becomes a version, merged on top of the state it was written
//! from. A file that goes is written again; documents go only in the app.
//!
//! What was written last lies in the profile (`readable.json`): per document
//! its file, the hash of what was written and the state it showed — so a
//! change by hand is told apart and merged against the right state.
//!
//! In the world's folder only the game master's device keeps it: in a shared
//! folder two devices would otherwise each take the other's files for
//! changes by hand. Anyone may keep one of their own beside their profile —
//! for an encrypted world too, then of what this device can read, and in the
//! clear on this device alone (`ON` switches it on).

use super::*;
use crate::document::{self, Bases};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const FOLDER: &str = "text";
const IMAGES: &str = "images";
const KEPT: &str = "readable.json";
/// In a world's private folder: keep a copy of one's own.
const ON: &str = "readable-on";

#[derive(Default, Serialize, Deserialize)]
struct Written {
    /// The journal's version the copy was made from — what changed after it is written again.
    version: u64,
    /// By document.
    docs: BTreeMap<String, Doc>,
    /// By image: its file in `images/`.
    images: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Doc {
    /// Below `text/`.
    path: String,
    hash: String,
    base: Bases,
    /// Lines the copy adds that the code does not have: image fields. They
    /// go again before a change by hand is taken in.
    #[serde(default)]
    added: Vec<String>,
}

/// Worlds whose copy is being written right now.
static WRITING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Brings the copy up to date in the background — once at a time per world.
pub(crate) fn refresh_soon(store: &Store, world: &str) {
    let writing = WRITING.get_or_init(Default::default);
    if !writing.lock().is_ok_and(|mut w| w.insert(world.to_string())) {
        return;
    }
    let (store, world) = (store.clone(), world.to_string());
    std::thread::spawn(move || {
        if let Err(e) = refresh(&store, &world) {
            eprintln!("readable copy of {world}: {}", e.0);
        }
        if let Ok(mut w) = WRITING.get_or_init(Default::default).lock() {
            w.remove(&world);
        }
    });
}

/// Takes in what was changed by hand, then writes what changed in the app.
/// Returns how many files were written or taken in.
pub(crate) fn refresh(store: &Store, world: &str) -> Answer<usize> {
    let Some(root) = place(store, world)? else { return Ok(0) };
    let kept = store.private_world_dir(world).join(KEPT);
    let mut written: Written = std::fs::read(&kept).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
    let me = store.load_identity()?;
    let mut count = 0;

    // By hand first: a file whose bytes are not what was written.
    let back = images_back(&written.images);
    for (article, doc) in written.docs.clone() {
        let Ok(text) = std::fs::read_to_string(root.join(&doc.path)) else { continue };
        if hash(&text) == doc.hash {
            continue;
        }
        let mut text = text;
        for line in &doc.added {
            text = text.replacen(&format!("{line}\n"), "", 1);
        }
        // A block that held nothing else goes with them.
        if let Some(rest) = text.strip_prefix("---\n---\n\n") {
            text = rest.to_string();
        }
        let code = from_files(&text, &back);
        let session = format!("readable-{}", now_ms());
        let saved = document::save(store, &me, world, &article, &code, &doc.base, &session);
        let _ = store.close_session(world, &session);
        match saved {
            Ok(()) => count += 1,
            // Not taken: what stands in the app is written over it below.
            Err(e) => eprintln!("{}: {}", doc.path, e.0),
        }
        written.version = 0;
    }

    let version = store.journal_version(world)?;
    let changed = if written.version == 0 { None } else { store.changed_since(world, written.version)? };
    crate::store::quietly(|| export(store, world, &root, &mut written, changed, &me).map(|n| count += n))?;
    written.version = version;
    aw_store::durable::write(&kept, &serde_json::to_vec(&written)?)?;
    Ok(count)
}

/// Writes what changed — `None`: everything — and takes away what went.
fn export(
    store: &Store,
    world: &str,
    root: &std::path::Path,
    written: &mut Written,
    changed: Option<std::collections::HashSet<ArticleId>>,
    me: &Identity,
) -> Answer<usize> {
    let mut count = 0;
    let shared = document::Shared::load(store, world, me)?;
    let held = store.view_keys(world, me)?;
    let views = build_view(&store.load_articles(world)?, &held);
    let templates = store.load_templates(world)?;
    let type_name = |id: &str| aw_model::types::find(&templates, id).map(|t| t.name.clone()).unwrap_or_else(|| "Documents".into());

    // Images first: the texts name their files.
    std::fs::create_dir_all(root.join(IMAGES))?;
    let dir = store.world_dir(world);
    let images: Vec<(String, aw_store::media::ImageMeta)> = store.with_journal(world, |journal| {
        Ok(journal.images().into_iter().filter_map(|(image, ..)| journal.image_meta(&image).map(|m| (image, m))).collect())
    })?;
    let mut taken: HashSet<String> = written.images.values().cloned().collect();
    for (image, meta) in &images {
        if store.switching(world) {
            return Err(AppError("The world is being switched.".into()));
        }
        let path = match written.images.get(image) {
            Some(file) if root.join(IMAGES).join(file).exists() => continue,
            Some(file) => file.clone(),
            None => unique(&mut taken, &format!("{}.{}", image_name(&meta.name), meta.image.format.extension())),
        };
        let target = root.join(IMAGES).join(&path);
        // Kept whole and as it is: the same file under a second name, not a copy.
        let whole = (meta.image.key.is_empty() && meta.image.parts.len() == 1)
            .then(|| aw_store::media::parts_dir(&dir).join(format!("{}.bin", meta.image.parts[0])))
            .filter(|p| std::fs::hard_link(p, &target).is_ok());
        if whole.is_none() {
            let Ok((bytes, _)) = store.image_bytes(world, image, false) else { continue };
            aw_store::durable::write(&target, &bytes)?;
        }
        written.images.insert(image.clone(), path);
        count += 1;
    }
    let files: HashMap<String, String> = written.images.iter().map(|(id, file)| (id.clone(), file.clone())).collect();

    let mut taken: HashSet<String> = written.docs.values().map(|d| d.path.to_lowercase()).collect();
    let here: HashSet<&str> = views.iter().filter(|v| !v.pending).map(|v| v.id.as_str()).collect();
    for view in views.iter().filter(|v| !v.pending) {
        // Switching began: nothing more in the clear — the switch decides what stays.
        if store.switching(world) {
            return Err(AppError("The world is being switched.".into()));
        }
        let before = written.docs.get(&view.id).cloned();
        let gone = before.as_ref().is_none_or(|d| !root.join(&d.path).exists());
        let touched = changed.as_ref().is_none_or(|c| c.iter().any(|a| a.to_string() == view.id));
        if !gone && !touched {
            continue;
        }
        let Ok(code) = document::code_in(store, world, &view.id, &shared) else { continue };
        // Image fields as lines of their own in the properties at the top.
        let added: Vec<String> = code
            .image_fields
            .iter()
            .filter_map(|(label, image)| files.get(image).map(|file| format!("{label}: ../{IMAGES}/{file}")))
            .collect();
        let text = with_lines(&to_files(&code.code, &files), &added);
        let folder = file_name(&type_name(&view.article_type));
        let wanted = format!("{folder}/{}.md", file_name(&view.name));
        // The same name stays where it was; a new one, where it is free.
        let path = match &before {
            Some(d) if same_place(&d.path, &wanted) => d.path.clone(),
            _ => {
                if let Some(d) = &before {
                    let _ = std::fs::remove_file(root.join(&d.path));
                    taken.remove(&d.path.to_lowercase());
                }
                unique_doc(&mut taken, &folder, &file_name(&view.name))
            }
        };
        let h = hash(&text);
        if before.as_ref().is_none_or(|d| d.hash != h || gone) {
            aw_store::durable::write(&root.join(&path), text.as_bytes())?;
            count += 1;
        }
        written.docs.insert(view.id.clone(), Doc { path, hash: h, base: code.base, added });
    }
    // Documents gone in the app go here too.
    let gone: Vec<String> = written.docs.keys().filter(|id| !here.contains(id.as_str())).cloned().collect();
    for id in gone {
        if let Some(d) = written.docs.remove(&id) {
            let _ = std::fs::remove_file(root.join(&d.path));
            count += 1;
        }
    }
    Ok(count)
}

/// Lines into the properties at the top — or a block of them, where there
/// are none.
fn with_lines(code: &str, lines: &[String]) -> String {
    if lines.is_empty() {
        return code.to_string();
    }
    let block: String = lines.iter().map(|l| format!("{l}\n")).collect();
    match code.strip_prefix("---\n") {
        Some(rest) => format!("---\n{block}{rest}"),
        None => format!("---\n{block}---\n\n{code}"),
    }
}

/// Whether a file stands for the name and type wanted — `Name (2).md` too.
fn same_place(path: &str, wanted: &str) -> bool {
    let stem = |p: &str| {
        let p = p.strip_suffix(".md").unwrap_or(p);
        match p.rfind(" (") {
            Some(i) if p.ends_with(')') && p[i + 2..p.len() - 1].chars().all(|c| c.is_ascii_digit()) => p[..i].to_string(),
            _ => p.to_string(),
        }
    };
    stem(path).eq_ignore_ascii_case(&stem(wanted))
}

fn unique_doc(taken: &mut HashSet<String>, folder: &str, name: &str) -> String {
    for n in 1.. {
        let path = if n == 1 { format!("{folder}/{name}.md") } else { format!("{folder}/{name} ({n}).md") };
        if taken.insert(path.to_lowercase()) {
            return path;
        }
    }
    unreachable!()
}

fn unique(taken: &mut HashSet<String>, file: &str) -> String {
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    for n in 1.. {
        let name = if n == 1 { file.to_string() } else { format!("{stem}-{n}.{ext}") };
        if taken.insert(name.clone()) {
            return name;
        }
    }
    unreachable!()
}

/// An image's name as its file: letters, digits, `-` and `_` only — the
/// text names it without quotes.
fn image_name(name: &str) -> String {
    let clean: String = name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).take(80).collect();
    let clean = clean.trim_matches('-').to_string();
    if clean.is_empty() { "image".into() } else { clean }
}

/// A name as a file may carry it — on any system.
fn file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    let clean = clean.trim().trim_matches('.').to_string();
    let clean: String = clean.chars().take(100).collect();
    if clean.is_empty() { "Untitled".into() } else { clean }
}

/// Images by their files instead of their ids: `img:…` → `../images/Red-Dragon.png`.
fn to_files(code: &str, files: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(i) = rest.find("img:") {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let end = after[4..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).map(|e| e + 4).unwrap_or(after.len());
        match files.get(&after[..end]) {
            Some(file) => out.push_str(&format!("../{IMAGES}/{file}")),
            None => out.push_str(&after[..end]),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// The file names back to the ids — longest first, so none stands for part of another.
fn images_back(files: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut back: Vec<(String, String)> = files.iter().map(|(id, file)| (format!("../{IMAGES}/{file}"), id.clone())).collect();
    back.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
    back
}

fn from_files(text: &str, back: &[(String, String)]) -> String {
    let mut out = text.to_string();
    for (path, id) in back {
        out = out.replace(path, id);
    }
    out
}

fn hash(text: &str) -> String {
    aw_store::journal::record_id(text.as_bytes())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Waits until no copy of the world is being written — one under way when
/// switching began stops at its next document (`export` looks).
pub(crate) fn wait_idle(world: &str) {
    while WRITING.get_or_init(Default::default).lock().is_ok_and(|w| w.contains(world)) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Takes the copy in the world's folder away — the world is encrypted
/// again, and nothing of it may lie there in the clear. One's own copy beside
/// the profile stays where it was switched on.
pub(crate) fn remove(store: &Store, world: &str) {
    wait_idle(world);
    let _ = std::fs::remove_dir_all(store.world_dir(world).join(FOLDER));
    let _ = std::fs::remove_file(store.private_world_dir(world).join(KEPT));
}

/// Where this device keeps a world's readable copy, if anywhere: in the
/// world's folder for the GM of an unencrypted world, else beside the
/// profile where switched on.
pub(crate) fn place(store: &Store, world: &str) -> Answer<Option<PathBuf>> {
    // Mid-switch every part changes once more: after that.
    // Held only while online: nothing of it stays, in the clear least of all.
    if !store.holds(world) || store.switching(world) || store.is_fleeting(world) {
        return Ok(None);
    }
    if store.is_gm(world) && !store.is_encrypted(world)? {
        return Ok(Some(store.world_dir(world).join(FOLDER)));
    }
    let private = store.private_world_dir(world);
    Ok(private.join(ON).exists().then(|| private.join(FOLDER)))
}

/// Whether the copy lies in the world's folder — always there, not to switch off.
pub(crate) fn in_world(store: &Store, world: &str) -> Answer<bool> {
    Ok(store.is_gm(world) && !store.is_encrypted(world)?)
}

/// Switches one's own copy on or off; off, it goes.
pub(crate) fn set_own(store: &Store, world: &str, on: bool) -> Answer<()> {
    let private = store.private_world_dir(world);
    if on {
        aw_store::durable::write(&private.join(ON), b"")?;
        refresh_soon(store, world);
    } else {
        let _ = std::fs::remove_file(private.join(ON));
        let _ = std::fs::remove_dir_all(private.join(FOLDER));
        let _ = std::fs::remove_file(private.join(KEPT));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_follows_the_world_and_the_world_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("profile"));
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let meta = new_world(&store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        assert_eq!(refresh(&store, &world).unwrap(), 0, "encrypted: no copy");
        store.set_encrypted(&world, false).unwrap();

        let mut raw = Vec::new();
        let img = image::RgbImage::from_pixel(40, 30, image::Rgb([20, 200, 30]));
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png).unwrap();
        let image = store.upload_image(&world, &raw, "Green Field", "", &gm).unwrap();
        let code = document::code_of(&store, &gm, &world, &meta.home).unwrap().code;
        let text = code.replace("<block>\n</block>", &format!("<block>\nThe inn is old.\n<image src={image}>\n</block>"));
        document::save_now(&store, &gm, &world, &meta.home, &text).unwrap();

        assert!(refresh(&store, &world).unwrap() > 0);
        let root = place(&store, &world).unwrap().unwrap();
        let file = root.join("Documents").join("Eldermere.md");
        let written = std::fs::read_to_string(&file).unwrap_or_else(|_| {
            let all: Vec<_> = walk(&root);
            panic!("not there: {all:?}")
        });
        assert!(written.contains("The inn is old.") && written.contains("../images/Green-Field.png"), "{written}");
        assert_eq!(std::fs::read(root.join("images").join("Green-Field.png")).unwrap(), store.image_bytes(&world, &image, false).unwrap().0);
        assert_eq!(refresh(&store, &world).unwrap(), 0, "nothing new, nothing written");

        // Changed by hand, elsewhere: a version in the app.
        std::fs::write(&file, written.replace("The inn is old.", "The inn is very old.")).unwrap();
        refresh(&store, &world).unwrap();
        let now = document::code_of(&store, &gm, &world, &meta.home).unwrap().code;
        assert!(now.contains("The inn is very old.") && now.contains(&image), "{now}");
        // A file that goes comes back.
        std::fs::remove_file(&file).unwrap();
        refresh(&store, &world).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("very old"));

        store.set_encrypted(&world, true).unwrap();
        assert!(!root.exists(), "encrypted again: the copy is gone");

        // One's own, beside the profile: of an encrypted world too.
        set_own(&store, &world, true).unwrap();
        refresh(&store, &world).unwrap();
        let own = place(&store, &world).unwrap().unwrap();
        assert!(own.starts_with(store.profile_dir()));
        assert!(std::fs::read_to_string(own.join("Documents").join("Eldermere.md")).unwrap().contains("very old"));
        set_own(&store, &world, false).unwrap();
        assert!(!own.exists());
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                out.extend(walk(&e.path()));
            } else {
                out.push(e.path());
            }
        }
        out
    }
}

/// How long the copy of a large world takes, from nothing — on a copy:
/// `AW_PROFILE=… AW_WORLDS=… AW_WORLD=… cargo test --release -p aw-desktop readable_timing -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn readable_timing() {
    let var = |k: &str| std::env::var(k).expect(k);
    let store = Store::with_roots(PathBuf::from(var("AW_PROFILE")), vec![PathBuf::from(var("AW_WORLDS"))]);
    let world = var("AW_WORLD");
    let root = place(&store, &world).unwrap().expect("a copy here");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(store.private_world_dir(&world).join(KEPT));
    let t = std::time::Instant::now();
    let n = refresh(&store, &world).unwrap();
    println!("whole copy: {n} files in {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let n = refresh(&store, &world).unwrap();
    println!("again: {n} files in {:?}", t.elapsed());
}
