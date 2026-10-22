//! Articles and layers.
//!
//! There is exactly one base type — the article — and everything else is a
//! template on top of it. An article is not a document but a stack of
//! layers; every layer carries its own policy and is encrypted on its own.
//!
//! The choice sits on the layer, not on the article. A place thus carries
//! its public description, the rogue's note about the back door and the GM's
//! note about the guards in a single document — and everyone sees exactly
//! their part of it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

use crate::attribute::Policy;
use crate::calendar::WorldDate;

macro_rules! ulid_id {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Ulid);

        impl $name {
            pub fn new() -> Self {
                $name(Ulid::generate())
            }
            pub fn as_ulid(&self) -> Ulid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", $prefix, self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self)
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }

        impl FromStr for $name {
            type Err = ModelError;
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let body = text.strip_prefix(concat!($prefix, ":")).unwrap_or(text);
                Ulid::from_string(body)
                    .map($name)
                    .map_err(|_| ModelError::BadId(text.to_string()))
            }
        }
    };
}

ulid_id!(ArticleId, "art", "Identifier of an article. Immutable — wiki links point at it.");
ulid_id!(LayerId, "lay", "Identifier of a layer within an article.");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("'{0}' is not a valid identifier")]
    BadId(String),

    #[error("an article needs at least one layer")]
    NoLayers,

    #[error("layer {0} does not exist in this article")]
    NoSuchLayer(LayerId),
}

/// What a layer is meant for. Every layer goes to every member, encrypted;
/// who reads it decide the keys alone. The level says whether players are
/// ever meant to: what only the GM reads, what is held back to be revealed,
/// what is open lore.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// Only for the GM — never meant to be revealed.
    #[default]
    GmOnly,
    /// A secret meant to be revealed: everyone holds it already, the key follows later.
    Sealed,
    /// Public lore, encrypted with the campaign key.
    Open,
}

impl Level {
    /// Whether players are ever meant to read it.
    pub fn for_players(&self) -> bool {
        !matches!(self, Level::GmOnly)
    }
}

/// Who may write to a layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteAccess {
    /// Only whoever created it — a player journal, say.
    Owner,
    #[default]
    Gm,
    Players,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FieldValue {
    Text(String),
    Number(f64),
    WorldDate(WorldDate),
    Reference(ArticleId),
    ReferenceList(Vec<ArticleId>),
    Image { hash: String },
}

/// A layer: a unit of access and at the same time a unit of encryption.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    /// Heading of the section in the interface.
    pub title: String,
    pub policy: Policy,
    #[serde(default)]
    pub level: Level,
    #[serde(default)]
    pub write: WriteAccess,
    /// Increases on revocation and on a history cut.
    #[serde(default = "first_epoch")]
    pub epoch: u32,
    #[serde(default)]
    pub fields: BTreeMap<String, FieldValue>,
    /// Rich text as a ProseMirror document.
    #[serde(default)]
    pub body: serde_json::Value,
}

fn first_epoch() -> u32 {
    1
}

impl Layer {
    pub fn new(title: impl Into<String>, policy: Policy) -> Self {
        Layer {
            id: LayerId::new(),
            title: title.into(),
            policy,
            level: Level::default(),
            write: WriteAccess::default(),
            epoch: first_epoch(),
            fields: BTreeMap::new(),
            body: serde_json::Value::Null,
        }
    }

    pub fn with_level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    pub fn with_body(mut self, body: serde_json::Value) -> Self {
        self.body = body;
        self
    }

    /// Moves the layer to the next epoch.
    ///
    /// The caller must then assign a fresh data key — exactly that cuts off
    /// history and locks out those who left.
    pub fn next_epoch(&mut self) -> u32 {
        self.epoch += 1;
        self.epoch
    }
}

/// An article: identifier, template type and a stack of layers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Article {
    pub id: ArticleId,
    /// Template slug: `place`, `character`, `organization`, `session-log`, …
    pub article_type: String,
    pub slug: String,
    pub layers: Vec<Layer>,
}

impl Article {
    pub fn new(article_type: impl Into<String>, slug: impl Into<String>, base: Layer) -> Self {
        Article {
            id: ArticleId::new(),
            article_type: article_type.into(),
            slug: slug.into(),
            layers: vec![base],
        }
    }

    pub fn push_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// The layers a viewer with these attributes can open.
    ///
    /// This is the view "preview as player" renders — and in the real app
    /// exactly the set that could be decrypted at all.
    pub fn visible_layers(
        &self,
        held: &std::collections::BTreeSet<crate::attribute::Attribute>,
    ) -> Vec<&Layer> {
        self.layers
            .iter()
            .filter(|l| l.policy.satisfied_by(held))
            .collect()
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.layers.is_empty() {
            return Err(ModelError::NoLayers);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute::Attribute;
    use std::collections::BTreeSet;

    fn held(list: &[&str]) -> BTreeSet<Attribute> {
        list.iter().map(|s| s.parse().unwrap()).collect()
    }

    fn citadel() -> Article {
        let mut art = Article::new(
            "place",
            "black-citadel",
            Layer::new("Description", Policy::just(Attribute::Players)).with_level(Level::Open),
        );
        art.push_layer(
            Layer::new(
                "Back door",
                "class:rogue".parse().unwrap(),
            )
            .with_level(Level::Sealed),
        );
        art.push_layer(
            Layer::new("Guards", Policy::just(Attribute::Gm)).with_level(Level::GmOnly),
        );
        art
    }

    #[test]
    fn a_player_sees_only_the_public_description() {
        let art = citadel();
        let view = art.visible_layers(&held(&["players", "class:mage"]));
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].title, "Description");
    }

    #[test]
    fn the_rogue_also_sees_the_back_door() {
        let art = citadel();
        let view = art.visible_layers(&held(&["players", "class:rogue"]));
        let title: Vec<&str> = view.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(title, vec!["Description", "Back door"]);
    }

    #[test]
    fn the_gm_sees_the_guards() {
        let art = citadel();
        let view = art.visible_layers(&held(&["gm"]));
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].title, "Guards");
    }

    #[test]
    fn gm_only_layers_are_not_meant_for_players() {
        let art = citadel();
        let delivered: Vec<&str> = art
            .layers
            .iter()
            .filter(|l| l.level.for_players())
            .map(|l| l.title.as_str())
            .collect();
        assert_eq!(delivered, vec!["Description", "Back door"]);
    }

    #[test]
    fn the_epoch_starts_at_one_and_rises() {
        let mut layer = Layer::new("x", Policy::just(Attribute::Players));
        assert_eq!(layer.epoch, 1);
        assert_eq!(layer.next_epoch(), 2);
    }

    #[test]
    fn ids_are_unique_and_readable() {
        let a = ArticleId::new();
        let b = ArticleId::new();
        assert_ne!(a, b);
        assert!(a.to_string().starts_with("art:"));
        assert_eq!(a.to_string().parse::<ArticleId>().unwrap(), a);
    }

    #[test]
    fn json_round_trip() {
        let art = citadel();
        let json = serde_json::to_string(&art).unwrap();
        assert_eq!(serde_json::from_str::<Article>(&json).unwrap(), art);
    }
}
