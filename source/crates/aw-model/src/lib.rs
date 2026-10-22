//! Data types for AnotherWorldbuilder: articles, layers, attributes,
//! policies and the world calendar.
//!
//! This crate knows no keys and no persistence. It only describes what a
//! world looks like — what of it can be decrypted is decided by `aw-policy`.

pub mod article;
pub mod attribute;
pub mod calendar;
pub mod document;
pub mod editor;
pub mod link;
pub mod markdown;
pub mod names;
pub mod navigation;
pub mod relation;
pub mod template;
pub mod template_text;
pub mod types;

pub use article::{
    Article, ArticleId, FieldValue, Layer, LayerId, ModelError, Level, WriteAccess,
};
pub use attribute::{Attribute, Conjunction, Policy, PolicyError};
pub use calendar::{CalendarError, Era, LeapRule, Month, Moon, WorldCalendar, WorldDate};
pub use template::{
    BlockPart, FieldKind, Access, Infobox, Template, TemplateError, TemplateField,
};
