//! The world token: the root of a world's key hierarchy.
//!
//! Whoever creates a world generates a key pair that concerns only this
//! world. It signs memberships, attribute grants and epoch changes.
//!
//! The **secret** part never leaves the device and is not synced. The
//! **public** part must travel along: without it a player cannot check
//! whether a grant really comes from the GM or a fellow player forged it.
//! It is part of the invitation code.
//!
//! It follows directly: whoever loses the world token loses the world. No
//! server can help.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::ids::Id;
use crate::{random_bytes, CryptoError, Result};

/// Format version of the world token file.
pub const WORLD_VERSION: u8 = 1;

/// Identifier of a world, derived from its public key.
pub type WorldId = Id;

/// The private world token. Only the GM has it.
pub struct WorldToken {
    world_id: WorldId,
    name: String,
    owner: SigningKey,
}

/// The public part that travels in the invitation code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldPublic {
    pub world_id: WorldId,
    pub name: String,
    /// The world's Ed25519 verifying key, hex-encoded.
    pub owner_key: String,
}

#[derive(Serialize, Deserialize)]
struct WorldFile {
    version: u8,
    world_id: WorldId,
    name: String,
    owner_key: String,
}

impl WorldToken {
    /// Creates a new world along with its token.
    pub fn create(name: impl Into<String>) -> Result<Self> {
        let owner = SigningKey::from_bytes(&random_bytes::<32>()?);
        Ok(Self::assemble(name.into(), owner))
    }

    fn assemble(name: String, owner: SigningKey) -> Self {
        let world_id = Id::derive("world", &[owner.verifying_key().as_bytes()]);
        WorldToken {
            world_id,
            name,
            owner,
        }
    }

    pub fn world_id(&self) -> &WorldId {
        &self.world_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn public(&self) -> WorldPublic {
        WorldPublic {
            world_id: self.world_id.clone(),
            name: self.name.clone(),
            owner_key: hex::encode(self.owner.verifying_key().as_bytes()),
        }
    }

    /// Signs a grant, a membership or an epoch change.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.owner.sign(message).to_bytes()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json()?)?;
        crate::restrict_permissions(path)?;
        Ok(())
    }

    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(&WorldFile {
            version: WORLD_VERSION,
            world_id: self.world_id.clone(),
            name: self.name.clone(),
            owner_key: hex::encode(self.owner.to_bytes()),
        })?)
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read(path)?)
    }

    pub fn from_json(raw: &[u8]) -> Result<Self> {
        let file: WorldFile = serde_json::from_slice(raw)?;
        if file.version != WORLD_VERSION {
            return Err(CryptoError::UnknownVersion {
                found: file.version,
                known: WORLD_VERSION,
            });
        }
        let bytes: [u8; 32] = hex::decode(&file.owner_key)
            .map_err(|e| CryptoError::BadKey(e.to_string()))?
            .try_into()
            .map_err(|_| CryptoError::BadKey("world key is not 32 bytes".into()))?;

        let token = Self::assemble(file.name, SigningKey::from_bytes(&bytes));
        if token.world_id != file.world_id {
            return Err(CryptoError::BadKey(
                "world id in the file does not match the key".into(),
            ));
        }
        Ok(token)
    }
}

impl WorldPublic {
    /// Whether the world id really belongs to this owner key.
    ///
    /// The id is derived from the key. A world that arrives over the network
    /// is only trusted once its key produces its id — otherwise someone could
    /// send a known world id signed with a key of their own.
    pub fn is_genuine(&self) -> bool {
        hex::decode(&self.owner_key)
            .ok()
            .filter(|k| k.len() == 32)
            .is_some_and(|k| Id::derive("world", &[&k]) == self.world_id)
    }

    /// Checks whether a grant really comes from the holder of the world token.
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> Result<()> {
        let bytes: [u8; 32] = hex::decode(&self.owner_key)
            .map_err(|e| CryptoError::BadKey(e.to_string()))?
            .try_into()
            .map_err(|_| CryptoError::BadKey("world verifying key is not 32 bytes".into()))?;
        let verify =
            VerifyingKey::from_bytes(&bytes).map_err(|e| CryptoError::BadKey(e.to_string()))?;
        verify
            .verify(message, &Signature::from_bytes(signature))
            .map_err(|_| CryptoError::BadSignature)?;

        // The id must match the key, or a foreign world could be slipped in
        // under a false id.
        let expected = Id::derive("world", &[&bytes]);
        if expected != self.world_id {
            return Err(CryptoError::BadKey(
                "world id does not match the verifying key".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_world_is_genuine_only_with_its_own_key() {
        let world = WorldToken::create("Eldermere").unwrap().public();
        assert!(world.is_genuine());
        let other = WorldToken::create("Other").unwrap().public();
        let forged = WorldPublic { world_id: world.world_id.clone(), ..other };
        assert!(!forged.is_genuine(), "Eldermere's id with another key");
    }

    use super::*;

    #[test]
    fn the_id_is_derived_from_the_key() {
        let world = WorldToken::create("Eldermere").unwrap();
        assert_eq!(world.world_id().prefix(), "world");
    }

    #[test]
    fn renaming_does_not_change_the_id() {
        let mut world = WorldToken::create("Eldermere").unwrap();
        let before = world.world_id().clone();
        world.rename("Middle-earth");
        assert_eq!(world.world_id(), &before);
    }

    #[test]
    fn a_player_checks_the_gms_grant() {
        let world = WorldToken::create("Eldermere").unwrap();
        let public = world.public();

        let grant = b"character:eleanel -> player:bob";
        let sig = world.sign(grant);

        public.verify(grant, &sig).unwrap();
    }

    #[test]
    fn a_fellow_player_cannot_forge_a_grant() {
        let world = WorldToken::create("Eldermere").unwrap();
        let mallory = WorldToken::create("Eldermere").unwrap();

        let forged = b"class:rogue -> player:mallory";
        let sig = mallory.sign(forged);

        assert!(matches!(
            world.public().verify(forged, &sig),
            Err(CryptoError::BadSignature)
        ));
    }

    #[test]
    fn a_swapped_id_is_caught() {
        let real = WorldToken::create("Eldermere").unwrap();
        let mut forged = real.public();
        forged.world_id = Id::derive("world", &[b"made up"]);

        let message = b"anything";
        let sig = real.sign(message);
        assert!(forged.verify(message, &sig).is_err());
    }

    #[test]
    fn file_round_trip() {
        let dir = std::env::temp_dir().join(format!("aw-world-{}", std::process::id()));
        let path = dir.join("owner.key");

        let world = WorldToken::create("Eldermere").unwrap();
        world.save(&path).unwrap();

        let again = WorldToken::load(&path).unwrap();
        assert_eq!(again.world_id(), world.world_id());
        assert_eq!(again.name(), "Eldermere");

        std::fs::remove_dir_all(&dir).ok();
    }
}
