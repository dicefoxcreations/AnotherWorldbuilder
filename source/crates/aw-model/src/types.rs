//! Types among each other: supertypes, inheritance and the parent axis.
//!
//! A type may name a supertype. It then inherits its fields and blocks —
//! `City` under `Place` has everything a place has, plus its own. And it
//! counts as that type: a reference to `Place` offers cities too.
//!
//! This is the *what-is-it* axis. The *where-is-it* axis — Chicago lies in
//! Illinois — is a reference field marked `parent`, and lives in the
//! documents, not in the types. The two are kept apart on purpose: a city
//! state lies directly in a continent, an island belongs to no country. A
//! chain of types would break on that; a free parent does not.
//!
//! The frontend mirrors these rules in `apps/desktop/src/shared/types.ts` for display. What
//! is checked before saving is checked here.

use crate::attribute::Policy;
use crate::template::{Access, BlockPart, FieldKind, Template, TemplateBlock, TemplateError, TemplateField};

/// What `%%Name%%` reads where no property is called so: the document's name.
pub const NAME: &str = "Name";

/// Finds a type by id.
pub fn find<'a>(all: &'a [Template], id: &str) -> Option<&'a Template> {
    all.iter().find(|t| t.id == id)
}

/// The chain of supertypes above `id`, starting with its direct supertype.
///
/// Stops on a cycle or a missing type instead of looping — a damaged file
/// must not hang the application.
pub fn ancestors<'a>(all: &'a [Template], start: &Template) -> Vec<&'a Template> {
    let mut out: Vec<&Template> = Vec::new();
    let mut next = start.supertype.as_str();
    while !next.is_empty() {
        let Some(t) = find(all, next) else { break };
        if t.id == start.id || out.iter().any(|o| o.id == t.id) {
            break;
        }
        out.push(t);
        next = t.supertype.as_str();
    }
    out
}

/// Whether the type `id` is `ancestor` or lies below it.
pub fn is_a(all: &[Template], id: &str, ancestor: &str) -> bool {
    if id == ancestor {
        return true;
    }
    match find(all, id) {
        Some(t) => ancestors(all, t).iter().any(|a| a.id == ancestor),
        None => false,
    }
}

/// The topmost type above `start` — or `start` itself if it has none.
pub fn root_id(all: &[Template], start: &Template) -> String {
    ancestors(all, start)
        .last()
        .map(|t| t.id.clone())
        .unwrap_or_else(|| start.id.clone())
}

/// The family a type belongs to — what a document of it may change into.
///
/// The root of its tree, unless a type on the way up has its subtypes as
/// kinds apart: then the one below it. `Means` holds spells, items and
/// features together for what points at them, yet an item never becomes a
/// spell; under `Character` an NPC may turn out a player character.
pub fn family_id(all: &[Template], start: &Template) -> String {
    let chain: Vec<&Template> = std::iter::once(start).chain(ancestors(all, start)).collect();
    for pair in chain.windows(2) {
        if pair[1].kinds_apart {
            return pair[0].id.clone();
        }
    }
    chain.last().map(|t| t.id.clone()).unwrap_or_else(|| start.id.clone())
}

/// The type a reference field points at.
///
/// A bare `parent` names no type: it points at the root of the owning type's
/// tree — a city lies in a place, an order is part of an organisation.
pub fn target_of(all: &[Template], owner: &Template, field: &TemplateField) -> Option<String> {
    match &field.kind {
        FieldKind::Reference { article_type, .. } | FieldKind::Relation { article_type, .. }
            if !article_type.is_empty() =>
        {
            Some(article_type.clone())
        }
        FieldKind::Reference { parent: true, .. } => Some(root_id(all, owner)),
        _ => None,
    }
}

/// The type with everything it inherits.
///
/// Fields and blocks of the supertypes come first, the type's own after
/// them — the more general stands above the more specific. Alignment and
/// width of the infobox come from the nearest type that has fields of its
/// own; name, icon, access and overview stay the type's own.
pub fn effective(all: &[Template], template: &Template) -> Template {
    let mut chain: Vec<&Template> = ancestors(all, template);
    chain.reverse();
    chain.push(template);

    let mut out = template.clone();
    out.properties = unfold(all, chain.iter().flat_map(|t| t.properties.iter().cloned()));
    // The rules: the supertypes', the type's own, and those of every group
    // it takes in.
    let mut groups = Vec::new();
    for t in &chain {
        groups_in(all, &t.properties, &mut groups);
    }
    out.rules = chain
        .iter()
        .copied()
        .chain(groups.iter().filter_map(|g| find(all, g)))
        .flat_map(|t| t.rules.iter().cloned())
        .collect();
    out.blocks = chain.iter().flat_map(|t| t.blocks.iter().cloned()).collect();
    // The nearest type that orders its blocks orders them all.
    if let Some(order) = chain.iter().rev().map(|t| &t.order).find(|o| !o.is_empty()) {
        out.blocks = in_order(out.blocks, order);
    }
    // The nearest type that sorts its documents sorts those below it too.
    if let Some(sort) = chain.iter().rev().map(|t| &t.sort).find(|o| !o.is_empty()) {
        out.sort = sort.clone();
    }
    // The infobox is laid out as a whole: the nearest type that shows
    // anything decides what is shown, and how.
    if let Some(shaper) = chain.iter().rev().find(|t| !t.infobox.show.is_empty()) {
        out.infobox = shaper.infobox.clone();
    }
    out
}

/// Who sees and edits a part as its type sets it out, in a document of
/// `type_id` that has the access `document`.
///
/// A part with the same access as the type that sets it out follows the
/// document: an NPC opened to the players opens its sections to them too,
/// and one locked locks them. A part with access of its own — a secret —
/// keeps it.
pub fn set_out(all: &[Template], type_id: &str, block: &str, part: &BlockPart, document: &Access) -> Access {
    let own = Access { view: part.view.clone(), edit: part.edit.clone() };
    let Some(start) = find(all, type_id) else { return own };
    let setter = std::iter::once(start).chain(ancestors(all, start)).find(|t| t.blocks.iter().any(|b| b.key == block));
    match setter.and_then(|t| t.auth.as_ref()) {
        Some(auth) if same_policy(&auth.view, &own.view) && same_policy(&auth.edit, &own.edit) => document.clone(),
        _ => own,
    }
}

/// Whether two policies say the same, however they are written.
fn same_policy(a: &str, b: &str) -> bool {
    a == b || a.parse::<Policy>().ok().is_some_and(|a| b.parse::<Policy>().ok() == Some(a))
}

/// Blocks in an order: those it names as it names them, the rest after them
/// as they came.
pub fn in_order(blocks: Vec<TemplateBlock>, order: &[String]) -> Vec<TemplateBlock> {
    let rank = |b: &TemplateBlock| order.iter().position(|k| *k == b.key).unwrap_or(order.len());
    let mut blocks = blocks;
    blocks.sort_by_key(rank);
    blocks
}

/// Fields with the groups among them unfolded: in place of `Stats: group
/// Abilities` stand the abilities, each marked with the path of groups it
/// lies in — `Stats`, or `Values › Stats` for a group in a group. One that
/// contains itself, or does not exist, gives nothing.
pub fn unfold(all: &[Template], fields: impl IntoIterator<Item = TemplateField>) -> Vec<TemplateField> {
    fn push(all: &[Template], field: TemplateField, path: &[String], seen: &mut Vec<String>, out: &mut Vec<TemplateField>) {
        let FieldKind::Group { group } = &field.kind else {
            let mut field = field;
            field.in_group = path.to_vec();
            out.push(field);
            return;
        };
        let Some(g) = find(all, group).filter(|g| g.group) else { return };
        if seen.contains(&g.id) {
            return;
        }
        seen.push(g.id.clone());
        let inner: Vec<String> = path.iter().cloned().chain(std::iter::once(field.label.clone())).collect();
        for member in &g.properties {
            push(all, member.clone(), &inner, seen, out);
        }
        seen.pop();
    }
    let mut out = Vec::new();
    for field in fields {
        push(all, field, &[], &mut Vec::new(), &mut out);
    }
    out
}

/// The groups a field list takes in, directly or through other groups.
fn groups_in(all: &[Template], fields: &[TemplateField], seen: &mut Vec<String>) {
    for field in fields {
        if let FieldKind::Group { group } = &field.kind {
            if seen.contains(group) {
                continue;
            }
            seen.push(group.clone());
            if let Some(g) = find(all, group) {
                groups_in(all, &g.properties, seen);
            }
        }
    }
}

/// Whether a type takes in the group `id` — directly or through others.
pub fn uses_group(all: &[Template], template: &Template, id: &str) -> bool {
    let mut seen = Vec::new();
    for t in ancestors(all, template).into_iter().chain(std::iter::once(template)) {
        groups_in(all, &t.properties, &mut seen);
    }
    seen.iter().any(|g| g == id)
}

/// Two properties of the same name, once the groups are unfolded.
fn twice(whole: &Template) -> Result<(), TemplateError> {
    let mut names = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();
    for f in &whole.properties {
        if !names.insert(f.label.to_lowercase()) || !keys.insert(f.key.clone()) {
            return Err(TemplateError::Binding(format!(
                "{} has “{}” twice — once more from a group or a supertype.",
                whole.name, f.label
            )));
        }
    }
    let mut rules = std::collections::BTreeSet::new();
    for r in &whole.rules {
        if !rules.insert(r.name.to_lowercase()) {
            return Err(TemplateError::Rule(format!(
                "{} has the rule {} twice — once more from a group or a supertype.",
                whole.name, r.name
            )));
        }
    }
    Ok(())
}

/// The parent field of a type — its own or an inherited one.
pub fn parent_field(effective: &Template) -> Option<&TemplateField> {
    effective.properties.iter().find(|f| f.kind.is_parent())
}

/// Checks one element that binds a property: does it exist, and can this
/// element show it?
pub fn check_binding(effective: &Template, element: Option<&str>, property: &str) -> Result<(), String> {
    // `%%Name%%` reads the document's name — unless the type has a property
    // of that name.
    if element.is_none() && property.trim().eq_ignore_ascii_case(NAME) && effective.property(property).is_none() {
        return Ok(());
    }
    let Some(field) = effective.property(property) else {
        let known: Vec<&str> = effective.properties.iter().map(|p| p.label.as_str()).collect();
        return Err(if known.is_empty() {
            format!("There is no property “{property}” — this type has none yet. Declare it on top, between ---.")
        } else {
            format!("There is no property “{property}”. There is: {}.", known.join(", "))
        });
    };
    if let Some(element) = element {
        if !crate::template_text::shows(element, &field.kind) {
            return Err(format!(
                "<{element}> cannot show “{property}” — bind it with <{}>.",
                crate::template_text::element_for(&field.kind)
            ));
        }
    }
    Ok(())
}

/// Checks what a type can only be checked for among the others.
///
/// `all` is every type of the world *except* the one being saved — it may be
/// new, or its stored version may be about to be replaced.
pub fn validate_hierarchy(all: &[Template], template: &Template) -> Result<(), TemplateError> {
    // A group is no type: nothing is a kind of it.
    if !template.supertype.is_empty() && find(all, &template.supertype).is_none_or(|t| t.group) {
        return Err(TemplateError::UnknownSupertype(template.supertype.clone()));
    }
    if template.group && (!template.supertype.is_empty() || template.blocks.len() > 1 || !template.infobox.show.is_empty()) {
        return Err(TemplateError::GroupWithMore);
    }
    for field in &template.properties {
        match &field.kind {
            FieldKind::Group { group } => {
                let Some(g) = find(all, group).filter(|g| g.group) else {
                    let name = find(all, group).map(|t| t.name.clone()).unwrap_or_else(|| group.clone());
                    return Err(TemplateError::NoGroup(name));
                };
                let mut seen = vec![g.id.clone()];
                groups_in(all, &g.properties, &mut seen);
                if seen.contains(&template.id) {
                    return Err(TemplateError::GroupCycle(template.name.clone()));
                }
            }
            FieldKind::Reference { article_type, .. } | FieldKind::Relation { article_type, .. } => {
                if let Some(g) = find(all, article_type).filter(|g| g.group) {
                    return Err(TemplateError::PicksGroup(g.name.clone()));
                }
            }
            _ => {}
        }
        if let FieldKind::Relation { role_type, .. } = &field.kind {
            if let Some(g) = find(all, role_type).filter(|g| g.group) {
                return Err(TemplateError::PicksGroup(g.name.clone()));
            }
        }
    }
    // A group changed changes every type that takes it in — none of them
    // may end up with a name twice.
    let with_this: Vec<Template> =
        all.iter().filter(|t| t.id != template.id).cloned().chain(std::iter::once(template.clone())).collect();
    if template.group {
        twice(&effective(&with_this, template))?;
        for t in with_this.iter().filter(|t| !t.group && uses_group(&with_this, t, &template.id)) {
            twice(&effective(&with_this, t))?;
        }
        return Ok(());
    }
    // Walk up by id: the type being saved is not in `all`, so the chain has to
    // be recognised by the name it is referred to by, not by finding it.
    let mut seen: Vec<&str> = Vec::new();
    let mut next = template.supertype.as_str();
    while !next.is_empty() && !seen.contains(&next) {
        if next == template.id {
            return Err(TemplateError::SupertypeCycle(template.name.clone()));
        }
        seen.push(next);
        next = find(all, next).map(|t| t.supertype.as_str()).unwrap_or("");
    }

    // One parent field per type — counting the inherited ones. Two axes of
    // the tree would give two answers to “where does this lie?”.
    let whole = effective(all, template);
    twice(&whole)?;
    for shown in &whole.infobox.show {
        check_binding(&whole, Some(&shown.element), &shown.property).map_err(TemplateError::Binding)?;
    }
    let parents: Vec<&TemplateField> = whole
        .properties
        .iter()
        .filter(|f| f.kind.is_parent())
        .collect();
    if let [first, second, ..] = parents.as_slice() {
        return Err(TemplateError::TwoParents(first.label.clone(), second.label.clone()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
