//! The history of a document: its versions, what each changed, and taking
//! one back or carrying it over.
//!
//! Which version a change belongs to stays with the change itself: each
//! carries its session and its author, whichever version holds it.

use super::*;

/// One editing session on an article — a version.
#[derive(Clone, Debug, Serialize)]
pub struct Session {
    pub session: String,
    pub author: IdentityPublic,
    pub from: u64,
    pub to: u64,
    /// When the last of it came to this device. Long after `to`: written
    /// offline, merged only now — news, even though it is old.
    pub arrived: u64,
    /// The layers it changed.
    pub layers: Vec<LayerId>,
    /// Changes the access of a part, adds or removes one — the game master's.
    pub structure: bool,
    /// Written for a generation a layer has left meanwhile, and not carried
    /// over yet: its changes are in the history, not in the current text.
    pub stale: bool,
    /// Sessions of others, written at the same time on the same lines.
    pub overlaps: Vec<String>,
    pub review: Review,
    /// Whether the game master may delete it: written down as versions, none
    /// of them the newest, none waiting.
    pub removable: bool,
}

impl Journal {
    /// Each change of an open layer: its session, its author's actor, its
    /// time — read once per state of the document.
    fn made(&mut self, layer: LayerId, generation: u32) -> Vec<Made> {
        let Some(opened) = self.open_layer(layer, generation) else { return Vec::new() };
        let heads = opened.doc.doc.get_heads();
        if let Some((at, made)) = &opened.index {
            if *at == heads {
                return made.clone();
            }
        }
        let made: Vec<Made> = opened
            .doc
            .doc
            .get_changes(&[])
            .iter()
            .map(|c| Made {
                hash: c.hash(),
                session: c.message().unwrap_or_default().to_string(),
                actor: c.actor_id().to_bytes().to_vec(),
                time: c.timestamp().max(0) as u64,
            })
            .collect();
        opened.index = Some((heads, made.clone()));
        made
    }

    /// Who wrote a session: the author of its drafts and versions.
    fn session_author(&self, session: &str) -> Option<String> {
        self.versions
            .values()
            .find(|v| v.session == session)
            .map(|v| v.author.id.to_string())
            .or_else(|| self.drafts.values().chain(self.attached.values()).find(|d| d.session == session).map(|d| d.author.id.to_string()))
    }

    /// The changes a session made to a layer in a generation.
    fn session_changes(&mut self, layer: LayerId, generation: u32, session: &str) -> Vec<ChangeHash> {
        let Some(author) = self.session_author(session) else { return Vec::new() };
        let prefix = layer::author_prefix(&author);
        self.made(layer, generation)
            .into_iter()
            .filter(|m| m.session == session && m.actor.starts_with(&prefix))
            .map(|m| m.hash)
            .collect()
    }

    /// Where a session wrote: layer and generation.
    fn writes_of(&self, session: &str) -> BTreeSet<(LayerId, u32)> {
        self.versions
            .values()
            .filter(|v| v.session == session)
            .flat_map(|v| v.wrote.iter().copied())
            .chain(
                self.drafts
                    .values()
                    .chain(self.attached.values())
                    .filter(|d| d.session == session)
                    .map(|d| (d.layer, d.generation)),
            )
            .collect()
    }

    /// The versions of an article: its editing sessions, newest first.
    pub fn sessions(&mut self, article: ArticleId) -> Vec<Session> {
        let gm = self.world_item().map(|(.., gm)| gm.clone());
        let mut by: BTreeMap<String, Session> = BTreeMap::new();
        #[allow(clippy::too_many_arguments)]
        let touch = |by: &mut BTreeMap<String, Session>, session: &str, author: &IdentityPublic, from: u64, to: u64, arrived: u64, layers: &[LayerId], structure: bool| {
            let entry = by.entry(session.to_string()).or_insert_with(|| Session {
                session: session.to_string(),
                author: author.clone(),
                from,
                to,
                arrived,
                layers: Vec::new(),
                structure: false,
                stale: false,
                overlaps: Vec::new(),
                review: Review::Counted,
                removable: false,
            });
            entry.from = entry.from.min(from);
            entry.to = entry.to.max(to);
            entry.arrived = entry.arrived.max(arrived);
            entry.structure |= structure;
            for l in layers {
                if !entry.layers.contains(l) {
                    entry.layers.push(*l);
                }
            }
        };
        for (id, v) in self.versions.iter().filter(|(_, v)| v.article == article) {
            let arrived = self.arrived.get(id).copied().unwrap_or(v.until);
            let layers: Vec<LayerId> = v.wrote.iter().map(|(l, _)| *l).collect();
            touch(&mut by, &v.session, &v.author, v.from, v.until, arrived, &layers, false);
        }
        for (id, d) in self.drafts.iter().filter(|(_, d)| d.article == article) {
            let arrived = self.arrived.get(id).copied().unwrap_or(d.time);
            touch(&mut by, &d.session, &d.author, d.time, d.time, arrived, &[d.layer], false);
        }
        // Written together and come along in someone else's version.
        for (version, v) in self.versions.iter().filter(|(_, v)| v.article == article) {
            let arrived = self.arrived.get(version).copied().unwrap_or(v.until);
            for draft in &v.drafts {
                if let Some(d) = self.attached.get(draft) {
                    touch(&mut by, &d.session, &d.author, d.time, d.time, arrived, &[d.layer], false);
                }
            }
        }
        if let Some(gm) = &gm {
            for (id, control) in self.controls.iter().filter(|(_, c)| !c.session.is_empty()) {
                let layer = match &control.item {
                    Item::Layer(h) if h.article == article => Some(h.layer),
                    Item::Removed { article: a, layer } if *a == article => *layer,
                    _ => continue,
                };
                let arrived = self.arrived.get(id).copied().unwrap_or(control.time);
                let layers: Vec<LayerId> = layer.into_iter().collect();
                touch(&mut by, &control.session, gm, control.time, control.time, arrived, &layers, true);
            }
        }
        // Written for a generation a layer has left, and not carried over.
        let removable: HashSet<String> = self.removable(article).into_iter().collect();
        let mut stale: HashSet<String> = HashSet::new();
        for session in by.values() {
            if session.layers.clone().iter().any(|l| self.stale_in(*l, &session.session)) {
                stale.insert(session.session.clone());
            }
        }
        for session in by.values_mut() {
            session.stale = stale.contains(&session.session);
            session.review = self.session_review(&session.session);
            let versions = self.versions_of(article, &session.session);
            session.removable = !versions.is_empty() && versions.iter().all(|v| removable.contains(v));
        }
        // Who wrote on the same lines at the same time.
        let layers: BTreeSet<LayerId> = by.values().flat_map(|s| s.layers.iter().copied()).collect();
        for layer in layers {
            for (a, b) in self.overlaps(layer) {
                if let Some(s) = by.get_mut(&a) {
                    if !s.overlaps.contains(&b) {
                        s.overlaps.push(b.clone());
                    }
                }
                if let Some(s) = by.get_mut(&b) {
                    if !s.overlaps.contains(&a) {
                        s.overlaps.push(a.clone());
                    }
                }
            }
        }
        let mut out: Vec<Session> = by.into_values().collect();
        out.sort_by(|a, b| b.to.cmp(&a.to).then(b.from.cmp(&a.from)).then(b.arrived.cmp(&a.arrived)));
        out
    }

    /// Pairs of sessions by different authors that were written at the same
    /// time — neither saw the other — and changed the same lines or fields.
    pub fn overlaps(&mut self, layer: LayerId) -> Vec<(String, String)> {
        let Some(generation) = self.head(layer).map(|h| h.generation) else { return Vec::new() };
        let made = self.made(layer, generation);
        let mut sessions: BTreeMap<String, (String, Vec<ChangeHash>)> = BTreeMap::new();
        let names: BTreeSet<String> = made.iter().map(|m| m.session.clone()).filter(|s| !s.is_empty()).collect();
        for name in names {
            let Some(author) = self.session_author(&name) else { continue };
            let changes = self.session_changes(layer, generation, &name);
            sessions.insert(name, (author, changes));
        }
        let Some(opened) = self.open_layer(layer, generation) else { return Vec::new() };
        let doc = &mut opened.doc;
        let mut reach: BTreeMap<String, HashSet<ChangeHash>> = BTreeMap::new();
        for (name, (_, changes)) in &sessions {
            reach.insert(name.clone(), doc.ancestry(changes));
        }
        let mut out = Vec::new();
        let list: Vec<(&String, &(String, Vec<ChangeHash>))> = sessions.iter().collect();
        for (i, (a, (author_a, changes_a))) in list.iter().enumerate() {
            for (b, (author_b, changes_b)) in &list[i + 1..] {
                if author_a == author_b || changes_a.is_empty() || changes_b.is_empty() {
                    continue;
                }
                let (reach_a, reach_b) = (&reach[*a], &reach[*b]);
                // One saw the other: no overlap, just a sequence.
                if changes_a.iter().any(|h| reach_b.contains(h)) || changes_b.iter().any(|h| reach_a.contains(h)) {
                    continue;
                }
                let common: HashSet<ChangeHash> = reach_a.intersection(reach_b).copied().collect();
                let base_heads = doc.tips(&common);
                let tips_a = doc.tips(reach_a);
                let tips_b = doc.tips(reach_b);
                let (Ok(base), Ok(pa), Ok(pb)) =
                    (doc.plain_at(&base_heads), doc.plain_at(&tips_a), doc.plain_at(&tips_b))
                else {
                    continue;
                };
                let lines = layer::overlapping(
                    &layer::changed_lines(&base.text, &pa.text),
                    &layer::changed_lines(&base.text, &pb.text),
                );
                let fields = pa.fields.iter().any(|(k, v)| {
                    base.fields.get(k) != Some(v) && pb.fields.get(k).is_some_and(|w| w != v && base.fields.get(k) != Some(w))
                });
                let title = pa.title != base.title && pb.title != base.title && pa.title != pb.title;
                if lines || fields || title {
                    out.push(((*a).clone(), (*b).clone()));
                }
            }
        }
        out
    }

    /// The newest generation a session wrote a layer in.
    fn wrote_in(&self, layer: LayerId, session: &str) -> Option<u32> {
        self.writes_of(session).into_iter().filter(|(l, _)| *l == layer).map(|(_, g)| g).max()
    }

    /// A layer before and after a session — the version compared with the
    /// one before it.
    pub fn compare(&mut self, layer: LayerId, session: &str) -> Result<(Plain, Plain)> {
        // Where the session wrote last — carried over, that is the current one.
        let generation = self
            .wrote_in(layer, session)
            .or_else(|| self.head(layer).map(|h| h.generation))
            .ok_or("there is no such layer")?;
        self.compare_in(layer, generation, session)
    }

    fn compare_in(&mut self, layer: LayerId, generation: u32, session: &str) -> Result<(Plain, Plain)> {
        let changes = self.session_changes(layer, generation, session);
        let opened = self.open_layer(layer, generation).ok_or("this part cannot be opened here")?;
        if changes.is_empty() {
            let now = opened.doc.plain()?;
            return Ok((now.clone(), now));
        }
        let before = opened.doc.before(&changes);
        let mut after = opened.doc.ancestry(&changes);
        after.extend(changes.iter().copied());
        let tips = opened.doc.tips(&after);
        Ok((opened.doc.plain_at(&before)?, opened.doc.plain_at(&tips)?))
    }

    /// Whether a session wrote to a layer only in generations it has left —
    /// and the next generation did not start from what it wrote. A change of
    /// access right after writing takes the text along; written offline
    /// meanwhile, it is left behind.
    fn stale_in(&mut self, layer: LayerId, session: &str) -> bool {
        let Some(current) = self.head(layer).map(|h| h.generation) else { return false };
        let generations: BTreeSet<u32> =
            self.writes_of(session).into_iter().filter(|(l, _)| *l == layer).map(|(_, g)| g).collect();
        let Some(&last) = generations.iter().max() else { return false };
        if generations.contains(&current) {
            return false;
        }
        let Some(next) = self.head_of(layer, last + 1).cloned() else { return true };
        let Ok(from) = layer::hashes(&next.from) else { return true };
        let changes = self.session_changes(layer, last, session);
        let Some(opened) = self.open_layer(layer, last) else { return true };
        let taken = opened.doc.ancestry(&from);
        changes.is_empty() || !changes.iter().all(|c| taken.contains(c))
    }

    /// One's own sessions that a new generation left behind.
    pub fn stale_of(&mut self, author: &str) -> Vec<(LayerId, String)> {
        let sessions: BTreeSet<String> = self
            .versions
            .values()
            .filter(|v| v.author.id.as_str() == author)
            .map(|v| v.session.clone())
            .chain(self.drafts.values().filter(|d| d.author.id.as_str() == author).map(|d| d.session.clone()))
            .collect();
        let mut out: BTreeSet<(LayerId, String)> = BTreeSet::new();
        for session in sessions {
            let writes = self.writes_of(&session);
            for (layer, _) in writes {
                if self.stale_in(layer, &session) {
                    out.insert((layer, session.clone()));
                }
            }
        }
        out.into_iter().collect()
    }

    /// Carries what a session wrote in a generation the layer has left over
    /// into the current one — as the same session, by `author`, who needs the
    /// right to edit the layer now. So what was written offline while the
    /// game master changed who may see or edit it still ends up in the text.
    pub fn carry(&mut self, layer: LayerId, session: &str, author: &Identity, time: u64) -> Result<Option<String>> {
        if !self.stale_in(layer, session) {
            return Ok(None);
        }
        let old = self.wrote_in(layer, session).ok_or("there is no such version")?;
        let (before, after) = self.compare_in(layer, old, session)?;
        let current = self.head(layer).ok_or("there is no such layer")?.generation;
        let now = self.plain(layer)?;
        let wanted = layer::carried(&before, &after, &now);
        self.edit(layer, current, &[], &wanted, author, session, time)
    }

    /// Where a layer stood with a version: its generation and heads —
    /// exactly after the session where it changed the layer, otherwise as
    /// it stood at `time`. A version that is not in the text, waiting or
    /// turned down, stands as the text would be with it taken in now.
    /// `None` if the layer did not exist yet.
    pub fn heads_at(&mut self, layer: LayerId, session: &str, time: u64) -> Result<Option<(u32, Vec<ChangeHash>)>> {
        if let Some(generation) = self.wrote_in(layer, session) {
            let changes = self.session_changes(layer, generation, session);
            let current = self.head(layer).map(|h| h.generation);
            let shown = match (self.session_review(session), current) {
                (Review::Counted, _) => None,
                (_, Some(now)) if now == generation => self.shown_heads(layer, generation),
                _ => None,
            };
            let opened = self.open_layer(layer, generation).ok_or("this part cannot be opened here")?;
            if changes.is_empty() {
                return Ok(Some((generation, opened.doc.doc.get_heads())));
            }
            let mut set = opened.doc.ancestry(&changes);
            if let Some(shown) = shown {
                set.extend(opened.doc.ancestry(&shown));
            }
            return Ok(Some((generation, opened.doc.tips(&set))));
        }
        // The generation that stood then.
        let generation = self
            .generations
            .iter()
            .filter(|((l, _), _)| *l == layer)
            .filter(|(_, (_, id))| self.controls.get(id).is_some_and(|c| c.time <= time))
            .map(|((_, g), _)| *g)
            .max();
        let Some(generation) = generation else { return Ok(None) };
        let shown = self.shown_heads(layer, generation).ok_or("this part cannot be opened here")?;
        let made = self.made(layer, generation);
        let opened = self.opened.get_mut(&(layer, generation)).ok_or("not open")?;
        let visible = opened.doc.ancestry(&shown);
        let then: Vec<ChangeHash> =
            made.iter().filter(|m| m.time <= time && visible.contains(&m.hash)).map(|m| m.hash).collect();
        let mut reach = opened.doc.ancestry(&then);
        if reach.is_empty() {
            // Only the start: the generation's first change.
            reach = opened.genesis.clone();
        }
        Ok(Some((generation, opened.doc.tips(&reach))))
    }

    /// A layer as it stood with a version — see `heads_at`. Empty if the
    /// layer did not exist yet.
    pub fn state_at(&mut self, layer: LayerId, session: &str, time: u64) -> Result<Plain> {
        match self.heads_at(layer, session, time)? {
            Some((generation, heads)) => self.plain_of(layer, generation, &heads),
            None => Ok(Plain::default()),
        }
    }

    /// A whole article as it stood with a version — each layer as
    /// `state_at` has it, in the generation that stood then. Layers that did
    /// not exist yet are left out, removed ones that still did are in.
    pub fn article_at(&mut self, article: ArticleId, session: &str, time: u64) -> Result<ArticleState> {
        if !self.controls.values().any(|c| matches!(&c.item, Item::Article { article: a } if *a == article)) {
            return Err("there is no such article".into());
        }
        // What a document is never changes; its last base layer knows.
        let About { article_type, slug } = self
            .generations
            .keys()
            .filter_map(|(l, g)| self.head_of(*l, *g))
            .filter(|h| h.article == article && h.is_base)
            .max_by_key(|h| h.generation)
            .and_then(|h| self.about_of(h))
            .unwrap_or_default();
        let writes = self.writes_of(session);
        let touched: BTreeSet<LayerId> = writes.iter().map(|(l, _)| *l).collect();
        // Which generation of which layer stood then.
        let mut then: BTreeMap<LayerId, u32> = BTreeMap::new();
        for ((layer, generation), (_, id)) in &self.generations {
            let Some(control) = self.controls.get(id) else { continue };
            let Item::Layer(head) = &control.item else { continue };
            if head.article != article || (control.time > time && !touched.contains(layer)) {
                continue;
            }
            let at = then.entry(*layer).or_insert(*generation);
            *at = (*at).max(*generation);
        }
        // A layer the version wrote in stands in the generation it wrote.
        for (layer, generation) in writes {
            if self.head_of(layer, generation).is_some_and(|h| h.article == article) {
                then.insert(layer, generation.max(then.get(&layer).copied().unwrap_or(0)));
            }
        }
        // Removed before the version: gone then already.
        let mut gone = BTreeSet::new();
        let mut all_gone = false;
        for c in self.controls.values().filter(|c| c.time <= time) {
            if let Item::Removed { article: a, layer } = &c.item {
                if *a == article {
                    match layer {
                        Some(l) => {
                            gone.insert(*l);
                        }
                        None => all_gone = true,
                    }
                }
            }
        }
        let mut layers = Vec::new();
        for (layer, generation) in then {
            if all_gone || gone.contains(&layer) {
                continue;
            }
            let Some(head) = self.head_of(layer, generation).cloned() else { continue };
            let plain = self.state_at(layer, session, time).ok();
            let open = match (plain, self.open_layer(layer, generation)) {
                (Some(plain), Some(o)) => Some((o.dek.clone(), o.access.clone(), plain)),
                _ => None,
            };
            layers.push(LayerState { head, open });
        }
        layers.sort_by_key(|l| (!l.head.is_base, self.born.get(&l.head.layer).copied().unwrap_or(0)));
        Ok(ArticleState { article, article_type, slug, layers })
    }

    /// Takes a session's changes to a layer back — as a new change by
    /// `author`, who needs the right to edit the layer.
    pub fn revert(&mut self, layer: LayerId, session: &str, author: &Identity, new_session: &str, time: u64) -> Result<Option<String>> {
        let head = self.head(layer).cloned().ok_or("there is no such layer")?;
        let keys = self.keys.clone().ok_or("no keys")?;
        let seed = head.write_seed(&keys)?.ok_or("you may not change this part")?;
        let changes = self.session_changes(layer, head.generation, session);
        if changes.is_empty() {
            return Ok(None);
        }
        let opened = self.open_layer(layer, head.generation).ok_or("this part cannot be opened here")?;
        let new = opened.doc.revert(&changes, author.id().as_str(), new_session, time)?;
        self.sign_changes(&head, new, &aw_crypto::WriteKey::from_seed(&seed), author, new_session, time)
    }

    /// The waiting versions that go with a verdict on `session`: taken in, it
    /// brings the ones it builds on; turned down, it takes the ones built on
    /// it. One cannot stand without the other. Returns their sessions.
    pub fn bound(&mut self, session: &str, accepted: bool) -> Vec<String> {
        let waiting: HashSet<String> =
            self.versions.keys().filter(|id| self.record_review(id) == Review::Waiting).cloned().collect();
        let mine: HashSet<String> =
            waiting.iter().filter(|id| self.versions[*id].session == session).cloned().collect();
        let below = |from: &str| -> HashSet<String> {
            let mut seen = HashSet::new();
            let mut open: Vec<String> = self.versions.get(from).map(|v| v.parents.clone()).unwrap_or_default();
            while let Some(id) = open.pop() {
                if seen.insert(id.clone()) {
                    if let Some(v) = self.versions.get(&id) {
                        open.extend(v.parents.iter().cloned());
                    }
                }
            }
            seen
        };
        let mut out = BTreeSet::new();
        if accepted {
            for id in &mine {
                for b in below(id) {
                    if waiting.contains(&b) && !mine.contains(&b) {
                        out.insert(self.versions[&b].session.clone());
                    }
                }
            }
        } else {
            for id in waiting.iter().filter(|id| !mine.contains(*id)) {
                if below(id).iter().any(|b| mine.contains(b)) {
                    out.insert(self.versions[id].session.clone());
                }
            }
        }
        out.into_iter().collect()
    }
}
