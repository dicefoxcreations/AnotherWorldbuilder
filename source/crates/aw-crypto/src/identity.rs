//! The identity file: display name plus key pairs, stored locally.
//!
//! On first start everyone — GM and player alike — picks a display name.
//! The app generates two key pairs for it: Ed25519 for signing, X25519 for
//! unwrapping attribute keys. Neither ever leaves the device.
//!
//! **The name is a label, the identity is the key.** The id is derived from
//! the public keys, never from the name. Whoever renames themselves keeps all
//! access — and conversely cannot write themselves into someone else's access
//! by taking their name.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::ids::Id;
use crate::{random_bytes, CryptoError, Result};

/// Format version of the identity file.
pub const IDENTITY_VERSION: u8 = 1;

/// A complete identity including its private keys.
pub struct Identity {
    name: String,
    id: Id,
    sign: SigningKey,
    seal: StaticSecret,
}

/// The public part that is transferred when joining.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityPublic {
    pub name: String,
    pub id: Id,
    /// Ed25519 verifying key, hex-encoded.
    pub verify_key: String,
    /// X25519 key for wrapping, hex-encoded.
    pub seal_key: String,
}

#[derive(Serialize, Deserialize)]
struct IdentityFile {
    version: u8,
    name: String,
    id: Id,
    sign_key: String,
    seal_key: String,
}

impl Identity {
    /// Creates a fresh identity with the given display name.
    pub fn create(name: impl Into<String>) -> Result<Self> {
        let sign = SigningKey::from_bytes(&random_bytes::<32>()?);
        let seal = StaticSecret::from(random_bytes::<32>()?);
        Ok(Self::assemble(name.into(), sign, seal))
    }

    fn assemble(name: String, sign: SigningKey, seal: StaticSecret) -> Self {
        let verify = sign.verifying_key();
        let seal_pub = PublicKey::from(&seal);
        let id = Id::derive("id", &[verify.as_bytes(), seal_pub.as_bytes()]);
        Identity {
            name,
            id,
            sign,
            seal,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Changes the display name. The id stays, access stays.
    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn public(&self) -> IdentityPublic {
        IdentityPublic {
            name: self.name.clone(),
            id: self.id.clone(),
            verify_key: hex::encode(self.sign.verifying_key().as_bytes()),
            seal_key: hex::encode(PublicKey::from(&self.seal).as_bytes()),
        }
    }


    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.sign.sign(message).to_bytes()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = IdentityFile {
            version: IDENTITY_VERSION,
            name: self.name.clone(),
            id: self.id.clone(),
            sign_key: hex::encode(self.sign.to_bytes()),
            seal_key: hex::encode(self.seal.to_bytes()),
        };
        std::fs::write(path, serde_json::to_vec_pretty(&file)?)?;
        crate::restrict_permissions(path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read(path)?;
        let file: IdentityFile = serde_json::from_slice(&raw)?;
        if file.version != IDENTITY_VERSION {
            return Err(CryptoError::UnknownVersion {
                found: file.version,
                known: IDENTITY_VERSION,
            });
        }

        let sign_bytes: [u8; 32] = hex::decode(&file.sign_key)
            .map_err(|e| CryptoError::BadKey(e.to_string()))?
            .try_into()
            .map_err(|_| CryptoError::BadKey("signing key is not 32 bytes".into()))?;
        let seal_bytes: [u8; 32] = hex::decode(&file.seal_key)
            .map_err(|e| CryptoError::BadKey(e.to_string()))?
            .try_into()
            .map_err(|_| CryptoError::BadKey("wrapping key is not 32 bytes".into()))?;

        let identity = Self::assemble(
            file.name,
            SigningKey::from_bytes(&sign_bytes),
            StaticSecret::from(seal_bytes),
        );

        // The id in the file is only a convenience for humans. What we rely
        // on is the freshly derived one.
        if identity.id != file.id {
            return Err(CryptoError::BadKey(
                "id in the file does not match the keys".into(),
            ));
        }
        Ok(identity)
    }
}

impl IdentityPublic {
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> Result<()> {
        let bytes: [u8; 32] = hex::decode(&self.verify_key)
            .map_err(|e| CryptoError::BadKey(e.to_string()))?
            .try_into()
            .map_err(|_| CryptoError::BadKey("verifying key is not 32 bytes".into()))?;
        let verify =
            VerifyingKey::from_bytes(&bytes).map_err(|e| CryptoError::BadKey(e.to_string()))?;
        verify
            .verify(message, &Signature::from_bytes(signature))
            .map_err(|_| CryptoError::BadSignature)
    }


    /// Whether the id really belongs to these keys.
    ///
    /// The id is a fingerprint of both public keys. A card that arrives over
    /// the network is only trusted once its keys produce its id — otherwise
    /// someone could send a known id together with keys of their own.
    pub fn is_genuine(&self) -> bool {
        let (Ok(verify), Ok(seal)) = (hex::decode(&self.verify_key), hex::decode(&self.seal_key)) else {
            return false;
        };
        verify.len() == 32 && seal.len() == 32 && Id::derive("id", &[&verify, &seal]) == self.id
    }

    /// Short fingerprint to read aloud when joining at the table.
    pub fn fingerprint(&self) -> String {
        self.id
            .short()
            .as_bytes()
            .chunks(4)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_is_genuine_only_with_its_own_keys() {
        let bob = Identity::create("Bob").unwrap().public();
        assert!(bob.is_genuine());
        let eve = Identity::create("Eve").unwrap().public();
        let forged = IdentityPublic { id: bob.id.clone(), ..eve };
        assert!(!forged.is_genuine(), "Bob's id with Eve's keys");
    }

    #[test]
    fn the_id_hangs_on_the_key_not_the_name() {
        let mut bob = Identity::create("Bob").unwrap();
        let before = bob.id().clone();
        bob.rename("Robert the Magnificent");
        assert_eq!(bob.id(), &before);
    }

    #[test]
    fn two_identities_of_the_same_name_differ() {
        let a = Identity::create("Bob").unwrap();
        let b = Identity::create("Bob").unwrap();
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn the_signature_is_checked() {
        let bob = Identity::create("Bob").unwrap();
        let sig = bob.sign(b"Eleanel belongs to Bob");
        bob.public().verify(b"Eleanel belongs to Bob", &sig).unwrap();
    }

    #[test]
    fn a_foreign_signature_fails() {
        let bob = Identity::create("Bob").unwrap();
        let mallory = Identity::create("Mallory").unwrap();
        let sig = mallory.sign(b"Eleanel belongs to Mallory");
        assert!(matches!(
            bob.public().verify(b"Eleanel belongs to Mallory", &sig),
            Err(CryptoError::BadSignature)
        ));
    }

    #[test]
    fn a_changed_message_fails() {
        let bob = Identity::create("Bob").unwrap();
        let sig = bob.sign(b"Bob is a player");
        assert!(bob.public().verify(b"Bob is the game master", &sig).is_err());
    }

    #[test]
    fn file_round_trip() {
        let dir = std::env::temp_dir().join(format!("aw-test-{}", std::process::id()));
        let path = dir.join("identity.json");
        let bob = Identity::create("Bob").unwrap();
        bob.save(&path).unwrap();

        let again = Identity::load(&path).unwrap();
        assert_eq!(again.id(), bob.id());
        assert_eq!(again.name(), "Bob");

        // Signatures of the loaded key verify against the old identity.
        let sig = again.sign(b"probe");
        bob.public().verify(b"probe", &sig).unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_tampered_id_is_detected() {
        let dir = std::env::temp_dir().join(format!("aw-test-tamper-{}", std::process::id()));
        let path = dir.join("identity.json");
        Identity::create("Bob").unwrap().save(&path).unwrap();

        let mut file: IdentityFile =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        file.id = Id::derive("id", &[b"made up"]);
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

        assert!(Identity::load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
