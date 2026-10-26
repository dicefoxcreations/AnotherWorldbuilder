//! Who is who in AnotherWorldbuilder: identities (Ed25519 to sign, an
//! X25519 key that is part of what makes an id), ids, and a passphrase lock
//! for one's own key backup (Argon2id, XChaCha20-Poly1305). It invents no
//! cryptography.

pub mod aead;
pub mod ids;
pub mod identity;
pub mod passphrase;

pub use aead::{decrypt, encrypt, ContentKey};
pub use ids::Id;
pub use identity::{Identity, IdentityPublic};

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
