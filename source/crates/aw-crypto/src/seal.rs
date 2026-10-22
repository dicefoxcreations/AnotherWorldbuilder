//! Key wrapping: sealing a data key for exactly one recipient.
//!
//! This is the building block the permission model rests on. A layer has a
//! random data key; it is wrapped once per entitled attribute and stored in
//! the layer header. Whoever holds the matching private key unwraps it —
//! everyone else holds noise.
//!
//! Scheme: ephemeral X25519, HKDF-SHA256 over the Diffie-Hellman result
//! together with both public keys, then XChaCha20-Poly1305.
//!
//! ```text
//! [version:1][ephemeral public key:32][aead ciphertext]
//! ```

use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::aead::{decrypt, encrypt, ContentKey};
use crate::kdf::{derive32, INFO_WRAP};
use crate::{random_bytes, CryptoError, Result};

/// Format version of the wrapping.
pub const SEAL_VERSION: u8 = 1;

/// A data key sealed for one recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    pub version: u8,
    /// Ephemeral public key, hex-encoded.
    pub ephemeral: String,
    /// The ciphertext of the data key, hex-encoded.
    pub sealed: String,
}

/// Wraps `dek` for the holder of `recipient`.
pub fn seal_key(recipient: &PublicKey, dek: &ContentKey) -> Result<WrappedKey> {
    let ephemeral_secret = StaticSecret::from(random_bytes::<32>()?);
    let ephemeral_public = PublicKey::from(&ephemeral_secret);
    let shared = ephemeral_secret.diffie_hellman(recipient);

    let wrapping = ContentKey::from_bytes(wrapping_key(
        shared.as_bytes(),
        ephemeral_public.as_bytes(),
        recipient.as_bytes(),
    ));
    let sealed = encrypt(&wrapping, dek.as_bytes())?;

    Ok(WrappedKey {
        version: SEAL_VERSION,
        ephemeral: hex::encode(ephemeral_public.as_bytes()),
        sealed: hex::encode(sealed),
    })
}

/// Unwraps what `seal_key` sealed for the holder of `secret`.
pub fn unseal_key(secret: &StaticSecret, wrapped: &WrappedKey) -> Result<ContentKey> {
    if wrapped.version != SEAL_VERSION {
        return Err(CryptoError::UnknownVersion {
            found: wrapped.version,
            known: SEAL_VERSION,
        });
    }

    let ephemeral_bytes: [u8; 32] = hex::decode(&wrapped.ephemeral)
        .map_err(|e| CryptoError::BadKey(e.to_string()))?
        .try_into()
        .map_err(|_| CryptoError::BadKey("ephemeral key is not 32 bytes".into()))?;
    let ephemeral_public = PublicKey::from(ephemeral_bytes);

    let own_public = PublicKey::from(secret);
    let shared = secret.diffie_hellman(&ephemeral_public);

    let wrapping = ContentKey::from_bytes(wrapping_key(
        shared.as_bytes(),
        ephemeral_public.as_bytes(),
        own_public.as_bytes(),
    ));

    let sealed = hex::decode(&wrapped.sealed).map_err(|e| CryptoError::BadKey(e.to_string()))?;
    let raw = decrypt(&wrapping, &sealed)?;
    let bytes: [u8; 32] = raw
        .try_into()
        .map_err(|_| CryptoError::BadKey("data key is not 32 bytes".into()))?;
    Ok(ContentKey::from_bytes(bytes))
}

/// Binds the wrapping key to both public keys.
///
/// Without this binding a ciphertext could be reinterpreted for another
/// recipient; with it, it belongs to exactly this key pair.
fn wrapping_key(shared: &[u8; 32], ephemeral: &[u8; 32], recipient: &[u8; 32]) -> [u8; 32] {
    let mut ikm = Vec::with_capacity(96);
    ikm.extend_from_slice(shared);
    ikm.extend_from_slice(ephemeral);
    ikm.extend_from_slice(recipient);
    derive32(&ikm, None, INFO_WRAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> (StaticSecret, PublicKey) {
        let secret = StaticSecret::from(random_bytes::<32>().unwrap());
        let public = PublicKey::from(&secret);
        (secret, public)
    }

    #[test]
    fn the_recipient_unpacks() {
        let (secret, public) = keypair();
        let dek = ContentKey::generate().unwrap();

        let wrapped = seal_key(&public, &dek).unwrap();
        let again = unseal_key(&secret, &wrapped).unwrap();

        assert_eq!(dek.as_bytes(), again.as_bytes());
    }

    #[test]
    fn a_stranger_does_not_unpack() {
        let (_, public) = keypair();
        let (foreign_secret, _) = keypair();
        let dek = ContentKey::generate().unwrap();

        let wrapped = seal_key(&public, &dek).unwrap();
        assert!(unseal_key(&foreign_secret, &wrapped).is_err());
    }

    #[test]
    fn wrapping_twice_gives_different_ciphertexts() {
        let (_, public) = keypair();
        let dek = ContentKey::generate().unwrap();

        let a = seal_key(&public, &dek).unwrap();
        let b = seal_key(&public, &dek).unwrap();
        assert_ne!(a.sealed, b.sealed);
        assert_ne!(a.ephemeral, b.ephemeral);
    }

    #[test]
    fn the_same_dek_for_two_recipients() {
        // The OR case: one layer, two entitled attributes.
        let (secret_a, public_a) = keypair();
        let (secret_b, public_b) = keypair();
        let dek = ContentKey::generate().unwrap();

        let for_a = seal_key(&public_a, &dek).unwrap();
        let for_b = seal_key(&public_b, &dek).unwrap();

        assert_eq!(unseal_key(&secret_a, &for_a).unwrap().as_bytes(), dek.as_bytes());
        assert_eq!(unseal_key(&secret_b, &for_b).unwrap().as_bytes(), dek.as_bytes());
        // Nothing works crosswise.
        assert!(unseal_key(&secret_a, &for_b).is_err());
    }

    #[test]
    fn an_unknown_version_is_rejected() {
        let (secret, public) = keypair();
        let dek = ContentKey::generate().unwrap();
        let mut wrapped = seal_key(&public, &dek).unwrap();
        wrapped.version = 99;

        assert!(matches!(
            unseal_key(&secret, &wrapped),
            Err(CryptoError::UnknownVersion { found: 99, .. })
        ));
    }
}
