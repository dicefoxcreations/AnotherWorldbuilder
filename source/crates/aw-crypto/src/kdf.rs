//! Key derivation with HKDF-SHA256.
//!
//! Two uses:
//!
//! 1. Turn a Diffie-Hellman result into a wrapping key.
//! 2. Build an AND key from several attribute keys that only someone
//!    holding every single key can build.

use hkdf::Hkdf;
use sha2::Sha256;

/// Domain separation: every derivation purpose gets its own string, so the
/// same input never yields the same key twice.
pub const INFO_WRAP: &[u8] = b"anotherworldbuilder/v1/wrap";
pub const INFO_CONJUNCTION: &[u8] = b"anotherworldbuilder/v1/and";

/// Derives 32 bytes from `ikm`.
pub fn derive32(ikm: &[u8], salt: Option<&[u8]>, info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("32 bytes are far below the HKDF maximum");
    okm
}

/// Builds the AND key over several attributes.
///
/// `parts` are pairs of attribute name and its key. The result is
/// **independent of order**: a policy may write
/// `class:rogue AND faction:thieves-guild` in any order and must still hit
/// the same key.
///
/// Duplicate attribute names are merged, so the input is normalized first:
/// `a AND a` yields the same key as `a AND a AND a`.
pub fn conjunction_key(parts: &[(&str, [u8; 32])]) -> [u8; 32] {
    let mut sorted: Vec<&(&str, [u8; 32])> = parts.iter().collect();
    sorted.sort_by_key(|(name, _)| *name);
    sorted.dedup_by_key(|(name, _)| *name);

    let mut ikm = Vec::with_capacity(sorted.len() * 64);
    for (name, key) in sorted {
        // Length prefixes, so ("ab","c") and ("a","bc") do not collide.
        ikm.extend_from_slice(&(name.len() as u64).to_le_bytes());
        ikm.extend_from_slice(name.as_bytes());
        ikm.extend_from_slice(key);
    }
    derive32(&ikm, None, INFO_CONJUNCTION)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn order_does_not_matter() {
        let a = conjunction_key(&[("class:rogue", key(1)), ("faction:guild", key(2))]);
        let b = conjunction_key(&[("faction:guild", key(2)), ("class:rogue", key(1))]);
        assert_eq!(a, b);
    }

    #[test]
    fn other_attributes_give_another_key() {
        let a = conjunction_key(&[("class:rogue", key(1))]);
        let b = conjunction_key(&[("class:mage", key(1))]);
        assert_ne!(a, b);
    }

    #[test]
    fn another_key_gives_another_key() {
        let a = conjunction_key(&[("class:rogue", key(1))]);
        let b = conjunction_key(&[("class:rogue", key(2))]);
        assert_ne!(a, b);
    }

    #[test]
    fn a_subset_does_not_match() {
        // Whoever holds only one attribute must not be able to build the AND key.
        let both = conjunction_key(&[("a", key(1)), ("b", key(2))]);
        let only_a = conjunction_key(&[("a", key(1))]);
        assert_ne!(both, only_a);
    }

    #[test]
    fn domain_separation_works() {
        let as_wrap = derive32(b"same material", None, INFO_WRAP);
        let as_and = derive32(b"same material", None, INFO_CONJUNCTION);
        assert_ne!(as_wrap, as_and);
    }
}
