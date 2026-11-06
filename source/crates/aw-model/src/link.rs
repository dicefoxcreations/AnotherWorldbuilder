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
mod tests {
    use super::*;

    fn link(of: &str, name: &str) -> Link {
        Link::to(of, name)
    }

    #[test]
    fn a_link_names_type_and_document() {
        assert_eq!(
            scan("He is an @@Species:Elves@@."),
            vec![Piece::Text("He is an "), Piece::Link(link("Species", "Elves")), Piece::Text(".")]
        );
    }

    #[test]
    fn the_type_may_be_left_out() {
        assert_eq!(links("to @@Morningstone@@, then"), vec![link("", "Morningstone")]);
    }

    #[test]
    fn spaces_need_nothing_extra() {
        assert_eq!(links("He speaks @@Language:High Elven@@."), vec![link("Language", "High Elven")]);
    }

    #[test]
    fn a_link_may_show_other_text() {
        assert_eq!(
            links("He is an @@Species:Elves|Elf@@."),
            vec![Link { text: "Elf".into(), ..link("Species", "Elves") }]
        );
    }

    #[test]
    fn a_single_sign_is_text() {
        assert!(links("write to mail@example.org").is_empty());
        assert!(links("@Elves@").is_empty());
        assert!(links("@@not closed").is_empty());
        assert!(links("@@Elves\n@@").is_empty(), "a link stays on its line");
    }

    #[test]
    fn a_link_may_lead_to_a_mark_here_or_in_another_document() {
        let here = links("as @@#^2f9e17|the inn@@ said");
        assert_eq!(here, vec![Link { of: "".into(), name: "".into(), property: "^2f9e17".into(), text: "the inn".into() }]);
        assert_eq!(here[0].mark(), Some("2f9e17"));
        assert_eq!(links("@@Session log:Session 1#^c70ff9@@")[0].mark(), Some("c70ff9"));
        // Nothing but a property, or nothing at all, is no link.
        assert!(links("@@#Status@@ @@#^@@ @@|x@@").is_empty());
        assert_eq!(write_full("", "", "^2f9e17", "the inn"), "@@#^2f9e17|the inn@@");
    }

    #[test]
    fn a_link_may_name_a_property() {
        assert_eq!(
            links("Tim is @@Character:Tim#Status@@."),
            vec![Link { property: "Status".into(), ..link("Character", "Tim") }]
        );
        assert_eq!(write_full("Character", "Tim", "Status", "state"), "@@Character:Tim#Status|state@@");
        assert_eq!(links("@@Character:Tim#Status|state@@")[0].text, "state");
    }

    #[test]
    fn two_links_on_one_line() {
        assert_eq!(links("@@Elves@@ and @@Dwarves@@"), vec![link("", "Elves"), link("", "Dwarves")]);
    }

    #[test]
    fn written_links_read_back() {
        for (of, name) in [
            ("Species", "Elf"),
            ("Place", "New York"),
            ("", "Morningstone"),
            ("", "Chapter 1: Start"),
            ("Städte", "Köln"),
        ] {
            let text = write(of, name);
            assert_eq!(links(&text), vec![link(of, name)], "{text}");
        }
        assert_eq!(write("Species", "Elf"), "@@Species:Elf@@");
    }
}
