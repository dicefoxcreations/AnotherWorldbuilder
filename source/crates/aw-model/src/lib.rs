//! What a world is made of, apart from how it is stored: types and their
//! text form, documents as an editor writes them, who may see what
//! (attributes and policies), links, names, navigation and the world's
//! calendar.

pub mod attribute;
pub mod calendar;
pub mod document;
pub mod editor;
pub mod ids;
pub mod link;
pub mod markdown;
pub mod names;
pub mod navigation;
pub mod relation;
pub mod template;
pub mod template_text;
pub mod types;

pub use ids::{ArticleId, LayerId, ModelError};
pub use attribute::{Attribute, Conjunction, Policy, PolicyError};
pub use calendar::{CalendarError, Era, LeapRule, Month, Moon, WorldCalendar, WorldDate};
pub use template::{
    BlockPart, FieldKind, Access, Infobox, Template, TemplateError, TemplateField,
};
