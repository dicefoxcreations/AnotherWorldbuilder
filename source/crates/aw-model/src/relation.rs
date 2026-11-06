//! Relations between documents: who someone is to whom.
//!
//! A property `Relations: @@Character@@ as @@Relation@@` holds entries, each
//! naming a document and two roles — what that document is to this one, and
//! what this one is to it:
//!
//! ```text
//! Relations: @@Character:Olga@@ Mother/Daughter, @@Character:Ada@@ Friend, @@Kurt@@
//! ```
//!
//! Olga is the mother, this one her daughter. Ada is a friend; what this one
//! is to her stays open. Kurt is related, and nobody has said how yet. The
//! roles are documents of the relation's role type, written by name.
//!
//! **Every entry stands in both documents.** Saving one side writes the
//! other: Olga's document then says `@@Marie@@ Daughter/Mother`. Each side is
//! complete on its own — when Olga is a secret, Marie's page still knows it
//! has a mother, it just cannot say who.
//!
//! An entry whose document the reader cannot open is not written at all: it
//! cannot be changed or removed by someone who does not even know whom it
//! names, and saving keeps it as it was.

use serde::{Deserialize, Serialize};

use crate::link::{self, Link, Piece};

/// One entry as stored: ids throughout.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The other document.
    pub with: String,
    /// What the other document is to this one — empty while open.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    /// What this document is to the other.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub own_role: String,
}

impl Entry {
    /// The same entry, seen from the other side.
    pub fn mirrored(&self, me: &str) -> Entry {
        Entry { with: me.to_string(), role: self.own_role.clone(), own_role: self.role.clone() }
    }
}

/// The entries of a stored value. Anything that is no entry is skipped.
pub fn entries(value: &serde_json::Value) -> Vec<Entry> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| serde_json::from_value::<Entry>(v.clone()).ok())
            .filter(|e| !e.with.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// The value to store for these entries.
pub fn value(entries: &[Entry]) -> serde_json::Value {
    serde_json::to_value(entries).unwrap_or_default()
}

/// One entry as written: the link, and the roles by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub link: Link,
    pub role: String,
    pub own_role: String,
}

/// Writes one entry: `@@Character:Olga@@ Mother/Daughter`.
///
/// `link` is the link as written already. An open own role leaves the `/`
/// out; an open role of the other keeps it, so the own one stays the own.
pub fn write(link: &str, role: &str, own_role: &str) -> String {
    match (role.is_empty(), own_role.is_empty()) {
        (true, true) => link.to_string(),
        (false, true) => format!("{link} {role}"),
        (_, false) => format!("{link} {role}/{own_role}"),
    }
}

/// Reads what a property of relations says.
pub fn read(text: &str) -> Result<Vec<Written>, String> {
    let mut out: Vec<Written> = Vec::new();
    for piece in link::scan(text) {
        match piece {
            Piece::Link(link) => out.push(Written { link, role: String::new(), own_role: String::new() }),
            Piece::Text(t) => {
                // What follows a comma belongs to the next entry — and that
                // begins with a link.
                let (roles, after) = match t.split_once(',') {
                    Some((roles, after)) => (roles, after),
                    None => (t, ""),
                };
                if !after.trim().trim_matches(',').trim().is_empty() {
                    return Err(format!(
                        "“{}” — after a comma comes the next @@Name@@.",
                        after.trim()
                    ));
                }
                let roles = roles.trim();
                if roles.is_empty() {
                    continue;
                }
                let Some(last) = out.last_mut() else {
                    return Err(format!("“{roles}” — every relation begins with @@Name@@."));
                };
                if !last.role.is_empty() || !last.own_role.is_empty() {
                    return Err(format!("“{roles}” — after a comma comes the next @@Name@@."));
                }
                let (role, own) = roles.split_once('/').unwrap_or((roles, ""));
                last.role = role.trim().to_string();
                last.own_role = own.trim().to_string();
                if last.own_role.contains('/') {
                    return Err(format!("“{roles}” — two roles at most: theirs/mine."));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_ok(text: &str) -> Vec<(String, String, String)> {
        read(text)
            .unwrap_or_else(|e| panic!("{text}: {e}"))
            .into_iter()
            .map(|w| (w.link.name, w.role, w.own_role))
            .collect()
    }

    #[test]
    fn entries_carry_both_roles() {
        assert_eq!(
            read_ok("@@Character:Olga@@ Mother/Daughter, @@Ada@@ Best friend, @@Kurt@@, @@Uwe@@ /Nephew"),
            vec![
                ("Olga".into(), "Mother".into(), "Daughter".into()),
                ("Ada".into(), "Best friend".into(), String::new()),
                ("Kurt".into(), String::new(), String::new()),
                ("Uwe".into(), String::new(), "Nephew".into()),
            ]
        );
        assert!(read_ok("").is_empty());
    }

    #[test]
    fn writing_reads_back() {
        for (role, own) in [("Mother", "Daughter"), ("Friend", ""), ("", ""), ("", "Nephew")] {
            let text = write("@@Olga@@", role, own);
            assert_eq!(read_ok(&text), vec![("Olga".into(), role.into(), own.into())], "{text}");
        }
    }

    #[test]
    fn stray_words_are_errors() {
        for (text, says) in [
            ("Mother @@Olga@@", "begins with"),
            ("@@Olga@@ Mother, Mentor", "next @@Name@@"),
            ("@@Olga@@ a/b/c", "two roles"),
        ] {
            let e = read(text).expect_err(text);
            assert!(e.contains(says), "{text}: {e}");
        }
    }

    #[test]
    fn mirrored_swaps_the_roles() {
        let e = Entry { with: "art:olga".into(), role: "Mother".into(), own_role: "Daughter".into() };
        assert_eq!(
            e.mirrored("art:marie"),
            Entry { with: "art:marie".into(), role: "Daughter".into(), own_role: "Mother".into() }
        );
    }

    #[test]
    fn stored_entries_round_trip() {
        let list = vec![
            Entry { with: "art:a".into(), role: "r".into(), own_role: String::new() },
            Entry { with: "art:b".into(), ..Entry::default() },
        ];
        let v = value(&list);
        assert_eq!(v, serde_json::json!([{ "with": "art:a", "role": "r" }, { "with": "art:b" }]));
        assert_eq!(entries(&v), list);
        assert!(entries(&serde_json::json!("text")).is_empty());
    }
}
