//! A document as text — its “code”.
//!
//! Properties on top, as front matter; below them the same skeleton a template
//! has — `<article>` with `<name>` and `<body>` inside. What a type sets out,
//! a document of it fills in: `view`/`edit` on `<article>` are the document's
//! own access, `<name>` its name, and the body holds `<block>` for a section.
//! Access works the same everywhere: any element may say `view=` and
//! `edit=`, and the innermost wins — a `<block>` inside a block is a region
//! of its text, an `<image view=gm>` an element only some may see. Every
//! document is built this way — the world's page and a type's page too.
//!
//! ```text
//! ---
//! Lies in: @@Place:France@@
//! Climate: @@Climate:warm@@
//! Mayor: Vellmar
//! ---
//!
//! <article view=players edit=gm>
//! <name=Paris>
//! <body>
//! <block title="History">
//! Everyone knows this.
//!
//! <block view=gm edit=gm>
//! Only the GM knows this.
//! </block>
//! <image size=300x200 view=gm edit=gm>
//! </block>
//! </body>
//! </article>
//! ```
//!
//! Nothing is indented, unlike in a template: in Markdown four spaces make a
//! code block.
//!
//! `<cover alias="A stranger" view=all edit=gm>` at the top of the body is
//! what those who may not open the document know of it: links to it lead
//! them to an empty page under the alias — or under its name where there is
//! none. Whoever `view=` leaves out gets no link at all.
//!
//! **This is only a translation.** What is stored stays the layers; every
//! part with access of its own is a layer of its own with its own rule.
//! Which layer a part becomes, and whether the reader may change it, is
//! decided when saving — where the world is held. The text only ever
//! contains what the reader may see.
//!
//! Property values are text here. `@@Type:Name@@` names a document (see
//! [`crate::link`]); resolving it to an id happens on saving, where it is
//! known which documents exist.

use serde::{Deserialize, Serialize};

use crate::editor::LineError;
use crate::link::{self, Link};
use crate::template::{Access, TemplateBlock};
use crate::template_text::{
    self, access_on, block_tag, element_with_access, image_line, image_with, read_access, read_tag, value, Directory,
};

/// One property: a name and its value as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    pub value: String,
    /// The groups it lies in, outermost first — the code writes it under
    /// their names, indented once per group. Only for the eye: a property is
    /// found by its name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group: Vec<String>,
}

/// What `<cover>` says: the name under which those who may not open the
/// document know it, and who may follow links to it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cover {
    /// Empty: the document's name.
    #[serde(default)]
    pub alias: String,
    pub access: Access,
}

impl Cover {
    /// Whoever may follow links to a document under cover by default: everybody.
    pub fn open() -> Access {
        Access { view: "all".into(), edit: "gm".into() }
    }
}

/// A document in the shape its code has.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentText {
    /// What `<name>` says.
    #[serde(default)]
    pub name: String,
    /// What `<article>` says: who sees the document, and what holds for
    /// every part that says nothing of its own.
    #[serde(default)]
    pub access: Access,
    /// What `<infobox view=… edit=…>` says: who sees and changes the
    /// property values. `None`: as the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infobox: Option<Access>,
    /// What `<cover>` says — `None` where the document has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<Cover>,
    pub properties: Vec<Property>,
    pub blocks: Vec<TemplateBlock>,
}

/// What reading a document's code gives.
#[derive(Serialize, Debug)]
pub struct DocumentRead {
    /// What was understood — even with errors, so the other side keeps up.
    pub document: DocumentText,
    pub errors: Vec<LineError>,
}

/// How a document is written.
#[derive(Clone, Debug)]
pub struct Shape {
    /// The names of the properties its type declares — even those this
    /// reader cannot open. Text may use them all.
    pub declared: Vec<String>,
    /// The document's access as stored — what the bracket falls back to for
    /// whatever it leaves out.
    pub defaults: Access,
}

const FENCE: &str = "---";

/// Writes a document as code.
pub fn to_code(doc: &DocumentText) -> String {
    let joined = joined(doc);
    let doc = &joined;
    let mut out = String::new();
    if !doc.properties.is_empty() {
        out.push_str(FENCE);
        out.push('\n');
        let mut open: &[String] = &[];
        for p in &doc.properties {
            // A group's properties stand together, under its name — each
            // group that begins here gets its heading.
            let same = open.iter().zip(&p.group).take_while(|(a, b)| a == b).count();
            for (depth, name) in p.group.iter().enumerate().skip(same) {
                out.push_str(&format!("{}{name}:\n", "  ".repeat(depth)));
            }
            open = &p.group;
            let indent = "  ".repeat(open.len());
            if p.value.is_empty() {
                out.push_str(&format!("{indent}{}:\n", p.name));
            } else {
                out.push_str(&format!("{indent}{}: {}\n", p.name, p.value));
            }
        }
        out.push_str(FENCE);
        out.push_str("\n\n");
    }

    out.push_str(&access_on("article", &doc.access));
    out.push_str(&format!("\n<name={}>\n", value(&doc.name)));
    out.push_str("<body>\n");
    if let Some(cover) = &doc.cover {
        out.push_str(&cover_line(cover));
        out.push_str("\n\n");
    }
    // The infobox is laid out by the type; the document only says who may
    // see and change it — where that differs.
    if let Some(infobox) = doc.infobox.as_ref().filter(|a| **a != doc.access) {
        out.push_str(&access_on("infobox", infobox));
        out.push_str("\n\n");
    }
    write_body(&mut out, doc);
    out.push_str("</body>\n</article>\n");
    out
}

/// What stands between `<body>` and `</body>`.
fn write_body(out: &mut String, doc: &DocumentText) {
    // Whatever says nothing of its own takes the article's access — so a
    // document does not repeat its own visibility in every section.
    let defaults = &doc.access;
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // Access that holds for the whole block stands on its tag — the way
        // anyone would write it. Only a change inside is written inside.
        let first = block.parts.first().map(access_of).unwrap_or_else(|| defaults.clone());
        let uniform = block.parts.iter().all(|p| access_of(p) == first);
        let own = (uniform && first != *defaults).then_some(&first);
        out.push_str(&block_tag(block, own));
        out.push('\n');
        let applies = if uniform { first } else { defaults.clone() };
        write_parts(out, block, &applies);
        out.push_str("</block>\n");
    }
}

fn access_of(part: &crate::template::BlockPart) -> Access {
    Access {
        view: part.view.clone(),
        edit: part.edit.clone(),
    }
}

/// The parts of a block — one that differs from `applies` says so: a single
/// element on itself, anything else in an inner `<block>`.
///
/// No indent: in Markdown four spaces make a code block.
fn write_parts(out: &mut String, block: &TemplateBlock, applies: &Access) {
    for (i, part) in block.parts.iter().enumerate() {
        if i > 0 && !out.ends_with("\n\n") {
            out.push('\n');
        }
        let own = access_of(part);
        let text = part.text.trim_end();
        // Two parts of the same access one after the other — a part between
        // them this reader cannot open: the second stands apart, or they
        // would read back as one.
        let after_plain = i > 0 && access_of(&block.parts[i - 1]) == *applies;
        if own == *applies && !after_plain {
            if !text.is_empty() {
                out.push_str(text);
                out.push('\n');
            }
        } else if let Some(element) = element_with_access(text, &own) {
            out.push_str(&element);
            out.push('\n');
        } else {
            out.push_str(&access_on("block", &own));
            out.push('\n');
            if !text.is_empty() {
                out.push_str(text);
                out.push('\n');
            }
            out.push_str("</block>\n");
        }
    }
}

/// Reads a document's code.
///
/// `previous` is the state before this change. The blocks take their ids from
/// it — a section keeps its layer when its title changes or it moves.
pub fn from_code(text: &str, previous: &DocumentText, shape: &Shape) -> DocumentRead {
    let mut errors: Vec<LineError> = Vec::new();
    let (properties, body, body_line) = split_properties(text, &mut errors);
    let skeleton = split_skeleton(&body, body_line, &shape.defaults, &mut errors);
    let name = skeleton.name.unwrap_or_else(|| previous.name.clone());
    let access = skeleton.access.unwrap_or_else(|| shape.defaults.clone());
    let (cover, inner) = cover_of(&skeleton.inner, skeleton.line, &mut errors);
    let (infobox, inner) = infobox_access(&inner, skeleton.line, &access, &mut errors);

    // The body is read by the template reader. It is wrapped first in the
    // skeleton with the article's access, so a part without access of its
    // own gets the document's.
    let wrapped = format!(
        "{}\n<name>\n<body>\n{}\n</body>\n</article>\n",
        access_on("article", &access),
        inner
    );
    let extra = 3;

    let parsed = template_text::from_text(&wrapped, &Directory::default());
    for e in parsed.errors {
        // Lines of the wrapping do not exist for whoever typed.
        let line = if e.line > extra { e.line - extra + skeleton.line - 1 } else { 0 };
        errors.push(LineError { line, text: e.text });
    }
    if !parsed.infobox.show.is_empty() {
        errors.push(LineError {
            line: 0,
            text: "A document has no <infobox> of its own — its type lays it out.".into(),
        });
    }
    // An element may only bind, and %%…%% only read, what the document has.
    for used in &parsed.uses {
        let line = if used.line > extra { used.line - extra + skeleton.line - 1 } else { 0 };
        let known = properties.iter().map(|p| &p.name).chain(&shape.declared);
        let name = used.element.is_none() && used.property.trim().eq_ignore_ascii_case(crate::types::NAME);
        if !name && !known.into_iter().any(|n| n.eq_ignore_ascii_case(&used.property)) {
            errors.push(LineError { line, text: format!("There is no property “{}” — it goes on top, between ---.", used.property) });
        }
    }

    let mut blocks = parsed.blocks;
    template_text::assign_block_ids(&mut blocks, &previous.blocks);
    split_stand_ins(&mut blocks);
    split_secret_pins(&mut blocks, &previous.blocks);
    errors.sort_by_key(|e| e.line);

    DocumentRead {
        document: DocumentText { name, access, infobox, cover, properties, blocks },
        errors,
    }
}

/// Takes `<infobox view=… edit=…>` out of the body — the infobox's own
/// access — and leaves its line empty, so the lines below keep their numbers.
/// The same as the document's is none of its own.
fn infobox_access(inner: &str, first: usize, document: &Access, errors: &mut Vec<LineError>) -> (Option<Access>, String) {
    let mut found = None;
    let lines: Vec<&str> = inner
        .lines()
        .enumerate()
        .map(|(i, line)| match line_tag(line).filter(|t| t.name == "infobox") {
            Some(tag) if tag.closing => "",
            Some(tag) => {
                if tag.attributes.iter().any(|(n, _)| !ACCESS.contains(&n.as_str())) {
                    errors.push(LineError { line: first + i, text: "A document's <infobox> only says who sees and edits it — its type lays it out.".into() });
                }
                match read_access(&tag, document) {
                    Ok(access) => found = Some(access),
                    Err(text) => errors.push(LineError { line: first + i, text }),
                }
                ""
            }
            None => line,
        })
        .collect();
    (found.filter(|a| a != document), lines.join("\n"))
}

/// `<cover alias=… view=… edit=…>`.
fn cover_line(cover: &Cover) -> String {
    let alias = if cover.alias.is_empty() { String::new() } else { format!(" alias={}", value(&cover.alias)) };
    format!("<cover{alias} view={} edit={}>", value(&cover.access.view), value(&cover.access.edit))
}

/// Takes `<cover …>` out of the body and leaves its line empty, as
/// `infobox_access` does.
fn cover_of(inner: &str, first: usize, errors: &mut Vec<LineError>) -> (Option<Cover>, String) {
    let mut found = None;
    let lines: Vec<&str> = inner
        .lines()
        .enumerate()
        .map(|(i, line)| match line_tag(line).filter(|t| t.name == "cover") {
            Some(tag) if tag.closing => "",
            Some(tag) => {
                if tag.attributes.iter().any(|(n, _)| n != "alias" && !ACCESS.contains(&n.as_str())) {
                    errors.push(LineError { line: first + i, text: "<cover> says an alias and who may follow links: alias=… view=… edit=…".into() });
                }
                match read_access(&tag, &Cover::open()) {
                    Ok(access) => found = Some(Cover { alias: tag.attr("alias").unwrap_or_default().trim().to_string(), access }),
                    Err(text) => errors.push(LineError { line: first + i, text }),
                }
                ""
            }
            None => line,
        })
        .collect();
    (found, lines.join("\n"))
}

// --- Secret pins -------------------------------------------------------------
//
// A pin or region on a map may say who sees it: `<pin at=… view=gm>`. The map
// stands in a part of the text, and what is in a part, everyone who opens it
// has — so such pins leave it when saved. Those with the same access go
// together into a part of their own at the end of the block, as
// `<pins for=img:…>` with the map's picture to find it by. The reader's page
// puts them back onto that map; whoever cannot open the part never had them.
// The GM reveals a pin by giving it wider access: it moves to the part that
// access has — onto the map itself once it is the map's own.

const PINS: &str = "pins";
const ACCESS: [&str; 2] = ["view", "edit"];

/// A line's tag, if it is one on a line of its own.
fn line_tag(line: &str) -> Option<template_text::Tag> {
    let raw = line.trim().strip_prefix('<')?.strip_suffix('>')?;
    (!raw.contains(['<', '>'])).then(|| read_tag(raw, 0))
}

/// Whether a part holds nothing but pins set apart — `<pins for=…>` sections.
fn pins_sections(text: &str) -> Option<Vec<(String, Vec<String>)>> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut open = false;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let tag = line_tag(line)?;
        match (tag.name.as_str(), tag.closing, open) {
            (PINS, false, false) => {
                out.push((tag.attr("for").unwrap_or_default().to_string(), Vec::new()));
                open = true;
            }
            (PINS, true, true) => open = false,
            ("pin" | "area", false, true) => out.last_mut()?.1.push(line.trim().to_string()),
            _ => return None,
        }
    }
    (!out.is_empty() && !open).then_some(out)
}

/// Whether a text has a pin or region on a map with access of its own.
pub(crate) fn secret_pins_in(text: &str) -> bool {
    let mut inside = false;
    text.lines().filter_map(line_tag).any(|t| {
        if t.name == "map" {
            inside = !t.closing;
        }
        inside && matches!(t.name.as_str(), "pin" | "area") && ACCESS.iter().any(|n| t.attr(n).is_some())
    })
}

/// Takes the pins with access of their own off the maps and puts them into
/// parts of their own — one per access, at the end of their block. A part
/// that held such pins before keeps its key where its access is the same.
fn split_secret_pins(blocks: &mut [TemplateBlock], previous: &[TemplateBlock]) {
    for block in blocks.iter_mut() {
        let mut groups: Vec<(Access, Vec<(String, Vec<String>)>)> = Vec::new();
        for part in block.parts.iter_mut() {
            let own = access_of(part);
            let mut map: Option<String> = None;
            let mut changed = false;
            let mut lines: Vec<String> = Vec::new();
            for line in part.text.lines() {
                if let Some(tag) = line_tag(line) {
                    match (tag.name.as_str(), tag.closing) {
                        ("map", false) => map = Some(tag.attr("src").unwrap_or_default().to_string()),
                        ("map", true) => map = None,
                        ("pin" | "area", false) if map.is_some() && ACCESS.iter().any(|n| tag.attr(n).is_some()) => {
                            changed = true;
                            let access = read_access(&tag, &own).unwrap_or_else(|_| own.clone());
                            let mut bare = tag.clone();
                            bare.attributes.retain(|(n, _)| !ACCESS.contains(&n.as_str()));
                            let bare = template_text::tag_back(&bare);
                            if access == own {
                                lines.push(bare);
                                continue;
                            }
                            let src = map.clone().unwrap_or_default();
                            let at = match groups.iter().position(|(a, _)| *a == access) {
                                Some(at) => at,
                                None => {
                                    groups.push((access, Vec::new()));
                                    groups.len() - 1
                                }
                            };
                            let maps = &mut groups[at].1;
                            match maps.iter_mut().find(|(s, _)| *s == src) {
                                Some((_, pins)) => pins.push(bare),
                                None => maps.push((src, vec![bare])),
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                lines.push(line.to_string());
            }
            if changed {
                part.text = lines.join("\n");
            }
        }
        if groups.is_empty() {
            continue;
        }
        let before = previous.iter().find(|b| b.key == block.key);
        let mut taken: std::collections::BTreeSet<String> = block
            .parts
            .iter()
            .chain(before.into_iter().flat_map(|b| b.parts.iter()))
            .map(|p| p.key.clone())
            .collect();
        let mut reused: std::collections::BTreeSet<String> = Default::default();
        for (access, maps) in groups {
            let text = maps
                .iter()
                .map(|(src, pins)| format!("<{PINS} for={}>\n{}\n</{PINS}>", value(src), pins.join("\n")))
                .collect::<Vec<_>>()
                .join("\n");
            // The same access as before: the same part, so its layer stays.
            let kept = before
                .into_iter()
                .flat_map(|b| b.parts.iter())
                .find(|p| {
                    access_of(p) == access
                        && pins_sections(&p.text).is_some()
                        && !reused.contains(&p.key)
                        && !block.parts.iter().any(|q| q.key == p.key)
                })
                .map(|p| p.key.clone());
            let key = kept.unwrap_or_else(|| {
                let n = (0..).find(|n| !taken.contains(&TemplateBlock::part_key(&block.key, *n))).unwrap_or(0);
                TemplateBlock::part_key(&block.key, n)
            });
            taken.insert(key.clone());
            reused.insert(key.clone());
            block.parts.push(crate::template::BlockPart { key, text, view: access.view, edit: access.edit });
        }
    }
}

/// The other way round, for the code and the visual side: the pins set
/// apart go back onto their map, each with its access on itself. Where the
/// map is not here to be read, they stay as they are.
fn join_secret_pins(doc: &mut DocumentText) {
    for block in &mut doc.blocks {
        let maps: Vec<String> = block
            .parts
            .iter()
            .flat_map(|p| p.text.lines())
            .filter_map(line_tag)
            .filter(|t| t.name == "map" && !t.closing)
            .map(|t| t.attr("src").unwrap_or_default().to_string())
            .collect();
        let mut found: Vec<(String, Vec<String>)> = Vec::new();
        block.parts.retain(|part| {
            let Some(sections) = pins_sections(&part.text) else { return true };
            if !sections.iter().all(|(src, _)| maps.contains(src)) {
                return true;
            }
            let access = access_of(part);
            for (src, pins) in sections {
                let written: Vec<String> =
                    pins.iter().map(|p| element_with_access(p, &access).unwrap_or_else(|| p.clone())).collect();
                match found.iter_mut().find(|(s, _)| *s == src) {
                    Some((_, all)) => all.extend(written),
                    None => found.push((src, written)),
                }
            }
            false
        });
        for (src, pins) in found {
            // Onto the first map with that picture, before its end.
            for part in &mut block.parts {
                let mut lines: Vec<String> = Vec::new();
                let mut inside = false;
                let mut placed = false;
                for line in part.text.lines() {
                    match line_tag(line) {
                        Some(t) if t.name == "map" && !t.closing => inside = t.attr("src").unwrap_or_default() == src,
                        Some(t) if t.name == "map" && t.closing && inside && !placed => {
                            lines.extend(pins.iter().cloned());
                            inside = false;
                            placed = true;
                        }
                        _ => {}
                    }
                    lines.push(line.to_string());
                }
                if placed {
                    part.text = lines.join("\n");
                    break;
                }
            }
        }
    }
}

/// What the code and the visual side show: images with their stand-ins,
/// maps with all their pins — as far as this reader may read them.
pub fn joined(doc: &DocumentText) -> DocumentText {
    let mut out = join_stand_ins(doc);
    join_secret_pins(&mut out);
    out
}

// --- Stand-ins ---------------------------------------------------------------
//
// An image only some may see can name what the others see in its place:
// `<image src=img:aldric view=gm hidden=img:silhouette hidden-caption="A
// stranger">`. The real image lies in its part, with its access; the
// stand-in has to be readable by the others, so it goes into the nearest
// part they may read — as `<image src=img:silhouette caption="A stranger"
// for=<part>>`, shown only to whoever cannot open that part. In the code
// whoever can read both sees them as one tag again.

const HIDDEN: &str = "hidden";
const HIDDEN_CAPTION: &str = "hidden-caption";
/// What a stand-in takes over from its image: where and how large it stands.
const PLACE: [&str; 3] = ["align", "width", "size"];

/// Takes `hidden=` and `hidden-caption=` off the images of each part and
/// puts the stand-ins where the others can read them. Nothing is added as
/// a part of its own — the parts keep their places, and with them their
/// layers.
fn split_stand_ins(blocks: &mut [TemplateBlock]) {
    let order: Vec<(usize, usize)> =
        blocks.iter().enumerate().flat_map(|(b, block)| (0..block.parts.len()).map(move |p| (b, p))).collect();
    for (at, &(b, p)) in order.iter().enumerate() {
        let key = blocks[b].parts[p].key.clone();
        let mut stand_ins = Vec::new();
        let lines: Vec<String> = blocks[b].parts[p]
            .text
            .lines()
            .map(|line| {
                let Some(attributes) = image_line(line) else { return line.to_string() };
                let hidden = attributes.iter().find(|(n, _)| n == HIDDEN).map(|(_, v)| v.clone());
                let caption = attributes.iter().find(|(n, _)| n == HIDDEN_CAPTION).map(|(_, v)| v.clone());
                if hidden.is_none() && caption.is_none() {
                    return line.to_string();
                }
                let mut stand_in = Vec::new();
                if let Some(src) = hidden.filter(|s| !s.is_empty()) {
                    stand_in.push(("src".to_string(), src));
                }
                if let Some(caption) = caption.filter(|c| !c.is_empty()) {
                    stand_in.push(("caption".to_string(), caption));
                }
                stand_in.extend(attributes.iter().filter(|(n, _)| PLACE.contains(&n.as_str())).cloned());
                stand_in.push(("for".to_string(), key.clone()));
                stand_ins.push(image_with(&stand_in));
                let kept: Vec<(String, String)> =
                    attributes.into_iter().filter(|(n, _)| n != HIDDEN && n != HIDDEN_CAPTION).collect();
                image_with(&kept)
            })
            .collect();
        if stand_ins.is_empty() {
            continue;
        }
        blocks[b].parts[p].text = lines.join("\n");
        let own = access_of(&blocks[b].parts[p]);
        let other = |&(ob, op): &(usize, usize)| access_of(&blocks[ob].parts[op]) != own;
        // The nearest part others may read: before it, else after it.
        let before = order[..at].iter().rev().find(|x| other(x)).copied();
        let after = order[at + 1..].iter().find(|x| other(x)).copied();
        match (before, after) {
            (Some((hb, hp)), _) => {
                let host = &mut blocks[hb].parts[hp].text;
                for line in stand_ins {
                    if !host.is_empty() && !host.ends_with('\n') {
                        host.push('\n');
                    }
                    host.push_str(&line);
                }
            }
            (None, Some((hb, hp))) => {
                let host = &mut blocks[hb].parts[hp].text;
                *host = format!("{}\n{}", stand_ins.join("\n"), host);
            }
            // Nobody else reads this document: there is nobody to stand in for.
            (None, None) => {}
        }
    }
}

/// The other way round, for the code and the visual side: each stand-in
/// goes back onto its image as `hidden=` and `hidden-caption=` — where the
/// part it stands for is here to be read. Where it is not, the stand-in
/// stays as it is.
pub fn join_stand_ins(doc: &DocumentText) -> DocumentText {
    let mut out = doc.clone();
    let keys: std::collections::HashSet<String> =
        doc.blocks.iter().flat_map(|b| b.parts.iter().map(|p| p.key.clone())).collect();
    let mut found: std::collections::HashMap<String, Vec<Vec<(String, String)>>> = Default::default();
    for block in &mut out.blocks {
        for part in &mut block.parts {
            let mut kept = Vec::new();
            for line in part.text.lines() {
                let target = image_line(line).and_then(|a| {
                    let target = a.iter().find(|(n, _)| n == "for").map(|(_, v)| v.clone())?;
                    keys.contains(&target).then_some((target, a))
                });
                match target {
                    Some((target, attributes)) => found.entry(target).or_default().push(attributes),
                    None => kept.push(line),
                }
            }
            part.text = kept.join("\n");
        }
    }
    for block in &mut out.blocks {
        for part in &mut block.parts {
            let Some(stand_ins) = found.get_mut(&part.key) else { continue };
            stand_ins.reverse();
            let lines: Vec<String> = part
                .text
                .lines()
                .map(|line| {
                    let Some(mut attributes) = image_line(line) else { return line.to_string() };
                    let Some(stand_in) = stand_ins.pop() else { return line.to_string() };
                    for (name, value) in stand_in {
                        match name.as_str() {
                            "src" => attributes.push((HIDDEN.into(), value)),
                            "caption" => attributes.push((HIDDEN_CAPTION.into(), value)),
                            _ => {}
                        }
                    }
                    image_with(&attributes)
                })
                .collect();
            part.text = lines.join("\n");
        }
    }
    out
}

/// What the skeleton around the body says.
struct Skeleton {
    name: Option<String>,
    access: Option<Access>,
    /// What stands between `<body>` and `</body>`.
    inner: String,
    /// The line `inner` starts on (1-based).
    line: usize,
}

/// Takes `<article>`, `<name>` and `<body>` off the body.
///
/// Each stands on a line of their own, the way they are written. Whatever is
/// missing is an error — the skeleton can be changed but not left out — and
/// whatever was understood is still returned, so the visual side keeps up.
fn split_skeleton(body: &str, first: usize, defaults: &Access, errors: &mut Vec<LineError>) -> Skeleton {
    let lines: Vec<&str> = body.lines().collect();
    let is = |line: &str, tag: &str| line.trim().eq_ignore_ascii_case(tag);
    let open = lines.iter().position(|l| is(l, "<body>"));
    let close = lines.iter().rposition(|l| is(l, "</body>"));
    let (Some(open), Some(close)) = (open, close) else {
        errors.push(LineError {
            line: 0,
            text: "<body> and </body> are missing — the document's text stands between them.".into(),
        });
        return Skeleton { name: None, access: None, inner: body.to_string(), line: first };
    };
    if close < open {
        errors.push(LineError {
            line: first + close,
            text: "</body> stands before <body>.".into(),
        });
        return Skeleton { name: None, access: None, inner: String::new(), line: first };
    }

    let mut out = Skeleton {
        name: None,
        access: None,
        inner: lines[open + 1..close].join("\n"),
        line: first + open + 1,
    };
    let mut seen: Vec<&str> = Vec::new();
    let outside = lines[..open].iter().enumerate().chain(lines[close + 1..].iter().enumerate().map(|(i, l)| (i + close + 1, l)));
    for (i, line) in outside {
        let number = first + i;
        let bare = line.trim();
        if bare.is_empty() || (bare.starts_with("<!--") && bare.ends_with("-->")) {
            continue;
        }
        let Some(raw) = bare.strip_prefix('<').and_then(|b| b.strip_suffix('>')) else {
            errors.push(LineError {
                line: number,
                text: format!("“{bare}” stands outside the body. Text goes between <body> and </body>."),
            });
            continue;
        };
        let tag = read_tag(raw, number);
        let piece = match (tag.name.as_str(), tag.closing, i < open) {
            ("article", false, true) => {
                match read_access(&tag, defaults) {
                    Ok(access) => out.access = Some(access),
                    Err(text) => errors.push(LineError { line: number, text }),
                }
                "<article>"
            }
            ("name", false, true) => {
                match tag.value.as_deref().map(str::trim) {
                    Some(name) if !name.is_empty() => out.name = Some(name.to_string()),
                    _ => errors.push(LineError {
                        line: number,
                        text: "The document needs a name: <name=\"…\">.".into(),
                    }),
                }
                "<name>"
            }
            ("article", true, false) => "</article>",
            _ => {
                errors.push(LineError {
                    line: number,
                    text: format!("{bare} does not belong here. Everything but the skeleton goes between <body> and </body>."),
                });
                continue;
            }
        };
        if seen.contains(&piece) {
            errors.push(LineError { line: number, text: format!("{piece} appears twice.") });
        }
        seen.push(piece);
    }
    let missing: Vec<&str> = ["<article>", "<name>", "</article>"]
        .into_iter()
        .filter(|p| !seen.contains(p))
        .collect();
    if !missing.is_empty() {
        errors.push(LineError {
            line: 0,
            text: format!("The skeleton is missing {} — it belongs around every document.", missing.join(", ")),
        });
    }
    out
}

/// Splits `--- … ---` off the top.
///
/// Returns the properties, the body and the line the body starts on (1-based).
fn split_properties(text: &str, errors: &mut Vec<LineError>) -> (Vec<Property>, String, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let Some(start) = first.filter(|i| lines[*i].trim() == FENCE) else {
        return (Vec::new(), text.to_string(), 1);
    };
    let Some(end) = lines[start + 1..].iter().position(|l| l.trim() == FENCE).map(|p| p + start + 1)
    else {
        errors.push(LineError {
            line: start + 1,
            text: "The properties are not closed — a line with --- is missing below them.".into(),
        });
        return (Vec::new(), text.to_string(), 1);
    };

    let mut properties: Vec<Property> = Vec::new();
    let inner = &lines[start + 1..end];
    let depth = |l: &str| l.len() - l.trim_start().len();
    // A name without a value, with lines indented deeper below it, heads a
    // group; the groups open are those indented less than the line.
    let mut open: Vec<(usize, String)> = Vec::new();
    for (i, line) in inner.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let number = start + 2 + i;
        let here = depth(line);
        while open.last().is_some_and(|(d, _)| *d >= here) {
            open.pop();
        }
        match line.split_once(':') {
            Some((name, value)) if !name.trim().is_empty() => {
                let name = name.trim().to_string();
                let next = inner[i + 1..].iter().find(|l| !l.trim().is_empty());
                if value.trim().is_empty() && next.is_some_and(|l| depth(l) > here) {
                    open.push((here, name));
                    continue;
                }
                let group: Vec<String> = open.iter().map(|(_, n)| n.clone()).collect();
                if properties.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                    errors.push(LineError {
                        line: number,
                        text: format!("“{name}” appears twice."),
                    });
                    continue;
                }
                properties.push(Property {
                    name,
                    value: unquote(value.trim()).to_string(),
                    group,
                });
            }
            _ => errors.push(LineError {
                line: number,
                text: format!("“{}” is no property — write Name: value.", line.trim()),
            }),
        }
    }
    let body = lines[end + 1..].join("\n");
    (properties, body, end + 2)
}

/// `"warm"` and `warm` are the same value — quotes are only needed for YAML
/// habits, not by us.
fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

/// The documents a value refers to: `@@Place:France@@, @@Place:Spain@@`.
///
/// A value without links names documents by its plain text — whoever types
/// `Climate: warm` means the document “warm” as well.
pub fn names_in(value: &str) -> Vec<Link> {
    let out = link::links(value);
    if !out.is_empty() {
        return out;
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| Link::to("", v))
        .collect()
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
