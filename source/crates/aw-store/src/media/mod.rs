//! Images: uploaded once, kept as they came, used wherever someone may.
//!
//! An image has three levels, like a document:
//!
//! - its **id** (`img:<author>-<ulid>`), which never changes and stands in
//!   the text (`<image src=img:…>`) or in an image field;
//! - its **name**, which can change and is searched in the gallery — sealed,
//!   since a name can give away what players should not know yet;
//! - its **files**: the original, cut into parts of at most 4 MiB, each
//!   encrypted with the image's own key, named by the hash of its bytes —
//!   or, in an unencrypted world, whole and as it is: an image file any
//!   viewer opens.
//!
//! The original stays as it came — no scaling, no new encoding. Only what an
//! image says about its maker goes: see `strip`. For the gallery a small
//! preview is made beside it.
//!
//! Who may see an image is decided where it is used: inserted into a part of
//! a document, its key goes into that part's content (`ImageRef`). Whoever
//! may read the part can open the image; nobody else.

pub mod strip;

use aw_crypto::{decrypt, encrypt, ContentKey, Identity, IdentityPublic};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::journal::{record_id, Sealed};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const IMAGE: &str = "anotherworldbuilder/image/1";
/// The largest part of an image file.
pub const PART: usize = 4 * 1024 * 1024;
/// The longer side of a gallery preview, in pixels.
pub const PREVIEW: u32 = 256;
/// More than this is not decoded for a preview — the image itself stays.
const DECODE_LIMIT: u64 = 768 * 1024 * 1024;

/// The kinds of images taken in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Jpeg,
    Png,
    Webp,
    Gif,
}

impl Format {
    /// What the bytes are, by their first bytes — not by a file name.
    pub fn of(raw: &[u8]) -> Option<Format> {
        if raw.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Format::Jpeg)
        } else if raw.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Format::Png)
        } else if raw.starts_with(b"GIF87a") || raw.starts_with(b"GIF89a") {
            Some(Format::Gif)
        } else if raw.len() >= 12 && &raw[..4] == b"RIFF" && &raw[8..12] == b"WEBP" {
            Some(Format::Webp)
        } else {
            None
        }
    }

    /// The usual ending of a file of this kind.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Webp => "webp",
            Format::Gif => "gif",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Png => "image/png",
            Format::Webp => "image/webp",
            Format::Gif => "image/gif",
        }
    }

    fn image(self) -> image::ImageFormat {
        match self {
            Format::Jpeg => image::ImageFormat::Jpeg,
            Format::Png => image::ImageFormat::Png,
            Format::Webp => image::ImageFormat::WebP,
            Format::Gif => image::ImageFormat::Gif,
        }
    }
}

/// An image made ready for the world: metadata gone, measured, previewed.
pub struct Prepared {
    pub format: Format,
    /// The original without its metadata — every pixel as it came.
    pub original: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// A small picture for the gallery, if the image could be decoded within
    /// bounds. A very large map may have none; it is no less an image.
    pub preview: Option<(Vec<u8>, Format)>,
}

/// Takes an uploaded file in: checks what it is, takes its metadata out,
/// measures it and makes a preview. The pixels are not touched.
pub fn prepare(raw: &[u8]) -> Result<Prepared> {
    let format = Format::of(raw).ok_or("Only PNG, JPEG, WebP and GIF images can be used.")?;
    let original = strip::strip(format, raw)?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&original), format.image());
    let (width, height) = reader.into_dimensions().map_err(|_| "This image could not be read.")?;
    reader = image::ImageReader::with_format(std::io::Cursor::new(&original), format.image());
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(DECODE_LIMIT);
    reader.limits(limits);
    let preview = reader.decode().ok().and_then(|img| {
        let mut img = img.thumbnail(PREVIEW, PREVIEW);
        if let Some(o) = image::metadata::Orientation::from_exif(strip::orientation_of(format, &original) as u8) {
            img.apply_orientation(o);
        }
        let (bytes, kind) = if img.color().has_alpha() {
            let mut out = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
            (out, Format::Png)
        } else {
            let mut out = Vec::new();
            let rgb = image::DynamicImage::ImageRgb8(img.to_rgb8());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82).encode_image(&rgb).ok()?;
            (out, Format::Jpeg)
        };
        Some((bytes, kind))
    });
    Ok(Prepared { format, original, width, height, preview })
}

/// Cuts bytes into parts and encrypts each for itself: (name, ciphertext).
/// Without a key — an unencrypted world — one part, the bytes as they are.
pub fn seal_parts(key: Option<&ContentKey>, bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let Some(key) = key else {
        return Ok(vec![(record_id(bytes), bytes.to_vec())]);
    };
    let mut out = Vec::new();
    for chunk in bytes.chunks(PART) {
        let sealed = encrypt(key, chunk)?;
        out.push((record_id(&sealed), sealed));
    }
    Ok(out)
}

/// Puts decrypted parts back together.
pub fn open_parts(key: Option<&ContentKey>, parts: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for part in parts {
        match key {
            Some(key) => out.extend(decrypt(key, part)?),
            None => out.extend_from_slice(part),
        }
    }
    Ok(out)
}

/// A new image id, carrying a mark of its author — so only they and the game
/// master write about it.
pub fn new_id(author: &str) -> String {
    format!("img:{}-{}", author_mark(author), ulid::Ulid::generate())
}

/// The id an image gets when `author` takes it in from a package: the same
/// for the same original, so taking a package in twice keeps one image.
pub fn imported_id(author: &str, original: &str) -> String {
    if id_by(original, author) {
        return original.to_string();
    }
    let hash = blake3::hash(format!("anotherworldbuilder/image-import/1\0{original}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    format!("img:{}-{}", author_mark(author), ulid::Ulid::from_bytes(bytes))
}

fn author_mark(author: &str) -> String {
    blake3::hash(format!("anotherworldbuilder/image-author/1\0{author}").as_bytes()).to_hex()[..12].to_string()
}

/// Whether an id is an image id, and written by `author`.
pub fn id_by(id: &str, author: &str) -> bool {
    id.strip_prefix("img:").and_then(|rest| rest.split_once('-')).is_some_and(|(mark, _)| mark == author_mark(author))
}

/// Whether a string has the shape of an image id.
pub fn is_id(id: &str) -> bool {
    id.strip_prefix("img:").and_then(|rest| rest.split_once('-')).is_some_and(|(mark, ulid)| {
        mark.len() == 12 && mark.bytes().all(|b| b.is_ascii_hexdigit()) && ulid.parse::<ulid::Ulid>().is_ok()
    })
}

/// What a part of a document needs to show an image: where its bytes lie
/// and the key. Kept in the part's content, encrypted with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    pub format: Format,
    pub parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_format: Option<Format>,
    /// The image's key, hex — empty where it is kept as it is.
    pub key: String,
}

impl ImageRef {
    pub fn key(&self) -> Result<Option<ContentKey>> {
        if self.key.is_empty() {
            return Ok(None);
        }
        let raw: [u8; 32] = hex::decode(&self.key)?.try_into().map_err(|_| "a key has 32 bytes")?;
        Ok(Some(ContentKey::from_bytes(raw)))
    }
}

/// How an image is put into a text unless one says otherwise there: who
/// may see and change it, and what the others see in its place. Empty means
/// nothing of its own — the image takes the access of where it stands.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageDefaults {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub view: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub edit: String,
    /// The stand-in image, `img:…`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hidden: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hidden_caption: String,
}

/// What only the game master and the uploader learn of an image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageMeta {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Bytes of the original.
    pub size: u64,
    /// Milliseconds since 1970, when it was uploaded.
    pub uploaded: u64,
    /// Everything needed to open it.
    pub image: ImageRef,
    #[serde(default)]
    pub defaults: ImageDefaults,
    /// The folder it lies in, by id (`FolderNode`). Empty, or a folder that
    /// is gone: in none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

/// A folder of the gallery. Folders hang in a tree by `parent`; renaming or
/// moving one changes nothing in the images that lie in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolderNode {
    pub id: String,
    pub name: String,
    /// Empty: at the top.
    #[serde(default)]
    pub parent: String,
}

/// Whether a tree of folders holds: ids once each, names given, every
/// parent there, no folder inside itself.
pub fn check_folders(folders: &[FolderNode]) -> Result<()> {
    let ids: std::collections::HashSet<&str> = folders.iter().map(|f| f.id.as_str()).collect();
    if ids.len() != folders.len() || folders.iter().any(|f| f.id.is_empty()) {
        return Err("Every folder needs an id of its own.".into());
    }
    for f in folders {
        if f.name.trim().is_empty() {
            return Err("Every folder needs a name.".into());
        }
        let mut seen = std::collections::HashSet::new();
        let mut at = f.parent.as_str();
        while !at.is_empty() {
            if !ids.contains(at) {
                return Err("A folder lies in a folder that is not there.".into());
            }
            if !seen.insert(at) || at == f.id {
                return Err("A folder cannot lie inside itself.".into());
            }
            at = folders.iter().find(|p| p.id == at).map(|p| p.parent.as_str()).unwrap_or("");
        }
    }
    Ok(())
}

/// An image in the gallery: its files, and — sealed for the game master and
/// the uploader — its name and key. Renaming writes a new entry; the newest
/// counts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageEntry {
    pub world: String,
    pub image: String,
    pub author: IdentityPublic,
    /// Milliseconds since 1970, by the author's clock.
    pub time: u64,
    /// The parts of the original and of the preview — needed by every device,
    /// so they stand in the open.
    pub parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<String>,
    /// `ImageMeta`.
    pub sealed: Sealed,
    pub signature: String,
}

impl ImageEntry {
    fn signed_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(IMAGE, &self.world, &self.image, &self.author, self.time, &self.parts, &self.preview, &self.sealed))?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &str,
        image: &str,
        author: &Identity,
        gm: &IdentityPublic,
        uploader: &IdentityPublic,
        meta: &ImageMeta,
        time: u64,
        encrypted: bool,
    ) -> Result<Self> {
        let mut recipients = vec![gm];
        if uploader.id != gm.id {
            recipients.push(uploader);
        }
        let bytes = serde_json::to_vec(meta)?;
        let mut entry = ImageEntry {
            world: world.to_string(),
            image: image.to_string(),
            author: author.public(),
            time,
            parts: meta.image.parts.clone(),
            preview: meta.image.preview.clone(),
            sealed: if encrypted { Sealed::new(&recipients, &bytes)? } else { Sealed::plain(&bytes) },
            signature: String::new(),
        };
        entry.signature = hex::encode(author.sign(&entry.signed_bytes()?));
        Ok(entry)
    }

    /// Checks the signature, and that whoever wrote it may: the uploader —
    /// whose mark the id carries — or the game master.
    pub fn verify(&self, world: &str, gm: Option<&IdentityPublic>) -> Result<()> {
        if self.world != world {
            return Err("a record of another world".into());
        }
        if !self.author.is_genuine() {
            return Err("an author whose id does not match their keys".into());
        }
        let signature: [u8; 64] = hex::decode(&self.signature)?.try_into().map_err(|_| "a signature has 64 bytes")?;
        self.author.verify(&self.signed_bytes()?, &signature)?;
        let by_gm = gm.is_some_and(|g| g.id == self.author.id);
        if !by_gm && !id_by(&self.image, self.author.id.as_str()) {
            return Err("only its uploader or the game master writes about an image".into());
        }
        if self.parts.is_empty() || self.parts.iter().chain(&self.preview).any(|p| !is_part(p)) {
            return Err("an image without proper parts".into());
        }
        Ok(())
    }

    /// What the entry holds, for whoever it is sealed for.
    pub fn meta(&self, secret: &x25519_dalek::StaticSecret) -> Option<ImageMeta> {
        serde_json::from_slice(&self.sealed.open(secret)?).ok()
    }
}

fn is_part(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

// --- Parts on disk ----------------------------------------------------------------

/// Where the parts of a world's images lie.
pub fn parts_dir(world_dir: &Path) -> PathBuf {
    world_dir.join("media").join("parts")
}

fn part_path(world_dir: &Path, name: &str) -> Option<PathBuf> {
    is_part(name).then(|| parts_dir(world_dir).join(format!("{name}.bin")))
}

/// Keeps many parts at once, as `put_part` each — synced together.
pub fn put_parts(world_dir: &Path, parts: &[(String, Vec<u8>)]) -> Result<()> {
    let mut files = Vec::with_capacity(parts.len());
    for (name, bytes) in parts {
        let path = part_path(world_dir, name).ok_or("not a part's name")?;
        if record_id(bytes) != *name {
            return Err("a part whose bytes do not match its name".into());
        }
        files.push((path, bytes.clone()));
    }
    crate::durable::write_all(&files)?;
    Ok(())
}

pub fn has_part(world_dir: &Path, name: &str) -> bool {
    part_path(world_dir, name).is_some_and(|p| p.exists())
}

/// Keeps a part — only if its bytes are what its name says.
pub fn put_part(world_dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let path = part_path(world_dir, name).ok_or("not a part's name")?;
    if record_id(bytes) != name {
        return Err("a part whose bytes do not match its name".into());
    }
    if !path.exists() {
        crate::durable::write(&path, bytes)?;
    }
    Ok(())
}

pub fn read_part(world_dir: &Path, name: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(part_path(world_dir, name)?).ok()?;
    (record_id(&bytes) == name).then_some(bytes)
}

pub fn remove_part(world_dir: &Path, name: &str) {
    if let Some(path) = part_path(world_dir, name) {
        let _ = std::fs::remove_file(path);
    }
}

/// The parts that lie here, by name.
pub fn parts_here(world_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(parts_dir(world_dir)) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".bin")).map(str::to_string))
        .filter(|n| is_part(n))
        .collect()
}

/// An image, put back together from the parts here.
pub fn open_image(world_dir: &Path, image: &ImageRef, preview: bool) -> Result<Vec<u8>> {
    let names = if preview && !image.preview.is_empty() { &image.preview } else { &image.parts };
    let mut parts = Vec::new();
    for name in names {
        parts.push(read_part(world_dir, name).ok_or("a part of this image is not here yet")?);
    }
    open_parts(image.key()?.as_ref(), &parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn an_upload_keeps_its_pixels_and_gets_a_small_preview() {
        let raw = png(1200, 600);
        let ready = prepare(&raw).unwrap();
        assert_eq!((ready.format, ready.width, ready.height), (Format::Png, 1200, 600));
        let back = image::load_from_memory(&ready.original).unwrap();
        assert_eq!(back.to_rgba8(), image::load_from_memory(&raw).unwrap().to_rgba8(), "every pixel as it came");
        let (preview, _) = ready.preview.unwrap();
        let small = image::load_from_memory(&preview).unwrap();
        assert_eq!((small.width(), small.height()), (256, 128));
    }

    #[test]
    fn a_real_jpeg_with_a_location_in_it_decodes_the_same_without() {
        let img = image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 120]));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&img).unwrap();
        // A phone's EXIF block with a place in it, right after the start.
        let mut exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x25\x88\x04\0\x01\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        exif.extend_from_slice(b"GPS 52.5200N 13.4050E Marla's flat");
        let mut raw = jpeg[..2].to_vec();
        raw.extend_from_slice(&[0xFF, 0xE1]);
        raw.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        raw.extend(exif);
        raw.extend_from_slice(&jpeg[2..]);
        let ready = prepare(&raw).unwrap();
        assert!(!String::from_utf8_lossy(&ready.original).contains("GPS"));
        assert_eq!(image::load_from_memory(&ready.original).unwrap().to_rgb8(), image::load_from_memory(&jpeg).unwrap().to_rgb8());
        assert!(ready.preview.is_some());
    }

    #[test]
    fn parts_go_back_together_and_only_with_the_key() {
        let key = ContentKey::generate().unwrap();
        let bytes: Vec<u8> = (0..(PART * 2 + 17)).map(|i| (i % 251) as u8).collect();
        let parts = seal_parts(Some(&key), &bytes).unwrap();
        assert_eq!(parts.len(), 3);
        let raw: Vec<Vec<u8>> = parts.iter().map(|(_, b)| b.clone()).collect();
        assert_eq!(open_parts(Some(&key), &raw).unwrap(), bytes);
        assert!(open_parts(Some(&ContentKey::generate().unwrap()), &raw).is_err());
    }

    #[test]
    fn an_image_id_names_its_author() {
        let id = new_id("id:abc");
        assert!(is_id(&id) && id_by(&id, "id:abc") && !id_by(&id, "id:def"));
        assert!(!is_id("img:../x") && !is_id("art:01"));
    }

    #[test]
    fn what_is_no_image_is_refused() {
        assert!(prepare(b"<svg onload=alert(1)>").is_err());
    }
}
