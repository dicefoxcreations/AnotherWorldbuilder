//! Atomic replacement. Temporary files are private from creation on Unix.
use std::{io::Write, path::Path};

pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing parent"))?;
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    sync_parent(path)
}

/// Replaced atomically, without waiting on the disk: for what can be made
/// anew — an index, a receipt. After a crash it is the old one, or none.
pub fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("missing parent"))?;
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn sync_parent(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(
        path.parent()
            .ok_or_else(|| std::io::Error::other("missing parent"))?,
    )?
    .sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Many files at once, each replaced atomically as by `write`. All of a
/// batch is written before any is synced: the first sync takes the rest
/// along, where one file after the other waits on the disk every time.
pub fn write_all(files: &[(std::path::PathBuf, Vec<u8>)]) -> std::io::Result<()> {
    // Open files are limited; a batch stays well below.
    const BATCH: usize = 256;
    let mut dirs = std::collections::BTreeSet::new();
    for batch in files.chunks(BATCH) {
        let mut written = Vec::with_capacity(batch.len());
        for (path, bytes) in batch {
            let dir = path.parent().ok_or_else(|| std::io::Error::other("missing parent"))?;
            if dirs.insert(dir.to_path_buf()) {
                std::fs::create_dir_all(dir)?;
            }
            let mut file = tempfile::NamedTempFile::new_in(dir)?;
            file.write_all(bytes)?;
            written.push((file, path));
        }
        for (file, _) in &written {
            file.as_file().sync_all()?;
        }
        for (file, path) in written {
            file.persist(path).map_err(|e| e.error)?;
        }
    }
    for dir in dirs {
        #[cfg(unix)]
        std::fs::File::open(&dir)?.sync_all()?;
        #[cfg(not(unix))]
        let _ = dir;
    }
    Ok(())
}
