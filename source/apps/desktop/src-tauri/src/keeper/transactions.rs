//! An online edit is private staging until its final, authenticated commit:
//! held in memory, never in files. The commit writes it into the world; a
//! server that restarts before has none of it, and the app sends the command
//! again or tells its user. Records are named by what they are, so one sent
//! twice changes nothing. Only encrypted journal records and image parts pass
//! through this module.

use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};

/// The most a command may stage: a picture or a few, not a world.
const MOST: usize = 256 * 1024 * 1024;
/// How long a receipt answers a commit asked again; a command staged and
/// never finished goes after a day.
const RECEIPT_FOR: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const STAGED_FOR: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// A command on its way, piece by piece.
pub(super) struct Staged {
    base: String,
    records: Vec<String>,
    parts: Vec<(String, Vec<u8>)>,
    control: bool,
    size: usize,
    /// The next piece it waits for.
    pieces: u32,
    at: std::time::Instant,
}

impl Keeper {
    fn transaction_key(world: &str, member: &str, transaction: &str) -> Answer<String> {
        if transaction.len() != 64 || !transaction.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError("Invalid online transaction.".into()));
        }
        Ok(format!("{world}\n{member}\n{transaction}"))
    }

    pub(super) fn online_command(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
        let member = who.id.to_string();
        match ask {
            Ask::Release { transaction } => {
                let key = Self::transaction_key(world, &member, &transaction)?;
                self.receipts.lock().map_err(AppError::from)?.remove(&key);
                Ok(Reply::Staged)
            }
            Ask::Abort { transaction } => {
                let key = Self::transaction_key(world, &member, &transaction)?;
                self.staged.lock().map_err(AppError::from)?.remove(&key);
                Ok(Reply::Staged)
            }
            Ask::Commit { transaction, base, records, parts, finish, piece } => {
                let key = Self::transaction_key(world, &member, &transaction)?;
                if let Some((reply, _)) = self.receipts.lock().map_err(AppError::from)?.get(&key) {
                    return Ok(reply.clone());
                }
                if base.len() != 64 || !base.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(AppError("Invalid world revision.".into()));
                }
                let public = self.store.load_world_meta(world)?.public;
                let gm = self.store.with_journal(world, |j| Ok(j.world_item().map(|(.., gm)| gm.clone())))?;
                // Checked before it is kept: what does not check out is not staged.
                let mut checked = Vec::with_capacity(records.len());
                let mut control = false;
                for record in records {
                    let parsed = aw_store::journal::verify_record_envelope(&record, &public, gm.as_ref())?;
                    control |= matches!(parsed, aw_store::journal::Record::Control(_));
                    checked.push(record);
                }
                let mut pieces = Vec::with_capacity(parts.len());
                for (name, encoded) in parts {
                    let raw = STANDARD.decode(encoded)?;
                    if name.len() != 64 || aw_store::journal::record_id(&raw) != name {
                        return Err(AppError("An online image part is damaged.".into()));
                    }
                    pieces.push((name, raw));
                }
                let staged = {
                    let mut all = self.staged.lock().map_err(AppError::from)?;
                    // A piece after one this server has not got — it started anew
                    // meanwhile: the command is lost, and is not half written.
                    let had = all.get(&key).map_or(0, |s| s.pieces);
                    if piece != had {
                        all.remove(&key);
                        return Err(AppError("The server lost this command. It was not saved; please try again.".into()));
                    }
                    let staged = all.entry(key.clone()).or_insert_with(|| Staged { base: base.clone(), records: Vec::new(), parts: Vec::new(), control: false, size: 0, pieces: 0, at: std::time::Instant::now() });
                    if staged.base != base {
                        return Err(AppError("The online transaction changed.".into()));
                    }
                    staged.pieces += 1;
                    staged.size += checked.iter().map(String::len).sum::<usize>() + pieces.iter().map(|(_, raw)| raw.len()).sum::<usize>();
                    if staged.size > MOST {
                        all.remove(&key);
                        return Err(AppError("This command is too big for the server.".into()));
                    }
                    staged.control |= control;
                    staged.records.extend(checked);
                    staged.parts.extend(pieces);
                    if !finish {
                        return Ok(Reply::Staged);
                    }
                    all.remove(&key).expect("staged just now")
                };
                // The per-world Keeper claim excludes every other writer here.
                if staged.control && self.store.digest(world)? != staged.base {
                    return Ok(Reply::Conflict);
                }
                for records in staged.records.chunks(2000) {
                    self.store.receive(world, records)?;
                }
                for parts in staged.parts.chunks(32) {
                    self.store.take_parts(world, parts)?;
                }
                let reply = Reply::Committed { digest: self.store.digest(world)? };
                self.receipts.lock().map_err(AppError::from)?.insert(key, (reply.clone(), std::time::Instant::now()));
                Ok(reply)
            }
            _ => Ok(Reply::NotOffered),
        }
    }

    /// Receipts nobody asks for any more, and commands staged and never
    /// finished, go.
    pub(super) fn release_receipts(&self) {
        if let Ok(mut receipts) = self.receipts.lock() {
            receipts.retain(|_, (_, at)| at.elapsed() < RECEIPT_FOR);
        }
        if let Ok(mut staged) = self.staged.lock() {
            staged.retain(|_, s| s.at.elapsed() < STAGED_FOR);
        }
    }
}
