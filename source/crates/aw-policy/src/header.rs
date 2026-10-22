//! The layer header: where the wrapped data key lies.
//!
//! Every layer has a random data key. It is wrapped once per AND group of
//! the policy and stored in the header:
//!
//! ```text
//! policy   class:rogue OR background:noble
//! slots    <tag of class:rogue@1>          -> ciphertext of the data key
//!          <tag of background:noble@1>     -> ciphertext of the data key
//! ```
//!
//! OR therefore costs just one more entry. AND costs a key derived via
//! HKDF that only someone holding every part can build.
//!
//! The header goes to everyone, so it names nothing: neither the policy nor
//! the attributes. A slot is addressed by tags only the holder of the
//! attribute key can compute — and with a salt of each header's own, the
//! same attribute looks different in every header. Who may read a layer
//! learns its policy from the layer itself, once it is open.

use aw_crypto::kdf::{conjunction_key, derive32};
use aw_crypto::{decrypt, encrypt, random_bytes, ContentKey};
use aw_model::Policy;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::keyring::{HeldKeys, Keyring};
use crate::slot::AttributeAt;
use crate::{PolicyKeyError, Result};

/// Format version of the layer header.
///
/// Present from day one, even before anything was encrypted. A header
/// without a version could never change later without making existing
/// worlds unreadable.
pub const HEADER_VERSION: u8 = 2;

const TAG_INFO: &[u8] = b"anotherworldbuilder/v1/slot-tag";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerHeader {
    pub version: u8,
    /// Epoch of the layer. Increases on a history cut.
    pub epoch: u32,
    /// The header's own salt, hex-encoded — it makes every header's tags
    /// its own.
    pub salt: String,
    /// Slot — the tags of its parts, sorted and joined by `+` — to the
    /// hex-encoded ciphertext of the data key.
    pub slots: BTreeMap<String, String>,
}

/// The tag of one attribute key in a header: computable only with the key.
fn tag(key: &[u8; 32], salt: &[u8]) -> String {
    hex::encode(&derive32(key, Some(salt), TAG_INFO)[..12])
}

/// Wraps `dek` for every AND group of the policy.
pub fn wrap_dek(
    policy: &Policy,
    epoch: u32,
    keyring: &Keyring,
    dek: &ContentKey,
) -> Result<LayerHeader> {
    let salt = random_bytes::<16>()?;
    let mut slots = BTreeMap::new();

    for group in policy.groups() {
        let mut parts = Vec::new();
        for attribute in group.attributes() {
            let current = keyring
                .current(attribute)
                .ok_or_else(|| PolicyKeyError::UnknownAttribute(attribute.to_string()))?;
            parts.push(current);
        }
        let mut tags: Vec<String> = parts.iter().map(|k| tag(&k.key, &salt)).collect();
        tags.sort();
        tags.dedup();
        let named: Vec<(String, [u8; 32])> = parts
            .iter()
            .map(|k| (k.at.to_string(), k.key))
            .collect();
        let slot_key = ContentKey::from_bytes(derive_slot_key(&named));

        slots.insert(tags.join("+"), hex::encode(encrypt(&slot_key, dek.as_bytes())?));
    }

    Ok(LayerHeader {
        version: HEADER_VERSION,
        epoch,
        salt: hex::encode(salt),
        slots,
    })
}

/// Opens the data key if the held keys hit a slot.
///
/// Returns `None` when no slot fits. That is the normal case for every
/// layer this viewer may not see — and it is exactly the same result an
/// attacker gets with the complete ciphertext.
pub fn open_dek(header: &LayerHeader, held: &HeldKeys) -> Result<Option<ContentKey>> {
    if header.version != HEADER_VERSION {
        return Err(PolicyKeyError::Crypto(
            aw_crypto::CryptoError::UnknownVersion {
                found: header.version,
                known: HEADER_VERSION,
            },
        ));
    }
    let Ok(salt) = hex::decode(&header.salt) else {
        return Ok(None);
    };
    // Each held key's tag in this header.
    let mine: HashMap<String, (&AttributeAt, [u8; 32])> = held
        .iter()
        .map(|(at, key)| (tag(key, &salt), (at, *key)))
        .collect();

    for (slot, sealed_hex) in &header.slots {
        // Every part of the slot must be held, otherwise the next slot.
        let named: Option<Vec<(String, [u8; 32])>> = slot
            .split('+')
            .map(|t| mine.get(t).map(|(at, key)| (at.to_string(), *key)))
            .collect();
        let Some(named) = named else {
            continue;
        };

        let slot_key = ContentKey::from_bytes(derive_slot_key(&named));
        let Ok(sealed) = hex::decode(sealed_hex) else {
            continue;
        };
        if let Ok(raw) = decrypt(&slot_key, &sealed) {
            if let Ok(bytes) = <[u8; 32]>::try_from(raw) {
                return Ok(Some(ContentKey::from_bytes(bytes)));
            }
        }
    }
    Ok(None)
}

/// Derives the key of a slot from its attribute keys.
///
/// Also for slots with only one attribute, so there is exactly one code path.
fn derive_slot_key(named: &[(String, [u8; 32])]) -> [u8; 32] {
    let refs: Vec<(&str, [u8; 32])> = named.iter().map(|(n, k)| (n.as_str(), *k)).collect();
    conjunction_key(&refs)
}

/// Convenience: attribute in its current epoch.
pub fn at_current(keyring: &Keyring, attribute: &aw_model::Attribute) -> Option<AttributeAt> {
    keyring.current(attribute).map(|k| k.at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::Attribute;

    fn attr(text: &str) -> Attribute {
        text.parse().unwrap()
    }

    /// Builds a keyring with every attribute of a policy.
    fn ring_for(policy: &Policy) -> Keyring {
        let mut ring = Keyring::new();
        for attribute in policy.mentioned() {
            ring.ensure(attribute).unwrap();
        }
        ring
    }

    /// Gives a viewer exactly the keys of the named attributes.
    fn held_for(ring: &Keyring, attributes: &[Attribute]) -> HeldKeys {
        let mut held = HeldKeys::new();
        for attribute in attributes {
            if let Some(key) = ring.current(attribute) {
                held.insert(key.at, key.key);
            }
        }
        held
    }

    #[test]
    fn the_entitled_one_opens() {
        let policy: Policy = "class:rogue OR background:noble".parse().unwrap();
        let ring = ring_for(&policy);
        let dek = ContentKey::generate().unwrap();
        let header = wrap_dek(&policy, 1, &ring, &dek).unwrap();

        let held = held_for(&ring, &[attr("class:rogue")]);
        let opened = open_dek(&header, &held).unwrap().unwrap();
        assert_eq!(opened.as_bytes(), dek.as_bytes());
    }

    #[test]
    fn a_header_names_nothing_and_no_two_look_alike() {
        let policy: Policy = "knowledge:traitor-is-aldric OR class:rogue AND faction:guild".parse().unwrap();
        let ring = ring_for(&policy);
        let dek = ContentKey::generate().unwrap();
        let one = wrap_dek(&policy, 1, &ring, &dek).unwrap();
        let two = wrap_dek(&policy, 1, &ring, &dek).unwrap();
        let text = serde_json::to_string(&one).unwrap();
        for name in ["knowledge", "aldric", "rogue", "guild", "@1"] {
            assert!(!text.contains(name), "{name} in {text}");
        }
        let tags = |h: &LayerHeader| h.slots.keys().flat_map(|s| s.split('+').map(String::from)).collect::<Vec<_>>();
        assert!(tags(&one).iter().all(|t| !tags(&two).contains(t)), "the same attribute, unlinkable");
        let held = held_for(&ring, &[attr("class:rogue"), attr("faction:guild")]);
        assert!(open_dek(&two, &held).unwrap().is_some());
    }

    #[test]
    fn someone_without_rights_does_not_open() {
        let policy: Policy = "class:rogue".parse().unwrap();
        let ring = ring_for(&policy);
        let dek = ContentKey::generate().unwrap();
        let header = wrap_dek(&policy, 1, &ring, &dek).unwrap();

        let mut foreign = Keyring::new();
        foreign.ensure(&attr("class:mage")).unwrap();
        let held = held_for(&foreign, &[attr("class:mage")]);

        assert!(open_dek(&header, &held).unwrap().is_none());
    }

    #[test]
    fn and_requires_both_parts() {
        let policy: Policy = "class:rogue AND faction:guild".parse().unwrap();
        let ring = ring_for(&policy);
        let dek = ContentKey::generate().unwrap();
        let header = wrap_dek(&policy, 1, &ring, &dek).unwrap();

        assert!(open_dek(&header, &held_for(&ring, &[attr("class:rogue")]))
            .unwrap()
            .is_none());
        assert!(open_dek(&header, &held_for(&ring, &[attr("faction:guild")]))
            .unwrap()
            .is_none());

        let both = held_for(&ring, &[attr("class:rogue"), attr("faction:guild")]);
        assert!(open_dek(&header, &both).unwrap().is_some());
    }

    #[test]
    fn or_costs_one_slot_per_group() {
        let policy: Policy = "a:1 OR b:2 OR c:3".parse().unwrap();
        let ring = ring_for(&policy);
        let dek = ContentKey::generate().unwrap();
        let header = wrap_dek(&policy, 1, &ring, &dek).unwrap();
        assert_eq!(header.slots.len(), 3);
    }

    #[test]
    fn revoking_locks_the_next_layer() {
        let policy: Policy = "class:rogue".parse().unwrap();
        let mut ring = ring_for(&policy);
        let rogue = attr("class:rogue");

        // Bob holds the key of the first epoch.
        let bob = held_for(&ring, std::slice::from_ref(&rogue));

        let old_layer = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();
        assert!(open_dek(&old_layer, &bob).unwrap().is_some());

        // Bob changes class.
        ring.rotate(&rogue).unwrap();
        let new_layer = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();

        assert!(
            open_dek(&new_layer, &bob).unwrap().is_none(),
            "after revocation nothing new may open"
        );
        assert!(
            open_dek(&old_layer, &bob).unwrap().is_some(),
            "existing content stays readable — that cannot be taken back"
        );
    }

    #[test]
    fn a_history_cut_locks_the_old_state() {
        // New layer epoch plus a fresh data key: whoever never got the old
        // key does not see the past either.
        let policy: Policy = "players".parse().unwrap();
        let ring = ring_for(&policy);
        let held = held_for(&ring, &[attr("players")]);

        let old_dek = ContentKey::generate().unwrap();
        let new_dek = ContentKey::generate().unwrap();
        let old = wrap_dek(&policy, 1, &ring, &old_dek).unwrap();
        let new = wrap_dek(&policy, 2, &ring, &new_dek).unwrap();

        assert_eq!(old.epoch, 1);
        assert_eq!(new.epoch, 2);
        assert_ne!(
            open_dek(&old, &held).unwrap().unwrap().as_bytes(),
            open_dek(&new, &held).unwrap().unwrap().as_bytes()
        );
    }

    #[test]
    fn an_unknown_header_version_is_rejected() {
        let policy: Policy = "players".parse().unwrap();
        let ring = ring_for(&policy);
        let mut header = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();
        header.version = 99;
        assert!(open_dek(&header, &HeldKeys::new()).is_err());
    }

    #[test]
    fn header_round_trip_through_json() {
        let policy: Policy = "class:rogue AND faction:guild OR gm".parse().unwrap();
        let ring = ring_for(&policy);
        let dek = ContentKey::generate().unwrap();
        let header = wrap_dek(&policy, 1, &ring, &dek).unwrap();

        let json = serde_json::to_string(&header).unwrap();
        let again: LayerHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(again, header);

        let held = held_for(&ring, &[attr("gm")]);
        assert_eq!(
            open_dek(&again, &held).unwrap().unwrap().as_bytes(),
            dek.as_bytes()
        );
    }
}
