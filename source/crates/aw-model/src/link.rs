//! Links between documents: `@@Species:Elves@@`.
//!
//! `@@` opens a link, a second `@@` closes it. The type stands in front,
//! divided by a colon — it says which “Elves” is meant when a species and a
//! language share a name, and it is what one filters by while typing. After a
//! `|` follows the text shown in its place:
//!
//! ```text
//! She came in the autumn of 412 to @@Place:New Morningstone@@.
//! He is an @@Species:Elves|Elf@@ and speaks @@Language:High Elven@@.
//! ```
//!
//! After a `#` follows a property of the target — then the link shows its
//! value: `Tim is @@Character:Tim#Status@@.` After `#^` follows a mark in
//! the target's text instead (`^2f9e17` closing a paragraph, see
//! [`crate::markdown`]) — the link leads there. Without a name it is a
//! place in the same document: `@@#^2f9e17|the inn@@`.
//!
//! The type may be left out: `@@Morningstone@@`. Doubled, like `%%value%%`:
//! a doubled sign is markup, a single one is text — `mail@example.org` never
//! needs a rule of its own.

use serde::{Deserialize, Serialize};

/// What opens and closes a link.
pub const MARK: &str = "@@";

/// One link as it stands in the text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The name of the target's type — empty when the link does not say.
    pub of: String,
    /// The name of the target.
    pub name: String,
    /// A property of the target, after `#` — empty for the target itself.
    /// Starting with `^`, a mark in its text instead (see [`Link::mark`]).
    pub property: String,
    /// What is shown — the name, unless the link says otherwise after `|`.
    pub text: String,
}

impl Link {
    /// A link to a document, as written without anything further.
    pub fn to(of: &str, name: &str) -> Self {
        Link { of: of.into(), name: name.into(), property: String::new(), text: name.into() }
    }

    /// The mark in the target's text it leads to — `#^2f9e17`.
    pub fn mark(&self) -> Option<&str> {
        self.property.strip_prefix('^').filter(|m| !m.is_empty())
    }
}

/// A piece of text: plain, or a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    Link(Link),
}

/// Writes a link.
pub fn write(of: &str, name: &str) -> String {
    if of.is_empty() && name.contains(':') {
        // A name holding a colon would otherwise read as type and name; the
        // empty type in front keeps it whole.
        format!("{MARK}:{name}{MARK}")
    } else if of.is_empty() {
        format!("{MARK}{name}{MARK}")
    } else {
        format!("{MARK}{of}:{name}{MARK}")
    }
}

/// Writes a link with a property and text of its own — each where given.
pub fn write_full(of: &str, name: &str, property: &str, shown: &str) -> String {
    let plain = write(of, name);
    let mut out = plain[..plain.len() - MARK.len()].to_string();
    if !property.is_empty() {
        out.push('#');
        out.push_str(property);
    }
    if !shown.is_empty() && shown != name {
        out.push('|');
        out.push_str(shown);
    }
    out.push_str(MARK);
    out
}

/// The links in a piece of text.
pub fn links(text: &str) -> Vec<Link> {
    scan(text)
        .into_iter()
        .filter_map(|p| match p {
            Piece::Link(l) => Some(l),
            Piece::Text(_) => None,
        })
        .collect()
}

/// Splits a piece of text into plain text and links.
pub fn scan(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut plain = 0;
    let mut from = 0;
    while let Some(open) = text[from..].find(MARK).map(|i| i + from) {
        let inner_start = open + MARK.len();
        match text[inner_start..].find(MARK).map(|i| i + inner_start) {
            Some(close) => match link_in(&text[inner_start..close]) {
                Some(link) => {
                    if plain < open {
                        out.push(Piece::Text(&text[plain..open]));
                    }
                    out.push(Piece::Link(link));
                    plain = close + MARK.len();
                    from = plain;
                }
                // Not a link — the second mark may open the next one.
                None => from = close,
            },
            None => break,
        }
    }
    if plain < text.len() {
        out.push(Piece::Text(&text[plain..]));
    }
    out
}

/// What stands between the marks, if it is a link.
fn link_in(inner: &str) -> Option<Link> {
    if inner.contains('\n') {
        return None;
    }
    let (target, text) = match inner.split_once('|') {
        Some((t, d)) => (t, d.trim()),
        None => (inner, ""),
    };
    let (target, property) = match target.rsplit_once('#') {
        Some((t, p)) => (t, p.trim()),
        None => (target, ""),
    };
    let (of, name) = match target.split_once(':') {
        Some((of, name)) => (of.trim(), name.trim()),
        None => ("", target.trim()),
    };
    // Without a name only a mark in the same document.
    if name.is_empty() && !(of.is_empty() && property.len() > 1 && property.starts_with('^')) {
        return None;
    }
    let text = if !text.is_empty() { text } else if name.is_empty() { &property[1..] } else { name };
    Some(Link { of: of.into(), name: name.into(), property: property.into(), text: text.into() })
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
