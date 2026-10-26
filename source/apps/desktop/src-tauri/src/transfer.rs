//! Moving types and documents between worlds: a **package**.
//!
//! A package is a folder — `package.json` and the images beside it in
//! `images/`. It is plain text, as the world is: everything the game master
//! sees goes in.
//!
//! Documents keep their ids, and so do types: links between them stay whole,
//! and taking the same package in twice adds nothing. Images get ids of the
//! importing game master's (`media::imported_id`) — still the same for the
//! same original — and every text and value naming them is written anew.
//! Access travels along: a part only the GM sees stays the GM's.

use super::*;
use aw_model::Template;
use aw_store::media::{self as store_media, ImageDefaults};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const FORMAT: &str = "aw-package";
pub const VERSION: u32 = 1;
const FILE: &str = "package.json";
const IMAGES: &str = "images";

#[derive(Serialize, Deserialize, Default)]
pub struct Package {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub name: String,
    /// Texts as written — names (`@@Spell:Fireball@@`, `%%Armor class%%`)
    /// instead of ids, the way an editor shows them. Taking the package in
    /// translates them. An export writes ids.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub written: bool,
    #[serde(default)]
    pub types: Vec<Template>,
    #[serde(default)]
    pub documents: Vec<PackedDocument>,
    #[serde(default)]
    pub images: Vec<PackedImage>,
    /// The side bar's entries — taken in where the world has none yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<Vec<aw_model::navigation::NavEntry>>,
    /// How the world counts its days — taken in where the world still has
    /// the default; the dates in the documents are days of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<aw_model::WorldCalendar>,
    /// Which of its documents is the world's page — the first thing one sees
    /// inside it. Taken where the world's own page is still empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PackedDocument {
    pub id: String,
    /// The type's id — empty for a document without one.
    #[serde(rename = "type", default)]
    pub article_type: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    /// Who sees and who changes the document as a whole.
    pub view: String,
    pub edit: String,
    /// What the base part holds beside the name — the order of its own sections.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, serde_json::Value>,
    pub parts: Vec<PackedPart>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PackedPart {
    pub section: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, serde_json::Value>,
    pub view: String,
    pub edit: String,
    #[serde(default = "open_level")]
    pub level: String,
}

fn open_level() -> String {
    aw_model::template::LEVEL_OPEN.into()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PackedImage {
    pub id: String,
    pub name: String,
    /// Relative to the package's folder.
    pub file: String,
    #[serde(default)]
    pub defaults: ImageDefaults,
}

/// What a package holds, measured against a world — before taking it in.
#[derive(Serialize, Default, Debug, PartialEq)]
pub struct Summary {
    pub name: String,
    pub types_new: usize,
    pub types_known: usize,
    pub documents_new: usize,
    pub documents_known: usize,
    pub images_new: usize,
    pub images_known: usize,
    /// Images that could not be taken in — no picture the app reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images_skipped: Vec<String>,
}

/// The folder a package lies in — given it, or its `package.json`.
fn folder_of(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|n| n == FILE) {
        path.parent().map(Path::to_path_buf).unwrap_or_default()
    } else {
        path.to_path_buf()
    }
}

pub fn read(path: &Path) -> Answer<(PathBuf, Package)> {
    let folder = folder_of(path);
    let raw = std::fs::read(folder.join(FILE)).map_err(|_| AppError("There is no package here.".into()))?;
    let package: Package = serde_json::from_slice(&raw)?;
    if package.format != FORMAT {
        return Err(AppError("This is no package of this app.".into()));
    }
    if package.version > VERSION {
        return Err(AppError("This package comes from a newer version of the app.".into()));
    }
    Ok((folder, package))
}

// --- Export --------------------------------------------------------------

/// What goes into a package.
#[derive(Deserialize, Default)]
pub struct Choice {
    #[serde(default)]
    pub name: String,
    /// Types by id. Their supertypes, the groups they take in and the types
    /// their references name come along.
    #[serde(default)]
    pub types: Vec<String>,
    /// Types whose documents come along — each with its type.
    #[serde(default)]
    pub documents_of: Vec<String>,
    /// Single documents by id, with their types.
    #[serde(default)]
    pub documents: Vec<String>,
}

/// The types `wanted` needs to make sense: itself, its supertypes, the
/// groups it takes in, the types its references point at.
fn with_needs(all: &[Template], wanted: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut todo: Vec<String> = wanted.into_iter().collect();
    while let Some(id) = todo.pop() {
        let Some(t) = all.iter().find(|t| t.id == id) else { continue };
        if !out.insert(id) {
            continue;
        }
        if !t.supertype.is_empty() {
            todo.push(t.supertype.clone());
        }
        for field in &t.properties {
            match &field.kind {
                aw_model::FieldKind::Group { group } => todo.push(group.clone()),
                aw_model::FieldKind::Reference { article_type, .. } if !article_type.is_empty() => todo.push(article_type.clone()),
                aw_model::FieldKind::Relation { article_type, role_type } => {
                    todo.push(article_type.clone());
                    todo.push(role_type.clone());
                }
                _ => {}
            }
        }
    }
    out
}

/// Tells how far it is: `documents`, then `images`, each with done and total.
pub fn export(store: &Store, world: &str, choice: &Choice, path: &Path, progress: &dyn Fn(&str, usize, usize)) -> Answer<Summary> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM exports.".into()));
    }
    let templates = store.load_templates(world)?;
    let stored = store.db(world)?.documents()?;
    let is_a = |id: &str, ancestor: &str| aw_model::types::is_a(&templates, id, ancestor);

    // The documents, then the types they and the choice need.
    let mut documents: BTreeSet<String> = choice.documents.iter().cloned().collect();
    for a in stored.iter() {
        if choice.documents_of.iter().any(|t| is_a(&a.doc_type, t)) {
            documents.insert(a.id.clone());
        }
    }
    let types = with_needs(
        &templates,
        choice.types.iter().chain(&choice.documents_of).cloned().chain(stored.iter().filter(|a| documents.contains(&a.id)).map(|a| a.doc_type.clone())),
    );
    // A type's page goes with it.
    for t in templates.iter().filter(|t| types.contains(&t.id) && !t.page.is_empty()) {
        documents.insert(t.page.clone());
    }

    let mut package = Package { format: FORMAT.into(), version: VERSION, name: choice.name.clone(), ..Package::default() };
    package.types = templates.iter().filter(|t| types.contains(&t.id)).cloned().collect();
    let mut images = BTreeSet::new();
    let total = stored.iter().filter(|a| documents.contains(&a.id)).count();
    for (n, a) in stored.iter().filter(|a| documents.contains(&a.id)).enumerate() {
        progress("documents", n, total);
        let mut doc: Option<PackedDocument> = None;
        let mut parts = Vec::new();
        for layer in &a.layers {
            images.extend(layer.images.iter().cloned());
            images.extend(store::named_images(&layer.text));
            let fields: BTreeMap<String, serde_json::Value> = layer.fields.clone().into_iter().collect();
            if layer.base {
                doc = Some(PackedDocument {
                    id: a.id.clone(),
                    article_type: a.doc_type.clone(),
                    slug: a.slug.clone(),
                    name: layer.title.clone(),
                    view: layer.access.view.clone(),
                    edit: layer.access.edit.clone(),
                    fields,
                    parts: Vec::new(),
                });
            } else {
                parts.push(PackedPart {
                    section: layer.section.clone(),
                    title: layer.title.clone(),
                    text: layer.text.clone(),
                    fields,
                    view: layer.access.view.clone(),
                    edit: layer.access.edit.clone(),
                    level: layer.access.level.clone(),
                });
            }
        }
        if let Some(mut doc) = doc {
            doc.parts = parts;
            package.documents.push(doc);
        }
    }
    for t in &package.types {
        if let Ok(text) = serde_json::to_string(t) {
            images.extend(store::named_images(&text));
        }
    }

    package.navigation = store.navigation(world)?;
    let calendar = store.load_world_meta(world)?.calendar;
    package.calendar = (calendar != aw_model::WorldCalendar::default()).then_some(calendar);
    let home = store.load_world_meta(world)?.home;
    package.home = package.documents.iter().any(|d| d.id == home).then_some(home);
    let folder = folder_of(path);
    std::fs::create_dir_all(folder.join(IMAGES))?;
    let gallery: HashMap<String, store::GalleryImage> = store.gallery(world)?.into_iter().map(|g| (g.id.clone(), g)).collect();
    let total = images.len();
    for (n, id) in images.into_iter().enumerate() {
        progress("images", n, total);
        let Ok((bytes, mime)) = store.image_bytes(world, &id, false) else { continue };
        let extension = mime.rsplit('/').next().unwrap_or("bin").replace("jpeg", "jpg");
        let file = format!("{IMAGES}/{}.{extension}", id.trim_start_matches("img:"));
        std::fs::write(folder.join(&file), bytes)?;
        let found = gallery.get(&id);
        package.images.push(PackedImage {
            name: found.map(|g| g.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone()),
            defaults: found.map(|g| g.defaults.clone()).unwrap_or_default(),
            id,
            file,
        });
    }
    aw_store::durable::write(&folder.join(FILE), &serde_json::to_vec_pretty(&package)?)?;
    Ok(Summary {
        name: package.name,
        types_new: package.types.len(),
        documents_new: package.documents.len(),
        images_new: package.images.len(),
        ..Summary::default()
    })
}

// --- Import --------------------------------------------------------------

/// What a package would bring into a world.
pub fn summary(store: &Store, world: &str, package: &Package) -> Answer<Summary> {
    let templates = store.load_templates(world)?;
    let known: BTreeSet<String> = store.db(world)?.document_ids()?.into_iter().collect();
    let images: BTreeSet<String> = store.gallery(world)?.into_iter().map(|g| g.id).collect();
    let me_id = store.actor()?;
    let mut out = Summary { name: package.name.clone(), ..Summary::default() };
    for t in &package.types {
        if templates.iter().any(|x| x.id == t.id) { out.types_known += 1 } else { out.types_new += 1 }
    }
    for d in &package.documents {
        if known.contains(&d.id) { out.documents_known += 1 } else { out.documents_new += 1 }
    }
    for i in &package.images {
        if images.contains(&store_media::imported_id(&me_id, &i.id)) { out.images_known += 1 } else { out.images_new += 1 }
    }
    Ok(out)
}

/// Writes image ids anew wherever they stand.
fn renamed(text: &str, ids: &BTreeMap<String, String>) -> String {
    if !text.contains("img:") {
        return text.to_string();
    }
    let mut out = text.to_string();
    for (old, new) in ids {
        if old != new {
            out = out.replace(old.as_str(), new);
        }
    }
    out
}

fn renamed_value(value: &serde_json::Value, ids: &BTreeMap<String, String>) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(renamed(s, ids)),
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(|v| renamed_value(v, ids)).collect()),
        serde_json::Value::Object(o) => serde_json::Value::Object(o.iter().map(|(k, v)| (k.clone(), renamed_value(v, ids))).collect()),
        other => other.clone(),
    }
}

/// Takes a package in: types first (one of the same id is replaced), then
/// images, then the documents not here yet. Reports what it added.
pub fn import(store: &Store, world: &str, folder: &Path, package: &Package, progress: &dyn Fn(usize, usize)) -> Answer<Summary> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM imports.".into()));
    }
    if let Some(calendar) = &package.calendar {
        calendar.validate().map_err(AppError::from)?;
    }
    let before = summary(store, world, package)?;
    let me_id = store.actor()?;
    let ids: BTreeMap<String, String> = package.images.iter().map(|i| (i.id.clone(), store_media::imported_id(&me_id, &i.id))).collect();
    let total = package.images.len() + package.documents.len();
    let mut done = 0;

    // The types: replaced where the id is the same, added otherwise.
    let mut templates = store.load_templates(world)?;
    let mut incoming = Vec::new();
    for t in &package.types {
        let t: Template = serde_json::from_value(renamed_value(&serde_json::to_value(t)?, &ids))?;
        match templates.iter_mut().find(|x| x.id == t.id) {
            // A page the world has stays, unless the package brings its own.
            Some(slot) => *slot = Template { page: if t.page.is_empty() { slot.page.clone() } else { t.page.clone() }, ..t.clone() },
            None => templates.push(t.clone()),
        }
        incoming.push(t.id);
    }
    // Written texts name what the world and the package hold.
    let catalog = if package.written {
        let mut documents: Vec<(String, String, String)> = view_with(store, world, &Reader::gm())?.into_iter().map(|a| (a.id, a.name, a.article_type)).collect();
        documents.extend(package.documents.iter().map(|d| (d.id.clone(), d.name.clone(), d.article_type.clone())));
        let known = document::known_of(documents, templates.clone());
        for t in templates.iter_mut().filter(|t| incoming.contains(&t.id)) {
            let id = t.id.clone();
            for part in t.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()) {
                part.text = names::stored(&part.text, &known, &id);
            }
            for block in &mut t.blocks {
                block.new = document::types_as(&known, &block.new, true);
            }
            default_documents(t, &known, true);
        }
        Some(known)
    } else {
        None
    };
    if !package.types.is_empty() {
        for t in &templates {
            aw_model::types::validate_hierarchy(&templates, t).map_err(|e| AppError(format!("The types do not fit together: {e}")))?;
        }
        store.save_templates(world, &templates)?;
    }

    // The images.
    let here: BTreeSet<String> = store.gallery(world)?.into_iter().map(|g| g.id).collect();
    let mut skipped = Vec::new();
    for image in &package.images {
        let id = &ids[&image.id];
        if !here.contains(id) {
            let raw = std::fs::read(folder.join(&image.file))?;
            // One picture the app cannot read does not stop the rest.
            if store.upload_image_as(world, id, &raw, &image.name, "").is_err() {
                skipped.push(image.name.clone());
                done += 1;
                continue;
            }
            if image.defaults != ImageDefaults::default() {
                store.set_image_defaults(world, id, image.defaults.clone())?;
            }
        }
        done += 1;
        progress(done, total);
    }

    // The documents not here yet.
    let db = store.db(world)?;
    let known: BTreeSet<String> = db.document_ids()?.into_iter().collect();
    let by = store.actor()?;
    let at = store::now();
    for doc in &package.documents {
        done += 1;
        if known.contains(&doc.id) {
            continue;
        }
        doc.view.parse::<Policy>().map_err(AppError::from)?;
        let new_id = || aw_model::LayerId::new().to_string();
        let base_level = if doc.view.parse::<Policy>().map(|p| p.to_string()).as_deref() == Ok("gm") { "gm-only" } else { "open" };
        let mut layers = vec![store::Layer {
            id: new_id(),
            base: true,
            section: String::new(),
            title: doc.name.clone(),
            access: store::Access { view: doc.view.clone(), edit: doc.edit.clone(), level: base_level.into() },
            text: String::new(),
            fields: doc.fields.clone().into_iter().collect(),
            images: Vec::new(),
        }];
        for part in &doc.parts {
            part.view.parse::<Policy>().map_err(AppError::from)?;
            let text = renamed(&part.text, &ids);
            let text = match &catalog {
                Some(catalog) => names::stored(&text, catalog, &doc.article_type),
                None => text,
            };
            let fields: serde_json::Map<String, serde_json::Value> = part.fields.iter().map(|(k, v)| (k.clone(), renamed_value(v, &ids))).collect();
            let mut images = store::named_images(&text);
            for value in fields.values() {
                images.extend(store::named_images(&value.to_string()));
            }
            layers.push(store::Layer {
                id: new_id(),
                base: false,
                section: part.section.clone(),
                title: part.title.clone(),
                access: store::Access { view: part.view.clone(), edit: part.edit.clone(), level: part.level.clone() },
                text,
                fields,
                images: images.into_iter().collect(),
            });
        }
        db.put_document(&store::Document {
            id: doc.id.clone(),
            doc_type: doc.article_type.clone(),
            slug: doc.slug.clone(),
            layers,
            created: at,
            created_by: by.clone(),
            changed: at,
            changed_by: by.clone(),
            proposed: false,
        })?;
        if done % 50 == 0 {
            progress(done, total);
        }
    }
    // Every type has its page, as one made in the app: what lacks one gets
    // the plain list.
    let here: BTreeSet<String> = db.document_ids()?.into_iter().collect();
    let mut templates = store.load_templates(world)?;
    let mut paged = false;
    for t in templates.iter_mut().filter(|t| !t.group && !here.contains(&t.page)) {
        t.page = create_type_page(store, world, t)?;
        paged = true;
    }
    if paged {
        store.save_templates(world, &templates)?;
    }
    if let Some(navigation) = &package.navigation {
        if store.navigation(world)?.is_none() {
            store.set_navigation(world, navigation)?;
        }
    }
    if let Some(home) = package.home.as_ref().filter(|h| package.documents.iter().any(|d| d.id == **h)) {
        take_home(store, world, home)?;
    }
    if let Some(calendar) = &package.calendar {
        let mut meta = store.load_world_meta(world)?;
        if meta.calendar == aw_model::WorldCalendar::default() {
            meta.calendar = calendar.clone();
            store.save_world_meta(&meta)?;
        }
    }
    progress(total, total);
    Ok(Summary { images_skipped: skipped, ..before })
}

/// The package's page becomes the world's — where the world's own is still
/// empty, which then goes. A page with text stays the world's.
fn take_home(store: &Store, world: &str, home: &str) -> Answer<()> {
    let mut meta = store.load_world_meta(world)?;
    if meta.home == home {
        return Ok(());
    }
    let db = store.db(world)?;
    let old = db.document(&meta.home)?;
    let empty = old.as_ref().is_none_or(|d| d.layers.iter().filter(|l| !l.base).all(|l| l.text.trim().is_empty()));
    if !empty {
        return Ok(());
    }
    meta.home = home.to_string();
    store.save_world_meta(&meta)?;
    if let Some(old) = old {
        db.delete_document(&old.id)?;
    }
    Ok(())
}

// --- Commands ------------------------------------------------------------

#[cfg(feature = "gui")]
/// Writes a package into a folder — plain text.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn package_export(app: tauri::AppHandle, world: String, path: String, choice: Choice) -> Answer<Summary> {
    let store = store(&app)?;
    tauri::async_runtime::spawn_blocking(move || export(&store, &world, &choice, Path::new(&path), &|_, _, _| {}))
    .await
    .map_err(AppError::from)?
}

#[cfg(feature = "gui")]
/// What a package would bring into a world.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn package_read(app: tauri::AppHandle, world: String, path: String) -> Answer<Summary> {
    let store = store(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, package) = read(Path::new(&path))?;
        summary(&store, &world, &package)
    })
    .await
    .map_err(AppError::from)?
}

/// One package taken in at a time.
static TAKING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Takes in a package sent as content — from the browser, or from a member
/// working on a server: its `package.json`, and its files base64 under the
/// paths the package names.
pub(crate) fn take_in(store: &Store, world: &str, package: &serde_json::Value, files: &BTreeMap<String, String>, progress: &dyn Fn(usize, usize)) -> Answer<Summary> {
    use base64::Engine;
    if !store.is_gm(world) {
        return Err(AppError("Only the GM imports.".into()));
    }
    let _one = TAKING.lock().map_err(AppError::from)?;
    let folder = store.profile_dir().join("tmp").join(format!("package-{}", hex::encode(aw_crypto::random_bytes::<6>().map_err(|e| AppError(e.to_string()))?)));
    let taken = (|| {
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(FILE), serde_json::to_vec(package)?)?;
        for (name, data) in files {
            let path = Path::new(name);
            // Inside the package's folder, nowhere else.
            if path.is_absolute() || path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                return Err(AppError(format!("Not a file of the package: {name}")));
            }
            let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| AppError(e.to_string()))?;
            let target = folder.join(path);
            std::fs::create_dir_all(target.parent().unwrap_or(&folder))?;
            std::fs::write(target, bytes)?;
        }
        let (dir, package) = read(&folder)?;
        import(store, world, &dir, &package, progress)
    })();
    let _ = std::fs::remove_dir_all(&folder);
    taken
}

#[cfg(feature = "gui")]
/// Takes a package in; tells the page how far it is (`package-progress`).
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn package_import(app: tauri::AppHandle, world: String, path: String) -> Answer<Summary> {
    #[cfg(feature = "gui")]
    use tauri::Emitter;
    let store = store(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (folder, package) = read(Path::new(&path))?;
        let tell = |done: usize, total: usize| {
            let _ = app.emit("package-progress", (done, total));
        };
        import(&store, &world, &folder, &package, &tell)
    })
    .await
    .map_err(AppError::from)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::template::TemplateField;

    struct World {
        dir: PathBuf,
        store: Store,
        gm: Identity,
        id: String,
    }

    impl Drop for World {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn world(tag: &str, gm: &str) -> World {
        let dir = std::env::temp_dir().join(format!("aw-transfer-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let gm = Identity::create(gm).unwrap();
        store.save_identity(&gm).unwrap();
        let meta = new_world(&store, &gm, "World".into()).unwrap();
        World { id: meta.id.clone(), dir, store, gm }
    }

    fn png() -> Vec<u8> {
        let img = image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([200, (x * 6) as u8, (y * 8) as u8]));
        let mut raw = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png).unwrap();
        raw
    }

    fn part(section: &str, text: &str, view: &str) -> NewLayer {
        NewLayer { title: String::new(), text: text.into(), policy: view.into(), level: if view == "gm" { "gm-only".into() } else { "open".into() }, section: section.into(), edit: "gm".into(), fields: serde_json::Value::Null, parent: None }
    }

    /// A real package into a fresh world, timed: `AW_PACKAGE=… cargo test
    /// --release a_real_package -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_real_package() {
        let Some(path) = std::env::var_os("AW_PACKAGE") else { return };
        let to = world("real", "Marla");
        let (folder, package) = read(Path::new(&path)).unwrap();
        let started = std::time::Instant::now();
        let last = std::cell::Cell::new(0);
        import(&to.store, &to.id, &folder, &package, &|done, total| {
            if done / 500 != last.get() / 500 || done == total {
                eprintln!("{done}/{total} after {:?}", started.elapsed());
            }
            last.set(done);
        })
        .unwrap();
        eprintln!("import: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let n = to.store.db(&to.id).unwrap().document_ids().unwrap().len();
        eprintln!("load: {:?} ({n})", started.elapsed());
    }

    /// A package into a new world of a real profile, as its owner — the app
    /// closed: `AW_PROFILE=… AW_WORLDS=… AW_PACKAGE=… AW_WORLD_NAME=… cargo
    /// test --release into_a_profile -- --ignored --nocapture`. With
    /// `AW_WORLD=world:…` into that world instead.
    #[test]
    #[ignore]
    fn into_a_profile() {
        let var = |name| std::env::var_os(name).map(std::path::PathBuf::from);
        let (Some(profile), Some(worlds), Some(path), Some(name)) = (var("AW_PROFILE"), var("AW_WORLDS"), var("AW_PACKAGE"), std::env::var("AW_WORLD_NAME").ok()) else { return };
        let store = Store::with_roots(profile, vec![worlds]);
        let me = store.load_identity().unwrap();
        let id = match std::env::var("AW_WORLD") {
            Ok(id) => id,
            Err(_) => new_world(&store, &me, name).unwrap().id.clone(),
        };
        let (folder, package) = read(&path).unwrap();
        let started = std::time::Instant::now();
        let done = import(&store, &id, &folder, &package, &|done, total| {
            if done % 1000 == 0 || done == total {
                eprintln!("{done}/{total} after {:?}", started.elapsed());
            }
        })
        .unwrap();
        eprintln!("{id}: {done:?} in {:?}", started.elapsed());
    }

    #[test]
    fn a_type_without_a_page_gets_one_and_keeps_it() {
        let to = world("pages", "Tim");
        let package: Package = serde_json::from_value(serde_json::json!({
            "format": "aw-package", "version": 1,
            "types": [{ "id": "quest", "name": "Quest" }, { "id": "stats", "name": "Stats", "group": true }],
        }))
        .unwrap();
        import(&to.store, &to.id, &to.dir, &package, &|_, _| {}).unwrap();
        let page_of = |id: &str| to.store.load_templates(&to.id).unwrap().into_iter().find(|t| t.id == id).unwrap().page;
        let page = page_of("quest");
        assert!(to.store.db(&to.id).unwrap().document_ids().unwrap().contains(&page), "the page is there");
        assert_eq!(page_of("stats"), "", "a group has none");
        import(&to.store, &to.id, &to.dir, &package, &|_, _| {}).unwrap();
        assert_eq!(page_of("quest"), page, "and keeps it");
    }

    #[test]
    fn the_package_page_becomes_the_world_page_while_that_is_empty() {
        let to = world("home", "Tim");
        let empty = to.store.load_world_meta(&to.id).unwrap().home;
        let page = "art:01J00000000000000000000001";
        let package: Package = serde_json::from_value(serde_json::json!({
            "format": "aw-package", "version": 1, "home": page,
            "documents": [{ "id": page, "type": "", "slug": "campaign", "name": "Campaign", "view": "all", "edit": "gm", "fields": {},
                "parts": [{ "section": "overview", "title": "", "text": "Welcome.", "fields": {}, "view": "all", "edit": "gm", "level": "open" }] }],
        }))
        .unwrap();
        import(&to.store, &to.id, &to.dir, &package, &|_, _| {}).unwrap();
        assert_eq!(to.store.load_world_meta(&to.id).unwrap().home, page);
        assert!(!to.store.db(&to.id).unwrap().document_ids().unwrap().contains(&empty), "the empty page went");
    }

    #[test]
    fn a_section_at_the_side_stays_there_in_the_editor() {
        let to = world("side", "Tim");
        let page = "art:01J00000000000000000000002";
        let package: Package = serde_json::from_value(serde_json::json!({
            "format": "aw-package", "version": 1,
            "documents": [{ "id": page, "type": "", "slug": "campaign", "name": "Campaign", "view": "all", "edit": "gm", "fields": {},
                "parts": [
                    { "section": "overview", "title": "", "text": "Welcome.", "fields": {}, "view": "all", "edit": "gm", "level": "open" },
                    { "section": "timeline", "title": "Timeline", "text": "<timeline>", "fields": { "shape": { "place": "side", "width": 380 } }, "view": "all", "edit": "gm", "level": "open" },
                ] }],
        }))
        .unwrap();
        import(&to.store, &to.id, &to.dir, &package, &|_, _| {}).unwrap();
        let opened = crate::document::code_of(&to.store, &to.id, page).unwrap();
        let side = opened.document.blocks.iter().find(|b| b.key == "timeline").unwrap();
        assert_eq!((side.place, side.width), (aw_model::template::BlockPlace::Side, 380));
        assert!(opened.code.contains("side width=380"), "{}", opened.code);
    }

    #[test]
    fn a_package_carries_types_documents_and_images_into_another_world() {
        let from = world("from", "Marla");
        let mut monster: Template = serde_json::from_value(serde_json::json!({ "id": "monster", "name": "Monster" })).unwrap();
        monster.properties = vec![TemplateField::new("cr", "CR", aw_model::FieldKind::Text { multiline: false })];
        monster.normalize();
        from.store.save_templates(&from.id, &[monster]).unwrap();
        let image = from.store.upload_image(&from.id, &png(), "Goblin", "").unwrap();
        let goblin = create_document(&from.store, &from.id, "monster".into(), "goblin".into(), "Goblin".into(), "all".into(), vec![
            part("story", &format!("Small and mean.\n<image src={image}>"), "all"),
            part("secret", "Works for the wizard in the tower.", "gm"),
        ])
        .unwrap();
        let mut meta = from.store.load_world_meta(&from.id).unwrap();
        meta.calendar.eras[0].months[0].name = "Hammer".into();
        from.store.save_world_meta(&meta).unwrap();
        let folder = from.dir.join("Goblins.awpack");
        let exported = export(&from.store, &from.id, &Choice { name: "Goblins".into(), documents_of: vec!["monster".into()], ..Choice::default() }, &folder, &|_, _, _| {}).unwrap();
        assert_eq!((exported.types_new, exported.documents_new, exported.images_new), (1, 1, 1));

        let to = world("to", "Tim");
        let (folder, package) = read(&folder.join("package.json")).unwrap();
        let before = summary(&to.store, &to.id, &package).unwrap();
        assert_eq!((before.types_new, before.documents_new, before.images_new), (1, 1, 1));
        import(&to.store, &to.id, &folder, &package, &|_, _| {}).unwrap();

        // The same document, the same access, the image under Tim's id.
        let view = view_with(&to.store, &to.id, &Reader::gm()).unwrap();
        let doc = view.iter().find(|a| a.id == goblin).expect("the document came");
        let story = doc.layers.iter().find(|l| l.section == "story").unwrap();
        let new_image = store_media::imported_id(&to.gm.id().to_string(), &image);
        assert!(story.text.contains(&new_image), "{}", story.text);
        assert_eq!(doc.layers.iter().find(|l| l.section == "secret").unwrap().policy, "gm");
        assert!(to.store.image_bytes(&to.id, &new_image, false).is_ok(), "the image opens");
        assert!(to.store.load_templates(&to.id).unwrap().iter().any(|t| t.id == "monster"));
        assert_eq!(to.store.load_world_meta(&to.id).unwrap().calendar, meta.calendar, "the calendar came along");
        let page = to.store.load_world_meta(&to.id).unwrap().home;
        assert_ne!(page, "", "the world keeps a page of its own");

        // Twice adds nothing.
        let again = summary(&to.store, &to.id, &package).unwrap();
        assert_eq!((again.documents_new, again.images_new, again.documents_known, again.images_known), (0, 0, 1, 1));
        import(&to.store, &to.id, &folder, &package, &|_, _| {}).unwrap();
        assert_eq!(to.store.db(&to.id).unwrap().document_ids().unwrap().iter().filter(|a| **a == goblin).count(), 1);
    }
}
