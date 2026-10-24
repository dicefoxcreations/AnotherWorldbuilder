//! An online edit is private staging until its final, authenticated commit.
//! Only encrypted journal records and image parts pass through this module.

use super::*;
use aw_store::{durable, journal::Journal};
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Serialize, Deserialize)]
struct Transaction {
    world: String,
    member: String,
    base: String,
    created: u64,
}

impl Keeper {
    fn transactions(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).join("online-transactions")
    }

    fn transaction_path(&self, world: &str, member: &str, transaction: &str) -> Answer<PathBuf> {
        if transaction.len() != 64 || !transaction.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError("Invalid online transaction.".into()));
        }
        let scope = blake3::hash(format!("{world}\n{member}").as_bytes()).to_hex().to_string();
        Ok(self.transactions().join(scope).join(transaction))
    }

    pub(super) fn online_command(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
        let member = who.id.to_string();
        match ask {
            Ask::Release { transaction } => {
                let path = self.transaction_path(world, &member, &transaction)?;
                if path.join("receipt.json").is_file() { std::fs::remove_dir_all(path)? }
                Ok(Reply::Staged)
            }
            Ask::Abort { transaction } => {
                let path = self.transaction_path(world, &member, &transaction)?;
                // A published intent is completed, never undone by a disconnected client.
                if path.exists() && !path.join("commit").exists() && !path.join("receipt.json").exists() {
                    std::fs::remove_dir_all(path)?;
                }
                Ok(Reply::Staged)
            }
            Ask::Commit { transaction, base, records, parts, finish } => {
                let path = self.transaction_path(world, &member, &transaction)?;
                if let Ok(raw) = std::fs::read(path.join("receipt.json")) {
                    return Ok(serde_json::from_slice(&raw)?);
                }
                if path.join("commit").exists() {
                    return self.finish_online_command(&path);
                }
                if base.len() != 64 || !base.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(AppError("Invalid world revision.".into()));
                }
                let meta = Transaction { world: world.into(), member, base, created: network::now() };
                if let Ok(raw) = std::fs::read(path.join("transaction.json")) {
                    let before: Transaction = serde_json::from_slice(&raw)?;
                    if before.world != meta.world || before.member != meta.member || before.base != meta.base {
                        return Err(AppError("The online transaction changed.".into()));
                    }
                } else {
                    std::fs::create_dir_all(path.join("records"))?;
                    std::fs::create_dir_all(path.join("parts"))?;
                    durable::write(&path.join("transaction.json"), &serde_json::to_vec(&meta)?)?;
                }
                let mut bytes = Vec::new();
                let public = self.store.load_world_meta(world)?.public;
                let gm = self.store.with_journal(world, |j| Ok(j.world_item().map(|(.., gm)| gm.clone())))?;
                for record in records {
                    let parsed = aw_store::journal::verify_record_envelope(&record, &public, gm.as_ref())?;
                    if matches!(parsed, aw_store::journal::Record::Control(_)) {
                        bytes.push((path.join("has-control"), Vec::new()));
                    }
                    let id = aw_store::journal::record_id(record.as_bytes());
                    bytes.push((path.join("records").join(id), record.into_bytes()));
                }
                for (name, encoded) in parts {
                    let raw = STANDARD.decode(encoded)?;
                    if name.len() != 64 || aw_store::journal::record_id(&raw) != name {
                        return Err(AppError("An online image part is damaged.".into()));
                    }
                    bytes.push((path.join("parts").join(name), raw));
                }
                durable::write_all(&bytes)?;
                durable::write(&path.join("touched"), b"")?;
                if !finish {
                    return Ok(Reply::Staged);
                }
                // The per-world Keeper claim excludes every other writer here.
                let current = Journal::digest(&self.store.inventory(world)?);
                if current != meta.base && path.join("has-control").exists() {
                    std::fs::remove_dir_all(path)?;
                    return Ok(Reply::Conflict);
                }
                durable::write(&path.join("commit"), b"complete this command before serving the world")?;
                self.finish_online_command(&path)
            }
            _ => Ok(Reply::NotOffered),
        }
    }

    fn finish_online_command(&self, path: &Path) -> Answer<Reply> {
        let meta: Transaction = serde_json::from_slice(&std::fs::read(path.join("transaction.json"))?)?;
        let mut records = Vec::new();
        for file in std::fs::read_dir(path.join("records"))? {
            records.push(std::fs::read_to_string(file?.path())?);
            if records.len() == 2000 {
                self.store.receive(&meta.world, &records)?;
                records.clear();
            }
        }
        if !records.is_empty() {
            self.store.receive(&meta.world, &records)?;
        }
        for file in std::fs::read_dir(path.join("parts"))? {
            let file = file?;
            let name = file.file_name().to_string_lossy().into_owned();
            self.store.take_parts(&meta.world, &[(name, std::fs::read(file.path())?)])?;
        }
        let reply = Reply::Committed { digest: Journal::digest(&self.store.inventory(&meta.world)?) };
        durable::write(&path.join("receipt.json"), &serde_json::to_vec(&reply)?)?;
        // Receipts make a retry safe if the connection went after the commit.
        std::fs::remove_dir_all(path.join("records"))?;
        std::fs::remove_dir_all(path.join("parts"))?;
        let _ = std::fs::remove_file(path.join("commit"));
        Ok(reply)
    }

    /// Receipts answer a commit asked again after its answer got lost; after
    /// a while nobody asks again, and they go.
    pub(super) fn release_receipts(&self, older: std::time::Duration) {
        let Ok(scopes) = std::fs::read_dir(self.transactions()) else { return };
        for scope in scopes.flatten() {
            let Ok(entries) = std::fs::read_dir(scope.path()) else { continue };
            for entry in entries.flatten() {
                let receipt = entry.path().join("receipt.json");
                let old = std::fs::metadata(&receipt).and_then(|m| m.modified()).ok()
                    .and_then(|at| at.elapsed().ok()).is_some_and(|age| age > older);
                if old {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    }

    pub(super) fn recover_online_commands(&self) -> Answer<()> {
        let root = self.transactions();
        if !root.exists() {
            return Ok(());
        }
        for scope in std::fs::read_dir(&root)? {
            let scope = scope?.path();
            if !scope.is_dir() { continue }
            for entry in std::fs::read_dir(scope)? {
                let path = entry?.path();
                if !path.is_dir() { continue }
                if path.join("receipt.json").exists() {
                    // A crash after writing the receipt needs no journal replay.
                    for name in ["records", "parts"] {
                        let _ = std::fs::remove_dir_all(path.join(name));
                    }
                } else if path.join("commit").exists() {
                    self.finish_online_command(&path)?;
                } else if let Ok(raw) = std::fs::read(path.join("transaction.json")) {
                    if serde_json::from_slice::<Transaction>(&raw).is_ok_and(|m| network::now().saturating_sub(m.created) > 86400) {
                        std::fs::remove_dir_all(path)?;
                    }
                }
            }
        }
        Ok(())
    }
}
