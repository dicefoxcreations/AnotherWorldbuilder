//! Attributes and policies — the language in which access grants are written.
//!
//! An attribute is what a grant hangs on: `class:rogue`, `player:bob`,
//! `players`. A policy is an OR over AND groups:
//!
//! ```text
//! class:rogue OR background:noble
//! class:rogue AND faction:thieves-guild
//! ```
//!
//! NOT does not exist: a rule says who may, never who may not.
//!
//! The canonical notation (`Display`) is what is stored and compared; it
//! must never change casually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

pub const OR_WORD: &str = "OR";
pub const AND_WORD: &str = "AND";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("empty policy — at least one attribute is needed")]
    Empty,

    #[error("'{0}' is not a valid attribute: expected 'kind:value', 'all', 'players' or 'gm'")]
    BadAttribute(String),

    #[error("'{0}' contains characters that are not allowed in attributes")]
    BadCharacters(String),
}

/// A single attribute.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Attribute {
    /// Everyone with any access to the world — including guest bundles.
    All,
    /// All players of the campaign.
    Players,
    /// Game master and co-game masters.
    Gm,
    /// Everything else: `class:rogue`, `player:bob`, `knowledge:traitor`.
    Named { kind: String, value: String },
}

impl Attribute {
    pub fn named(kind: impl Into<String>, value: impl Into<String>) -> Self {
        Attribute::Named {
            kind: kind.into(),
            value: value.into(),
        }
    }

    /// The kind of the attribute, e.g. `class`. For the special attributes
    /// it is their own name.
    pub fn kind(&self) -> &str {
        match self {
            Attribute::All => "all",
            Attribute::Players => "players",
            Attribute::Gm => "gm",
            Attribute::Named { kind, .. } => kind,
        }
    }
}

fn valid_part(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl FromStr for Attribute {
    type Err = PolicyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        match text {
            "all" => return Ok(Attribute::All),
            "players" => return Ok(Attribute::Players),
            "gm" => return Ok(Attribute::Gm),
            _ => {}
        }
        let (kind, value) = text
            .split_once(':')
            .ok_or_else(|| PolicyError::BadAttribute(text.to_string()))?;
        if !valid_part(kind) || !valid_part(value) {
            return Err(PolicyError::BadCharacters(text.to_string()));
        }
        Ok(Attribute::named(kind, value))
    }
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Attribute::All => f.write_str("all"),
            Attribute::Players => f.write_str("players"),
            Attribute::Gm => f.write_str("gm"),
            Attribute::Named { kind, value } => write!(f, "{kind}:{value}"),
        }
    }
}

impl Serialize for Attribute {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Attribute {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// An AND group. Sorted and free of duplicates internally, so equal groups
/// also look equal — and hit the same derived key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Conjunction(Vec<Attribute>);

impl Conjunction {
    pub fn new(attributes: impl IntoIterator<Item = Attribute>) -> Result<Self, PolicyError> {
        let set: BTreeSet<Attribute> = attributes.into_iter().collect();
        if set.is_empty() {
            return Err(PolicyError::Empty);
        }
        Ok(Conjunction(set.into_iter().collect()))
    }

    pub fn single(attribute: Attribute) -> Self {
        Conjunction(vec![attribute])
    }

    pub fn attributes(&self) -> &[Attribute] {
        &self.0
    }

    /// True when *all* attributes of this group are present.
    pub fn satisfied_by(&self, held: &BTreeSet<Attribute>) -> bool {
        self.0.iter().all(|a| held.contains(a))
    }

    pub fn is_single(&self) -> bool {
        self.0.len() == 1
    }
}

impl fmt::Display for Conjunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|a| a.to_string()).collect();
        f.write_str(&parts.join(&format!(" {AND_WORD} ")))
    }
}

/// An access grant: OR over AND groups (disjunctive normal form).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy(Vec<Conjunction>);

impl Policy {
    pub fn new(groups: impl IntoIterator<Item = Conjunction>) -> Result<Self, PolicyError> {
        let set: BTreeSet<Conjunction> = groups.into_iter().collect();
        if set.is_empty() {
            return Err(PolicyError::Empty);
        }

        // Absorption: whoever already gets in through a smaller group does
        // not need the larger one. `a OR (a AND b)` is simply `a`. That keeps
        // the policy readable and saves entries in the layer header.
        let all: Vec<Conjunction> = set.into_iter().collect();
        let needed: Vec<Conjunction> = all
            .iter()
            .filter(|group| {
                !all.iter().any(|other| {
                    other != *group
                        && other
                            .attributes()
                            .iter()
                            .all(|a| group.attributes().contains(a))
                })
            })
            .cloned()
            .collect();

        Ok(Policy(if needed.is_empty() { all } else { needed }))
    }

    /// The most common form: a single attribute.
    pub fn just(attribute: Attribute) -> Self {
        Policy(vec![Conjunction::single(attribute)])
    }


    pub fn groups(&self) -> &[Conjunction] {
        &self.0
    }

    /// True when at least one AND group is fully satisfied.
    pub fn satisfied_by(&self, held: &BTreeSet<Attribute>) -> bool {
        self.0.iter().any(|g| g.satisfied_by(held))
    }

    /// Combines two policies with AND.
    ///
    /// Needed for nested sections: an inner box is only visible when the
    /// outer one is visible too. Without this combination the inner one could
    /// be opened while the outer stays shut.
    ///
    /// In disjunctive normal form that means: merge every group of one with
    /// every group of the other.
    pub fn and(&self, other: &Policy) -> Policy {
        let mut groups = Vec::with_capacity(self.0.len() * other.0.len());
        for left in &self.0 {
            for right in &other.0 {
                let merged = left
                    .attributes()
                    .iter()
                    .chain(right.attributes())
                    .cloned();
                if let Ok(group) = Conjunction::new(merged) {
                    groups.push(group);
                }
            }
        }
        Policy::new(groups).unwrap_or_else(|_| self.clone())
    }


}

impl FromStr for Policy {
    type Err = PolicyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut groups = Vec::new();
        for chunk in split_keyword(text, OR_WORD) {
            let mut attributes = Vec::new();
            for part in split_keyword(&chunk, AND_WORD) {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                attributes.push(trimmed.parse::<Attribute>()?);
            }
            if !attributes.is_empty() {
                groups.push(Conjunction::new(attributes)?);
            }
        }
        Policy::new(groups)
    }
}

/// Splits at a keyword, case-insensitively.
fn split_keyword(text: &str, keyword: &str) -> Vec<String> {
    // ASCII only: every position in `upper` is one in `text` — a letter that
    // shrinks or grows in capitals (`ı` → `I`) would cut it mid-letter.
    let upper = text.to_ascii_uppercase();
    let mut parts = Vec::new();
    let mut rest_start = 0usize;
    let mut search_from = 0usize;

    while let Some(found) = upper[search_from..].find(keyword) {
        let at = search_from + found;
        let end = at + keyword.len();
        let before_ok = at == 0 || upper[..at].ends_with(char::is_whitespace);
        let after_ok = end == upper.len() || upper[end..].starts_with(char::is_whitespace);

        if before_ok && after_ok {
            parts.push(text[rest_start..at].to_string());
            rest_start = end;
        }
        search_from = end;
    }
    parts.push(text[rest_start..].to_string());
    parts
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|g| g.to_string()).collect();
        f.write_str(&parts.join(&format!(" {OR_WORD} ")))
    }
}

#[cfg(test)]
#[path = "attribute_tests.rs"]
mod tests;
