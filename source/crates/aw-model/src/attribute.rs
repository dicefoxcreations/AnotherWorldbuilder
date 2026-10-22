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
//! OR is the normal case and costs nothing — the data key is simply wrapped
//! several times. AND costs a derived key. NOT does not exist: whoever has
//! a key, has it.
//!
//! The canonical notation (`Display`) is stable because it feeds into key
//! derivation. It must never change casually.

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

    /// Several attributes, any one of which suffices.
    pub fn any_of(attributes: impl IntoIterator<Item = Attribute>) -> Result<Self, PolicyError> {
        Policy::new(attributes.into_iter().map(Conjunction::single))
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

    /// All attributes that occur anywhere — for the UI.
    pub fn mentioned(&self) -> BTreeSet<&Attribute> {
        self.0.iter().flat_map(|g| g.attributes().iter()).collect()
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
    let upper = text.to_uppercase();
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
mod tests {
    use super::*;

    fn held(list: &[&str]) -> BTreeSet<Attribute> {
        list.iter().map(|s| s.parse().unwrap()).collect()
    }

    fn attr(text: &str) -> Attribute {
        text.parse().unwrap()
    }

    #[test]
    fn special_attributes_are_recognised() {
        assert_eq!("all".parse::<Attribute>().unwrap(), Attribute::All);
        assert_eq!("players".parse::<Attribute>().unwrap(), Attribute::Players);
        assert_eq!("gm".parse::<Attribute>().unwrap(), Attribute::Gm);
    }

    #[test]
    fn named_attributes_are_split() {
        let a: Attribute = "class:rogue".parse().unwrap();
        assert_eq!(a, Attribute::named("class", "rogue"));
        assert_eq!(a.to_string(), "class:rogue");
    }

    #[test]
    fn nonsense_is_rejected() {
        assert!("without colon".parse::<Attribute>().is_err());
        assert!("class:".parse::<Attribute>().is_err());
        assert!(":rogue".parse::<Attribute>().is_err());
        assert!("class:ro gue".parse::<Attribute>().is_err());
    }

    #[test]
    fn for_or_one_hit_is_enough() {
        let p: Policy = "class:rogue OR background:noble".parse().unwrap();
        assert!(p.satisfied_by(&held(&["class:rogue"])));
        assert!(p.satisfied_by(&held(&["background:noble"])));
        assert!(!p.satisfied_by(&held(&["class:mage"])));
    }

    #[test]
    fn and_requires_all() {
        let p: Policy = "class:rogue AND faction:thieves-guild".parse().unwrap();
        assert!(!p.satisfied_by(&held(&["class:rogue"])));
        assert!(!p.satisfied_by(&held(&["faction:thieves-guild"])));
        assert!(p.satisfied_by(&held(&["class:rogue", "faction:thieves-guild"])));
    }

    #[test]
    fn mixed() {
        let p: Policy = "class:rogue AND faction:thieves-guild OR player:bob"
            .parse()
            .unwrap();
        assert!(p.satisfied_by(&held(&["player:bob"])));
        assert!(p.satisfied_by(&held(&["class:rogue", "faction:thieves-guild"])));
        assert!(!p.satisfied_by(&held(&["class:rogue"])));
    }

    #[test]
    fn order_does_not_change_the_policy() {
        let a: Policy = "b:2 AND a:1".parse().unwrap();
        let b: Policy = "a:1 AND b:2".parse().unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_string(), b.to_string());
    }

    #[test]
    fn the_keyword_is_case_insensitive() {
        let a: Policy = "a:1 or b:2".parse().unwrap();
        let b: Policy = "a:1 OR b:2".parse().unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn a_keyword_inside_a_value_does_not_disturb() {
        // "border" contains "OR" but must not split.
        let p: Policy = "faction:border".parse().unwrap();
        assert_eq!(p.groups().len(), 1);
        assert_eq!(p.to_string(), "faction:border");
    }

    #[test]
    fn the_canonical_form_reads_back() {
        let p: Policy = "class:rogue AND faction:guild OR player:bob"
            .parse()
            .unwrap();
        let again: Policy = p.to_string().parse().unwrap();
        assert_eq!(p, again);
    }

    #[test]
    fn an_empty_policy_is_an_error() {
        assert_eq!("".parse::<Policy>(), Err(PolicyError::Empty));
        assert_eq!("   ".parse::<Policy>(), Err(PolicyError::Empty));
    }

    #[test]
    fn duplicate_attributes_are_merged() {
        let p: Policy = "a:1 AND a:1".parse().unwrap();
        assert_eq!(p.groups()[0].attributes().len(), 1);
        assert!(p.groups()[0].is_single());
    }

    #[test]
    fn and_demands_both_sides() {
        let outer: Policy = "players".parse().unwrap();
        let inner: Policy = "class:rogue".parse().unwrap();
        let both = outer.and(&inner);

        assert!(!both.satisfied_by(&held(&["players"])));
        assert!(!both.satisfied_by(&held(&["class:rogue"])));
        assert!(both.satisfied_by(&held(&["players", "class:rogue"])));
    }

    #[test]
    fn and_multiplies_the_groups_out() {
        let a: Policy = "a:1 OR b:2".parse().unwrap();
        let b: Policy = "c:3 OR d:4".parse().unwrap();
        let both = a.and(&b);

        assert_eq!(both.groups().len(), 4);
        assert!(both.satisfied_by(&held(&["a:1", "d:4"])));
        assert!(both.satisfied_by(&held(&["b:2", "c:3"])));
        assert!(!both.satisfied_by(&held(&["a:1", "b:2"])));
    }

    #[test]
    fn and_with_itself_changes_nothing() {
        let p: Policy = "players OR gm".parse().unwrap();
        assert_eq!(p.and(&p), p);
    }

    #[test]
    fn superfluous_groups_fall_away() {
        // Whoever gets in with `a:1` alone does not need `a:1 AND b:2`.
        let p: Policy = "a:1 OR a:1 AND b:2".parse().unwrap();
        assert_eq!(p.groups().len(), 1);
        assert_eq!(p.to_string(), "a:1");
        assert!(p.satisfied_by(&held(&["a:1"])));
    }

    #[test]
    fn absorption_does_not_change_the_meaning() {
        // Exhaustive over four attributes: the same set must get in before
        // and after simplifying.
        let universe = ["a:1", "b:2", "c:3", "d:4"];
        let raw: Policy = "a:1 AND b:2 OR a:1 OR c:3 AND d:4".parse().unwrap();

        for mask in 0..16u32 {
            let set: BTreeSet<Attribute> = universe
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, n)| attr(n))
                .collect();
            let expected = set.contains(&attr("a:1"))
                || (set.contains(&attr("c:3")) && set.contains(&attr("d:4")));
            assert_eq!(raw.satisfied_by(&set), expected, "for mask {mask}");
        }
    }

    #[test]
    fn json_round_trip() {
        let p: Policy = "class:rogue OR player:bob".parse().unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Policy>(&json).unwrap(), p);
    }
}
