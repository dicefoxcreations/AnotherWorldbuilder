//! The identity file: display name plus key pairs, stored locally.
//!
//! On first start everyone — GM and player alike — picks a display name.
//! The app generates two key pairs for it: Ed25519 for signing, and an
//! X25519 one whose public half is part of what makes the id. Neither
//! private key ever leaves the device.
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

    /// The identity file's bytes — private keys included.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        let file = IdentityFile {
            version: IDENTITY_VERSION,
            name: self.name.clone(),
            id: self.id.clone(),
            sign_key: hex::encode(self.sign.to_bytes()),
            seal_key: hex::encode(self.seal.to_bytes()),
        };
        Ok(serde_json::to_vec_pretty(&file)?)
    }

    /// Written whole or not at all: a renaming cut short by a crash or an
    /// empty battery leaves the old file, never half of one — the keys are
    /// all one has. Private from the first byte on, where the OS knows that.
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".tmp");
        let fresh = path.with_file_name(name);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&fresh)?;
        file.write_all(&self.to_json()?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&fresh, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read(path)?)
    }

    /// An identity from the bytes of its file — checked like `load` does.
    pub fn from_json(raw: &[u8]) -> Result<Self> {
        let file: IdentityFile = serde_json::from_slice(raw)?;
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
#[path = "identity_tests.rs"]
mod tests;
