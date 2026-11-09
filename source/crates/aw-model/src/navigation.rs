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
#[path = "navigation_tests.rs"]
mod tests;
