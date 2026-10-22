//! Slots: the addresses in the layer header under which a data key lies.
//!
//! A slot names exactly which attribute keys in which epoch are needed to
//! open it:
//!
//! ```text
//! class:rogue@3                       one attribute
//! class:rogue@3+faction:guild@1       AND over two attributes
//! ```
//!
//! The notation is sorted and therefore unique: the same AND group always
//! yields the same slot, however the policy wrote it.

use aw_model::Attribute;
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use crate::PolicyKeyError;

/// An attribute in a particular epoch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttributeAt {
    pub attribute: Attribute,
    pub epoch: u32,
}

impl AttributeAt {
    pub fn new(attribute: Attribute, epoch: u32) -> Self {
        AttributeAt { attribute, epoch }
    }
}

impl fmt::Display for AttributeAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.attribute, self.epoch)
    }
}

impl FromStr for AttributeAt {
    type Err = PolicyKeyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (attr, epoch) = text
            .rsplit_once('@')
            .ok_or_else(|| PolicyKeyError::BadSlot(text.to_string()))?;
        Ok(AttributeAt {
            attribute: attr
                .parse()
                .map_err(|_| PolicyKeyError::BadSlot(text.to_string()))?,
            epoch: epoch
                .parse()
                .map_err(|_| PolicyKeyError::BadSlot(text.to_string()))?,
        })
    }
}

/// The address of a wrapped data key in the layer header.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(Vec<AttributeAt>);

impl Slot {
    pub fn new(parts: impl IntoIterator<Item = AttributeAt>) -> Result<Self, PolicyKeyError> {
        let set: BTreeSet<AttributeAt> = parts.into_iter().collect();
        if set.is_empty() {
            return Err(PolicyKeyError::BadSlot("empty".into()));
        }
        Ok(Slot(set.into_iter().collect()))
    }

    pub fn parts(&self) -> &[AttributeAt] {
        &self.0
    }

    pub fn is_single(&self) -> bool {
        self.0.len() == 1
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|p| p.to_string()).collect();
        f.write_str(&parts.join("+"))
    }
}

impl FromStr for Slot {
    type Err = PolicyKeyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let parts = text
            .split('+')
            .map(AttributeAt::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        Slot::new(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, epoch: u32) -> AttributeAt {
        AttributeAt::new(text.parse().unwrap(), epoch)
    }

    #[test]
    fn a_single_slot() {
        let slot = Slot::new([at("class:rogue", 3)]).unwrap();
        assert_eq!(slot.to_string(), "class:rogue@3");
        assert!(slot.is_single());
    }

    #[test]
    fn order_does_not_matter() {
        let a = Slot::new([at("class:rogue", 3), at("faction:guild", 1)]).unwrap();
        let b = Slot::new([at("faction:guild", 1), at("class:rogue", 3)]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_string(), b.to_string());
    }

    #[test]
    fn round_trip_through_text() {
        let slot = Slot::new([at("class:rogue", 3), at("faction:guild", 1)]).unwrap();
        assert_eq!(slot.to_string().parse::<Slot>().unwrap(), slot);
    }

    #[test]
    fn the_epoch_tells_slots_apart() {
        let old = Slot::new([at("class:rogue", 1)]).unwrap();
        let new = Slot::new([at("class:rogue", 2)]).unwrap();
        assert_ne!(old, new);
    }

    #[test]
    fn nonsense_is_rejected() {
        assert!("without-epoch".parse::<Slot>().is_err());
        assert!("class:rogue@notanumber".parse::<Slot>().is_err());
        assert!("".parse::<Slot>().is_err());
    }
}
