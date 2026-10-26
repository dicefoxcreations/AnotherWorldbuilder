//! What a layer holds, as a CRDT: its title, its text and its fields.
//!
//! One Automerge document per layer and generation. Concurrent changes merge
//! by themselves — two people writing in different places of the same text
//! both keep what they wrote. Who may see or edit a layer is not in here:
//! that is the game master's, in the layer's head.

use automerge::{
    transaction::{CommitOptions, Transactable}, ActorId, AutoCommit, Change, ChangeHash, Cursor, CursorPosition, ObjId,
    ObjType, ReadDoc, ScalarValue, Value, ROOT,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::str::FromStr;

use super::diff::{self, Range};
use super::Result;
use crate::media::ImageRef;

/// The plaintext of a layer's content.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Plain {
    pub title: String,
    pub text: String,
    /// Template field key → value.
    pub fields: BTreeMap<String, serde_json::Value>,
    /// The images this layer uses, by id — with what it takes to open them.
    /// Whoever may read the layer may see them.
    #[serde(default)]
    pub images: BTreeMap<String, ImageRef>,
}

/// A layer's document, opened.
pub struct LayerDoc {
    pub doc: AutoCommit,
    text: ObjId,
    fields: ObjId,
    images: ObjId,
}

/// The actor of a change starts with its author: whoever signs a change can
/// only have written it under their own name.
pub fn author_prefix(author: &str) -> [u8; 16] {
    let hash = blake3::hash(format!("anotherworldbuilder/actor/1\0{author}").as_bytes());
    hash.as_bytes()[..16].try_into().expect("16 bytes")
}

fn actor(author: &str) -> Result<ActorId> {
    let mut bytes = author_prefix(author).to_vec();
    bytes.extend_from_slice(&aw_crypto::random_bytes::<8>()?);
    Ok(ActorId::from(bytes))
}

pub fn hashes(heads: &[String]) -> Result<Vec<ChangeHash>> {
    heads.iter().map(|h| ChangeHash::from_str(h).map_err(|e| e.to_string().into())).collect()
}

pub fn texts(heads: &[ChangeHash]) -> Vec<String> {
    heads.iter().map(ToString::to_string).collect()
}

impl LayerDoc {
    /// A new document holding `plain` — the start of a generation.
    pub fn genesis(plain: &Plain, layer: &str, generation: u32) -> Result<Self> {
        let seed = blake3::hash(format!("anotherworldbuilder/genesis/1\0{layer}\0{generation}").as_bytes());
        let mut doc = AutoCommit::new().with_actor(ActorId::from(seed.as_bytes()[..16].to_vec()));
        doc.put(ROOT, "title", plain.title.as_str())?;
        let text = doc.put_object(ROOT, "text", ObjType::Text)?;
        doc.splice_text(&text, 0, 0, &plain.text)?;
        let fields = doc.put_object(ROOT, "fields", ObjType::Map)?;
        for (key, value) in &plain.fields {
            doc.put(&fields, key.as_str(), serde_json::to_string(value)?)?;
        }
        let images = doc.put_object(ROOT, "images", ObjType::Map)?;
        for (key, value) in &plain.images {
            doc.put(&images, key.as_str(), serde_json::to_string(value)?)?;
        }
        // The same on every device that loads it: no clock, no session.
        doc.commit_with(CommitOptions::default().with_time(0));
        Ok(LayerDoc { doc, text, fields, images })
    }

    pub fn load(bytes: &[u8]) -> Result<Self> {
        Self::from_doc(AutoCommit::load(bytes)?)
    }

    /// A document put together from changes.
    pub fn from_loaded(doc: AutoCommit) -> Result<Self> {
        Self::from_doc(doc)
    }

    fn from_doc(doc: AutoCommit) -> Result<Self> {
        let text = match doc.get(ROOT, "text")? {
            Some((Value::Object(ObjType::Text), id)) => id,
            _ => return Err("a layer document without text".into()),
        };
        let fields = match doc.get(ROOT, "fields")? {
            Some((Value::Object(ObjType::Map), id)) => id,
            _ => return Err("a layer document without fields".into()),
        };
        let images = match doc.get(ROOT, "images")? {
            Some((Value::Object(ObjType::Map), id)) => id,
            _ => return Err("a layer document without images".into()),
        };
        Ok(LayerDoc { doc, text, fields, images })
    }

    pub fn heads(&mut self) -> Vec<String> {
        texts(&self.doc.get_heads())
    }

    /// The content as it stands now.
    pub fn plain(&self) -> Result<Plain> {
        let title = match self.doc.get(ROOT, "title")? {
            Some((Value::Scalar(s), _)) => match s.as_ref() {
                ScalarValue::Str(s) => s.to_string(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        let mut fields = BTreeMap::new();
        for key in self.doc.keys(&self.fields) {
            if let Some((Value::Scalar(s), _)) = self.doc.get(&self.fields, key.as_str())? {
                if let ScalarValue::Str(json) = s.as_ref() {
                    if let Ok(value) = serde_json::from_str(json) {
                        fields.insert(key, value);
                    }
                }
            }
        }
        let mut images = BTreeMap::new();
        for key in self.doc.keys(&self.images) {
            if let Some((Value::Scalar(s), _)) = self.doc.get(&self.images, key.as_str())? {
                if let ScalarValue::Str(json) = s.as_ref() {
                    if let Ok(value) = serde_json::from_str(json) {
                        images.insert(key, value);
                    }
                }
            }
        }
        Ok(Plain { title, text: self.doc.text(&self.text)?, fields, images })
    }

    /// The content as it stood at `heads`.
    pub fn plain_at(&mut self, heads: &[ChangeHash]) -> Result<Plain> {
        Self::from_doc(self.doc.fork_at(heads)?)?.plain()
    }

    /// Writes `wanted` as a change on top of `base` — the state the writer
    /// saw — and merges it. Returns the new changes, to be signed and sent.
    /// Each change carries its session and its time (milliseconds): which
    /// version it belongs to stays with the change itself, whichever file
    /// later holds it.
    ///
    /// Only what differs from `base` is written: text as the smallest edit,
    /// each field on its own. So a concurrent change elsewhere survives.
    pub fn edit(&mut self, base: &[ChangeHash], wanted: &Plain, author: &str, session: &str, time: u64) -> Result<Vec<Change>> {
        let mut branch = Self::from_doc(self.doc.fork_at(base)?)?;
        branch.doc.set_actor(actor(author)?);
        branch.write(wanted)?;
        if branch.doc.commit_with(CommitOptions::default().with_message(session).with_time(time as i64)).is_none() {
            return Ok(Vec::new());
        }
        let changes: Vec<Change> = branch.doc.get_changes(base).into_iter().collect();
        self.doc.apply_changes(changes.clone())?;
        Ok(changes)
    }

    /// Changes the branch's content to `wanted`, touching only what differs.
    fn write(&mut self, wanted: &Plain) -> Result<()> {
        let now = self.plain()?;
        if now.title != wanted.title {
            self.doc.put(ROOT, "title", wanted.title.as_str())?;
        }
        if now.text != wanted.text {
            // Letter by letter: what stays is not taken out and put back, so
            // what someone wrote beside it at the same time stays where it was.
            let text = self.text.clone();
            for s in diff::edits(&now.text, &wanted.text) {
                self.doc.splice_text(&text, s.at, s.delete as isize, &s.insert)?;
            }
        }
        let fields = self.fields.clone();
        for (key, value) in &wanted.fields {
            if now.fields.get(key) != Some(value) {
                self.doc.put(&fields, key.as_str(), serde_json::to_string(value)?)?;
            }
        }
        for key in now.fields.keys() {
            if !wanted.fields.contains_key(key) {
                self.doc.delete(&fields, key.as_str())?;
            }
        }
        let images = self.images.clone();
        for (key, value) in &wanted.images {
            if now.images.get(key) != Some(value) {
                self.doc.put(&images, key.as_str(), serde_json::to_string(value)?)?;
            }
        }
        for key in now.images.keys() {
            if !wanted.images.contains_key(key) {
                self.doc.delete(&images, key.as_str())?;
            }
        }
        Ok(())
    }

    /// What lies between two states of the text: the stretches of `left`'s
    /// text that `right` no longer has, and the stretches of `right`'s text
    /// that `left` did not have yet. Measured by the characters themselves,
    /// not by comparing texts — a character typed again is a new one.
    pub fn marks(&mut self, left: &[ChangeHash], right: &[ChangeHash]) -> Result<(Vec<Range>, Vec<Range>)> {
        let chars = |doc: &AutoCommit, text: &ObjId| -> Vec<(String, usize)> {
            doc.list_range(text, ..)
                .map(|item| {
                    let units = match &item.value {
                        automerge::ValueRef::Scalar(automerge::ScalarValueRef::Str(s)) => s.encode_utf16().count(),
                        _ => 1,
                    };
                    (item.id().to_string(), units)
                })
                .collect()
        };
        let (l, r) = (self.doc.fork_at(left)?, self.doc.fork_at(right)?);
        let (l, r) = (chars(&l, &self.text), chars(&r, &self.text));
        let in_l: HashSet<&String> = l.iter().map(|(id, _)| id).collect();
        let in_r: HashSet<&String> = r.iter().map(|(id, _)| id).collect();
        let stretches = |list: &[(String, usize)], other: &HashSet<&String>| {
            let mut out: Vec<Range> = Vec::new();
            let mut at = 0;
            for (id, units) in list {
                if !other.contains(id) {
                    match out.last_mut() {
                        Some(last) if last.end == at => last.end += units,
                        _ => out.push(Range { start: at, end: at + units }),
                    }
                }
                at += units;
            }
            out
        };
        Ok((stretches(&l, &in_r), stretches(&r, &in_l)))
    }

    /// A stable place in the text: it stays with the character at
    /// `position` (UTF-16) whatever is written around it.
    pub fn cursor(&self, position: usize) -> Result<String> {
        let cursor = if position >= self.doc.length(&self.text) {
            self.doc.get_cursor(&self.text, CursorPosition::End, None)?
        } else {
            self.doc.get_cursor(&self.text, position, None)?
        };
        Ok(cursor.to_string())
    }

    /// Where a stable place stands in the text now.
    pub fn position(&self, cursor: &str) -> Result<usize> {
        let cursor = Cursor::try_from(cursor).map_err(|e| e.to_string())?;
        Ok(self.doc.get_cursor_position(&self.text, &cursor, None)?)
    }

    /// Takes in changes that came from elsewhere. Whatever waits for changes
    /// not here yet stays queued until they come.
    pub fn apply(&mut self, changes: Vec<Change>) -> Result<()> {
        self.doc.apply_changes(changes)?;
        Ok(())
    }

    /// Whether every one of these changes is in the document.
    pub fn has_all(&mut self, hashes: &[ChangeHash]) -> bool {
        hashes.iter().all(|h| self.doc.get_change_by_hash(h).is_some())
    }

    /// Everything these changes build on — including themselves.
    pub fn ancestry(&mut self, from: &[ChangeHash]) -> HashSet<ChangeHash> {
        let mut seen = HashSet::new();
        let mut open: Vec<ChangeHash> = from.to_vec();
        while let Some(h) = open.pop() {
            if !seen.insert(h) {
                continue;
            }
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                open.extend(change.deps().iter().copied());
            }
        }
        seen
    }

    /// Where a set of changes started: what they depend on, without them.
    pub fn before(&mut self, changes: &[ChangeHash]) -> Vec<ChangeHash> {
        let inside: BTreeSet<ChangeHash> = changes.iter().copied().collect();
        let mut out = BTreeSet::new();
        for h in changes {
            if let Some(change) = self.doc.get_change_by_hash(h) {
                out.extend(change.deps().iter().copied().filter(|d| !inside.contains(d)));
            }
        }
        out.into_iter().collect()
    }

    /// The newest changes of a set of changes and everything below them.
    pub fn tips(&mut self, set: &HashSet<ChangeHash>) -> Vec<ChangeHash> {
        let mut below = HashSet::new();
        for h in set {
            if let Some(change) = self.doc.get_change_by_hash(h) {
                below.extend(change.deps().iter().copied());
            }
        }
        let mut tips: Vec<ChangeHash> = set.iter().filter(|h| !below.contains(*h)).copied().collect();
        tips.sort();
        tips
    }

    /// Takes back what `changes` did — as a new change on top of them, merged
    /// into now. What others wrote meanwhile stays: a deletion takes exactly
    /// the characters that were inserted, not whatever stands there now.
    pub fn revert(&mut self, changes: &[ChangeHash], author: &str, session: &str, time: u64) -> Result<Vec<Change>> {
        let before_heads = self.before(changes);
        let before = self.plain_at(&before_heads)?;
        let mut after_set = self.ancestry(changes);
        after_set.extend(changes.iter().copied());
        let after_heads = self.tips(&after_set);
        let after = self.plain_at(&after_heads)?;
        // Only what the changes touched goes back.
        let mut wanted = after.clone();
        if before.title != after.title {
            wanted.title = before.title.clone();
        }
        wanted.text = before.text.clone();
        let keys: BTreeSet<&String> = before.fields.keys().chain(after.fields.keys()).collect();
        for key in keys {
            if before.fields.get(key) != after.fields.get(key) {
                match before.fields.get(key) {
                    Some(v) => wanted.fields.insert(key.clone(), v.clone()),
                    None => wanted.fields.remove(key),
                };
            }
        }
        self.edit(&after_heads, &wanted, author, session, time)
    }

    /// The document as it stood at `heads`, saved whole — what a version
    /// holds of a layer.
    pub fn save_at(&mut self, heads: &[ChangeHash]) -> Result<Vec<u8>> {
        Ok(self.doc.fork_at(heads)?.save())
    }
}

/// What was written against `seen`, made again on top of `current`: the
/// text as one splice, each field and the title only where it changed.
pub fn carried(seen: &Plain, wanted: &Plain, current: &Plain) -> Plain {
    let mut out = current.clone();
    if wanted.title != seen.title {
        out.title = wanted.title.clone();
    }
    out.text = carry_text(&seen.text, &wanted.text, &current.text);
    for key in seen.fields.keys().chain(wanted.fields.keys()) {
        if seen.fields.get(key) != wanted.fields.get(key) {
            match wanted.fields.get(key) {
                Some(v) => out.fields.insert(key.clone(), v.clone()),
                None => out.fields.remove(key),
            };
        }
    }
    // What the carried text shows of images comes along with its keys.
    for (key, image) in &wanted.images {
        out.images.entry(key.clone()).or_insert_with(|| image.clone());
    }
    out
}

/// Where two texts differ: common start, end of the changed stretch in
/// `before`, and what replaces it.
fn splice(before: &[char], after: &[char]) -> (usize, usize, Vec<char>) {
    let start = before.iter().zip(after).take_while(|(x, y)| x == y).count();
    let tail = before[start..].iter().rev().zip(after[start..].iter().rev()).take_while(|(x, y)| x == y).count();
    (start, before.len() - tail, after[start..after.len() - tail].to_vec())
}

/// What `ours` changed against `base`, made again on top of `theirs` — the
/// same rule the editor follows (`shared/editor/Merge.ts`).
fn carry_text(base: &str, ours: &str, theirs: &str) -> String {
    let (base, ours, theirs): (Vec<char>, Vec<char>, Vec<char>) =
        (base.chars().collect(), ours.chars().collect(), theirs.chars().collect());
    if ours == base {
        return theirs.iter().collect();
    }
    if theirs == base {
        return ours.iter().collect();
    }
    let (start, end, insert) = splice(&base, &ours);
    let (other_start, other_end, other_insert) = splice(&base, &theirs);
    let join = |at: usize, until: usize| theirs[..at].iter().chain(&insert).chain(&theirs[until..]).collect();
    if end <= other_start {
        return join(start, end);
    }
    if start >= other_end {
        let shift = |p: usize| (p + theirs.len()).saturating_sub(base.len());
        return join(shift(start), shift(end));
    }
    // In the same place: nothing of theirs is lost, ours comes after it.
    let at = other_start + other_insert.len();
    join(at, at)
}

/// The lines two texts differ in — as ranges of `base`'s lines that were
/// replaced. An insertion is an empty range.
pub fn changed_lines(base: &str, other: &str) -> Vec<(usize, usize)> {
    let a: Vec<&str> = base.lines().collect();
    let b: Vec<&str> = other.lines().collect();
    // Common start and end cost nothing; only the middle needs the table.
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a_mid, b_mid) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    if a_mid.is_empty() && b_mid.is_empty() {
        return Vec::new();
    }
    // Too big for the table: the whole middle counts as one change.
    if a_mid.len() * b_mid.len() > 4_000_000 {
        return vec![(head, head + a_mid.len())];
    }
    let (n, m) = (a_mid.len(), b_mid.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a_mid[i] == b_mid[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<usize> = None;
    while i < n || j < m {
        if i < n && j < m && a_mid[i] == b_mid[j] {
            if let Some(start) = open.take() {
                out.push((head + start, head + i));
            }
            i += 1;
            j += 1;
        } else {
            open.get_or_insert(i);
            if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
                j += 1;
            } else {
                i += 1;
            }
        }
    }
    if let Some(start) = open {
        out.push((head + start, head + n));
    }
    out
}

/// Whether two sets of changed line ranges touch the same place.
pub fn overlapping(a: &[(usize, usize)], b: &[(usize, usize)]) -> bool {
    a.iter().any(|&(a1, a2)| {
        b.iter().any(|&(b1, b2)| {
            (a1 < b2 && b1 < a2) || ((a1 == a2 || b1 == b2) && a1 <= b2 && b1 <= a2)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Plain {
        Plain { title: "Tavern".into(), text: text.into(), ..Plain::default() }
    }

    #[test]
    fn concurrent_edits_in_different_places_both_stay() {
        let mut doc = LayerDoc::genesis(&plain("one\ntwo\nthree"), "lay", 1).unwrap();
        let base = doc.doc.get_heads();
        doc.edit(&base, &plain("ONE\ntwo\nthree"), "id:a", "s", 1).unwrap();
        doc.edit(&base, &plain("one\ntwo\nTHREE"), "id:b", "s", 1).unwrap();
        assert_eq!(doc.plain().unwrap().text, "ONE\ntwo\nTHREE");
    }

    #[test]
    fn two_typing_at_the_same_spot_letter_by_letter_both_stay() {
        let start = "Paragraph 13: at the pub.\n\nParagraph 14: on.";
        let mut doc = LayerDoc::genesis(&plain(start), "lay", 1).unwrap();
        let base = doc.doc.get_heads();
        // Each writes on what they last saw of their own — not the other's.
        let (mut a_heads, mut b_heads) = (base.clone(), base);
        let (mut a_text, mut b_text) = (start.to_string(), start.to_string());
        let (a_typed, b_typed) = (" GM thirteen.", " Bob thirteen, same paragraph.");
        for n in 0..a_typed.len().max(b_typed.len()) {
            for (who, typed, text, heads) in [("id:a", a_typed, &mut a_text, &mut a_heads), ("id:b", b_typed, &mut b_text, &mut b_heads)] {
                let Some(c) = typed.chars().nth(n) else { continue };
                let at = text.find("\n\n").unwrap();
                text.insert(at, c);
                let changes = doc.edit(heads, &plain(text), who, "s", 1).unwrap();
                *heads = changes.iter().map(|c| c.hash()).collect();
            }
        }
        let now = doc.plain().unwrap().text;
        assert!(now.contains("GM thirteen.") && now.contains("Bob thirteen, same paragraph."), "{now}");
    }

    #[test]
    fn one_who_wrote_around_what_came_in_keeps_their_own() {
        let para = |n: u32, more: &str| format!("Paragraph {n}: The group walks on to the pub.{more}");
        let text = |more: &str| [para(15, ""), para(16, more), para(17, "")].join("\n\n");
        let mut doc = LayerDoc::genesis(&plain(&text("")), "lay", 1).unwrap();
        let base = doc.doc.get_heads();
        let heads = |c: &[Change]| c.iter().map(|c| c.hash()).collect::<Vec<_>>();
        // The GM types; Bob reads the first letter.
        let mut gm = heads(&doc.edit(&base, &plain(&text(" G")), "id:gm", "s1", 1).unwrap());
        let bob_base = gm.clone();
        for more in [" GM", " GM ", " GM 1"] {
            let c = doc.edit(&gm, &plain(&text(more)), "id:gm", "s1", 1).unwrap();
            if !c.is_empty() {
                gm = heads(&c);
            }
        }
        // Bob writes before the G he saw.
        doc.edit(&bob_base, &plain(&text(" Bob 16  G")), "id:bob", "s2", 1).unwrap();
        let now = doc.plain().unwrap().text;
        assert!(now.contains("Bob 16") && now.contains("M 1"), "{now}");
    }

    #[test]
    fn a_revert_takes_back_only_its_own_changes() {
        let mut doc = LayerDoc::genesis(&plain("The inn is old."), "lay", 1).unwrap();
        let base = doc.doc.get_heads();
        let mine: Vec<ChangeHash> = doc
            .edit(&base, &plain("The inn is old and dark."), "id:bob", "s", 1)
            .unwrap()
            .iter()
            .map(|c| c.hash())
            .collect();
        let now = doc.doc.get_heads();
        doc.edit(&now, &plain("Rain. The inn is old and dark."), "id:mia", "s", 1).unwrap();
        doc.revert(&mine, "id:gm", "s", 1).unwrap();
        assert_eq!(doc.plain().unwrap().text, "Rain. The inn is old.");
    }

    #[test]
    fn marks_come_from_the_characters_themselves() {
        let mut doc = LayerDoc::genesis(&plain("aaaa and more"), "lay", 1).unwrap();
        let before = doc.doc.get_heads();
        doc.edit(&before, &plain("aaabbb and more"), "id:bob", "s", 1).unwrap();
        let after = doc.doc.get_heads();
        let cut = |text: &str, ranges: &[Range]| -> Vec<String> {
            let u: Vec<u16> = text.encode_utf16().collect();
            ranges.iter().map(|r| String::from_utf16(&u[r.start..r.end]).unwrap()).collect()
        };
        let (gone, added) = doc.marks(&before, &after).unwrap();
        assert_eq!(cut("aaaa and more", &gone), ["a"]);
        assert_eq!(cut("aaabbb and more", &added), ["bbb"]);
        // Concurrent writing elsewhere does not show as this change.
        doc.edit(&after, &plain("aaabbb and more 😀!"), "id:mia", "s", 1).unwrap();
        let now = doc.doc.get_heads();
        let (gone, added) = doc.marks(&before, &now).unwrap();
        assert_eq!(cut("aaaa and more", &gone), ["a"]);
        assert_eq!(cut("aaabbb and more 😀!", &added), ["bbb", " 😀!"]);
    }

    #[test]
    fn fields_merge_key_by_key() {
        let mut start = plain("");
        start.fields.insert("mayor".into(), "Vellmar".into());
        let mut doc = LayerDoc::genesis(&start, "lay", 1).unwrap();
        let base = doc.doc.get_heads();
        let mut a = start.clone();
        a.fields.insert("climate".into(), "warm".into());
        let mut b = start.clone();
        b.fields.insert("mayor".into(), "Dati".into());
        doc.edit(&base, &a, "id:a", "s", 1).unwrap();
        doc.edit(&base, &b, "id:b", "s", 1).unwrap();
        let now = doc.plain().unwrap();
        assert_eq!(now.fields["climate"], "warm");
        assert_eq!(now.fields["mayor"], "Dati");
    }

    #[test]
    fn carrying_text_keeps_both_sides() {
        assert_eq!(carry_text("One. Two.", "One. Two. Three.", "Zero. One. Two."), "Zero. One. Two. Three.");
        assert_eq!(carry_text("One.", "One.", "One!"), "One!");
        assert_eq!(carry_text("One.", "Uno.", "One."), "Uno.");
    }

    #[test]
    fn line_changes_and_their_overlap() {
        let base = "a\nb\nc\nd";
        assert_eq!(changed_lines(base, "a\nB\nc\nd"), vec![(1, 2)]);
        assert_eq!(changed_lines(base, "a\nb\nx\nc\nd"), vec![(2, 2)]);
        assert!(overlapping(&changed_lines(base, "a\nB\nc\nd"), &changed_lines(base, "a\nBB\nc\nd")));
        assert!(!overlapping(&changed_lines(base, "A\nb\nc\nd"), &changed_lines(base, "a\nb\nc\nD")));
    }
}
