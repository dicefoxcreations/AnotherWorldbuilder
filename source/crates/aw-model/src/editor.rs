//! What the template editor needs on every keystroke.
//!
//! Belongs to the model, not to the application: no interface here and no
//! Tauri. The command in the app calls this, the test bench calls the same —
//! otherwise there would be two versions of the same rules, and one of them
//! would be the one nobody checks against.

use crate::template::Template;
use crate::template_text::{self, Directory};
use serde::Serialize;

/// What comes out of reading a template description.
#[derive(Serialize)]
pub struct ReadResult {
    /// The template that was read — even when there were errors.
    pub template: Option<Template>,
    pub errors: Vec<LineError>,
    /// The text as it would have to stand — if the skeleton was missing.
    ///
    /// `<article>`, `<name>` and `<body>` belong to every article.
    /// They can be changed, not deleted: if one of them is gone, the text
    /// comes back from here with it put in again.
    pub repair: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct LineError {
    pub line: usize,
    pub text: String,
}

/// Reads a template out of its text form.
///
/// Called while typing, not only on save: the editor shows beside it what is
/// becoming of it. That is why errors come back as a list and not as an
/// aborted call — a half-finished line is the normal case while typing and
/// no reason to empty the preview.
///
/// `base` is the state before this keystroke. Whether a changed title is a
/// rename hangs on it — then the block keeps its id, and the texts of
/// existing articles stay attached to it.
///
/// `others` are the world's other types and groups: what the supertypes
/// bring along and the groups' properties may be bound as well as the
/// type's own.
pub fn read(text: &str, base: Template, directory: &Directory, others: &[Template]) -> ReadResult {
    if base.group {
        return read_group(text, base, directory);
    }
    let parsed = template_text::from_text(text, directory);

    let previous_blocks = base.blocks.clone();
    let previous_fields = base.properties.clone();

    // The template comes back even with errors: while typing, a half-written
    // tag is the normal case, and the preview should show what was already
    // understood instead of freezing on the last intact state.
    let mut template = base;
    template.properties = parsed.properties;
    template.rules = parsed.rules;
    template.infobox = parsed.infobox;
    template.blocks = parsed.blocks;
    template.auth = parsed.auth;
    template.create = parsed.create;
    template.name_pattern = parsed.name_pattern;
    // The ids do not stand in the text — they come from what was there
    // before. Only so does a block survive a rename: its title is a label,
    // its id is what the layers of the articles hang on.
    template_text::assign_block_ids(&mut template.blocks, &previous_blocks);
    template_text::assign_field_ids(&mut template.properties, &previous_fields);
    template.normalize();

    // Id and name stand in the editor's header line, not in the text. A
    // draft has neither yet — that is no error here, otherwise the editor
    // would report a line that does not exist.
    let mut candidate_template = template.clone();
    if candidate_template.id.is_empty() {
        candidate_template.id = "draft".into();
    }

    let clean = parsed.errors.is_empty();
    let mut errors: Vec<LineError> = parsed
        .errors
        .into_iter()
        .map(|e| LineError {
            line: e.line,
            text: e.text,
        })
        .collect();
    if let Err(e) = candidate_template.validate() {
        errors.push(LineError {
            line: 0,
            text: e.to_string(),
        });
    }
    // Every element binds a property that exists — its own or an inherited
    // one — and can show it.
    let with_this: Vec<Template> =
        others.iter().filter(|t| t.id != template.id).cloned().chain(std::iter::once(template.clone())).collect();
    let whole = crate::types::effective(&with_this, &template);
    // The infobox keeps what it shows by key: a property renamed later is
    // still the one it shows. The text says its name.
    for shown in &mut template.infobox.show {
        if let Some(p) = whole.property(&shown.property) {
            shown.property = p.key.clone();
        }
    }
    // The order names blocks by title — the inherited ones as well; kept by
    // key, so a block renamed later keeps its place.
    template.order = Vec::new();
    if let Some((line, names)) = &parsed.order {
        for name in names {
            match whole.blocks.iter().find(|b| b.title.eq_ignore_ascii_case(name) || b.key == *name) {
                Some(b) if template.order.contains(&b.key) => {
                    errors.push(LineError { line: *line, text: format!("<order> names “{name}” twice.") })
                }
                Some(b) => template.order.push(b.key.clone()),
                None => errors.push(LineError {
                    line: *line,
                    text: format!(
                        "<order> names “{name}”, a block this type does not have. There is: {}.",
                        whole.blocks.iter().map(|b| if b.title.is_empty() { b.key.as_str() } else { b.title.as_str() }).collect::<Vec<_>>().join(", ")
                    ),
                }),
            }
        }
    }
    // The sort names properties — inherited ones too — kept by key.
    template.sort = Vec::new();
    if let Some((line, names)) = &parsed.sort {
        for name in names {
            let key = if name.eq_ignore_ascii_case("name") { Some("name".to_string()) } else { whole.property(name).map(|p| p.key.clone()) };
            match key {
                Some(k) if template.sort.contains(&k) => errors.push(LineError { line: *line, text: format!("<sort> names “{name}” twice.") }),
                Some(k) => template.sort.push(k),
                None => errors.push(LineError {
                    line: *line,
                    text: format!("<sort> names “{name}”, a property this type does not have. There is: name, {}.", whole.properties.iter().map(|p| p.label.as_str()).collect::<Vec<_>>().join(", ")),
                }),
            }
        }
    }
    for used in &parsed.uses {
        if let Err(text) = crate::types::check_binding(&whole, used.element.as_deref(), &used.property) {
            errors.push(LineError { line: used.line, text });
        }
    }
    // A pin of its own access is set apart when a document saves — a type
    // would hand it on as text, to everyone who reads its documents.
    if template.blocks.iter().flat_map(|b| &b.parts).any(|p| crate::document::secret_pins_in(&p.text)) {
        errors.push(LineError { line: 0, text: "A pin or region in a type cannot have access of its own — give it that in the document.".into() });
    }
    errors.sort_by_key(|e| e.line);

    // Repairing happens only when nothing is lost. Everything between
    // <body> and </body> belongs to whoever is writing; if something stands
    // there that no tag has claimed yet, the repair waits. Otherwise putting
    // the skeleton back would delete the very sentence someone just began.
    let repair = if parsed.skeleton_intact || !clean {
        None
    } else {
        Some(template_text::to_text(&template, directory))
    };

    ReadResult {
        template: Some(template),
        errors,
        repair,
    }
}

/// A property group: its properties, one per line, and its layout — no
/// skeleton to keep.
fn read_group(text: &str, base: Template, directory: &Directory) -> ReadResult {
    let template_text::GroupText { mut properties, rules, layout, errors } = template_text::group_from_text(text, directory);
    template_text::assign_field_ids(&mut properties, &base.properties);
    let mut template = base;
    template.properties = properties;
    template.rules = rules;
    // The layout is the group's one block.
    template.blocks = if layout.is_empty() {
        Vec::new()
    } else {
        let mut block = crate::template::TemplateBlock::fresh(template_text::LAYOUT, "");
        block.parts[0].text = layout;
        vec![block]
    };
    let mut errors: Vec<LineError> = errors.into_iter().map(|e| LineError { line: e.line, text: e.text }).collect();
    let mut candidate = template.clone();
    if candidate.id.is_empty() {
        candidate.id = "draft".into();
    }
    if let Err(e) = candidate.validate() {
        errors.push(LineError { line: 0, text: e.to_string() });
    }
    ReadResult { template: Some(template), errors, repair: None }
}
