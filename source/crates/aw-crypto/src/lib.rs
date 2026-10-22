//! Cryptographic foundations for AnotherWorldbuilder.
//!
//! This crate is the only place in the project that touches key material.
//! It invents no cryptography: X25519 for wrapping, XChaCha20-Poly1305 for
//! content, HKDF-SHA256 for derived keys, Ed25519 for signatures, Argon2id
//! for a passphrase.
//!
//! Every serialized format carries a version number from the first byte on.
//! Without it the format could never change later without making existing
//! worlds unreadable.

pub mod aead;
pub mod ids;
pub mod identity;
pub mod kdf;
pub mod passphrase;
pub mod seal;
pub mod world;
pub mod write;

pub use aead::{decrypt, encrypt, ContentKey};
pub use ids::Id;
pub use identity::{Identity, IdentityPublic};
pub use seal::{seal_key, unseal_key, WrappedKey};
pub use world::{WorldId, WorldPublic, WorldToken};
pub use write::{verify_write, WriteKey};

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("random source unavailable: {0}")]
    Random(#[from] getrandom::Error),

    #[error("unknown format version {found}, this application knows up to {known}")]
    UnknownVersion { found: u8, known: u8 },

    #[error("data too short: {context} needs at least {expected} bytes, has {actual}")]
    TooShort {
        context: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("decryption failed — wrong key or tampered data")]
    Decrypt,

    #[error("invalid signature")]
    BadSignature,

    #[error("invalid key material: {0}")]
    BadKey(String),

    #[error("identity file is malformed: {0}")]
    Format(#[from] serde_json::Error),

    #[error("file not readable or writable: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CryptoError>;

/// Cryptographically secure random bytes from the operating system.
pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf)?;
    Ok(buf)
}

/// Restricts file permissions to "owner only", where the OS supports that.
///
/// The identity file and the world token are key material. They have no
/// business in a world-readable file.
pub(crate) fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
