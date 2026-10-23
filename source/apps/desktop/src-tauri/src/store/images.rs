//! Images in a world: uploading, naming, opening, and the gallery.
//!
//! See `aw_store::media` and `docs/architecture/images.md`. The original is
//! kept as it came, without its metadata; its parts lie in
//! `<world>/media/parts/`, its gallery entry in `<world>/media/`.

use super::*;
use aw_store::media::{self, FolderNode, ImageDefaults, ImageEntry, ImageMeta, ImageRef};
use std::collections::{BTreeMap, BTreeSet};

/// Image id → the documents whose readable parts show it.
type UsesOf = BTreeMap<String, BTreeSet<ArticleId>>;

/// A document an image stands in.
#[derive(Clone, Debug, Serialize)]
pub struct UsedIn {
    pub id: String,
    pub name: String,
}

/// An image as the gallery shows it.
#[derive(Clone, Debug, Serialize)]
pub struct GalleryImage {
    pub id: String,
    /// Empty where this device may not learn it — someone else's upload.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub uploaded: u64,
    /// Whether it is this device's own upload.
    pub own: bool,
    /// The documents whose open parts use it now.
    pub used_in: Vec<UsedIn>,
    /// Named only in older versions of documents — for the game master.
    pub in_versions: bool,
    /// Whether every part is here yet.
    pub complete: bool,
    /// How it is put into a text — for whoever may learn its name.
    pub defaults: ImageDefaults,
    /// Its folder — for whoever may learn its name.
    pub folder: String,
}



/// The image ids a text or a field names.
pub fn named_images(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("img:") {
        let tail = &rest[at..];
        let end = tail.find(|c: char| !(c.is_ascii_alphanumeric() || c == ':' || c == '-')).unwrap_or(tail.len());
        if media::is_id(&tail[..end]) {
            out.insert(tail[..end].to_string());
        }
        rest = &tail[4..];
    }
    out
}

impl Store {
    /// Takes an image in: its metadata out, cut into encrypted parts, a
    /// gallery entry with its name. Returns the image's id.
    pub fn upload_image(&self, world_id: &str, raw: &[u8], name: &str, folder: &str, me: &Identity) -> Result<String> {
        self.upload_image_as(world_id, &media::new_id(me.id().as_str()), raw, name, folder, me)
    }

    /// The same under an id chosen beforehand — one of `me`'s, see
    /// `media::imported_id`.
    pub fn upload_image_as(&self, world_id: &str, image: &str, raw: &[u8], name: &str, folder: &str, me: &Identity) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Journal("Give the image a name.".into()));
        }
        let ready = media::prepare(raw).map_err(StoreError::from)?;
        // Unencrypted, the image lies there whole, as it is.
        let key = if self.is_encrypted(world_id)? { Some(ContentKey::generate()?) } else { None };
        let dir = self.world_dir(world_id);
        let mut parts = Vec::new();
        for (part, bytes) in media::seal_parts(key.as_ref(), &ready.original).map_err(StoreError::from)? {
            media::put_part(&dir, &part, &bytes).map_err(StoreError::from)?;
            parts.push(part);
        }
        let mut preview = Vec::new();
        let mut preview_format = None;
        if let Some((bytes, format)) = &ready.preview {
            for (part, sealed) in media::seal_parts(key.as_ref(), bytes).map_err(StoreError::from)? {
                media::put_part(&dir, &part, &sealed).map_err(StoreError::from)?;
                preview.push(part);
            }
            preview_format = Some(*format);
        }
        let image = image.to_string();
        let meta = ImageMeta {
            name: name.to_string(),
            width: ready.width,
            height: ready.height,
            size: ready.original.len() as u64,
            uploaded: now(),
            image: ImageRef { format: ready.format, parts, preview, preview_format, key: key.as_ref().map(|k| hex::encode(k.as_bytes())).unwrap_or_default() },
            defaults: ImageDefaults::default(),
            folder: folder.to_string(),
        };
        self.with_journal(world_id, |journal| {
            let gm = journal.game_master().ok_or_else(|| StoreError::Journal("The world names no game master.".into()))?;
            let entry = ImageEntry::new(world_id, &image, me, &gm, &me.public(), &meta, now(), journal.encrypted()).map_err(StoreError::from)?;
            journal.put_image(entry)?;
            Ok(())
        })?;
        Ok(image)
    }

    /// Every image into the form the world is in now — encrypted with a key
    /// of its own, or whole and as it is. Returns the new references, by
    /// image. An image whose parts are not here stays as it is.
    pub(super) fn convert_images(&self, world_id: &str, encrypted: bool) -> Result<HashMap<String, ImageRef>> {
        // So many at a time: their parts written and synced together, their
        // entries into the journal at once.
        const BATCH: usize = 64;
        let me = self.load_identity()?;
        let dir = self.world_dir(world_id);
        let (images, gm, uploaders) = self.with_journal(world_id, |journal| {
            let gm = journal.game_master().ok_or_else(|| StoreError::Journal("The world names no game master.".into()))?;
            let images: Vec<(String, ImageMeta)> = journal
                .images()
                .into_iter()
                .filter_map(|(image, ..)| journal.image_meta(&image).map(|meta| (image, meta)))
                .filter(|(_, meta)| meta.image.key.is_empty() == encrypted)
                .collect();
            let uploaders: HashMap<String, IdentityPublic> =
                images.iter().filter_map(|(image, _)| journal.uploader(image).map(|u| (image.clone(), u))).collect();
            Ok((images, gm, uploaders))
        })?;
        let mut refs = HashMap::new();
        for (n, batch) in images.chunks(BATCH).enumerate() {
            super::progress(world_id, 2, "Images", n * BATCH, images.len());
            let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
            let mut entries = Vec::new();
            for (image, meta) in batch {
                let Ok(original) = media::open_image(&dir, &meta.image, false) else { continue };
                let preview = if meta.image.preview.is_empty() { None } else { media::open_image(&dir, &meta.image, true).ok() };
                let key = if encrypted { Some(ContentKey::generate()?) } else { None };
                let mut put = |bytes: &[u8]| -> Result<Vec<String>> {
                    let sealed = media::seal_parts(key.as_ref(), bytes).map_err(StoreError::from)?;
                    let names = sealed.iter().map(|(n, _)| n.clone()).collect();
                    parts.extend(sealed);
                    Ok(names)
                };
                let new_parts = put(&original)?;
                let new_preview = match &preview {
                    Some(bytes) => put(bytes)?,
                    None => Vec::new(),
                };
                let mut meta = meta.clone();
                meta.image = ImageRef {
                    format: meta.image.format,
                    parts: new_parts,
                    preview: new_preview,
                    preview_format: meta.image.preview_format,
                    key: key.as_ref().map(|k| hex::encode(k.as_bytes())).unwrap_or_default(),
                };
                let uploader = uploaders.get(image).cloned().unwrap_or_else(|| me.public());
                entries.push(ImageEntry::new(world_id, image, &me, &gm, &uploader, &meta, now(), encrypted).map_err(StoreError::from)?);
                refs.insert(image.clone(), meta.image);
            }
            media::put_parts(&dir, &parts).map_err(StoreError::from)?;
            self.with_journal(world_id, |journal| {
                journal.put_images(entries)?;
                Ok(())
            })?;
        }
        Ok(refs)
    }

    /// What images left behind in the other form: older gallery entries and
    /// their parts — gone once every part names the new ones. Plain parts
    /// must not stay in a world encrypted again.
    pub(super) fn left_in_other_form(&self, world_id: &str, encrypted: bool) -> Result<Vec<String>> {
        self.with_journal(world_id, |journal| {
            let newest: BTreeSet<String> = journal.images().into_iter().map(|(_, id, _)| id).collect();
            let mut out = Vec::new();
            for (id, entry) in journal.image_records() {
                if newest.contains(id) {
                    continue;
                }
                let Some(meta) = journal.entry_meta(entry) else { continue };
                if meta.image.key.is_empty() == encrypted {
                    out.push(id.clone());
                    out.extend(entry.parts.iter().chain(&entry.preview).cloned());
                }
            }
            Ok(out)
        })
    }

    /// Gives an image a new name — its uploader or the game master.
    pub fn rename_image(&self, world_id: &str, image: &str, name: &str, me: &Identity) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Journal("Give the image a name.".into()));
        }
        self.change_image(world_id, image, me, |meta| meta.name = name.to_string())
    }

    /// Sets how an image is put into a text — its uploader or the game master.
    pub fn set_image_defaults(&self, world_id: &str, image: &str, defaults: ImageDefaults, me: &Identity) -> Result<()> {
        if !defaults.hidden.is_empty() && !media::is_id(&defaults.hidden) {
            return Err(StoreError::Journal("The stand-in has to be an image.".into()));
        }
        self.change_image(world_id, image, me, |meta| meta.defaults = defaults)
    }

    /// Puts an image into a folder of the gallery, by the folder's id —
    /// its uploader or the game master. Empty: into none.
    pub fn move_image(&self, world_id: &str, image: &str, folder: &str, me: &Identity) -> Result<()> {
        self.change_image(world_id, image, me, |meta| meta.folder = folder.to_string())
    }

    /// The folders of the gallery — the game master's; nobody else sorts.
    pub fn gallery_folders(&self, world_id: &str) -> Result<Vec<FolderNode>> {
        if !self.is_gm(world_id) {
            return Ok(Vec::new());
        }
        self.with_journal(world_id, |journal| Ok(journal.gallery_folders()))
    }

    /// Sets the folders of the gallery, as a whole tree — the game master's.
    pub fn set_gallery_folders(&self, world_id: &str, folders: Vec<FolderNode>) -> Result<()> {
        if !self.is_gm(world_id) {
            return Err(StoreError::Journal("Only the GM sorts the gallery.".into()));
        }
        media::check_folders(&folders).map_err(StoreError::from)?;
        if self.gallery_folders(world_id)? == folders {
            return Ok(());
        }
        self.seal_gallery_folders(world_id, &folders)
    }

    /// The folders sealed for the game master — or as they are, unencrypted.
    pub(super) fn seal_gallery_folders(&self, world_id: &str, folders: &[FolderNode]) -> Result<()> {
        let bytes = serde_json::to_vec(folders)?;
        let sealed = if self.is_encrypted(world_id)? { Sealed::new(&[&self.load_identity()?.public()], &bytes)? } else { Sealed::plain(&bytes) };
        self.control(world_id, Item::Gallery { sealed }, "")
    }

    /// Writes a new gallery entry for an image, changed by `change`.
    fn change_image(&self, world_id: &str, image: &str, me: &Identity, change: impl FnOnce(&mut ImageMeta)) -> Result<()> {
        self.with_journal(world_id, |journal| {
            let mut meta = journal.image_meta(image).ok_or_else(|| StoreError::Journal("You may not change this image.".into()))?;
            let before = meta.clone();
            change(&mut meta);
            if meta == before {
                return Ok(());
            }
            let gm = journal.game_master().ok_or_else(|| StoreError::Journal("The world names no game master.".into()))?;
            let uploader = journal.uploader(image).unwrap_or_else(|| me.public());
            let entry = ImageEntry::new(world_id, image, me, &gm, &uploader, &meta, now(), journal.encrypted()).map_err(StoreError::from)?;
            journal.put_image(entry)?;
            Ok(())
        })
    }

    /// Deletes images for good, on every device — the game master's.
    pub fn delete_images(&self, world_id: &str, images: &[String]) -> Result<()> {
        if !self.is_gm(world_id) {
            return Err(StoreError::Journal("Only the GM deletes images.".into()));
        }
        let records = self.with_journal(world_id, |journal| {
            let mut out = Vec::new();
            let all = journal.images();
            for image in images {
                for (id, _, entry) in all.iter().filter(|(i, ..)| i == image) {
                    let _ = id;
                    out.extend(entry.parts.iter().chain(&entry.preview).cloned());
                }
                out.extend(journal.image_entries(image));
            }
            Ok(out)
        })?;
        if records.is_empty() {
            return Ok(());
        }
        self.control(world_id, Item::Deleted { records }, "")
    }

    /// An image's bytes — the original or its preview — if this device may
    /// open it.
    pub fn image_bytes(&self, world_id: &str, image: &str, preview: bool) -> Result<(Vec<u8>, &'static str)> {
        // In a member's name: only an image a part they may read shows.
        if crate::host::acting().is_some() && !self.image_uses(world_id)?.0.contains_key(image) {
            return Err(StoreError::Journal("This image is not available here.".into()));
        }
        let found = self.with_journal(world_id, |journal| Ok(journal.image_ref(image)))?;
        let found = found.ok_or_else(|| StoreError::Journal("This image is not available here.".into()))?;
        let names = if preview && !found.preview.is_empty() { &found.preview } else { &found.parts };
        let mut parts = Vec::new();
        for name in names {
            parts.push(self.part(world_id, name).ok_or_else(|| StoreError::Journal("This image is not available offline.".into()))?);
        }
        let bytes = media::open_parts(found.key().map_err(StoreError::from)?.as_ref(), &parts).map_err(StoreError::from)?;
        let mime = match (preview && !found.preview.is_empty(), found.preview_format) {
            (true, Some(format)) => format.mime(),
            _ => found.format.mime(),
        };
        Ok((bytes, mime))
    }

    /// Where the images are used, and the documents' names — read from the
    /// articles as built (kept between starts), each part opened with this
    /// device's keys; and kept until the world changes, since the gallery
    /// asks again after every move.
    fn image_uses(&self, world_id: &str) -> Result<(Arc<UsesOf>, Arc<BTreeMap<String, String>>)> {
        type Seen = HashMap<(PathBuf, String), (u64, Arc<UsesOf>, Arc<BTreeMap<String, String>>)>;
        static SEEN: OnceLock<Mutex<Seen>> = OnceLock::new();
        // Acting for a member, with their keys — and kept apart from this device's own.
        let acting = crate::host::acting();
        let who = acting.as_ref().map(|a| a.member.id.to_string()).unwrap_or_default();
        let key = (self.profile_dir().join(who), world_id.to_string());
        let articles = self.load_articles(world_id)?;
        let (version, held) = self.with_journal(world_id, |journal| {
            let held = match &acting {
                Some(a) => a.held.clone(),
                None => self.device_keys(world_id, journal)?,
            };
            Ok((journal.version(), held))
        })?;
        if let Some((v, uses, names)) = SEEN.get_or_init(Default::default).lock().ok().and_then(|s| s.get(&key).cloned()) {
            if v == version {
                return Ok((uses, names));
            }
        }
        let mut uses: UsesOf = BTreeMap::new();
        let mut names = BTreeMap::new();
        for article in &articles {
            for layer in article.layers.iter().filter(|l| !l.sealed.is_empty()) {
                let Some(content) = layer.content(&held) else { continue };
                if layer.is_base {
                    names.insert(article.id.to_string(), content.title.clone());
                }
                for image in content.images {
                    uses.entry(image).or_default().insert(article.id);
                }
            }
        }
        let (uses, names) = (Arc::new(uses), Arc::new(names));
        if let Ok(mut seen) = SEEN.get_or_init(Default::default).lock() {
            seen.insert(key, (version, uses.clone(), names.clone()));
        }
        Ok((uses, names))
    }

    /// The gallery: as game master every image, otherwise one's own uploads
    /// and the images of parts one may read.
    pub fn gallery(&self, world_id: &str, me: &Identity) -> Result<Vec<GalleryImage>> {
        let gm = self.is_gm(world_id);
        let dir = self.world_dir(world_id);
        let (uses, names) = self.image_uses(world_id)?;
        self.with_journal(world_id, |journal| {
            let in_versions = if gm { journal.images_in_versions() } else { BTreeSet::new() };
            let mut out = Vec::new();
            for (image, _, entry) in journal.images() {
                let meta = journal.entry_meta(entry);
                let used: Vec<UsedIn> = uses
                    .get(&image)
                    .map(|set| {
                        set.iter()
                            .map(|a| UsedIn { id: a.to_string(), name: names.get(&a.to_string()).cloned().unwrap_or_default() })
                            .collect()
                    })
                    .unwrap_or_default();
                if !gm && meta.is_none() && used.is_empty() {
                    continue;
                }
                let complete = entry.parts.iter().chain(&entry.preview).all(|p| media::has_part(&dir, p));
                out.push(GalleryImage {
                    id: image.clone(),
                    name: meta.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
                    width: meta.as_ref().map(|m| m.width).unwrap_or(0),
                    height: meta.as_ref().map(|m| m.height).unwrap_or(0),
                    size: meta.as_ref().map(|m| m.size).unwrap_or(0),
                    uploaded: meta.as_ref().map(|m| m.uploaded).unwrap_or(entry.time),
                    own: media::id_by(&image, me.id().as_str()),
                    in_versions: used.is_empty() && in_versions.contains(&image),
                    used_in: used,
                    complete,
                    defaults: meta.as_ref().map(|m| m.defaults.clone()).unwrap_or_default(),
                    folder: meta.map(|m| m.folder).unwrap_or_default(),
                });
            }
            out.sort_by_key(|a| std::cmp::Reverse(a.uploaded));
            Ok(out)
        })
    }

    /// Fills in what a part's content needs for the images its text and
    /// fields name — keeping what it had, finding the rest where this device
    /// may. Images it no longer names leave it. An image this device cannot
    /// open cannot be used here.
    pub fn with_images(&self, world_id: &str, seen: &Plain, wanted: &Plain) -> Result<Plain> {
        let mut named = named_images(&wanted.text);
        for value in wanted.fields.values() {
            named.extend(named_images(&value.to_string()));
        }
        let mut out = wanted.clone();
        out.images.clear();
        for image in named {
            let found = match seen.images.get(&image).or_else(|| wanted.images.get(&image)) {
                Some(found) => Some(found.clone()),
                None => self.with_journal(world_id, |journal| Ok(journal.image_ref(&image)))?,
            };
            let found = found.ok_or_else(|| StoreError::Journal("An image in this text is not available to you.".into()))?;
            out.images.insert(image, found);
        }
        Ok(out)
    }

    /// A part, from the world's folder.
    fn part(&self, world_id: &str, name: &str) -> Option<Vec<u8>> {
        media::read_part(&self.world_dir(world_id), name)
    }

    /// The parts of the images this device knows.
    pub(crate) fn wanted_parts(&self, world_id: &str) -> Result<BTreeSet<String>> {
        self.with_journal(world_id, |journal| Ok(journal.needed_parts()))
    }

    /// The parts this device lacks of the images it knows.
    pub fn missing_parts(&self, world_id: &str) -> Result<Vec<String>> {
        if !self.holds(world_id) {
            return Ok(Vec::new());
        }
        let dir = self.world_dir(world_id);
        let needed = self.wanted_parts(world_id)?;
        Ok(needed.into_iter().filter(|p| !media::has_part(&dir, p)).collect())
    }

    /// The parts asked for that lie here: (name, bytes).
    pub fn parts(&self, world_id: &str, names: &[String]) -> Vec<(String, Vec<u8>)> {
        let dir = self.world_dir(world_id);
        names.iter().filter_map(|n| media::read_part(&dir, n).map(|b| (n.clone(), b))).collect()
    }

    /// Keeps parts from elsewhere — only those an image here needs, and only
    /// if their bytes are what their names say. One lying here that is no
    /// longer whole is replaced.
    pub fn take_parts(&self, world_id: &str, parts: &[(String, Vec<u8>)]) -> Result<usize> {
        let needed = self.wanted_parts(world_id)?;
        let dir = self.world_dir(world_id);
        let mut new = 0;
        for (name, bytes) in parts {
            let whole = media::has_part(&dir, name) && media::read_part(&dir, name).is_some();
            if needed.contains(name) && !whole && media::put_part(&dir, name, bytes).is_ok() {
                new += 1;
            }
        }
        Ok(new)
    }
}
