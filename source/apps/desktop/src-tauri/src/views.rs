//! A world as the interface reads it — in three askings, so that a large
//! one opens at once and no answer outgrows what a message holds:
//!
//! - `view_index`: every document without its text — names, types, access,
//!   property values. Enough for the outline, links, lists and graphs. In
//!   parts, by id, each after the one before; what changes meanwhile comes
//!   with `view_since` from where the first part began.
//! - `view_texts`: whole documents by id — the page one opens first, then
//!   the rest, a part at a time.
//! - `view_since`: what changed after a write — the documents themselves,
//!   or that everything has to be read anew.
//!
//! Each answer is JSON text, written once as it goes over the wire.

use super::*;
use aw_store::db::Since;

/// How much text one answer carries — small, so that each comes at once.
const PART: usize = 2 * 1024 * 1024;
/// More changed documents than this come by id, read like the rest.
const MANY: usize = 400;

/// What changes how documents are seen: the reader, and the world opened.
fn state(store: &Store, world: &str, reader: &Reader) -> Answer<(u64, String)> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    reader.key().hash(&mut hash);
    Ok((store.db(world)?.epoch(), format!("{:016x}", hash.finish())))
}

/// Shapes the documents into what the interface gets.
struct Viewer {
    reader: Reader,
    /// What one wrote that waits for the game master stands there for oneself.
    own_waiting: bool,
    /// The types, for the sections a player may start — none for the GM.
    templates: Vec<Template>,
    effective: std::collections::HashMap<String, Option<Template>>,
}

impl Viewer {
    fn new(store: &Store, world: &str, reader: Reader) -> Answer<Self> {
        let own_waiting = !reader.is_gm() && store.moderated(world)?;
        let templates = if reader.is_gm() { Vec::new() } else { store.load_templates(world)? };
        Ok(Viewer { reader, own_waiting, templates, effective: Default::default() })
    }

    /// A document as this reader sees it — none where they see nothing.
    fn view(&mut self, store: &Store, world: &str, doc: store::Document) -> Answer<Option<ArticleView>> {
        let doc = if self.own_waiting { document::with_own_waiting(store, world, doc)? } else { doc };
        let Some(mut view) = article_view(&doc, &self.reader) else { return Ok(None) };
        if !self.reader.is_gm() && !view.cover {
            let templates = &self.templates;
            let template = self
                .effective
                .entry(doc.doc_type.clone())
                .or_insert_with(|| aw_model::types::find(templates, &doc.doc_type).map(|t| aw_model::types::effective(templates, t)));
            view.startable = document::startable_of(templates, template.as_ref(), &doc, &self.reader);
        }
        Ok(Some(view))
    }
}

/// Every document `reader` sees, whole — for the tests, which look at all of them at once.
#[cfg(test)]
pub(crate) fn whole(store: &Store, world: &str, reader: &Reader) -> Answer<Vec<ArticleView>> {
    let mut viewer = Viewer::new(store, world, reader.clone())?;
    let mut out = Vec::new();
    for doc in store.db(world)?.documents()? {
        out.extend(viewer.view(store, world, doc)?);
    }
    Ok(out)
}

/// A document without its text: what it is, not what it says.
fn without_text(mut view: ArticleView) -> ArticleView {
    for layer in &mut view.layers {
        if !document::is_infobox(&layer.section) {
            layer.text = String::new();
        }
    }
    view.partial = true;
    view
}

// --- The index ---------------------------------------------------------------

/// How many documents are read from the database at a time for the index.
const READ: usize = 300;

/// A part of the index: the documents after id `after`, as many as fit —
/// and the id the next part begins after, none after the last. `writes` is
/// the state this part began at: from the first part's, `view_since` brings
/// what changed while the rest was read.
pub(crate) fn index_in(store: &Store, world: &str, preview: Option<Vec<String>>, after: &str) -> Answer<String> {
    index_within(store, world, preview, after, PART)
}

/// `index_in` with a part's size given.
pub(crate) fn index_within(store: &Store, world: &str, preview: Option<Vec<String>>, after: &str, budget: usize) -> Answer<String> {
    let reader = reader_in(store, world, preview)?;
    let (epoch, seen) = state(store, world, &reader)?;
    let db = store.db(world)?;
    let writes = db.writes();
    let mut viewer = Viewer::new(store, world, reader)?;
    let (mut articles, mut size, mut last) = (Vec::new(), 0, after.to_string());
    let mut more = true;
    'reading: while more {
        let docs = db.documents_after(&last, READ)?;
        more = docs.len() == READ;
        for doc in docs {
            let id = doc.id.clone();
            if let Some(view) = viewer.view(store, world, doc)? {
                let text = json_text(&without_text(view))?;
                if !articles.is_empty() && size + text.len() > budget {
                    more = true;
                    break 'reading;
                }
                size += text.len() + 1;
                articles.push(text);
            }
            last = id;
        }
    }
    let next = if more { json_text(&last)? } else { "null".into() };
    Ok(format!(
        "{{\"epoch\":{epoch},\"writes\":{writes},\"reader\":\"{seen}\",\"next\":{next},\"articles\":[{}]}}",
        articles.join(",")
    ))
}

fn json_text<T: Serialize>(value: &T) -> Answer<String> {
    Ok(serde_json::to_string(value)?)
}

// --- Whole documents -----------------------------------------------------------

/// Whole documents by id, as many as fit in one answer — at least one. What
/// did not fit is `rest`; what this reader does not see, or what is gone, is
/// `gone`.
pub(crate) fn texts_in(store: &Store, world: &str, preview: Option<Vec<String>>, ids: Vec<String>) -> Answer<String> {
    let reader = reader_in(store, world, preview)?;
    let db = store.db(world)?;
    let mut viewer = Viewer::new(store, world, reader)?;
    let (mut articles, mut gone, mut size) = (Vec::new(), Vec::new(), 0);
    let mut ids = ids.into_iter();
    let mut rest = Vec::new();
    for id in ids.by_ref() {
        let view = match db.document(&id)? {
            Some(doc) => viewer.view(store, world, doc)?,
            None => None,
        };
        let Some(view) = view else {
            gone.push(id);
            continue;
        };
        let text = json_text(&view)?;
        if !articles.is_empty() && size + text.len() > PART {
            rest.push(id);
            break;
        }
        size += text.len() + 1;
        articles.push(text);
    }
    rest.extend(ids);
    Ok(format!(
        "{{\"articles\":[{}],\"gone\":{},\"rest\":{}}}",
        articles.join(","),
        json_text(&gone)?,
        json_text(&rest)?
    ))
}

// --- What changed --------------------------------------------------------------

/// What changed after write `since` of opening `epoch`: the documents whole
/// as they are now, and those gone or no longer seen. Many of them — a
/// package read in — only by id, `changed`, to be read like the rest. Only
/// another opening, a journal that no longer reaches back, other types or
/// other roles of the reader need the index anew: `whole`.
pub(crate) fn since_in(store: &Store, world: &str, preview: Option<Vec<String>>, epoch: u64, since: u64) -> Answer<String> {
    let reader = reader_in(store, world, preview.clone())?;
    let (now_epoch, seen) = state(store, world, &reader)?;
    let (writes, changed) = store.db(world)?.changed_since(since);
    let head = format!("\"epoch\":{now_epoch},\"writes\":{writes},\"reader\":\"{seen}\"");
    let ids = match changed {
        Since::Documents(ids) if epoch == now_epoch => ids,
        _ => return Ok(format!("{{{head},\"whole\":true}}")),
    };
    if ids.len() > MANY {
        return Ok(format!("{{{head},\"whole\":false,\"articles\":[],\"gone\":[],\"changed\":{}}}", json_text(&ids)?));
    }
    let part: serde_json::Value = serde_json::from_str(&texts_in(store, world, preview, ids)?)?;
    Ok(format!(
        "{{{head},\"whole\":false,\"articles\":{},\"gone\":{},\"changed\":{}}}",
        part["articles"], part["gone"], part["rest"]
    ))
}

#[cfg(test)]
#[path = "views_tests.rs"]
mod tests;
