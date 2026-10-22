//! Bytes locked with a passphrase — for a backup of one's keys that may lie
//! anywhere.
//!
//! Argon2id turns the passphrase into the key, slow and memory-hungry on
//! purpose: whoever finds the file has to try passphrases one by one at that
//! cost. The content is sealed with XChaCha20-Poly1305 like everything else.
//!
//! ```text
//! [version:1][salt:16][aead ciphertext]
//! ```

use crate::{decrypt, encrypt, ContentKey, CryptoError};
use argon2::{Algorithm, Argon2, Params, Version};

const VERSION: u8 = 1;
const SALT: usize = 16;

/// Argon2id with 64 MiB and three passes: about half a second on a laptop.
fn key_of(passphrase: &str, salt: &[u8]) -> Result<ContentKey, CryptoError> {
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|e| CryptoError::BadKey(e.to_string()))?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| CryptoError::BadKey(e.to_string()))?;
    Ok(ContentKey::from_bytes(key))
}

/// Locks `plaintext` with `passphrase`.
pub fn lock(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut salt = [0u8; SALT];
    getrandom::fill(&mut salt)?;
    let mut out = vec![VERSION];
    out.extend_from_slice(&salt);
    out.extend_from_slice(&encrypt(&key_of(passphrase, &salt)?, plaintext)?);
    Ok(out)
}

/// Opens what `lock` locked — a wrong passphrase fails like tampered bytes.
pub fn unlock(passphrase: &str, locked: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if locked.len() < 1 + SALT {
        return Err(CryptoError::TooShort { context: "locked backup", expected: 1 + SALT, actual: locked.len() });
    }
    if locked[0] != VERSION {
        return Err(CryptoError::UnknownVersion { found: locked[0], known: VERSION });
    }
    let (salt, sealed) = locked[1..].split_at(SALT);
    decrypt(&key_of(passphrase, salt)?, sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_passphrase_opens_it() {
        let locked = lock("correct horse", b"identity and keys").unwrap();
        assert_eq!(unlock("correct horse", &locked).unwrap(), b"identity and keys");
        assert!(unlock("wrong horse", &locked).is_err());
        let mut tampered = locked.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(unlock("correct horse", &tampered).is_err());
    }

    #[test]
    fn the_same_passphrase_locks_differently_each_time() {
        assert_ne!(lock("p", b"x").unwrap(), lock("p", b"x").unwrap());
    }
}
