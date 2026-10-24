//! The journal of a world: every change, signed, the way it happened.
//!
//! A world is not a file that gets overwritten but a growing set of records.
//! Each record is one of three kinds:
//!
//! - **Control** — the world's structure: its name and calendar, the types,
//!   which articles and layers exist, who may see and edit each layer, whose
//!   keys are whose. Only the game master writes these; the world token signs
//!   them. Of each thing the newest record counts.
//! - **Version** — one editing session on a document, whole: every layer of
//!   the document as an Automerge document, encrypted. Layers the author did
//!   not change are taken over from the versions it builds on, as ciphertext.
//!   So the newest version alone shows the document.
//! - **Draft** — what an editing session wrote so far: encrypted Automerge
//!   changes of one layer, signed by their author and by the layer's write
//!   key. Drafts live in the profile and go out over the network for writing
//!   together; once the session's version is written they are gone.
//!
//! Records never change once written and are named by their hash. So two
//! devices compare what they have by name and swap what is missing — over
//! the network or through a shared folder — and every device checks every
//! record itself, no matter who passed it on.
//!
//! ```text
//! <world>/control/<hash>.json             control (the world's own: world-<hash>.json)
//! <world>/articles/<article>/<hash>.json  versions
//! <profile>/…/drafts/<hash>.json           drafts
//! ```
//!
//! Who may see a layer is its data key; who may edit it, its write key. A
//! change of either starts a new generation of the layer, with fresh keys and
//! the content so far as its start. Whoever lost a right keeps the past but
//! can neither read nor write what follows.

pub mod diff;
mod history;
pub mod layer;
pub mod types;
mod writing;

use automerge::{AutoCommit, Change, ChangeHash};
use aw_crypto::{
    decrypt, encrypt, seal_key, unseal_key, verify_write, ContentKey, Identity, IdentityPublic, WorldPublic, WorldToken,
    WrappedKey,
};
use aw_model::{ArticleId, LayerId, Level, Policy, Template, WorldCalendar};
use aw_policy::{open_dek, wrap_dek, GrantedKey, HeldKeys, Keyring, LayerHeader};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use history::Session;
pub use layer::{LayerDoc, Plain};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const CONTROL: &str = "anotherworldbuilder/control/1";
const DRAFT: &str = "anotherworldbuilder/draft/1";
const SUGGESTION: &str = "anotherworldbuilder/suggestion/1";
const BLOCK: &str = "anotherworldbuilder/block/1";
const VERSION: &str = "anotherworldbuilder/version/1";
const PROPOSAL: &str = "anotherworldbuilder/proposal/1";
const BADGE: &str = "anotherworldbuilder/badge/1";
/// A whole document travels in one version — texts are not that big.
pub const MAX_RECORD: usize = 16 * 1024 * 1024;
/// Versions kept per document when the game master has not said otherwise.
pub const VERSION_LIMIT: u32 = 50;

// --- Records ----------------------------------------------------------------

/// Who may see and edit a layer, and which part of the template it is. In
/// the head, but encrypted: whoever cannot open a layer need not know that.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Access {
    /// Key of the section in the template.
    pub section: String,
    /// The view as it was written.
    pub own: String,
    /// Who may change the layer.
    pub edit: String,
}

/// One generation of a layer: its keys and where its content starts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerHead {
    pub article: ArticleId,
    pub layer: LayerId,
    pub is_base: bool,
    pub level: Level,
    pub generation: u32,
    /// The data key, wrapped for the view.
    pub header: LayerHeader,
    /// The write key's seed, wrapped for the edit rule.
    pub write: LayerHeader,
    /// The write key's public half.
    pub writer: String,
    /// `Access`, encrypted with the data key.
    pub access: String,
    /// The generation's first state, an Automerge document encrypted with
    /// the data key.
    pub genesis: String,
    /// Kept as it is, not encrypted: the world is unencrypted. Then there is
    /// no data key, `header` and `write` hold nothing, and the write key's
    /// seed lies in `seed` — everyone who has the world may read and edit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    /// Only on a base layer: what the document is (`About`), encrypted with
    /// the data key — whoever cannot open the document does not learn it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// The heads of the previous generation its start was taken from — what
    /// it already holds of what was written there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub from: Vec<String>,
}

impl LayerHead {
    /// What opens the layer's content here: its data key, if one of the
    /// keys held opens it — or nothing at all, where it is kept as it is.
    pub fn data_key(&self, keys: &HeldKeys) -> Result<Option<DataKey>> {
        if self.plain {
            return Ok(Some(DataKey::Plain));
        }
        Ok(open_dek(&self.header, keys)?.map(DataKey::Sealed))
    }

    /// The seed of the write key, if one of the keys held opens it.
    pub fn write_seed(&self, keys: &HeldKeys) -> Result<Option<ContentKey>> {
        if let Some(seed) = &self.seed {
            let bytes: [u8; 32] = hex::decode(seed)?.try_into().map_err(|_| "a seed has 32 bytes")?;
            return Ok(Some(ContentKey::from_bytes(bytes)));
        }
        Ok(open_dek(&self.write, keys)?)
    }
}

/// How a layer's content is kept: encrypted with its data key, or as it is.
pub enum DataKey {
    Plain,
    Sealed(ContentKey),
}

impl Clone for DataKey {
    fn clone(&self) -> Self {
        match self {
            DataKey::Plain => DataKey::Plain,
            DataKey::Sealed(key) => DataKey::Sealed(ContentKey::from_bytes(*key.as_bytes())),
        }
    }
}

impl DataKey {
    /// The bytes as kept in a record: hex, encrypted where there is a key.
    pub fn seal(&self, bytes: &[u8]) -> Result<String> {
        Ok(match self {
            DataKey::Plain => hex::encode(bytes),
            DataKey::Sealed(key) => hex::encode(encrypt(key, bytes)?),
        })
    }

    pub fn open(&self, text: &str) -> Result<Vec<u8>> {
        let bytes = hex::decode(text)?;
        Ok(match self {
            DataKey::Plain => bytes,
            DataKey::Sealed(key) => decrypt(key, &bytes)?,
        })
    }
}

/// What a document is: its type and its slug.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct About {
    pub article_type: String,
    pub slug: String,
}

/// Bytes sealed for a few people: a fresh key wrapped for each of them,
/// the bytes encrypted with it. Everyone else holds noise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sealed {
    pub keys: Vec<WrappedKey>,
    pub data: String,
}

impl Sealed {
    pub fn new(recipients: &[&IdentityPublic], bytes: &[u8]) -> Result<Self> {
        let key = ContentKey::generate()?;
        let mut keys = Vec::new();
        for r in recipients {
            keys.push(seal_key(&r.seal_public()?, &key)?);
        }
        Ok(Sealed { keys, data: hex::encode(encrypt(&key, bytes)?) })
    }

    /// The same in an unencrypted world: for anyone, kept as it is.
    pub fn plain(bytes: &[u8]) -> Self {
        Sealed { keys: Vec::new(), data: hex::encode(bytes) }
    }

    /// Whether it lies there as it is.
    pub fn is_plain(&self) -> bool {
        self.keys.is_empty()
    }

    /// The key, for whoever is among the recipients.
    pub fn key(&self, secret: &x25519_dalek::StaticSecret) -> Option<ContentKey> {
        self.keys.iter().find_map(|k| unseal_key(secret, k).ok())
    }

    pub fn open(&self, secret: &x25519_dalek::StaticSecret) -> Option<Vec<u8>> {
        let bytes = hex::decode(&self.data).ok()?;
        if self.is_plain() {
            return Some(bytes);
        }
        decrypt(&self.key(secret)?, &bytes).ok()
    }
}

/// What a grant holds: the member's attributes and their keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrantContent {
    pub attributes: Vec<String>,
    pub keys: Vec<GrantedKey>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "item", rename_all = "kebab-case")]
pub enum Item {
    World { public: WorldPublic, calendar: WorldCalendar, icon: String, home: String, gm: IdentityPublic },
    /// The types, cut along their permissions — see `types`.
    Templates { frame: types::SealedFor, pieces: Vec<types::SealedFor> },
    /// That a document exists. What it is stands in its base layer.
    Article { article: ArticleId },
    Layer(LayerHead),
    /// For good: ids are never used twice.
    Removed { article: ArticleId, layer: Option<LayerId> },
    /// A member's attributes and keys (`GrantContent`), sealed for them and
    /// for the game master who wrote it.
    Grant { member: IdentityPublic, sealed: Sealed },
    /// A member taken out — standing in for their grant (the same key), so
    /// every device knows: the others' devices stop passing the world on to
    /// them. A new grant lets them in again.
    Revoked { member: String },
    /// Whether what players write waits for the game master before the
    /// others see it. `settled` are the sessions that stood when it was
    /// switched on — they count as they did.
    Moderation { on: bool, settled: Vec<String> },
    /// A member whose writing counts without waiting. When trust is taken,
    /// `settled` are their sessions that stood until then.
    Trust { member: String, trusted: bool, settled: Vec<String> },
    /// The game master's word on waiting versions. Taken in, the key of each
    /// sealed block comes along, encrypted with its layer's data key — so
    /// everyone who may read the layer reads it now.
    Verdict { article: ArticleId, accepted: bool, versions: Vec<String>, keys: BTreeMap<String, String> },
    /// How many versions of each document stay.
    Versions { max: u32 },
    /// The dice the world rolls with — what the dice bar offers, by sides.
    /// Empty: the usual set.
    Dice { sides: Vec<u32> },
    /// Records gone for good: every device removes them and takes them in
    /// no more.
    Deleted { records: Vec<String> },
    /// The folders of the gallery (`Vec<media::FolderNode>`), sealed for the
    /// game master — how they sort is theirs.
    Gallery { sealed: Sealed },
    /// The side bar's entries (`Vec<NavEntry>`): what players get sealed for
    /// everyone, the whole of it for the game master.
    Navigation { players: types::SealedFor, gm: types::SealedFor },
    /// Whether the world is encrypted — without the record, it is. Off,
    /// new parts are kept as they are, and nobody's view or rights count:
    /// whoever has the world reads and edits all of it.
    Encryption { on: bool },
    /// Whether players of an encrypted world keep a copy of it — without the
    /// record, they do. Off, they work online: with the GM's device as host,
    /// or with a server, whose copy they hold only while they are online.
    Copies { players: bool },
    /// Servers whose writing in a member's name counts as that member's —
    /// what a server does for the players of an unencrypted world it holds.
    /// By the ids of the servers' identities.
    Hosts { ids: Vec<String> },
}

impl Item {
    /// What the record is about. Of each, the newest record counts.
    fn key(&self) -> String {
        match self {
            Item::World { .. } => "world".into(),
            Item::Templates { .. } => "templates".into(),
            Item::Article { article, .. } => format!("article {article}"),
            Item::Layer(h) => format!("layer {}", h.layer),
            Item::Removed { article, layer: None } => format!("removed {article}"),
            Item::Removed { layer: Some(layer), .. } => format!("removed {layer}"),
            Item::Grant { member, .. } => format!("grant {}", member.id),
            Item::Revoked { member } => format!("grant {member}"),
            Item::Moderation { .. } => "moderation".into(),
            Item::Trust { member, .. } => format!("trust {member}"),
            Item::Verdict { versions, .. } => format!("verdict {}", record_id(versions.join(" ").as_bytes())),
            Item::Versions { .. } => "versions".into(),
            Item::Dice { .. } => "dice".into(),
            Item::Deleted { records } => format!("deleted {}", record_id(records.join(" ").as_bytes())),
            Item::Gallery { .. } => "gallery".into(),
            Item::Navigation { .. } => "navigation".into(),
            Item::Encryption { .. } => "encryption".into(),
            Item::Copies { .. } => "copies".into(),
            Item::Hosts { .. } => "hosts".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Control {
    pub world: String,
    /// The game master's count — the higher, the newer.
    pub seq: u64,
    /// Milliseconds since 1970, by the game master's clock.
    pub time: u64,
    /// The editing session it came from; empty outside of one.
    pub session: String,
    pub item: Item,
    pub signature: String,
}

impl Control {
    fn signed_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(CONTROL, &self.world, self.seq, self.time, &self.session, &self.item))?)
    }

    fn verify(&self, world: &WorldPublic) -> Result<()> {
        if self.world != world.world_id.as_str() {
            return Err("a record of another world".into());
        }
        world.verify(&self.signed_bytes()?, &signature(&self.signature)?)?;
        if let Item::World { public, .. } = &self.item {
            if public.world_id != world.world_id || public.owner_key != world.owner_key {
                return Err("a world record for another world".into());
            }
        }
        Ok(())
    }
}

/// What an editing session wrote to one layer — until its version is there.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub world: String,
    pub article: ArticleId,
    pub layer: LayerId,
    pub generation: u32,
    pub author: IdentityPublic,
    /// One editing session — from going into edit mode until leaving it —
    /// is one version.
    pub session: String,
    /// Order within the session.
    pub seq: u32,
    /// Milliseconds since 1970, by the author's clock.
    pub time: u64,
    /// The Automerge changes, encrypted with the layer's data key — or,
    /// with `seal`, with a key of their own.
    pub changes: String,
    /// Written while it waits for the game master: the changes' key, wrapped
    /// for the game master and the author only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seal: Option<Vec<WrappedKey>>,
    /// By the layer's write key: the author was allowed to.
    pub write_signature: String,
    /// By the author: it was them.
    pub signature: String,
}

impl Draft {
    fn signed_bytes(&self) -> Result<Vec<u8>> {
        let what = if self.seal.is_some() { SUGGESTION } else { DRAFT };
        Ok(serde_json::to_vec(&(
            what,
            &self.world,
            &self.article,
            &self.layer,
            self.generation,
            &self.author,
            &self.session,
            self.seq,
            self.time,
            &self.changes,
            &self.seal,
        ))?)
    }

    fn verify(&self, head: &LayerHead) -> Result<()> {
        if head.layer != self.layer || head.article != self.article || head.generation != self.generation {
            return Err("a change for another layer".into());
        }
        if !self.author.is_genuine() {
            return Err("an author whose id does not match their keys".into());
        }
        let bytes = self.signed_bytes()?;
        self.author.verify(&bytes, &signature(&self.signature)?)?;
        verify_write(&head.writer, &bytes, &signature(&self.write_signature)?)?;
        Ok(())
    }
}

/// One layer of a version: the layer's Automerge document, saved whole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub layer: LayerId,
    pub generation: u32,
    /// Encrypted with the layer's data key — or, with `seal`, with a key of
    /// its own while it waits for the game master.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seal: Option<Vec<WrappedKey>>,
    /// By the layer's write key: whoever wrote it was allowed to. A block
    /// taken over into a later version keeps it.
    pub write_signature: String,
}

impl Block {
    fn signed_bytes(&self, world: &str, article: &ArticleId) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(BLOCK, world, article, &self.layer, self.generation, &self.state, &self.seal))?)
    }

    /// Its name — what a verdict reveals its key under.
    pub fn id(&self) -> String {
        record_id(self.state.as_bytes())
    }
}

/// One editing session on a document, as a whole document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Version {
    pub world: String,
    pub article: ArticleId,
    pub author: IdentityPublic,
    pub session: String,
    /// The session's first and last change, by the author's clock
    /// (milliseconds). The author's drafts up to `until` are in here.
    pub from: u64,
    pub until: u64,
    /// The versions it builds on: the document as the author saw it.
    pub parents: Vec<String>,
    /// Where the session wrote: layer and generation.
    pub wrote: Vec<(LayerId, u32)>,
    pub blocks: Vec<Block>,
    /// Drafts whose changes are in here and in no version it builds on —
    /// someone else's, written together; or the author's own where no block
    /// could be written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drafts: Vec<String>,
    pub signature: String,
}

impl Version {
    fn signed_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            VERSION,
            &self.world,
            &self.article,
            &self.author,
            &self.session,
            self.from,
            self.until,
            &self.parents,
            &self.wrote,
            &self.blocks,
            &self.drafts,
        ))?)
    }
}

/// What the journal keeps of a version in memory and in its index — all
/// but the blocks' content, which stays in the file until needed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionHead {
    pub article: ArticleId,
    pub author: IdentityPublic,
    pub session: String,
    pub from: u64,
    pub until: u64,
    pub parents: Vec<String>,
    pub wrote: Vec<(LayerId, u32)>,
    /// Layer, generation, name and whether it is sealed — of each block.
    pub blocks: Vec<(LayerId, u32, String, bool)>,
    /// The drafts inside, by id.
    pub drafts: Vec<String>,
}

impl VersionHead {
    fn of(version: &Version) -> Self {
        VersionHead {
            article: version.article,
            author: version.author.clone(),
            session: version.session.clone(),
            from: version.from,
            until: version.until,
            parents: version.parents.clone(),
            wrote: version.wrote.clone(),
            blocks: version.blocks.iter().map(|b| (b.layer, b.generation, b.id(), b.seal.is_some())).collect(),
            drafts: version.drafts.iter().map(|raw| record_id(raw.as_bytes())).collect(),
        }
    }
}

/// What a device remembers of a world between runs, in its profile: the
/// heads of the versions it checked, the drafts inside them, and the
/// blocks whose changes checked out — so it need not read every version
/// again to show the newest.
#[derive(Default, Serialize, Deserialize)]
struct Index {
    versions: BTreeMap<String, (VersionHead, u64)>,
    attached: BTreeMap<String, Draft>,
    trusted: BTreeSet<String>,
}

/// A record that waits for what it builds on.
enum Pending {
    /// Bytes from elsewhere — and whether they still need to be written down.
    Record(Vec<u8>, Box<Record>, bool),
    /// A version known from the index, its file on disk.
    Head(Box<VersionHead>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Record {
    Control(Control),
    Draft(Draft),
    Version(Version),
    Image(crate::media::ImageEntry),
    Proposal(Proposal),
    Badge(Badge),
}

/// How a member is shown in a world: their colour — on their name, and on
/// the documents assigned to them they name (`of`, `doc:…`), which only
/// they and the GM know otherwise. Signed by them; of each member the newest
/// counts. Every device of the world takes it in — no game master needed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Badge {
    pub world: String,
    pub author: IdentityPublic,
    /// Milliseconds since 1970, by the author's clock.
    pub at: u64,
    /// `#rrggbb`.
    pub colour: String,
    pub of: Vec<String>,
    pub signature: String,
}

impl Badge {
    pub fn new(world: &str, author: &Identity, colour: &str, of: Vec<String>, at: u64) -> Result<Self> {
        let mut b = Badge { world: world.to_string(), author: author.public(), at, colour: colour.to_string(), of, signature: String::new() };
        b.check()?;
        b.signature = hex::encode(author.sign(&b.signed_bytes()?));
        Ok(b)
    }

    fn signed_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(BADGE, &self.world, &self.author, self.at, &self.colour, &self.of))?)
    }

    fn check(&self) -> Result<()> {
        let hex = self.colour.strip_prefix('#').is_some_and(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()));
        if !hex {
            return Err("a colour is #rrggbb".into());
        }
        if self.of.len() > 32 || self.of.iter().any(|a| !a.starts_with("doc:") || a.len() > 64) {
            return Err("a badge names documents only".into());
        }
        Ok(())
    }

    fn verify(&self, world: &str) -> Result<()> {
        if self.world != world {
            return Err("a record of another world".into());
        }
        if !self.author.is_genuine() {
            return Err("an author whose id does not match their keys".into());
        }
        self.check()?;
        self.author.verify(&self.signed_bytes()?, &signature(&self.signature)?)?;
        Ok(())
    }
}

/// A member's new document, for the game master's device to make.
///
/// What a world has — which documents, which parts, their keys — only the
/// GM writes (`Control`). A player who may make documents of a type (its
/// `create=`) writes this instead: the document as they filled it in,
/// sealed for the GM and themselves, signed by them. It travels like
/// everything else; when the GM's device has it, it makes the document with
/// the type's access and deletes the proposal (`Item::Deleted`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub world: String,
    pub author: IdentityPublic,
    /// Milliseconds since 1970, by the author's clock.
    pub at: u64,
    /// The document asked for — what `Proposal::open` gives — sealed for
    /// the GM and the author.
    pub sealed: Sealed,
    pub signature: String,
}

impl Proposal {
    /// Seals `content` for the GM and the author — in an encrypted world —
    /// and signs it.
    pub fn new(world: &str, author: &Identity, gm: &IdentityPublic, content: &[u8], at: u64, encrypted: bool) -> Result<Self> {
        let me = author.public();
        let sealed = if encrypted { Sealed::new(&[gm, &me], content)? } else { Sealed::plain(content) };
        let mut p = Proposal { world: world.to_string(), sealed, author: me, at, signature: String::new() };
        p.signature = hex::encode(author.sign(&p.signed_bytes()?));
        Ok(p)
    }

    fn signed_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(PROPOSAL, &self.world, &self.author, self.at, &self.sealed.keys, &self.sealed.data))?)
    }

    fn verify(&self, world: &str) -> Result<()> {
        if self.world != world {
            return Err("a record of another world".into());
        }
        if !self.author.is_genuine() {
            return Err("an author whose id does not match their keys".into());
        }
        self.author.verify(&self.signed_bytes()?, &signature(&self.signature)?)?;
        Ok(())
    }

    /// The document asked for, for the GM or the author.
    pub fn open(&self, secret: &x25519_dalek::StaticSecret) -> Option<Vec<u8>> {
        self.sealed.open(secret)
    }
}

fn signature(text: &str) -> Result<[u8; 64]> {
    hex::decode(text)?.try_into().map_err(|_| "a signature has 64 bytes".into())
}

/// The world `world_id` among a set of records — taken from its own record,
/// and only if the world's key signed it. How a world not held yet comes in.
/// Records of other worlds and forgeries in between are passed over.
pub fn world_of(records: &[String], world_id: &str) -> Result<WorldPublic> {
    records
        .iter()
        .filter_map(|raw| world_record(raw).ok())
        .map(|(public, ..)| public)
        .find(|public| public.world_id.as_str() == world_id)
        .ok_or_else(|| "the world's own record is missing".into())
}

/// What a world's own record says — the world, its icon, its game master —
/// if the world's key signed it.
pub fn world_record(raw: &str) -> Result<(WorldPublic, String, IdentityPublic)> {
    let Record::Control(control) = serde_json::from_str::<Record>(raw)? else {
        return Err("not a world's own record".into());
    };
    let Item::World { public, icon, gm, .. } = &control.item else {
        return Err("not a world's own record".into());
    };
    if !public.is_genuine() {
        return Err("the world id does not belong to its key".into());
    }
    control.verify(public)?;
    Ok((public.clone(), icon.clone(), gm.clone()))
}

/// The world record's count — which of several is the newest.
pub fn world_seq(raw: &str) -> Option<u64> {
    match serde_json::from_str::<Record>(raw).ok()? {
        Record::Control(c) if matches!(c.item, Item::World { .. }) => Some(c.seq),
        _ => None,
    }
}

/// Whom a record grants keys — if it is a grant the world's key signed.
pub fn grantee(raw: &str, world: &WorldPublic) -> Option<String> {
    let Ok(Record::Control(control)) = serde_json::from_str::<Record>(raw) else { return None };
    let Item::Grant { member, .. } = &control.item else { return None };
    control.verify(world).ok()?;
    Some(member.id.to_string())
}

/// A record's name: the hash of exactly its bytes.
pub fn record_id(raw: &[u8]) -> String {
    blake3::hash(raw).to_hex().to_string()
}

/// Validates the signed outer record before an online transaction is
/// published. Layer write keys and causal dependencies are checked when
/// the complete transaction is taken into the journal.
pub fn verify_record_envelope(raw: &str, world: &WorldPublic, gm: Option<&IdentityPublic>) -> Result<Record> {
    if raw.len() > MAX_RECORD { return Err("an online record is too large".into()) }
    let record: Record = serde_json::from_str(raw)?;
    let expected = world.world_id.as_str();
    match &record {
        Record::Control(c) => c.verify(world)?,
        Record::Draft(d) => {
            if d.world != expected || !d.author.is_genuine() { return Err("a draft of another world or identity".into()) }
            d.author.verify(&d.signed_bytes()?, &signature(&d.signature)?)?;
        }
        Record::Version(v) => {
            if v.world != expected || !v.author.is_genuine() { return Err("a version of another world or identity".into()) }
            v.author.verify(&v.signed_bytes()?, &signature(&v.signature)?)?;
        }
        Record::Image(i) => i.verify(expected, gm)?,
        Record::Proposal(p) => p.verify(expected)?,
        Record::Badge(b) => b.verify(expected)?,
    }
    Ok(record)
}

/// A filesystem-safe name from an id: `art:01M…` → `art-01M…`.
pub fn dir_name(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect()
}

/// Where a record lies below the world's folder — the same in a local
/// folder and in a shared one: `control/<hash>.json` (the world's own record
/// `control/world-<hash>.json`) and `articles/<article>/<hash>.json`. Drafts
/// are not shared through folders: `None`.
fn place(id: &str, record: &Record) -> Option<(Vec<String>, String)> {
    match record {
        Record::Control(c) if matches!(c.item, Item::World { .. }) => Some((vec!["control".into()], format!("world-{id}.json"))),
        Record::Control(_) => Some((vec!["control".into()], format!("{id}.json"))),
        Record::Version(v) => Some((vec!["articles".into(), dir_name(&v.article.to_string())], format!("{id}.json"))),
        Record::Image(_) => Some((vec!["media".into()], format!("{id}.json"))),
        Record::Proposal(_) => Some((vec!["proposals".into()], format!("{id}.json"))),
        Record::Badge(_) => Some((vec!["members".into()], format!("{id}.json"))),
        Record::Draft(_) => None,
    }
}

/// Where a shared record lies below the world's folder: its folders and
/// its file name.
pub fn shared_path(raw: &[u8]) -> Option<(Vec<String>, String)> {
    place(&record_id(raw), &serde_json::from_slice(raw).ok()?)
}

/// The record id in a file name, if it is one: `<hash>.json` or
/// `world-<hash>.json`.
pub fn id_in_name(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".json")?;
    let hash = stem.strip_prefix("world-").unwrap_or(stem);
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hash)
}

/// A new generation of a layer — fresh keys, and `plain` as its start.
#[allow(clippy::too_many_arguments)]
pub fn new_head(
    ring: &mut Keyring,
    article: ArticleId,
    layer: LayerId,
    is_base: bool,
    level: Level,
    generation: u32,
    view: &Policy,
    access: &Access,
    plain: &Plain,
    about: Option<&About>,
    encrypted: bool,
) -> Result<LayerHead> {
    if encrypted {
        ensure_keys(ring, view, access)?;
    }
    make_head(ring, article, layer, is_base, level, generation, view, access, plain, about, encrypted)
}

/// The attributes a layer's view and edit rule name, made where new — before
/// `make_head`, which only reads the keyring.
pub fn ensure_keys(ring: &mut Keyring, view: &Policy, access: &Access) -> Result<()> {
    for attribute in view.mentioned().into_iter().chain(edit_rule(access).mentioned()) {
        ring.ensure(attribute)?;
    }
    Ok(())
}

/// Without a readable rule the game master decides.
fn edit_rule(access: &Access) -> Policy {
    access.edit.parse().unwrap_or_else(|_| "gm".parse().expect("gm is a policy"))
}

/// `new_head` with the attributes made already — many at once, from several
/// threads.
#[allow(clippy::too_many_arguments)]
pub fn make_head(
    ring: &Keyring,
    article: ArticleId,
    layer: LayerId,
    is_base: bool,
    level: Level,
    generation: u32,
    view: &Policy,
    access: &Access,
    plain: &Plain,
    about: Option<&About>,
    encrypted: bool,
) -> Result<LayerHead> {
    let edit = edit_rule(access);
    let (writer, seed) = aw_crypto::WriteKey::generate()?;
    let (key, header, write, open_seed) = if encrypted {
        let dek = ContentKey::generate()?;
        let header = wrap_dek(view, generation, ring, &dek)?;
        let write = wrap_dek(&edit, generation, ring, &seed)?;
        (DataKey::Sealed(dek), header, write, None)
    } else {
        let none = LayerHeader { version: aw_policy::HEADER_VERSION, epoch: generation, salt: String::new(), slots: BTreeMap::new() };
        (DataKey::Plain, none.clone(), none, Some(hex::encode(seed.as_bytes())))
    };
    let mut genesis = LayerDoc::genesis(plain, &layer.to_string(), generation)?;
    Ok(LayerHead {
        article,
        layer,
        is_base,
        level,
        generation,
        header,
        write,
        writer: writer.public(),
        access: key.seal(&serde_json::to_vec(access)?)?,
        genesis: key.seal(&genesis.doc.save())?,
        about: about.map(|a| key.seal(&serde_json::to_vec(a)?)).transpose()?,
        from: Vec::new(),
        plain: !encrypted,
        seed: open_seed,
    })
}

// --- The journal ------------------------------------------------------------

/// A layer's document opened on this device.
struct Opened {
    doc: LayerDoc,
    dek: DataKey,
    access: Access,
    /// The changes of the generation's start.
    genesis: HashSet<ChangeHash>,
    /// Drafts and versions already in the document, with the changes they
    /// brought.
    applied: HashMap<String, Vec<ChangeHash>>,
    /// The heads of what this device shows, and the journal's version they
    /// were found for. `None`: everything.
    visible: Option<(u64, Option<Vec<ChangeHash>>)>,
    /// Each change's session, author and time — for the heads it was made at.
    index: Option<(Vec<ChangeHash>, Vec<Made>)>,
}

/// Who made a change, in which session, when.
#[derive(Clone)]
struct Made {
    hash: ChangeHash,
    session: String,
    actor: Vec<u8>,
    time: u64,
}

/// Where a version stands with the game master.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Review {
    /// In the text.
    Counted,
    /// Written while moderated; only its author and the game master have it.
    Waiting,
    /// Turned down — it stays in the history, not in the text.
    Rejected,
}

/// A layer as it stands, for this device.
pub struct LayerState {
    pub head: LayerHead,
    /// Only where this device can open it: data key, access and content.
    pub open: Option<(DataKey, Access, Plain)>,
}

pub struct ArticleState {
    pub article: ArticleId,
    pub article_type: String,
    pub slug: String,
    /// In the order they came into being — the base layer first.
    pub layers: Vec<LayerState>,
}

/// What came of taking in a record.
#[derive(Debug, PartialEq, Eq)]
pub enum Added {
    New(String),
    Known,
    /// What it builds on is not here yet — it waits.
    Waiting,
}

/// How often every folder is read again regardless of its time — for what
/// a folder's time does not show. A large world takes a moment for it.
const FULL_SCAN: std::time::Duration = std::time::Duration::from_secs(300);

pub struct Journal {
    /// The world's folder.
    dir: PathBuf,
    /// This device's own folder for the world, in the profile: drafts and
    /// the index.
    private: PathBuf,
    drafts_dir: PathBuf,
    world: WorldPublic,
    /// The bytes of control records and drafts. Versions stay in their files.
    raw: BTreeMap<String, Vec<u8>>,
    controls: BTreeMap<String, Control>,
    drafts: BTreeMap<String, Draft>,
    versions: BTreeMap<String, VersionHead>,
    /// Versions read whole, by id.
    full: std::cell::RefCell<HashMap<String, std::sync::Arc<Version>>>,
    /// Drafts that came inside a version, by their own id.
    attached: BTreeMap<String, Draft>,
    /// Gallery entries, by their own id.
    images: BTreeMap<String, crate::media::ImageEntry>,
    /// New documents members asked for, by the proposal's id.
    proposals: BTreeMap<String, Proposal>,
    /// The newest badge of each member, by member id — with its record's id.
    badges: BTreeMap<String, (String, Badge)>,
    /// Blocks whose changes checked out on this device.
    trusted: HashSet<String>,
    /// Versions known from the index, whose files need no reading.
    indexed: BTreeMap<String, (VersionHead, u64)>,
    /// Whether the index needs writing.
    dirty: bool,
    /// Records taken in together, written together: one wait on the disk
    /// for all of them, not two for each.
    batch: Option<Vec<(PathBuf, Vec<u8>, String)>>,
    /// Records whose layer head or parents have not come yet.
    waiting: BTreeMap<String, Pending>,
    /// Files read already, whatever came of them.
    read: HashSet<PathBuf>,
    /// What each folder held when last looked through, with its modification
    /// time: a folder whose time did not change is not read again. A world of
    /// ten thousand documents has as many folders.
    listings: HashMap<PathBuf, (std::time::SystemTime, HashSet<String>)>,
    /// When every folder was last read regardless — now and then, in case a
    /// file system keeps coarse times.
    full_scan: Option<std::time::Instant>,
    /// Item key → newest (seq, record id).
    latest: BTreeMap<String, (u64, String)>,
    /// Every generation of every layer → (seq, record id) of its head.
    generations: BTreeMap<(LayerId, u32), (u64, String)>,
    /// When a layer came into being, for the order of layers.
    born: BTreeMap<LayerId, u64>,
    /// The layers each article ever had — so its parts are found without
    /// looking through every layer of the world.
    article_layers: HashMap<ArticleId, BTreeSet<LayerId>>,
    /// Drafts and versions by layer and generation, in the order they came.
    by_layer: BTreeMap<(LayerId, u32), Vec<String>>,
    max_seq: u64,
    keys: Option<HeldKeys>,
    /// Whose device this is — their own waiting versions show here — and
    /// their secret, which opens what is sealed for them.
    me: Option<(String, x25519_dalek::StaticSecret)>,
    /// The game master's word on versions: (seq, taken in).
    verdicts: HashMap<String, (u64, bool)>,
    /// Keys of sealed blocks a verdict took in: block id → the key,
    /// encrypted with the layer's data key.
    revealed: HashMap<String, String>,
    /// Records the game master deleted.
    deleted: HashSet<String>,
    opened: HashMap<(LayerId, u32), Opened>,
    /// Rises with every change — so a projection knows when it is stale.
    version: u64,
    /// The version at which each article last changed — and at which
    /// everything did (keys, review, a reload). A projection rebuilds only
    /// what changed since it was made.
    changed_at: HashMap<ArticleId, u64>,
    all_changed_at: u64,
    /// What each base layer says the document is, once opened — by layer and
    /// generation; it never changes within one.
    abouts: std::cell::RefCell<HashMap<(LayerId, u32), Option<About>>>,
    /// The images the readable parts name, with what opens them — by version.
    image_refs: (u64, Option<HashMap<String, crate::media::ImageRef>>),
    /// The images old versions still name, as of a version of the journal:
    /// reading every version takes its time.
    in_versions_seen: (u64, Option<BTreeSet<String>>),
    /// When each record came to this device — its file's time.
    arrived: BTreeMap<String, u64>,
    /// Every record in the order it was taken in since the journal was
    /// opened — so whoever asks gets only what is newer than what they had.
    arrivals: Vec<String>,
    /// Names this order: opened again, the order is another one.
    epoch: String,
    /// The inventory's digest, as of (records, versions, arrivals).
    digest_cache: std::cell::RefCell<Option<((usize, usize, usize), String)>>,
}

/// A name for one opening of a journal, unlike any before it.
fn epoch() -> String {
    static OPENED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    blake3::hash(format!("{nanos}-{n}-{}", std::process::id()).as_bytes()).to_hex()[..16].to_string()
}

/// Milliseconds since 1970 — the journal's clock.
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The order records can be taken in: control first, then versions, then
/// drafts — each by its own count or time.
fn order(record: &Record) -> (u8, u64) {
    match record {
        Record::Control(c) => (0, c.seq),
        Record::Version(v) => (1, v.until),
        Record::Draft(d) => (2, d.time),
        Record::Image(i) => (3, i.time),
        Record::Proposal(p) => (4, p.at),
        Record::Badge(b) => (5, b.at),
    }
}

impl Journal {
    /// Opens the journal of the world in `dir`, with this device's drafts
    /// and index in `private`, checking every record it has not checked
    /// before.
    pub fn open(dir: &Path, private: &Path, world: &WorldPublic) -> Result<Self> {
        if !world.is_genuine() {
            return Err("the world id does not belong to its key".into());
        }
        // An index that does not read is no loss: everything is read anew.
        let index: Index = std::fs::read(private.join("index.json"))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        let mut journal = Journal {
            dir: dir.to_path_buf(),
            private: private.to_path_buf(),
            drafts_dir: private.join("drafts"),
            world: world.clone(),
            raw: BTreeMap::new(),
            controls: BTreeMap::new(),
            drafts: BTreeMap::new(),
            versions: BTreeMap::new(),
            full: Default::default(),
            images: BTreeMap::new(),
            proposals: BTreeMap::new(),
            badges: BTreeMap::new(),
            attached: index.attached,
            trusted: index.trusted.into_iter().collect(),
            indexed: index.versions,
            dirty: false,
            batch: None,
            waiting: BTreeMap::new(),
            read: HashSet::new(),
            listings: HashMap::new(),
            full_scan: None,
            latest: BTreeMap::new(),
            generations: BTreeMap::new(),
            born: BTreeMap::new(),
            article_layers: HashMap::new(),
            by_layer: BTreeMap::new(),
            max_seq: 0,
            keys: None,
            me: None,
            verdicts: HashMap::new(),
            revealed: HashMap::new(),
            deleted: HashSet::new(),
            opened: HashMap::new(),
            version: 0,
            changed_at: HashMap::new(),
            all_changed_at: 0,
            abouts: Default::default(),
            image_refs: (0, None),
            in_versions_seen: (0, None),
            arrived: BTreeMap::new(),
            arrivals: Vec::new(),
            epoch: epoch(),
            digest_cache: Default::default(),
        };
        journal.refresh()?;
        // Parts of deleted images leave the disk, too — unless an image here
        // needs them still: a part is named by its bytes, and an image kept as
        // it is gets the same name again each time.
        if !journal.deleted.is_empty() {
            let needed = journal.needed_parts();
            for part in crate::media::parts_here(dir) {
                if journal.deleted.contains(&part) && !needed.contains(&part) {
                    crate::media::remove_part(dir, &part);
                }
            }
        }
        Ok(journal)
    }

    pub fn world(&self) -> &WorldPublic {
        &self.world
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// The folders records lie in.
    fn folders(&self) -> Vec<PathBuf> {
        let mut out = vec![self.dir.join("control"), self.dir.join("media"), self.dir.join("proposals"), self.dir.join("members")];
        if let Ok(entries) = std::fs::read_dir(self.dir.join("articles")) {
            out.extend(entries.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()));
        }
        out.push(self.drafts_dir.clone());
        out
    }

    /// Picks up records that appeared in the folders meanwhile — and lets go
    /// of versions whose files are gone. Whatever does not check out is left
    /// lying and ignored.
    pub fn refresh(&mut self) -> Result<bool> {
        let mut found = Vec::new();
        let mut heads: Vec<(String, VersionHead)> = Vec::new();
        let mut present: HashSet<String> = HashSet::new();
        let full = self.full_scan.is_none_or(|at| at.elapsed() >= FULL_SCAN);
        if full {
            self.full_scan = Some(std::time::Instant::now());
        }
        let folders: Vec<(PathBuf, Option<std::time::SystemTime>)> =
            self.folders().into_iter().map(|f| { let stamp = std::fs::metadata(&f).and_then(|m| m.modified()).ok(); (f, stamp) }).collect();
        // No folder came, went or changed since the last look: nothing to pick up.
        if !full
            && folders.iter().filter(|(_, stamp)| stamp.is_some()).count() == self.listings.len()
            && folders.iter().all(|(folder, stamp)| stamp.is_none_or(|stamp| self.listings.get(folder).is_some_and(|(was, _)| *was == stamp)))
        {
            return Ok(false);
        }
        // A folder gone is listed no more.
        let there: HashSet<&PathBuf> = folders.iter().filter(|(_, stamp)| stamp.is_some()).map(|(f, _)| f).collect();
        self.listings.retain(|f, _| there.contains(f));
        for (folder, stamp) in folders {
            if let (false, Some(stamp), Some((was, ids))) = (full, stamp, self.listings.get(&folder)) {
                if *was == stamp {
                    present.extend(ids.iter().cloned());
                    continue;
                }
            }
            let Ok(entries) = std::fs::read_dir(&folder) else { continue };
            let mut listed = HashSet::new();
            for entry in entries {
                let path = entry?.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
                let Some(id) = id_in_name(name).map(str::to_string) else { continue };
                listed.insert(id.clone());
                present.insert(id.clone());
                if self.read.contains(&path) || self.versions.contains_key(&id) || self.waiting.contains_key(&id) {
                    continue;
                }
                // Checked before: its head is enough.
                if let Some((head, at)) = self.indexed.get(&id) {
                    if self.dir.join("articles").join(dir_name(&head.article.to_string())).join(name) == path {
                        self.arrived.entry(id.clone()).or_insert(*at);
                        heads.push((id, head.clone()));
                        continue;
                    }
                }
                self.read.insert(path.clone());
                let meta = std::fs::metadata(&path)?;
                if meta.len() > MAX_RECORD as u64 {
                    continue;
                }
                let at = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let raw = std::fs::read(&path)?;
                found.push((raw, at));
            }
            if let Some(stamp) = stamp {
                self.listings.insert(folder, (stamp, listed));
            }
        }
        // Versions whose files went — deleted elsewhere — are gone here too.
        let gone: Vec<String> = self.versions.keys().filter(|id| !present.contains(*id)).cloned().collect();
        let mut changed = !gone.is_empty();
        if changed {
            for id in &gone {
                self.indexed.remove(id);
            }
            self.dirty = true;
            self.flush()?;
            self.reload()?;
            return Ok(true);
        }
        let mut parsed: Vec<(Vec<u8>, Record, u64)> = found
            .into_iter()
            .filter_map(|(raw, at)| Some((raw.clone(), serde_json::from_slice(&raw).ok()?, at)))
            .collect();
        parsed.sort_by_key(|(_, r, _)| order(r));
        // What the game master deleted goes first — it is not taken in.
        for (raw, record, _) in &parsed {
            if let Record::Control(c) = record {
                if let Item::Deleted { records } = &c.item {
                    if c.verify(&self.world).is_ok() && !self.deleted.contains(&record_id(raw)) {
                        self.deleted.extend(records.iter().cloned());
                    }
                }
            }
        }
        let (controls, rest): (Vec<_>, Vec<_>) = parsed.into_iter().partition(|(_, r, _)| matches!(r, Record::Control(_)));
        for (raw, record, at) in controls {
            changed |= self.take_found(raw, record, at);
        }
        heads.sort_by_key(|(_, h)| h.until);
        for (id, head) in heads {
            if self.deleted.contains(&id) {
                let path = self.dir.join("articles").join(dir_name(&head.article.to_string())).join(format!("{id}.json"));
                let _ = std::fs::remove_file(path);
                self.indexed.remove(&id);
                self.dirty = true;
                continue;
            }
            if let Ok(Added::New(_)) = self.take_head(id, head) {
                changed = true;
            }
        }
        for (raw, record, at) in rest {
            changed |= self.take_found(raw, record, at);
        }
        // What the index knows but no folder holds any more is forgotten.
        let before = self.indexed.len();
        self.indexed.retain(|id, _| present.contains(id));
        self.dirty |= self.indexed.len() != before;
        self.flush()?;
        Ok(changed)
    }

    fn take_found(&mut self, raw: Vec<u8>, record: Record, at: u64) -> bool {
        let id = record_id(&raw);
        if self.deleted.contains(&id) {
            self.forget_file(&id, &record);
            return false;
        }
        self.arrived.entry(id).or_insert(at);
        matches!(self.take(raw, record, false), Ok(Added::New(_)))
    }

    /// Writes the index down, if it changed.
    fn flush(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let index = Index {
            versions: self.indexed.clone(),
            attached: self.attached.clone(),
            trusted: self.trusted.iter().cloned().collect(),
        };
        // Only what the folders hold, known in advance: lost, it is read again.
        crate::durable::replace(&self.private.join("index.json"), &serde_json::to_vec(&index)?)?;
        self.dirty = false;
        Ok(())
    }

    /// Takes in a record from elsewhere — checked, then kept.
    pub fn add(&mut self, raw: &[u8]) -> Result<Added> {
        if raw.len() > MAX_RECORD {
            return Err("a record too big".into());
        }
        let record: Record = serde_json::from_slice(raw)?;
        let deletes = matches!(&record, Record::Control(Control { item: Item::Deleted { .. }, .. }));
        let added = self.take(raw.to_vec(), record, true)?;
        // Something is gone for good: read everything anew without it.
        if deletes && matches!(added, Added::New(_)) {
            self.reload()?;
        }
        self.flush()?;
        Ok(added)
    }

    /// Takes in many records from elsewhere — as `add` each, the index kept
    /// once at the end: a whole world arriving would write it anew for every
    /// version. Returns how many were new.
    pub fn add_all(&mut self, raws: &[&[u8]]) -> Result<usize> {
        self.batch = Some(Vec::new());
        let taken = self.take_all(raws);
        let written = self.write_batch();
        // What did not reach the disk is not kept.
        if taken.is_err() || written.is_err() {
            let _ = self.reload();
        }
        let new = taken?;
        written?;
        self.flush()?;
        Ok(new)
    }

    fn take_all(&mut self, raws: &[&[u8]]) -> Result<usize> {
        let mut new = 0;
        for raw in raws {
            if raw.len() > MAX_RECORD {
                continue;
            }
            let Ok(record) = serde_json::from_slice::<Record>(raw) else { continue };
            let deletes = matches!(&record, Record::Control(Control { item: Item::Deleted { .. }, .. }));
            if let Ok(Added::New(_)) = self.take(raw.to_vec(), record, true) {
                new += 1;
                // Something is gone for good: read everything anew without it.
                if deletes {
                    self.write_batch()?;
                    self.reload()?;
                    self.batch = Some(Vec::new());
                }
            }
        }
        Ok(new)
    }

    /// Writes what was taken in together.
    fn write_batch(&mut self) -> Result<()> {
        let Some(batch) = self.batch.take() else { return Ok(()) };
        let files: Vec<(PathBuf, Vec<u8>)> = batch.iter().map(|(path, raw, _)| (path.clone(), raw.clone())).collect();
        crate::durable::write_all(&files)?;
        for (path, _, id) in &batch {
            self.listed(path, id, true);
        }
        Ok(())
    }

    /// Forgets everything and reads it all from disk again — keys and who
    /// this device is stay.
    fn reload(&mut self) -> Result<()> {
        let keys = self.keys.take();
        let me = self.me.take();
        let version = self.version;
        self.flush()?;
        let mut fresh = Journal::open(&self.dir, &self.private, &self.world)?;
        fresh.keys = keys;
        fresh.me = me;
        fresh.version = version + fresh.version + 1;
        fresh.all_changed_at = fresh.version;
        *self = fresh;
        Ok(())
    }

    fn take(&mut self, raw: Vec<u8>, record: Record, write: bool) -> Result<Added> {
        let id = record_id(&raw);
        if self.raw.contains_key(&id) || self.versions.contains_key(&id) || self.deleted.contains(&id) {
            return Ok(Added::Known);
        }
        // Which article this changes — `None`: possibly any.
        let touches: Option<Option<ArticleId>> = match &record {
            Record::Control(c) => match &c.item {
                Item::Layer(h) => Some(Some(h.article)),
                Item::Article { article } | Item::Removed { article, .. } => Some(Some(*article)),
                Item::Verdict { .. } | Item::Moderation { .. } | Item::Trust { .. } | Item::Grant { .. } | Item::Revoked { .. } | Item::Deleted { .. } => Some(None),
                _ => None,
            },
            Record::Draft(d) => Some(Some(d.article)),
            Record::Proposal(_) | Record::Badge(_) => Some(None),
            Record::Image(_) | Record::Version(_) => None,
        };
        match record {
            Record::Control(control) => {
                control.verify(&self.world)?;
                self.waiting.remove(&id);
                if write {
                    self.store(&id, &raw, &Record::Control(control.clone()))?;
                }
                self.raw.insert(id.clone(), raw);
                self.arrivals.push(id.clone());
                self.arrived.entry(id.clone()).or_insert_with(now);
                let verdict = matches!(control.item, Item::Verdict { .. });
                self.index_control(&id, control);
                // A verdict may bring the keys of sealed blocks.
                if verdict {
                    self.opened.clear();
                }
            }
            Record::Image(entry) => {
                entry.verify(self.world.world_id.as_str(), self.gm())?;
                self.waiting.remove(&id);
                if write {
                    self.store(&id, &raw, &Record::Image(entry.clone()))?;
                }
                self.raw.insert(id.clone(), raw);
                self.arrivals.push(id.clone());
                self.arrived.entry(id.clone()).or_insert_with(now);
                self.images.insert(id.clone(), entry);
            }
            Record::Badge(badge) => {
                badge.verify(self.world.world_id.as_str())?;
                // Only a member's — whose grant may still be on its way.
                if !self.is_member(&badge.author) {
                    self.waiting.insert(id, Pending::Record(raw, Box::new(Record::Badge(badge)), write));
                    return Ok(Added::Waiting);
                }
                self.waiting.remove(&id);
                if write {
                    self.store(&id, &raw, &Record::Badge(badge.clone()))?;
                }
                self.raw.insert(id.clone(), raw);
                self.arrivals.push(id.clone());
                self.arrived.entry(id.clone()).or_insert_with(now);
                let member = badge.author.id.to_string();
                if self.badges.get(&member).is_none_or(|(_, b)| b.at <= badge.at) {
                    self.badges.insert(member, (id.clone(), badge));
                }
            }
            Record::Proposal(proposal) => {
                proposal.verify(self.world.world_id.as_str())?;
                // Only a member asks — whose grant may still be on its way.
                if !self.is_member(&proposal.author) {
                    self.waiting.insert(id, Pending::Record(raw, Box::new(Record::Proposal(proposal)), write));
                    return Ok(Added::Waiting);
                }
                self.waiting.remove(&id);
                if write {
                    self.store(&id, &raw, &Record::Proposal(proposal.clone()))?;
                }
                self.raw.insert(id.clone(), raw);
                self.arrivals.push(id.clone());
                self.arrived.entry(id.clone()).or_insert_with(now);
                self.proposals.insert(id.clone(), proposal);
            }
            Record::Draft(draft) => {
                if draft.world != self.world.world_id.as_str() {
                    return Err("a record of another world".into());
                }
                // Its version is here already: nothing to keep.
                if self.covered(&draft) {
                    self.waiting.remove(&id);
                    self.forget_file(&id, &Record::Draft(draft));
                    return Ok(Added::Known);
                }
                let Some(head) = self.head_of(draft.layer, draft.generation).cloned() else {
                    self.waiting.insert(id, Pending::Record(raw, Box::new(Record::Draft(draft)), write));
                    return Ok(Added::Waiting);
                };
                draft.verify(&head)?;
                if head.plain && !self.writes_plain(&draft.author) {
                    self.waiting.insert(id, Pending::Record(raw, Box::new(Record::Draft(draft)), write));
                    return Ok(Added::Waiting);
                }
                self.waiting.remove(&id);
                if write {
                    self.store(&id, &raw, &Record::Draft(draft.clone()))?;
                }
                self.raw.insert(id.clone(), raw);
                self.arrivals.push(id.clone());
                self.arrived.entry(id.clone()).or_insert_with(now);
                let key = (draft.layer, draft.generation);
                self.by_layer.entry(key).or_default().push(id.clone());
                self.drafts.insert(id.clone(), draft);
                self.apply_to_open(key, &id);
            }
            Record::Version(version) => {
                let attached = match self.check_version(&version)? {
                    Some(attached) => attached,
                    None => {
                        self.waiting.insert(id, Pending::Record(raw, Box::new(Record::Version(version)), write));
                        return Ok(Added::Waiting);
                    }
                };
                self.waiting.remove(&id);
                if write {
                    self.store(&id, &raw, &Record::Version(version.clone()))?;
                }
                let at = *self.arrived.entry(id.clone()).or_insert_with(now);
                for (draft_id, draft) in attached {
                    self.attached.insert(draft_id, draft);
                }
                let head = VersionHead::of(&version);
                self.indexed.insert(id.clone(), (head.clone(), at));
                self.dirty = true;
                self.full.borrow_mut().insert(id.clone(), std::sync::Arc::new(version));
                return self.admit(id, head);
            }
        }
        self.version += 1;
        match touches {
            Some(Some(article)) => self.touch(article),
            Some(None) => self.all_changed_at = self.version,
            None => {}
        }
        self.retry_waiting();
        Ok(Added::New(id))
    }

    fn touch(&mut self, article: ArticleId) {
        self.changed_at.insert(article, self.version);
    }

    /// The articles that changed since `version` — `None` when it could be
    /// any of them.
    pub fn changed_since(&self, version: u64) -> Option<HashSet<ArticleId>> {
        if self.all_changed_at > version || version > self.version {
            return None;
        }
        Some(self.changed_at.iter().filter(|(_, at)| **at > version).map(|(a, _)| *a).collect())
    }

    /// A version known from the index — checked when it first came.
    fn take_head(&mut self, id: String, head: VersionHead) -> Result<Added> {
        if self.versions.contains_key(&id) {
            return Ok(Added::Known);
        }
        if !self.head_ready(&head) {
            self.waiting.insert(id, Pending::Head(Box::new(head)));
            return Ok(Added::Waiting);
        }
        self.waiting.remove(&id);
        self.admit(id, head)
    }

    /// Whether the layer heads and versions a version builds on are here.
    fn head_ready(&self, head: &VersionHead) -> bool {
        head.parents.iter().all(|p| self.versions.contains_key(p) || self.deleted.contains(p))
            && head.blocks.iter().all(|(l, g, ..)| self.head_of(*l, *g).is_some())
            && head.drafts.iter().all(|d| self.attached.get(d).is_some_and(|d| self.head_of(d.layer, d.generation).is_some()))
    }

    /// Takes a checked version in: by layer, and in place of the author's
    /// drafts it holds.
    fn admit(&mut self, id: String, head: VersionHead) -> Result<Added> {
        let head_article = head.article;
        let mut keys: BTreeSet<(LayerId, u32)> = head.blocks.iter().map(|(l, g, ..)| (*l, *g)).collect();
        for d in &head.drafts {
            if let Some(draft) = self.attached.get(d) {
                keys.insert((draft.layer, draft.generation));
            }
        }
        for key in &keys {
            self.by_layer.entry(*key).or_default().push(id.clone());
        }
        // The author's drafts of the session are in here now.
        let gone: Vec<String> = self
            .drafts
            .iter()
            .filter(|(_, d)| {
                d.author.id == head.author.id && d.session == head.session && d.article == head.article && d.time <= head.until
            })
            .map(|(id, _)| id.clone())
            .collect();
        self.versions.insert(id.clone(), head);
        self.arrivals.push(id.clone());
        if gone.is_empty() {
            for key in keys {
                self.apply_to_open(key, &id);
            }
        } else {
            for draft in gone {
                self.drop_draft(&draft);
            }
            self.opened.clear();
        }
        self.version += 1;
        self.touch(head_article);
        self.retry_waiting();
        Ok(Added::New(id))
    }

    /// A version whole — read from its file the first time it is needed.
    pub fn version_full(&self, id: &str) -> Option<std::sync::Arc<Version>> {
        if let Some(v) = self.full.borrow().get(id) {
            return Some(v.clone());
        }
        let path = self.version_path(id)?;
        let raw = std::fs::read(path).ok()?;
        self.remember(id, &raw)
    }

    /// Keeps a version read whole in memory — if its bytes are what its id
    /// says.
    fn remember(&self, id: &str, raw: &[u8]) -> Option<std::sync::Arc<Version>> {
        if record_id(raw) != id || !self.versions.contains_key(id) {
            return None;
        }
        let Record::Version(version) = serde_json::from_slice(raw).ok()? else { return None };
        let version = std::sync::Arc::new(version);
        self.full.borrow_mut().insert(id.to_string(), version.clone());
        Some(version)
    }

    fn version_path(&self, id: &str) -> Option<PathBuf> {
        let head = self.versions.get(id)?;
        Some(self.dir.join("articles").join(dir_name(&head.article.to_string())).join(format!("{id}.json")))
    }

    /// The number of documents, without reading any of them.
    pub fn article_count(&self) -> usize {
        self.article_ids().len()
    }

    /// Takes in what waited for what just came.
    fn retry_waiting(&mut self) {
        loop {
            let ready: Vec<String> = self
                .waiting
                .iter()
                .filter(|(_, pending)| match pending {
                    Pending::Record(_, record, _) => self.ready(record),
                    Pending::Head(head) => self.head_ready(head),
                })
                .map(|(id, _)| id.clone())
                .collect();
            if ready.is_empty() {
                return;
            }
            let mut progress = false;
            for id in ready {
                let taken = match self.waiting.remove(&id) {
                    Some(Pending::Record(raw, record, write)) => self.take(raw, *record, write),
                    Some(Pending::Head(head)) => self.take_head(id, *head),
                    None => continue,
                };
                if let Ok(Added::New(_)) = taken {
                    progress = true;
                }
            }
            if !progress {
                return;
            }
        }
    }

    /// Whether what a record builds on is here.
    fn ready(&self, record: &Record) -> bool {
        match record {
            Record::Control(_) | Record::Image(_) => true,
            Record::Draft(d) => self.head_of(d.layer, d.generation).is_some(),
            Record::Version(v) => matches!(self.check_version(v), Ok(Some(_))),
            Record::Proposal(p) => self.is_member(&p.author),
            Record::Badge(b) => self.is_member(&b.author),
        }
    }

    /// Whether someone belongs to the world: granted keys, or its GM.
    fn is_member(&self, who: &IdentityPublic) -> bool {
        self.grant(who.id.as_str()).is_some() || self.gm().is_some_and(|gm| gm.id == who.id)
    }

    /// Whose writing in an unencrypted part counts — where anyone holding the
    /// world could write: its members, and the servers the GM named, which
    /// write in members' names.
    fn writes_plain(&self, who: &IdentityPublic) -> bool {
        self.is_member(who) || self.hosts().iter().any(|h| h == who.id.as_str())
    }

    /// Checks a version: the author's signature, each block's write key and
    /// each draft inside it. `None` while a layer head or a version it builds
    /// on is not here yet. Returns the drafts inside, by id.
    fn check_version(&self, version: &Version) -> Result<Option<Vec<(String, Draft)>>> {
        if version.world != self.world.world_id.as_str() {
            return Err("a record of another world".into());
        }
        if !version.author.is_genuine() {
            return Err("an author whose id does not match their keys".into());
        }
        version.author.verify(&version.signed_bytes()?, &signature(&version.signature)?)?;
        if version.parents.iter().any(|p| !self.versions.contains_key(p) && !self.deleted.contains(p)) {
            return Ok(None);
        }
        for block in &version.blocks {
            let Some(head) = self.head_of(block.layer, block.generation) else { return Ok(None) };
            if head.article != version.article {
                return Err("a block of another document".into());
            }
            verify_write(&head.writer, &block.signed_bytes(&version.world, &version.article)?, &signature(&block.write_signature)?)?;
            // Unencrypted, the write key lies open: only a member's writing
            // counts — whose grant may still be on its way.
            if head.plain && !self.writes_plain(&version.author) {
                return Ok(None);
            }
        }
        let mut attached = Vec::new();
        for raw in &version.drafts {
            let draft: Draft = serde_json::from_str(raw)?;
            if draft.world != version.world || draft.article != version.article {
                return Err("a draft of another document".into());
            }
            let Some(head) = self.head_of(draft.layer, draft.generation) else { return Ok(None) };
            draft.verify(head)?;
            if head.plain && !self.writes_plain(&draft.author) {
                return Ok(None);
            }
            attached.push((record_id(raw.as_bytes()), draft));
        }
        Ok(Some(attached))
    }

    /// Whether a draft's version is here: written by its author for its
    /// session, holding everything up to it.
    fn covered(&self, draft: &Draft) -> bool {
        self.versions.values().any(|v| {
            v.author.id == draft.author.id && v.session == draft.session && v.article == draft.article && draft.time <= v.until
        })
    }

    /// A draft its version holds now: out of memory, and off the disk.
    fn drop_draft(&mut self, id: &str) {
        let Some(draft) = self.drafts.remove(id) else { return };
        self.raw.remove(id);
        if let Some(ids) = self.by_layer.get_mut(&(draft.layer, draft.generation)) {
            ids.retain(|i| i != id);
        }
        self.forget_file(id, &Record::Draft(draft));
    }

    /// Removes a record's file — a draft its version holds, or what the game
    /// master deleted.
    fn forget_file(&mut self, id: &str, record: &Record) {
        if let Some(path) = self.path_of(id, record) {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            self.read.remove(&path);
            self.listed(&path, id, false);
        }
    }

    /// One's own file written or removed: the folder's listing follows, so
    /// it need not be read again for it.
    fn listed(&mut self, path: &Path, id: &str, there: bool) {
        let Some(folder) = path.parent() else { return };
        let Some((stamp, ids)) = self.listings.get_mut(folder) else { return };
        if there {
            ids.insert(id.to_string());
        } else {
            ids.remove(id);
        }
        if let Ok(now) = std::fs::metadata(folder).and_then(|m| m.modified()) {
            *stamp = now;
        }
    }

    /// `listed` for a record known to be new.
    fn listed_new(&mut self, path: &Path, id: &str) {
        let Some(folder) = path.parent() else { return };
        let Some((stamp, ids)) = self.listings.get_mut(folder) else { return };
        ids.insert(id.to_string());
        if let Ok(now) = std::fs::metadata(folder).and_then(|m| m.modified()) {
            *stamp = now;
        }
    }

    fn path_of(&self, id: &str, record: &Record) -> Option<PathBuf> {
        Some(match place(id, record) {
            Some((folders, name)) => folders.iter().fold(self.dir.clone(), |p, f| p.join(f)).join(name),
            None => self.drafts_dir.join(format!("{id}.json")),
        })
    }

    fn store(&mut self, id: &str, raw: &[u8], record: &Record) -> Result<()> {
        let path = self.path_of(id, record).ok_or("a record without a place")?;
        if let Some(batch) = &mut self.batch {
            batch.push((path.clone(), raw.to_vec(), id.to_string()));
            self.read.insert(path);
            return Ok(());
        }
        crate::durable::write(&path, raw)?;
        self.listed(&path, id, true);
        self.read.insert(path);
        Ok(())
    }

    fn index_control(&mut self, id: &str, control: Control) {
        let seq = control.seq;
        self.max_seq = self.max_seq.max(seq);
        let key = control.item.key();
        if self.latest.get(&key).is_none_or(|(s, _)| *s < seq) {
            self.latest.insert(key, (seq, id.to_string()));
        }
        if let Item::Verdict { accepted, versions, keys, .. } = &control.item {
            for id in versions {
                if self.verdicts.get(id).is_none_or(|(s, _)| *s < seq) {
                    self.verdicts.insert(id.clone(), (seq, *accepted));
                }
            }
            if *accepted {
                self.revealed.extend(keys.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        if let Item::Layer(head) = &control.item {
            let at = (head.layer, head.generation);
            if self.generations.get(&at).is_none_or(|(s, _)| *s < seq) {
                self.generations.insert(at, (seq, id.to_string()));
            }
            let born = self.born.entry(head.layer).or_insert(seq);
            *born = (*born).min(seq);
            self.article_layers.entry(head.article).or_default().insert(head.layer);
        }
        self.controls.insert(id.to_string(), control);
    }

    fn item(&self, key: &str) -> Option<&Control> {
        self.latest.get(key).and_then(|(_, id)| self.controls.get(id))
    }

    fn head_of(&self, layer: LayerId, generation: u32) -> Option<&LayerHead> {
        let (_, id) = self.generations.get(&(layer, generation))?;
        match &self.controls.get(id)?.item {
            Item::Layer(head) => Some(head),
            _ => None,
        }
    }

    /// The current head of a layer.
    pub fn head(&self, layer: LayerId) -> Option<&LayerHead> {
        match &self.item(&format!("layer {layer}"))?.item {
            Item::Layer(head) => Some(head),
            _ => None,
        }
    }

    fn removed(&self, article: ArticleId, layer: Option<LayerId>) -> bool {
        self.latest.contains_key(&format!("removed {article}"))
            || layer.is_some_and(|l| self.latest.contains_key(&format!("removed {l}")))
    }

    // --- The world's structure ---------------------------------------------

    pub fn world_item(&self) -> Option<(&WorldPublic, &WorldCalendar, &str, &str, &IdentityPublic)> {
        match &self.item("world")?.item {
            Item::World { public, calendar, icon, home, gm } => Some((public, calendar, icon, home, gm)),
            _ => None,
        }
    }

    /// The types, as far as this device may see them.
    pub fn templates(&self) -> Option<Vec<Template>> {
        match &self.item("templates")?.item {
            Item::Templates { frame, pieces } => types::open(frame, pieces, self.keys.as_ref()?),
            _ => None,
        }
    }

    pub fn grant(&self, member: &str) -> Option<(&IdentityPublic, &Sealed)> {
        match &self.item(&format!("grant {member}"))?.item {
            Item::Grant { member, sealed } => Some((member, sealed)),
            _ => None,
        }
    }

    /// What a member's grant holds — for the member, or the game master who
    /// wrote it.
    pub fn grant_content(&self, member: &str, secret: &x25519_dalek::StaticSecret) -> Option<GrantContent> {
        serde_json::from_slice(&self.grant(member)?.1.open(secret)?).ok()
    }

    /// The current signed admissions, excluding revoked members.
    pub fn granted_members(&self) -> Vec<IdentityPublic> {
        self.latest.keys().filter_map(|key| key.strip_prefix("grant "))
            .filter_map(|id| self.grant(id).map(|(member, _)| member.clone())).collect()
    }

    /// How many versions of each document stay.
    pub fn version_limit(&self) -> u32 {
        match self.item("versions").map(|c| &c.item) {
            Some(Item::Versions { max }) => *max,
            _ => VERSION_LIMIT,
        }
    }

    /// The side bar's entries, as far as this device may see them — `None`
    /// while the game master has set none.
    pub fn navigation(&self) -> Option<Vec<aw_model::navigation::NavEntry>> {
        match &self.item("navigation")?.item {
            Item::Navigation { players, gm } => {
                let held = self.keys.as_ref()?;
                gm.open(held).or_else(|| players.open(held))
            }
            _ => None,
        }
    }

    /// Whether the world is encrypted.
    pub fn encrypted(&self) -> bool {
        !matches!(self.item("encryption").map(|c| &c.item), Some(Item::Encryption { on: false }))
    }

    /// Whether players keep a copy: an encrypted world whose GM did not say
    /// otherwise. An unencrypted one stays with its host — a copy would give
    /// everyone everything.
    pub fn copies(&self) -> bool {
        self.encrypted() && !matches!(self.item("copies").map(|c| &c.item), Some(Item::Copies { players: false }))
    }

    /// The servers the GM lets write in members' names — see `Item::Hosts`.
    pub fn hosts(&self) -> Vec<String> {
        match self.item("hosts").map(|c| &c.item) {
            Some(Item::Hosts { ids }) => ids.clone(),
            _ => Vec::new(),
        }
    }

    /// Whom a session was written for: by the GM or a host they named, in a
    /// member's name, that member (`<member id>:<session>`); otherwise its
    /// author. Anyone else's session is their own, whatever it says.
    pub fn written_for<'a>(&self, author: &'a str, session: &'a str) -> &'a str {
        let vouched = self.gm().is_some_and(|gm| gm.id.as_str() == author) || self.hosts().iter().any(|h| h == author);
        let member = session.strip_prefix("id:").and_then(|rest| rest.split_once(':')).map(|(body, _)| &session[..3 + body.len()]);
        match member {
            Some(m) if vouched => m,
            _ => author,
        }
    }

    /// The place in the journal of the record that last switched encryption.
    pub fn encryption_switched_at(&self) -> Option<u64> {
        self.item("encryption").map(|c| c.seq)
    }

    /// The place in the journal of the record that began a layer's generation.
    pub fn generation_seq(&self, layer: LayerId, generation: u32) -> Option<u64> {
        self.generations.get(&(layer, generation)).map(|(seq, _)| *seq)
    }

    /// The dice the world rolls with, by sides — empty for the usual set.
    pub fn dice(&self) -> Vec<u32> {
        match self.item("dice").map(|c| &c.item) {
            Some(Item::Dice { sides }) => sides.clone(),
            _ => Vec::new(),
        }
    }

    // --- Review ------------------------------------------------------------

    /// Whose device this is: their own waiting versions show here, and what
    /// is sealed for them opens.
    pub fn set_me(&mut self, me: &Identity) {
        if self.me.as_ref().map(|(id, _)| id.as_str()) != Some(me.id().as_str()) {
            self.me = Some((me.id().to_string(), me.seal_secret().clone()));
            self.opened.clear();
            self.version += 1;
            self.all_changed_at = self.version;
        }
    }

    fn me_id(&self) -> Option<&str> {
        self.me.as_ref().map(|(id, _)| id.as_str())
    }

    /// Whether players' writing waits for the game master.
    pub fn moderated(&self) -> bool {
        matches!(self.item("moderation").map(|c| &c.item), Some(Item::Moderation { on: true, .. }))
    }

    /// Whether a member's writing counts without waiting.
    pub fn trusted(&self, member: &str) -> bool {
        matches!(self.item(&format!("trust {member}")).map(|c| &c.item), Some(Item::Trust { trusted: true, .. }))
    }

    /// How each member is shown — the newest badge of each.
    pub fn badges(&self) -> impl Iterator<Item = &Badge> {
        self.badges.values().map(|(_, b)| b)
    }

    /// New documents members asked for, by the proposal's id.
    pub fn proposals(&self) -> impl Iterator<Item = (&String, &Proposal)> {
        self.proposals.iter()
    }

    fn gm(&self) -> Option<&IdentityPublic> {
        self.world_item().map(|(.., gm)| gm)
    }

    fn is_gm(&self) -> bool {
        self.gm().is_some_and(|gm| Some(gm.id.as_str()) == self.me_id())
    }

    /// Whether what `author` writes now waits for the game master.
    pub fn waits(&self, author: &str) -> bool {
        self.moderated() && self.gm().is_none_or(|gm| gm.id.as_str() != author) && !self.trusted(author)
    }

    /// Where a session of `author` stands, verdicts aside.
    fn review_of(&self, author: &str, session: &str) -> Review {
        let Some(Item::Moderation { on: true, settled }) = self.item("moderation").map(|c| &c.item) else {
            return Review::Counted;
        };
        // A server writes in a member's name: theirs to wait or not. The GM's
        // own device, acting as host, vouches as GM.
        let author = if self.hosts().iter().any(|h| h == author) { self.written_for(author, session) } else { author };
        if self.gm().is_some_and(|gm| gm.id.as_str() == author) || settled.iter().any(|s| s == session) {
            return Review::Counted;
        }
        match self.item(&format!("trust {author}")).map(|c| &c.item) {
            Some(Item::Trust { trusted: true, .. }) => Review::Counted,
            Some(Item::Trust { settled, .. }) if settled.iter().any(|s| s == session) => Review::Counted,
            _ => Review::Waiting,
        }
    }

    /// Where a version or a draft stands with the game master.
    pub fn record_review(&self, id: &str) -> Review {
        if let Some(v) = self.versions.get(id) {
            if let Some((_, accepted)) = self.verdicts.get(id) {
                return if *accepted { Review::Counted } else { Review::Rejected };
            }
            return self.review_of(v.author.id.as_str(), &v.session);
        }
        if let Some(d) = self.drafts.get(id) {
            return self.review_of(d.author.id.as_str(), &d.session);
        }
        Review::Counted
    }

    fn record_author(&self, id: &str) -> Option<&str> {
        self.versions
            .get(id)
            .map(|v| v.author.id.as_str())
            .or_else(|| self.drafts.get(id).map(|d| d.author.id.as_str()))
    }

    /// Where a version stands: waiting while any of it waits, turned down
    /// when what was decided of it was turned down.
    pub fn session_review(&self, session: &str) -> Review {
        let mut out = Review::Counted;
        let ids = self
            .versions
            .iter()
            .filter(|(_, v)| v.session == session)
            .map(|(id, _)| id)
            .chain(self.drafts.iter().filter(|(_, d)| d.session == session).map(|(id, _)| id));
        for id in ids {
            match self.record_review(id) {
                Review::Waiting => return Review::Waiting,
                Review::Rejected => out = Review::Rejected,
                Review::Counted => {}
            }
        }
        out
    }

    /// Whether this device leaves a record out of the text: what was turned
    /// down, and what still waits — except for its author, who sees it.
    fn hidden(&self, id: &str) -> bool {
        match self.record_review(id) {
            Review::Counted => false,
            Review::Rejected => true,
            Review::Waiting => self.record_author(id) != self.me_id(),
        }
    }

    /// The sessions written by players — or by one — so far: what stands
    /// when moderation starts, or when trust is taken.
    pub fn sessions_by(&self, author: Option<&str>) -> Vec<String> {
        let gm = self.gm().map(|g| g.id.to_string());
        let mine = |a: &IdentityPublic| Some(a.id.to_string()) != gm && author.is_none_or(|x| a.id.as_str() == x);
        let set: BTreeSet<String> = self
            .versions
            .values()
            .filter(|v| mine(&v.author))
            .map(|v| v.session.clone())
            .chain(self.drafts.values().filter(|d| mine(&d.author)).map(|d| d.session.clone()))
            .collect();
        set.into_iter().collect()
    }

    /// The versions that wait, by article: on the game master's device all
    /// that players wrote, on a player's their own.
    pub fn waiting(&self) -> Vec<(ArticleId, String)> {
        let me = self.me_id();
        let is_gm = self.is_gm();
        let set: BTreeSet<(ArticleId, String)> = self
            .versions
            .iter()
            .filter(|(id, v)| (is_gm || Some(v.author.id.as_str()) == me) && self.record_review(id) == Review::Waiting)
            .map(|(_, v)| (v.article, v.session.clone()))
            .collect();
        set.into_iter().collect()
    }

    /// The waiting versions of a session.
    pub fn waiting_versions(&self, session: &str) -> Vec<String> {
        self.versions
            .iter()
            .filter(|(id, v)| v.session == session && self.record_review(id) == Review::Waiting)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The keys of the sealed blocks of versions, for a verdict that takes
    /// them in: each encrypted with its layer's data key. Only who the seal
    /// was made for — the game master — can do this.
    pub fn reveal(&mut self, versions: &[String]) -> Result<BTreeMap<String, String>> {
        let secret = self.me.as_ref().ok_or("no identity here")?.1.clone();
        let keys = self.keys.clone().ok_or("no keys")?;
        let mut out = BTreeMap::new();
        for id in versions {
            let Some(version) = self.version_full(id) else { continue };
            for block in &version.blocks {
                let Some(seal) = &block.seal else { continue };
                let key = seal.iter().find_map(|k| unseal_key(&secret, k).ok()).ok_or("this was not sealed for you")?;
                let head = self.head_of(block.layer, block.generation).ok_or("there is no such layer")?;
                let dek = head.data_key(&keys)?.ok_or("this part cannot be opened here")?;
                out.insert(block.id(), dek.seal(key.as_bytes())?);
            }
        }
        Ok(out)
    }

    /// What a document is — for whoever can open its base layer.
    fn about_of(&self, head: &LayerHead) -> Option<About> {
        let dek = head.data_key(self.keys.as_ref()?).ok()??;
        serde_json::from_slice(&dek.open(head.about.as_ref()?).ok()?).ok()
    }

    /// What a document is, from its current base layer.
    pub fn about(&self, article: ArticleId) -> Option<About> {
        let base = self.layers_of(article).into_iter().find(|h| h.is_base)?;
        let key = (base.layer, base.generation);
        if let Some(known) = self.abouts.borrow().get(&key) {
            return known.clone();
        }
        let about = self.about_of(base);
        self.abouts.borrow_mut().insert(key, about.clone());
        about
    }

    /// The layers of an article that exist now, in the order they came.
    fn layers_of(&self, article: ArticleId) -> Vec<&LayerHead> {
        let Some(layers) = self.article_layers.get(&article) else { return Vec::new() };
        let mut heads: Vec<&LayerHead> = layers
            .iter()
            .filter_map(|layer| self.head(*layer))
            .filter(|h| h.article == article && !self.removed(article, Some(h.layer)))
            .collect();
        heads.sort_by_key(|h| (!h.is_base, self.born.get(&h.layer).copied().unwrap_or(0)));
        heads
    }

    fn article_ids(&self) -> Vec<(ArticleId, String, String)> {
        let mut out: Vec<(u64, ArticleId, String, String)> = self
            .latest
            .iter()
            .filter(|(k, _)| k.starts_with("article "))
            .filter_map(|(_, (seq, id))| match &self.controls.get(id)?.item {
                Item::Article { article } if !self.removed(*article, None) => {
                    let about = self.about(*article).unwrap_or_default();
                    Some((*seq, *article, about.article_type, about.slug))
                }
                _ => None,
            })
            .collect();
        out.sort_by_key(|(seq, ..)| *seq);
        out.into_iter().map(|(_, a, t, s)| (a, t, s)).collect()
    }

    /// The game master's next count.
    pub fn next_seq(&self) -> u64 {
        self.max_seq + 1
    }

    // --- Keys and documents -------------------------------------------------

    /// The keys this device opens layers with. A change of keys — a new
    /// grant, say — opens everything afresh.
    pub fn set_keys(&mut self, keys: HeldKeys) {
        if self.keys.as_ref() != Some(&keys) {
            self.keys = Some(keys);
            self.opened.clear();
            self.abouts.borrow_mut().clear();
            self.version += 1;
            self.all_changed_at = self.version;
        }
    }

    fn open_layer(&mut self, layer: LayerId, generation: u32) -> Option<&mut Opened> {
        let key = (layer, generation);
        if !self.opened.contains_key(&key) {
            let head = self.head_of(layer, generation)?.clone();
            let dek = head.data_key(self.keys.as_ref()?).ok()??;
            let access: Access = serde_json::from_slice(&dek.open(&head.access).ok()?).ok()?;
            let genesis = dek.open(&head.genesis).ok()?;
            let mut doc = LayerDoc::load(&genesis).ok()?;
            let genesis = doc.doc.get_changes(&[]).iter().map(|c| c.hash()).collect();
            self.opened.insert(
                key,
                Opened { doc, dek, access, genesis, applied: HashMap::new(), visible: None, index: None },
            );
            for id in self.to_apply(key) {
                self.apply_to_open(key, &id);
            }
        }
        self.opened.get_mut(&key)
    }

    /// What opening a layer takes in: every draft, and of the versions only
    /// the newest — each holds what came before — where their blocks checked
    /// out here before. Otherwise every version, in order, so each is checked
    /// against what it builds on.
    fn to_apply(&self, key: (LayerId, u32)) -> Vec<String> {
        let all = self.by_layer.get(&key).cloned().unwrap_or_default();
        let (newest, checked) = self.newest_in(key);
        if !checked {
            return all;
        }
        all.into_iter().filter(|id| !self.versions.contains_key(id) || newest.contains(id)).collect()
    }

    /// A layer's newest versions — none builds on them there — and whether
    /// their blocks checked out here before.
    fn newest_in(&self, key: (LayerId, u32)) -> (HashSet<String>, bool) {
        let all = self.by_layer.get(&key).cloned().unwrap_or_default();
        let versions: Vec<&String> = all.iter().filter(|id| self.versions.contains_key(*id)).collect();
        let mut below: HashSet<String> = HashSet::new();
        for id in &versions {
            let mut open: Vec<String> = self.versions[*id].parents.clone();
            while let Some(p) = open.pop() {
                if below.insert(p.clone()) {
                    if let Some(v) = self.versions.get(&p) {
                        open.extend(v.parents.iter().cloned());
                    }
                }
            }
        }
        let newest: HashSet<String> = versions.iter().filter(|id| !below.contains(**id)).map(|id| (*id).clone()).collect();
        let checked = newest.iter().all(|id| {
            let head = &self.versions[id];
            head.drafts.is_empty()
                && head.blocks.iter().filter(|(l, g, ..)| (*l, *g) == key).all(|(_, _, block, _)| self.trusted.contains(block))
        });
        (newest, checked)
    }

    /// The key a sealed block or draft opens with: from a verdict, or for
    /// whoever it was sealed for.
    fn seal_key(&self, seal: &[WrappedKey], revealed: Option<&String>, dek: &DataKey) -> Option<ContentKey> {
        let from_verdict = revealed.and_then(|k| {
            let raw: [u8; 32] = dek.open(k).ok()?.try_into().ok()?;
            Some(ContentKey::from_bytes(raw))
        });
        from_verdict.or_else(|| self.me.as_ref().and_then(|(_, secret)| seal.iter().find_map(|k| unseal_key(secret, k).ok())))
    }

    /// Decrypts a draft's changes — and checks they are its author's own.
    fn draft_changes(&self, draft: &Draft, dek: &DataKey) -> Result<Vec<Change>> {
        let key = match &draft.seal {
            None => dek.clone(),
            Some(seal) => DataKey::Sealed(self.seal_key(seal, None, dek).ok_or("sealed for others")?),
        };
        let list: Vec<String> = serde_json::from_slice(&key.open(&draft.changes)?)?;
        let prefix = layer::author_prefix(draft.author.id.as_str());
        let mut out = Vec::new();
        for hex_change in list {
            let change = Change::from_bytes(hex::decode(hex_change)?)?;
            if !change.actor_id().to_bytes().starts_with(&prefix) {
                return Err("a change under someone else's name".into());
            }
            out.push(change);
        }
        Ok(out)
    }

    /// A block's changes, decrypted.
    fn block_changes(&self, block: &Block, dek: &DataKey) -> Result<Vec<Change>> {
        let key = match &block.seal {
            None => dek.clone(),
            Some(seal) => DataKey::Sealed(self.seal_key(seal, self.revealed.get(&block.id()), dek).ok_or("sealed for others")?),
        };
        let bytes = key.open(&block.state)?;
        Ok(AutoCommit::load(&bytes)?.get_changes(&[]))
    }

    /// The changes a record brings to a layer — a draft's, or a version's
    /// drafts and blocks there. A version may only bring changes nobody
    /// else signed if they are its author's own; a block that does not keep
    /// to that stays out.
    fn record_changes(&mut self, key: (LayerId, u32), id: &str) -> Vec<Change> {
        let Some(opened) = self.opened.get(&key) else { return Vec::new() };
        let dek = opened.dek.clone();
        if let Some(draft) = self.drafts.get(id) {
            return self.draft_changes(draft, &dek).unwrap_or_default();
        }
        let Some(version) = self.version_full(id) else { return Vec::new() };
        let mut out: Vec<Change> = Vec::new();
        for raw in &version.drafts {
            let Some(draft) = self.attached.get(&record_id(raw.as_bytes())) else { continue };
            if (draft.layer, draft.generation) == key {
                out.extend(self.draft_changes(draft, &dek).unwrap_or_default());
            }
        }
        // Building on something deleted, the author's word has to do.
        let lenient = version.parents.iter().any(|p| self.deleted.contains(p));
        let prefix = layer::author_prefix(version.author.id.as_str());
        let signed: HashSet<ChangeHash> = out.iter().map(|c| c.hash()).collect();
        for block in version.blocks.iter().filter(|b| (b.layer, b.generation) == key) {
            let Ok(changes) = self.block_changes(block, &dek) else { continue };
            let opened = self.opened.get_mut(&key).expect("open");
            let foreign = changes.iter().any(|c| {
                !signed.contains(&c.hash())
                    && opened.doc.doc.get_change_by_hash(&c.hash()).is_none()
                    && !c.actor_id().to_bytes().starts_with(&prefix)
            });
            if foreign && !lenient {
                continue;
            }
            if self.trusted.insert(block.id()) {
                self.dirty = true;
            }
            out.extend(changes);
        }
        out
    }

    fn apply_to_open(&mut self, key: (LayerId, u32), id: &str) {
        match self.opened.get(&key) {
            Some(opened) if !opened.applied.contains_key(id) => {}
            _ => return,
        }
        let changes = self.record_changes(key, id);
        let Some(opened) = self.opened.get_mut(&key) else { return };
        let hashes = changes.iter().map(|c| c.hash()).collect();
        if opened.doc.apply(changes).is_ok() {
            opened.applied.insert(id.to_string(), hashes);
        }
    }

    /// The heads of what this device shows of an open layer — `None` when
    /// that is everything it holds.
    fn visible(&mut self, layer: LayerId, generation: u32) -> Option<Vec<ChangeHash>> {
        let version = self.version;
        if let Some((v, heads)) = self.opened.get(&(layer, generation))?.visible.as_ref() {
            if *v == version {
                return heads.clone();
            }
        }
        let ids: Vec<String> = self.by_layer.get(&(layer, generation)).cloned().unwrap_or_default();
        let (hidden, counted): (Vec<String>, Vec<String>) = ids.into_iter().partition(|id| self.hidden(id));
        let opened = self.opened.get_mut(&(layer, generation))?;
        let heads = if hidden.is_empty() {
            None
        } else {
            // What only hidden records bring stays out; what a counted one
            // brings as well stays in.
            let kept_by: HashSet<ChangeHash> = counted
                .iter()
                .filter_map(|id| opened.applied.get(id))
                .flatten()
                .chain(opened.genesis.iter())
                .copied()
                .collect();
            let out: HashSet<ChangeHash> = hidden
                .iter()
                .filter_map(|id| opened.applied.get(id))
                .flatten()
                .copied()
                .filter(|h| !kept_by.contains(h))
                .collect();
            if out.is_empty() {
                None
            } else {
                let kept: HashSet<ChangeHash> =
                    opened.doc.doc.get_changes(&[]).iter().map(|c| c.hash()).filter(|h| !out.contains(h)).collect();
                Some(opened.doc.tips(&kept))
            }
        };
        opened.visible = Some((version, heads.clone()));
        heads
    }

    /// The heads of a layer as this device shows it.
    fn shown_heads(&mut self, layer: LayerId, generation: u32) -> Option<Vec<ChangeHash>> {
        self.open_layer(layer, generation)?;
        let visible = self.visible(layer, generation);
        let opened = self.opened.get_mut(&(layer, generation))?;
        Some(visible.unwrap_or_else(|| opened.doc.doc.get_heads()))
    }

    /// A layer's content as this device shows it.
    fn shown(&mut self, layer: LayerId, generation: u32) -> Option<Plain> {
        self.open_layer(layer, generation)?;
        let visible = self.visible(layer, generation);
        let opened = self.opened.get_mut(&(layer, generation))?;
        match visible {
            None => opened.doc.plain().ok(),
            Some(heads) => opened.doc.plain_at(&heads).ok(),
        }
    }

    /// A layer's current generation and the heads this device shows.
    pub fn shown_state(&mut self, layer: LayerId) -> Option<(u32, Vec<ChangeHash>)> {
        let generation = self.head(layer)?.generation;
        Some((generation, self.shown_heads(layer, generation)?))
    }

    /// A layer's content at exactly these heads.
    pub fn plain_of(&mut self, layer: LayerId, generation: u32, heads: &[ChangeHash]) -> Result<Plain> {
        self.open_layer(layer, generation).ok_or("this part cannot be opened here")?.doc.plain_at(heads)
    }

    /// What lies between two states of a layer's text in one generation —
    /// gone from the left, new in the right — by the characters themselves.
    pub fn marks(&mut self, layer: LayerId, generation: u32, left: &[ChangeHash], right: &[ChangeHash]) -> Result<(Vec<diff::Range>, Vec<diff::Range>)> {
        self.open_layer(layer, generation).ok_or("this part cannot be opened here")?.doc.marks(left, right)
    }

    /// The whole world as this device sees it.
    pub fn articles(&mut self) -> Vec<ArticleState> {
        self.article_ids().into_iter().map(|(article, article_type, slug)| self.article_state(article, article_type, slug)).collect()
    }

    /// The articles there are, in order: id, type and slug.
    pub fn article_list(&self) -> Vec<(ArticleId, String, String)> {
        self.article_ids()
    }

    /// One article as this device sees it.
    pub fn article_state(&mut self, article: ArticleId, article_type: String, slug: String) -> ArticleState {
        let heads: Vec<LayerHead> = self.layers_of(article).into_iter().cloned().collect();
        let mut layers = Vec::new();
        for head in heads {
            let plain = self.shown(head.layer, head.generation);
            let open = self.opened.get(&(head.layer, head.generation)).and_then(|o| {
                Some((o.dek.clone(), o.access.clone(), plain?))
            });
            layers.push(LayerState { head, open });
        }
        ArticleState { article, article_type, slug, layers }
    }

    /// Everything an article's state as shown here hangs on, as one hash —
    /// without opening any of it: its type and slug, each layer's head,
    /// every record taken in there and whether it counts, the keys verdicts
    /// revealed for its blocks, and which keys this device holds. Equal
    /// hashes, equal state: what was built once need not be built again.
    pub fn article_fingerprint(&self, article: ArticleId, article_type: &str, slug: &str) -> String {
        let mut h = blake3::Hasher::new();
        h.update(article_type.as_bytes());
        h.update(b"\0");
        h.update(slug.as_bytes());
        let mut held: Vec<String> = self.keys.as_ref().map(|k| k.iter().map(|(at, _)| at.to_string()).collect()).unwrap_or_default();
        held.sort();
        for at in &held {
            h.update(b"\0k");
            h.update(at.as_bytes());
        }
        for head in self.layers_of(article) {
            let key = (head.layer, head.generation);
            h.update(b"\0l");
            if let Some((_, id)) = self.latest.get(&format!("layer {}", head.layer)) {
                h.update(id.as_bytes());
            }
            h.update(&head.generation.to_le_bytes());
            let mut ids = self.by_layer.get(&key).cloned().unwrap_or_default();
            ids.sort();
            for id in &ids {
                h.update(id.as_bytes());
                h.update(if self.hidden(id) { b"-" } else { b"+" });
                if let Some(v) = self.versions.get(id) {
                    for (l, g, block, _) in &v.blocks {
                        if (*l, *g) == key {
                            if let Some(revealed) = self.revealed.get(block) {
                                h.update(revealed.as_bytes());
                            }
                        }
                    }
                }
            }
        }
        h.finalize().to_hex().to_string()
    }

    /// A layer's generation and its current heads — what an editor starts
    /// from.
    pub fn state(&mut self, layer: LayerId) -> Option<(u32, Vec<String>)> {
        let (generation, heads) = self.shown_state(layer)?;
        Some((generation, layer::texts(&heads)))
    }

    /// Who could see and edit a layer in a generation, and how far it went.
    pub fn access_at(&mut self, layer: LayerId, generation: u32) -> Result<(Access, Level)> {
        let level = self.head_of(layer, generation).ok_or("there is no such layer")?.level;
        let opened = self.open_layer(layer, generation).ok_or("this part cannot be opened here")?;
        Ok((opened.access.clone(), level))
    }

    /// A layer's content as it stood at `heads` of `generation` — what an
    /// editor saw when it started.
    pub fn plain_at(&mut self, layer: LayerId, generation: u32, heads: &[String]) -> Result<Plain> {
        let heads = layer::hashes(heads)?;
        if heads.is_empty() {
            return self.shown(layer, generation).ok_or_else(|| "this part cannot be opened here".into());
        }
        self.open_layer(layer, generation).ok_or("this part cannot be opened here")?.doc.plain_at(&heads)
    }

    /// A layer's content as it stands — for a new generation's start.
    pub fn plain(&mut self, layer: LayerId) -> Result<Plain> {
        let generation = self.head(layer).ok_or("there is no such layer")?.generation;
        self.shown(layer, generation).ok_or_else(|| "this part cannot be opened here".into())
    }

    /// A stable place in a layer's text, from a position (UTF-16) — for
    /// showing others where one writes.
    pub fn cursor(&mut self, layer: LayerId, position: usize) -> Result<String> {
        let generation = self.head(layer).ok_or("there is no such layer")?.generation;
        self.open_layer(layer, generation).ok_or("this part cannot be opened here")?.doc.cursor(position)
    }

    /// Where a stable place stands in a layer's text now.
    pub fn position(&mut self, layer: LayerId, cursor: &str) -> Result<usize> {
        let generation = self.head(layer).ok_or("there is no such layer")?.generation;
        self.open_layer(layer, generation).ok_or("this part cannot be opened here")?.doc.position(cursor)
    }

    // --- Images -------------------------------------------------------------

    /// The images, each as its newest entry says: (image id, entry id, entry).
    pub fn images(&self) -> Vec<(String, String, &crate::media::ImageEntry)> {
        let mut newest: BTreeMap<&str, (&String, &crate::media::ImageEntry)> = BTreeMap::new();
        for (id, entry) in &self.images {
            match newest.get(entry.image.as_str()) {
                Some((_, e)) if (e.time, &e.signature) >= (entry.time, &entry.signature) => {}
                _ => {
                    newest.insert(entry.image.as_str(), (id, entry));
                }
            }
        }
        newest.into_iter().map(|(image, (id, e))| (image.to_string(), id.clone(), e)).collect()
    }

    /// Every entry about one image, by id.
    pub fn image_entries(&self, image: &str) -> Vec<String> {
        self.images.iter().filter(|(_, e)| e.image == image).map(|(id, _)| id.clone()).collect()
    }

    /// The folders of the gallery — for the game master, who sealed them.
    pub fn gallery_folders(&self) -> Vec<crate::media::FolderNode> {
        let Some(Item::Gallery { sealed }) = self.item("gallery").map(|c| &c.item) else { return Vec::new() };
        let Some((_, secret)) = &self.me else { return Vec::new() };
        sealed.open(secret).and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
    }

    /// What this device may learn of an image from the gallery — as game
    /// master, or as the one who uploaded it.
    pub fn image_meta(&self, image: &str) -> Option<crate::media::ImageMeta> {
        let entry = self
            .images
            .values()
            .filter(|e| e.image == image)
            .max_by(|a, b| (a.time, &a.signature).cmp(&(b.time, &b.signature)))?;
        self.entry_meta(entry)
    }

    /// What this device may learn of an image from one of its entries.
    pub fn entry_meta(&self, entry: &crate::media::ImageEntry) -> Option<crate::media::ImageMeta> {
        entry.meta(&self.me.as_ref()?.1)
    }

    /// Every gallery entry, by its record id.
    pub fn image_records(&self) -> impl Iterator<Item = (&String, &crate::media::ImageEntry)> {
        self.images.iter()
    }

    /// Every part any image needs — what each device should have. Deleted
    /// entries are gone from here already; a part deleted with them counts
    /// again where a living entry names it.
    pub fn needed_parts(&self) -> BTreeSet<String> {
        self.images.values().flat_map(|e| e.parts.iter().chain(&e.preview).cloned()).collect()
    }

    /// The images versions still name that the parts no longer use.
    pub fn images_in_versions(&mut self) -> BTreeSet<String> {
        if let (v, Some(seen)) = &self.in_versions_seen {
            if *v == self.version {
                return seen.clone();
            }
        }
        let at = self.version;
        let mut out = BTreeSet::new();
        let keys: Vec<(LayerId, u32)> = self.by_layer.keys().copied().collect();
        for (layer, generation) in keys {
            let ids = self.by_layer.get(&(layer, generation)).cloned().unwrap_or_default();
            // A part never written since it began names nothing old: not opened.
            if !ids.iter().any(|id| self.versions.contains_key(id)) {
                continue;
            }
            let Some(opened) = self.open_layer(layer, generation) else { continue };
            let dek = opened.dek.clone();
            for id in ids.iter().filter(|id| self.versions.contains_key(*id)) {
                let Some(version) = self.version_full(id) else { continue };
                for block in version.blocks.iter().filter(|b| (b.layer, b.generation) == (layer, generation)) {
                    let Ok(changes) = self.block_changes(block, &dek) else { continue };
                    let mut doc = AutoCommit::new();
                    if doc.apply_changes(changes).is_err() {
                        continue;
                    }
                    if let Ok(plain) = LayerDoc::from_loaded(doc).and_then(|d| d.plain()) {
                        out.extend(plain.images.keys().cloned());
                    }
                }
            }
        }
        self.in_versions_seen = (at, Some(out.clone()));
        out
    }

    // --- Passing on ---------------------------------------------------------
    //
    // Everything goes to every member. What someone may read decides their
    // keys alone: a layer only for the game master is noise to everyone else,
    // a version waiting for review is sealed for the game master and its
    // author, a grant for its member.

    /// Everything here, by name — drafts too, for the network.
    pub fn inventory(&self) -> Vec<String> {
        let mut out: Vec<String> = self.raw.keys().chain(self.versions.keys()).cloned().collect();
        out.sort();
        out
    }

    /// What every member gets: control, versions, gallery entries,
    /// proposals and each member's newest badge.
    pub fn shared(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .controls
            .keys()
            .chain(self.versions.keys())
            .chain(self.images.keys())
            .chain(self.proposals.keys())
            .chain(self.badges.values().map(|(id, _)| id))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// One hash over a set of names — equal sets, equal digest.
    /// `digest` of this inventory — worked out again only when it changed.
    pub fn inventory_digest(&self) -> String {
        let key = (self.raw.len(), self.versions.len(), self.arrivals.len());
        if let Some((at, digest)) = &*self.digest_cache.borrow() {
            if *at == key {
                return digest.clone();
            }
        }
        let digest = Journal::digest(&self.inventory());
        *self.digest_cache.borrow_mut() = Some((key, digest.clone()));
        digest
    }

    /// Where the order of arrivals stands: its name and length.
    pub fn arrival_mark(&self) -> (String, u64) {
        (self.epoch.clone(), self.arrivals.len() as u64)
    }

    /// What came after `at` in the order named `epoch` and is still here,
    /// each with the place after it — `None`: another order, ask in full.
    pub fn since(&self, epoch: &str, at: u64) -> Option<Vec<(u64, String)>> {
        let at = usize::try_from(at).ok()?;
        if epoch != self.epoch || at > self.arrivals.len() {
            return None;
        }
        Some(
            self.arrivals[at..]
                .iter()
                .enumerate()
                .filter(|(_, id)| self.raw.contains_key(*id) || self.versions.contains_key(*id))
                .map(|(i, id)| ((at + i + 1) as u64, id.clone()))
                .collect(),
        )
    }

    pub fn digest(ids: &[String]) -> String {
        let mut sorted: Vec<&String> = ids.iter().collect();
        sorted.sort();
        let mut hasher = blake3::Hasher::new();
        for id in sorted {
            hasher.update(id.as_bytes());
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    }

    /// A record's bytes, exactly as they were signed — a version's from its
    /// file.
    pub fn raw(&self, id: &str) -> Option<Vec<u8>> {
        if let Some(raw) = self.raw.get(id) {
            return Some(raw.clone());
        }
        let head = self.versions.get(id)?;
        let path = self.dir.join("articles").join(dir_name(&head.article.to_string())).join(format!("{id}.json"));
        std::fs::read(path).ok().filter(|raw| record_id(raw) == id)
    }

    /// Records in an order the receiver can take them in: control first,
    /// then versions, then drafts — gallery entries, proposals and badges last.
    pub fn in_order(&self, ids: &[String]) -> Vec<String> {
        let mut out: Vec<(u8, u64, String)> = ids
            .iter()
            .filter_map(|id| {
                if let Some(c) = self.controls.get(id) {
                    Some((0, c.seq, id.clone()))
                } else if let Some(v) = self.versions.get(id) {
                    Some((1, v.until, id.clone()))
                } else if let Some(d) = self.drafts.get(id) {
                    Some((2, d.time, id.clone()))
                } else if let Some(i) = self.images.get(id) {
                    Some((3, i.time, id.clone()))
                } else if let Some(p) = self.proposals.get(id) {
                    Some((4, p.at, id.clone()))
                } else {
                    // A badge — the newest of its member, or one before it.
                    match serde_json::from_slice::<Record>(self.raw.get(id)?).ok()? {
                        Record::Badge(b) => Some((5, b.at, id.clone())),
                        _ => None,
                    }
                }
            })
            .collect();
        out.sort();
        out.into_iter().map(|(_, _, id)| id).collect()
    }
}

#[cfg(test)]
mod tests;
