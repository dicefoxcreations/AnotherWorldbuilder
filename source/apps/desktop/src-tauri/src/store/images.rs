//! Images in a world: uploading, naming, opening, and the gallery.
//!
//! See `docs/architecture/images.md`. The original is kept as it came,
//! without its metadata, as an ordinary file in `<world>/media/`, beside a
//! small preview; what the gallery says of it stands in the world's database.

use super::*;
use aw_store::media::{self, FolderNode, ImageDefaults};
use std::collections::{BTreeMap, BTreeSet};

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
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub uploaded: u64,
    /// Whether it is this member's own upload.
    pub own: bool,
    /// The documents whose parts the reader sees use it now.
    pub used_in: Vec<UsedIn>,
    /// Named only in older versions of documents — for the game master.
    pub in_versions: bool,
    /// Whether its file is here.
    pub complete: bool,
    /// How it is put into a text.
    pub defaults: ImageDefaults,
    /// Its folder.
    pub folder: String,
}

impl Store {
    /// Takes an image in: its metadata out, the original and a preview as
    /// files, a gallery entry with its name. Returns the image's id.
    pub fn upload_image(&self, world_id: &str, raw: &[u8], name: &str, folder: &str) -> Result<String> {
        let me = self.actor()?;
        self.upload_image_as(world_id, &media::new_id(&me), raw, name, folder)
    }

    /// The same under an id chosen beforehand — see `media::imported_id`.
    pub fn upload_image_as(&self, world_id: &str, image: &str, raw: &[u8], name: &str, folder: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(err("Give the image a name."));
        }
        self.reader(world_id)?;
        let ready = media::prepare(raw).map_err(StoreError::from)?;
        let db = self.db(world_id)?;
        let dir = db.dir().join(aw_store::db::MEDIA);
        std::fs::create_dir_all(&dir)?;
        let stem = dir_name(image);
        let file = format!("{stem}.{}", ready.format.extension());
        aw_store::durable::write(&dir.join(&file), &ready.original)?;
        let preview = match &ready.preview {
            Some((bytes, format)) => {
                let name = format!("{stem}.preview.{}", format.extension());
                aw_store::durable::write(&dir.join(&name), bytes)?;
                name
            }
            None => String::new(),
        };
        db.put_image(&Image {
            id: image.to_string(),
            file,
            preview,
            name: name.to_string(),
            folder: folder.to_string(),
            mime: ready.format.mime().to_string(),
            width: ready.width,
            height: ready.height,
            size: ready.original.len() as u64,
            uploaded: now(),
            uploaded_by: self.actor()?,
            defaults: serde_json::to_value(ImageDefaults::default())?,
        })?;
        Ok(image.to_string())
    }

    /// Gives an image a new name — its uploader or the game master.
    pub fn rename_image(&self, world_id: &str, image: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(err("Give the image a name."));
        }
        self.change_image(world_id, image, |i| i.name = name.to_string())
    }

    /// Sets how an image is put into a text — its uploader or the game master.
    pub fn set_image_defaults(&self, world_id: &str, image: &str, defaults: ImageDefaults) -> Result<()> {
        if !defaults.hidden.is_empty() && !media::is_id(&defaults.hidden) {
            return Err(err("The stand-in has to be an image."));
        }
        let value = serde_json::to_value(defaults)?;
        self.change_image(world_id, image, |i| i.defaults = value)
    }

    /// Puts an image into a folder of the gallery, by the folder's id —
    /// its uploader or the game master. Empty: into none.
    pub fn move_image(&self, world_id: &str, image: &str, folder: &str) -> Result<()> {
        self.change_image(world_id, image, |i| i.folder = folder.to_string())
    }

    /// The folders of the gallery — the game master's; nobody else sorts.
    pub fn gallery_folders(&self, world_id: &str) -> Result<Vec<FolderNode>> {
        if !self.is_gm(world_id) {
            return Ok(Vec::new());
        }
        Ok(self.db(world_id)?.folders()?.into_iter().map(|f| FolderNode { id: f.id, name: f.name, parent: f.parent }).collect())
    }

    /// Sets the folders of the gallery, as a whole tree — the game master's.
    pub fn set_gallery_folders(&self, world_id: &str, folders: Vec<FolderNode>) -> Result<()> {
        if !self.is_gm(world_id) {
            return Err(err("Only the GM sorts the gallery."));
        }
        media::check_folders(&folders).map_err(StoreError::from)?;
        let rows: Vec<Folder> = folders.into_iter().map(|f| Folder { id: f.id, name: f.name, parent: f.parent }).collect();
        Ok(self.db(world_id)?.save_folders(&rows)?)
    }

    /// Changes an image's gallery entry — its uploader's or the game master's.
    fn change_image(&self, world_id: &str, image: &str, change: impl FnOnce(&mut Image)) -> Result<()> {
        let db = self.db(world_id)?;
        let mut found = db.image(image)?.ok_or_else(|| err("There is no such image."))?;
        if !self.is_gm(world_id) && found.uploaded_by != self.actor()? {
            return Err(err("You may not change this image."));
        }
        change(&mut found);
        Ok(db.put_image(&found)?)
    }

    /// Deletes images for good — the game master's.
    pub fn delete_images(&self, world_id: &str, images: &[String]) -> Result<()> {
        if !self.is_gm(world_id) {
            return Err(err("Only the GM deletes images."));
        }
        let db = self.db(world_id)?;
        for image in images {
            db.delete_image(image)?;
        }
        Ok(())
    }

    /// An image's bytes — the original or its preview — if the one this
    /// runs for may see it.
    pub fn image_bytes(&self, world_id: &str, image: &str, preview: bool) -> Result<(Vec<u8>, String)> {
        let found = self.db(world_id)?.image(image)?.ok_or_else(|| err("This image is not available here."))?;
        if !self.sees_image(world_id, &found)? {
            return Err(err("This image is not available here."));
        }
        let (file, mime) = if preview && !found.preview.is_empty() {
            let ext = found.preview.rsplit('.').next().unwrap_or_default();
            let mime = media::Format::of_extension(ext).map(|f| f.mime().to_string()).unwrap_or_else(|| found.mime.clone());
            (found.preview.clone(), mime)
        } else {
            (found.file.clone(), found.mime.clone())
        };
        let bytes = std::fs::read(self.world_dir(world_id).join(aw_store::db::MEDIA).join(file))?;
        Ok((bytes, mime))
    }

    /// Whether the one this runs for may see an image: the game master
    /// every one; everyone else their own uploads and the images of parts
    /// they may see — and a stand-in such an image names.
    fn sees_image(&self, world_id: &str, image: &Image) -> Result<bool> {
        let reader = self.reader(world_id)?;
        if reader.is_gm() || image.uploaded_by == self.actor()? {
            return Ok(true);
        }
        let (uses, _) = self.image_uses(world_id, &reader)?;
        if uses.contains_key(&image.id) {
            return Ok(true);
        }
        // A stand-in shows where the image it stands in for is used.
        let db = self.db(world_id)?;
        Ok(db.images()?.iter().any(|other| {
            uses.contains_key(&other.id) && other.defaults.get("hidden").and_then(|h| h.as_str()) == Some(image.id.as_str())
        }))
    }

    /// Where the images are used, as `reader` sees the documents — and the
    /// documents' names.
    fn image_uses(&self, world_id: &str, reader: &Reader) -> Result<(BTreeMap<String, BTreeSet<String>>, BTreeMap<String, String>)> {
        let mut uses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut names = BTreeMap::new();
        for doc in self.db(world_id)?.documents_for(reader)? {
            names.insert(doc.id.clone(), doc.name().to_string());
            for layer in &doc.layers {
                for image in &layer.images {
                    uses.entry(image.clone()).or_default().insert(doc.id.clone());
                }
            }
        }
        Ok((uses, names))
    }

    /// The gallery: as game master every image, otherwise one's own uploads
    /// and the images of parts one may see.
    pub fn gallery(&self, world_id: &str) -> Result<Vec<GalleryImage>> {
        let reader = self.reader(world_id)?;
        let me = self.actor()?;
        let db = self.db(world_id)?;
        let dir = db.dir().join(aw_store::db::MEDIA);
        let (uses, names) = self.image_uses(world_id, &reader)?;
        let in_versions: BTreeSet<String> = if reader.is_gm() {
            let mut all = BTreeSet::new();
            for doc in db.document_ids()? {
                for v in db.versions(&doc)? {
                    for l in v.content.layers.iter().chain(&v.content.before) {
                        all.extend(l.images.iter().cloned());
                    }
                }
            }
            all
        } else {
            BTreeSet::new()
        };
        let mut out = Vec::new();
        for image in db.images()? {
            let used: Vec<UsedIn> = uses
                .get(&image.id)
                .map(|set| set.iter().map(|a| UsedIn { id: a.clone(), name: names.get(a).cloned().unwrap_or_default() }).collect())
                .unwrap_or_default();
            let own = image.uploaded_by == me;
            if !reader.is_gm() && !own && used.is_empty() {
                continue;
            }
            out.push(GalleryImage {
                complete: dir.join(&image.file).exists(),
                in_versions: used.is_empty() && in_versions.contains(&image.id),
                used_in: used,
                own,
                defaults: serde_json::from_value(image.defaults.clone()).unwrap_or_default(),
                id: image.id,
                name: image.name,
                width: image.width,
                height: image.height,
                size: image.size,
                uploaded: image.uploaded,
                folder: image.folder,
            });
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.uploaded));
        Ok(out)
    }
}
