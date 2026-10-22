//! Signed notes where a member's device can be reached, independent of their transport.
use aw_crypto::{Identity, IdentityPublic};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const PEER: &str = "anotherworldbuilder/peer/1";

/// Where a member's device can be reached over the internet — left in the
/// group's shared folder, so the others can connect directly. Signed: nobody
/// sends the others towards an address in someone else's name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeerNote {
    pub world: String,
    /// The device's endpoint — its QUIC key.
    pub endpoint: String,
    /// Addresses to try, as `ip:port`.
    pub addrs: Vec<String>,
    pub identity: IdentityPublic,
    /// Seconds since 1970, by the writing side's clock.
    pub at: u64,
    pub signature: String,
}

impl PeerNote {
    pub fn new(world: &str, endpoint: &str, addrs: Vec<String>, me: &Identity, at: u64) -> Result<Self> {
        let identity = me.public();
        let signature = hex::encode(me.sign(&peer_bytes(world, endpoint, &addrs, &identity, at)?));
        Ok(PeerNote { world: world.to_string(), endpoint: endpoint.to_string(), addrs, identity, at, signature })
    }

    /// For this world, and by whom it claims to be.
    pub fn check(&self, world: &str) -> Result<()> {
        if self.world != world {
            return Err("a note for another world".into());
        }
        if !self.identity.is_genuine() {
            return Err("an id that does not match its keys".into());
        }
        let signature: [u8; 64] = hex::decode(&self.signature)?.try_into().map_err(|_| "a signature has 64 bytes")?;
        self.identity.verify(&peer_bytes(world, &self.endpoint, &self.addrs, &self.identity, self.at)?, &signature)?;
        Ok(())
    }
}

fn peer_bytes(world: &str, endpoint: &str, addrs: &[String], identity: &IdentityPublic, at: u64) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(PEER, world, endpoint, addrs, identity, at))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_peer_note_holds_only_as_written() {
        let bob = Identity::create("Bob").unwrap();
        let note = PeerNote::new("world:1", "e1", vec!["1.2.3.4:5".into()], &bob, 7).unwrap();
        assert!(note.check("world:1").is_ok());
        assert!(note.check("world:2").is_err());
        let elsewhere = PeerNote { addrs: vec!["6.6.6.6:6".into()], ..note.clone() };
        assert!(elsewhere.check("world:1").is_err(), "the addresses are signed");
        let eve = Identity::create("Eve").unwrap();
        let forged = PeerNote { identity: bob.public(), ..PeerNote::new("world:1", "e1", vec![], &eve, 7).unwrap() };
        assert!(forged.check("world:1").is_err());
    }

}
