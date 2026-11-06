//! The world's navigation: what the side bar on the left lists, in the
//! game master's order.
//!
//! An entry is a type (unfolded, its documents), a single document, or a
//! group of entries under a label of its own. A type may hold types too —
//! its subtypes, each with the documents that are theirs: Weapons, and
//! below them Homebrew. Which documents a reader sees
//! under a type is theirs alone — the entry only names the type. An entry
//! marked `gm` is for the game master: players never receive it.

use serde::{Deserialize, Serialize};

/// How deep groups may nest.
pub const MAX_DEPTH: usize = 4;
/// How many entries a navigation may hold in all.
pub const MAX_ENTRIES: usize = 500;
/// How long a label may be, in characters.
pub const MAX_LABEL: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NavKind {
    /// The documents of a type and its subtypes — those of a type entry
    /// below it stand there instead.
    Type,
    /// One document.
    Document,
    /// Entries under a label.
    Group,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavEntry {
    /// Stable within the navigation — what a reader's device keeps folded.
    pub id: String,
    pub kind: NavKind,
    /// The type's or the document's id; empty for a group.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
    /// Empty: the type's or the document's name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Only for the game master.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gm: bool,
    /// A group's entries, or a type's types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NavEntry>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NavError {
    #[error("the navigation holds more than {MAX_ENTRIES} entries")]
    TooMany,
    #[error("groups nest at most {MAX_DEPTH} deep")]
    TooDeep,
    #[error("an entry needs an id, once")]
    Id,
    #[error("a label has at most {MAX_LABEL} characters")]
    LongLabel,
    #[error("a group needs a label")]
    UnnamedGroup,
    #[error("only a group holds entries, and a type only types")]
    Children,
    #[error("a type or document entry needs its target")]
    NoTarget,
}

/// Whether a navigation can be kept as it is.
pub fn check(entries: &[NavEntry]) -> Result<(), NavError> {
    let mut ids = std::collections::HashSet::new();
    fn walk<'a>(entries: &'a [NavEntry], depth: usize, ids: &mut std::collections::HashSet<&'a str>) -> Result<(), NavError> {
        // An entry with nothing below it goes no deeper.
        if depth > MAX_DEPTH && !entries.is_empty() {
            return Err(NavError::TooDeep);
        }
        for e in entries {
            if e.id.is_empty() || !ids.insert(e.id.as_str()) {
                return Err(NavError::Id);
            }
            if ids.len() > MAX_ENTRIES {
                return Err(NavError::TooMany);
            }
            if e.label.chars().count() > MAX_LABEL {
                return Err(NavError::LongLabel);
            }
            match e.kind {
                NavKind::Group if e.label.trim().is_empty() => return Err(NavError::UnnamedGroup),
                NavKind::Group => walk(&e.children, depth + 1, ids)?,
                _ if e.target.is_empty() => return Err(NavError::NoTarget),
                NavKind::Type if e.children.iter().any(|c| c.kind != NavKind::Type) => return Err(NavError::Children),
                NavKind::Type => walk(&e.children, depth + 1, ids)?,
                _ if !e.children.is_empty() => return Err(NavError::Children),
                _ => {}
            }
        }
        Ok(())
    }
    walk(entries, 1, &mut ids)
}

/// What players receive: everything not marked for the game master.
pub fn for_players(entries: &[NavEntry]) -> Vec<NavEntry> {
    entries
        .iter()
        .filter(|e| !e.gm)
        .map(|e| NavEntry { children: for_players(&e.children), ..e.clone() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, kind: NavKind, target: &str, gm: bool, children: Vec<NavEntry>) -> NavEntry {
        let label = if kind == NavKind::Group { id.to_string() } else { String::new() };
        NavEntry { id: id.into(), kind, target: target.into(), label, gm, children }
    }

    #[test]
    fn players_get_no_gm_entries() {
        let nav = vec![
            entry("a", NavKind::Type, "place", false, vec![]),
            entry("b", NavKind::Group, "", false, vec![
                entry("c", NavKind::Type, "npc", true, vec![]),
                entry("d", NavKind::Document, "x", false, vec![]),
            ]),
            entry("e", NavKind::Group, "", true, vec![entry("f", NavKind::Type, "secret", false, vec![])]),
        ];
        assert_eq!(check(&nav), Ok(()));
        let open = for_players(&nav);
        assert_eq!(open.len(), 2);
        assert_eq!(open[1].children.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["d"]);
    }

    #[test]
    fn checks_the_shape() {
        let twice = vec![entry("a", NavKind::Type, "t", false, vec![]), entry("a", NavKind::Type, "u", false, vec![])];
        assert_eq!(check(&twice), Err(NavError::Id));
        let typed_children = vec![entry("a", NavKind::Type, "t", false, vec![entry("b", NavKind::Type, "u", false, vec![])])];
        assert_eq!(check(&typed_children), Ok(()), "a type holds its subtypes");
        let typed_document = vec![entry("a", NavKind::Type, "t", false, vec![entry("b", NavKind::Document, "x", false, vec![])])];
        assert_eq!(check(&typed_document), Err(NavError::Children));
        let document_children = vec![entry("a", NavKind::Document, "x", false, vec![entry("b", NavKind::Type, "u", false, vec![])])];
        assert_eq!(check(&document_children), Err(NavError::Children));
        let mut deep = entry("g5", NavKind::Group, "", false, vec![]);
        for i in (1..5).rev() {
            deep = entry(&format!("g{i}"), NavKind::Group, "", false, vec![deep]);
        }
        assert_eq!(check(&[deep]), Err(NavError::TooDeep));
        let mut deepest = entry("t4", NavKind::Type, "t", false, vec![]);
        for i in (1..4).rev() {
            deepest = entry(&format!("t{i}"), NavKind::Type, "t", false, vec![deepest]);
        }
        assert_eq!(check(&[deepest]), Ok(()), "four deep, the last holding nothing");
        assert_eq!(check(&[entry("a", NavKind::Document, "", false, vec![])]), Err(NavError::NoTarget));
    }
}
