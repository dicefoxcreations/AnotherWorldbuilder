//! The ids of documents and of their parts: a prefix and a ULID, made once
//! and never changed.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;


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

ulid_id!(ArticleId, "art", "Identifier of a document. Immutable — links point at it.");
ulid_id!(LayerId, "lay", "Identifier of a part of a document.");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("'{0}' is not a valid identifier")]
    BadId(String),
}
