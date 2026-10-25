//! Attribute keys and epochs.
//!
//! The GM holds a complete keyring: one symmetric key per attribute and
//! epoch. A player holds only the subset that was unwrapped for them.
//!
//! Revocation is an epoch change: the attribute gets a new key that is only
//! wrapped for the remaining members. Everything new uses it. Ciphertext
//! already delivered stays readable — that is unavoidable, and the UI says
//! so.

use aw_crypto::{seal_key, unseal_key, ContentKey, IdentityPublic, WrappedKey};
use aw_model::Attribute;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use x25519_dalek::StaticSecret;

use crate::slot::AttributeAt;
use crate::{PolicyKeyError, Result};

/// An attribute key in a particular epoch.
#[derive(Clone)]
pub struct AttributeKey {
    pub at: AttributeAt,
    pub key: [u8; 32],
}

/// The GM's complete keyring.
///
/// Lies next to the world token on disk and never leaves the device. What
/// a member receives are only the individual keys wrapped via `grant`.
#[derive(Default)]
pub struct Keyring {
    keys: BTreeMap<AttributeAt, [u8; 32]>,
    current: BTreeMap<Attribute, u32>,
}

impl Keyring {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an attribute if it does not exist yet and returns its
    /// current epoch.
    pub fn ensure(&mut self, attribute: &Attribute) -> Result<u32> {
        if let Some(epoch) = self.current.get(attribute) {
            return Ok(*epoch);
        }
        self.install(attribute.clone(), 1)?;
        Ok(1)
    }

    /// Moves an attribute to the next epoch — the revocation.
    ///
    /// The new key must then be wrapped for the remaining members; whoever
    /// left does not get it.
    pub fn rotate(&mut self, attribute: &Attribute) -> Result<u32> {
        let next = self.current.get(attribute).copied().unwrap_or(0) + 1;
        self.install(attribute.clone(), next)?;
        Ok(next)
    }

    fn install(&mut self, attribute: Attribute, epoch: u32) -> Result<()> {
        let key = aw_crypto::random_bytes::<32>()?;
        self.keys.insert(AttributeAt::new(attribute.clone(), epoch), key);
        self.current.insert(attribute, epoch);
        Ok(())
    }

    pub fn current_epoch(&self, attribute: &Attribute) -> Option<u32> {
        self.current.get(attribute).copied()
    }

    pub fn key_at(&self, at: &AttributeAt) -> Option<[u8; 32]> {
        self.keys.get(at).copied()
    }

    /// The current key of an attribute together with its epoch.
    pub fn current(&self, attribute: &Attribute) -> Option<AttributeKey> {
        let epoch = self.current_epoch(attribute)?;
        let at = AttributeAt::new(attribute.clone(), epoch);
        let key = self.key_at(&at)?;
        Some(AttributeKey { at, key })
    }

    pub fn attributes(&self) -> impl Iterator<Item = &Attribute> {
        self.current.keys()
    }

    /// The keys of exactly the named attributes, in their current epoch.
    ///
    /// This is the engine behind "preview as player": the preview hides
    /// nothing, it simply gets fewer keys and so decrypts exactly as much as
    /// that player's real device would.
    pub fn restrict_to(&self, attributes: &[Attribute]) -> HeldKeys {
        let mut held = HeldKeys::new();
        for attribute in attributes {
            if let Some(key) = self.current(attribute) {
                held.insert(key.at, key.key);
            }
        }
        held
    }

    /// All keys — the GM's view.
    pub fn everything(&self) -> HeldKeys {
        let mut held = HeldKeys::new();
        for (at, key) in &self.keys {
            held.insert(at.clone(), *key);
        }
        held
    }

    pub fn to_json(&self) -> Result<String> {
        let file = KeyringFile {
            version: KEYRING_VERSION,
            keys: self
                .keys
                .iter()
                .map(|(at, key)| (at.to_string(), hex::encode(key)))
                .collect(),
            current: self
                .current
                .iter()
                .map(|(a, e)| (a.to_string(), *e))
                .collect(),
        };
        serde_json::to_string_pretty(&file).map_err(|e| PolicyKeyError::BadSlot(e.to_string()))
    }

    pub fn from_json(text: &str) -> Result<Self> {
        let file: KeyringFile =
            serde_json::from_str(text).map_err(|e| PolicyKeyError::BadSlot(e.to_string()))?;
        if file.version != KEYRING_VERSION {
            return Err(PolicyKeyError::Crypto(
                aw_crypto::CryptoError::UnknownVersion {
                    found: file.version,
                    known: KEYRING_VERSION,
                },
            ));
        }
        let mut ring = Keyring::new();
        for (at_text, key_hex) in file.keys {
            let at: AttributeAt = at_text.parse()?;
            let key: [u8; 32] = hex::decode(&key_hex)
                .map_err(|e| PolicyKeyError::BadSlot(e.to_string()))?
                .try_into()
                .map_err(|_| PolicyKeyError::BadSlot("key is not 32 bytes".into()))?;
            ring.keys.insert(at, key);
        }
        for (a_text, epoch) in file.current {
            let attribute: Attribute = a_text
                .parse()
                .map_err(|_| PolicyKeyError::UnknownAttribute(a_text.clone()))?;
            ring.current.insert(attribute, epoch);
        }
        Ok(ring)
    }

    /// Wraps the keys of the named attributes for a member — of every epoch
    /// up to the current one: whoever comes after a revocation reads what
    /// was written before it, as those who stayed do. The result goes into
    /// the control document.
    pub fn grant(
        &self,
        member: &IdentityPublic,
        attributes: &[Attribute],
    ) -> Result<Vec<GrantedKey>> {
        let recipient = member.seal_public()?;
        let mut out = Vec::new();
        for attribute in attributes {
            let current = self
                .current_epoch(attribute)
                .ok_or_else(|| PolicyKeyError::UnknownAttribute(attribute.to_string()))?;
            for epoch in 1..=current {
                let at = AttributeAt::new(attribute.clone(), epoch);
                let Some(key) = self.key_at(&at) else { continue };
                out.push(GrantedKey {
                    at: at.to_string(),
                    wrapped: seal_key(&recipient, &ContentKey::from_bytes(key))?,
                });
            }
        }
        Ok(out)
    }
}

/// Format version of the keyring file.
pub const KEYRING_VERSION: u8 = 1;

#[derive(Serialize, Deserialize)]
struct KeyringFile {
    version: u8,
    keys: BTreeMap<String, String>,
    current: BTreeMap<String, u32>,
}

/// An attribute key wrapped for a member, as it stands in the control
/// document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantedKey {
    /// Attribute and epoch, e.g. `class:rogue@3`.
    pub at: String,
    pub wrapped: WrappedKey,
}

/// The keys a device was actually able to unwrap.
///
/// What is not in here cannot be opened — regardless of what the UI shows
/// or which documents the device received.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct HeldKeys {
    keys: BTreeMap<AttributeAt, [u8; 32]>,
}

impl HeldKeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// Unwraps the granted keys with one's own private key.
    ///
    /// Entries that cannot be opened are skipped: a control document holds
    /// the grants of *all* members.
    pub fn unseal(secret: &StaticSecret, granted: &[GrantedKey]) -> Self {
        let mut held = HeldKeys::new();
        for entry in granted {
            let Ok(at) = entry.at.parse::<AttributeAt>() else {
                continue;
            };
            if let Ok(key) = unseal_key(secret, &entry.wrapped) {
                held.keys.insert(at, *key.as_bytes());
            }
        }
        held
    }

    /// The keys held for these attributes, every epoch of them, wrapped for
    /// someone else — what a supporter hands a player they let in.
    pub fn grant_to(&self, member: &IdentityPublic, attributes: &[Attribute]) -> Result<Vec<GrantedKey>> {
        let recipient = member.seal_public()?;
        let mut out = Vec::new();
        for (at, key) in self.keys.iter().filter(|(at, _)| attributes.contains(&at.attribute)) {
            out.push(GrantedKey { at: at.to_string(), wrapped: seal_key(&recipient, &ContentKey::from_bytes(*key))? });
        }
        Ok(out)
    }

    pub fn insert(&mut self, at: AttributeAt, key: [u8; 32]) {
        self.keys.insert(at, key);
    }

    pub fn key_at(&self, at: &AttributeAt) -> Option<[u8; 32]> {
        self.keys.get(at).copied()
    }

    /// Every held key, with the attribute and epoch it is for.
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeAt, &[u8; 32])> {
        self.keys.iter()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The attributes this device can hold — without epochs.
    ///
    /// Exactly this set is what "preview as player" renders.
    pub fn attributes(&self) -> std::collections::BTreeSet<Attribute> {
        self.keys.keys().map(|at| at.attribute.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_crypto::Identity;

    fn attr(text: &str) -> Attribute {
        text.parse().unwrap()
    }

    #[test]
    fn a_new_attribute_starts_at_epoch_one() {
        let mut ring = Keyring::new();
        assert_eq!(ring.ensure(&attr("class:rogue")).unwrap(), 1);
        assert_eq!(ring.ensure(&attr("class:rogue")).unwrap(), 1);
    }

    #[test]
    fn revoking_raises_the_epoch_and_changes_the_key() {
        let mut ring = Keyring::new();
        let a = attr("class:rogue");
        ring.ensure(&a).unwrap();
        let old = ring.current(&a).unwrap();

        assert_eq!(ring.rotate(&a).unwrap(), 2);
        let new = ring.current(&a).unwrap();

        assert_eq!(new.at.epoch, 2);
        assert_ne!(old.key, new.key);
        // The old key stays known so existing content can still be opened.
        assert_eq!(ring.key_at(&old.at), Some(old.key));
    }

    #[test]
    fn a_member_unpacks_their_keys() {
        let mut ring = Keyring::new();
        let rogue = attr("class:rogue");
        let players = attr("players");
        ring.ensure(&rogue).unwrap();
        ring.ensure(&players).unwrap();

        let bob = Identity::create("Bob").unwrap();
        let granted = ring.grant(&bob.public(), &[rogue.clone(), players.clone()]).unwrap();

        let held = HeldKeys::unseal(bob.seal_secret(), &granted);
        assert_eq!(held.len(), 2);
        assert_eq!(
            held.key_at(&AttributeAt::new(rogue.clone(), 1)),
            Some(ring.current(&rogue).unwrap().key)
        );
    }

    #[test]
    fn foreign_grants_are_skipped() {
        let mut ring = Keyring::new();
        let rogue = attr("class:rogue");
        ring.ensure(&rogue).unwrap();

        let bob = Identity::create("Bob").unwrap();
        let mallory = Identity::create("Mallory").unwrap();

        // The control document holds the grants of all members.
        let mut all = ring.grant(&bob.public(), std::slice::from_ref(&rogue)).unwrap();
        all.extend(ring.grant(&mallory.public(), std::slice::from_ref(&rogue)).unwrap());

        let held = HeldKeys::unseal(bob.seal_secret(), &all);
        assert_eq!(held.len(), 1, "Bob may only open his own entry");
    }

    #[test]
    fn someone_removed_does_not_get_the_new_key() {
        let mut ring = Keyring::new();
        let rogue = attr("class:rogue");
        ring.ensure(&rogue).unwrap();

        let bob = Identity::create("Bob").unwrap();
        let old = ring.grant(&bob.public(), std::slice::from_ref(&rogue)).unwrap();
        let bob_old = HeldKeys::unseal(bob.seal_secret(), &old);

        // Bob changes class: new epoch, wrapped only for the others.
        ring.rotate(&rogue).unwrap();
        let anna = Identity::create("Anna").unwrap();
        let new = ring.grant(&anna.public(), std::slice::from_ref(&rogue)).unwrap();

        let bob_after_revocation = HeldKeys::unseal(bob.seal_secret(), &new);
        assert!(bob_after_revocation.is_empty());
        // He still has the old key — that cannot be taken back.
        assert!(bob_old.key_at(&AttributeAt::new(rogue, 1)).is_some());
    }

    #[test]
    fn keyring_round_trip_through_json() {
        let mut ring = Keyring::new();
        ring.ensure(&attr("class:rogue")).unwrap();
        ring.ensure(&attr("players")).unwrap();
        ring.rotate(&attr("class:rogue")).unwrap();

        let again = Keyring::from_json(&ring.to_json().unwrap()).unwrap();
        assert_eq!(again.current_epoch(&attr("class:rogue")), Some(2));
        assert_eq!(again.current_epoch(&attr("players")), Some(1));
        // The old key must survive too, or existing content would be
        // unreadable after a restart.
        assert_eq!(again.everything().len(), 3);
        assert_eq!(
            again.current(&attr("class:rogue")).unwrap().key,
            ring.current(&attr("class:rogue")).unwrap().key
        );
    }

    #[test]
    fn the_preview_gets_exactly_the_named_keys() {
        let mut ring = Keyring::new();
        for a in ["players", "class:rogue", "gm"] {
            ring.ensure(&attr(a)).unwrap();
        }
        let preview = ring.restrict_to(&[attr("players"), attr("class:rogue")]);
        assert_eq!(preview.len(), 2);
        assert_eq!(ring.everything().len(), 3);
    }

    #[test]
    fn an_unknown_attribute_reports_itself() {
        let ring = Keyring::new();
        let bob = Identity::create("Bob").unwrap();
        assert!(matches!(
            ring.grant(&bob.public(), &[attr("class:rogue")]),
            Err(PolicyKeyError::UnknownAttribute(_))
        ));
    }
}
