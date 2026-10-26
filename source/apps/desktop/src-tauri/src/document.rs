//! A document as code: opening, reading, writing and saving it.
//!
//! The translation itself lives in `aw_model::document`. What happens here
//! needs to know who reads: which layers they may see, and what a changed
//! piece of text does to the layers.
//!
//! The rules on saving:
//!
//! - The code only ever holds what the reader may see. Layers they may not
//!   see are not in it — and are left exactly as they are.
//! - Saving writes what changed against the state the editor started from,
//!   not against now: whatever someone else wrote meanwhile stays, and the
//!   two merge.
//! - Changing a part's text needs the right to edit it.
//! - Changing who edits a part, adding a part and removing one is the game
//!   master's. Who may edit a part may say who sees it.

use super::*;
use aw_model::document::{self, DocumentRead, DocumentText, Property, Shape};
use aw_model::link;
use aw_model::names::{self, Catalog};
use aw_model::relation::{self, Entry};
use aw_model::template::{Access as TypeAccess, BlockPart, FieldKind, TemplateBlock};
use aw_store::diff::{self, Range};
use std::collections::BTreeMap;

type Access = TypeAccess;

/// Key of the layer that carries the properties.
pub(crate) const INFOBOX_SECTION: &str = "properties";

/// A property with access of its own lies in a layer of its own:
/// `properties:<key>`.
fn field_section(key: &str) -> String {
    format!("{INFOBOX_SECTION}:{key}")
}

/// Whether a layer carries property values — the common one or one's own.
pub(crate) fn is_infobox(section: &str) -> bool {
    section == INFOBOX_SECTION || section.strip_prefix(INFOBOX_SECTION).is_some_and(|r| r.starts_with(':'))
}

/// What a reader sees of a layer.
#[derive(Clone, Debug)]
pub(crate) struct LayerContent {
    pub title: String,
    pub text: String,
    /// Key of the section in the template.
    pub section: String,
    /// Who may change it.
    pub edit: String,
    /// Who sees it, as written.
    pub own: String,
    /// The template fields of this section, key → value.
    pub fields: serde_json::Value,
}

/// A layer as `reader` sees it — or why not.
fn layer_content(layer: &store::Layer, reader: &Reader) -> Result<LayerContent, AppError> {
    if !reader.sees(&layer.access) {
        return Err(AppError("This part is not there for you.".into()));
    }
    Ok(LayerContent {
        title: layer.title.clone(),
        text: layer.text.clone(),
        section: layer.section.clone(),
        edit: layer.access.edit.clone(),
        own: layer.access.view.clone(),
        fields: fields_value(&layer.fields),
    })
}

/// What a layer holds that its writer changes: title, text, fields, images.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Plain {
    pub title: String,
    pub text: String,
    pub fields: BTreeMap<String, serde_json::Value>,
    pub images: Vec<String>,
}

impl Plain {
    fn of(layer: &store::Layer) -> Plain {
        Plain { title: layer.title.clone(), text: layer.text.clone(), fields: layer.fields.clone().into_iter().collect(), images: layer.images.clone() }
    }

    fn into_layer(self, layer: &mut store::Layer) {
        layer.title = self.title;
        layer.text = self.text;
        layer.fields = self.fields.into_iter().collect();
        layer.images = self.images;
    }
}

/// A document opened for one reader.
struct Opened {
    /// The document, whole, as it is stored.
    doc: store::Document,
    /// The article's id.
    id: String,
    text: DocumentText,
    shape: Shape,
    /// The type with everything it inherits — `None` without one.
    template: Option<Template>,
    /// Readable layers by section: position in `doc.layers` and what the reader sees.
    readable: Vec<(String, usize, LayerContent)>,
    /// The base layer: its position and what the reader sees — name and access.
    base: (usize, LayerContent),
    /// Every document and type the reader knows — for names in the text.
    known: std::sync::Arc<Known>,
    /// The document's type by id — whose properties its text uses.
    article_type: String,
    /// Whether the reader is the game master.
    gm: bool,
    /// The world's calendar — how its dates are written.
    calendar: aw_model::WorldCalendar,
}

/// A document a link can point at.
#[derive(Clone)]
struct Named {
    id: String,
    name: String,
    /// Its type by id — empty for a document without one.
    article_type: String,
    /// The name of its type, then those of the types above it — a link to
    /// `@@Place:Paris@@` finds Paris when it is a city.
    types: Vec<String>,
}

impl Named {
    fn is(&self, link: &link::Link) -> bool {
        self.id == link.name
            || (self.name.eq_ignore_ascii_case(&link.name)
                && (link.of.is_empty() || self.types.iter().any(|t| t.eq_ignore_ascii_case(&link.of))))
    }

    /// How a link to it is written.
    fn link(&self) -> String {
        link::write(self.types.first().map(String::as_str).unwrap_or_default(), &self.name)
    }
}

/// What this reader knows by name: the documents they can see, the types.
pub(crate) struct Known {
    names: Vec<Named>,
    templates: Vec<Template>,
}

/// The same, kept per world, reader and state of the world: every keystroke
/// in an editor asks for it — with thousands of documents, that is what
/// typing would wait for.
fn known_shared(store: &Store, world: &str, reader: &Reader, templates: Vec<Template>) -> Result<std::sync::Arc<Known>, AppError> {
    type Kept = std::collections::HashMap<(PathBuf, String, String), (u64, std::sync::Arc<Known>)>;
    static KEPT: std::sync::OnceLock<std::sync::Mutex<Kept>> = std::sync::OnceLock::new();
    let db = store.db(world)?;
    let at = (store.world_dir(world), world.to_string(), reader.key());
    let revision = db.revision();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((r, known)) = kept.lock().ok().and_then(|m| m.get(&at).cloned()) {
        if r == revision && known.templates == templates {
            return Ok(known);
        }
    }
    let documents = db.names_for(reader)?;
    let fresh = std::sync::Arc::new(known_of(documents, templates));
    if let Ok(mut m) = kept.lock() {
        m.insert(at, (revision, fresh.clone()));
    }
    Ok(fresh)
}

/// The same, from documents as viewed.
pub(crate) fn known_from(views: Vec<crate::ArticleView>, templates: Vec<Template>) -> Known {
    known_of(views.into_iter().map(|a| (a.id, a.name, a.article_type)).collect(), templates)
}

/// The same from id, name and type of each document — documents not stored
/// yet among them, as a package brings them.
pub(crate) fn known_of(documents: Vec<(String, String, String)>, templates: Vec<Template>) -> Known {
    let names = documents
        .into_iter()
        .map(|(id, name, article_type)| Named {
            types: types::find(&templates, &article_type)
                .map(|t| std::iter::once(t).chain(types::ancestors(&templates, t)).map(|t| t.name.clone()).collect())
                .unwrap_or_default(),
            id,
            name,
            article_type,
        })
        .collect();
    Known { names, templates }
}

impl Catalog for Known {
    fn document_id(&self, of: &str, name: &str) -> Option<String> {
        let link = link::Link::to(of, name);
        self.names.iter().find(|n| n.is(&link)).map(|n| n.id.clone())
    }
    fn document_name(&self, id: &str) -> Option<(String, String)> {
        self.names.iter().find(|n| n.id == id).map(|n| (n.types.first().cloned().unwrap_or_default(), n.name.clone()))
    }
    fn document_type(&self, id: &str) -> Option<String> {
        self.names.iter().find(|n| n.id == id).map(|n| n.article_type.clone())
    }
    fn type_id(&self, name: &str) -> Option<String> {
        let name = name.trim();
        self.templates.iter().find(|t| t.id == name || t.name.eq_ignore_ascii_case(name)).map(|t| t.id.clone())
    }
    fn type_name(&self, id: &str) -> Option<String> {
        types::find(&self.templates, id).map(|t| t.name.clone())
    }
    fn property_key(&self, owner: &str, name: &str) -> Option<String> {
        let t = types::find(&self.templates, owner)?;
        types::effective(&self.templates, t).property(name).map(|p| p.key.clone())
    }
    fn property(&self, key: &str) -> Option<(String, bool)> {
        self.templates
            .iter()
            .flat_map(|t| t.properties.iter())
            .find(|p| p.key == key)
            .map(|p| (p.label.clone(), matches!(p.kind, FieldKind::Reference { .. })))
    }
}

impl Opened {
    fn layer(&self, section: &str) -> Option<&(String, usize, LayerContent)> {
        self.readable.iter().find(|(s, _, _)| s == section)
    }

    fn layer_id(&self, i: usize) -> &str {
        &self.doc.layers[i].id
    }
}

/// The state of a layer an editor starts from: its content's hash, under
/// which the holder keeps what it handed out (`seen`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Base {
    pub generation: u32,
    pub heads: Vec<String>,
}

/// Every layer the editor sees, by id — with the state it saw.
pub type Bases = BTreeMap<String, Base>;

/// What the frontend gets when a document is opened for editing.
#[derive(Serialize)]
pub struct DocumentCode {
    pub code: String,
    pub document: DocumentText,
    pub base: Bases,
}

/// The states of layers handed out to editors, by their hash — what a save
/// is measured against. Kept in memory: after a restart an editor's state
/// is gone, and its save counts against the layer as it stands.
fn handed_out() -> &'static std::sync::Mutex<(std::collections::HashMap<String, Plain>, std::collections::VecDeque<String>)> {
    static KEPT: std::sync::OnceLock<std::sync::Mutex<(std::collections::HashMap<String, Plain>, std::collections::VecDeque<String>)>> = std::sync::OnceLock::new();
    KEPT.get_or_init(Default::default)
}

/// How many states stay kept — enough for many editors on many documents.
const KEPT_STATES: usize = 20_000;

/// The hash a layer's state is known by.
fn state_hash(layer: &str, plain: &Plain) -> String {
    let mut h = blake3::Hasher::new();
    h.update(layer.as_bytes());
    for piece in [&plain.title, &plain.text] {
        h.update(&(piece.len() as u64).to_le_bytes());
        h.update(piece.as_bytes());
    }
    h.update(serde_json::to_string(&plain.fields).unwrap_or_default().as_bytes());
    h.update(plain.images.join(",").as_bytes());
    h.finalize().to_hex().to_string()
}

/// Keeps a state under its hash, for the save to come.
fn keep_state(hash: &str, plain: Plain) {
    if let Ok(mut kept) = handed_out().lock() {
        let (states, order) = &mut *kept;
        if states.insert(hash.to_string(), plain).is_none() {
            order.push_back(hash.to_string());
            while order.len() > KEPT_STATES {
                if let Some(old) = order.pop_front() {
                    states.remove(&old);
                }
            }
        }
    }
}

/// The hash a layer's state is known by — and kept under, for the save to come.
fn hand_out(layer: &store::Layer) -> Base {
    let plain = Plain::of(layer);
    let hash = state_hash(&layer.id, &plain);
    keep_state(&hash, plain);
    Base { generation: 0, heads: vec![hash] }
}

/// Forgets every state handed out — as another device, or a restart, would.
#[cfg(test)]
pub(crate) fn forget_states() {
    if let Ok(mut kept) = handed_out().lock() {
        *kept = Default::default();
    }
}

/// The states a save is measured against, by layer — to go along with a
/// save written elsewhere (a mailbox), where they are not kept.
pub(crate) fn states_of(base: &Bases) -> BTreeMap<String, Plain> {
    base.keys().filter_map(|layer| Some((layer.clone(), seen_state(base, layer)?))).collect()
}

/// Takes in states that came along with a save — each only under the hash
/// it really has.
pub(crate) fn take_states(base: &Bases, states: BTreeMap<String, Plain>) {
    for (layer, plain) in states {
        let hash = state_hash(&layer, &plain);
        if base.get(&layer).and_then(|b| b.heads.first()) == Some(&hash) {
            keep_state(&hash, plain);
        }
    }
}

/// A layer's state as an editor saw it — `None` where it was not handed out
/// here, or no longer kept.
fn seen_state(base: &Bases, layer: &str) -> Option<Plain> {
    let hash = base.get(layer)?.heads.first()?;
    handed_out().lock().ok()?.0.get(hash).cloned()
}

fn open(store: &Store, world: &str, article: &str, reader: &Reader) -> Result<Opened, AppError> {
    let doc = store.db(world)?.document(article)?.ok_or_else(|| AppError("There is no such article.".into()))?;
    // What one wrote that waits for the game master stands there for oneself.
    let doc = if store.moderated(world)? && !reader.is_gm() { history::with_own_waiting(store, world, doc)? } else { doc };
    let templates = store.templates_for(world, reader)?;
    let calendar = store.load_world_meta(world)?.calendar;
    open_doc(store, world, doc, reader, templates, calendar)
}

fn open_doc(store: &Store, world: &str, doc: store::Document, reader: &Reader, templates: Vec<Template>, calendar: aw_model::WorldCalendar) -> Result<Opened, AppError> {
    let template = types::find(&templates, &doc.doc_type).map(|t| types::effective(&templates, t));
    let base_index = doc.layers.iter().position(|l| l.base).ok_or_else(|| AppError("This document has no base layer.".into()))?;
    let base_content = layer_content(&doc.layers[base_index], reader).map_err(|_| AppError("There is no such article.".into()))?;
    let visibility = base_content.own.clone();

    let mut readable = Vec::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        if layer.base {
            continue;
        }
        if let Ok(content) = layer_content(layer, reader) {
            readable.push((content.section.clone(), i, content));
        }
    }

    let known = known_shared(store, world, reader, templates.clone())?;
    let name = base_content.title.clone();
    let edit = base_content.edit.clone();
    let article_type = doc.doc_type.clone();

    let mut opened = Opened {
        id: doc.id.clone(),
        doc,
        text: DocumentText::default(),
        shape: Shape {
            declared: template.iter().flat_map(|t| t.properties.iter().map(|p| p.label.clone())).collect(),
            defaults: Access { view: visibility, edit },
        },
        template,
        readable,
        base: (base_index, base_content),
        known,
        article_type,
        gm: reader.is_gm(),
        calendar,
    };
    opened.text = DocumentText {
        name,
        access: opened.shape.defaults.clone(),
        infobox: infobox_of(&opened),
        properties: properties_of(&opened),
        blocks: blocks_of(&opened),
    };
    Ok(opened)
}

/// The infobox's own access — its layer's, or before it has one what the type
/// says. `None` where it is the document's.
fn infobox_of(opened: &Opened) -> Option<Access> {
    let access = match opened.layer(INFOBOX_SECTION) {
        Some((_, _, content)) => Access { view: content.own.clone(), edit: content.edit.clone() },
        None => opened.template.as_ref()?.infobox.access.clone()?,
    };
    let defaults = &opened.shape.defaults;
    (access.view != defaults.view || access.edit != defaults.edit).then_some(access)
}

/// Every infobox value this reader can open, from whichever layer holds it.
fn stored_fields(opened: &Opened) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for (section, _, content) in &opened.readable {
        if is_infobox(section) {
            if let Some(map) = content.fields.as_object() {
                out.extend(map.clone());
            }
        }
    }
    out
}

/// Whether this reader has a field at all. One with access of its own is
/// theirs only when its layer opens for them — or when they are the game
/// master, who can give it one.
fn has_field(opened: &Opened, field: &aw_model::template::TemplateField) -> bool {
    field.access.is_none() || opened.gm || opened.layer(&field_section(&field.key)).is_some()
}

/// The properties: the type's fields in their order, then the free ones.
fn properties_of(opened: &Opened) -> Vec<Property> {
    let fields = stored_fields(opened);
    let mut out = Vec::new();
    let mut known = std::collections::BTreeSet::new();

    for field in opened.template.iter().flat_map(|t| t.properties.iter()) {
        // An image is no property — it has its own place in the infobox.
        if matches!(field.kind, FieldKind::Image { .. }) || !has_field(opened, field) {
            continue;
        }
        known.insert(field.key.clone());
        let name = if field.label.is_empty() { field.key.clone() } else { field.label.clone() };
        let value = fields
            .get(&field.key)
            .map(|v| shown(opened, &field.kind, v))
            .unwrap_or_default();
        out.push(Property { name, value, group: field.in_group.clone() });
    }
    for (key, value) in &fields {
        if known.contains(key) || opened.template.iter().any(|t| t.properties.iter().any(|f| &f.key == key)) {
            continue;
        }
        out.push(Property {
            name: key.clone(),
            value: names::written(&plain(value), &*opened.known, &opened.article_type),
            group: Vec::new(),
        });
    }
    out
}

/// A stored value as it is written: references by the names of their targets.
fn shown(opened: &Opened, kind: &FieldKind, value: &serde_json::Value) -> String {
    if matches!(kind, FieldKind::Relation { .. }) {
        return relations_written(opened, value);
    }
    if let (FieldKind::WorldDate, Some(n)) = (kind, value.as_f64()) {
        return opened.calendar.format_moment(n);
    }
    if let FieldKind::Reference { count, .. } = kind {
        return picks(value)
            .into_iter()
            .filter(|(id, _)| !id.is_empty())
            .map(|(id, n)| {
                // What this reader cannot open keeps its id — otherwise saving
                // would silently drop a reference nobody here can see.
                let link = opened
                    .known
                    .names
                    .iter()
                    .find(|named| named.id == id)
                    .map(Named::link)
                    .unwrap_or_else(|| link::write("", &id));
                // A pick's own number stands after it: `@@Class:Fighter@@ 3`.
                match n {
                    Some(n) if !count.is_empty() => format!("{link} {}", plain(&n)),
                    _ => link,
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
    names::written(&plain(value), &*opened.known, &opened.article_type)
}

/// Relations as written: `@@Character:Olga@@ Mother/Daughter, …`.
///
/// An entry whose document this reader cannot open is left out — nobody
/// should change or remove what they do not even know. Saving keeps it.
fn relations_written(opened: &Opened, value: &serde_json::Value) -> String {
    let role = |id: &str| -> String {
        if id.is_empty() {
            return String::new();
        }
        // A role nobody here can open keeps its id, so it is not lost.
        opened.known.names.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_else(|| id.to_string())
    };
    relation::entries(value)
        .iter()
        .filter_map(|e| {
            let target = opened.known.names.iter().find(|n| n.id == e.with)?;
            Some(relation::write(&target.link(), &role(&e.role), &role(&e.own_role)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether this reader can open the document an entry names.
fn knows(opened: &Opened, entry: &Entry) -> bool {
    opened.known.names.iter().any(|n| n.id == entry.with)
}

/// The document a link names — by name, or by the id kept for one this
/// reader cannot open.
fn document_named(opened: &Opened, link: &link::Link, label: &str) -> Result<String, AppError> {
    opened
        .known
        .names
        .iter()
        .find(|n| n.is(link))
        .map(|n| n.id.clone())
        .or_else(|| link.name.starts_with("art:").then(|| link.name.clone()))
        .ok_or_else(|| {
            let name = if link.of.is_empty() { link.name.clone() } else { format!("{}:{}", link.of, link.name) };
            AppError(format!("{label}: there is no document “{name}” here."))
        })
}

/// Turns written relations into stored entries — and keeps the ones this
/// reader could not see.
fn relations_from(
    opened: &Opened,
    field: &aw_model::template::TemplateField,
    role_type: &str,
    written: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let label = if field.label.is_empty() { &field.key } else { &field.label };
    let role = |name: &str| -> Result<String, AppError> {
        if name.is_empty() || name.starts_with("art:") {
            return Ok(name.to_string());
        }
        opened
            .known
            .names
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case(name) && types::is_a(&*opened.known.templates, &n.article_type, role_type))
            .map(|n| n.id.clone())
            .ok_or_else(|| {
                let of = types::find(&*opened.known.templates, role_type).map(|t| t.name.clone()).unwrap_or_default();
                AppError(format!("{label}: “{name}” is no {of}."))
            })
    };
    let mut out: Vec<Entry> = Vec::new();
    for w in relation::read(written.unwrap_or_default()).map_err(|e| AppError(format!("{label}: {e}")))? {
        let with = document_named(opened, &w.link, label)?;
        if with == opened.id {
            return Err(AppError(format!("{label}: a document has no relation to itself.")));
        }
        let entry = Entry { with, role: role(&w.role)?, own_role: role(&w.own_role)? };
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    let before = stored_fields(opened).get(&field.key).map(relation::entries).unwrap_or_default();
    for locked in before.into_iter().filter(|e| !knows(opened, e)) {
        if !out.contains(&locked) {
            out.push(locked);
        }
    }
    Ok(relation::value(&out))
}

/// The picks of a reference as stored: ids — each with its own number where
/// the field counts (`with Level`), stored as `{"with": id, "count": 3}`.
pub(crate) fn picks(value: &serde_json::Value) -> Vec<(String, Option<serde_json::Value>)> {
    let one = |v: &serde_json::Value| match v {
        serde_json::Value::Object(m) => (m.get("with").map(plain).unwrap_or_default(), m.get("count").cloned()),
        other => (plain(other), None),
    };
    match value {
        serde_json::Value::Array(items) => items.iter().map(one).collect(),
        serde_json::Value::Null => Vec::new(),
        other => vec![one(other)],
    }
}

/// A pick as stored: its id, with its number where it has one.
fn pick_value(id: String, count: Option<serde_json::Value>) -> serde_json::Value {
    match count {
        Some(n) => serde_json::json!({ "with": id, "count": n }),
        None => serde_json::Value::String(id),
    }
}

fn plain(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(items) => items.iter().map(plain).collect::<Vec<_>>().join(", "),
        serde_json::Value::Number(n) => number_text(n),
        other => other.to_string(),
    }
}

/// A date and time of ours as stored — `2026-09-12T19:00` — from
/// `2026-09-12 19:00`, `2026-09-12T19:00`, `12.09.2026, 19:00` or a date alone.
fn moment_of_ours(text: &str) -> Option<String> {
    let text = text.trim();
    let (date, time) = match text.split_once(['T', ' ']) {
        Some((d, t)) => (d.trim_end_matches(','), Some(t.trim())),
        None => (text, None),
    };
    let numbers = |s: &str, sep: char| -> Option<Vec<u32>> { s.split(sep).map(|p| p.trim().parse().ok()).collect() };
    let (y, m, d) = match (numbers(date, '-'), numbers(date, '.')) {
        (Some(v), _) if v.len() == 3 => (v[0], v[1], v[2]),
        (_, Some(v)) if v.len() == 3 => (v[2], v[1], v[0]),
        _ => return None,
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1000 {
        return None;
    }
    let out = format!("{y:04}-{m:02}-{d:02}");
    match time.filter(|t| !t.is_empty()) {
        None => Some(out),
        Some(t) => {
            let hm = numbers(t, ':')?;
            match hm.as_slice() {
                [h, mi] | [h, mi, _] if *h < 24 && *mi < 60 => Some(format!("{out}T{h:02}:{mi:02}")),
                _ => None,
            }
        }
    }
}

/// A number as one writes it: `24`, not `24.0`.
fn number_text(n: &serde_json::Number) -> String {
    match n.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
        _ => n.to_string(),
    }
}

/// A number as stored: whole numbers as integers.
fn number_value(n: f64) -> serde_json::Value {
    if n.fract() == 0.0 && n.abs() < 1e15 { serde_json::json!(n as i64) } else { serde_json::json!(n) }
}

/// The order of a document's own sections — those its type does not set
/// out — by key, as its base layer keeps it. Sections it does not name
/// follow in the order they came.
pub fn own_order(fields: &serde_json::Value) -> Vec<String> {
    fields.get(OWN_ORDER).and_then(|o| o.as_array()).map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// Where the base layer keeps that order.
const OWN_ORDER: &str = "order";

/// Where a section of the document's own keeps how it looks — in its first part.
const OWN_SHAPE: &str = "shape";

/// How a section of the document's own looks: what a type says of its blocks.
#[derive(serde::Serialize, serde::Deserialize, PartialEq)]
struct OwnShape {
    #[serde(default)]
    style: aw_model::template::BlockStyle,
    #[serde(default)]
    place: aw_model::template::BlockPlace,
    #[serde(default)]
    fold: aw_model::template::BlockFold,
    #[serde(default = "first_heading")]
    heading: u8,
    /// The types its create button makes, by id.
    #[serde(default)]
    new: Vec<String>,
    /// At the side: how wide, in pixels.
    #[serde(default)]
    width: u16,
}

fn first_heading() -> u8 {
    1
}

impl Default for OwnShape {
    fn default() -> Self {
        OwnShape { style: Default::default(), place: Default::default(), fold: Default::default(), heading: 1, new: Vec::new(), width: 0 }
    }
}

impl OwnShape {
    fn of(fields: &serde_json::Value) -> OwnShape {
        fields.get(OWN_SHAPE).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }
    fn from_block(block: &TemplateBlock, known: &impl Catalog) -> OwnShape {
        OwnShape { style: block.style, place: block.place, fold: block.fold, heading: block.heading, new: types_as(known, &block.new, true), width: block.width }
    }
    /// Put into a part's fields — or taken out, where it is as without.
    fn put(self, fields: &mut std::collections::BTreeMap<String, serde_json::Value>) {
        if self == OwnShape::default() {
            fields.remove(OWN_SHAPE);
        } else {
            fields.insert(OWN_SHAPE.into(), serde_json::to_value(self).expect("a shape is JSON"));
        }
    }
}

/// Types by id, as stored — or by name, as written: what stays unknown, as it is.
pub(crate) fn types_as(known: &impl Catalog, types: &[String], to_ids: bool) -> Vec<String> {
    types.iter().map(|t| if to_ids { known.type_id(t) } else { known.type_name(t) }.unwrap_or_else(|| t.clone())).collect()
}


/// Which part of which block a section is: `history-3` is part 2 of `history`.
fn part_index(section: &str, block: &str) -> Option<usize> {
    if section == block {
        return Some(0);
    }
    let n: usize = section.strip_prefix(block)?.strip_prefix('-')?.parse().ok()?;
    (n >= 2).then(|| n - 1)
}

/// The blocks as far as the reader can see them.
fn blocks_of(opened: &Opened) -> Vec<TemplateBlock> {
    // Which blocks there are: the type's first — and sections no template
    // block knows after them, they are content too. Without a type every
    // section is one of those.
    let mut wanted: Vec<(String, String, bool, OwnShape)> = Vec::new();
    if let Some(template) = &opened.template {
        for block in &template.blocks {
            let shape = OwnShape { new: types_as(&*opened.known, &block.new, false), ..OwnShape::from_block(block, &*opened.known) };
            wanted.push((block.key.clone(), block.title.clone(), true, shape));
        }
    }
    for (section, _, content) in &opened.readable {
        if is_infobox(section) || wanted.iter().any(|(k, ..)| part_index(section, k).is_some()) {
            continue;
        }
        // A section of the document's own keeps how it looks in its first part.
        let shape = OwnShape::of(&content.fields);
        let shape = OwnShape { new: types_as(&*opened.known, &shape.new, false), ..shape };
        wanted.push((section.clone(), content.title.clone(), false, shape));
    }
    // The document's own sections in the order it keeps — see `own_order`.
    let order = own_order(&opened.base.1.fields);
    let rank = |key: &str| order.iter().position(|k| k == key).unwrap_or(usize::MAX);
    let first_own = wanted.iter().position(|(_, _, of_type, _)| !of_type).unwrap_or(wanted.len());
    wanted[first_own..].sort_by_key(|(key, ..)| rank(key));

    let mut out = Vec::new();
    for (key, title, keep_empty, OwnShape { style, place, fold, heading, new, width }) in wanted {
        let mut parts: Vec<(usize, BlockPart)> = opened
            .readable
            .iter()
            .filter(|(s, _, _)| !is_infobox(s))
            .filter_map(|(section, _, content)| {
                let n = part_index(section, &key)?;
                Some((
                    n,
                    BlockPart {
                        key: section.clone(),
                        text: names::written(&content.text, &*opened.known, &opened.article_type),
                        view: content.own.clone(),
                        edit: content.edit.clone(),
                    },
                ))
            })
            .collect();
        parts.sort_by_key(|(n, _)| *n);
        // An empty block is shown only to the game master: only they can
        // start a new part in it. A closed layer does not say which block it
        // belongs to, so for anyone else “empty” may just mean “closed”.
        if parts.is_empty() && !(keep_empty && opened.gm) {
            continue;
        }
        // Empty, it has the parts its type sets out — empty, to write in. Left
        // empty they stay no layer.
        if parts.is_empty() {
            let set_out = opened.template.iter().flat_map(|t| &t.blocks).find(|b| b.key == key);
            parts = set_out
                .map(|b| b.parts.iter().enumerate().map(|(n, p)| (n, BlockPart { text: String::new(), ..p.clone() })).collect())
                .unwrap_or_default();
        }
        out.push(TemplateBlock {
            key,
            title,
            style,
            parts: parts.into_iter().map(|(_, p)| p).collect(),
            place,
            fold,
            heading,
            new,
            width,
        });
    }
    out
}

/// Turns the written properties into the stored values.
fn fields_from(opened: &Opened, properties: &[Property]) -> Result<serde_json::Value, AppError> {
    let mut fields = serde_json::Map::new();
    let mut used = std::collections::BTreeSet::new();

    for field in opened.template.iter().flat_map(|t| t.properties.iter()) {
        let label = if field.label.is_empty() { &field.key } else { &field.label };
        // An image keeps whatever it had — it is not written as a property.
        if matches!(field.kind, FieldKind::Image { .. }) {
            if let Some(old) = stored_fields(opened).get(&field.key).cloned() {
                fields.insert(field.key.clone(), old);
            }
            continue;
        }
        let property = properties.iter().find(|p| p.name.eq_ignore_ascii_case(label));
        // Relations are kept even without a line: what this reader cannot
        // see is still there.
        if let FieldKind::Relation { role_type, .. } = &field.kind {
            if let Some(p) = property {
                used.insert(p.name.to_lowercase());
            }
            let value = relations_from(opened, field, role_type, property.map(|p| p.value.as_str()))?;
            if value.as_array().is_some_and(|a| !a.is_empty()) {
                fields.insert(field.key.clone(), value);
            }
            continue;
        }
        let Some(property) = property else {
            continue;
        };
        used.insert(property.name.to_lowercase());
        if property.value.trim().is_empty() {
            continue;
        }
        let value = match &field.kind {
            FieldKind::Reference { multiple, count, every, .. } => {
                // With a number per pick, the words after each link are its
                // number: `@@Class:Fighter@@ 3, @@Class:Rogue@@ 2`.
                let mut numbers: Vec<Option<serde_json::Value>> = Vec::new();
                if !count.is_empty() {
                    for piece in link::scan(&property.value) {
                        match piece {
                            link::Piece::Link(_) => numbers.push(None),
                            link::Piece::Text(text) => {
                                let written = text.trim().trim_matches(',').trim();
                                if written.is_empty() {
                                    continue;
                                }
                                let n: f64 = written.replace(',', ".").parse().map_err(|_| {
                                    AppError(format!("{label}: after each document stands its {count} — “{written}” is no number."))
                                })?;
                                match numbers.last_mut() {
                                    Some(slot) if slot.is_none() => *slot = Some(number_value(n)),
                                    _ => return Err(AppError(format!("{label}: “{written}” stands before any document."))),
                                }
                            }
                        }
                    }
                }
                let mut ids = Vec::new();
                for (i, link) in document::names_in(&property.value).into_iter().enumerate() {
                    let id = opened
                        .known
                        .names
                        .iter()
                        .find(|n| n.is(&link))
                        .map(|n| n.id.clone())
                        // An id kept from a document this reader cannot open.
                        .or_else(|| link.name.starts_with("art:").then(|| link.name.clone()))
                        .ok_or_else(|| {
                            let name = if link.of.is_empty() {
                                link.name.clone()
                            } else {
                                format!("{}:{}", link.of, link.name)
                            };
                            AppError(format!("{label}: there is no document “{name}” here."))
                        })?;
                    let number = numbers.get(i).cloned().flatten();
                    // `every`: all documents of the type are there anyway —
                    // only a number other than 0 says anything.
                    if *every && number.as_ref().and_then(|n| n.as_f64()).unwrap_or(0.0) == 0.0 {
                        continue;
                    }
                    ids.push(pick_value(id, number));
                }
                if *multiple {
                    serde_json::Value::Array(ids)
                } else if ids.len() > 1 {
                    return Err(AppError(format!("{label} takes one document, not several.")));
                } else {
                    ids.into_iter().next().unwrap_or(serde_json::Value::Null)
                }
            }
            FieldKind::Number { .. } => {
                // A bare number is the document's own — a score as rolled.
                // With an operator it changes what picks it (`adds`): `+1`
                // raises, `-1` lowers, `=19` sets, `>=19` sets at least,
                // `<=3` at most. See `shared/Sheet.ts`.
                let written = property.value.trim();
                // A formula of its own, reckoned where it is read — for
                // whoever picks the document: `[[Dexterity mod]]`.
                if written.starts_with("[[") && written.ends_with("]]") && written.len() > 4 {
                    serde_json::Value::String(written.to_string())
                } else {
                    let (op, digits) = [(">=", ">="), ("≥", ">="), ("<=", "<="), ("≤", "<="), ("+", "+"), ("-", "-"), ("=", "=")]
                        .iter()
                        .find_map(|(sign, op)| written.strip_prefix(sign).map(|rest| (Some(*op), rest.trim())))
                        .unwrap_or((None, written));
                    let n: f64 = digits.parse().map_err(|_| {
                        AppError(format!(
                            "{label} is a number or a formula in [[…]], with +, -, =, >= or <= before it if it changes another — “{}” is none.",
                            property.value
                        ))
                    })?;
                    match (op, number_value(n)) {
                        (Some(op), serde_json::Value::Number(n)) => serde_json::Value::String(format!("{op}{}", number_text(&n))),
                        (_, value) => value,
                    }
                }
            }
            FieldKind::WorldDate => {
                let n = opened.calendar.parse_moment(&property.value).ok_or_else(|| {
                    let example = opened.calendar.format_moment(opened.calendar.eras.last().map_or(0, |e| e.start) as f64);
                    AppError(format!("{label} is a date — “{}” is not. Like this: {example}", property.value))
                })?;
                number_value(n)
            }
            FieldKind::DateTime => {
                let moment = moment_of_ours(&property.value).ok_or_else(|| {
                    AppError(format!("{label} is a date and time — “{}” is not. Like this: 2026-09-12 19:00", property.value))
                })?;
                serde_json::Value::String(moment)
            }
            _ => serde_json::Value::String(names::stored(&property.value, &*opened.known, &opened.article_type)),
        };
        fields.insert(field.key.clone(), value);
    }

    // Everything else is a free property — named by whoever wrote it.
    for property in properties {
        if used.contains(&property.name.to_lowercase()) {
            continue;
        }
        fields.insert(
            property.name.clone(),
            serde_json::Value::String(names::stored(&property.value, &*opened.known, &opened.article_type)),
        );
    }
    Ok(serde_json::Value::Object(fields))
}


#[cfg(feature = "gui")]
/// Opens a document as code.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn document_open(app: tauri::AppHandle, world: String, article: String) -> Answer<DocumentCode> {
    document_open_in(&store(&app)?, world, article)
}

/// `document_open` for whom the store is given.
pub(crate) fn document_open_in(store: &Store, world: String, article: String) -> Answer<DocumentCode> {
    code_of(store, &world, &article)
}

#[cfg(feature = "gui")]
/// Reads code while it is typed — errors come back with their line.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn document_read(app: tauri::AppHandle, world: String, article: String, text: String, base: DocumentText) -> Answer<DocumentRead> {
    document_read_in(&store(&app)?, world, article, text, base)
}

/// `document_read` for whom the store is given.
pub(crate) fn document_read_in(store: &Store, world: String, article: String, text: String, base: DocumentText) -> Answer<DocumentRead> {
    let opened = open(store, &world, &article, &store.reader(&world)?)?;
    // The ids of the blocks come from the state before this keystroke — so a
    // section keeps its layer while its title is being retyped.
    let mut read = document::from_code(&text, &base, &opened.shape);
    read.document = document::joined(&read.document);
    Ok(read)
}

/// Writes a document as code — after a change on the visual side.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn document_write(document: DocumentText) -> Answer<String> {
    Ok(aw_model::document::to_code(&document))
}

#[cfg(feature = "gui")]
/// Saves a document from its code — what changed against `base` — and
/// returns it as it stands now, with whatever others wrote meanwhile.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn document_save(app: tauri::AppHandle, world: String, article: String, text: String, base: Bases, session: String) -> Answer<DocumentCode> {
    document_save_in(&store(&app)?, world, article, text, base, session)
}

/// `document_save` for whom the store is given.
pub(crate) fn document_save_in(store: &Store, world: String, article: String, text: String, base: Bases, session: String) -> Answer<DocumentCode> {
    save(store, &world, &article, &text, &base, &session)?;
    code_of(store, &world, &article)
}


/// Opens a document as code — without Tauri, for the command and the tests.
pub(crate) fn code_of(store: &Store, world: &str, article: &str) -> Answer<DocumentCode> {
    let opened = open(store, world, article, &store.reader(world)?)?;
    Ok(code_from(opened))
}

fn code_from(opened: Opened) -> DocumentCode {
    let mut base = Bases::new();
    for i in std::iter::once(opened.base.0).chain(opened.readable.iter().map(|(_, i, _)| *i)) {
        let layer = &opened.doc.layers[i];
        base.insert(layer.id.clone(), hand_out(layer));
    }
    DocumentCode { code: document::to_code(&opened.text), document: document::joined(&opened.text), base }
}

/// A document being changed by one save: as it was, and as it becomes.
struct Changing {
    /// As it stood when the save began — what a version keeps as before.
    was: store::Document,
    now: store::Document,
}

impl Changing {
    fn new(doc: &store::Document) -> Self {
        Changing { was: doc.clone(), now: doc.clone() }
    }

    fn at(&self, id: &str) -> Option<usize> {
        self.now.layers.iter().position(|l| l.id == id)
    }

    /// Writes a layer's content, on top of `seen` — what the writer saw.
    /// What others wrote meanwhile stays; letter by letter where both wrote.
    fn write(&mut self, reader: &Reader, id: &str, seen: &Plain, wanted: &Plain) -> Answer<()> {
        let Some(i) = self.at(id) else { return Ok(()) };
        let layer = &mut self.now.layers[i];
        if seen == wanted {
            return Ok(());
        }
        if !reader.may_edit(&layer.access) {
            return Err(AppError("You may not change this part.".into()));
        }
        let mut merged = Plain::of(layer);
        if wanted.title != seen.title {
            merged.title = aw_store::db::merge(&seen.title, &wanted.title, &merged.title);
        }
        if wanted.text != seen.text {
            merged.text = aw_store::db::merge(&seen.text, &wanted.text, &merged.text);
        }
        let keys: std::collections::BTreeSet<&String> = wanted.fields.keys().chain(seen.fields.keys()).collect();
        for key in keys {
            let (was, is) = (seen.fields.get(key), wanted.fields.get(key));
            if was == is {
                continue;
            }
            match (was, is, merged.fields.get(key)) {
                (Some(serde_json::Value::String(b)), Some(serde_json::Value::String(o)), Some(serde_json::Value::String(t))) => {
                    let value = aw_store::db::merge(b, o, t);
                    merged.fields.insert(key.clone(), serde_json::Value::String(value));
                }
                (_, Some(value), _) => {
                    merged.fields.insert(key.clone(), value.clone());
                }
                (_, None, _) => {
                    merged.fields.remove(key);
                }
            }
        }
        // The images it names: as the text and fields name them now.
        let mut named = store::named_images(&merged.text);
        for value in merged.fields.values() {
            named.extend(store::named_images(&value.to_string()));
        }
        merged.images = named.into_iter().collect();
        merged.into_layer(layer);
        Ok(())
    }

    /// Who sees and edits a layer, from now on.
    fn set_access(&mut self, id: &str, view: &str, edit: &str) -> Answer<()> {
        view.parse::<Policy>().map_err(AppError::from)?;
        edit.parse::<Policy>().map_err(AppError::from)?;
        if let Some(i) = self.at(id) {
            self.now.layers[i].access = store::Access { view: view.to_string(), edit: edit.to_string() };
        }
        Ok(())
    }

    /// A new layer, after the others.
    fn add(&mut self, section: String, view: String, edit: String, plain: Plain) -> Answer<()> {
        view.parse::<Policy>().map_err(AppError::from)?;
        edit.parse::<Policy>().map_err(AppError::from)?;
        let mut layer = store::Layer {
            id: aw_model::LayerId::new().to_string(),
            base: false,
            section,
            title: String::new(),
            access: store::Access { view, edit },
            text: String::new(),
            fields: Default::default(),
            images: Vec::new(),
        };
        let mut plain = plain;
        let mut named = store::named_images(&plain.text);
        for value in plain.fields.values() {
            named.extend(store::named_images(&value.to_string()));
        }
        plain.images = named.into_iter().collect();
        plain.into_layer(&mut layer);
        self.now.layers.push(layer);
        Ok(())
    }

    fn remove(&mut self, id: &str) {
        self.now.layers.retain(|l| l.id != id || l.base);
    }

    /// Writes it, if anything changed — and keeps what it changed as part
    /// of the session's version.
    fn finish(self, store: &Store, world: &str, session: &str) -> Answer<()> {
        if self.now.layers == self.was.layers {
            return Ok(());
        }
        history::keep(store, world, session, self.was, self.now)
    }
}

pub(crate) fn save(store: &Store, world: &str, article: &str, text: &str, base: &Bases, session: &str) -> Answer<()> {
    let reader = store.reader(world)?;
    let opened = open(store, world, article, &reader)?;
    let read = document::from_code(text, &opened.text, &opened.shape);
    if let Some(e) = read.errors.first() {
        return Err(AppError(if e.line > 0 { format!("Line {}: {}", e.line, e.text) } else { e.text.clone() }));
    }
    let gm = reader.is_gm();
    let id = opened.id.clone();
    let mut doc = Changing::new(&opened.doc);

    // A layer's content as the editor saw it — what is written is measured
    // against this, never against now. Not kept any more: as it is now.
    let seen = |i: usize| -> Option<Plain> {
        let layer = &opened.doc.layers[i];
        base.contains_key(&layer.id).then(|| seen_state(base, &layer.id).unwrap_or_else(|| Plain::of(layer)))
    };
    // Who could see and edit a layer as the editor saw it.
    let seen_access = |i: usize| -> Answer<LayerContent> {
        let layer = &opened.doc.layers[i];
        if !base.contains_key(&layer.id) {
            return Err(AppError("This part was not open here.".into()));
        }
        layer_content(layer, &reader)
    };

    // --- The skeleton: name and access -----------------------------------
    let name = read.document.name.trim();
    let access = &read.document.access;
    let renamed = name != opened.text.name;
    // The world's page and a type's page carry their owner's name — renaming
    // the one renames the other. Whether the name is free is asked first,
    // before anything is written.
    let owner = if renamed { owner_of(store, world, article)? } else { None };
    if let Some(owner) = &owner {
        check_owner_name(store, world, owner, name)?;
    }
    let (base_index, base_content) = &opened.base;
    // The document's own sections, in the order written — the GM's to keep.
    // Only those that are there to keep: written in, or already a layer.
    let typed = |key: &str| opened.template.as_ref().is_some_and(|t| t.blocks.iter().any(|b| b.key == key));
    let order: Vec<String> = read
        .document
        .blocks
        .iter()
        .filter(|b| !typed(&b.key))
        .filter(|b| b.parts.iter().any(|p| !p.text.trim().is_empty() || opened.layer(&p.key).is_some()) || !b.title.trim().is_empty())
        .map(|b| b.key.clone())
        .collect();
    let reorder = gm && order != own_order(&base_content.fields) && order.len() > 1;
    if renamed && !reader.allows(&opened.shape.defaults.edit) {
        return Err(AppError("You may not rename this document.".into()));
    }
    if renamed || reorder {
        if let Some(was) = seen(*base_index) {
            let mut wanted = was.clone();
            if renamed {
                wanted.title = name.to_string();
            }
            if reorder {
                wanted.fields.insert(OWN_ORDER.into(), serde_json::json!(order));
            }
            // The base is the GM's to change; renaming is whoever may edit the document.
            let base_id = opened.layer_id(*base_index).to_string();
            let edit_rule = reader.allows(&opened.shape.defaults.edit);
            if edit_rule {
                let as_gm = Reader::gm();
                doc.write(&as_gm, &base_id, &was, &wanted)?;
            }
        }
    }
    let was = seen_access(*base_index).map(|c| (c.own, c.edit)).unwrap_or((opened.shape.defaults.view.clone(), opened.shape.defaults.edit.clone()));
    if access.view != was.0 || access.edit != was.1 {
        if !gm {
            return Err(AppError("Only the GM can change who sees or edits the document.".into()));
        }
        doc.set_access(opened.layer_id(*base_index), &access.view, &access.edit)?;
    }

    // --- The properties --------------------------------------------------
    // Each value goes where it lies already; a new one where its field says:
    // with access of its own into a layer of its own, otherwise into the
    // common one.
    let fields = fields_from(&opened, &read.document.properties)?;
    let template_fields: Vec<&aw_model::template::TemplateField> = opened.template.iter().flat_map(|t| t.properties.iter()).collect();
    let mut wanted_fields: BTreeMap<String, serde_json::Map<String, serde_json::Value>> = opened
        .readable
        .iter()
        .filter(|(section, i, _)| is_infobox(section) && base.contains_key(opened.layer_id(*i)))
        .map(|(section, _, _)| (section.clone(), serde_json::Map::new()))
        .collect();
    if read.document.infobox.is_some() {
        wanted_fields.entry(INFOBOX_SECTION.to_string()).or_default();
    }
    for (key, value) in fields.as_object().into_iter().flatten() {
        let holder = opened.readable.iter().find(|(section, _, c)| is_infobox(section) && c.fields.as_object().is_some_and(|m| m.contains_key(key)));
        let section = match holder {
            Some((section, _, _)) => section.clone(),
            None if template_fields.iter().any(|f| &f.key == key && f.access.is_some()) => field_section(key),
            None => INFOBOX_SECTION.to_string(),
        };
        wanted_fields.entry(section).or_default().insert(key.clone(), value.clone());
    }
    // Who sees and changes the infobox — its own, or the document's.
    let infobox = read.document.infobox.clone().unwrap_or_else(|| access.clone());
    if let Some((_, i, content)) = opened.layer(INFOBOX_SECTION) {
        let (view, edit) = seen_access(*i).map(|c| (c.own, c.edit)).unwrap_or((content.own.clone(), content.edit.clone()));
        if infobox.view != view || infobox.edit != edit {
            if !gm {
                return Err(AppError("Only the GM can change who sees or edits the infobox.".into()));
            }
            doc.set_access(opened.layer_id(*i), &infobox.view, &infobox.edit)?;
        }
    }
    for (section, values) in wanted_fields {
        match opened.layer(&section) {
            Some((_, i, _)) => {
                let Some(was) = seen(*i) else { continue };
                let mut wanted = was.clone();
                wanted.fields = values.into_iter().collect();
                doc.write(&reader, opened.layer_id(*i), &was, &wanted)?;
            }
            None => {
                // An infobox given access of its own keeps it, even before it holds anything.
                let kept = section == INFOBOX_SECTION && read.document.infobox.is_some() && gm;
                if values.is_empty() && !kept {
                    continue;
                }
                let own = template_fields.iter().find(|f| field_section(&f.key) == section).and_then(|f| f.access.clone());
                let (view, edit) = match own {
                    Some(a) => (a.view, a.edit),
                    // Without access of its own a property is the infobox's.
                    None => (infobox.view.clone(), infobox.edit.clone()),
                };
                let (view, edit) = (crate::own_view(&view, &id, &opened.article_type), crate::own_view(&edit, &id, &opened.article_type));
                // Who may edit the infobox may give the document its first values.
                if !gm && !reader.may_edit(&store::Access { view: view.clone(), edit: edit.clone() }) {
                    return Err(AppError("You may not give this document properties.".into()));
                }
                doc.add(section, view, edit, Plain { fields: values.into_iter().collect(), ..Plain::default() })?;
            }
        }
    }

    // What changed in relations goes into the documents they name too.
    let mirrors = mirrored(&opened, &fields);

    // --- The sections ----------------------------------------------------
    let mut kept = std::collections::BTreeSet::new();
    for block in &read.document.blocks {
        for part in &block.parts {
            kept.insert(part.key.clone());
            // `self`: whoever is assigned to this document.
            let mut part = part.clone();
            part.view = crate::own_view(&part.view, &id, &opened.article_type);
            part.edit = crate::own_view(&part.edit, &id, &opened.article_type);
            let part = &part;
            // Names as written, ids as stored.
            let text = names::stored(&part.text, &*opened.known, &opened.article_type);
            match opened.layer(&part.key) {
                Some((_, i, content)) => {
                    // The title of a block belongs to its type, not to the
                    // layer — it counts only where the block is the layer's
                    // own.
                    let own_title = opened.template.as_ref().is_none_or(|t| !t.blocks.iter().any(|b| b.key == block.key));
                    if let Some(was) = seen(*i) {
                        let mut wanted = was.clone();
                        wanted.text = text;
                        if own_title {
                            wanted.title = block.title.clone();
                            // How it looks, on its first part.
                            if part.key == block.key {
                                OwnShape::from_block(block, &*opened.known).put(&mut wanted.fields);
                            }
                        }
                        doc.write(&reader, opened.layer_id(*i), &was, &wanted)?;
                    }
                    let was = seen_access(*i).map(|c| (c.own, c.edit)).unwrap_or((content.own.clone(), content.edit.clone()));
                    let access_changed = part.view != was.0 || part.edit != was.1;
                    if access_changed && !gm {
                        // Who may edit a part says who sees it. Who edits it
                        // stays the GM's to say.
                        if part.edit != was.1 || !reader.allows(&was.1) {
                            return Err(AppError("Only the GM can change who edits a part.".into()));
                        }
                    }
                    if access_changed {
                        doc.set_access(opened.layer_id(*i), &part.view, &part.edit)?;
                    }
                }
                None => {
                    // An empty new part — an empty section someone left as it
                    // was — is no layer. A section of the document's own that
                    // was given a name is kept, to be written in.
                    let named_own = !typed(&block.key) && !block.title.trim().is_empty() && part.key == block.key;
                    // A type's section given access of its own is kept too —
                    // empty, it would read as the type has it.
                    let as_typed = opened.template.as_ref().and_then(|t| t.blocks.iter().find(|b| b.key == block.key)).and_then(|b| b.parts.iter().find(|p| p.key == part.key));
                    let own_access = as_typed.is_some_and(|t| {
                        let same = |a: &str, b: &str| a == b || a.parse::<Policy>().ok().is_some_and(|a| b.parse::<Policy>().ok() == Some(a));
                        !(same(&crate::own_view(&t.view, &id, &opened.article_type), &part.view)
                            && same(&crate::own_view(&t.edit, &id, &opened.article_type), &part.edit))
                    });
                    if part.text.trim().is_empty() && !named_own && !own_access {
                        continue;
                    }
                    // A part as its type sets it out is written by whoever may
                    // edit it; one with access of its own is the GM's.
                    let as_set_out = as_typed.is_some() && !own_access && reader.may_edit(&store::Access { view: part.view.clone(), edit: part.edit.clone() });
                    if !gm && !as_set_out {
                        return Err(AppError("Only the GM can add a part with access of its own.".into()));
                    }
                    let mut fields = BTreeMap::new();
                    if !typed(&block.key) && part.key == block.key {
                        OwnShape::from_block(block, &*opened.known).put(&mut fields);
                    }
                    doc.add(part.key.clone(), part.view.clone(), part.edit.clone(), Plain { title: block.title.clone(), text, fields, images: Vec::new() })?;
                }
            }
        }
    }

    // What the editor saw and did not keep is removed — what it did not see,
    // or what came meanwhile, stays untouched.
    let removed: Vec<String> = opened
        .readable
        .iter()
        .filter(|(s, i, _)| !is_infobox(s) && !kept.contains(s) && base.contains_key(opened.layer_id(*i)))
        .map(|(_, i, _)| opened.layer_id(*i).to_string())
        .collect();
    if !removed.is_empty() && !gm {
        return Err(AppError("Only the GM can remove a part.".into()));
    }
    for layer in &removed {
        doc.remove(layer);
    }
    doc.finish(store, world, session)?;

    for (other, field, mirrored) in mirrors {
        let _ = set_relations(store, world, &other, &field, &id, &mirrored, session);
    }
    if let Some(owner) = owner {
        name_owner(store, world, &owner, name)?;
    }
    Ok(())
}

// --- Relations, on both sides ----------------------------------------------

/// What changed in a document's relations, for the documents they name:
/// each side holds the whole entry, its own way round.
///
/// What cannot be written there — a document this reader may not change, a
/// type without such a property — stays one-sided. That is no error: the
/// page shows it.
fn mirrored(opened: &Opened, fields: &serde_json::Value) -> Vec<(String, aw_model::template::TemplateField, Vec<Entry>)> {
    let Some(template) = &opened.template else { return Vec::new() };
    let before = stored_fields(opened);
    let mut out = Vec::new();
    for field in template.properties.iter().filter(|f| matches!(f.kind, FieldKind::Relation { .. })) {
        let old = before.get(&field.key).map(relation::entries).unwrap_or_default();
        let new = fields.get(&field.key).map(relation::entries).unwrap_or_default();
        let mut others: Vec<&str> = old.iter().chain(&new).map(|e| e.with.as_str()).collect();
        others.sort_unstable();
        others.dedup();
        for other in others {
            let about = |list: &[Entry]| list.iter().filter(|e| e.with == other).cloned().collect::<Vec<_>>();
            let (was, is) = (about(&old), about(&new));
            if was == is || other == opened.id || !opened.known.names.iter().any(|n| n.id == other) {
                continue;
            }
            out.push((other.to_string(), field.clone(), is.iter().map(|e| e.mirrored(&opened.id)).collect()));
        }
    }
    out
}

/// Sets what a document's relation property says about one other document
/// — every entry naming `about` is replaced by `wanted`, in place.
///
/// The property is found by key, or else by name: a character and a deity
/// may both have “Relations”. Returns whether the document has one at all.
fn set_relations(store: &Store, world: &str, article: &str, field: &aw_model::template::TemplateField, about: &str, wanted: &[Entry], session: &str) -> Answer<bool> {
    let reader = store.reader(world)?;
    let opened = open(store, world, article, &reader)?;
    let Some(template) = &opened.template else { return Ok(false) };
    let is_relation = |f: &&aw_model::template::TemplateField| matches!(f.kind, FieldKind::Relation { .. });
    let Some(target) = template
        .properties
        .iter()
        .filter(is_relation)
        .find(|f| f.key == field.key)
        .or_else(|| template.properties.iter().filter(is_relation).find(|f| f.label.eq_ignore_ascii_case(&field.label)))
    else {
        return Ok(false);
    };

    let replaced = |current: Option<&serde_json::Value>| -> Vec<Entry> {
        let mut out = Vec::new();
        let mut placed = false;
        for e in current.map(relation::entries).unwrap_or_default() {
            if e.with != about {
                out.push(e);
            } else if !placed {
                out.extend(wanted.iter().cloned());
                placed = true;
            }
        }
        if !placed {
            out.extend(wanted.iter().cloned());
        }
        out
    };

    let section = opened
        .readable
        .iter()
        .find(|(s, _, c)| is_infobox(s) && c.fields.get(&target.key).is_some())
        .map(|(s, ..)| s.clone())
        .unwrap_or_else(|| if target.access.is_some() { field_section(&target.key) } else { INFOBOX_SECTION.to_string() });
    let mut doc = Changing::new(&opened.doc);
    match opened.layer(&section) {
        Some((_, i, _)) => {
            let layer = &opened.doc.layers[*i];
            let was = Plain::of(layer);
            let mut plain = was.clone();
            let list = replaced(plain.fields.get(&target.key));
            if list.is_empty() {
                if plain.fields.remove(&target.key).is_none() {
                    return Ok(true);
                }
            } else {
                let value = relation::value(&list);
                if plain.fields.get(&target.key) == Some(&value) {
                    return Ok(true);
                }
                plain.fields.insert(target.key.clone(), value);
            }
            doc.write(&reader, &layer.id, &was, &plain)?;
        }
        None if wanted.is_empty() => {}
        None => {
            let (view, edit) = match &target.access {
                Some(a) => (a.view.clone(), a.edit.clone()),
                None => (opened.shape.defaults.view.clone(), opened.shape.defaults.edit.clone()),
            };
            if !opened.gm && !reader.may_edit(&store::Access { view: view.clone(), edit: edit.clone() }) {
                return Err(AppError("You may not give this document properties.".into()));
            }
            let mut fields = BTreeMap::new();
            fields.insert(target.key.clone(), relation::value(&replaced(None)));
            doc.add(section, view, edit, Plain { fields, ..Plain::default() })?;
        }
    }
    doc.finish(store, world, session)?;
    Ok(true)
}

#[cfg(feature = "gui")]
/// Puts an image into an image field of a document — a portrait, a coat of
/// arms — or takes it out (`None`). Whoever may read the part holding the
/// field sees the image.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_field_set(app: tauri::AppHandle, world: String, article: String, field: String, image: Option<String>) -> Answer<()> {
    image_field_set_in(&store(&app)?, world, article, field, image)
}

/// `image_field_set` for whom the store is given.
pub(crate) fn image_field_set_in(store: &Store, world: String, article: String, field: String, image: Option<String>) -> Answer<()> {
    let session = format!("image-{}", aw_model::template_text::new_id());
    set_image_field(store, &world, &article, &field, image.as_deref(), &session)
}

pub(crate) fn set_image_field(store: &Store, world: &str, article: &str, field: &str, image: Option<&str>, session: &str) -> Answer<()> {
    let value = image.map(|i| serde_json::Value::String(i.to_string()));
    set_field(store, world, article, field, value, session, |k| matches!(k, FieldKind::Image { .. }))
        .map_err(|e| if e.0 == NO_SUCH_FIELD { AppError("This document has no such image.".into()) } else { e })
}

#[cfg(feature = "gui")]
/// Sets the documents a property picks — from a list on the page, a click
/// each — or empties it. Only a property that picks several.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn references_set(app: tauri::AppHandle, world: String, article: String, field: String, ids: Vec<String>) -> Answer<()> {
    references_set_in(&store(&app)?, world, article, field, ids)
}

/// `references_set` for whom the store is given.
pub(crate) fn references_set_in(store: &Store, world: String, article: String, field: String, ids: Vec<String>) -> Answer<()> {
    for id in &ids {
        id.parse::<aw_model::ArticleId>().map_err(AppError::from)?;
    }
    let session = format!("references-{}", aw_model::template_text::new_id());
    // A pick that stays keeps its number — the levels of a class stay when
    // another class is added.
    let opened = open(store, &world, &article, &store.reader(&world)?)?;
    let before = stored_fields(&opened).get(&field).map(picks).unwrap_or_default();
    let value = (!ids.is_empty()).then(|| {
        serde_json::Value::Array(
            ids.into_iter()
                .map(|id| {
                    let n = before.iter().find(|(old, _)| *old == id).and_then(|(_, n)| n.clone());
                    pick_value(id, n)
                })
                .collect(),
        )
    });
    set_field(store, &world, &article, &field, value, &session, |k| matches!(k, FieldKind::Reference { multiple: true, .. }))
}

const NO_SUCH_FIELD: &str = "This document has no such property.";

/// Writes one property's value into the part holding it — a new part for
/// it if the document has none yet — or takes it out (`None`).
fn set_field(store: &Store, world: &str, article: &str, field: &str, value: Option<serde_json::Value>, session: &str, kind: impl Fn(&FieldKind) -> bool) -> Answer<()> {
    let reader = store.reader(world)?;
    let opened = open(store, world, article, &reader)?;
    let target = opened
        .template
        .iter()
        .flat_map(|t| t.properties.iter())
        .find(|f| f.key == field && kind(&f.kind))
        .cloned()
        .ok_or_else(|| AppError(NO_SUCH_FIELD.into()))?;
    let section = opened
        .readable
        .iter()
        .find(|(s, _, c)| is_infobox(s) && c.fields.get(&target.key).is_some())
        .map(|(s, ..)| s.clone())
        .unwrap_or_else(|| if target.access.is_some() { field_section(&target.key) } else { INFOBOX_SECTION.to_string() });
    let mut doc = Changing::new(&opened.doc);
    match opened.layer(&section) {
        Some((_, i, _)) => {
            let layer = &opened.doc.layers[*i];
            let was = Plain::of(layer);
            let mut plain = was.clone();
            match &value {
                Some(v) => plain.fields.insert(target.key.clone(), v.clone()),
                None => plain.fields.remove(&target.key),
            };
            doc.write(&reader, &layer.id, &was, &plain)?;
        }
        None if value.is_none() => {}
        None => {
            let (view, edit) = match &target.access {
                Some(a) => (a.view.clone(), a.edit.clone()),
                None => (opened.shape.defaults.view.clone(), opened.shape.defaults.edit.clone()),
            };
            if !opened.gm && !reader.may_edit(&store::Access { view: view.clone(), edit: edit.clone() }) {
                return Err(AppError("You may not give this document properties.".into()));
            }
            let mut fields = BTreeMap::new();
            fields.insert(target.key.clone(), value.clone().expect("a value"));
            doc.add(section, view, edit, Plain { fields, ..Plain::default() })?;
        }
    }
    doc.finish(store, world, session)
}

/// One relation to set from the graph: what `article` says about `with`,
/// and — as far as this reader may — the other way round. `None` removes it.
#[derive(Deserialize)]
pub struct RelationChange {
    pub article: String,
    pub property: String,
    pub with: String,
    pub roles: Option<(String, String)>,
}

#[cfg(feature = "gui")]
/// Sets relations from the graph — all of them as one change, so parents
/// joined to their sons are one step in each document's history.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn relations_set(app: tauri::AppHandle, world: String, changes: Vec<RelationChange>) -> Answer<()> {
    relations_set_in(&store(&app)?, world, changes)
}

pub(crate) fn relations_set_in(store: &Store, world: String, changes: Vec<RelationChange>) -> Answer<()> {
    let session = format!("relations-{}", aw_model::template_text::new_id());
    for c in changes {
        set_relation(store, &world, &c.article, &c.property, &c.with, c.roles, &session)?;
    }
    Ok(())
}

pub(crate) fn set_relation(store: &Store, world: &str, article: &str, property: &str, with: &str, roles: Option<(String, String)>, session: &str) -> Answer<()> {
    if article == with {
        return Err(AppError("A document has no relation to itself.".into()));
    }
    let templates = store.load_templates(world)?;
    let field = templates
        .iter()
        .flat_map(|t| t.properties.iter())
        .find(|f| f.key == property && matches!(f.kind, FieldKind::Relation { .. }))
        .cloned()
        .ok_or_else(|| AppError("There is no such relation property.".into()))?;
    let wanted: Vec<Entry> = roles.into_iter().map(|(role, own_role)| Entry { with: with.to_string(), role, own_role }).collect();
    if !set_relations(store, world, article, &field, with, &wanted, session)? {
        return Err(AppError(format!("This document has no “{}”.", field.label)));
    }
    let mirrored: Vec<Entry> = wanted.iter().map(|e| e.mirrored(article)).collect();
    let _ = set_relations(store, world, with, &field, article, &mirrored, session);
    Ok(())
}

/// Before a document goes: the documents it names forget it — as far as this
/// reader may change them. Otherwise they would show a relation to nobody.
pub(crate) fn forget_relations(store: &Store, world: &str, article: &str) -> Answer<()> {
    let opened = open(store, world, article, &store.reader(world)?)?;
    let Some(template) = &opened.template else { return Ok(()) };
    let fields = stored_fields(&opened);
    let session = format!("relations-{}", aw_model::template_text::new_id());
    for field in template.properties.iter().filter(|f| matches!(f.kind, FieldKind::Relation { .. })) {
        for entry in fields.get(&field.key).map(relation::entries).unwrap_or_default() {
            let _ = set_relations(store, world, &entry.with, field, article, &[], &session);
        }
    }
    Ok(())
}

/// Whose page a document is.
pub(crate) enum Owner {
    World,
    /// The type, by id.
    Type(String),
}

pub(crate) fn owner_of(store: &Store, world: &str, article: &str) -> Answer<Option<Owner>> {
    if store.load_world_meta(world)?.home == article {
        return Ok(Some(Owner::World));
    }
    Ok(store.load_templates(world)?.into_iter().find(|t| t.page == article).map(|t| Owner::Type(t.id)))
}

/// No two types, and no two worlds, share a name — in every list they could
/// not be told apart.
fn check_owner_name(store: &Store, world: &str, owner: &Owner, name: &str) -> Answer<()> {
    let taken = match owner {
        Owner::World => store.list_worlds()?.iter().any(|m| m.id != world && m.name.trim().eq_ignore_ascii_case(name)),
        Owner::Type(id) => store.load_templates(world)?.iter().any(|t| &t.id != id && t.name.trim().eq_ignore_ascii_case(name)),
    };
    if taken {
        let what = if matches!(owner, Owner::World) { "world" } else { "type" };
        return Err(AppError(format!("A {what} called “{name}” already exists.")));
    }
    Ok(())
}

/// Gives the owner the name its page now carries.
fn name_owner(store: &Store, world: &str, owner: &Owner, name: &str) -> Answer<()> {
    match owner {
        Owner::World => {
            let mut meta = store.load_world_meta(world)?;
            meta.name = name.to_string();
            store.save_world_meta(&meta)?;
        }
        Owner::Type(id) => {
            let mut templates = store.load_templates(world)?;
            let Some(t) = templates.iter_mut().find(|t| &t.id == id) else { return Ok(()) };
            if t.name == name {
                return Ok(());
            }
            // Nothing else follows by hand: text stores the type's id, and
            // its name is looked up whenever it is shown.
            t.name = name.to_string();
            store.save_templates(world, &templates)?;
        }
    }
    Ok(())
}

/// Gives a document a new name — the way its code would.
pub(crate) fn rename_document(store: &Store, world: &str, article: &str, name: &str) -> Answer<()> {
    let code = code_of(store, world, article)?;
    let mut doc = code.document;
    doc.name = name.to_string();
    save(store, world, article, &document::to_code(&doc), &code.base, "")
}

// --- Where others write ---------------------------------------------------

/// Marks a caret while a text passes through the translation.
const MARK_START: char = '\u{E000}';
const MARK_END: char = '\u{E001}';

/// The byte index of a UTF-16 position — the browser counts in UTF-16.
fn byte_at(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Where a caret stands in the editor: in the code, or in a part's text on
/// the visual side. Positions in UTF-16, as the browser counts.
#[derive(Deserialize)]
pub struct Caret {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub part: Option<String>,
    pub start: usize,
    pub end: usize,
}

/// Where someone's caret stands, for this reader: in a part's text as
/// written, and in the code.
#[derive(Serialize)]
pub struct CaretAt {
    pub part: String,
    pub part_start: usize,
    pub part_end: usize,
    pub code_start: usize,
    pub code_end: usize,
}

/// The layer a caret stands in, and where in its stored text.
pub(crate) fn caret_in(store: &Store, world: &str, article: &str, caret: Caret) -> Answer<Option<(String, usize, usize)>> {
    let opened = open(store, world, article, &store.reader(world)?)?;
    let (a, b) = (caret.start.min(caret.end), caret.start.max(caret.end));
    let found = match (caret.part, caret.code) {
        (Some(part), _) => opened.text.blocks.iter().flat_map(|b| b.parts.iter()).find(|p| p.key == part).map(|p| (part, a, b, p.text.clone())),
        (None, Some(code)) => {
            // Marked, the caret comes out in whichever part it stood.
            let mut marked = code.clone();
            marked.insert(byte_at(&code, b), MARK_END);
            marked.insert(byte_at(&code, a), MARK_START);
            let read = document::from_code(&marked, &opened.text, &opened.shape);
            read.document.blocks.iter().flat_map(|b| b.parts.iter()).find_map(|p| {
                let i = p.text.find(MARK_START)?;
                let start = utf16_len(&p.text[..i]);
                let end = p.text.find(MARK_END).map(|j| utf16_len(&p.text[..j]) - 1).unwrap_or(start);
                Some((p.key.clone(), start, end, p.text.replace([MARK_START, MARK_END], "")))
            })
        }
        (None, None) => None,
    };
    let Some((part, start, end, text)) = found else { return Ok(None) };
    let Some((_, i, _)) = opened.layer(&part) else { return Ok(None) };
    // Written with names, stored with ids — the place counts in the stored text.
    let stored = |at: usize| utf16_len(&names::stored(&text[..byte_at(&text, at)], &*opened.known, &opened.article_type));
    Ok(Some((opened.layer_id(*i).to_string(), stored(start), stored(end))))
}

/// Where a place in a layer's stored text stands for this reader.
pub(crate) fn caret_at(store: &Store, world: &str, article: &str, layer: &str, start: usize, end: usize) -> Answer<Option<CaretAt>> {
    let opened = open(store, world, article, &store.reader(world)?)?;
    let Some((section, _, content)) = opened.readable.iter().find(|(s, i, _)| opened.layer_id(*i) == layer && !is_infobox(s)) else {
        return Ok(None);
    };
    let written = |at: usize| utf16_len(&names::written(&content.text[..byte_at(&content.text, at)], &*opened.known, &opened.article_type));
    let (part_start, part_end) = (written(start.min(end)), written(start.max(end)));
    let mut doc = opened.text.clone();
    let Some(part) = doc.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()).find(|p| &p.key == section) else {
        return Ok(None);
    };
    let text = part.text.clone();
    let mut marked = text.clone();
    marked.insert(byte_at(&text, part_end), MARK_END);
    marked.insert(byte_at(&text, part_start), MARK_START);
    part.text = marked;
    let code = document::to_code(&doc);
    let (Some(i), Some(j)) = (code.find(MARK_START), code.find(MARK_END)) else { return Ok(None) };
    Ok(Some(CaretAt { part: section.clone(), part_start, part_end, code_start: utf16_len(&code[..i]), code_end: utf16_len(&code[..j]) - 1 }))
}

// --- Versions -------------------------------------------------------------

/// The history of a document: each editing session is a version — the
/// layers it changed, as they were before it and as it left them.
pub(crate) mod history {
    use super::*;
    use store::{Version, VersionContent};

    /// Keeps what one save changed in its session's version: the layers it
    /// touched as they were the first time the session touched them, and as
    /// they are now. Where what players write waits for the game master, it
    /// waits — the document stays as it was for everyone else.
    pub(super) fn keep(store: &Store, world: &str, session: &str, was: store::Document, now: store::Document) -> Answer<()> {
        let db = store.db(world)?;
        let author = store.actor()?;
        let session = if session.is_empty() { format!("change-{}", aw_model::template_text::new_id()) } else { session.to_string() };
        let at = store::now();
        let changed = |id: &str| was.layers.iter().find(|l| l.id == id) != now.layers.iter().find(|l| l.id == id);
        let ids: Vec<String> = was.layers.iter().chain(&now.layers).map(|l| l.id.clone()).filter(|id| changed(id)).collect();
        let review = if waits(store, world, &author)? { "waiting" } else { "counted" };
        let mut version = db.versions(&now.id)?.into_iter().find(|v| v.session == session).unwrap_or_else(|| Version {
            document: now.id.clone(),
            session: session.clone(),
            author: author.clone(),
            started: at,
            ended: at,
            arrived: at,
            review: review.into(),
            content: VersionContent::default(),
            signature: String::new(),
        });
        for id in ids {
            if !version.content.before.iter().any(|l| l.id == id) && !version.content.added.contains(&id) {
                match was.layers.iter().find(|l| l.id == id) {
                    Some(layer) => version.content.before.push(layer.clone()),
                    None => version.content.added.push(id.clone()),
                }
            }
            version.content.layers.retain(|l| l.id != id);
            if let Some(layer) = now.layers.iter().find(|l| l.id == id) {
                version.content.layers.push(layer.clone());
            }
        }
        version.ended = at;
        version.arrived = at;
        let waiting = version.review == "waiting";
        db.put_version(&version)?;
        if !waiting {
            db.put_document(&now)?;
            db.trim_versions(&now.id, store.version_limit(world)? as usize)?;
        }
        Ok(())
    }

    /// Whether what `author` writes waits for the game master: in a world
    /// with review, for a player not trusted.
    fn waits(store: &Store, world: &str, author: &str) -> Answer<bool> {
        if !store.moderated(world)? {
            return Ok(false);
        }
        Ok(store.db(world)?.member(author)?.is_some_and(|m| m.role == store::Role::Player && !m.trusted))
    }

    /// The document as its author sees it: with what they wrote that waits
    /// for the game master, on top.
    pub(crate) fn with_own_waiting(store: &Store, world: &str, mut doc: store::Document) -> Answer<store::Document> {
        let me = store.actor()?;
        for v in store.db(world)?.versions(&doc.id)?.into_iter().filter(|v| v.review == "waiting" && v.author == me) {
            apply(&mut doc, &v);
        }
        Ok(doc)
    }

    /// A version's change, put onto a document as it stands: what it wrote,
    /// merged with what others wrote since; what it added, added; what it
    /// removed, removed.
    fn apply(doc: &mut store::Document, v: &Version) {
        for after in &v.content.layers {
            let before = v.content.before.iter().find(|l| l.id == after.id);
            match (doc.layers.iter_mut().find(|l| l.id == after.id), before) {
                (Some(now), Some(before)) => {
                    let mut plain = Plain::of(now);
                    plain.title = aw_store::db::merge(&before.title, &after.title, &now.title);
                    plain.text = aw_store::db::merge(&before.text, &after.text, &now.text);
                    for (k, value) in &after.fields {
                        if before.fields.get(k) != Some(value) {
                            plain.fields.insert(k.clone(), value.clone());
                        }
                    }
                    for k in before.fields.keys().filter(|k| !after.fields.contains_key(*k)) {
                        plain.fields.remove(k);
                    }
                    plain.images = after.images.clone();
                    plain.into_layer(now);
                    if before.access != after.access {
                        now.access = after.access.clone();
                    }
                }
                (Some(now), None) => *now = after.clone(),
                (None, _) => doc.layers.push(after.clone()),
            }
        }
        for gone in v.content.before.iter().filter(|b| !v.content.layers.iter().any(|a| a.id == b.id)) {
            doc.layers.retain(|l| l.id != gone.id || l.base);
        }
    }

    /// The document as it stood right after `v`: the versions after it
    /// taken back, newest first.
    pub(super) fn document_then(store: &Store, world: &str, article: &str, session: &str) -> Answer<(store::Document, Version)> {
        let db = store.db(world)?;
        let mut doc = db.document(article)?.ok_or_else(|| AppError("There is no such article.".into()))?;
        let mut versions = db.versions(article)?;
        let at = versions.iter().position(|v| v.session == session).ok_or_else(|| AppError("There is no such version.".into()))?;
        let version = versions[at].clone();
        if version.review != "counted" {
            apply(&mut doc, &version);
            return Ok((doc, version));
        }
        versions.retain(|v| v.review == "counted" && (v.ended, &v.session) > (version.ended, &version.session));
        versions.sort_by(|a, b| b.ended.cmp(&a.ended));
        for later in versions {
            for after in &later.content.layers {
                match later.content.before.iter().find(|b| b.id == after.id) {
                    Some(before) => {
                        if let Some(l) = doc.layers.iter_mut().find(|l| l.id == after.id) {
                            *l = before.clone();
                        }
                    }
                    None => doc.layers.retain(|l| l.id != after.id || l.base),
                }
            }
            for gone in later.content.before.iter().filter(|b| !later.content.layers.iter().any(|a| a.id == b.id)) {
                if !doc.layers.iter().any(|l| l.id == gone.id) {
                    doc.layers.push(gone.clone());
                }
            }
        }
        Ok((doc, version))
    }

    /// One version of a document: an editing session.
    #[derive(Serialize)]
    pub struct VersionInfo {
        pub session: String,
        pub author: String,
        pub author_id: String,
        /// Milliseconds since 1970.
        pub from: u64,
        pub to: u64,
        /// When it came — the same as `to`, where it was written on the holder.
        pub arrived: u64,
        /// The parts it changed, by what the reader calls them.
        pub parts: Vec<String>,
        /// Changed who sees or edits a part, added or removed one.
        pub structure: bool,
        pub stale: bool,
        /// Versions by others, written at the same time on the same parts.
        pub overlaps: Vec<String>,
        /// `counted`, `waiting` or `rejected`.
        pub review: String,
        /// Whether the GM may delete it — not the newest, not waiting.
        pub removable: bool,
    }

    /// A part before and after a version.
    #[derive(Serialize)]
    pub struct PartChange {
        pub layer: String,
        pub part: String,
        pub before: String,
        pub after: String,
        /// Whether the reader may take this version of the part back.
        pub may_revert: bool,
    }

    /// What the reader calls a layer: its block's title, or the document's
    /// name for the base.
    fn part_names(opened: &Opened) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert(opened.layer_id(opened.base.0).to_string(), "Name".to_string());
        for (section, i, content) in &opened.readable {
            let title = if is_infobox(section) {
                "Properties".to_string()
            } else {
                opened
                    .text
                    .blocks
                    .iter()
                    .find(|b| b.parts.iter().any(|p| &p.key == section))
                    .map(|b| b.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| if content.title.is_empty() { "Text".into() } else { content.title.clone() })
            };
            out.insert(opened.layer_id(*i).to_string(), title);
        }
        out
    }

    /// Who wrote it, by name — as a member, or by id where they left.
    fn author_name(store: &Store, world: &str, id: &str) -> String {
        store.db(world).ok().and_then(|db| db.member(id).ok().flatten()).map(|m| m.name).unwrap_or_else(|| id.to_string())
    }

    /// The versions a reader may see of a document, newest first: those
    /// that waited and those turned down only for the GM and their author.
    fn visible(store: &Store, world: &str, doc: &store::Document, reader: &Reader) -> Answer<Vec<Version>> {
        let me = store.actor()?;
        let mut out: Vec<Version> = store
            .db(world)?
            .versions(&doc.id)?
            .into_iter()
            .filter(|v| v.review == "counted" || reader.is_gm() || v.author == me)
            .filter_map(|v| v.seen_by(doc, reader).or_else(|| (v.content.layers.is_empty() && v.content.before.is_empty()).then_some(v)))
            .collect();
        out.sort_by(|a, b| b.ended.cmp(&a.ended));
        Ok(out)
    }

    pub(crate) fn article_versions_in(store: &Store, world: String, article: String) -> Answer<Vec<VersionInfo>> {
        let reader = store.reader(&world)?;
        let opened = open(store, &world, &article, &reader)?;
        let names = part_names(&opened);
        let versions = visible(store, &world, &opened.doc, &reader)?;
        let newest = versions.iter().find(|v| v.review == "counted").map(|v| v.session.clone());
        let touched = |v: &Version| -> std::collections::BTreeSet<String> { v.content.layers.iter().chain(&v.content.before).map(|l| l.id.clone()).collect() };
        Ok(versions
            .iter()
            .map(|v| {
                let ids = touched(v);
                let parts: Vec<String> = ids.iter().filter_map(|l| names.get(l).cloned()).collect();
                let structure = !v.content.added.is_empty()
                    || v.content.before.iter().any(|b| v.content.layers.iter().find(|a| a.id == b.id).is_none_or(|a| a.access != b.access));
                let overlaps = versions
                    .iter()
                    .filter(|o| o.session != v.session && o.author != v.author && o.started <= v.ended && v.started <= o.ended && !touched(o).is_disjoint(&ids))
                    .map(|o| o.session.clone())
                    .collect();
                VersionInfo {
                    session: v.session.clone(),
                    author: author_name(store, &world, &v.author),
                    author_id: v.author.clone(),
                    from: v.started,
                    to: v.ended,
                    arrived: v.arrived,
                    parts,
                    structure,
                    stale: false,
                    overlaps,
                    review: v.review.clone(),
                    removable: reader.is_gm() && v.review == "counted" && newest.as_deref() != Some(v.session.as_str()),
                }
            })
            .collect())
    }

    pub(crate) fn version_compare_in(store: &Store, world: String, article: String, session: String) -> Answer<Vec<PartChange>> {
        let reader = store.reader(&world)?;
        let opened = open(store, &world, &article, &reader)?;
        let names = part_names(&opened);
        let version = visible(store, &world, &opened.doc, &reader)?.into_iter().find(|v| v.session == session).ok_or_else(|| AppError("There is no such version.".into()))?;
        let written = |layer: Option<&store::Layer>| {
            let Some(layer) = layer else { return String::new() };
            let mut out = names::written(&layer.text, &*opened.known, &opened.article_type);
            if !layer.title.is_empty() {
                out = if out.is_empty() { layer.title.clone() } else { format!("{}\n\n{out}", layer.title) };
            }
            for (key, value) in &layer.fields {
                out.push_str(&format!("\n{key}: {}", plain_value(value)));
            }
            out
        };
        let ids: std::collections::BTreeSet<String> = version.content.layers.iter().chain(&version.content.before).map(|l| l.id.clone()).collect();
        let mut out = Vec::new();
        for id in ids {
            let Some(part) = names.get(&id) else { continue };
            let before = version.content.before.iter().find(|l| l.id == id);
            let after = version.content.layers.iter().find(|l| l.id == id);
            let may_revert = opened.doc.layers.iter().find(|l| l.id == id).map(|l| reader.may_edit(&l.access)).unwrap_or_else(|| reader.allows(&opened.shape.defaults.edit));
            out.push(PartChange { layer: id, part: part.clone(), before: written(before), after: written(after), may_revert });
        }
        Ok(out)
    }

    /// A document as it stood with a version — as the page shows it, and as
    /// the editor does.
    #[derive(Serialize)]
    pub struct VersionDocument {
        pub view: ArticleView,
        pub code: String,
        pub document: DocumentText,
    }

    pub(crate) fn version_document_in(store: &Store, world: String, article: String, session: String) -> Answer<VersionDocument> {
        let reader = store.reader(&world)?;
        let (then, _) = document_then(store, &world, &article, &session)?;
        let view = article_view(&then, &reader).ok_or_else(|| AppError("This document did not stand like this for you then.".into()))?;
        let opened = open_doc(store, &world, then, &reader, store.load_templates(&world)?, store.load_world_meta(&world)?.calendar)?;
        Ok(VersionDocument { view, code: document::to_code(&opened.text), document: opened.text })
    }

    /// What is marked on one side of a version and now: in the code, in each
    /// part's text as written, and in each layer's text as the page reads it.
    #[derive(Serialize, Default)]
    pub struct SideMarks {
        pub code: Vec<Range>,
        pub parts: BTreeMap<String, Vec<Range>>,
        pub layers: BTreeMap<String, Vec<Range>>,
    }

    /// Where a version and now differ.
    #[derive(Serialize)]
    pub struct VersionMarks {
        /// The version: what now no longer has.
        pub then: SideMarks,
        /// Now: what the version did not have.
        pub now: SideMarks,
        /// The code the marks of `now` were made for. Once the editor's text
        /// differs from it, they no longer fit.
        pub now_code: String,
        /// A version that is not in the text stands as the text would be with it.
        pub review: String,
    }

    /// Every stretch of `text`.
    fn whole(text: &str) -> Vec<Range> {
        let end = utf16_len(text);
        if text.trim().is_empty() { Vec::new() } else { vec![Range { start: 0, end }] }
    }

    /// `text` with every range between the two marks.
    fn with_marks(text: &str, ranges: &[Range]) -> String {
        let mut out = String::new();
        let mut at = 0;
        let (mut starts, mut ends): (Vec<usize>, Vec<usize>) = ranges.iter().map(|r| (r.start, r.end)).unzip();
        starts.sort_unstable();
        ends.sort_unstable();
        let (mut s, mut e) = (starts.into_iter().peekable(), ends.into_iter().peekable());
        for c in text.chars() {
            while e.peek() == Some(&at) {
                out.push(MARK_END);
                e.next();
            }
            while s.peek() == Some(&at) {
                out.push(MARK_START);
                s.next();
            }
            out.push(c);
            at += c.len_utf16();
        }
        out.extend(e.map(|_| MARK_END));
        out
    }

    /// The text without marks, and where they stood.
    fn without_marks(text: &str) -> (String, Vec<Range>) {
        let (mut out, mut ranges, mut open, mut at) = (String::new(), Vec::new(), None, 0);
        for c in text.chars() {
            match c {
                MARK_START => open = Some(at),
                MARK_END => {
                    if let Some(start) = open.take() {
                        if start < at {
                            ranges.push(Range { start, end: at });
                        }
                    }
                }
                _ => {
                    out.push(c);
                    at += c.len_utf16();
                }
            }
        }
        (out, ranges)
    }

    /// The marks of one side, carried from the layers' stored texts into the
    /// parts as written and into the code. The name and properties that differ
    /// from `other` are marked whole.
    fn side_marks(opened: &Opened, by_layer: &BTreeMap<String, Vec<Range>>, other: &DocumentText) -> SideMarks {
        let mut out = SideMarks::default();
        for (section, i, content) in opened.readable.iter().filter(|(s, _, _)| !is_infobox(s)) {
            let id = opened.layer_id(*i);
            let Some(ranges) = by_layer.get(id).filter(|r| !r.is_empty()) else { continue };
            out.layers.insert(id.to_string(), ranges.clone());
            // Stored with ids, written with names: the places move with it.
            let text = &content.text;
            let written = |at: usize| utf16_len(&names::written(&text[..byte_at(text, at)], &*opened.known, &opened.article_type));
            let total = utf16_len(&names::written(text, &*opened.known, &opened.article_type));
            let parts: Vec<Range> = ranges
                .iter()
                .map(|r| {
                    let start = written(r.start).min(total);
                    Range { start, end: written(r.end).clamp(start, total) }
                })
                .filter(|r| r.start < r.end)
                .collect();
            out.parts.insert(section.clone(), parts);
        }
        let code = document::to_code(&opened.text);
        let marked = |whole_too: bool| {
            let mut doc = opened.text.clone();
            for part in doc.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()) {
                if let Some(ranges) = out.parts.get(&part.key) {
                    part.text = with_marks(&part.text, ranges);
                }
            }
            if whole_too {
                if doc.name != other.name {
                    doc.name = with_marks(&doc.name, &whole(&doc.name));
                }
                for p in doc.properties.iter_mut() {
                    if other.properties.iter().find(|q| q.name == p.name).is_none_or(|q| q.value != p.value) {
                        p.value = with_marks(&p.value, &whole(&p.value));
                    }
                }
            }
            without_marks(&document::to_code(&doc))
        };
        // A mark the code writes differently — quoted, say — would shift every
        // place after it: then without the whole ones, or without any.
        for whole_too in [true, false] {
            let (text, ranges) = marked(whole_too);
            if text == code {
                out.code = ranges;
                break;
            }
        }
        out
    }

    pub(crate) fn version_marks_in(store: &Store, world: String, article: String, session: String) -> Answer<VersionMarks> {
        let reader = store.reader(&world)?;
        let now = open(store, &world, &article, &reader)?;
        let (then_doc, version) = document_then(store, &world, &article, &session)?;
        let then = open_doc(store, &world, then_doc, &reader, store.load_templates(&world)?, store.load_world_meta(&world)?.calendar)?;
        let texts = |o: &Opened| -> BTreeMap<String, String> {
            o.readable.iter().filter(|(s, _, _)| !is_infobox(s)).map(|(_, i, c)| (o.layer_id(*i).to_string(), c.text.clone())).collect()
        };
        let (then_texts, now_texts) = (texts(&then), texts(&now));
        let (mut gone, mut added) = (BTreeMap::new(), BTreeMap::new());
        let layers: std::collections::BTreeSet<String> = then_texts.keys().chain(now_texts.keys()).cloned().collect();
        for layer in layers {
            let (g, a) = match (then_texts.get(&layer), now_texts.get(&layer)) {
                (Some(a), Some(b)) => diff::ranges(a, b),
                (Some(a), None) => (whole(a), Vec::new()),
                (None, Some(b)) => (Vec::new(), whole(b)),
                (None, None) => continue,
            };
            gone.insert(layer.clone(), g);
            added.insert(layer, a);
        }
        Ok(VersionMarks {
            then: side_marks(&then, &gone, &now.text),
            now: side_marks(&now, &added, &then.text),
            now_code: document::to_code(&now.text),
            review: version.review,
        })
    }

    fn plain_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => number_text(n),
            other => other.to_string(),
        }
    }

    /// Takes a version back — in every part the reader may edit. What others
    /// wrote since stays. A new version of its own, `into`.
    pub(crate) fn version_revert_in(store: &Store, world: &str, article: &str, session: &str, into: &str) -> Answer<usize> {
        let reader = store.reader(world)?;
        let db = store.db(world)?;
        let doc = db.document(article)?.ok_or_else(|| AppError("There is no such article.".into()))?;
        let version = visible(store, world, &doc, &reader)?.into_iter().find(|v| v.session == session).ok_or_else(|| AppError("There is no such version.".into()))?;
        let mut changing = Changing::new(&doc);
        let mut done = 0;
        for after in &version.content.layers {
            let Some(before) = version.content.before.iter().find(|b| b.id == after.id) else { continue };
            let Some(now) = doc.layers.iter().find(|l| l.id == after.id) else { continue };
            if !reader.may_edit(&now.access) {
                continue;
            }
            changing.write(&reader, &after.id, &Plain::of(after), &Plain::of(before))?;
            done += 1;
        }
        if done == 0 {
            return Err(AppError("You may not change any part of this version.".into()));
        }
        changing.finish(store, world, into)?;
        Ok(done)
    }

    /// The GM's word on a waiting version: taken into the text, or turned down.
    pub(crate) fn verdict_in(store: &Store, world: &str, article: &str, session: &str, accepted: bool) -> Answer<()> {
        if !store.is_gm(world) {
            return Err(AppError("Only the GM takes versions in or turns them down.".into()));
        }
        let db = store.db(world)?;
        let mut version = db.versions(article)?.into_iter().find(|v| v.session == session && v.review == "waiting").ok_or_else(|| AppError("This version does not wait.".into()))?;
        if accepted {
            let mut doc = db.document(article)?.ok_or_else(|| AppError("There is no such article.".into()))?;
            apply(&mut doc, &version);
            doc.changed = store::now();
            doc.changed_by = version.author.clone();
            db.put_document(&doc)?;
            version.review = "counted".into();
        } else {
            version.review = "rejected".into();
        }
        db.put_version(&version)?;
        Ok(())
    }

    /// A version waiting for review, and where.
    #[derive(Serialize)]
    pub struct WaitingVersion {
        pub article: String,
        pub name: String,
        pub session: String,
        pub author: String,
        /// Milliseconds since 1970.
        pub to: u64,
    }

    /// The versions waiting: for the GM, everything players wrote; for a
    /// player, their own — newest first.
    pub(crate) fn waiting_in(store: &Store, world: &str) -> Answer<Vec<WaitingVersion>> {
        let reader = store.reader(world)?;
        let me = store.actor()?;
        let db = store.db(world)?;
        let mut out = Vec::new();
        for doc in db.documents()? {
            for v in db.versions(&doc.id)?.into_iter().filter(|v| v.review == "waiting" && (reader.is_gm() || v.author == me)) {
                out.push(WaitingVersion { article: doc.id.clone(), name: doc.name().to_string(), session: v.session.clone(), author: author_name(store, world, &v.author), to: v.ended });
            }
        }
        out.sort_by_key(|v| std::cmp::Reverse(v.to));
        Ok(out)
    }

    /// Deletes a version for good — the GM's. The newest stays, and so does what waits.
    pub(crate) fn version_delete_in(store: &Store, world: &str, article: &str, session: &str) -> Answer<()> {
        if !store.is_gm(world) {
            return Err(AppError("Only the GM deletes versions.".into()));
        }
        let db = store.db(world)?;
        let mut versions = db.versions(article)?;
        versions.sort_by(|a, b| b.ended.cmp(&a.ended));
        let newest = versions.iter().find(|v| v.review == "counted").map(|v| v.session.clone());
        let Some(v) = versions.iter().find(|v| v.session == session) else { return Ok(()) };
        if newest.as_deref() == Some(session) || v.review == "waiting" {
            return Err(AppError("The newest version stays.".into()));
        }
        db.delete_version(article, session)?;
        Ok(())
    }
}

pub(crate) use history::{waiting_in, with_own_waiting};
pub use history::{PartChange, VersionDocument, VersionInfo, VersionMarks, WaitingVersion};

#[cfg(feature = "gui")]
/// The versions of a document, newest first — as far as the reader can see
/// what they changed.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn article_versions(app: tauri::AppHandle, world: String, article: String) -> Answer<Vec<VersionInfo>> {
    history::article_versions_in(&store(&app)?, world, article)
}

#[cfg(feature = "gui")]
/// What a version changed, part by part — as written, with names.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_compare(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<Vec<PartChange>> {
    history::version_compare_in(&store(&app)?, world, article, session)
}

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_document(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<VersionDocument> {
    history::version_document_in(&store(&app)?, world, article, session)
}

#[cfg(feature = "gui")]
/// The GM's word on a waiting version: taken into the text, or turned down.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_verdict(app: tauri::AppHandle, world: String, article: String, session: String, accepted: bool) -> Answer<()> {
    history::verdict_in(&store(&app)?, &world, &article, &session, accepted)
}

#[cfg(feature = "gui")]
/// The versions waiting: for the GM, everything players wrote; for a
/// player, their own — newest first.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn waiting_versions(app: tauri::AppHandle, world: String) -> Answer<Vec<WaitingVersion>> {
    history::waiting_in(&store(&app)?, &world)
}

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_marks(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<VersionMarks> {
    history::version_marks_in(&store(&app)?, world, article, session)
}

#[cfg(feature = "gui")]
/// Deletes a version for good — the GM's. The newest stays.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_delete(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<()> {
    history::version_delete_in(&store(&app)?, &world, &article, &session)
}

#[cfg(feature = "gui")]
/// Takes a version back — in every part the reader may edit. What others
/// wrote since stays.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_revert(app: tauri::AppHandle, world: String, article: String, session: String, into: String) -> Answer<usize> {
    history::version_revert_in(&store(&app)?, &world, &article, &session, &into)
}

/// Saves on top of the state as it is now — the way an editor opened just
/// before would.
#[cfg(test)]
pub(crate) fn save_now(store: &Store, world: &str, article: &str, text: &str) -> Answer<()> {
    let base = code_of(store, world, article)?.base;
    save(store, world, article, text, &base, "test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::template::{Infobox, TemplateField};
    use store::Role;

    struct World {
        dir: PathBuf,
        store: Store,
        id: String,
        home: String,
    }

    impl Drop for World {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn world(tag: &str) -> World {
        let dir = std::env::temp_dir().join(format!("aw-doc-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let meta = new_world(&store, &gm, "Eldermere".into()).unwrap();
        World { id: meta.id.clone(), home: meta.home.clone(), dir, store }
    }

    fn field(key: &str, label: &str, kind: FieldKind) -> TemplateField {
        TemplateField::new(key, label, kind)
    }

    fn template(id: &str, fields: Vec<TemplateField>, blocks: Vec<TemplateBlock>) -> Template {
        let mut t = Template {
            id: id.into(),
            name: id.into(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
            description: String::new(),
            properties: fields,
            infobox: Infobox { align: "right".into(),  ..Infobox::default() },
            auth: None,
            name_pattern: aw_model::template::just_input(),
            order: Vec::new(), sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            blocks,
        };
        t.normalize();
        t
    }

    fn part(key: &str, view: &str, edit: &str) -> BlockPart {
        BlockPart { key: key.into(), text: String::new(), view: view.into(), edit: edit.into() }
    }

    fn layer(section: &str, text: &str, view: &str, edit: &str) -> NewLayer {
        NewLayer {
            title: String::new(),
            text: text.into(),
            policy: view.into(),
            section: section.into(),
            edit: edit.into(),
            fields: serde_json::Value::Null,
            parent: None,
        }
    }

    fn doc(w: &World, kind: &str, name: &str, layers: Vec<NewLayer>) -> String {
        create_document(&w.store, &w.id, kind.into(), name.to_lowercase(), name.into(), "all".into(), layers).unwrap()
    }

    /// What someone with these keys reads, section by section.
    fn texts(w: &World, held: &Reader, article: &str) -> Vec<(String, String)> {
        let view = crate::view_with(&w.store, &w.id, held).unwrap();
        let a = view.into_iter().find(|a| a.id == article).expect("readable");
        a.layers.into_iter().map(|l| (l.section, l.text)).collect()
    }

    /// A player joins the world: a member of it, with no roles but their own.
    fn join(w: &World, name: &str) -> String {
        let player = Identity::create(name).unwrap();
        w.store.db(&w.id).unwrap().put_member(&store::member_of(&player.public(), Role::Player)).unwrap();
        player.id().to_string()
    }

    /// What `f` does in the name of that player.
    fn as_bob<T>(id: &str, f: impl FnOnce() -> T) -> T {
        store::acting_for(id, f)
    }

    /// A document's code around its body — the skeleton every document has.
    fn skeleton(name: &str, body: &str) -> String {
        format!("<article view=all edit=gm>\n<name={name}>\n<body>\n{body}</body>\n</article>\n")
    }

    #[test]
    fn the_world_page_is_built_like_every_document() {
        let w = world("page");
        let code = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(code.code, skeleton("Eldermere", "<block>\n</block>\n"), "a fresh page is one empty block");

        let body = "<block>\n# Welcome\n\nA world of mist.\n\n<list type=Place style=cards>\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", body)).unwrap();
        let again = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(again.code, skeleton("Eldermere", body));
    }

    #[test]
    fn a_section_keeps_its_create_button() {
        let w = world("new");
        let mut event = template("event", Vec::new(), Vec::new());
        event.name = "Event".into();
        crate::save_template(&w.store, &w.id, event).unwrap();
        let body = "<block title=Timeline new=\"Event, Session log\">\nWhat happened.\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", body)).unwrap();
        let back = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", body));
        assert_eq!(back.document.blocks[0].new, vec!["Event".to_string(), "Session log".to_string()]);
        // Stored by id: a renamed type is named anew.
        let mut event = w.store.load_templates(&w.id).unwrap().into_iter().find(|t| t.name == "Event").unwrap();
        event.name = "Happening".into();
        crate::save_template(&w.store, &w.id, event).unwrap();
        assert!(code_of(&w.store, &w.id, &w.home).unwrap().code.contains("new=\"Happening, Session log\""));
        // Taken off again.
        let plain = "<block title=Timeline>\nWhat happened.\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", plain)).unwrap();
        assert!(code_of(&w.store, &w.id, &w.home).unwrap().document.blocks[0].new.is_empty());
    }

    #[test]
    fn a_section_of_its_own_looks_as_a_types_does() {
        let w = world("shape");
        let body = "<block>\nWelcome.\n</block>\n\n<block title=Places style=box bottom fold=folded heading=2>\nHere.\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", body)).unwrap();
        let back = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", body));
        let places = &back.document.blocks[1];
        assert_eq!((places.style, places.place, places.fold, places.heading), (aw_model::template::BlockStyle::Box, aw_model::template::BlockPlace::Bottom, aw_model::template::BlockFold::Folded, 2));
        // The read view has it too: in the part's fields.
        let view = crate::view_with(&w.store, &w.id, &Reader::gm()).unwrap();
        let home = view.iter().find(|a| a.id == w.home).unwrap();
        let first = home.layers.iter().find(|l| l.title == "Places").unwrap();
        assert_eq!(first.fields["shape"]["place"], "bottom");
    }

    #[test]
    fn a_page_keeps_its_own_sections_in_the_order_written() {
        let w = world("own");
        let two = "<block>\nWelcome.\n</block>\n\n<block title=Places>\nThe coast.\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", two)).unwrap();
        // A new section between them, named but empty yet: it stays, there.
        let three = "<block>\nWelcome.\n</block>\n\n<block title=Timeline>\n</block>\n\n<block title=Places>\nThe coast.\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", three)).unwrap();
        assert_eq!(code_of(&w.store, &w.id, &w.home).unwrap().code, skeleton("Eldermere", three));
        // Moved to the top, it stays there — and the page's readers have the order.
        let moved = "<block title=Places>\nThe coast.\n</block>\n\n<block>\nWelcome.\n</block>\n\n<block title=Timeline>\n</block>\n";
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", moved)).unwrap();
        let back = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", moved));
        let view = crate::view_with(&w.store, &w.id, &Reader::holding(&[])).unwrap();
        let page = view.iter().find(|a| a.id == w.home).unwrap();
        assert_eq!(page.order, back.document.blocks.iter().map(|b| b.key.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn an_empty_section_has_its_part_to_write_in() {
        let w = world("empty");
        let story = TemplateBlock { key: "story".into(), title: "Story".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("story", "players", "gm")] };
        let allies = TemplateBlock { key: "allies".into(), title: "Allies".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("allies", "players", "gm")] };
        w.store.save_templates(&w.id, &[template("place", vec![], vec![story, allies])]).unwrap();
        let tavern = doc(&w, "place", "Tavern", vec![layer("story", "The tavern is old.", "players", "gm")]);
        let code = code_of(&w.store, &w.id, &tavern).unwrap();
        let block = code.document.blocks.iter().find(|b| b.key == "allies").expect("the empty section is there");
        assert_eq!((block.parts.len(), block.parts[0].key.as_str(), block.parts[0].text.as_str()), (1, "allies", ""));
        // Written in, it becomes a layer; left empty, it stays none.
        save_now(&w.store, &w.id, &tavern, &code.code.replace("<block title=Allies view=players edit=gm>\n", "<block title=Allies view=players edit=gm>\nThe smugglers.\n")).unwrap();
        let back = code_of(&w.store, &w.id, &tavern).unwrap();
        assert!(back.code.contains("<block title=Allies view=players edit=gm>\nThe smugglers.\n</block>"), "{}", back.code);
        // Empty, but with access of its own: that is kept.
        let inn = doc(&w, "place", "Inn", Vec::new());
        let empty = code_of(&w.store, &w.id, &inn).unwrap().code;
        let ruled = empty.replace("<block title=Allies view=players edit=gm>", "<block title=Allies view=players edit=\"gm OR player:bob\">");
        assert_ne!(ruled, empty);
        save_now(&w.store, &w.id, &inn, &ruled).unwrap();
        let again = code_of(&w.store, &w.id, &inn).unwrap();
        assert!(again.code.contains("<block title=Allies view=players edit=\"gm OR player:bob\">\n</block>"), "{}", again.code);
    }

    #[test]
    fn a_secret_pin_reaches_only_whom_it_names() {
        let w = world("pins");
        let body = |cave: &str| {
            format!("<block>\nThe coast.\n<map src=img:chart>\n<pin at=10,20 label=Harbour>\n<pin at=30,40 label=Cave{cave}>\n</map>\n</block>\n")
        };
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", &body(" view=gm edit=gm"))).unwrap();

        // The GM writes and reads one map; it reads back the same.
        let back = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", &body(" view=gm edit=gm")));
        // Stored apart: the players' part has the harbour only.
        let players = Reader::holding(&[]);
        let theirs = texts(&w, &players, &w.home);
        assert!(theirs.iter().any(|(_, t)| t.contains("Harbour")), "{theirs:?}");
        assert!(theirs.iter().all(|(_, t)| !t.contains("Cave")), "the cave is not theirs: {theirs:?}");
        let gms = texts(&w, &Reader::gm(), &w.home);
        assert!(gms.iter().any(|(_, t)| t.starts_with("<pins for=img:chart>") && t.contains("Cave")), "{gms:?}");

        // Revealed: onto the map, for everyone.
        save_now(&w.store, &w.id, &w.home, &skeleton("Eldermere", &body(""))).unwrap();
        let theirs = texts(&w, &players, &w.home);
        assert!(theirs.iter().any(|(_, t)| t.contains("<pin at=30,40 label=Cave>")), "{theirs:?}");
        assert_eq!(code_of(&w.store, &w.id, &w.home).unwrap().code, skeleton("Eldermere", &body("")));
    }

    #[test]
    fn the_skeleton_renames_and_releases() {
        let w = world("skeleton");
        let code = skeleton("Eldermere", "<block>\nFor everyone.\n</block>\n")
            .replace("<name=Eldermere>", "<name=\"The Mist Lands\">")
            .replace("<article view=all edit=gm>", "<article view=gm edit=gm>");
        save_now(&w.store, &w.id, &w.home, &code).unwrap();

        let back = code_of(&w.store, &w.id, &w.home).unwrap();
        assert_eq!(back.document.name, "The Mist Lands");
        assert_eq!(back.document.access.view, "gm");
        let players = Reader::holding(&[]);
        let view = crate::view_with(&w.store, &w.id, &players).unwrap();
        assert!(view.iter().all(|a| a.id != w.home), "the bracket hides the page from players now");
    }

    #[test]
    fn a_secret_region_becomes_a_layer_of_its_own() {
        let w = world("secret");
        save_now(
            &w.store,
            &w.id,
            &w.home,
            &skeleton(
                "Eldermere",
                "<block>\nFor everyone.\n\n<block view=gm edit=gm>\nThe hidden door.\n</block>\n</block>\n",
            ),
        )
        .unwrap();
        let players = Reader::holding(&[]);
        let seen: Vec<String> = texts(&w, &players, &w.home).into_iter().map(|(_, t)| t).collect();
        assert_eq!(seen, vec!["For everyone."], "players do not get the secret");

        let all: Vec<String> = texts(&w, &Reader::gm(), &w.home).into_iter().map(|(_, t)| t).collect();
        assert!(all.contains(&"The hidden door.".to_string()), "{all:?}");

        // The layer is kept gm-only.
        let page = w.store.db(&w.id).unwrap().document(&w.home).unwrap().unwrap();
        assert!(page.layers.iter().any(|l| l.access.view == "gm"));
    }

    #[test]
    fn properties_are_the_infobox_and_free_ones_too() {
        let w = world("props");
        let climate = template("climate", vec![], vec![]);
        let place = template(
            "place",
            vec![
                field("climate", "Climate", FieldKind::Reference { article_type: "climate".into(), multiple: false, parent: false, adds: false, count: String::new(), every: false, from: String::new() }),
                field("mayor", "Mayor", FieldKind::Text { multiline: false }),
            ],
            vec![],
        );
        w.store.save_templates(&w.id, &[climate, place]).unwrap();
        let warm = doc(&w, "climate", "warm", vec![]);
        let paris = doc(&w, "place", "Paris", vec![]);

        let code = code_of(&w.store, &w.id, &paris).unwrap();
        assert!(code.code.starts_with("---\nClimate:\nMayor:\n---"), "the type's fields stand ready:\n{}", code.code);

        let body = skeleton("Paris", "");
        save_now(&w.store, &w.id, &paris, &format!("---\nClimate: @@warm@@\nMayor: Vellmar\nMotto: Fluctuat nec mergitur\n---\n\n{body}")).unwrap();
        let held = Reader::gm();
        let view = crate::view_with(&w.store, &w.id, &held).unwrap();
        let fields = &view.iter().find(|a| a.id == paris).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields;
        assert_eq!(fields["climate"], serde_json::json!(warm), "a reference is stored by id");
        assert_eq!(fields["mayor"], "Vellmar");
        assert_eq!(fields["Motto"], "Fluctuat nec mergitur", "a free property by its name");

        let back = code_of(&w.store, &w.id, &paris).unwrap();
        assert_eq!(
            back.code,
            format!("---\nClimate: @@climate:warm@@\nMayor: Vellmar\nMotto: Fluctuat nec mergitur\n---\n\n{body}"),
            "a reference is written with its type"
        );

        let err = save_now(&w.store, &w.id, &paris, &format!("---\nClimate: @@tropical@@\n---\n\n{body}")).unwrap_err();
        assert!(err.0.contains("tropical"), "{}", err.0);
        let err = save_now(&w.store, &w.id, &paris, &format!("---\nClimate: @@place:warm@@\n---\n\n{body}")).unwrap_err();
        assert!(err.0.contains("place:warm"), "the type has to fit: {}", err.0);
    }

    #[test]
    fn a_number_is_a_value_or_says_what_it_does() {
        let w = world("sets");
        let item = template("item", vec![field("str", "Strength", FieldKind::Number { unit: None })], vec![]);
        w.store.save_templates(&w.id, &[item]).unwrap();
        let gauntlets = doc(&w, "item", "Gauntlets", vec![]);
        let code = |value: &str| format!("---\nStrength: {value}\n---\n\n{}", skeleton("Gauntlets", ""));
        let stored = |w: &World| {
            let held = Reader::gm();
            let view = crate::view_with(&w.store, &w.id, &held).unwrap();
            view.iter().find(|a| a.id == gauntlets).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields["str"].clone()
        };
        for (written, kept, back) in [
            ("= 19", serde_json::json!("=19"), "=19"),
            (">= 19", serde_json::json!(">=19"), ">=19"),
            ("≥19", serde_json::json!(">=19"), ">=19"),
            ("<=3", serde_json::json!("<=3"), "<=3"),
            ("[[14 + min(Dexterity mod, 2)]]", serde_json::json!("[[14 + min(Dexterity mod, 2)]]"), "[[14 + min(Dexterity mod, 2)]]"),
            ("+1", serde_json::json!("+1"), "+1"),
            ("- 2", serde_json::json!("-2"), "-2"),
            ("15", serde_json::json!(15), "15"),
        ] {
            save_now(&w.store, &w.id, &gauntlets, &code(written)).unwrap();
            assert_eq!(stored(&w), kept, "{written}");
            assert!(code_of(&w.store, &w.id, &gauntlets).unwrap().code.contains(&format!("Strength: {back}\n")), "{written}");
        }
        assert!(save_now(&w.store, &w.id, &gauntlets, &code("= much")).is_err());
    }

    #[test]
    fn dates_are_written_in_the_world_calendar_and_ours() {
        let w = world("dates");
        let session = template(
            "session",
            vec![field("when", "When", FieldKind::WorldDate), field("played", "Played", FieldKind::DateTime)],
            vec![],
        );
        w.store.save_templates(&w.id, &[session]).unwrap();
        let one = doc(&w, "session", "One", vec![]);
        let code = |when: &str, played: &str| format!("---\nWhen: {when}\nPlayed: {played}\n---\n\n{}", skeleton("One", ""));
        save_now(&w.store, &w.id, &one, &code("15 Hay Moon 812, 18:30", "12.09.2026, 19:00")).unwrap();

        let held = Reader::gm();
        let view = crate::view_with(&w.store, &w.id, &held).unwrap();
        let fields = &view.iter().find(|a| a.id == one).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields;
        let day = aw_model::Era::default().to_date(812, 7, 15).unwrap().0 as f64;
        assert_eq!(fields["when"].as_f64(), Some(day + 18.5 / 24.0), "a day count, the time as its fraction");
        assert_eq!(fields["played"], "2026-09-12T19:00");

        let back = code_of(&w.store, &w.id, &one).unwrap().code;
        assert!(back.contains("When: 15 Hay Moon 812 AR, 18:30"), "{back}");
        assert!(back.contains("Played: 2026-09-12T19:00"), "{back}");
        let err = save_now(&w.store, &w.id, &one, &code("next Tuesday", "")).unwrap_err();
        assert!(err.0.contains("When is a date"), "{}", err.0);
    }

    #[test]
    fn text_reads_and_binds_only_what_exists() {
        let w = world("uses");
        let place = template(
            "place",
            vec![field("mayor", "Mayor", FieldKind::Text { multiline: false })],
            vec![TemplateBlock {
                key: "story".into(),
                title: "Story".into(),
                style: Default::default(),
                place: Default::default(), width: 0,
                fold: Default::default(),
                heading: 1,
                new: Vec::new(),
                parts: vec![part("story", "all", "gm")],
            }],
        );
        w.store.save_templates(&w.id, &[place]).unwrap();
        let paris = doc(&w, "place", "Paris", vec![]);

        let code = |text: &str| {
            format!("---\nMayor: Vellmar\n---\n\n{}", skeleton("Paris", &format!("<block title=Story>\n{text}\n</block>\n")))
        };
        save_now(&w.store, &w.id, &paris, &code("The mayor is %%Mayor%%.\n<text bind=Mayor>")).unwrap();
        let err = save_now(&w.store, &w.id, &paris, &code("Ask %%Nobody%%.")).unwrap_err();
        assert!(err.0.contains("Nobody") && err.0.starts_with("Line 9"), "{}", err.0);

        // Stored by key — so renaming the property renames it everywhere.
        let held = Reader::gm();
        let view = crate::view_with(&w.store, &w.id, &held).unwrap();
        let story = view.iter().find(|a| a.id == paris).unwrap().layers.iter().find(|l| l.section == "story").unwrap();
        assert_eq!(story.text, "The mayor is %%mayor%%.\n<text bind=mayor>", "by key, not by name");
        let mut types = w.store.load_templates(&w.id).unwrap();
        types[0].properties[0].label = "Bürgermeister".into();
        w.store.save_templates(&w.id, &types).unwrap();
        let back = code_of(&w.store, &w.id, &paris).unwrap().code;
        assert!(back.contains("Bürgermeister: Vellmar"), "{back}");
        assert!(back.contains("The mayor is %%Bürgermeister%%.\n<text bind=Bürgermeister>"), "{back}");

        // A list names the listed type's properties — and comes back with the
        // current names.
        let list = "<list type=place show=Bürgermeister where=\"Bürgermeister=Vellmar\">";
        save_now(&w.store, &w.id, &paris, &code(list).replace("Mayor: Vellmar", "Bürgermeister: Vellmar")).unwrap();
        let back = code_of(&w.store, &w.id, &paris).unwrap().code;
        // Quoted: unquoted, its `=` would make Markdown read the tag as text.
        assert!(back.contains("<list type=place show=Bürgermeister where=\"Bürgermeister=Vellmar\">"), "{back}");

        // Another document's property: by the document's id and the key.
        let home = skeleton("Eldermere", "<block>\nParis is ruled by @@place:Paris#Bürgermeister@@.\n</block>\n");
        save_now(&w.store, &w.id, &w.home, &home).unwrap();
        let stored = crate::view_with(&w.store, &w.id, &held).unwrap();
        let text = &stored.iter().find(|a| a.id == w.home).unwrap().layers[0].text;
        assert_eq!(text, &format!("Paris is ruled by @@{paris}#mayor@@."));
        let mut types = w.store.load_templates(&w.id).unwrap();
        types[0].properties[0].label = "Mayor".into();
        w.store.save_templates(&w.id, &types).unwrap();
        let back = code_of(&w.store, &w.id, &w.home).unwrap().code;
        assert!(back.contains("@@place:Paris#Mayor@@"), "{back}");
    }

    /// How long ten thousand documents take — a compendium's size. Run by hand:
    /// `cargo test --release many_documents -- --ignored --nocapture`.
    #[test]
    fn the_infobox_has_access_of_its_own() {
        let w = world("infobox");
        let npc = template("npc", vec![field("title", "Title", FieldKind::Text { multiline: false })], vec![]);
        w.store.save_templates(&w.id, &[npc]).unwrap();
        let mira = doc(&w, "npc", "Mira", vec![]);
        let infobox = |held: &Reader| {
            let all = crate::view_with(&w.store, &w.id, held).unwrap();
            let layer = all.iter().find(|a| a.id == mira).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).map(|l| (l.policy.clone(), l.edit.clone())).unwrap();
            layer
        };
        let held = Reader::gm();

        // Players may change the infobox of a document only the GM changes.
        let body = skeleton("Mira", "<infobox view=all edit=players>\n");
        save_now(&w.store, &w.id, &mira, &format!("---\nTitle: Innkeeper\n---\n\n{body}")).unwrap();
        assert_eq!(infobox(&held), ("all".to_string(), "players".to_string()));
        let back = code_of(&w.store, &w.id, &mira).unwrap().code;
        assert!(back.contains("<body>\n<infobox view=all edit=players>\n"), "{back}");

        // Without one of its own it goes as the document.
        save_now(&w.store, &w.id, &mira, &format!("---\nTitle: Innkeeper\n---\n\n{}", skeleton("Mira", ""))).unwrap();
        assert_eq!(infobox(&held), ("all".to_string(), "gm".to_string()));
    }

    #[test]
    fn a_region_written_into_a_section_leaves_the_infobox_as_it_was() {
        let w = world("region-infobox");
        let mut log = template(
            "log",
            vec![field("date", "Date", FieldKind::Text { multiline: false })],
            vec![TemplateBlock { parts: vec![part("journal", "all", "players")], ..TemplateBlock::fresh("journal", "Journal") }],
        );
        log.auth = Some(Access { view: "all".into(), edit: "players".into() });
        w.store.save_templates(&w.id, &[log]).unwrap();
        let mut props = layer(INFOBOX_SECTION, "", "all", "players");
        props.fields = serde_json::json!({ "date": "1 Hammer" });
        let id = create_document(&w.store, &w.id, "log".into(), "s1".into(), "Session 1".into(), "all".into(), vec![props, layer("journal", "They meet.", "all", "players")]).unwrap();
        let held = Reader::gm();
        let infobox = || crate::view_with(&w.store, &w.id, &held).unwrap().into_iter().find(|a| a.id == id).unwrap().layers.into_iter().find(|l| l.section == INFOBOX_SECTION).map(|l| (l.policy, l.edit)).unwrap();
        assert_eq!(infobox(), ("all".to_string(), "players".to_string()));
        let code = code_of(&w.store, &w.id, &id).unwrap().code;
        let region = code.replace("They meet.\n", "They meet.\n\n<block view=\"gm OR doc:ABC\" edit=\"gm OR doc:ABC\">\nA secret.\n</block>\n");
        assert_ne!(region, code);
        save_now(&w.store, &w.id, &id, &region).unwrap();
        assert_eq!(infobox(), ("all".to_string(), "players".to_string()), "{}", code_of(&w.store, &w.id, &id).unwrap().code);
    }

    #[test]
    fn a_player_gets_types_and_the_side_bar_without_what_is_the_gms() {
        use aw_model::navigation::{NavEntry, NavKind};
        let w = world("types-seen");
        let mut secret = field("goal", "True goal", FieldKind::Text { multiline: false });
        secret.access = Some(Access { view: "gm".into(), edit: "gm".into() });
        let mut notes = part("notes", "gm", "gm");
        notes.text = "Dragon lair beneath the inn.".into();
        let npc = template(
            "npc",
            vec![field("title", "Title", FieldKind::Text { multiline: false }), secret],
            vec![
                TemplateBlock { parts: vec![part("story", "all", "gm")], ..TemplateBlock::fresh("story", "Story") },
                TemplateBlock { parts: vec![notes], ..TemplateBlock::fresh("notes", "GM notes") },
            ],
        );
        w.store.save_templates(&w.id, &[npc]).unwrap();
        let entry = |id: &str, label: &str, gm: bool| NavEntry { id: id.into(), kind: NavKind::Group, target: String::new(), label: label.into(), gm, children: Vec::new() };
        w.store.set_navigation(&w.id, &[entry("a", "Places", false), entry("b", "Secret lair", true)]).unwrap();
        let bob = join(&w, "Bob");

        let types = serde_json::to_string(&as_bob(&bob, || crate::templates_load_in(&w.store, w.id.clone())).unwrap()).unwrap();
        assert!(types.contains("Title") && types.contains("Story"), "{types}");
        assert!(!types.contains("True goal") && !types.contains("Dragon lair") && !types.contains("GM notes"), "{types}");
        let nav = serde_json::to_string(&as_bob(&bob, || crate::navigation_load_in(&w.store, w.id.clone())).unwrap()).unwrap();
        assert!(nav.contains("Places") && !nav.contains("Secret lair"), "{nav}");

        // The GM gets everything.
        let all = serde_json::to_string(&crate::templates_load_in(&w.store, w.id.clone()).unwrap()).unwrap();
        assert!(all.contains("True goal") && all.contains("Dragon lair"));
        assert!(serde_json::to_string(&crate::navigation_load_in(&w.store, w.id.clone()).unwrap()).unwrap().contains("Secret lair"));

        // Nor does a document of the type show the player the GM's section.
        let mira = doc(&w, "npc", "Mira", vec![]);
        let code = as_bob(&bob, || code_of(&w.store, &w.id, &mira)).unwrap().code;
        assert!(!code.contains("GM notes") && !code.contains("Dragon lair") && !code.contains("True goal"), "{code}");
    }

    #[test]
    fn a_player_does_not_learn_on_which_hidden_page_the_gm_is() {
        let w = world("presence");
        let lair = create_document(&w.store, &w.id, String::new(), "lair".into(), "Lair".into(), "gm".into(), vec![]).unwrap();
        let bob = join(&w, "Bob");
        crate::live::world_presence_in(&w.store, w.id.clone(), lair.clone()).unwrap();
        let seen = as_bob(&bob, || crate::live::world_presence_in(&w.store, w.id.clone(), w.home.clone())).unwrap();
        let marla = seen.iter().find(|h| h.name == "Marla").expect("the GM is there");
        assert_eq!(marla.article, "", "but not where");
        let gm = crate::live::world_presence_in(&w.store, w.id.clone(), lair.clone()).unwrap();
        assert_eq!(gm.iter().find(|h| h.name == "Bob").unwrap().article, w.home);
    }

    #[test]
    fn a_field_with_access_of_its_own_is_a_layer_of_its_own() {
        let w = world("field");
        let mut secret = field("goal", "True goal", FieldKind::Text { multiline: false });
        secret.access = Some(Access { view: "gm".into(), edit: "gm".into() });
        let npc = template("npc", vec![field("title", "Title", FieldKind::Text { multiline: false }), secret], vec![]);
        w.store.save_templates(&w.id, &[npc]).unwrap();
        let mira = doc(&w, "npc", "Mira", vec![]);

        let body = skeleton("Mira", "");
        save_now(&w.store, &w.id, &mira, &format!("---\nTitle: Innkeeper\nTrue goal: Revenge\n---\n\n{body}")).unwrap();

        let all = crate::view_with(&w.store, &w.id, &Reader::gm()).unwrap();
        let layers = &all.iter().find(|a| a.id == mira).unwrap().layers;
        let own = layers.iter().find(|l| l.section == "properties:goal").expect("a layer of its own");
        assert_eq!(own.fields["goal"], "Revenge");
        assert!(layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields.get("goal").is_none());
        let raw = w.store.db(&w.id).unwrap().document(&mira).unwrap().unwrap();
        assert!(raw.layers.iter().any(|l| l.access.view == "gm"), "only the GM reads it");

        // Players get the title, not the goal — not even its label.
        let players = Reader::holding(&[]);
        let theirs = crate::view_with(&w.store, &w.id, &players).unwrap();
        let layers = &theirs.iter().find(|a| a.id == mira).unwrap().layers;
        assert!(layers.iter().all(|l| l.section != "properties:goal"));

        let back = code_of(&w.store, &w.id, &mira).unwrap();
        assert!(back.code.starts_with("---\nTitle: Innkeeper\nTrue goal: Revenge\n---"), "{}", back.code);
    }

    #[test]
    fn a_reader_who_misses_a_part_between_two_writes_both() {
        let w = world("between");
        let body = "<block title=Journal>\nFirst.\n<block view=gm edit=gm>\nSecret.\n</block>\nSecond.\n</block>\n";
        let page = doc(&w, "", "Session", vec![]);
        let code = format!("<article view=all edit=players>\n<name=Session>\n<body>\n{body}</body>\n</article>\n");
        save_now(&w.store, &w.id, &page, &code).unwrap();
        let bob = join(&w, "Bob");

        // Bob sees two parts, apart, and changes the second.
        let theirs = as_bob(&bob, || code_of(&w.store, &w.id, &page)).unwrap().code;
        assert!(!theirs.contains("Secret"), "{theirs}");
        as_bob(&bob, || save_now(&w.store, &w.id, &page, &theirs.replace("Second.", "Second, _changed_."))).unwrap();
        let back = code_of(&w.store, &w.id, &page).unwrap().code;
        assert!(back.contains("First.\n\n<block view=gm edit=gm>\nSecret.\n</block>\n\nSecond, _changed_.\n</block>"), "{back}");
    }

    #[test]
    fn a_types_access_goes_to_its_documents_and_their_sections_alike() {
        let w = world("typeaccess");
        let story = TemplateBlock { key: "story".into(), title: "Story".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("story", "all", "gm")] };
        let secret = TemplateBlock { key: "secret".into(), title: "Secret".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("secret", "gm", "gm")] };
        w.store.save_templates(&w.id, &[template("npc", vec![], vec![story, secret])]).unwrap();
        let ilva = doc(&w, "npc", "Ilva", vec![layer("story", "A smuggler.", "all", "gm"), layer("secret", "Works for the duke.", "gm", "gm")]);
        let other = doc(&w, "npc", "Bram", vec![layer("story", "A fisher.", "all", "gm")]);
        let n = crate::type_access_apply_in(&w.store, &w.id, "npc", &[ilva.clone()], None, Some("players".into())).unwrap();
        assert_eq!(n, 1);
        let code = code_of(&w.store, &w.id, &ilva).unwrap().code;
        assert!(code.starts_with("<article view=all edit=players>"), "{code}");
        assert!(code.contains("<block title=Story>\nA smuggler."), "the section follows: {code}");
        assert!(code.contains("<block title=Secret view=gm edit=gm>"), "the secret keeps its own: {code}");
        assert!(code_of(&w.store, &w.id, &other).unwrap().code.contains("edit=gm>"), "only the documents named");
    }

    #[test]
    fn a_player_may_only_what_they_may() {
        let w = world("player");
        let story = TemplateBlock {
            key: "story".into(),
            title: "Story".into(),
            style: Default::default(),
            place: Default::default(), width: 0,
            fold: Default::default(),
            heading: 1,
            new: Vec::new(),
            parts: vec![part("story", "players", "gm")],
        };
        let journal = TemplateBlock {
            key: "journal".into(),
            title: "Journal".into(),
            style: Default::default(),
            place: Default::default(), width: 0,
            fold: Default::default(),
            heading: 1,
            new: Vec::new(),
            parts: vec![part("journal", "players", "players")],
        };
        w.store.save_templates(&w.id, &[template("place", vec![], vec![story, journal])]).unwrap();
        let tavern = doc(&w, "place", "Tavern", vec![
            layer("story", "The tavern is old.", "players", "gm"),
            layer("story-2", "The innkeeper is a spy.", "gm", "gm"),
            layer("journal", "We arrived at night.", "players", "players"),
        ]);

        // Bob joins.
        let bob = join(&w, "Bob");

        let code = as_bob(&bob, || code_of(&w.store, &w.id, &tavern)).unwrap().code;
        assert!(!code.contains("spy"), "the code holds only what Bob may see:\n{code}");

        // The journal is his to write.
        let journal_edit = code.replace("We arrived at night.", "We arrived at night. It rained.");
        as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &journal_edit)).unwrap();

        // The story is not.
        let story_edit = journal_edit.replace("The tavern is old.", "The tavern is mine.");
        let err = as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &story_edit)).unwrap_err();
        assert!(err.0.contains("may not change"), "{}", err.0);

        // Who sees his journal he may say; who edits it is not his to say.
        assert!(journal_edit.contains("<block title=Journal view=players edit=players>"), "{journal_edit}");
        let access_edit = journal_edit.replace("<block title=Journal view=players edit=players>", "<block title=Journal view=all edit=players>");
        as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &access_edit)).unwrap();
        let now = code_of(&w.store, &w.id, &tavern).unwrap().code;
        assert!(now.contains("<block title=Journal view=all edit=players>"), "set: {now}");
        let access_edit = journal_edit.replace("<block title=Journal view=players edit=players>", "<block title=Journal view=players edit=gm>");
        let err = as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &access_edit)).unwrap_err();
        assert!(err.0.contains("Only the GM"), "{}", err.0);

        // Nor the name, nor who sees the whole document.
        let code = as_bob(&bob, || code_of(&w.store, &w.id, &tavern)).unwrap().code;
        let rename = code.replace("<name=Tavern>", "<name=\"Bob's place\">");
        assert!(as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &rename)).is_err());
        let release = code.replace("<article view=all edit=gm>", "<article view=players edit=gm>");
        let err = as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &release)).unwrap_err();
        assert!(err.0.contains("Only the GM"), "{}", err.0);

        // Nor removing a part.
        let removal = code.replace("The tavern is old.\n", "");
        assert!(as_bob(&bob, || save_now(&w.store, &w.id, &tavern, &removal)).is_err());

        // And the part he never saw is still there, untouched — and never
        // came to him at all.
        let stored = w.store.db(&w.id).unwrap().document(&tavern).unwrap().unwrap();
        assert_eq!(stored.layers.len(), 4, "base, story, secret, journal");
        let theirs = as_bob(&bob, || crate::view_in(&w.store, w.id.clone(), None)).unwrap();
        let t = theirs.iter().find(|a| a.id == tavern).unwrap();
        assert!(t.layers.iter().all(|l| !l.text.contains("spy")));
        let journal = t.layers.iter().find(|l| l.section == "journal").unwrap();
        assert_eq!(journal.text, "We arrived at night. It rained.");
    }

    #[test]
    fn relations_stand_on_both_sides() {
        let w = world("relations");
        let roles = template("relation", vec![], vec![]);
        let character = template(
            "character",
            vec![field("rel", "Relations", FieldKind::Relation { article_type: "character".into(), role_type: "relation".into() })],
            vec![],
        );
        w.store.save_templates(&w.id, &[roles, character]).unwrap();
        let [mother, daughter, aunt] = ["Mother", "Daughter", "Aunt"].map(|n| doc(&w, "relation", n, vec![]));
        let marie = doc(&w, "character", "Marie", vec![]);
        let olga = doc(&w, "character", "Olga", vec![]);
        let helga = create_document(&w.store, &w.id, "character".into(), "helga".into(), "Helga".into(), "gm".into(), vec![]).unwrap();
        let body = skeleton("Marie", "");
        let code = |rel: &str| format!("---\nRelations: {rel}\n---\n\n{body}");
        let relations = |article: &str| -> String {
            let back = code_of(&w.store, &w.id, article).unwrap();
            back.document.properties.iter().find(|p| p.name == "Relations").map(|p| p.value.clone()).unwrap_or_default()
        };

        save_now(&w.store, &w.id, &marie, &code("@@Olga@@ Mother/Daughter, @@Helga@@ Aunt")).unwrap();
        assert_eq!(relations(&marie), "@@character:Olga@@ Mother/Daughter, @@character:Helga@@ Aunt");
        assert_eq!(relations(&olga), "@@character:Marie@@ Daughter/Mother", "written on the other side, turned round");
        assert_eq!(relations(&helga), "@@character:Marie@@ /Aunt");
        let held = Reader::gm();
        let view = crate::view_with(&w.store, &w.id, &held).unwrap();
        let stored = &view.iter().find(|a| a.id == marie).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields;
        assert_eq!(stored["rel"][0], serde_json::json!({ "with": olga, "role": mother, "own_role": daughter }), "by id");

        // A change goes over too; what is gone from one side goes from both.
        save_now(&w.store, &w.id, &marie, &code("@@Olga@@ Aunt, @@Helga@@ Aunt")).unwrap();
        assert_eq!(relations(&olga), "@@character:Marie@@ /Aunt");
        save_now(&w.store, &w.id, &marie, &code("@@Helga@@ Aunt")).unwrap();
        assert_eq!(relations(&olga), "");

        // Unknown roles and relations to oneself are errors.
        let err = save_now(&w.store, &w.id, &marie, &code("@@Olga@@ Cousin")).unwrap_err();
        assert!(err.0.contains("Cousin"), "{}", err.0);
        let err = save_now(&w.store, &w.id, &marie, &code("@@Marie@@ Aunt")).unwrap_err();
        assert!(err.0.contains("itself"), "{}", err.0);

        // A player who cannot open Helga does not see whom the entry names —
        // and saving what they see keeps it.
        let players = Reader::holding(&[]);
        let theirs = open(&w.store, &w.id, &marie, &players).unwrap();
        let written = theirs.text.properties.iter().find(|p| p.name == "Relations").unwrap();
        assert_eq!(written.value, "", "the entry to Helga is not in their text");
        let fields = fields_from(&theirs, &[Property { name: "Relations".into(), value: "@@Olga@@ Mother".into(), group: Vec::new() }]).unwrap();
        let kept = relation::entries(&fields["rel"]);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert!(kept.iter().any(|e| e.with == helga && e.role == aunt), "{kept:?}");

        // From the graph: one call, both sides.
        set_relation(&w.store, &w.id, &olga, "rel", &marie, Some((daughter.clone(), mother.clone())), "graph").unwrap();
        assert_eq!(relations(&marie), "@@character:Helga@@ Aunt, @@character:Olga@@ Mother/Daughter");
        set_relation(&w.store, &w.id, &marie, "rel", &olga, None, "graph").unwrap();
        assert_eq!(relations(&olga), "");

        // A document that goes is forgotten by those it named.
        forget_relations(&w.store, &w.id, &helga).unwrap();
        assert_eq!(relations(&marie), "");

        // A graph in the text keeps its people and lines — stored by id,
        // written by name.
        let graph = "<graph around=self depth=2>\n<person who=Olga at=3,4>\n<line between=\"Marie, Olga\" color=amber sides=\"bottom, top\">\n</graph>\n";
        let code = format!("---\nRelations: @@Olga@@ Mother/Daughter\n---\n\n{}", skeleton("Marie", &format!("<block title=Tree>\n{graph}</block>\n")));
        save_now(&w.store, &w.id, &marie, &code).unwrap();
        let back = code_of(&w.store, &w.id, &marie).unwrap().code;
        assert!(back.contains(graph), "{back}");
        let stored = crate::view_with(&w.store, &w.id, &held).unwrap();
        let text = &stored.iter().find(|a| a.id == marie).unwrap().layers.iter().find(|l| l.title == "Tree" || l.text.contains("<graph")).unwrap().text;
        assert!(text.contains(&format!("<person who={olga} at=3,4>")), "{text}");
    }

    #[test]
    fn a_moment_of_ours_is_read_in_the_usual_ways() {
        for (written, stored) in [
            ("2026-09-12 19:00", Some("2026-09-12T19:00")),
            ("2026-09-12T19:00", Some("2026-09-12T19:00")),
            ("12.09.2026, 19:00", Some("2026-09-12T19:00")),
            ("12.09.2026", Some("2026-09-12")),
            ("2026-9-3 7:05", Some("2026-09-03T07:05")),
            ("tomorrow", None),
            ("2026-13-01", None),
            ("2026-09-12 25:00", None),
        ] {
            assert_eq!(moment_of_ours(written).as_deref(), stored, "{written}");
        }
    }
}
