//! A document as code: opening, reading, writing and saving it.
//!
//! The translation itself lives in `aw_model::document`. What happens here
//! needs the keys: which layers the reader can open, and what a changed piece
//! of text does to the encrypted packages.
//!
//! The rules on saving:
//!
//! - The code only ever holds what the reader could decrypt. Layers they
//!   cannot open are not in it — and are left exactly as they are.
//! - Saving writes what changed against the state the editor started from,
//!   not against now: whatever someone else wrote meanwhile stays, and the
//!   two merge.
//! - Changing a part's text needs the right to edit it.
//! - Changing who sees a part, adding a part and removing one needs the full
//!   keyring — the game master's. A player cannot wrap a key for anyone.

use super::*;
use aw_model::document::{self, DocumentRead, DocumentText, Property, Shape};
use aw_model::link;
use aw_model::names::{self, Catalog};
use aw_model::relation::{self, Entry};
use aw_model::template::{Access, BlockPart, FieldKind, TemplateBlock};
use aw_store::journal::diff::{self, Range};
use aw_store::journal::{Access as LayerAccess, Review};

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

/// A document opened with the reader's keys.
struct Opened {
    /// Position of the article in the stored list.
    index: usize,
    /// The article's id.
    id: String,
    text: DocumentText,
    shape: Shape,
    /// The type with everything it inherits — `None` without one.
    template: Option<Template>,
    /// Readable layers by section: position in the layer list and plaintext.
    readable: Vec<(String, usize, LayerContent)>,
    /// The base layer: its position and plaintext — name and access.
    base: (usize, LayerContent),
    /// Every document and type the reader knows — for names in the text.
    known: std::sync::Arc<Known>,
    /// The document's type by id — whose properties its text uses.
    article_type: String,
    /// Whether the reader holds the full keyring.
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

/// What this reader knows by name: the documents they can open, the types.
pub(crate) struct Known {
    names: Vec<Named>,
    templates: Vec<Template>,
}

pub(crate) fn known(articles: &[StoredArticle], held: &HeldKeys, templates: Vec<Template>) -> Known {
    known_of(names_of(articles, held), templates)
}

/// Id, name and type of each document this reader may see: only its base
/// opened, on every core — the rest of it is not needed for its name.
fn names_of(articles: &[StoredArticle], held: &HeldKeys) -> Vec<(String, String, String)> {
    let one = |a: &StoredArticle| {
        let content = a.layers.iter().find(|l| l.is_base)?.content(held)?;
        Some((a.id.to_string(), content.title, a.article_type.clone()))
    };
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    if articles.len() < 256 || threads < 2 {
        return articles.iter().filter_map(one).collect();
    }
    std::thread::scope(|scope| {
        let workers: Vec<_> = articles.chunks(articles.len().div_ceil(threads)).map(|some| scope.spawn(move || some.iter().filter_map(one).collect::<Vec<_>>())).collect();
        workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
    })
}

/// The same, kept per world and keys held, and brought up to date by what
/// changed since: building it opens every document, and every keystroke in
/// an editor asks for it — with thousands of documents, that is what typing
/// would wait for. A save changes one document; only that one is read again.
fn known_shared(
    store: &Store,
    world: &str,
    articles: &[StoredArticle],
    held: &HeldKeys,
    templates: Vec<Template>,
) -> Result<std::sync::Arc<Known>, AppError> {
    type Kept = std::collections::HashMap<(PathBuf, String), (u64, String, std::sync::Arc<Known>)>;
    static KEPT: std::sync::OnceLock<std::sync::Mutex<Kept>> = std::sync::OnceLock::new();
    let at = (store.profile_dir().to_path_buf(), world.to_string());
    let version = store.journal_version(world)?;
    let mut keys: Vec<String> = held.iter().map(|(a, _)| a.to_string()).collect();
    keys.sort();
    let keys = keys.join(" ");
    let kept = KEPT.get_or_init(Default::default);
    let before = kept.lock().ok().and_then(|m| m.get(&at).cloned());
    let fresh = match before {
        Some((v, k, known)) if k == keys && known.templates == templates => {
            if v == version {
                return Ok(known);
            }
            match store.changed_since(world, v)? {
                // Only the documents that changed: read again, put in place.
                Some(changed) => {
                    let ids: std::collections::HashSet<String> = changed.iter().map(|a| a.to_string()).collect();
                    let again: Vec<StoredArticle> = articles.iter().filter(|a| ids.contains(&a.id.to_string())).cloned().collect();
                    let mut names: Vec<Named> = known.names.iter().filter(|n| !ids.contains(&n.id)).cloned().collect();
                    names.extend(self::known(&again, held, templates.clone()).names);
                    Known { names, templates }
                }
                None => self::known(articles, held, templates),
            }
        }
        _ => self::known(articles, held, templates),
    };
    let fresh = std::sync::Arc::new(fresh);
    if let Ok(mut m) = kept.lock() {
        m.insert(at, (version, keys, fresh.clone()));
    }
    Ok(fresh)
}

/// The same, from documents as viewed — those waiting on the GM among them.
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
                .map(|t| {
                    std::iter::once(t)
                        .chain(types::ancestors(&templates, t))
                        .map(|t| t.name.clone())
                        .collect()
                })
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
        self.names
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.types.first().cloned().unwrap_or_default(), n.name.clone()))
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
}

/// The state of a layer an editor starts from.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Base {
    pub generation: u32,
    pub heads: Vec<String>,
}

/// Every layer the editor sees, by id — with the state it saw.
pub type Bases = std::collections::BTreeMap<String, Base>;

/// What the frontend gets when a document is opened for editing.
#[derive(Serialize)]
pub struct DocumentCode {
    pub code: String,
    pub document: DocumentText,
    pub base: Bases,
    /// The images of its image fields, by label — for the readable copy.
    #[serde(skip)]
    pub image_fields: Vec<(String, String)>,
}

fn open(
    store: &Store,
    world: &str,
    article: &str,
    held: &HeldKeys,
    articles: &[StoredArticle],
) -> Result<Opened, AppError> {
    let templates = store.load_templates(world)?;
    let calendar = store.load_world_meta(world)?.calendar;
    open_with(store, world, article, held, articles, templates, store.is_gm(world), calendar)
}

/// What opening many documents takes from the world — loaded once, not once
/// for each of them.
pub(crate) struct Shared {
    held: HeldKeys,
    articles: std::sync::Arc<Vec<StoredArticle>>,
    templates: Vec<Template>,
    gm: bool,
    calendar: aw_model::WorldCalendar,
}

impl Shared {
    pub(crate) fn load(store: &Store, world: &str, identity: &Identity) -> Answer<Self> {
        Ok(Shared {
            held: store.view_keys(world, identity)?,
            articles: store.load_articles(world)?,
            templates: store.load_templates(world)?,
            gm: store.is_gm(world),
            calendar: store.load_world_meta(world)?.calendar,
        })
    }
}

/// `code_of` for one of many documents.
pub(crate) fn code_in(store: &Store, world: &str, article: &str, shared: &Shared) -> Answer<DocumentCode> {
    let opened = open_with(store, world, article, &shared.held, &shared.articles, shared.templates.clone(), shared.gm, shared.calendar.clone())?;
    code_from(store, world, &shared.articles, opened)
}

#[allow(clippy::too_many_arguments)]
fn open_with(
    store: &Store,
    world: &str,
    article: &str,
    held: &HeldKeys,
    articles: &[StoredArticle],
    templates: Vec<Template>,
    gm: bool,
    calendar: aw_model::WorldCalendar,
) -> Result<Opened, AppError> {
    let index = articles
        .iter()
        .position(|a| a.id.to_string() == article)
        .ok_or_else(|| AppError("There is no such article.".into()))?;
    let stored = &articles[index];
    let template = types::find(&templates, &stored.article_type).map(|t| types::effective(&templates, t));

    let base_index = stored
        .layers
        .iter()
        .position(|l| l.is_base)
        .ok_or_else(|| AppError("This document has no base layer.".into()))?;
    let base = &stored.layers[base_index];
    let base_content = layer_content(base, held)?;
    let visibility = base_content.own.clone();

    let mut readable = Vec::new();
    for (i, layer) in stored.layers.iter().enumerate() {
        if layer.is_base {
            continue;
        }
        if let Ok(content) = layer_content(layer, held) {
            readable.push((content.section.clone(), i, content));
        }
    }

    let known = known_shared(store, world, articles, held, templates.clone())?;
    let name = base_content.title.clone();
    let edit = base_content.edit.clone();

    let mut opened = Opened {
        index,
        id: stored.id.to_string(),
        text: DocumentText::default(),
        shape: Shape {
            declared: template.iter().flat_map(|t| t.properties.iter().map(|p| p.label.clone())).collect(),
            defaults: Access {
                view: visibility,
                edit,
                level: aw_model::template::LEVEL_OPEN.into(),
            },
        },
        template,
        readable,
        base: (base_index, base_content),
        known,
        article_type: stored.article_type.clone(),
        gm,
        calendar,
    };
    opened.text = DocumentText {
        name,
        access: opened.shape.defaults.clone(),
        infobox: infobox_of(&opened, &stored.layers),
        properties: properties_of(&opened),
        blocks: blocks_of(&opened, &stored.layers),
    };
    Ok(opened)
}

/// The infobox's own access — its layer's, or before it has one what the type
/// says. `None` where it is the document's.
fn infobox_of(opened: &Opened, layers: &[crate::store::StoredLayer]) -> Option<Access> {
    let access = match opened.layer(INFOBOX_SECTION) {
        Some((_, i, content)) => Access { view: content.own.clone(), edit: content.edit.clone(), level: level_text(&layers[*i].level).to_string() },
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
fn blocks_of(opened: &Opened, layers: &[StoredLayer]) -> Vec<TemplateBlock> {
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
            .filter_map(|(section, i, content)| {
                let n = part_index(section, &key)?;
                Some((
                    n,
                    BlockPart {
                        key: section.clone(),
                        text: names::written(&content.text, &*opened.known, &opened.article_type),
                        view: content.own.clone(),
                        edit: content.edit.clone(),
                        level: level_text(&layers[*i].level).to_string(),
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

fn with_keys_in(store: &Store, world: &str) -> Result<(Store, HeldKeys, std::sync::Arc<Vec<StoredArticle>>), AppError> {
    let store = store.clone();
    let identity = store.load_identity()?;
    let held = store.view_keys(world, &identity)?;
    let articles = store.load_articles(world)?;
    Ok((store, held, articles))
}

#[cfg(feature = "gui")]
/// Opens a document as code.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn document_open(app: tauri::AppHandle, world: String, article: String) -> Answer<DocumentCode> {
    document_open_in(&store(&app)?, world, article)
}

/// `document_open` for whom the store is given — the host, acting for a player.
pub(crate) fn document_open_in(store: &Store, world: String, article: String) -> Answer<DocumentCode> {
    let store = store.clone();
    let identity = store.load_identity()?;
    code_of(&store, &identity, &world, &article)
}

#[cfg(feature = "gui")]
/// Reads code while it is typed — errors come back with their line.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn document_read(app: tauri::AppHandle, world: String, article: String, text: String, base: DocumentText) -> Answer<DocumentRead> {
    document_read_in(&store(&app)?, world, article, text, base)
}

/// `document_read` for whom the store is given — the host, acting for a player.
pub(crate) fn document_read_in(store: &Store, world: String, article: String, text: String, base: DocumentText) -> Answer<DocumentRead> {
    let (store, held, articles) = with_keys_in(store, &world)?;
    let opened = open(&store, &world, &article, &held, &articles)?;
    // The ids of the blocks come from the state before this keystroke — so a
    // section keeps its layer while its title is being retyped.
    // The visual side shows a hidden image as one, with its stand-in.
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

/// `document_save` for whom the store is given — the host, acting for a player.
pub(crate) fn document_save_in(store: &Store, world: String, article: String, text: String, base: Bases, session: String) -> Answer<DocumentCode> {
    let store = store.clone();
    let identity = store.load_identity()?;
    let before = store.inventory(&world)?.len();
    save(&store, &identity, &world, &article, &text, &base, &session)?;
    crate::network::wrote_here(&world, store.inventory(&world)?.len().saturating_sub(before));
    code_of(&store, &identity, &world, &article)
}

#[cfg(feature = "gui")]
/// Ends an editing session — leaving edit mode: what it wrote becomes a
/// version of each document it wrote in.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn session_end(app: tauri::AppHandle, world: String, session: String) -> Answer<()> {
    session_end_in(&store(&app)?, world, session)
}

/// `session_end` for whom the store is given — the host, acting for a player.
pub(crate) fn session_end_in(store: &Store, world: String, session: String) -> Answer<()> {
    store.clone().close_session(&world, &session)?;
    Ok(())
}

/// Opens a document as code — without Tauri, for the command and the tests.
pub(crate) fn code_of(store: &Store, identity: &Identity, world: &str, article: &str) -> Answer<DocumentCode> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
    code_from(store, world, &articles, opened)
}

fn code_from(store: &Store, world: &str, articles: &[StoredArticle], opened: Opened) -> Answer<DocumentCode> {
    let mut base = Bases::new();
    let layers = &articles[opened.index].layers;
    for i in std::iter::once(opened.base.0).chain(opened.readable.iter().map(|(_, i, _)| *i)) {
        if let Some((generation, heads)) = store.layer_state(world, layers[i].id)? {
            base.insert(layers[i].id.to_string(), Base { generation, heads });
        }
    }
    // The images in its image fields — not part of the code.
    let fields = stored_fields(&opened);
    let image_fields = opened
        .template
        .iter()
        .flat_map(|t| t.properties.iter())
        .filter(|f| matches!(f.kind, FieldKind::Image { .. }))
        .filter_map(|f| {
            let image = fields.get(&f.key)?.as_str()?.to_string();
            Some((if f.label.is_empty() { f.key.clone() } else { f.label.clone() }, image))
        })
        .collect();
    Ok(DocumentCode { code: document::to_code(&opened.text), document: document::joined(&opened.text), base, image_fields })
}

pub(crate) fn save(
    store: &Store,
    identity: &Identity,
    world: &str,
    article: &str,
    text: &str,
    base: &Bases,
    session: &str,
) -> Answer<()> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
    let read = document::from_code(text, &opened.text, &opened.shape);
    if let Some(e) = read.errors.first() {
        return Err(AppError(if e.line > 0 { format!("Line {}: {}", e.line, e.text) } else { e.text.clone() }));
    }
    let gm = store.is_gm(world);
    let stored = &articles[opened.index];
    let layer_id = |i: usize| stored.layers[i].id;

    // A layer's content as the editor saw it — what is written is measured
    // against this, never against now.
    let seen = |i: usize| -> Answer<Option<(u32, Vec<String>, Plain)>> {
        let Some(b) = base.get(&layer_id(i).to_string()) else { return Ok(None) };
        Ok(Some((b.generation, b.heads.clone(), store.plain_at(world, layer_id(i), b.generation, &b.heads)?)))
    };
    // Who could see and edit a layer, as the editor saw it — a change is
    // measured against that too, not against now.
    let seen_access = |i: usize| -> Answer<(LayerAccess, Level)> {
        match base.get(&layer_id(i).to_string()) {
            Some(b) => Ok(store.access_at(world, layer_id(i), b.generation)?),
            None => Err(AppError("This part was not open here.".into())),
        }
    };
    let write = |i: usize, generation: u32, heads: &[String], wanted: &Plain| -> Answer<()> {
        // Who sees or edits the part changed while this was written: what
        // was written goes on top of the new generation, not into the old.
        let (generation, heads, wanted) = match store.layer_state(world, layer_id(i))? {
            Some((now, current_heads)) if now != generation => {
                let seen = store.plain_at(world, layer_id(i), generation, heads)?;
                let current = store.plain_at(world, layer_id(i), now, &current_heads)?;
                (now, current_heads, aw_store::journal::layer::carried(&seen, wanted, &current))
            }
            _ => (generation, heads.to_vec(), wanted.clone()),
        };
        // The images it names, with their keys — those it no longer names go.
        let seen = store.plain_at(world, layer_id(i), generation, &heads)?;
        let wanted = store.with_images(world, &seen, &wanted)?;
        store.edit_layer(world, layer_id(i), generation, &heads, &wanted, identity, session).map_err(|e| {
            if e.to_string().contains("may not") {
                AppError("You may not change this part.".into())
            } else {
                AppError::from(e)
            }
        })
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
    if renamed && !may_edit(&opened.shape.defaults.edit, &held) {
        return Err(AppError("You may not rename this document.".into()));
    }
    if renamed || reorder {
        if let Some((generation, heads, mut wanted)) = seen(*base_index)? {
            if renamed {
                wanted.title = name.to_string();
            }
            if reorder {
                wanted.fields.insert(OWN_ORDER.into(), serde_json::json!(order));
            }
            write(*base_index, generation, &heads, &wanted)?;
        }
    }
    let (was, _) = seen_access(*base_index).unwrap_or((
        LayerAccess { section: String::new(), own: opened.shape.defaults.view.clone(), edit: opened.shape.defaults.edit.clone() },
        Level::Open,
    ));
    if access.view != was.own || access.edit != was.edit {
        if !gm {
            return Err(AppError("Only the GM can change who sees or edits the document.".into()));
        }
        let view: Policy = access.view.parse().map_err(AppError::from)?;
        store.rekey(
            world,
            layer_id(*base_index),
            &view,
            LayerAccess { section: base_content.section.clone(), own: access.view.clone(), edit: access.edit.clone() },
            if view.to_string() == "gm" { Level::GmOnly } else { Level::Open },
            session,
        )?;
    }

    // --- The properties --------------------------------------------------
    // Each value goes where it lies already; a new one where its field says:
    // with access of its own into a layer of its own, otherwise into the
    // common one.
    let fields = fields_from(&opened, &read.document.properties)?;
    let template_fields: Vec<&aw_model::template::TemplateField> =
        opened.template.iter().flat_map(|t| t.properties.iter()).collect();
    let mut wanted_fields: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>> = opened
        .readable
        .iter()
        .filter(|(section, i, _)| is_infobox(section) && base.contains_key(&layer_id(*i).to_string()))
        .map(|(section, _, _)| (section.clone(), serde_json::Map::new()))
        .collect();
    if read.document.infobox.is_some() {
        wanted_fields.entry(INFOBOX_SECTION.to_string()).or_default();
    }
    for (key, value) in fields.as_object().into_iter().flatten() {
        let holder = opened.readable.iter().find(|(section, _, c)| {
            is_infobox(section) && c.fields.as_object().is_some_and(|m| m.contains_key(key))
        });
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
        let level = seen_access(*i).map(|(_, l)| l).unwrap_or(stored.layers[*i].level);
        let (view, edit) = seen_access(*i).map(|(a, _)| (a.own, a.edit)).unwrap_or((content.own.clone(), content.edit.clone()));
        // Without its own, it follows the document — at the level it has.
        let wanted_level = if read.document.infobox.is_some() { level_from(&infobox.level) } else { level };
        if infobox.view != view || infobox.edit != edit || wanted_level != level {
            if !gm {
                return Err(AppError("Only the GM can change who sees or edits the infobox.".into()));
            }
            store.ensure_attributes(world, &infobox.view.parse().map_err(AppError::from)?)?;
            store.rekey(
                world,
                layer_id(*i),
                &infobox.view.parse().map_err(AppError::from)?,
                LayerAccess { section: INFOBOX_SECTION.into(), own: infobox.view.clone(), edit: infobox.edit.clone() },
                wanted_level,
                session,
            )?;
        }
    }
    for (section, values) in wanted_fields {
        match opened.layer(&section) {
            Some((_, i, _)) => {
                let Some((generation, heads, mut wanted)) = seen(*i)? else { continue };
                wanted.fields = values.into_iter().collect();
                write(*i, generation, &heads, &wanted)?;
            }
            None => {
                // An infobox given access of its own keeps it, even before it holds anything.
                let kept = section == INFOBOX_SECTION && read.document.infobox.is_some() && gm;
                if values.is_empty() && !kept {
                    continue;
                }
                if !gm {
                    return Err(AppError("Only the GM can give a document properties.".into()));
                }
                let own = template_fields.iter().find(|f| field_section(&f.key) == section).and_then(|f| f.access.clone());
                let (view, edit, level) = match own {
                    Some(a) => (a.view, a.edit, level_from(&a.level)),
                    // Without access of its own a property is the infobox's.
                    None => (infobox.view.clone(), infobox.edit.clone(), level_from(&infobox.level)),
                };
                let (view, edit) = (crate::own_view(&view, &stored.id.to_string(), &opened.article_type), crate::own_view(&edit, &stored.id.to_string(), &opened.article_type));
                store.ensure_attributes(world, &view.parse().map_err(AppError::from)?)?;
                store.add_part(
                    world,
                    stored.id,
                    NewPart {
                        is_base: false,
                        level,
                        view: view.parse().map_err(AppError::from)?,
                        access: LayerAccess { section, own: view, edit },
                        plain: Plain { fields: values.into_iter().collect(), ..Plain::default() },
                    },
                    None,
                    session,
                )?;
            }
        }
    }

    // What changed in relations goes into the documents they name too.
    mirror(store, identity, world, &opened, &fields, session);

    // --- The sections ----------------------------------------------------
    let mut kept = std::collections::BTreeSet::new();
    for block in &read.document.blocks {
        for part in &block.parts {
            kept.insert(part.key.clone());
            // `self`: whoever is assigned to this document.
            let mut part = part.clone();
            part.view = crate::own_view(&part.view, &stored.id.to_string(), &opened.article_type);
            part.edit = crate::own_view(&part.edit, &stored.id.to_string(), &opened.article_type);
            let part = &part;
            // Names as written, ids as stored.
            let text = names::stored(&part.text, &*opened.known, &opened.article_type);
            let wanted_level = level_from(&part.level);
            match opened.layer(&part.key) {
                Some((_, i, content)) => {
                    // The title of a block belongs to its type, not to the
                    // layer — it counts only where the block is the layer's
                    // own.
                    let own_title = opened.template.as_ref().is_none_or(|t| !t.blocks.iter().any(|b| b.key == block.key));
                    if let Some((generation, heads, mut wanted)) = seen(*i)? {
                        wanted.text = text;
                        if own_title {
                            wanted.title = block.title.clone();
                            // How it looks, on its first part.
                            if part.key == block.key {
                                OwnShape::from_block(block, &*opened.known).put(&mut wanted.fields);
                            }
                        }
                        write(*i, generation, &heads, &wanted)?;
                    }
                    let (was, level) = seen_access(*i).unwrap_or((
                        LayerAccess { section: part.key.clone(), own: content.own.clone(), edit: content.edit.clone() },
                        stored.layers[*i].level,
                    ));
                    let access_changed = part.view != was.own || part.edit != was.edit || wanted_level != level;
                    if access_changed {
                        if !gm {
                            return Err(AppError("Only the GM can change who sees or edits a part.".into()));
                        }
                        store.ensure_attributes(world, &part.view.parse().map_err(AppError::from)?)?;
                        store.rekey(
                            world,
                            layer_id(*i),
                            &part.view.parse().map_err(AppError::from)?,
                            LayerAccess { section: part.key.clone(), own: part.view.clone(), edit: part.edit.clone() },
                            wanted_level,
                            session,
                        )?;
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
                        let id = stored.id.to_string();
                        let same = |a: &str, b: &str| a == b || a.parse::<Policy>().ok().is_some_and(|a| b.parse::<Policy>().ok() == Some(a));
                        !(same(&crate::own_view(&t.view, &id, &opened.article_type), &part.view)
                            && same(&crate::own_view(&t.edit, &id, &opened.article_type), &part.edit)
                            && t.level == part.level)
                    });
                    if part.text.trim().is_empty() && !named_own && !own_access {
                        continue;
                    }
                    if !gm {
                        return Err(AppError("Only the GM can add a part with access of its own.".into()));
                    }
                    store.ensure_attributes(world, &part.view.parse().map_err(AppError::from)?)?;
                    store.add_part(
                        world,
                        stored.id,
                        NewPart {
                            is_base: false,
                            level: wanted_level,
                            view: part.view.parse().map_err(AppError::from)?,
                            access: LayerAccess { section: part.key.clone(), own: part.view.clone(), edit: part.edit.clone() },
                            plain: Plain {
                                title: block.title.clone(),
                                text,
                                fields: {
                                    let mut fields = std::collections::BTreeMap::new();
                                    if !typed(&block.key) && part.key == block.key {
                                        OwnShape::from_block(block, &*opened.known).put(&mut fields);
                                    }
                                    fields
                                },
                                ..Plain::default()
                            },
                        },
                        None,
                        session,
                    )?;
                }
            }
        }
    }

    // What the editor saw and did not keep is removed — what it did not see,
    // or what came meanwhile, stays untouched.
    let removed: Vec<LayerId> = opened
        .readable
        .iter()
        .filter(|(s, i, _)| !is_infobox(s) && !kept.contains(s) && base.contains_key(&layer_id(*i).to_string()))
        .map(|(_, i, _)| layer_id(*i))
        .collect();
    if !removed.is_empty() && !gm {
        return Err(AppError("Only the GM can remove a part.".into()));
    }
    for layer in removed {
        store.remove(world, stored.id, Some(layer), session)?;
    }

    if let Some(owner) = owner {
        name_owner(store, world, &owner, name)?;
    }
    Ok(())
}

// --- Relations, on both sides ----------------------------------------------

/// Writes what changed in a document's relations into the documents they
/// name: each side holds the whole entry, its own way round.
///
/// What cannot be written there — a document this reader may not change, a
/// type without such a property — stays one-sided. That is no error: the
/// page shows it.
fn mirror(store: &Store, identity: &Identity, world: &str, opened: &Opened, fields: &serde_json::Value, session: &str) {
    let Some(template) = &opened.template else { return };
    let before = stored_fields(opened);
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
            let mirrored: Vec<Entry> = is.iter().map(|e| e.mirrored(&opened.id)).collect();
            let _ = set_relations(store, identity, world, other, field, &opened.id, &mirrored, session);
        }
    }
}

/// Sets what a document's relation property says about one other document
/// — every entry naming `about` is replaced by `wanted`, in place.
///
/// The property is found by key, or else by name: a character and a deity
/// may both have “Relations”. Returns whether the document has one at all.
fn set_relations(
    store: &Store,
    identity: &Identity,
    world: &str,
    article: &str,
    field: &aw_model::template::TemplateField,
    about: &str,
    wanted: &[Entry],
    session: &str,
) -> Answer<bool> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
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
    let stored = &articles[opened.index];
    match opened.layer(&section) {
        Some((_, i, _)) => {
            let layer = stored.layers[*i].id;
            let Some((generation, heads)) = store.layer_state(world, layer)? else { return Ok(true) };
            let mut plain = store.plain_at(world, layer, generation, &heads)?;
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
            store.edit_layer(world, layer, generation, &heads, &plain, identity, session).map_err(|e| {
                if e.to_string().contains("may not") {
                    AppError("You may not change this part.".into())
                } else {
                    AppError::from(e)
                }
            })?;
        }
        None if wanted.is_empty() => {}
        None => {
            if !opened.gm {
                return Err(AppError("Only the GM can give a document properties.".into()));
            }
            let (view, edit, level) = match &target.access {
                Some(a) => (a.view.clone(), a.edit.clone(), level_from(&a.level)),
                None => (opened.shape.defaults.view.clone(), opened.shape.defaults.edit.clone(), Level::Open),
            };
            let mut fields = std::collections::BTreeMap::new();
            fields.insert(target.key.clone(), relation::value(&replaced(None)));
            store.add_part(
                world,
                stored.id,
                NewPart {
                    is_base: false,
                    level,
                    view: view.parse().map_err(AppError::from)?,
                    access: LayerAccess { section, own: view, edit },
                    plain: Plain { fields, ..Plain::default() },
                },
                None,
                session,
            )?;
        }
    }
    Ok(true)
}

#[cfg(feature = "gui")]
/// Puts an image into an image field of a document — a portrait, a coat of
/// arms — or takes it out (`None`). The image's key goes into the part
/// holding the field: whoever may read that part sees the image.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn image_field_set(app: tauri::AppHandle, world: String, article: String, field: String, image: Option<String>) -> Answer<()> {
    image_field_set_in(&store(&app)?, world, article, field, image)
}

/// `image_field_set` for whom the store is given — the host, acting for a player.
pub(crate) fn image_field_set_in(store: &Store, world: String, article: String, field: String, image: Option<String>) -> Answer<()> {
    let store = store.clone();
    let identity = store.load_identity()?;
    let session = format!("image-{}", aw_model::template_text::new_id());
    set_image_field(&store, &identity, &world, &article, &field, image.as_deref(), &session)?;
    store.close_session(&world, &session)?;
    Ok(())
}

pub(crate) fn set_image_field(
    store: &Store,
    identity: &Identity,
    world: &str,
    article: &str,
    field: &str,
    image: Option<&str>,
    session: &str,
) -> Answer<()> {
    let value = image.map(|i| serde_json::Value::String(i.to_string()));
    set_field(store, identity, world, article, field, value, session, |k| matches!(k, FieldKind::Image { .. }))
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

/// `references_set` for whom the store is given — the host, acting for a player.
pub(crate) fn references_set_in(store: &Store, world: String, article: String, field: String, ids: Vec<String>) -> Answer<()> {
    let store = store.clone();
    let identity = store.load_identity()?;
    for id in &ids {
        id.parse::<ArticleId>().map_err(AppError::from)?;
    }
    let session = format!("references-{}", aw_model::template_text::new_id());
    // A pick that stays keeps its number — the levels of a class stay when
    // another class is added.
    let held = store.view_keys(&world, &identity)?;
    let opened = open(&store, &world, &article, &held, &store.load_articles(&world)?)?;
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
    set_field(&store, &identity, &world, &article, &field, value, &session, |k| {
        matches!(k, FieldKind::Reference { multiple: true, .. })
    })?;
    store.close_session(&world, &session)?;
    Ok(())
}

const NO_SUCH_FIELD: &str = "This document has no such property.";

/// Writes one property's value into the part holding it — a new part for
/// it if the document has none yet — or takes it out (`None`).
#[allow(clippy::too_many_arguments)]
fn set_field(
    store: &Store,
    identity: &Identity,
    world: &str,
    article: &str,
    field: &str,
    value: Option<serde_json::Value>,
    session: &str,
    kind: impl Fn(&FieldKind) -> bool,
) -> Answer<()> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
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
    let stored = &articles[opened.index];
    match opened.layer(&section) {
        Some((_, i, _)) => {
            let layer = stored.layers[*i].id;
            let Some((generation, heads)) = store.layer_state(world, layer)? else { return Ok(()) };
            let seen = store.plain_at(world, layer, generation, &heads)?;
            let mut plain = seen.clone();
            match &value {
                Some(v) => plain.fields.insert(target.key.clone(), v.clone()),
                None => plain.fields.remove(&target.key),
            };
            let plain = store.with_images(world, &seen, &plain)?;
            store.edit_layer(world, layer, generation, &heads, &plain, identity, session).map_err(|e| {
                if e.to_string().contains("may not") {
                    AppError("You may not change this part.".into())
                } else {
                    AppError::from(e)
                }
            })?;
        }
        None if value.is_none() => {}
        None => {
            if !opened.gm {
                return Err(AppError("Only the GM can give a document properties.".into()));
            }
            let (view, edit, level) = match &target.access {
                Some(a) => (a.view.clone(), a.edit.clone(), level_from(&a.level)),
                None => (opened.shape.defaults.view.clone(), opened.shape.defaults.edit.clone(), Level::Open),
            };
            let mut fields = std::collections::BTreeMap::new();
            fields.insert(target.key.clone(), value.clone().expect("a value"));
            let plain = Plain { fields, ..Plain::default() };
            store.add_part(
                world,
                stored.id,
                NewPart {
                    is_base: false,
                    level,
                    view: view.parse().map_err(AppError::from)?,
                    access: LayerAccess { section, own: view, edit },
                    plain,
                },
                None,
                session,
            )?;
        }
    }
    Ok(())
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
/// joined to their sons are one step in each document's history, and the
/// world is read again once, not once for each.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn relations_set(app: tauri::AppHandle, world: String, changes: Vec<RelationChange>) -> Answer<()> {
    let store = store(&app)?;
    let identity = store.load_identity()?;
    let session = format!("relations-{}", aw_model::template_text::new_id());
    for c in changes {
        set_relation(&store, &identity, &world, &c.article, &c.property, &c.with, c.roles, &session)?;
    }
    // One step, and done: a version of each document it touched.
    store.close_session(&world, &session)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn set_relation(
    store: &Store,
    identity: &Identity,
    world: &str,
    article: &str,
    property: &str,
    with: &str,
    roles: Option<(String, String)>,
    session: &str,
) -> Answer<()> {
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
    let wanted: Vec<Entry> = roles
        .into_iter()
        .map(|(role, own_role)| Entry { with: with.to_string(), role, own_role })
        .collect();
    if !set_relations(store, identity, world, article, &field, with, &wanted, session)? {
        return Err(AppError(format!("This document has no “{}”.", field.label)));
    }
    let mirrored: Vec<Entry> = wanted.iter().map(|e| e.mirrored(article)).collect();
    let _ = set_relations(store, identity, world, with, &field, article, &mirrored, session);
    Ok(())
}

/// Before a document goes: the documents it names forget it — as far as this
/// reader may change them. Otherwise they would show a relation to nobody.
pub(crate) fn forget_relations(store: &Store, identity: &Identity, world: &str, article: &str) -> Answer<()> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
    let Some(template) = &opened.template else { return Ok(()) };
    let fields = stored_fields(&opened);
    let session = format!("relations-{}", aw_model::template_text::new_id());
    for field in template.properties.iter().filter(|f| matches!(f.kind, FieldKind::Relation { .. })) {
        for entry in fields.get(&field.key).map(relation::entries).unwrap_or_default() {
            let _ = set_relations(store, identity, world, &entry.with, field, article, &[], &session);
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
    Ok(store
        .load_templates(world)?
        .into_iter()
        .find(|t| t.page == article)
        .map(|t| Owner::Type(t.id)))
}

/// No two types, and no two worlds, share a name — in every list they could
/// not be told apart.
fn check_owner_name(store: &Store, world: &str, owner: &Owner, name: &str) -> Answer<()> {
    let taken = match owner {
        Owner::World => store
            .list_worlds()?
            .iter()
            .any(|m| m.public.world_id.as_str() != world && m.public.name.trim().eq_ignore_ascii_case(name)),
        Owner::Type(id) => store
            .load_templates(world)?
            .iter()
            .any(|t| &t.id != id && t.name.trim().eq_ignore_ascii_case(name)),
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
            let mut token = store.load_token(world)?;
            token.rename(name);
            store.save_token(&token)?;
            let mut meta = store.load_world_meta(world)?;
            meta.public.name = name.to_string();
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
pub(crate) fn rename_document(store: &Store, identity: &Identity, world: &str, article: &str, name: &str) -> Answer<()> {
    let code = code_of(store, identity, world, article)?;
    let mut doc = code.document;
    doc.name = name.to_string();
    save(store, identity, world, article, &document::to_code(&doc), &code.base, "")
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
pub(crate) fn caret_in(store: &Store, identity: &Identity, world: &str, article: &str, caret: Caret) -> Answer<Option<(LayerId, usize, usize)>> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
    let (a, b) = (caret.start.min(caret.end), caret.start.max(caret.end));
    let found = match (caret.part, caret.code) {
        (Some(part), _) => opened
            .text
            .blocks
            .iter()
            .flat_map(|b| b.parts.iter())
            .find(|p| p.key == part)
            .map(|p| (part, a, b, p.text.clone())),
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
    Ok(Some((articles[opened.index].layers[*i].id, stored(start), stored(end))))
}

/// Where a place in a layer's stored text stands for this reader.
pub(crate) fn caret_at(store: &Store, identity: &Identity, world: &str, article: &str, layer: LayerId, start: usize, end: usize) -> Answer<Option<CaretAt>> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let opened = open(store, world, article, &held, &articles)?;
    let layers = &articles[opened.index].layers;
    let Some((section, _, content)) = opened.readable.iter().find(|(s, i, _)| layers[*i].id == layer && !is_infobox(s)) else {
        return Ok(None);
    };
    let written = |at: usize| {
        utf16_len(&names::written(&content.text[..byte_at(&content.text, at)], &*opened.known, &opened.article_type))
    };
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
    Ok(Some(CaretAt {
        part: section.clone(),
        part_start,
        part_end,
        code_start: utf16_len(&code[..i]),
        code_end: utf16_len(&code[..j]) - 1,
    }))
}

// --- Versions -------------------------------------------------------------

/// One version of a document: an editing session.
#[derive(Serialize)]
pub struct VersionInfo {
    pub session: String,
    pub author: String,
    pub author_id: String,
    /// Milliseconds since 1970, like every time of the journal.
    pub from: u64,
    pub to: u64,
    /// When it came to this device — long after `to` means: written
    /// offline, merged only now.
    pub arrived: u64,
    /// The parts it changed, by what the reader calls them.
    pub parts: Vec<String>,
    pub structure: bool,
    /// Written while the part's access changed — kept in the history, not
    /// in the text.
    pub stale: bool,
    /// Versions by others, written at the same time on the same lines.
    pub overlaps: Vec<String>,
    /// Whether it is in the text, waits for the GM, or was turned down.
    pub review: Review,
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
fn part_names(opened: &Opened, layers: &[StoredLayer]) -> std::collections::BTreeMap<LayerId, String> {
    let mut out = std::collections::BTreeMap::new();
    out.insert(layers[opened.base.0].id, "Name".to_string());
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
        out.insert(layers[*i].id, title);
    }
    out
}

#[cfg(feature = "gui")]
/// The versions of a document, newest first — as far as the reader can see
/// what they changed.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn article_versions(app: tauri::AppHandle, world: String, article: String) -> Answer<Vec<VersionInfo>> {
    article_versions_in(&store(&app)?, world, article)
}

/// `article_versions` for whom the store is given — the host, acting for a player.
pub(crate) fn article_versions_in(store: &Store, world: String, article: String) -> Answer<Vec<VersionInfo>> {
    let store = store.clone();
    let identity = store.load_identity()?;
    let held = store.view_keys(&world, &identity)?;
    let articles = store.load_articles(&world)?;
    let opened = open(&store, &world, &article, &held, &articles)?;
    let names = part_names(&opened, &articles[opened.index].layers);
    let id = articles[opened.index].id;
    let sessions = store.sessions(&world, id)?;
    // Written by a host in a member's name: the session names them — where
    // the GM or a server they named vouches for it. Their name as their grant has it.
    let written_for: Vec<Option<(String, String)>> = store.with_journal(&world, |journal| {
        Ok(sessions
            .iter()
            .map(|s| {
                let member = journal.written_for(s.author.id.as_str(), &s.session);
                (member != s.author.id.as_str())
                    .then(|| journal.grant(member).map(|(identity, _)| (identity.name.clone(), member.to_string())))
                    .flatten()
            })
            .collect())
    })?;
    Ok(sessions
        .into_iter()
        .zip(written_for)
        .filter_map(|(s, member)| {
            let parts: Vec<String> = s.layers.iter().filter_map(|l| names.get(l).cloned()).collect();
            let (author, author_id) = member.unwrap_or((s.author.name.clone(), s.author.id.to_string()));
            // A version of parts this reader cannot see is none of theirs.
            (!parts.is_empty() || s.layers.is_empty()).then(|| VersionInfo {
                session: s.session,
                author,
                author_id,
                from: s.from,
                to: s.to,
                arrived: s.arrived,
                parts,
                structure: s.structure,
                stale: s.stale,
                overlaps: s.overlaps,
                review: s.review,
                removable: s.removable && store.is_gm(&world),
            })
        })
        .collect())
}

#[cfg(feature = "gui")]
/// What a version changed, part by part — as written, with names.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_compare(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<Vec<PartChange>> {
    version_compare_in(&store(&app)?, world, article, session)
}

/// `version_compare` for whom the store is given — the host, acting for a player.
pub(crate) fn version_compare_in(store: &Store, world: String, article: String, session: String) -> Answer<Vec<PartChange>> {
    let store = store.clone();
    let identity = store.load_identity()?;
    let held = store.view_keys(&world, &identity)?;
    let articles = store.load_articles(&world)?;
    let opened = open(&store, &world, &article, &held, &articles)?;
    let names = part_names(&opened, &articles[opened.index].layers);
    let id = articles[opened.index].id;
    let Some(version) = store.sessions(&world, id)?.into_iter().find(|s| s.session == session) else {
        return Err(AppError("There is no such version.".into()));
    };
    let written = |plain: &Plain| {
        let mut out = names::written(&plain.text, &*opened.known, &opened.article_type);
        if !plain.title.is_empty() {
            out = if out.is_empty() { plain.title.clone() } else { format!("{}\n\n{out}", plain.title) };
        }
        for (key, value) in &plain.fields {
            out.push_str(&format!("\n{key}: {}", plain_value(value)));
        }
        out
    };
    let mut out = Vec::new();
    for layer in version.layers {
        let Some(part) = names.get(&layer) else { continue };
        let (before, after) = store.compare(&world, layer, &session)?;
        let may_revert = opened
            .readable
            .iter()
            .find(|(_, i, _)| articles[opened.index].layers[*i].id == layer)
            .map(|(_, _, c)| may_edit(&c.edit, &held))
            .unwrap_or_else(|| may_edit(&opened.shape.defaults.edit, &held));
        out.push(PartChange { layer: layer.to_string(), part: part.clone(), before: written(&before), after: written(&after), may_revert });
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

/// A document as it stood with a version.
pub(crate) fn document_at(store: &Store, identity: &Identity, world: &str, article: &str, session: &str) -> Answer<VersionDocument> {
    let held = store.view_keys(world, identity)?;
    // A document removed since too: its versions stay.
    let id: ArticleId = article.parse().map_err(|_| AppError("There is no such article.".into()))?;
    let version = store
        .sessions(world, id)?
        .into_iter()
        .find(|s| s.session == session)
        .ok_or_else(|| AppError("There is no such version.".into()))?;
    let then = store.load_articles_at(world, id, session, version.to)?;
    let view = build_view(&then, &held)
        .into_iter()
        .find(|a| a.id == article)
        .ok_or_else(|| AppError("This document did not stand like this for you then.".into()))?;
    let opened = open(store, world, article, &held, &then)?;
    Ok(VersionDocument { view, code: document::to_code(&opened.text), document: opened.text })
}

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_document(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<VersionDocument> {
    version_document_in(&store(&app)?, world, article, session)
}

/// Changes of an editing session that reach no one — see
/// `Journal::conflicts` — as the options list them.
#[derive(Serialize)]
pub struct ConflictView {
    pub article: String,
    /// The document's name as it stood then; empty where it cannot be read.
    pub name: String,
    pub session: String,
    pub author: String,
    pub mine: bool,
    /// Milliseconds since 1970.
    pub at: u64,
    /// `removed` or `waiting`.
    pub why: aw_store::journal::Stuck,
}

#[cfg(feature = "gui")]
/// Changes that reach no one: the GM sees all of them, everyone else their own.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn world_conflicts(app: tauri::AppHandle, world: String) -> Answer<Vec<ConflictView>> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    Ok(store
        .conflicts(&world)?
        .into_iter()
        .map(|(c, _)| {
            let article = c.article.to_string();
            let name = match c.why {
                aw_store::journal::Stuck::Removed => document_at(&store, &me, &world, &article, &c.session).map(|d| d.view.name).unwrap_or_default(),
                aw_store::journal::Stuck::Waiting => String::new(),
            };
            ConflictView { article, name, session: c.session, author: c.author.name.clone(), mine: c.author.id == *me.id(), at: c.at, why: c.why }
        })
        .collect())
}

#[cfg(feature = "gui")]
/// Lets changes that reach no one go, for good — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn conflict_dismiss(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<()> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM lets changes go.".into()));
    }
    let records: Vec<String> = store
        .conflicts(&world)?
        .into_iter()
        .filter(|(c, _)| c.article.to_string() == article && c.session == session)
        .flat_map(|(_, ids)| ids)
        .collect();
    if !records.is_empty() {
        store.control(&world, aw_store::journal::Item::Deleted { records }, "")?;
    }
    Ok(())
}

#[cfg(feature = "gui")]
/// Saves what was written to a document removed meanwhile as a new one of
/// the same type — the GM's at once, a player's as a proposal for the GM.
/// Its id.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn conflict_rescue(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<String> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    conflict_rescue_in(&store, &me, &world, &article, &session)
}

pub(crate) fn conflict_rescue_in(store: &Store, me: &Identity, world: &str, article: &str, session: &str) -> Answer<String> {
    let then = document_at(store, me, world, article, session)?;
    let view = then.view;
    let layers: Vec<crate::NewLayer> = view
        .layers
        .iter()
        .map(|l| crate::NewLayer { title: l.title.clone(), text: l.text.clone(), policy: l.policy.clone(), level: l.level.clone(), section: l.section.clone(), edit: l.edit.clone(), fields: l.fields.clone(), parent: None })
        .collect();
    let slug = aw_model::template_text::new_id();
    if store.is_gm(world) {
        let id = crate::create_document(store, world, view.article_type, slug, view.name, view.visibility, layers)?;
        let records: Vec<String> = store.conflicts(world)?.into_iter().filter(|(c, _)| c.article.to_string() == article && c.session == session).flat_map(|(_, ids)| ids).collect();
        if !records.is_empty() {
            store.control(world, aw_store::journal::Item::Deleted { records }, "")?;
        }
        Ok(id)
    } else {
        crate::propose_document(store, me, world, None, view.article_type, slug, view.name, layers)
    }
}

/// `version_document` for whom the store is given — the host, acting for a player.
pub(crate) fn version_document_in(store: &Store, world: String, article: String, session: String) -> Answer<VersionDocument> {
    let store = store.clone();
    let identity = store.load_identity()?;
    document_at(&store, &identity, &world, &article, &session)
}

#[cfg(feature = "gui")]
/// Takes a version's lines into the text that a change of access left
/// behind — for when their author may not write there any more and the game
/// master decides they belong in.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_carry(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<usize> {
    let store = store(&app)?;
    let identity = store.load_identity()?;
    let held = store.view_keys(&world, &identity)?;
    let articles = store.load_articles(&world)?;
    let opened = open(&store, &world, &article, &held, &articles)?;
    let id = articles[opened.index].id;
    let Some(version) = store.sessions(&world, id)?.into_iter().find(|s| s.session == session) else {
        return Err(AppError("There is no such version.".into()));
    };
    let mut done = 0;
    for layer in version.layers {
        if store.carry(&world, layer, &session, &identity).is_ok() {
            done += 1;
        }
    }
    if done == 0 {
        return Err(AppError("You may not write where this version belongs.".into()));
    }
    Ok(done)
}

#[cfg(feature = "gui")]
/// The GM's word on a waiting version: taken into the text, or turned down.
/// The versions it builds on, or that build on it, go with it.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_verdict(app: tauri::AppHandle, world: String, article: String, session: String, accepted: bool) -> Answer<()> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM takes versions in or turns them down.".into()));
    }
    let id = store
        .load_articles(&world)?
        .iter()
        .find(|a| a.id.to_string() == article)
        .map(|a| a.id)
        .ok_or_else(|| AppError("There is no such article.".into()))?;
    store.verdict(&world, id, &session, accepted)?;
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

#[cfg(feature = "gui")]
/// The versions waiting: for the GM, everything players wrote; for a
/// player, their own — newest first.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn waiting_versions(app: tauri::AppHandle, world: String) -> Answer<Vec<WaitingVersion>> {
    let store = store(&app)?;
    let identity = store.load_identity()?;
    let held = store.view_keys(&world, &identity)?;
    let views = build_view(&store.load_articles(&world)?, &held);
    let mut out = Vec::new();
    for (article, session) in store.waiting(&world)? {
        let Some(view) = views.iter().find(|v| v.id == article.to_string()) else { continue };
        let Some(version) = store.sessions(&world, article)?.into_iter().find(|s| s.session == session) else { continue };
        out.push(WaitingVersion { article: view.id.clone(), name: view.name.clone(), session, author: version.author.name, to: version.to });
    }
    out.sort_by_key(|v| std::cmp::Reverse(v.to));
    Ok(out)
}

/// What is marked on one side of a version and now: in the code, in each
/// part's text as written, and in each layer's text as the page reads it.
#[derive(Serialize, Default)]
pub struct SideMarks {
    pub code: Vec<Range>,
    pub parts: std::collections::BTreeMap<String, Vec<Range>>,
    pub layers: std::collections::BTreeMap<String, Vec<Range>>,
}

/// Where a version and now differ — by the characters themselves where the
/// journal can tell, otherwise as a reader would compare the two texts.
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
    pub review: Review,
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
fn side_marks(opened: &Opened, layers: &[StoredLayer], by_layer: &std::collections::BTreeMap<LayerId, Vec<Range>>, other: &DocumentText) -> SideMarks {
    let mut out = SideMarks::default();
    for (section, i, content) in opened.readable.iter().filter(|(s, _, _)| !is_infobox(s)) {
        let Some(ranges) = by_layer.get(&layers[*i].id).filter(|r| !r.is_empty()) else { continue };
        out.layers.insert(layers[*i].id.to_string(), ranges.clone());
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

/// Where a version and now differ, for this reader.
pub(crate) fn marks_of(store: &Store, identity: &Identity, world: &str, article: &str, session: &str) -> Answer<VersionMarks> {
    let held = store.view_keys(world, identity)?;
    let articles = store.load_articles(world)?;
    let now = open(store, world, article, &held, &articles)?;
    let id = articles[now.index].id;
    let version = store
        .sessions(world, id)?
        .into_iter()
        .find(|s| s.session == session)
        .ok_or_else(|| AppError("There is no such version.".into()))?;
    let then_articles = store.load_articles_at(world, id, session, version.to)?;
    let then = open(store, world, article, &held, &then_articles)?;
    let texts = |o: &Opened, all: &[StoredArticle]| -> std::collections::BTreeMap<LayerId, String> {
        o.readable.iter().filter(|(s, _, _)| !is_infobox(s)).map(|(_, i, c)| (all[o.index].layers[*i].id, c.text.clone())).collect()
    };
    let (then_texts, now_texts) = (texts(&then, &then_articles), texts(&now, &articles));
    let (mut gone, mut added) = (std::collections::BTreeMap::new(), std::collections::BTreeMap::new());
    let layers: std::collections::BTreeSet<LayerId> = then_texts.keys().chain(now_texts.keys()).copied().collect();
    for layer in layers {
        let (g, a) = match (then_texts.get(&layer), now_texts.get(&layer)) {
            (Some(a), Some(b)) => match store.layer_marks(world, layer, session, version.to)? {
                Some(exact) => exact,
                None => diff::ranges(a, b),
            },
            (Some(a), None) => (whole(a), Vec::new()),
            (None, Some(b)) => (Vec::new(), whole(b)),
            (None, None) => continue,
        };
        gone.insert(layer, g);
        added.insert(layer, a);
    }
    Ok(VersionMarks {
        then: side_marks(&then, &then_articles[then.index].layers, &gone, &now.text),
        now: side_marks(&now, &articles[now.index].layers, &added, &then.text),
        now_code: document::to_code(&now.text),
        review: store.review(world, session)?,
    })
}

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_marks(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<VersionMarks> {
    version_marks_in(&store(&app)?, world, article, session)
}

/// `version_marks` for whom the store is given — the host, acting for a player.
pub(crate) fn version_marks_in(store: &Store, world: String, article: String, session: String) -> Answer<VersionMarks> {
    let store = store.clone();
    let identity = store.load_identity()?;
    marks_of(&store, &identity, &world, &article, &session)
}

fn plain_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => number_text(n),
        other => other.to_string(),
    }
}

#[cfg(feature = "gui")]
/// Deletes a version for good — the GM's. The newest stays.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_delete(app: tauri::AppHandle, world: String, article: String, session: String) -> Answer<()> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM deletes versions.".into()));
    }
    let id: ArticleId = article.parse().map_err(AppError::from)?;
    store.delete_version(&world, id, &session)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Takes a version back — in every part the reader may edit. What others
/// wrote since stays.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn version_revert(
    app: tauri::AppHandle,
    world: String,
    article: String,
    session: String,
    into: String,
) -> Answer<usize> {
    let changes = version_compare(app.clone(), world.clone(), article, session.clone())?;
    let store = store(&app)?;
    let identity = store.load_identity()?;
    let mut done = 0;
    for change in changes.into_iter().filter(|c| c.may_revert) {
        let layer: LayerId = change.layer.parse().map_err(AppError::from)?;
        store.revert(&world, layer, &session, &identity, &into)?;
        done += 1;
    }
    if done == 0 {
        return Err(AppError("You may not change any part of this version.".into()));
    }
    Ok(done)
}

/// Saves on top of the state as it is now — the way an editor opened just
/// before would.
#[cfg(test)]
pub(crate) fn save_now(store: &Store, identity: &Identity, world: &str, article: &str, text: &str) -> Answer<()> {
    let base = code_of(store, identity, world, article)?.base;
    save(store, identity, world, article, text, &base, "test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::template::{Infobox, TemplateField};
    use store::{Member, Role};

    struct World {
        dir: PathBuf,
        store: Store,
        gm: Identity,
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
        World { id: meta.public.world_id.to_string(), home: meta.home.clone(), dir, store, gm }
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

    fn part(key: &str, view: &str, edit: &str, level: &str) -> BlockPart {
        BlockPart { key: key.into(), text: String::new(), view: view.into(), edit: edit.into(), level: level.into() }
    }

    fn layer(section: &str, text: &str, view: &str, edit: &str, level: &str) -> NewLayer {
        NewLayer {
            title: String::new(),
            text: text.into(),
            policy: view.into(),
            level: level.into(),
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
    fn texts(w: &World, held: &HeldKeys, article: &str) -> Vec<(String, String)> {
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), held);
        let a = view.into_iter().find(|a| a.id == article).expect("readable");
        a.layers.into_iter().map(|l| (l.section, l.text)).collect()
    }

    /// A document's code around its body — the skeleton every document has.
    fn skeleton(name: &str, body: &str) -> String {
        format!("<article view=all edit=gm>\n<name={name}>\n<body>\n{body}</body>\n</article>\n")
    }

    #[test]
    fn the_world_page_is_built_like_every_document() {
        let w = world("page");
        let code = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(code.code, skeleton("Eldermere", "<block>\n</block>\n"), "a fresh page is one empty block");

        let body = "<block>\n# Welcome\n\nA world of mist.\n\n<list type=Place style=cards>\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", body)).unwrap();
        let again = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(again.code, skeleton("Eldermere", body));
    }

    #[test]
    fn a_section_keeps_its_create_button() {
        let w = world("new");
        let mut event = template("event", Vec::new(), Vec::new());
        event.name = "Event".into();
        crate::save_template(&w.store, &w.gm, &w.id, event).unwrap();
        let body = "<block title=Timeline new=\"Event, Session log\">\nWhat happened.\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", body)).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", body));
        assert_eq!(back.document.blocks[0].new, vec!["Event".to_string(), "Session log".to_string()]);
        // Stored by id: a renamed type is named anew.
        let mut event = w.store.load_templates(&w.id).unwrap().into_iter().find(|t| t.name == "Event").unwrap();
        event.name = "Happening".into();
        crate::save_template(&w.store, &w.gm, &w.id, event).unwrap();
        assert!(code_of(&w.store, &w.gm, &w.id, &w.home).unwrap().code.contains("new=\"Happening, Session log\""));
        // Taken off again.
        let plain = "<block title=Timeline>\nWhat happened.\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", plain)).unwrap();
        assert!(code_of(&w.store, &w.gm, &w.id, &w.home).unwrap().document.blocks[0].new.is_empty());
    }

    #[test]
    fn a_section_of_its_own_looks_as_a_types_does() {
        let w = world("shape");
        let body = "<block>\nWelcome.\n</block>\n\n<block title=Places style=box bottom fold=folded heading=2>\nHere.\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", body)).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", body));
        let places = &back.document.blocks[1];
        assert_eq!((places.style, places.place, places.fold, places.heading), (aw_model::template::BlockStyle::Box, aw_model::template::BlockPlace::Bottom, aw_model::template::BlockFold::Folded, 2));
        // The read view has it too: in the part's fields.
        let view = crate::build_view(&w.store.load_articles(&w.id).unwrap(), &w.store.view_keys(&w.id, &w.gm).unwrap());
        let home = view.iter().find(|a| a.id == w.home).unwrap();
        let first = home.layers.iter().find(|l| l.title == "Places").unwrap();
        assert_eq!(first.fields["shape"]["place"], "bottom");
    }

    #[test]
    fn a_page_keeps_its_own_sections_in_the_order_written() {
        let w = world("own");
        let two = "<block>\nWelcome.\n</block>\n\n<block title=Places>\nThe coast.\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", two)).unwrap();
        // A new section between them, named but empty yet: it stays, there.
        let three = "<block>\nWelcome.\n</block>\n\n<block title=Timeline>\n</block>\n\n<block title=Places>\nThe coast.\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", three)).unwrap();
        assert_eq!(code_of(&w.store, &w.gm, &w.id, &w.home).unwrap().code, skeleton("Eldermere", three));
        // Moved to the top, it stays there — and the page's readers have the order.
        let moved = "<block title=Places>\nThe coast.\n</block>\n\n<block>\nWelcome.\n</block>\n\n<block title=Timeline>\n</block>\n";
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", moved)).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", moved));
        let ring = w.store.load_keyring(&w.id).unwrap();
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), &ring.restrict_to(&["players".parse().unwrap(), "all".parse().unwrap()]));
        let page = view.iter().find(|a| a.id == w.home).unwrap();
        assert_eq!(page.order, back.document.blocks.iter().map(|b| b.key.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn an_empty_section_has_its_part_to_write_in() {
        let w = world("empty");
        let story = TemplateBlock { key: "story".into(), title: "Story".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("story", "players", "gm", "open")] };
        let allies = TemplateBlock { key: "allies".into(), title: "Allies".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("allies", "players", "gm", "open")] };
        w.store.save_templates(&w.id, &[template("place", vec![], vec![story, allies])]).unwrap();
        let tavern = doc(&w, "place", "Tavern", vec![layer("story", "The tavern is old.", "players", "gm", "open")]);
        let code = code_of(&w.store, &w.gm, &w.id, &tavern).unwrap();
        let block = code.document.blocks.iter().find(|b| b.key == "allies").expect("the empty section is there");
        assert_eq!((block.parts.len(), block.parts[0].key.as_str(), block.parts[0].text.as_str()), (1, "allies", ""));
        // Written in, it becomes a layer; left empty, it stays none.
        save_now(&w.store, &w.gm, &w.id, &tavern, &code.code.replace("<block title=Allies view=players edit=gm>\n", "<block title=Allies view=players edit=gm>\nThe smugglers.\n")).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &tavern).unwrap();
        assert!(back.code.contains("<block title=Allies view=players edit=gm>\nThe smugglers.\n</block>"), "{}", back.code);
        // Empty, but with access of its own: that is kept.
        let inn = doc(&w, "place", "Inn", Vec::new());
        let empty = code_of(&w.store, &w.gm, &w.id, &inn).unwrap().code;
        let ruled = empty.replace("<block title=Allies view=players edit=gm>", "<block title=Allies view=players edit=\"gm OR player:bob\">");
        assert_ne!(ruled, empty);
        save_now(&w.store, &w.gm, &w.id, &inn, &ruled).unwrap();
        let again = code_of(&w.store, &w.gm, &w.id, &inn).unwrap();
        assert!(again.code.contains("<block title=Allies view=players edit=\"gm OR player:bob\">\n</block>"), "{}", again.code);
    }

    #[test]
    fn a_secret_pin_reaches_only_whom_it_names() {
        let w = world("pins");
        let body = |cave: &str| {
            format!("<block>\nThe coast.\n<map src=img:chart>\n<pin at=10,20 label=Harbour>\n<pin at=30,40 label=Cave{cave}>\n</map>\n</block>\n")
        };
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", &body(" view=gm edit=gm"))).unwrap();

        // The GM writes and reads one map; it reads back the same.
        let back = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(back.code, skeleton("Eldermere", &body(" view=gm edit=gm")));
        // Stored apart: the players' part has the harbour only.
        let ring = w.store.load_keyring(&w.id).unwrap();
        let players = ring.restrict_to(&["players".parse().unwrap(), "all".parse().unwrap()]);
        let theirs = texts(&w, &players, &w.home);
        assert!(theirs.iter().any(|(_, t)| t.contains("Harbour")), "{theirs:?}");
        assert!(theirs.iter().all(|(_, t)| !t.contains("Cave")), "the cave is not theirs: {theirs:?}");
        let gms = texts(&w, &ring.restrict_to(&ring.attributes().cloned().collect::<Vec<_>>()), &w.home);
        assert!(gms.iter().any(|(_, t)| t.starts_with("<pins for=img:chart>") && t.contains("Cave")), "{gms:?}");

        // Revealed: onto the map, for everyone.
        save_now(&w.store, &w.gm, &w.id, &w.home, &skeleton("Eldermere", &body(""))).unwrap();
        let theirs = texts(&w, &players, &w.home);
        assert!(theirs.iter().any(|(_, t)| t.contains("<pin at=30,40 label=Cave>")), "{theirs:?}");
        assert_eq!(code_of(&w.store, &w.gm, &w.id, &w.home).unwrap().code, skeleton("Eldermere", &body("")));
    }

    #[test]
    fn the_skeleton_renames_and_releases() {
        let w = world("skeleton");
        let code = skeleton("Eldermere", "<block>\nFor everyone.\n</block>\n")
            .replace("<name=Eldermere>", "<name=\"The Mist Lands\">")
            .replace("<article view=all edit=gm>", "<article view=gm edit=gm>");
        save_now(&w.store, &w.gm, &w.id, &w.home, &code).unwrap();

        let back = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap();
        assert_eq!(back.document.name, "The Mist Lands");
        assert_eq!(back.document.access.view, "gm");
        let ring = w.store.load_keyring(&w.id).unwrap();
        let players = ring.restrict_to(&["players".parse().unwrap(), "all".parse().unwrap()]);
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), &players);
        assert!(view.iter().all(|a| a.id != w.home), "the bracket hides the page from players now");
    }

    #[test]
    fn a_secret_region_becomes_a_layer_of_its_own() {
        let w = world("secret");
        save_now(
            &w.store,
            &w.gm,
            &w.id,
            &w.home,
            &skeleton(
                "Eldermere",
                "<block>\nFor everyone.\n\n<block view=gm edit=gm level=gm-only>\nThe hidden door.\n</block>\n</block>\n",
            ),
        )
        .unwrap();

        let ring = w.store.load_keyring(&w.id).unwrap();
        let players = ring.restrict_to(&["players".parse().unwrap(), "all".parse().unwrap()]);
        let seen: Vec<String> = texts(&w, &players, &w.home).into_iter().map(|(_, t)| t).collect();
        assert_eq!(seen, vec!["For everyone."], "players do not get the secret");

        let all: Vec<String> = texts(&w, &ring.everything(), &w.home).into_iter().map(|(_, t)| t).collect();
        assert!(all.contains(&"The hidden door.".to_string()), "{all:?}");

        // The layer is stored gm-only: players hold it, but cannot read it.
        let stored = w.store.load_articles(&w.id).unwrap();
        let page = stored.iter().find(|a| a.id.to_string() == w.home).unwrap();
        assert!(page.layers.iter().any(|l| l.level == Level::GmOnly));
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

        let code = code_of(&w.store, &w.gm, &w.id, &paris).unwrap();
        assert!(code.code.starts_with("---\nClimate:\nMayor:\n---"), "the type's fields stand ready:\n{}", code.code);

        let body = skeleton("Paris", "");
        save_now(&w.store, &w.gm, &w.id, &paris, &format!("---\nClimate: @@warm@@\nMayor: Vellmar\nMotto: Fluctuat nec mergitur\n---\n\n{body}")).unwrap();
        let held = w.store.load_keyring(&w.id).unwrap().everything();
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
        let fields = &view.iter().find(|a| a.id == paris).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields;
        assert_eq!(fields["climate"], serde_json::json!(warm), "a reference is stored by id");
        assert_eq!(fields["mayor"], "Vellmar");
        assert_eq!(fields["Motto"], "Fluctuat nec mergitur", "a free property by its name");

        let back = code_of(&w.store, &w.gm, &w.id, &paris).unwrap();
        assert_eq!(
            back.code,
            format!("---\nClimate: @@climate:warm@@\nMayor: Vellmar\nMotto: Fluctuat nec mergitur\n---\n\n{body}"),
            "a reference is written with its type"
        );

        let err = save_now(&w.store, &w.gm, &w.id, &paris, &format!("---\nClimate: @@tropical@@\n---\n\n{body}")).unwrap_err();
        assert!(err.0.contains("tropical"), "{}", err.0);
        let err = save_now(&w.store, &w.gm, &w.id, &paris, &format!("---\nClimate: @@place:warm@@\n---\n\n{body}")).unwrap_err();
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
            let held = w.store.load_keyring(&w.id).unwrap().everything();
            let view = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
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
            save_now(&w.store, &w.gm, &w.id, &gauntlets, &code(written)).unwrap();
            assert_eq!(stored(&w), kept, "{written}");
            assert!(code_of(&w.store, &w.gm, &w.id, &gauntlets).unwrap().code.contains(&format!("Strength: {back}\n")), "{written}");
        }
        assert!(save_now(&w.store, &w.gm, &w.id, &gauntlets, &code("= much")).is_err());
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
        save_now(&w.store, &w.gm, &w.id, &one, &code("15 Hay Moon 812, 18:30", "12.09.2026, 19:00")).unwrap();

        let held = w.store.load_keyring(&w.id).unwrap().everything();
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
        let fields = &view.iter().find(|a| a.id == one).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields;
        let day = aw_model::Era::default().to_date(812, 7, 15).unwrap().0 as f64;
        assert_eq!(fields["when"].as_f64(), Some(day + 18.5 / 24.0), "a day count, the time as its fraction");
        assert_eq!(fields["played"], "2026-09-12T19:00");

        let back = code_of(&w.store, &w.gm, &w.id, &one).unwrap().code;
        assert!(back.contains("When: 15 Hay Moon 812 AR, 18:30"), "{back}");
        assert!(back.contains("Played: 2026-09-12T19:00"), "{back}");
        let err = save_now(&w.store, &w.gm, &w.id, &one, &code("next Tuesday", "")).unwrap_err();
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
                parts: vec![part("story", "all", "gm", "open")],
            }],
        );
        w.store.save_templates(&w.id, &[place]).unwrap();
        let paris = doc(&w, "place", "Paris", vec![]);

        let code = |text: &str| {
            format!("---\nMayor: Vellmar\n---\n\n{}", skeleton("Paris", &format!("<block title=Story>\n{text}\n</block>\n")))
        };
        save_now(&w.store, &w.gm, &w.id, &paris, &code("The mayor is %%Mayor%%.\n<text bind=Mayor>")).unwrap();
        let err = save_now(&w.store, &w.gm, &w.id, &paris, &code("Ask %%Nobody%%.")).unwrap_err();
        assert!(err.0.contains("Nobody") && err.0.starts_with("Line 9"), "{}", err.0);

        // Stored by key — so renaming the property renames it everywhere.
        let held = w.store.load_keyring(&w.id).unwrap().everything();
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
        let story = view.iter().find(|a| a.id == paris).unwrap().layers.iter().find(|l| l.section == "story").unwrap();
        assert_eq!(story.text, "The mayor is %%mayor%%.\n<text bind=mayor>", "by key, not by name");
        let mut types = w.store.load_templates(&w.id).unwrap();
        types[0].properties[0].label = "Bürgermeister".into();
        w.store.save_templates(&w.id, &types).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &paris).unwrap().code;
        assert!(back.contains("Bürgermeister: Vellmar"), "{back}");
        assert!(back.contains("The mayor is %%Bürgermeister%%.\n<text bind=Bürgermeister>"), "{back}");

        // A list names the listed type's properties — and comes back with the
        // current names.
        let list = "<list type=place show=Bürgermeister where=\"Bürgermeister=Vellmar\">";
        save_now(&w.store, &w.gm, &w.id, &paris, &code(list).replace("Mayor: Vellmar", "Bürgermeister: Vellmar")).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &paris).unwrap().code;
        // Quoted: unquoted, its `=` would make Markdown read the tag as text.
        assert!(back.contains("<list type=place show=Bürgermeister where=\"Bürgermeister=Vellmar\">"), "{back}");

        // Another document's property: by the document's id and the key.
        let home = skeleton("Eldermere", "<block>\nParis is ruled by @@place:Paris#Bürgermeister@@.\n</block>\n");
        save_now(&w.store, &w.gm, &w.id, &w.home, &home).unwrap();
        let stored = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
        let text = &stored.iter().find(|a| a.id == w.home).unwrap().layers[0].text;
        assert_eq!(text, &format!("Paris is ruled by @@{paris}#mayor@@."));
        let mut types = w.store.load_templates(&w.id).unwrap();
        types[0].properties[0].label = "Mayor".into();
        w.store.save_templates(&w.id, &types).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &w.home).unwrap().code;
        assert!(back.contains("@@place:Paris#Mayor@@"), "{back}");
    }

    /// How long ten thousand documents take — a compendium's size. Run by hand:
    /// `cargo test --release many_documents -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn many_documents() {
        let w = world("many");
        let monster = template("monster", vec![field("cr", "CR", FieldKind::Text { multiline: false })], vec![]);
        w.store.save_templates(&w.id, &[monster]).unwrap();
        let text = "Armor Class 15. Hit Points 10 (3d6). Speed 30 ft.\n\n".repeat(30);
        let started = std::time::Instant::now();
        for i in 0..10_000 {
            let mut props = layer(INFOBOX_SECTION, "", "all", "gm", "open");
            props.fields = serde_json::json!({ "cr": "1/4" });
            doc(&w, "monster", &format!("Monster {i}"), vec![props, layer("story", &text, "all", "gm", "open")]);
        }
        eprintln!("create: {:?}", started.elapsed());
        let size: u64 = walk(&w.dir);
        eprintln!("on disk: {} MB", size / 1_000_000);
        let started = std::time::Instant::now();
        let stored = w.store.load_articles(&w.id).unwrap();
        eprintln!("load: {:?}", started.elapsed());
        let held = w.store.load_keyring(&w.id).unwrap().everything();
        let started = std::time::Instant::now();
        let view = build_view(&stored, &held);
        eprintln!("decrypt: {:?} ({} documents)", started.elapsed(), view.len());
        let started = std::time::Instant::now();
        let json = serde_json::to_string(&view).unwrap();
        eprintln!("to the page: {:?}, {} MB", started.elapsed(), json.len() / 1_000_000);
        fn walk(dir: &std::path::Path) -> u64 {
            std::fs::read_dir(dir).map(|r| r.flatten().map(|e| if e.path().is_dir() { walk(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum()).unwrap_or(0)
        }
    }

    /// Loading what `many_documents` left in `AW_BENCH_DIR`.
    #[test]
    #[ignore]
    fn many_documents_load() {
        let Some(dir) = std::env::var_os("AW_BENCH_DIR") else { return };
        let store = Store::new(PathBuf::from(dir));
        let id = store.list_worlds().unwrap().into_iter().next().unwrap().public.world_id.to_string();
        let started = std::time::Instant::now();
        store.with_journal(&id, |_| Ok(())).unwrap();
        eprintln!("open: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let stored = store.load_articles(&id).unwrap();
        eprintln!("load: {:?} ({} stored)", started.elapsed(), stored.len());
        let gm = store.load_identity().unwrap();
        let _ = gm;
        let started = std::time::Instant::now();
        create_document(&store, &id, "monster".into(), "new".into(), "New".into(), "all".into(), vec![]).unwrap();
        eprintln!("one more: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let after = store.load_articles(&id).unwrap();
        eprintln!("load after a change: {:?} ({} stored)", started.elapsed(), after.len());
        let held = store.load_keyring(&id).unwrap().everything();
        let started = std::time::Instant::now();
        let view = build_view(&stored, &held);
        eprintln!("decrypt: {:?} ({} documents)", started.elapsed(), view.len());
        let started = std::time::Instant::now();
        let json = serde_json::to_string(&view).unwrap();
        eprintln!("to the page: {:?}, {} MB", started.elapsed(), json.len() / 1_000_000);
    }

    /// Where opening one document as code spends its time, in a real profile
    /// (`AW_BENCH_DIR` with `profile/` and `worlds/`, `AW_BENCH_DOC` its id).
    #[test]
    #[ignore]
    fn open_timing() {
        let (Some(dir), Some(article)) = (std::env::var_os("AW_BENCH_DIR"), std::env::var("AW_BENCH_DOC").ok()) else { return };
        let store = Store::new(PathBuf::from(dir));
        let id = store.list_worlds().unwrap().into_iter().next().unwrap().public.world_id.to_string();
        for round in 0..4 {
            if round == 3 {
                // Past the full look at every folder.
                std::thread::sleep(std::time::Duration::from_secs(31));
            }
            let started = std::time::Instant::now();
            let identity = store.load_identity().unwrap();
            let t_identity = started.elapsed();
            let held = store.view_keys(&id, &identity).unwrap();
            let t_keys = started.elapsed();
            let articles = store.load_articles(&id).unwrap();
            let t_load = started.elapsed();
            let opened = open(&store, &id, &article, &held, &articles).unwrap();
            let t_open = started.elapsed();
            code_from(&store, &id, &articles, opened).unwrap();
            let t_code = started.elapsed();
            let started = std::time::Instant::now();
            store.with_journal(&id, |_| Ok(())).unwrap();
            let t_journal = started.elapsed();
            let layers = &articles.iter().find(|a| a.id.to_string() == article).unwrap().layers;
            let mut each = Vec::new();
            for l in layers {
                let at = std::time::Instant::now();
                store.layer_state(&id, l.id).unwrap();
                each.push(at.elapsed());
            }
            eprintln!("  journal alone {t_journal:?}, layer_state each {each:?}");
            eprintln!("round {round}: identity {t_identity:?}, keys {:?}, load {:?}, open {:?}, code {:?}", t_keys - t_identity, t_load - t_keys, t_open - t_load, t_code - t_open);
        }
    }

    #[test]
    fn the_infobox_has_access_of_its_own() {
        let w = world("infobox");
        let npc = template("npc", vec![field("title", "Title", FieldKind::Text { multiline: false })], vec![]);
        w.store.save_templates(&w.id, &[npc]).unwrap();
        let mira = doc(&w, "npc", "Mira", vec![]);
        let infobox = |held: &aw_policy::HeldKeys| {
            let all = build_view(&w.store.load_articles(&w.id).unwrap(), held);
            let layer = all.iter().find(|a| a.id == mira).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).map(|l| (l.policy.clone(), l.edit.clone())).unwrap();
            layer
        };
        let held = w.store.load_keyring(&w.id).unwrap().everything();

        // Players may change the infobox of a document only the GM changes.
        let body = skeleton("Mira", "<infobox view=all edit=players>\n");
        save_now(&w.store, &w.gm, &w.id, &mira, &format!("---\nTitle: Innkeeper\n---\n\n{body}")).unwrap();
        assert_eq!(infobox(&held), ("all".to_string(), "players".to_string()));
        let back = code_of(&w.store, &w.gm, &w.id, &mira).unwrap().code;
        assert!(back.contains("<body>\n<infobox view=all edit=players>\n"), "{back}");

        // Without one of its own it goes as the document.
        save_now(&w.store, &w.gm, &w.id, &mira, &format!("---\nTitle: Innkeeper\n---\n\n{}", skeleton("Mira", ""))).unwrap();
        assert_eq!(infobox(&held), ("all".to_string(), "gm".to_string()));
    }

    #[test]
    fn a_region_written_into_a_section_leaves_the_infobox_as_it_was() {
        let w = world("region-infobox");
        let mut log = template(
            "log",
            vec![field("date", "Date", FieldKind::Text { multiline: false })],
            vec![TemplateBlock { parts: vec![part("journal", "all", "players", "open")], ..TemplateBlock::fresh("journal", "Journal") }],
        );
        log.auth = Some(Access { view: "all".into(), edit: "players".into(), level: "open".into() });
        w.store.save_templates(&w.id, &[log]).unwrap();
        let mut props = layer(INFOBOX_SECTION, "", "all", "players", "open");
        props.fields = serde_json::json!({ "date": "1 Hammer" });
        let id = create_document(&w.store, &w.id, "log".into(), "s1".into(), "Session 1".into(), "all".into(), vec![props, layer("journal", "They meet.", "all", "players", "open")]).unwrap();
        let held = w.store.load_keyring(&w.id).unwrap().everything();
        let infobox = || build_view(&w.store.load_articles(&w.id).unwrap(), &held).into_iter().find(|a| a.id == id).unwrap().layers.into_iter().find(|l| l.section == INFOBOX_SECTION).map(|l| (l.policy, l.edit)).unwrap();
        assert_eq!(infobox(), ("all".to_string(), "players".to_string()));
        let code = code_of(&w.store, &w.gm, &w.id, &id).unwrap().code;
        let region = code.replace("They meet.\n", "They meet.\n\n<block view=\"gm OR doc:ABC\" edit=\"gm OR doc:ABC\">\nA secret.\n</block>\n");
        assert_ne!(region, code);
        save_now(&w.store, &w.gm, &w.id, &id, &region).unwrap();
        assert_eq!(infobox(), ("all".to_string(), "players".to_string()), "{}", code_of(&w.store, &w.gm, &w.id, &id).unwrap().code);
    }

    #[test]
    fn a_field_with_access_of_its_own_is_a_layer_of_its_own() {
        let w = world("field");
        let mut secret = field("goal", "True goal", FieldKind::Text { multiline: false });
        secret.access = Some(Access { view: "gm".into(), edit: "gm".into(), level: "gm-only".into() });
        let npc = template("npc", vec![field("title", "Title", FieldKind::Text { multiline: false }), secret], vec![]);
        w.store.save_templates(&w.id, &[npc]).unwrap();
        let mira = doc(&w, "npc", "Mira", vec![]);

        let body = skeleton("Mira", "");
        save_now(&w.store, &w.gm, &w.id, &mira, &format!("---\nTitle: Innkeeper\nTrue goal: Revenge\n---\n\n{body}")).unwrap();

        let stored = w.store.load_articles(&w.id).unwrap();
        let ring = w.store.load_keyring(&w.id).unwrap();
        let all = build_view(&stored, &ring.everything());
        let layers = &all.iter().find(|a| a.id == mira).unwrap().layers;
        let own = layers.iter().find(|l| l.section == "properties:goal").expect("a layer of its own");
        assert_eq!(own.fields["goal"], "Revenge");
        assert!(layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields.get("goal").is_none());
        let raw = stored.iter().find(|a| a.id.to_string() == mira).unwrap();
        assert!(raw.layers.iter().any(|l| l.level == Level::GmOnly), "only the GM reads it");

        // Players get the title, not the goal — not even its label.
        let players = ring.restrict_to(&["players".parse().unwrap(), "all".parse().unwrap()]);
        let theirs = build_view(&stored, &players);
        let layers = &theirs.iter().find(|a| a.id == mira).unwrap().layers;
        assert!(layers.iter().all(|l| l.section != "properties:goal"));

        let back = code_of(&w.store, &w.gm, &w.id, &mira).unwrap();
        assert!(back.code.starts_with("---\nTitle: Innkeeper\nTrue goal: Revenge\n---"), "{}", back.code);
    }

    #[test]
    fn a_reader_who_misses_a_part_between_two_writes_both() {
        let w = world("between");
        let body = "<block title=Journal>\nFirst.\n<block view=gm edit=gm level=gm-only>\nSecret.\n</block>\nSecond.\n</block>\n";
        let page = doc(&w, "", "Session", vec![]);
        let code = format!("<article view=all edit=players>\n<name=Session>\n<body>\n{body}</body>\n</article>\n");
        save_now(&w.store, &w.gm, &w.id, &page, &code).unwrap();
        let bob = Identity::create("Bob").unwrap();
        let mut meta = w.store.load_world_meta(&w.id).unwrap();
        meta.members.push(Member { identity: bob.public(), role: Role::Player, attributes: vec!["players".into(), "all".into()] });
        w.store.save_world_meta(&meta).unwrap();
        let dir = std::env::temp_dir().join(format!("aw-doc-between-bob-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let bobs = Store::new(dir.clone());
        bobs.save_identity(&bob).unwrap();
        bobs.receive(&w.id, &w.store.all_records(&w.id).unwrap()).unwrap();

        // Bob sees two parts, apart, and changes the second.
        let theirs = code_of(&bobs, &bob, &w.id, &page).unwrap().code;
        assert!(!theirs.contains("Secret"), "{theirs}");
        save_now(&bobs, &bob, &w.id, &page, &theirs.replace("Second.", "Second, _changed_.")).unwrap();
        bobs.close_session(&w.id, "test").unwrap();
        w.store.receive(&w.id, &bobs.all_records(&w.id).unwrap()).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &page).unwrap().code;
        assert!(back.contains("First.\n\n<block view=gm edit=gm level=gm-only>\nSecret.\n</block>\n\nSecond, _changed_.\n</block>"), "{back}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_types_access_goes_to_its_documents_and_their_sections_alike() {
        let w = world("typeaccess");
        let story = TemplateBlock { key: "story".into(), title: "Story".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("story", "all", "gm", "open")] };
        let secret = TemplateBlock { key: "secret".into(), title: "Secret".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("secret", "gm", "gm", "gm-only")] };
        w.store.save_templates(&w.id, &[template("npc", vec![], vec![story, secret])]).unwrap();
        let ilva = doc(&w, "npc", "Ilva", vec![layer("story", "A smuggler.", "all", "gm", "open"), layer("secret", "Works for the duke.", "gm", "gm", "gm-only")]);
        let other = doc(&w, "npc", "Bram", vec![layer("story", "A fisher.", "all", "gm", "open")]);
        let n = crate::type_access_apply_in(&w.store, &w.id, "npc", &[ilva.clone()], None, Some("players".into())).unwrap();
        assert_eq!(n, 1);
        let code = code_of(&w.store, &w.gm, &w.id, &ilva).unwrap().code;
        assert!(code.starts_with("<article view=all edit=players>"), "{code}");
        assert!(code.contains("<block title=Story>\nA smuggler."), "the section follows: {code}");
        assert!(code.contains("<block title=Secret view=gm edit=gm level=gm-only>"), "the secret keeps its own: {code}");
        assert!(code_of(&w.store, &w.gm, &w.id, &other).unwrap().code.contains("edit=gm>"), "only the documents named");
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
            parts: vec![part("story", "players", "gm", "open")],
        };
        let journal = TemplateBlock {
            key: "journal".into(),
            title: "Journal".into(),
            style: Default::default(),
            place: Default::default(), width: 0,
            fold: Default::default(),
            heading: 1,
            new: Vec::new(),
            parts: vec![part("journal", "players", "players", "open")],
        };
        w.store.save_templates(&w.id, &[template("place", vec![], vec![story, journal])]).unwrap();
        let tavern = doc(&w, "place", "Tavern", vec![
            layer("story", "The tavern is old.", "players", "gm", "open"),
            layer("story-2", "The innkeeper is a spy.", "gm", "gm", "sealed"),
            layer("journal", "We arrived at night.", "players", "players", "open"),
        ]);

        // Bob joins and gets the world.
        let bob = Identity::create("Bob").unwrap();
        let mut meta = w.store.load_world_meta(&w.id).unwrap();
        meta.members.push(Member { identity: bob.public(), role: Role::Player, attributes: vec!["players".into(), "all".into()] });
        w.store.save_world_meta(&meta).unwrap();
        let records = w.store.all_records(&w.id).unwrap();
        let dir = std::env::temp_dir().join(format!("aw-doc-bob-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let bobs = Store::new(dir.clone());
        bobs.save_identity(&bob).unwrap();
        bobs.receive(&w.id, &records).unwrap();

        let code = code_of(&bobs, &bob, &w.id, &tavern).unwrap().code;
        assert!(!code.contains("spy"), "the code holds only what Bob can open:\n{code}");

        // The journal is his to write.
        let journal_edit = code.replace("We arrived at night.", "We arrived at night. It rained.");
        save_now(&bobs, &bob, &w.id, &tavern, &journal_edit).unwrap();

        // The story is not.
        let story_edit = journal_edit.replace("The tavern is old.", "The tavern is mine.");
        let err = save_now(&bobs, &bob, &w.id, &tavern, &story_edit).unwrap_err();
        assert!(err.0.contains("may not change"), "{}", err.0);

        // Neither is who sees it.
        let access_edit = journal_edit.replace("<block title=Journal", "<block title=Journal view=all");
        let err = save_now(&bobs, &bob, &w.id, &tavern, &access_edit).unwrap_err();
        assert!(err.0.contains("Only the GM"), "{}", err.0);

        // Nor the name, nor who sees the whole document.
        let rename = journal_edit.replace("<name=Tavern>", "<name=\"Bob's place\">");
        assert!(save_now(&bobs, &bob, &w.id, &tavern, &rename).is_err());
        let release = journal_edit.replace("<article view=all edit=gm>", "<article view=players edit=gm>");
        let err = save_now(&bobs, &bob, &w.id, &tavern, &release).unwrap_err();
        assert!(err.0.contains("Only the GM"), "{}", err.0);

        // Nor removing a part.
        let removal = journal_edit.replace("The tavern is old.\n", "");
        assert!(save_now(&bobs, &bob, &w.id, &tavern, &removal).is_err());

        // And the part he never saw is still there, untouched — and never
        // came to him at all.
        let held = bobs.view_keys(&w.id, &bob).unwrap();
        let stored = bobs.load_articles(&w.id).unwrap();
        let t = stored.iter().find(|a| a.id.to_string() == tavern).unwrap();
        assert_eq!(t.layers.len(), 4, "base, story, secret, journal");
        let theirs = w.store.load_articles(&w.id).unwrap();
        assert_eq!(theirs.iter().find(|a| a.id.to_string() == tavern).unwrap().layers.len(), 4);
        let journal = build_view(&stored, &held)
            .into_iter()
            .find(|a| a.id == tavern)
            .unwrap()
            .layers
            .into_iter()
            .find(|l| l.section == "journal")
            .unwrap();
        assert_eq!(journal.text, "We arrived at night. It rained.");

        std::fs::remove_dir_all(&dir).ok();
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
            let back = code_of(&w.store, &w.gm, &w.id, article).unwrap();
            back.document.properties.iter().find(|p| p.name == "Relations").map(|p| p.value.clone()).unwrap_or_default()
        };

        save_now(&w.store, &w.gm, &w.id, &marie, &code("@@Olga@@ Mother/Daughter, @@Helga@@ Aunt")).unwrap();
        assert_eq!(relations(&marie), "@@character:Olga@@ Mother/Daughter, @@character:Helga@@ Aunt");
        assert_eq!(relations(&olga), "@@character:Marie@@ Daughter/Mother", "written on the other side, turned round");
        assert_eq!(relations(&helga), "@@character:Marie@@ /Aunt");
        let held = w.store.load_keyring(&w.id).unwrap().everything();
        let view = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
        let stored = &view.iter().find(|a| a.id == marie).unwrap().layers.iter().find(|l| l.section == INFOBOX_SECTION).unwrap().fields;
        assert_eq!(stored["rel"][0], serde_json::json!({ "with": olga, "role": mother, "own_role": daughter }), "by id");

        // A change goes over too; what is gone from one side goes from both.
        save_now(&w.store, &w.gm, &w.id, &marie, &code("@@Olga@@ Aunt, @@Helga@@ Aunt")).unwrap();
        assert_eq!(relations(&olga), "@@character:Marie@@ /Aunt");
        save_now(&w.store, &w.gm, &w.id, &marie, &code("@@Helga@@ Aunt")).unwrap();
        assert_eq!(relations(&olga), "");

        // Unknown roles and relations to oneself are errors.
        let err = save_now(&w.store, &w.gm, &w.id, &marie, &code("@@Olga@@ Cousin")).unwrap_err();
        assert!(err.0.contains("Cousin"), "{}", err.0);
        let err = save_now(&w.store, &w.gm, &w.id, &marie, &code("@@Marie@@ Aunt")).unwrap_err();
        assert!(err.0.contains("itself"), "{}", err.0);

        // A player who cannot open Helga does not see whom the entry names —
        // and saving what they see keeps it.
        let ring = w.store.load_keyring(&w.id).unwrap();
        let players = ring.restrict_to(&["players".parse().unwrap(), "all".parse().unwrap()]);
        let articles = w.store.load_articles(&w.id).unwrap();
        let theirs = open(&w.store, &w.id, &marie, &players, &articles).unwrap();
        let written = theirs.text.properties.iter().find(|p| p.name == "Relations").unwrap();
        assert_eq!(written.value, "", "the entry to Helga is not in their text");
        let fields = fields_from(&theirs, &[Property { name: "Relations".into(), value: "@@Olga@@ Mother".into(), group: Vec::new() }]).unwrap();
        let kept = relation::entries(&fields["rel"]);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert!(kept.iter().any(|e| e.with == helga && e.role == aunt), "{kept:?}");

        // From the graph: one call, both sides.
        set_relation(&w.store, &w.gm, &w.id, &olga, "rel", &marie, Some((daughter.clone(), mother.clone())), "graph").unwrap();
        assert_eq!(relations(&marie), "@@character:Helga@@ Aunt, @@character:Olga@@ Mother/Daughter");
        set_relation(&w.store, &w.gm, &w.id, &marie, "rel", &olga, None, "graph").unwrap();
        assert_eq!(relations(&olga), "");

        // A document that goes is forgotten by those it named.
        forget_relations(&w.store, &w.gm, &w.id, &helga).unwrap();
        assert_eq!(relations(&marie), "");

        // A graph in the text keeps its people and lines — stored by id,
        // written by name.
        let graph = "<graph around=self depth=2>\n<person who=Olga at=3,4>\n<line between=\"Marie, Olga\" color=amber sides=\"bottom, top\">\n</graph>\n";
        let code = format!("---\nRelations: @@Olga@@ Mother/Daughter\n---\n\n{}", skeleton("Marie", &format!("<block title=Tree>\n{graph}</block>\n")));
        save_now(&w.store, &w.gm, &w.id, &marie, &code).unwrap();
        let back = code_of(&w.store, &w.gm, &w.id, &marie).unwrap().code;
        assert!(back.contains(graph), "{back}");
        let stored = build_view(&w.store.load_articles(&w.id).unwrap(), &held);
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
