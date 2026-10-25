// Turns a world of record files — as worlds were kept before world format 4
// — into one of its first state, in a folder of its own: the old one stays
// as it was. Its images come along as they are (a reflink where the drive
// can), and of the private folder what cannot be had again; the rest — the
// index, the projection, the readable copy — the app builds anew.
// cargo run --release -p aw-store --example convert_world -- <old world> <old private> <new world> <new private>
use aw_store::shelf::{Mode, Shelf};
use std::path::{Path, PathBuf};

/// The world's folders of records, as they were.
const FOLDERS: [&str; 4] = ["control", "articles", "proposals", "members"];
/// What of the private folder goes along.
const KEPT: [&str; 8] = ["owner.key", "keyring.json", "state.json", "requests.json", "sealed-for.json", "takeovers.json", "backup.json", "dice.json"];

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
    let [_, old, old_private, new, new_private] = &args[..] else {
        eprintln!("convert_world <old world> <old private> <new world> <new private>");
        std::process::exit(2);
    };
    let (old, old_private, new, new_private) = (Path::new(old), Path::new(old_private), Path::new(new), Path::new(new_private));
    let empty = |dir: &Path| std::fs::read_dir(dir).map_or(true, |mut d| d.next().is_none());
    if !empty(new) || !empty(new_private) {
        eprintln!("The new folders must be empty.");
        std::process::exit(2);
    }
    let started = std::time::Instant::now();
    let mut paths = Vec::new();
    for folder in FOLDERS {
        files(&old.join(folder), &mut paths);
    }
    // The gallery's entries; the parts beside them stay what they are.
    let entries: Vec<PathBuf> = std::fs::read_dir(old.join("media")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    paths.extend(entries.iter().cloned());
    let mut records = Vec::with_capacity(paths.len());
    let mut skipped = Vec::new();
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
            _ => skipped.push(path.display().to_string()),
        }
    }
    println!("read {} records in {:?}", records.len(), started.elapsed());
    for path in &skipped {
        println!("  not a record: {path}");
    }

    std::fs::create_dir_all(new).unwrap();
    std::fs::create_dir_all(new_private).unwrap();
    std::fs::copy(old.join("world.json"), new.join("world.json")).unwrap();
    let copied = std::process::Command::new("cp").args(["-a", "--reflink=auto"]).arg(old.join("media")).arg(new).status().unwrap();
    assert!(copied.success(), "the images could not be copied");
    for entry in &entries {
        std::fs::remove_file(new.join("media").join(entry.file_name().unwrap())).unwrap();
    }
    for name in KEPT {
        if old_private.join(name).is_file() {
            std::fs::copy(old_private.join(name), new_private.join(name)).unwrap();
        }
    }

    let mut shelf = Shelf::open(new, new_private, Mode::Shared).unwrap();
    shelf.put(&records).unwrap();
    assert!(shelf.consolidate(&Default::default()).unwrap(), "a first state");
    // Everything is in the state: the update it came through goes.
    let _ = std::fs::remove_dir_all(new.join("updates"));
    println!("state of {} records written in {:?}", records.len(), started.elapsed());
}
