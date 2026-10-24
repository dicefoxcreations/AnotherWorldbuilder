//! Writing: control records, drafts while a session lasts, and its version
//! when it ends.

use super::*;
use aw_crypto::WriteKey;

impl Journal {
    fn append(&mut self, record: Record) -> Result<String> {
        let raw = serde_json::to_vec(&record)?;
        let id = match self.take(raw, record, true)? {
            Added::New(id) => id,
            other => return Err(format!("a record of one's own was not taken: {other:?}").into()),
        };
        self.flush()?;
        Ok(id)
    }

    /// A new document asked of the GM — see `Proposal`.
    pub fn propose(&mut self, author: &Identity, content: &[u8], at: u64) -> Result<String> {
        let gm = self.gm().cloned().ok_or("a world without its game master")?;
        let proposal = Proposal::new(self.world.world_id.as_str(), author, &gm, content, at, self.encrypted())?;
        self.append(Record::Proposal(proposal))
    }

    /// How one is shown in the world — see `Badge`.
    pub fn badge(&mut self, author: &Identity, colour: &str, of: Vec<String>, at: u64) -> Result<String> {
        let badge = Badge::new(self.world.world_id.as_str(), author, colour, of, at)?;
        self.append(Record::Badge(badge))
    }

    /// A gallery entry of one's own: a new image, or a new name.
    pub fn put_image(&mut self, entry: crate::media::ImageEntry) -> Result<String> {
        self.append(Record::Image(entry))
    }

    /// Whoever uploaded an image — the one its id names.
    pub fn uploader(&self, image: &str) -> Option<IdentityPublic> {
        self.images
            .values()
            .find(|e| e.image == image && crate::media::id_by(image, e.author.id.as_str()))
            .map(|e| e.author.clone())
    }

    /// The game master, as the world names them.
    pub fn game_master(&self) -> Option<IdentityPublic> {
        self.gm().cloned()
    }

    /// What it takes to open an image, as far as this device may: from a
    /// part it can read that uses the image, or from the gallery for the
    /// game master and the uploader.
    pub fn image_ref(&mut self, image: &str) -> Option<crate::media::ImageRef> {
        // The gallery first: the game master and the uploader learn it there.
        if let Some(meta) = self.image_meta(image) {
            return Some(meta.image);
        }
        // Otherwise from the parts one may read — gathered once per version:
        // a document with a picture must not open the whole world.
        if self.image_refs.0 != self.version || self.image_refs.1.is_none() {
            let mut found = HashMap::new();
            for article in self.articles() {
                for layer in article.layers {
                    if let Some((_, _, plain)) = layer.open {
                        for (id, image) in plain.images {
                            found.entry(id).or_insert(image);
                        }
                    }
                }
            }
            self.image_refs = (self.version, Some(found));
        }
        self.image_refs.1.as_ref()?.get(image).cloned()
    }

    /// A record of the world's structure — the game master's.
    pub fn control(&mut self, token: &WorldToken, item: Item, session: &str, time: u64) -> Result<String> {
        if token.public().owner_key != self.world.owner_key {
            return Err("only the game master changes the world's structure".into());
        }
        let deletes = matches!(item, Item::Deleted { .. });
        let mut control = Control {
            world: self.world.world_id.to_string(),
            seq: self.next_seq(),
            time,
            session: session.to_string(),
            item,
            signature: String::new(),
        };
        control.signature = hex::encode(token.sign(&control.signed_bytes()?));
        let id = self.append(Record::Control(control))?;
        if deletes {
            self.reload()?;
        }
        Ok(id)
    }

    /// Many gallery entries at once — as `put_image` each, written together
    /// and the index kept once.
    pub fn put_images(&mut self, entries: Vec<crate::media::ImageEntry>) -> Result<Vec<String>> {
        let mut records = Vec::with_capacity(entries.len());
        for entry in entries {
            let record = Record::Image(entry);
            let raw = serde_json::to_vec(&record)?;
            let id = record_id(&raw);
            let path = self.path_of(&id, &record).ok_or("a record without a place")?;
            records.push((id, path, raw, record));
        }
        let files: Vec<(PathBuf, Vec<u8>)> = records.iter().map(|(_, path, raw, _)| (path.clone(), raw.clone())).collect();
        crate::durable::write_all(&files)?;
        let mut ids = Vec::with_capacity(records.len());
        for (id, path, raw, record) in records {
            match self.take(raw, record, false)? {
                Added::New(_) | Added::Known => {}
                other => return Err(format!("a record of one's own was not taken: {other:?}").into()),
            }
            self.listed_new(&path, &id);
            self.read.insert(path);
            ids.push(id);
        }
        self.flush()?;
        Ok(ids)
    }

    /// Many changes to the structure at once — what `control` does for one,
    /// the index kept once at the end. None of them may delete.
    pub fn control_all(&mut self, token: &WorldToken, items: Vec<Item>, session: &str, time: u64) -> Result<Vec<String>> {
        if token.public().owner_key != self.world.owner_key {
            return Err("only the game master changes the world's structure".into());
        }
        if items.iter().any(|i| matches!(i, Item::Deleted { .. })) {
            return Err("a deletion goes alone".into());
        }
        let mut records = Vec::with_capacity(items.len());
        for item in items {
            let mut control = Control {
                world: self.world.world_id.to_string(),
                seq: self.next_seq(),
                time,
                session: session.to_string(),
                item,
                signature: String::new(),
            };
            control.signature = hex::encode(token.sign(&control.signed_bytes()?));
            // Counted here already: the next one follows it.
            self.max_seq = self.max_seq.max(control.seq);
            let record = Record::Control(control);
            let raw = serde_json::to_vec(&record)?;
            let id = record_id(&raw);
            let path = self.path_of(&id, &record).ok_or("a record without a place")?;
            records.push((id, path, raw, record));
        }
        // On disk first, all at once — then taken in, as `store` would after each.
        let files: Vec<(PathBuf, Vec<u8>)> = records.iter().map(|(_, path, raw, _)| (path.clone(), raw.clone())).collect();
        crate::durable::write_all(&files)?;
        let mut ids = Vec::with_capacity(records.len());
        for (id, path, raw, record) in records {
            match self.take(raw, record, false)? {
                Added::New(_) => {}
                other => return Err(format!("a record of one's own was not taken: {other:?}").into()),
            }
            self.listed_new(&path, &id);
            self.read.insert(path);
            ids.push(id);
        }
        self.flush()?;
        Ok(ids)
    }

    /// Writes a layer's content as `wanted`, on top of `base` — the heads the
    /// writer started from — as a draft of `session`. Empty `base` means: on
    /// top of now. Nothing is written when nothing changed.
    #[allow(clippy::too_many_arguments)]
    pub fn edit(
        &mut self,
        layer: LayerId,
        generation: u32,
        base: &[String],
        wanted: &Plain,
        author: &Identity,
        session: &str,
        time: u64,
    ) -> Result<Option<String>> {
        let head = self.head_of(layer, generation).cloned().ok_or("there is no such layer")?;
        let keys = self.keys.clone().ok_or("no keys")?;
        let base = if base.is_empty() {
            self.shown_heads(layer, generation).ok_or("this part cannot be opened here")?
        } else {
            layer::hashes(base)?
        };
        let opened = self.open_layer(layer, generation).ok_or("this part cannot be opened here")?;
        if !opened.doc.has_all(&base) {
            return Err("the state this was written on is not here".into());
        }
        // Unchanged is no change — and needs no right to change.
        if opened.doc.plain_at(&base)? == *wanted {
            return Ok(None);
        }
        let seed = head.write_seed(&keys)?.ok_or("you may not change this part")?;
        let time = self.draft_time(&head.article, author, session, time);
        let opened = self.opened.get_mut(&(layer, generation)).ok_or("not open")?;
        let changes = opened.doc.edit(&base, wanted, author.id().as_str(), session, time)?;
        self.sign_changes(&head, changes, &WriteKey::from_seed(&seed), author, session, time)
    }

    /// A draft's time: after whatever a version of its session holds
    /// already — so the version does not count it as its own.
    fn draft_time(&self, article: &ArticleId, author: &Identity, session: &str, time: u64) -> u64 {
        self.versions
            .values()
            .filter(|v| v.author.id == *author.id() && v.session == session && v.article == *article)
            .map(|v| v.until + 1)
            .fold(time, u64::max)
    }

    pub(super) fn sign_changes(
        &mut self,
        head: &LayerHead,
        changes: Vec<Change>,
        writer: &WriteKey,
        author: &Identity,
        session: &str,
        time: u64,
    ) -> Result<Option<String>> {
        if changes.is_empty() {
            return Ok(None);
        }
        let key = (head.layer, head.generation);
        let dek = self.opened.get(&key).ok_or("not open")?.dek.clone();
        let list: Vec<String> = changes.iter().map(|c| hex::encode(c.raw_bytes())).collect();
        let hashes: Vec<ChangeHash> = changes.iter().map(|c| c.hash()).collect();
        // Waiting for the game master: sealed for them and the author, so no
        // one else reads it before it is taken in — where anything is sealed.
        let (sealed, seal) = match self.gm().filter(|_| !head.plain && self.waits(author.id().as_str())) {
            Some(gm) => {
                let own = ContentKey::generate()?;
                let seal = vec![seal_key(&gm.seal_public()?, &own)?, seal_key(&author.public().seal_public()?, &own)?];
                (hex::encode(encrypt(&own, &serde_json::to_vec(&list)?)?), Some(seal))
            }
            None => (dek.seal(&serde_json::to_vec(&list)?)?, None),
        };
        let seq = self
            .drafts
            .values()
            .filter(|d| (d.layer, d.generation) == key && d.session == session && d.author.id == *author.id())
            .count() as u32
            + 1;
        let mut draft = Draft {
            world: self.world.world_id.to_string(),
            article: head.article,
            layer: head.layer,
            generation: head.generation,
            author: author.public(),
            session: session.to_string(),
            seq,
            time,
            changes: sealed,
            seal,
            write_signature: String::new(),
            signature: String::new(),
        };
        let bytes = draft.signed_bytes()?;
        draft.write_signature = hex::encode(writer.sign(&bytes));
        draft.signature = hex::encode(author.sign(&bytes));
        let id = self.append(Record::Draft(draft))?;
        if let Some(opened) = self.opened.get_mut(&key) {
            // Its changes are in the document already.
            opened.applied.insert(id.clone(), hashes);
        }
        Ok(Some(id))
    }

    // --- Versions -------------------------------------------------------------

    /// The sessions of `author` that wrote drafts no version holds yet, with
    /// the time of the last one.
    pub fn open_sessions(&self, author: &str) -> Vec<(String, u64)> {
        let mut out: BTreeMap<String, u64> = BTreeMap::new();
        for d in self.drafts.values().filter(|d| d.author.id.as_str() == author) {
            let last = out.entry(d.session.clone()).or_insert(0);
            *last = (*last).max(d.time);
        }
        out.into_iter().collect()
    }

    /// The versions of a document this device builds on: the newest ones it
    /// shows — more than one where two were written side by side.
    pub fn tips(&self, article: ArticleId) -> Vec<String> {
        let shown: Vec<(&String, &VersionHead)> =
            self.versions.iter().filter(|(id, v)| v.article == article && !self.hidden(id)).collect();
        let below: HashSet<&String> = shown.iter().flat_map(|(_, v)| v.parents.iter()).collect();
        shown.into_iter().filter(|(id, _)| !below.contains(id)).map(|(id, _)| id.clone()).collect()
    }

    /// Ends a session of `author`: for each document it wrote in, one
    /// version, holding the document whole. Returns the versions written.
    pub fn close(&mut self, session: &str, author: &Identity) -> Result<Vec<String>> {
        let articles: BTreeSet<ArticleId> = self
            .drafts
            .values()
            .filter(|d| d.session == session && d.author.id == *author.id())
            .map(|d| d.article)
            .collect();
        let mut out = Vec::new();
        for article in articles {
            out.push(self.close_article(article, session, author)?);
        }
        Ok(out)
    }

    fn close_article(&mut self, article: ArticleId, session: &str, author: &Identity) -> Result<String> {
        let mine: Vec<(String, Draft)> = self
            .drafts
            .iter()
            .filter(|(_, d)| d.article == article && d.session == session && d.author.id == *author.id())
            .map(|(id, d)| (id.clone(), d.clone()))
            .collect();
        let parents = self.tips(article);
        let keys = self.keys.clone().ok_or("no keys")?;
        // Waiting for the game master: the new blocks sealed for them and
        // the author.
        let seal = match self.gm().filter(|_| self.encrypted() && self.waits(author.id().as_str())).cloned() {
            Some(gm) => {
                let own = ContentKey::generate()?;
                let wrapped = vec![seal_key(&gm.seal_public()?, &own)?, seal_key(&author.public().seal_public()?, &own)?];
                Some((own, wrapped))
            }
            None => None,
        };
        let wrote: BTreeSet<(LayerId, u32)> = mine.iter().map(|(_, d)| (d.layer, d.generation)).collect();
        let mut blocks: Vec<Block> = Vec::new();
        let mut attach: BTreeSet<String> = BTreeSet::new();
        let heads: Vec<LayerHead> = self.layers_of(article).into_iter().cloned().collect();
        let mut done: BTreeSet<(LayerId, u32)> = BTreeSet::new();
        for head in heads {
            let key = (head.layer, head.generation);
            done.insert(key);
            let seed = if wrote.contains(&key) { head.write_seed(&keys).ok().flatten() } else { None };
            match seed {
                Some(seed) => {
                    let (block, foreign) = self.new_block(&head, &parents, author, &seed, seal.as_ref())?;
                    blocks.push(block);
                    attach.extend(foreign);
                }
                None => {
                    // Not changed here, or not to be signed here: as the
                    // versions it builds on hold it.
                    for block in self.parent_blocks(&parents, key) {
                        if !blocks.contains(&block) {
                            blocks.push(block);
                        }
                    }
                    if wrote.contains(&key) {
                        attach.extend(mine.iter().filter(|(_, d)| (d.layer, d.generation) == key).map(|(id, _)| id.clone()));
                    }
                }
            }
        }
        // Written in a generation the layer has left: the drafts themselves.
        for (id, d) in &mine {
            if !done.contains(&(d.layer, d.generation)) {
                attach.insert(id.clone());
            }
        }
        let drafts: Vec<String> = attach
            .iter()
            .filter_map(|id| self.raw.get(id))
            .map(|raw| String::from_utf8_lossy(raw).to_string())
            .collect();
        let mut version = Version {
            world: self.world.world_id.to_string(),
            article,
            author: author.public(),
            session: session.to_string(),
            from: mine.iter().map(|(_, d)| d.time).min().unwrap_or(0),
            until: mine.iter().map(|(_, d)| d.time).max().unwrap_or(0),
            parents,
            wrote: wrote.into_iter().collect(),
            blocks,
            drafts,
            signature: String::new(),
        };
        version.signature = hex::encode(author.sign(&version.signed_bytes()?));
        self.append(Record::Version(version))
    }

    /// The blocks of a layer the versions `parents` hold — taken over as they
    /// are, whether this device can read them or not.
    fn parent_blocks(&self, parents: &[String], key: (LayerId, u32)) -> Vec<Block> {
        let mut out: Vec<Block> = Vec::new();
        for p in parents {
            let Some(v) = self.version_full(p) else { continue };
            for b in v.blocks.iter().filter(|b| (b.layer, b.generation) == key) {
                if !out.contains(b) {
                    out.push(b.clone());
                }
            }
        }
        out
    }

    /// A layer as this device shows it, as a block — and the drafts of
    /// others whose changes are in it but in none of the versions it builds
    /// on, which have to come along.
    fn new_block(
        &mut self,
        head: &LayerHead,
        parents: &[String],
        author: &Identity,
        seed: &ContentKey,
        seal: Option<&(ContentKey, Vec<WrappedKey>)>,
    ) -> Result<(Block, Vec<String>)> {
        let key = (head.layer, head.generation);
        let shown = self.shown_heads(head.layer, head.generation).ok_or("this part cannot be opened here")?;
        let others: Vec<String> = self
            .by_layer
            .get(&key)
            .into_iter()
            .flatten()
            .filter(|id| self.drafts.get(*id).is_some_and(|d| d.author.id != *author.id()))
            .cloned()
            .collect();
        let prefix = layer::author_prefix(author.id().as_str());
        let opened = self.opened.get_mut(&key).ok_or("not open")?;
        let state = opened.doc.save_at(&shown)?;
        let inside = opened.doc.ancestry(&shown);
        let known: HashSet<ChangeHash> = parents
            .iter()
            .filter_map(|p| opened.applied.get(p))
            .flatten()
            .chain(opened.genesis.iter())
            .copied()
            .collect();
        let foreign: HashSet<ChangeHash> = inside
            .into_iter()
            .filter(|h| !known.contains(h))
            .filter(|h| opened.doc.doc.get_change_by_hash(h).is_some_and(|c| !c.actor_id().to_bytes().starts_with(&prefix)))
            .collect();
        let attach: Vec<String> = others
            .into_iter()
            .filter(|id| opened.applied.get(id).is_some_and(|hs| hs.iter().any(|h| foreign.contains(h))))
            .collect();
        let dek = opened.dek.clone();
        let (state, seal) = match seal {
            Some((own, wrapped)) => (hex::encode(encrypt(own, &state)?), Some(wrapped.clone())),
            None => (dek.seal(&state)?, None),
        };
        let mut block = Block { layer: head.layer, generation: head.generation, state, seal, write_signature: String::new() };
        let bytes = block.signed_bytes(self.world.world_id.as_str(), &head.article)?;
        block.write_signature = hex::encode(WriteKey::from_seed(seed).sign(&bytes));
        Ok((block, attach))
    }
}

// --- Keeping it small -----------------------------------------------------------

impl Journal {
    /// The versions of a document that count towards its limit: all but
    /// those that wait for the game master.
    pub fn kept_versions(&self, article: ArticleId) -> Vec<String> {
        let mut out: Vec<(&String, &VersionHead)> = self
            .versions
            .iter()
            .filter(|(id, v)| v.article == article && self.record_review(id) != Review::Waiting)
            .collect();
        out.sort_by_key(|(_, v)| v.until);
        out.into_iter().map(|(id, _)| id.clone()).collect()
    }

    /// The versions of a document that may go, oldest first: neither the
    /// newest — what the document stands on — nor what waits.
    pub fn removable(&self, article: ArticleId) -> Vec<String> {
        let below: HashSet<&String> = self.versions.values().flat_map(|v| v.parents.iter()).collect();
        self.kept_versions(article).into_iter().filter(|id| below.contains(id)).collect()
    }

    /// The versions of one session in a document.
    pub fn versions_of(&self, article: ArticleId, session: &str) -> Vec<String> {
        self.versions.iter().filter(|(_, v)| v.article == article && v.session == session).map(|(id, _)| id.clone()).collect()
    }

    /// The documents with more versions than `limit` — counting those that
    /// do not wait.
    pub fn over(&self, limit: usize) -> Vec<ArticleId> {
        let mut count: BTreeMap<ArticleId, usize> = BTreeMap::new();
        for (id, v) in &self.versions {
            if self.record_review(id) != Review::Waiting {
                *count.entry(v.article).or_insert(0) += 1;
            }
        }
        count.into_iter().filter(|(_, n)| *n > limit).map(|(a, _)| a).collect()
    }

    /// Whether versions of a document wait for the game master.
    pub fn has_waiting(&self, article: ArticleId) -> bool {
        self.versions.iter().any(|(id, v)| v.article == article && self.record_review(id) == Review::Waiting)
    }

    /// The parts of a document as they are now: layer and generation.
    pub fn parts_of(&self, article: ArticleId) -> Vec<(LayerId, u32)> {
        self.layers_of(article).into_iter().map(|h| (h.layer, h.generation)).collect()
    }

    /// Control records something newer replaced and nothing needs any more:
    /// earlier world records, types, grants, settings, verdicts on versions
    /// that are gone, whose keys no block left needs — and the generations
    /// of a part between its first and its current one that nothing was
    /// written in. The first stays: the parts of a document stand in the
    /// order they came, by it.
    pub fn superseded(&self) -> Vec<String> {
        let current: HashSet<&String> = self.latest.values().map(|(_, id)| id).collect();
        let blocks: HashSet<&String> = self.versions.values().flat_map(|v| v.blocks.iter().map(|(.., id, _)| id)).collect();
        let mut first: HashMap<LayerId, u32> = HashMap::new();
        for (layer, generation) in self.generations.keys() {
            let at = first.entry(*layer).or_insert(*generation);
            *at = (*at).min(*generation);
        }
        // Written in: that generation stays, and the one after it, which
        // tells whether what was written there was taken along.
        let written: HashSet<(LayerId, u32)> = self
            .by_layer
            .iter()
            .filter(|(_, ids)| !ids.is_empty())
            .flat_map(|((layer, generation), _)| [(*layer, *generation), (*layer, generation + 1)])
            .collect();
        self.controls
            .iter()
            .filter(|(id, c)| match &c.item {
                Item::World { .. }
                | Item::Templates { .. }
                | Item::Grant { .. }
                | Item::Revoked { .. }
                | Item::Moderation { .. }
                | Item::Trust { .. }
                | Item::Versions { .. }
                | Item::Gallery { .. }
                | Item::Navigation { .. }
                | Item::Encryption { .. }
                | Item::Copies { .. }
                | Item::Hosts { .. }
                | Item::Listing { .. } => !current.contains(id),
                Item::Verdict { versions, keys, .. } => {
                    versions.iter().all(|v| !self.versions.contains_key(v)) && keys.keys().all(|b| !blocks.contains(b))
                }
                Item::Layer(head) => {
                    !current.contains(id)
                        && first.get(&head.layer) != Some(&head.generation)
                        && !written.contains(&(head.layer, head.generation))
                }
                _ => false,
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}
