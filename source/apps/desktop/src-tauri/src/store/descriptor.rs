//! Public world descriptors. Earlier development formats are not supported.
use super::*;

const WORLD_LAYOUT: u8 = 3;

#[derive(Serialize, Deserialize)]
pub(super) struct Descriptor {
    pub version: u8,
    pub public: WorldPublic,
}

impl Store {
    pub(super) fn read_descriptor(&self, path: &Path) -> Result<Descriptor> {
        let descriptor: Descriptor = serde_json::from_slice(&std::fs::read(path)?)?;
        if descriptor.version != WORLD_LAYOUT {
            return Err(StoreError::UnknownVersion { found: descriptor.version, known: WORLD_LAYOUT });
        }
        if !descriptor.public.is_genuine() {
            return Err(StoreError::Journal("The world's public key does not match its ID.".into()));
        }
        Ok(descriptor)
    }

    pub(super) fn save_descriptor(&self, public: &WorldPublic) -> Result<()> {
        if !public.is_genuine() {
            return Err(StoreError::Journal("The world's public key does not match its ID.".into()));
        }
        aw_store::durable::write(&self.world_dir(public.world_id.as_str()).join("world.json"),
            &serde_json::to_vec_pretty(&Descriptor { version: WORLD_LAYOUT, public: public.clone() })?)?;
        Ok(())
    }

}
