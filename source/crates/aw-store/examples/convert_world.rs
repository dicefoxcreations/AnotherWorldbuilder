// Turns a world folder of record files — as worlds were kept before the
// shelf — into its first state. Once per world; images stay as they are.
// cargo run --release -p aw-store --example convert_world -- <world dir> <private dir>
use aw_store::shelf::{Mode, Shelf};
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (dir, private) = (Path::new(&args[1]), Path::new(&args[2]));
    let started = std::time::Instant::now();
    let folders = ["control", "articles", "proposals", "members"];
    let mut paths = Vec::new();
    for folder in folders {
        files(&dir.join(folder), &mut paths);
    }
    // The gallery's entries; the parts beside them stay.
    for entry in std::fs::read_dir(dir.join("media")).into_iter().flatten().flatten() {
        if entry.path().extension().is_some_and(|e| e == "json") {
            paths.push(entry.path());
        }
    }
    let mut records = Vec::with_capacity(paths.len());
    let mut skipped = 0;
    for path in &paths {
        let raw = std::fs::read(path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let id = stem.strip_prefix("world-").unwrap_or(&stem).to_string();
        match aw_store::journal::stored(&raw) {
            Some(mut stored) if stored.id == id => {
                stored.at = std::fs::metadata(path).and_then(|m| m.modified()).ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                records.push(stored);
            }
            _ => skipped += 1,
        }
    }
    println!("read {} records ({skipped} skipped) in {:?}", records.len(), started.elapsed());
    let mut shelf = Shelf::open(dir, private, Mode::Shared).unwrap();
    shelf.put(&records).unwrap();
    assert!(shelf.consolidate(&Default::default()).unwrap(), "a first state");
    // Everything is in the state: the one update file and the old folders go.
    let _ = std::fs::remove_dir_all(dir.join("updates"));
    for folder in folders {
        let _ = std::fs::remove_dir_all(dir.join(folder));
    }
    for path in paths.iter().filter(|p| p.starts_with(dir.join("media"))) {
        let _ = std::fs::remove_file(path);
    }
    println!("state written in {:?}", started.elapsed());
}
