//! The world's types, sealed so that each member sees only what they may.
//!
//! Types go to everyone, like everything. But a type says more than its
//! name: a part only for the game master carries its starting text, a
//! property only for rogues its label — and every part and property names
//! who may see it. So a type is cut along its permissions:
//!
//! - the **frame** — names, fields and parts everyone may see — is sealed
//!   for every member (`all` or `players`);
//! - each **part** and each **property** with a view of its own is sealed
//!   for exactly that view, and so is a block whose parts all have one.
//!
//! Whoever opens the frame and some pieces puts back together what they may
//! see; the game master, holding every key, gets the whole type back.

use aw_crypto::{decrypt, encrypt, ContentKey};
use aw_model::template::{BlockPart, Shown, Template, TemplateBlock, TemplateField};
use aw_model::{Attribute, Policy};
use aw_policy::{open_dek, wrap_dek, HeldKeys, Keyring, LayerHeader};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::Result;

/// Something sealed for one view: the key under the header, the bytes under
/// the key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SealedFor {
    pub header: LayerHeader,
    pub data: String,
    /// Not sealed at all: the world is unencrypted, and anyone reads it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
}

impl SealedFor {
    pub fn new<T: Serialize>(view: &Policy, ring: &mut Keyring, value: &T) -> Result<Self> {
        for attribute in view.mentioned() {
            ring.ensure(attribute)?;
        }
        let key = ContentKey::generate()?;
        Ok(SealedFor {
            header: wrap_dek(view, 1, ring, &key)?,
            data: hex::encode(encrypt(&key, &serde_json::to_vec(value)?)?),
            plain: false,
        })
    }

    /// The same, for an unencrypted world: kept as it is.
    pub fn plain<T: Serialize>(value: &T) -> Result<Self> {
        let header = LayerHeader { version: aw_policy::HEADER_VERSION, epoch: 1, salt: String::new(), slots: Default::default() };
        Ok(SealedFor { header, data: hex::encode(serde_json::to_vec(value)?), plain: true })
    }

    pub fn open<T: for<'de> Deserialize<'de>>(&self, held: &HeldKeys) -> Option<T> {
        let bytes = hex::decode(&self.data).ok()?;
        if self.plain {
            return serde_json::from_slice(&bytes).ok();
        }
        let key = open_dek(&self.header, held).ok()??;
        serde_json::from_slice(&decrypt(&key, &bytes).ok()?).ok()
    }
}

/// A piece of a type with a view of its own, and where it belongs.
#[derive(Serialize, Deserialize)]
#[serde(tag = "piece", rename_all = "kebab-case")]
enum Piece {
    /// A block whose parts all have a view of their own — without them.
    Block { template: String, index: usize, block: TemplateBlock },
    Part { template: String, block: String, index: usize, part: BlockPart },
    /// A property, with where the infobox shows it.
    Field { template: String, index: usize, field: TemplateField, shown: Vec<(usize, Shown)> },
}

/// Whether every member may see what this view guards.
fn for_everyone(view: &Policy) -> bool {
    let everyone: BTreeSet<Attribute> = [Attribute::All, Attribute::Players].into_iter().collect();
    view.satisfied_by(&everyone)
}

/// A view as written; one that does not read is the game master's.
fn view_of(text: &str) -> Policy {
    text.parse().unwrap_or_else(|_| "gm".parse().expect("gm is a policy"))
}

/// The types of an unencrypted world: whole, for everyone.
pub fn plain(templates: &[Template]) -> Result<(SealedFor, Vec<SealedFor>)> {
    Ok((SealedFor::plain(&templates.to_vec())?, Vec::new()))
}

/// The types, cut along their permissions and sealed.
pub fn seal(templates: &[Template], ring: &mut Keyring) -> Result<(SealedFor, Vec<SealedFor>)> {
    let mut frame = Vec::new();
    let mut pieces = Vec::new();
    for template in templates {
        let mut open = template.clone();
        open.blocks.clear();
        open.properties.clear();
        open.infobox.show.clear();

        let mut hidden_labels = BTreeSet::new();
        for (index, field) in template.properties.iter().enumerate() {
            let view = field.access.as_ref().map(|a| view_of(&a.view));
            match view.filter(|v| !for_everyone(v)) {
                Some(view) => {
                    hidden_labels.insert(field.label.clone());
                    let shown: Vec<(usize, Shown)> = template
                        .infobox
                        .show
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.property == field.label)
                        .map(|(i, s)| (i, s.clone()))
                        .collect();
                    let piece = Piece::Field { template: template.id.clone(), index, field: field.clone(), shown };
                    pieces.push(SealedFor::new(&view, ring, &piece)?);
                }
                None => open.properties.push(field.clone()),
            }
        }
        open.infobox.show = template.infobox.show.iter().filter(|s| !hidden_labels.contains(&s.property)).cloned().collect();

        for (index, block) in template.blocks.iter().enumerate() {
            let views: Vec<Policy> = block.parts.iter().map(|p| view_of(&p.view)).collect();
            let mut shell = block.clone();
            shell.parts = block.parts.iter().zip(&views).filter(|(_, v)| for_everyone(v)).map(|(p, _)| p.clone()).collect();
            for (part_index, (part, view)) in block.parts.iter().zip(&views).enumerate() {
                if !for_everyone(view) {
                    let piece = Piece::Part { template: template.id.clone(), block: block.key.clone(), index: part_index, part: part.clone() };
                    pieces.push(SealedFor::new(view, ring, &piece)?);
                }
            }
            if shell.parts.is_empty() {
                // Nothing of it is for everyone: the block itself is for whoever
                // may see one of its parts.
                let any: Policy = views.iter().map(ToString::to_string).collect::<Vec<_>>().join(" OR ").parse()?;
                let piece = Piece::Block { template: template.id.clone(), index, block: shell };
                pieces.push(SealedFor::new(&any, ring, &piece)?);
            } else {
                open.blocks.push(shell);
            }
        }
        frame.push(open);
    }
    // Every member has `all` and `players` — the frame opens with either.
    let everyone: Policy = "all OR players".parse()?;
    Ok((SealedFor::new(&everyone, ring, &frame)?, pieces))
}

/// The types as far as `held` may see them.
pub fn open(frame: &SealedFor, pieces: &[SealedFor], held: &HeldKeys) -> Option<Vec<Template>> {
    let mut templates: Vec<Template> = frame.open(held)?;
    let mut opened: Vec<Piece> = pieces.iter().filter_map(|p| p.open(held)).collect();
    // Blocks first, so parts find them; each in its original place.
    opened.sort_by_key(|p| match p {
        Piece::Block { index, .. } => (0, *index),
        Piece::Part { index, .. } => (1, *index),
        Piece::Field { index, .. } => (2, *index),
    });
    let at = |all: &[Template], id: &str| all.iter().position(|t| t.id == id);
    let mut shown_all: Vec<(String, usize, Shown)> = Vec::new();
    for piece in opened {
        match piece {
            Piece::Block { template, index, block } => {
                if let Some(t) = at(&templates, &template) {
                    let i = index.min(templates[t].blocks.len());
                    templates[t].blocks.insert(i, block);
                }
            }
            Piece::Part { template, block, index, part } => {
                if let Some(t) = at(&templates, &template) {
                    if let Some(b) = templates[t].blocks.iter_mut().find(|b| b.key == block) {
                        let i = index.min(b.parts.len());
                        b.parts.insert(i, part);
                    }
                }
            }
            Piece::Field { template, index, field, shown } => {
                if let Some(t) = at(&templates, &template) {
                    let i = index.min(templates[t].properties.len());
                    templates[t].properties.insert(i, field);
                    shown_all.extend(shown.into_iter().map(|(i, s)| (template.clone(), i, s)));
                }
            }
        }
    }
    shown_all.sort_by_key(|(_, i, _)| *i);
    for (template, index, shown) in shown_all {
        if let Some(t) = at(&templates, &template) {
            let i = index.min(templates[t].infobox.show.len());
            templates[t].infobox.show.insert(i, shown);
        }
    }
    Some(templates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::template::{Access, FieldKind};

    fn part(key: &str, text: &str, view: &str) -> BlockPart {
        BlockPart { key: key.into(), text: text.into(), view: view.into(), edit: "gm".into(), level: "open".into() }
    }

    fn character() -> Template {
        let mut t: Template = serde_json::from_value(serde_json::json!({ "id": "character", "name": "Character" })).unwrap();
        t.properties = vec![
            TemplateField::new("title", "Title", FieldKind::Text { multiline: false }),
            TemplateField { access: Some(Access { view: "gm".into(), edit: "gm".into(), level: "gm-only".into() }), ..TemplateField::new("goal", "True goal", FieldKind::Text { multiline: false }) },
        ];
        t.infobox.show = vec![
            Shown { element: "text".into(), property: "Title".into(), label: String::new() },
            Shown { element: "text".into(), property: "True goal".into(), label: String::new() },
        ];
        t.blocks = vec![
            TemplateBlock { key: "story".into(), title: "What they say".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![
                part("story", "She came in autumn.", "all"),
                part("story-2", "In truth she seeks the passage.", "gm"),
                part("story-3", "Rogues know the back door.", "class:rogue"),
            ] },
            TemplateBlock { key: "secret".into(), title: "What she hides".into(), style: Default::default(), place: Default::default(), width: 0, fold: Default::default(), heading: 1, new: Vec::new(), parts: vec![part("secret", "The traitor.", "gm")] },
        ];
        t
    }

    fn held(ring: &Keyring, names: &[&str]) -> HeldKeys {
        ring.restrict_to(&names.iter().map(|n| n.parse().unwrap()).collect::<Vec<_>>())
    }

    #[test]
    fn each_sees_the_type_as_far_as_they_may() {
        let mut ring = Keyring::new();
        for a in ["gm", "players", "all", "class:rogue"] {
            ring.ensure(&a.parse().unwrap()).unwrap();
        }
        let (frame, pieces) = seal(&[character()], &mut ring).unwrap();
        let all = serde_json::to_string(&(&frame, &pieces)).unwrap();
        for secret in ["In truth", "traitor", "True goal", "What she hides", "rogue", "Character"] {
            assert!(!all.contains(secret), "{secret} in the clear");
        }

        let gm = open(&frame, &pieces, &ring.everything()).unwrap();
        assert_eq!(gm, vec![character()], "the game master gets it whole");

        let player = &open(&frame, &pieces, &held(&ring, &["players", "all"])).unwrap()[0];
        assert_eq!(player.blocks.len(), 1);
        assert_eq!(player.blocks[0].parts.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["story"]);
        assert_eq!(player.properties.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["Title"]);
        assert_eq!(player.infobox.show.len(), 1);

        let rogue = &open(&frame, &pieces, &held(&ring, &["players", "all", "class:rogue"])).unwrap()[0];
        assert_eq!(rogue.blocks[0].parts.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["story", "story-3"]);

        let stranger = Keyring::new();
        assert!(open(&frame, &pieces, &stranger.everything()).is_none(), "outside the world: nothing");
    }
}
