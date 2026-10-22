//! Who can decrypt what — the security boundary of the project.
//!
//! This is where the key hierarchy lives: attribute keys per epoch, slot
//! keys derived from them, and below those the data key of a layer.
//!
//! App, relay and bundle export all share this crate. There is no second
//! implementation of the rule — neither in TypeScript nor in the relay.
//! That is exactly where the gaps would otherwise appear.

pub mod header;
pub mod keyring;
pub mod slot;

pub use header::{open_dek, wrap_dek, LayerHeader, HEADER_VERSION};
pub use keyring::{AttributeKey, GrantedKey, HeldKeys, Keyring};
pub use slot::{AttributeAt, Slot};

#[derive(Debug, thiserror::Error)]
pub enum PolicyKeyError {
    #[error(transparent)]
    Crypto(#[from] aw_crypto::CryptoError),

    #[error("'{0}' is not a valid slot address")]
    BadSlot(String),

    #[error("attribute '{0}' does not exist in this world")]
    UnknownAttribute(String),
}

pub type Result<T> = std::result::Result<T, PolicyKeyError>;
