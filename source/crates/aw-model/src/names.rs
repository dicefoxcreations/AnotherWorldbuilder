//! Names in the text, ids in storage.
//!
//! Whoever writes types names: `@@Place:Paris@@`, `<list type=Place>`,
//! `%%Mayor%%`, `<text bind=Mayor>`. What is stored is ids: `@@art:01HX…@@`,
//! `<list type=01hx…>`, `%%01hy…%%`, `<text bind=01hy…>`. So a rename changes
//! nothing that is stored — the name is looked up afresh whenever the text is
//! shown or opened, and a reference reaches its target even in a layer the
//! one renaming cannot open.
//!
//! The translation happens at the edges: when text comes in to be stored,
//! and when it goes out to be edited. A name that names nothing stays as it
//! is — a link to a page not written yet, a free property of one document
//! only. An id this reader cannot resolve stays an id: it points at something
//! they are not shown.
//!
//! A property is always somebody's: in a document's text the document type's,
//! in a `<list>` the listed type's. That is the `owner` below.

use crate::link::{self, Link, Piece};
use crate::template_text::{read_tag, tag_back};

/// What the translation needs to know: which documents, types and
/// properties exist for this reader, by name and by id.
pub trait Catalog {
    /// The id of a document by its type's name (may be empty) and its name.
    fn document_id(&self, of: &str, name: &str) -> Option<String>;
    /// A document's type name and name by its id.
    fn document_name(&self, id: &str) -> Option<(String, String)>;
    /// A document's type by id — whose properties `#Status` names.
    fn document_type(&self, id: &str) -> Option<String>;
    /// A type's id by its name — or by its id.
    fn type_id(&self, name: &str) -> Option<String>;
    fn type_name(&self, id: &str) -> Option<String>;
    /// The key of a property of a type (by id), inherited ones included — by
    /// its name, or by its key.
    fn property_key(&self, owner: &str, name: &str) -> Option<String>;
    /// A property by its key: its name, and whether it points at documents.
    fn property(&self, key: &str) -> Option<(String, bool)>;
}

/// The prefix of an article id — `art:01HX…`.
const ARTICLE: &str = "art";

/// The id a link points at, if it is stored as one.
pub fn id_of(link: &Link) -> Option<String> {
    let is_ulid = link.name.len() == 26 && link.name.chars().all(|c| c.is_ascii_alphanumeric());
    (link.of == ARTICLE && is_ulid).then(|| format!("{ARTICLE}:{}", link.name))
}

/// Text as it is stored: names become ids where they name something.
///
/// `owner` is the id of the type whose properties the text may use — empty
/// for a document without a type.
pub fn stored(text: &str, catalog: &impl Catalog, owner: &str) -> String {
    translate(text, &Way { catalog, owner, to_ids: true })
}

/// Text as it is written: ids become the current names where they resolve.
pub fn written(text: &str, catalog: &impl Catalog, owner: &str) -> String {
    translate(text, &Way { catalog, owner, to_ids: false })
}

struct Way<'a, C: Catalog> {
    catalog: &'a C,
    owner: &'a str,
    to_ids: bool,
}

impl<C: Catalog> Way<'_, C> {
    /// A property of `owner`, turned the way this goes.
    fn property(&self, owner: &str, name: &str) -> Option<String> {
        if self.to_ids {
            self.catalog.property_key(owner, name)
        } else {
            self.catalog.property(name).map(|(n, _)| n)
        }
    }

    fn document(&self, name: &str) -> Option<String> {
        if self.to_ids {
            self.catalog.document_id("", name)
        } else {
            self.catalog.document_name(name).map(|(_, n)| n)
        }
    }
}

fn translate<C: Catalog>(text: &str, way: &Way<C>) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in link::scan(text) {
        match piece {
            Piece::Text(t) => out.push_str(&values(&tags(t, way), way)),
            Piece::Link(l) => out.push_str(&translate_link(&l, way)),
        }
    }
    out
}

fn translate_link<C: Catalog>(l: &Link, way: &Way<C>) -> String {
    let shown = if l.text != l.name { l.text.as_str() } else { "" };
    // `#Status` is a property of the target's type.
    let target = id_of(l).or_else(|| way.catalog.document_id(&l.of, &l.name));
    let property = match (&target, l.property.is_empty()) {
        (_, true) => String::new(),
        (Some(id), false) => way
            .catalog
            .document_type(id)
            .and_then(|t| way.property(&t, &l.property))
            .unwrap_or_else(|| l.property.clone()),
        (None, false) => l.property.clone(),
    };
    let (of, name) = match (id_of(l), way.to_ids) {
        (Some(id), false) => match way.catalog.document_name(&id) {
            Some(found) => found,
            None => (l.of.clone(), l.name.clone()),
        },
        (None, true) => match way.catalog.document_id(&l.of, &l.name) {
            Some(id) => {
                let (of, name) = id.split_once(':').unwrap_or((ARTICLE, &id));
                (of.to_string(), name.to_string())
            }
            None => (l.of.clone(), l.name.clone()),
        },
        _ => (l.of.clone(), l.name.clone()),
    };
    link::write_full(&of, &name, &property, shown)
}

/// `%%Name%%` — a property of the owner.
fn values<C: Catalog>(text: &str, way: &Way<C>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("%%") {
        let Some(close) = rest[open + 2..].find("%%").map(|c| c + open + 2) else { break };
        let inner = &rest[open + 2..close];
        if inner.trim().is_empty() || inner.contains('\n') {
            out.push_str(&rest[..close]);
            rest = &rest[close..];
            continue;
        }
        let fresh = way.property(way.owner, inner.trim()).unwrap_or_else(|| inner.to_string());
        out.push_str(&rest[..open]);
        out.push_str(&format!("%%{fresh}%%"));
        rest = &rest[close + 2..];
    }
    out.push_str(rest);
    out
}

/// The elements of the language that stand in text — written the same way
/// every time: quotes only where a value needs them.
const ELEMENTS: [&str; 17] = ["list", "graph", "timeline", "calendar", "create", "person", "line", "image", "text", "textarea", "number", "date", "select", "view", "pin", "area", "embed"];

/// The elements in a piece of text: those that name something translated —
/// `<list …>` and any element with `bind=` — and all of them written the
/// same way. Anything else, raw HTML included, stays exactly as typed.
fn tags<C: Catalog>(text: &str, way: &Way<C>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        let Some(end) = rest[i..].find('>').map(|e| i + e) else { break };
        let raw = &rest[i + 1..end];
        out.push_str(&rest[..i]);
        if raw.contains('<') || raw.contains('\n') {
            out.push('<');
            rest = &rest[i + 1..];
            continue;
        }
        let mut tag = read_tag(raw, 0);
        let is_list = tag.name == "list" && !tag.closing;
        let tag_is_embed = tag.name == "embed";
        if tag.closing || !ELEMENTS.contains(&tag.name.as_str()) {
            out.push_str(&rest[i..=end]);
            rest = &rest[end + 1..];
            continue;
        }
        // A graph names what a list names; inside it, people.
        if is_list || (tag.name == "graph" && !tag.closing) {
            list(&mut tag, way);
        }
        // A timeline names types, several of them, and a document; a calendar
        // and a create button types.
        if tag.name == "timeline" || tag.name == "calendar" || tag.name == "create" {
            for (name, v) in tag.attributes_mut() {
                let fresh = match name.as_str() {
                    "type" => Some(
                        v.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(|s| if way.to_ids { way.catalog.type_id(s) } else { way.catalog.type_name(s) }.unwrap_or_else(|| s.to_string()))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    "around" if v.as_str() != "self" => way.document(v),
                    _ => None,
                };
                if let Some(fresh) = fresh {
                    *v = fresh;
                }
            }
        }
        // A view names a property group — a type of its own.
        if tag.name == "view" {
            for (name, v) in tag.attributes_mut() {
                if name == "of" {
                    let fresh = if way.to_ids { way.catalog.type_id(v) } else { way.catalog.type_name(v) };
                    if let Some(fresh) = fresh {
                        *v = fresh;
                    }
                }
            }
        }
        // A pin or a region on a map leads to a document; an embed shows one.
        if tag.name == "pin" || tag.name == "area" || tag.name == "embed" {
            for (name, v) in tag.attributes_mut() {
                if name == "to" || (name == "of" && tag_is_embed) {
                    if let Some(fresh) = way.document(v) {
                        *v = fresh;
                    }
                }
            }
        }
        if tag.name == "person" || tag.name == "line" {
            for (name, v) in tag.attributes_mut() {
                // Ends by comma, the two of a line's end by `&`.
                if name == "who" || name == "between" {
                    *v = v
                        .split(',')
                        .map(|end| {
                            end.split('&')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(|s| way.document(s).unwrap_or_else(|| s.to_string()))
                                .collect::<Vec<_>>()
                                .join(" & ")
                        })
                        .filter(|end| !end.is_empty())
                        .collect::<Vec<_>>()
                        .join(", ");
                }
            }
        }
        for (name, v) in tag.attributes_mut() {
            if name == "bind" {
                if let Some(fresh) = way.property(way.owner, v) {
                    *v = fresh;
                }
            }
        }
        out.push_str(&tag_back(&tag));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// `type=`, `below=`, `show=`, `where=` of one `<list>`, and the graph's
/// `via=` and `around=`. Its properties are the listed
/// type's — without one, those of the document the text belongs to.
fn list<C: Catalog>(tag: &mut crate::template_text::Tag, way: &Way<C>) {
    // The listed type by id, whichever way this goes.
    let listed = tag
        .attributes_mut()
        .find(|(n, _)| *n == "type")
        .and_then(|(_, v)| way.catalog.type_id(v))
        .unwrap_or_default();
    let owner = if listed.is_empty() { way.owner.to_string() } else { listed.clone() };
    for (name, v) in tag.attributes_mut() {
        let fresh = match name.as_str() {
            "type" if way.to_ids => way.catalog.type_id(v),
            "type" => way.catalog.type_name(v),
            "below" => way.document(v),
            // A property of the document the list stands in.
            "from" => way.property(way.owner, v),
            "count" | "by" => way.property(&listed, v),
            // Each `Name` or `Name desc`.
            "sort" => Some(
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        let (name, desc) = match s.rsplit_once(char::is_whitespace) {
                            Some((n, d)) if d.eq_ignore_ascii_case("desc") => (n.trim(), " desc"),
                            _ => (s, ""),
                        };
                        let fresh = if name.eq_ignore_ascii_case("name") { None } else { way.property(&listed, name) };
                        format!("{}{desc}", fresh.unwrap_or_else(|| name.to_string()))
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            "around" if v.as_str() == "self" => None,
            "around" => way.document(v),
            "via" => Some(
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| way.property(&owner, s).unwrap_or_else(|| s.to_string()))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            "show" => Some(
                crate::markdown::shown_list(v)
                    .iter()
                    .map(|s| way.property(&listed, s).unwrap_or_else(|| s.to_string()))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            "where" => Some(
                v.split(',')
                    .filter_map(|c| c.split_once('='))
                    .map(|(field, wanted)| {
                        let (field, wanted) = (field.trim(), wanted.trim());
                        // The value is a document where the property points at
                        // documents — otherwise it is text and stays.
                        let key = way.catalog.property_key(&listed, field);
                        let points = key.as_deref().and_then(|k| way.catalog.property(k)).is_some_and(|(_, p)| p);
                        let field = way.property(&listed, field).unwrap_or_else(|| field.to_string());
                        let wanted = if points && !wanted.is_empty() {
                            way.document(wanted).unwrap_or_else(|| wanted.to_string())
                        } else {
                            wanted.to_string()
                        };
                        format!("{field}={wanted}")
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            _ => None,
        };
        if let Some(fresh) = fresh {
            *v = fresh;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World;
    const PARIS: &str = "art:01HX0000000000000000000000";
    const EUROPE: &str = "art:01HX0000000000000000000001";
    const PLACE: &str = "01hxplace0000000000000000";
    const MAYOR: &str = "01hxmayor0000000000000000";
    const LIES_IN: &str = "01hxliesin000000000000000";

    impl Catalog for World {
        fn document_id(&self, of: &str, name: &str) -> Option<String> {
            let place = of.is_empty() || of.eq_ignore_ascii_case("Place");
            match name {
                _ if !place => None,
                n if n.eq_ignore_ascii_case("Paris") || n == PARIS => Some(PARIS.into()),
                n if n.eq_ignore_ascii_case("Europe") || n == EUROPE => Some(EUROPE.into()),
                _ => None,
            }
        }
        fn document_name(&self, id: &str) -> Option<(String, String)> {
            match id {
                PARIS => Some(("Place".into(), "Paris".into())),
                EUROPE => Some(("Place".into(), "Europe".into())),
                _ => None,
            }
        }
        fn document_type(&self, id: &str) -> Option<String> {
            [PARIS, EUROPE].contains(&id).then(|| PLACE.into())
        }
        fn type_id(&self, name: &str) -> Option<String> {
            (name.eq_ignore_ascii_case("Place") || name == PLACE).then(|| PLACE.into())
        }
        fn type_name(&self, id: &str) -> Option<String> {
            (id == PLACE).then(|| "Place".into())
        }
        fn property_key(&self, owner: &str, name: &str) -> Option<String> {
            if owner != PLACE {
                return None;
            }
            match name {
                n if n.eq_ignore_ascii_case("Mayor") || n == MAYOR => Some(MAYOR.into()),
                n if n.eq_ignore_ascii_case("Lies in") || n == LIES_IN => Some(LIES_IN.into()),
                _ => None,
            }
        }
        fn property(&self, key: &str) -> Option<(String, bool)> {
            match key {
                MAYOR => Some(("Mayor".into(), false)),
                LIES_IN => Some(("Lies in".into(), true)),
                _ => None,
            }
        }
    }

    fn round(written_text: &str, stored_text: &str) {
        assert_eq!(stored(written_text, &World, PLACE), stored_text);
        assert_eq!(written(stored_text, &World, PLACE), written_text);
    }

    #[test]
    fn links_go_in_and_come_back_out() {
        assert_eq!(
            stored("To @@Place:Paris@@, @@Paris|the city@@ and @@Nowhere@@.", &World, PLACE),
            format!("To @@{PARIS}@@, @@{PARIS}|the city@@ and @@Nowhere@@.")
        );
        assert_eq!(
            written(&format!("@@{PARIS}|the city@@"), &World, PLACE),
            "@@Place:Paris|the city@@"
        );
    }

    #[test]
    fn lists_go_in_and_come_back_out() {
        round(
            "<list type=Place below=Paris show=\"image, Mayor\" where=\"Lies in=Europe, Mayor=Vellmar\" style=cards>",
            &format!("<list type={PLACE} below={PARIS} show=\"image, {MAYOR}\" where=\"{LIES_IN}={EUROPE}, {MAYOR}=Vellmar\" style=cards>"),
        );
    }

    #[test]
    fn a_timeline_names_its_types_and_a_document() {
        round(
            "<timeline type=\"Place, Nowhere\" around=Paris show=Founded>",
            &format!("<timeline type=\"{PLACE}, Nowhere\" around={PARIS} show=Founded>"),
        );
    }

    #[test]
    fn a_view_names_its_group() {
        round("<view of=Place>", &format!("<view of={PLACE}>"));
    }

    #[test]
    fn a_calendar_names_its_types() {
        round("<calendar type=\"Place, Nowhere\">", &format!("<calendar type=\"{PLACE}, Nowhere\">"));
        round("<create type=Place label=\"New place\">", &format!("<create type={PLACE} label=\"New place\">"));
    }

    #[test]
    fn a_list_sorts_and_counts_by_properties() {
        round(
            "<list type=Place sort=\"Mayor desc, name, Mood\" count=\"Lies in\" by=Mayor>",
            &format!("<list type={PLACE} sort=\"{MAYOR} desc, name, Mood\" count={LIES_IN} by={MAYOR}>"),
        );
    }

    #[test]
    fn an_embed_names_its_document() {
        round("<embed of=Paris>", &format!("<embed of={PARIS}>"));
    }

    /// `from=` names a property of the document the list stands in.
    #[test]
    fn a_list_from_a_property_goes_in_and_comes_back_out() {
        round("<list from=Mayor style=table>", &format!("<list from={MAYOR} style=table>"));
    }

    /// One condition without a space keeps its quotes — unquoted, its `=`
    /// would make Markdown read the whole tag as text.
    #[test]
    fn a_single_condition_stays_quoted() {
        let stored_text = stored("<list type=Place where=\"Mayor=Vellmar\">", &World, PLACE);
        assert_eq!(stored_text, format!("<list type={PLACE} where=\"{MAYOR}=Vellmar\">"));
        let tree = crate::markdown::split(&stored_text);
        assert!(matches!(tree.first(), Some(crate::markdown::Node::Listing { .. })), "{tree:?}");
    }

    #[test]
    fn a_graph_names_its_relations_and_roles() {
        // Without a type the properties are those of the document's own type.
        round(
            "<graph via=\"Lies in, Mood\" around=self>",
            &format!("<graph via=\"{LIES_IN}, Mood\" around=self>"),
        );
        round("<graph type=Place around=Europe>", &format!("<graph type={PLACE} around={EUROPE}>"));
        round(
            "<graph around=self>\n<person who=Paris at=3,4>\n<line between=\"Paris, Europe & Paris\" color=amber>\n</graph>",
            &format!("<graph around=self>\n<person who={PARIS} at=3,4>\n<line between=\"{PARIS}, {EUROPE} & {PARIS}\" color=amber>\n</graph>"),
        );
    }

    #[test]
    fn a_pin_leads_to_a_document() {
        round(
            "<map src=img:abc>\n<pin at=34.5,62 label=\"Stair down\" to=Paris>\n<area at=\"1,2 3,4 5,6\" to=Europe>\n</map>",
            &format!("<map src=img:abc>\n<pin at=34.5,62 label=\"Stair down\" to={PARIS}>\n<area at=\"1,2 3,4 5,6\" to={EUROPE}>\n</map>"),
        );
    }

    #[test]
    fn properties_go_in_and_come_back_out() {
        round(
            "The mayor is %%Mayor%%, and %%Mood%% stays.\n<text bind=Mayor label=Head>",
            &format!("The mayor is %%{MAYOR}%%, and %%Mood%% stays.\n<text bind={MAYOR} label=Head>"),
        );
    }

    #[test]
    fn elements_are_written_the_same_way_and_html_stays() {
        assert_eq!(stored("<image size=\"300x200\" align=\"right\">", &World, PLACE), "<image size=300x200 align=right>");
        let html = "<span class=\"x\">a < b</span>";
        assert_eq!(stored(html, &World, PLACE), html);
    }

    #[test]
    fn a_property_of_another_document() {
        round(
            "Paris is ruled by @@Place:Paris#Mayor@@ — @@Place:Paris#Mayor|the mayor@@.",
            &format!("Paris is ruled by @@{PARIS}#{MAYOR}@@ — @@{PARIS}#{MAYOR}|the mayor@@."),
        );
    }

    #[test]
    fn what_cannot_be_resolved_stays() {
        let text = format!("@@{PARIS}@@ <list type=01hxother000000000000000> %%01hxgone0000000000000000%%");
        assert_eq!(written(&text, &World, ""), text.replace(&format!("@@{PARIS}@@"), "@@Place:Paris@@"));
    }
}
