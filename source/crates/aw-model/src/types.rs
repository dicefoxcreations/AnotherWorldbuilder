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

use crate::template::{FieldKind, Template, TemplateBlock, TemplateError, TemplateField};

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
mod tests {
    use super::*;
    use crate::template::{just_input, Infobox, Shown, TemplateBlock};

    fn field(key: &str, kind: FieldKind) -> TemplateField {
        TemplateField::new(key, key, kind)
    }

    fn text(key: &str) -> TemplateField {
        field(key, FieldKind::Text { multiline: false })
    }

    fn parent(key: &str) -> TemplateField {
        field(
            key,
            FieldKind::Reference { article_type: String::new(), multiple: false, parent: true, adds: false, count: String::new(), every: false, from: String::new() },
        )
    }

    fn template(id: &str, supertype: &str, fields: Vec<TemplateField>) -> Template {
        Template {
            id: id.into(),
            name: id.into(),
            icon: String::new(),
            supertype: supertype.into(),
            group: false,
            navigation_hidden: false,
            rules: Vec::new(),
            description: String::new(),
            auth: None,
            name_pattern: just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            properties: fields,
            infobox: Infobox { align: "right".into(),  ..Infobox::default() },
            blocks: vec![TemplateBlock::fresh(format!("{id}-block"), id)],
        }
    }

    fn world() -> Vec<Template> {
        vec![
            template("place", "", vec![parent("lies-in"), text("climate")]),
            template("city", "place", vec![text("mayor")]),
            template("capital", "city", vec![]),
            template("character", "", vec![text("title")]),
        ]
    }

    #[test]
    fn a_subtype_is_its_supertype() {
        let all = world();
        assert!(is_a(&all, "capital", "place"));
        assert!(is_a(&all, "capital", "city"));
        assert!(is_a(&all, "city", "city"));
        assert!(!is_a(&all, "place", "city"), "not the other way round");
        assert!(!is_a(&all, "character", "place"));
    }

    #[test]
    fn fields_and_blocks_are_inherited_general_first() {
        let all = world();
        let capital = effective(&all, find(&all, "capital").unwrap());
        let keys: Vec<&str> = capital.properties.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["lies-in", "climate", "mayor"]);
        let blocks: Vec<&str> = capital.blocks.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(blocks, vec!["place-block", "city-block", "capital-block"]);
        assert_eq!(capital.name, "capital", "the name stays the type's own");
    }

    #[test]
    fn a_subtype_may_put_the_inherited_blocks_in_another_order() {
        let mut all = world();
        let city = all.iter_mut().find(|t| t.id == "city").unwrap();
        city.order = vec!["city-block".into(), "place-block".into()];
        let order = |id: &str| -> Vec<String> {
            effective(&all, find(&all, id).unwrap()).blocks.into_iter().map(|b| b.key).collect()
        };
        assert_eq!(order("city"), vec!["city-block", "place-block"]);
        assert_eq!(order("capital"), vec!["city-block", "place-block", "capital-block"], "one not named comes last");
        assert_eq!(order("place"), vec!["place-block"], "the supertype keeps its own");
    }

    #[test]
    fn a_bare_parent_points_at_the_root() {
        let all = world();
        let capital = effective(&all, find(&all, "capital").unwrap());
        let lies_in = parent_field(&capital).expect("inherited parent field");
        assert_eq!(target_of(&all, &capital, lies_in).as_deref(), Some("place"));
    }

    #[test]
    fn a_cycle_is_rejected_and_does_not_hang() {
        let mut all = world();
        let mut place = find(&all, "place").unwrap().clone();
        place.supertype = "capital".into();
        all.retain(|t| t.id != "place");
        assert_eq!(
            validate_hierarchy(&all, &place),
            Err(TemplateError::SupertypeCycle("place".into()))
        );

        // Even stored like that, walking up must end.
        all.push(place);
        let _ = effective(&all, find(&all, "capital").unwrap());
        assert!(ancestors(&all, find(&all, "city").unwrap()).len() <= all.len());
    }

    #[test]
    fn an_unknown_supertype_is_rejected() {
        let all = world();
        let orphan = template("x", "gone", vec![]);
        assert_eq!(
            validate_hierarchy(&all, &orphan),
            Err(TemplateError::UnknownSupertype("gone".into()))
        );
    }

    #[test]
    fn a_second_parent_below_an_inherited_one_is_rejected() {
        let all = world();
        let city = template("town", "place", vec![parent("part-of")]);
        assert_eq!(
            validate_hierarchy(&all, &city),
            Err(TemplateError::TwoParents("lies-in".into(), "part-of".into()))
        );
    }

    #[test]
    fn the_infobox_comes_from_the_nearest_type_that_shows_something() {
        let shown = |p: &str| Shown { element: "text".into(), property: p.into(), label: String::new() };
        let mut all = world();
        for t in all.iter_mut() {
            match t.id.as_str() {
                "place" => {
                    t.infobox.align = "left".into();
                    t.infobox.show = vec![shown("lies-in")];
                }
                "city" => t.infobox.show = vec![shown("mayor"), shown("climate")],
                _ => {}
            }
        }
        // capital shows nothing of its own — city does, and city says right.
        let capital = effective(&all, find(&all, "capital").unwrap());
        assert_eq!(capital.infobox.align, "right");
        assert_eq!(capital.infobox.show.len(), 2);
        let mut city = find(&all, "city").unwrap().clone();
        city.infobox.show.clear();
        let city = effective(&all, &city);
        assert_eq!(city.infobox.align, "left", "showing nothing itself, place decides");
    }

    #[test]
    fn a_binding_needs_a_property_of_its_kind() {
        let all = world();
        let capital = effective(&all, find(&all, "capital").unwrap());
        assert_eq!(check_binding(&capital, Some("text"), "mayor"), Ok(()), "an inherited one counts");
        assert!(check_binding(&capital, Some("text"), "nobody").unwrap_err().contains("no property"));
        assert!(check_binding(&capital, Some("number"), "mayor").unwrap_err().contains("<text>"));
        assert_eq!(check_binding(&capital, None, "Mayor"), Ok(()), "%%…%% reads any kind, any case");
        assert_eq!(check_binding(&capital, None, "Name"), Ok(()), "%%Name%% is the document's name");
        assert!(check_binding(&capital, Some("text"), "Name").is_err(), "and nothing to bind");
    }

    fn group(id: &str, fields: Vec<TemplateField>) -> Template {
        Template { group: true, blocks: Vec::new(), ..template(id, "", fields) }
    }

    fn uses(key: &str, group: &str) -> TemplateField {
        field(key, FieldKind::Group { group: group.into() })
    }

    fn number(key: &str) -> TemplateField {
        field(key, FieldKind::Number { unit: None })
    }

    #[test]
    fn a_group_stands_unfolded_where_it_is_used() {
        let abilities = group("abilities", vec![number("Strength"), number("Dexterity")]);
        let character = template("character", "", vec![text("Title"), uses("Stats", "abilities"), text("Notes")]);
        let all = vec![abilities.clone(), character.clone()];
        let whole = effective(&all, &character);
        let names: Vec<(&str, String)> = whole.properties.iter().map(|f| (f.label.as_str(), f.in_group.join(" › "))).collect();
        let stats = || "Stats".to_string();
        assert_eq!(names, [("Title", String::new()), ("Strength", stats()), ("Dexterity", stats()), ("Notes", String::new())]);
        assert_eq!(whole.properties[1].key, abilities.properties[0].key, "the group's keys — the same in every type");
        assert_eq!(validate_hierarchy(&[abilities.clone()], &character), Ok(()));

        // What the group gains, every type has.
        let mut more = abilities.clone();
        more.properties.push(number("Charisma"));
        let whole = effective(&[more, character.clone()], &character);
        assert!(whole.properties.iter().any(|f| f.label == "Charisma" && f.in_group == ["Stats"]));
    }

    #[test]
    fn rules_come_with_supertypes_and_groups() {
        let rule = |name: &str, formula: &str| crate::template::Rule { name: name.into(), params: vec!["x".into()], formula: formula.into() };
        let mut defence = group("defence", vec![number("Armor Class")]);
        defence.rules.push(rule("check", "1d20 + x"));
        let mut stats = group("stats", vec![number("Strength"), uses("Defence", "defence")]);
        stats.rules.push(rule("mod", "floor((x - 10) / 2)"));
        let mut creature = template("creature", "", vec![uses("Values", "stats")]);
        creature.rules.push(rule("half", "x / 2"));
        let monster = template("monster", "creature", vec![]);
        let all = vec![defence.clone(), stats.clone(), creature.clone(), monster.clone()];
        let names: Vec<String> = effective(&all, &monster).rules.into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["half", "mod", "check"], "the type's own, then its groups', nested ones too");
        assert_eq!(validate_hierarchy(&all, &monster), Ok(()));

        // A rule twice, once from a group: not saved.
        let mut again = monster.clone();
        again.rules.push(rule("Mod", "x"));
        let e = validate_hierarchy(&all, &again).unwrap_err().to_string();
        assert!(e.contains("rule mod twice"), "{e}");
        // Nor a group that would give a type a rule it has.
        let mut clash = defence;
        clash.rules.push(rule("half", "x"));
        let all = vec![clash.clone(), stats, creature, monster];
        assert!(validate_hierarchy(&all, &clash).is_err());
    }

    #[test]
    fn a_group_in_a_group_keeps_its_path() {
        let defence = group("defence", vec![number("Armor Class")]);
        let stats = group("stats", vec![number("Strength"), uses("Defence", "defence")]);
        let monster = template("monster", "", vec![uses("Values", "stats")]);
        let all = vec![defence, stats, monster.clone()];
        let whole = effective(&all, &monster);
        assert_eq!(whole.properties[0].in_group, ["Values"]);
        assert_eq!(whole.properties[1].in_group, ["Values", "Defence"]);
        assert_eq!(whole.properties.len(), 2);
    }

    #[test]
    fn groups_are_checked_among_the_types() {
        let abilities = group("abilities", vec![number("Strength")]);
        let place = template("place", "", vec![]);
        // Only a group can be taken in.
        let wrong = template("character", "", vec![uses("Stats", "place")]);
        assert!(matches!(validate_hierarchy(&[place.clone()], &wrong), Err(TemplateError::NoGroup(_))));
        // A name twice, once from the group.
        let twice = template("character", "", vec![number("Strength"), uses("Stats", "abilities")]);
        assert!(validate_hierarchy(&[abilities.clone()], &twice).is_err());
        // Nothing is a kind of a group, and nothing picks one.
        let below = template("hero", "abilities", vec![]);
        assert!(validate_hierarchy(&[abilities.clone()], &below).is_err());
        let picks = template("hero", "", vec![field("Gift", FieldKind::Reference { article_type: "abilities".into(), multiple: false, parent: false, adds: false, count: String::new(), every: false, from: String::new() })]);
        assert!(matches!(validate_hierarchy(&[abilities.clone()], &picks), Err(TemplateError::PicksGroup(_))));
        // A group that would contain itself.
        let outer = group("outer", vec![uses("Inner", "inner")]);
        let inner = group("inner", vec![uses("Outer", "outer")]);
        assert!(matches!(validate_hierarchy(&[outer.clone()], &inner), Err(TemplateError::GroupCycle(_))));
        assert_eq!(effective(&[outer.clone(), inner.clone()], &outer).properties.len(), 0, "and unfolding it ends");
        // A group that gains what a type using it already has.
        let user = template("character", "", vec![number("Charisma"), uses("Stats", "abilities")]);
        let mut grown = abilities.clone();
        grown.properties.push(number("Charisma"));
        assert!(validate_hierarchy(&[abilities, user], &grown).is_err());
    }
}
