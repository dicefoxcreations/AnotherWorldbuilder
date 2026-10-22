//! The write key of a layer: whoever holds it may change the layer.
//!
//! Reading and writing are separate rights. Reading hangs on the data key,
//! writing on this key pair: its private half is wrapped for the edit rule
//! exactly as the data key is wrapped for the view rule, its public half
//! stands openly in the layer's header. A change signed with it proves that
//! its author could open the edit rule — without anyone having to know who
//! holds which attribute.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::{random_bytes, ContentKey, CryptoError, Result};

pub struct WriteKey(SigningKey);

impl WriteKey {
    /// A fresh key pair, and the seed to wrap for the edit rule.
    pub fn generate() -> Result<(Self, ContentKey)> {
        let seed = random_bytes::<32>()?;
        Ok((WriteKey(SigningKey::from_bytes(&seed)), ContentKey::from_bytes(seed)))
    }

    /// The key pair from its unwrapped seed.
    pub fn from_seed(seed: &ContentKey) -> Self {
        WriteKey(SigningKey::from_bytes(seed.as_bytes()))
    }

    /// The public half, hex-encoded — what a change is checked against.
    pub fn public(&self) -> String {
        hex::encode(self.0.verifying_key().as_bytes())
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }
}

/// Checks a signature against the public half of a write key.
pub fn verify_write(public: &str, message: &[u8], signature: &[u8; 64]) -> Result<()> {
    let bytes: [u8; 32] = hex::decode(public)
        .map_err(|e| CryptoError::BadKey(e.to_string()))?
        .try_into()
        .map_err(|_| CryptoError::BadKey("write key is not 32 bytes".into()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|e| CryptoError::BadKey(e.to_string()))?
        .verify(message, &Signature::from_bytes(signature))
        .map_err(|_| CryptoError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_seed_signs_what_the_public_half_accepts() {
        let (key, seed) = WriteKey::generate().unwrap();
        let again = WriteKey::from_seed(&seed);
        assert_eq!(key.public(), again.public());
        let signature = again.sign(b"a change");
        assert!(verify_write(&key.public(), b"a change", &signature).is_ok());
        assert!(verify_write(&key.public(), b"another change", &signature).is_err());
        let (other, _) = WriteKey::generate().unwrap();
        assert!(verify_write(&other.public(), b"a change", &signature).is_err());
    }
}
