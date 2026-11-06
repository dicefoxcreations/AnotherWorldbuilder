//! Stable identifiers, derived from public key material.
//!
//! The display name is a label, the identity is the key. That is why every
//! identifier is derived from the public key and never from a name.
//! Renaming therefore neither changes one's own access nor lets anyone write
//! themselves into someone else's.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Length of the identifier body in bytes; hex-encoded, twice as many characters.
const ID_BYTES: usize = 8;

/// An identifier of the form `<prefix>:<hex>`, e.g. `id:7f3a1c2b9d4e5f60`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Id(String);

impl Id {
    /// Derives an identifier deterministically from public material.
    ///
    /// The prefix is part of the hash input (domain separation): the same
    /// bytes give different identifiers under `id` and under `world`.
    pub fn derive(prefix: &str, material: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(prefix.as_bytes());
        hasher.update(b":");
        for part in material {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        Id(format!(
            "{prefix}:{}",
            hex::encode(&digest.as_bytes()[..ID_BYTES])
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The part before the colon, e.g. `id` or `world`.
    pub fn prefix(&self) -> &str {
        self.0.split_once(':').map(|(p, _)| p).unwrap_or(&self.0)
    }

    /// Short form for display next to a name.
    pub fn short(&self) -> &str {
        let body = self.0.split_once(':').map(|(_, b)| b).unwrap_or(&self.0);
        &body[..body.len().min(8)]
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic() {
        let a = Id::derive("id", &[b"keys"]);
        let b = Id::derive("id", &[b"keys"]);
        assert_eq!(a, b);
    }

    #[test]
    fn the_prefix_separates_namespaces() {
        let as_id = Id::derive("id", &[b"same material"]);
        let as_world = Id::derive("world", &[b"same material"]);
        assert_ne!(as_id.as_str()[3..], as_world.as_str()[6..]);
    }

    #[test]
    fn parts_are_length_prefixed() {
        // Without a length prefix, ["ab","c"] and ["a","bc"] would hash the same.
        let left = Id::derive("id", &[b"ab", b"c"]);
        let right = Id::derive("id", &[b"a", b"bc"]);
        assert_ne!(left, right);
    }

    #[test]
    fn the_form_is_prefix_colon_hex() {
        let id = Id::derive("id", &[b"x"]);
        assert_eq!(id.prefix(), "id");
        let (_, body) = id.as_str().split_once(':').unwrap();
        assert_eq!(body.len(), ID_BYTES * 2);
        assert!(body.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
