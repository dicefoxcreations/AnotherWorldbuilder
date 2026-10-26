//! Images: uploaded once, kept as they came, used wherever someone may.
//!
//! An image has an **id** (`img:<author>-<ulid>`), which never changes and
//! stands in the text (`<image src=img:…>`) or in an image field; a
//! **name**, which can change and is searched in the gallery; and its
//! **file**, the original as it came — no scaling, no new encoding — with a
//! small preview beside it for the gallery. Only what an image says about
//! its maker goes: see `strip`.
//!
//! Who may see an image is decided where it is used: whoever may read a
//! part that shows it may see it.

pub mod strip;

use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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

    /// The kind a file's ending names.
    pub fn of_extension(extension: &str) -> Option<Format> {
        [Format::Jpeg, Format::Png, Format::Webp, Format::Gif].into_iter().find(|f| f.extension().eq_ignore_ascii_case(extension))
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
