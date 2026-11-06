//! Symmetric content encryption with XChaCha20-Poly1305.
//!
//! Format of a ciphertext:
//!
//! ```text
//! [version:1][nonce:24][ciphertext + tag:16]
//! ```
//!
//! The version byte comes first, so the format can change later without
//! making existing worlds unreadable.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{random_bytes, CryptoError, Result};

/// Current format version for ciphertexts of this crate.
pub const AEAD_VERSION: u8 = 1;

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// A symmetric key for exactly one layer and one epoch.
///
/// Overwritten when it goes out of scope.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentKey(#[serde(with = "hex_bytes")] pub(crate) [u8; 32]);

impl ContentKey {
    /// Generates a fresh data key.
    pub fn generate() -> Result<Self> {
        Ok(ContentKey(random_bytes::<32>()?))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ContentKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for ContentKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for ContentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never key material in logs.
        f.write_str("ContentKey(<secret>)")
    }
}

/// Encrypts `plaintext` with `key`.
pub fn encrypt(key: &ContentKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    let nonce_bytes = random_bytes::<NONCE_LEN>()?;
    let nonce = XNonce::from(nonce_bytes);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| CryptoError::Decrypt)?;

    let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    out.push(AEAD_VERSION);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts what `encrypt` produced.
///
/// Fails when the key does not match or the data was changed — the two are
/// indistinguishable from outside, and that is intentional.
pub fn decrypt(key: &ContentKey, sealed: &[u8]) -> Result<Vec<u8>> {
    let min = 1 + NONCE_LEN + TAG_LEN;
    if sealed.len() < min {
        return Err(CryptoError::TooShort {
            context: "ciphertext",
            expected: min,
            actual: sealed.len(),
        });
    }
    if sealed[0] != AEAD_VERSION {
        return Err(CryptoError::UnknownVersion {
            found: sealed[0],
            known: AEAD_VERSION,
        });
    }

    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&sealed[1..1 + NONCE_LEN]);
    let nonce = XNonce::from(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    cipher
        .decrypt(&nonce, &sealed[1 + NONCE_LEN..])
        .map_err(|_| CryptoError::Decrypt)
}

/// Hex encoding for 32-byte keys in JSON, so the files are readable in the
/// options.
pub(crate) mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let text = String::deserialize(d)?;
        let raw = hex::decode(&text).map_err(serde::de::Error::custom)?;
        raw.try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = ContentKey::generate().unwrap();
        let sealed = encrypt(&key, b"The crypt lies beneath the altar.").unwrap();
        let open = decrypt(&key, &sealed).unwrap();
        assert_eq!(open, b"The crypt lies beneath the altar.");
    }

    #[test]
    fn a_wrong_key_does_not_open() {
        let key = ContentKey::generate().unwrap();
        let foreign = ContentKey::generate().unwrap();
        let sealed = encrypt(&key, b"secret").unwrap();
        assert!(matches!(
            decrypt(&foreign, &sealed),
            Err(CryptoError::Decrypt)
        ));
    }

    #[test]
    fn changed_data_is_detected() {
        let key = ContentKey::generate().unwrap();
        let mut sealed = encrypt(&key, b"secret").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(matches!(
            decrypt(&key, &sealed),
            Err(CryptoError::Decrypt)
        ));
    }

    #[test]
    fn the_version_byte_comes_first_and_is_checked() {
        let key = ContentKey::generate().unwrap();
        let mut sealed = encrypt(&key, b"secret").unwrap();
        assert_eq!(sealed[0], AEAD_VERSION);

        sealed[0] = 99;
        match decrypt(&key, &sealed) {
            Err(CryptoError::UnknownVersion { found, known }) => {
                assert_eq!(found, 99);
                assert_eq!(known, AEAD_VERSION);
            }
            other => panic!("expected UnknownVersion, got {other:?}"),
        }
    }

    #[test]
    fn two_ciphertexts_of_the_same_text_differ() {
        // The nonce must be fresh per message, or the cipher leaks.
        let key = ContentKey::generate().unwrap();
        let a = encrypt(&key, b"same text").unwrap();
        let b = encrypt(&key, b"same text").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn input_that_is_too_short_says_so_plainly() {
        let key = ContentKey::generate().unwrap();
        assert!(matches!(
            decrypt(&key, &[AEAD_VERSION, 0, 0]),
            Err(CryptoError::TooShort { .. })
        ));
    }
}
