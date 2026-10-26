//! Images, for the interface: uploading, the gallery, and their bytes. See
//! `store::images` and `docs/architecture/images.md`.

use super::*;
use crate::store::GalleryImage;

/// The largest file taken in — a detailed map may be big, but not without end.
const MAX_UPLOAD: u64 = 1024 * 1024 * 1024;

#[cfg(feature = "gui")]
/// Uploads an image under a name; returns its id.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_upload(app: tauri::AppHandle, world: String, path: String, name: String, folder: String) -> Answer<String> {
    let store = store(&app)?;
    if std::fs::metadata(&path)?.len() > MAX_UPLOAD {
        return Err(AppError("This file is too large.".into()));
    }
    let raw = std::fs::read(&path)?;
    Ok(store.upload_image(&world, &raw, &name, &folder)?)
}

#[cfg(feature = "gui")]
/// Gives an image a new name — its uploader or the GM.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_rename(app: tauri::AppHandle, world: String, image: String, name: String) -> Answer<()> {
    let store = store(&app)?;
    Ok(store.rename_image(&world, &image, &name)?)
}

#[cfg(feature = "gui")]
/// Sets how an image is put into a text: who sees it, who changes it, and
/// what the others see in its place.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_defaults_set(app: tauri::AppHandle, world: String, image: String, defaults: aw_store::media::ImageDefaults) -> Answer<()> {
    let store = store(&app)?;
    Ok(store.set_image_defaults(&world, &image, defaults)?)
}

#[cfg(feature = "gui")]
/// Puts an image into a folder of the gallery.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_move(app: tauri::AppHandle, world: String, image: String, folder: String) -> Answer<()> {
    let store = store(&app)?;
    Ok(store.move_image(&world, &image, &folder)?)
}

#[cfg(feature = "gui")]
/// The folders of the gallery — the GM's.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn gallery_folders(app: tauri::AppHandle, world: String) -> Answer<Vec<aw_store::media::FolderNode>> {
    Ok(store(&app)?.gallery_folders(&world)?)
}

#[cfg(feature = "gui")]
/// Sets the folders of the gallery, as a whole tree — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn gallery_folders_set(app: tauri::AppHandle, world: String, folders: Vec<aw_store::media::FolderNode>) -> Answer<()> {
    Ok(store(&app)?.set_gallery_folders(&world, folders)?)
}

#[cfg(feature = "gui")]
/// Deletes images for good, on every device — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_delete(app: tauri::AppHandle, world: String, images: Vec<String>) -> Answer<()> {
    Ok(store(&app)?.delete_images(&world, &images)?)
}

#[cfg(feature = "gui")]
/// The gallery as this reader may see it.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn gallery(app: tauri::AppHandle, world: String) -> Answer<Vec<GalleryImage>> {
    gallery_in(&store(&app)?, world)
}

/// `gallery` for whom the store is given.
pub(crate) fn gallery_in(store: &Store, world: String) -> Answer<Vec<GalleryImage>> {
    Ok(store.gallery(&world)?)
}

#[cfg(feature = "gui")]
/// An image as a `data:` URL — the original or its preview — if this reader
/// may see it.
// Off the main thread: a big image takes its time to read.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_data(app: tauri::AppHandle, world: String, image: String, preview: bool) -> Answer<String> {
    image_data_in(&store(&app)?, world, image, preview)
}

/// `image_data` for whom the store is given.
pub(crate) fn image_data_in(store: &Store, world: String, image: String, preview: bool) -> Answer<String> {
    let (bytes, mime) = store.image_bytes(&world, &image, preview)?;
    Ok(format!("data:{mime};base64,{}", crate::store::base64(&bytes)))
}
