//! The host: a world held here alone, worked on by its players online.
//!
//! A world without offline copies — every unencrypted one shared on the
//! network, and an encrypted one where the GM says so — stays on the GM's
//! device (or a server acting for them). Players hold nothing of it; their
//! app asks the host for each thing (`Ask::Call`), and the host does it in
//! their name: with exactly what they may see and change.
//!
//! What they may see is not rebuilt: the host takes the keys their grant
//! names out of its own keyring — the same decryption attempt as on their
//! device — and in an unencrypted world, where nothing is decrypted, checks
//! each part's view against their attributes. What they may change is
//! checked before anything is written (`may_write`), since the host itself
//! could write everything.
//!
//! The host signs what it writes; the player's id goes along in the session.

use super::*;
use aw_model::{Attribute, Policy};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// Someone the host acts for, during one call.
#[derive(Clone)]
pub(crate) struct Acting {
    pub member: IdentityPublic,
    /// Theirs, with `all` and `players`, which every player has.
    pub attributes: BTreeSet<Attribute>,
    /// The keys their grant names; for an attribute without one — an
    /// unencrypted world — a stand-in, so that what asks for attributes
    /// sees theirs.
    pub held: HeldKeys,
}

thread_local! {
    static ACTING: RefCell<Option<Acting>> = const { RefCell::new(None) };
}

/// Whom the host acts for right now, if anyone.
pub(crate) fn acting() -> Option<Acting> {
    ACTING.with(|a| a.borrow().clone())
}

/// Runs `f` in a member's name.
pub(crate) fn as_member<T>(acting: Acting, f: impl FnOnce() -> T) -> T {
    let before = ACTING.with(|a| a.replace(Some(acting)));
    let out = f();
    ACTING.with(|a| *a.borrow_mut() = before);
    out
}

/// Runs `f` in no one's name: the host as itself — the game master's device
/// writing its own word for a supporter it acts for.
pub(crate) fn as_nobody<T>(f: impl FnOnce() -> T) -> T {
    let before = ACTING.with(|a| a.take());
    let out = f();
    ACTING.with(|a| *a.borrow_mut() = before);
    out
}

/// What a member may see and change here — by the member list and, in an
/// encrypted world, the keys their grant names.
pub(crate) fn acting_for(store: &Store, world: &str, member: &IdentityPublic) -> Answer<Acting> {
    let meta = store.load_world_meta(world)?;
    let gm = store.load_identity()?;
    let grant = store.with_journal(world, |journal| Ok(journal.grant_content(member.id.as_str(), gm.seal_secret())))?;
    // By the list here, or — on a host that is not the GM's own device — by
    // what their grant says.
    let listed: Vec<String> = match (meta.member(member.id.as_str()), &grant) {
        (Some(m), _) => m.attributes.clone(),
        (None, Some(g)) => g.attributes.clone(),
        (None, None) => return Err(AppError("Not a member of this world.".into())),
    };
    let mut attributes: BTreeSet<Attribute> = [Attribute::All, Attribute::Players].into_iter().collect();
    attributes.extend(listed.iter().filter_map(|a| a.parse().ok()));
    // Their own name, as a view or an edit rule picks them.
    attributes.extend(Member::own_attribute(member).and_then(|a| a.parse().ok()));
    let granted: BTreeSet<String> = grant.map(|g| g.keys.into_iter().map(|k| k.at).collect()).unwrap_or_default();
    let mut held = HeldKeys::new();
    for (at, key) in store.load_keyring(world)?.everything().iter() {
        if granted.contains(&at.to_string()) {
            held.insert(at.clone(), *key);
        }
    }
    store::stand_ins(&mut held, attributes.iter().cloned());
    Ok(Acting { member: member.clone(), attributes, held })
}

/// Whether the one acted for may see what `view` guards — always, when the
/// host acts for no one.
pub(crate) fn may_see(view: &str) -> bool {
    match acting() {
        Some(a) => view.parse::<Policy>().is_ok_and(|p| p.satisfied_by(&a.attributes)),
        None => true,
    }
}

/// Before the host writes into a part in someone's name: whether they may.
/// Encrypted, their keys must open the part's write key; unencrypted, its
/// edit rule must name them.
pub(crate) fn may_write(store: &Store, world: &str, layer: aw_model::LayerId, generation: u32) -> Answer<()> {
    let Some(a) = acting() else { return Ok(()) };
    let head = store
        .with_journal(world, |journal| Ok(journal.head(layer).cloned()))?
        .ok_or_else(|| AppError("There is no such part.".into()))?;
    let allowed = if head.plain {
        let (access, _) = store.access_at(world, layer, generation)?;
        access.edit.parse::<Policy>().is_ok_and(|p| p.satisfied_by(&a.attributes))
    } else {
        head.write_seed(&a.held).ok().flatten().is_some()
    };
    if allowed {
        Ok(())
    } else {
        Err(AppError("You may not change this part.".into()))
    }
}

/// Whether a world stays here, its players working online: every
/// unencrypted one — it has no views of its own — and the encrypted ones the
/// GM keeps here.
pub(crate) fn online_only(store: &Store, world: &str) -> bool {
    !store.copies(world).unwrap_or(false)
}

/// One argument of a call, by the name the interface gives it.
fn arg<T: serde::de::DeserializeOwned>(args: &serde_json::Value, name: &str) -> Answer<T> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(serde_json::Value::Null)).map_err(|e| AppError(format!("{name}: {e}")))
}

fn out<T: Serialize>(answer: Answer<T>) -> Answer<serde_json::Value> {
    serde_json::to_value(answer?).map_err(AppError::from)
}

/// Does what a player's app asks, in their name — what a player does with a
/// world: read, write into what they may, look at versions and images, be
/// seen, roll; and, for a supporter, let players in, give them roles, take
/// them out. Nothing of the GM's.
pub(crate) fn call(store: &Store, world: &str, member: &IdentityPublic, command: &str, args: &serde_json::Value) -> Answer<serde_json::Value> {
    let acting = acting_for(store, world, member)?;
    let w = || world.to_string();
    as_member(acting, || match command {
        "world_info" => {
            let mut info = serde_json::to_value(crate::world_info(store, &store.load_world_meta(world)?, member.id.as_str())?)?;
            // Where it lies on the host is the host's.
            info["folder"] = "".into();
            info["token_file"] = "".into();
            info["readable"] = "".into();
            // The member as their grant names them — a server has no list of its own.
            let listed: Vec<String> =
                self::acting().map(|a| a.attributes.iter().filter(|x| !matches!(x, Attribute::All | Attribute::Players)).map(|x| x.to_string()).collect()).unwrap_or_default();
            info["attributes"] = serde_json::to_value(&listed)?;
            // A supporter sees the members as the GM does; a player, themselves.
            if !store.supports(world)? {
                info["members"] = serde_json::json!([{
                    "id": member.id.to_string(),
                    "name": member.name,
                    "fingerprint": member.fingerprint(),
                    "role": "player",
                    // Their roles — not their own name.
                    "attributes": listed.iter().filter(|a| Some(*a) != Member::own_attribute(member).as_ref()).collect::<Vec<_>>(),
                    "trusted": store.trusted(world, member.id.as_str())?,
                }]);
            }
            Ok(info)
        }
        // How far the world is — the player's app loads again when it moved on.
        "changes" => out(store.journal_version(world).map_err(AppError::from)),
        "view" => out(crate::view_in(store, w(), None)),
        "templates_load" => out(crate::templates_load_in(store, w())),
        "navigation_load" => out(crate::navigation_load_in(store, w())),
        "document_open" => out(document::document_open_in(store, w(), arg(args, "article")?)),
        "document_read" => out(document::document_read_in(store, w(), arg(args, "article")?, arg(args, "text")?, arg(args, "base")?)),
        "document_save" => out(document::document_save_in(store, w(), arg(args, "article")?, arg(args, "text")?, arg(args, "base")?, session(member, args)?)),
        "session_end" => out(document::session_end_in(store, w(), session(member, args)?)),
        "image_data" => out(crate::media::image_data_in(store, w(), arg(args, "image")?, arg(args, "preview")?)),
        "gallery" => out(crate::media::gallery_in(store, w())),
        "texts_written" => out(crate::texts_written_in(store, w(), arg(args, "articleType")?, arg(args, "texts")?)),
        "today_in_world" => out(crate::today_in_world_in(store, w(), arg(args, "day")?)),
        "article_versions" => out(document::article_versions_in(store, w(), arg(args, "article")?)),
        "version_document" => out(document::version_document_in(store, w(), arg(args, "article")?, arg(args, "session")?)),
        "version_compare" => out(document::version_compare_in(store, w(), arg(args, "article")?, arg(args, "session")?)),
        "version_marks" => out(document::version_marks_in(store, w(), arg(args, "article")?, arg(args, "session")?)),
        "image_field_set" => out(document::image_field_set_in(store, w(), arg(args, "article")?, arg(args, "field")?, arg(args, "image")?)),
        "references_set" => out(document::references_set_in(store, w(), arg(args, "article")?, arg(args, "field")?, arg(args, "ids")?)),
        "presence" => out(network::presence_in(store, w(), arg(args, "article")?, arg(args, "caret")?)),
        "world_presence" => out(network::world_presence_in(store, w(), arg(args, "article")?)),
        "dice_roll" => out(network::dice_roll_in(store, w(), arg(args, "roll")?)),
        "dice_rolls" => out(network::dice_rolls(w())),
        "join_requests" => out(network::join_requests_in(store, w())),
        "join_answer" => out(network::join_answer_in(store, w(), arg(args, "id")?, arg(args, "accept")?, member.id.as_str())),
        "member_attributes_set" => out(crate::member_attributes_set_in(store, w(), arg(args, "member")?, arg(args, "attributes")?, member.id.as_str())),
        "member_remove" => {
            store.remove_member(world, &arg::<String>(args, "id")?)?;
            out(crate::world_info(store, &store.load_world_meta(world)?, member.id.as_str()))
        }
        _ => Err(AppError("Not available online.".into())),
    })
}

/// A session of the player's — under their id, so what the host writes in
/// their name stays theirs to tell apart.
fn session(member: &IdentityPublic, args: &serde_json::Value) -> Answer<String> {
    let session: String = arg(args, "session")?;
    Ok(format!("{}:{session}", member.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::Level;
    use aw_store::journal::{Access, Plain};
    use store::NewPart;

    fn part(view: &str, edit: &str, title: &str, is_base: bool) -> NewPart {
        NewPart {
            is_base,
            level: Level::Open,
            view: view.parse().unwrap(),
            access: Access { section: title.into(), own: view.into(), edit: edit.into() },
            plain: Plain { title: title.into(), text: format!("Content of {title}"), ..Plain::default() },
        }
    }

    fn titles(store: &Store, world: &str) -> Vec<String> {
        let held = store.view_keys(world, &store.load_identity().unwrap()).unwrap();
        let mut out: Vec<String> = build_view(&store.load_articles(world).unwrap(), &held)
            .into_iter()
            .flat_map(|a| a.layers.into_iter().map(|l| l.title))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn the_host_shows_and_lets_change_only_what_the_player_may() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("gm"));
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let world = new_world(&store, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        let bob = Identity::create("Bob").unwrap();
        admit(&store, &world, bob.public(), vec!["class:rogue".into()]).unwrap();
        let article = store
            .create_article(
                &world,
                "place".into(),
                "citadel".into(),
                vec![
                    part("players", "gm", "Citadel", true),
                    part("players", "players", "Rumours", false),
                    part("class:rogue", "gm", "Back door", false),
                    part("gm", "gm", "Guards", false),
                ],
                "",
            )
            .unwrap()
            .to_string();

        for encrypted in [true, false] {
            store.set_encrypted(&world, encrypted).unwrap();
            let all = titles(&store, &world);
            assert!(all.contains(&"Guards".to_string()), "the GM sees everything");
            let seen = as_member(acting_for(&store, &world, &bob.public()).unwrap(), || titles(&store, &world));
            assert!(seen.contains(&"Rumours".to_string()) && seen.contains(&"Back door".to_string()), "{encrypted}: {seen:?}");
            assert!(!seen.contains(&"Guards".to_string()), "{encrypted}: the GM's part stays the GM's");
            assert!(!seen.contains(&"Eldermere".to_string()) || encrypted, "the world's own page as the player sees it");

            // In Bob's name: his part takes the change, the GM's does not.
            let wrote = |from: &str, to: &str| {
                as_member(acting_for(&store, &world, &bob.public()).unwrap(), || {
                    let code = document::code_of(&store, &gm, &world, &article)?;
                    document::save(&store, &gm, &world, &article, &code.code.replace(from, to), &code.base, "s")
                })
            };
            wrote("Content of Rumours", "Bob heard something.").unwrap();
            assert!(wrote("Content of Back door", "Bob changed it.").is_err(), "{encrypted}: seen, not his to change");
            let code = document::code_of(&store, &gm, &world, &article).unwrap().code;
            assert!(code.contains("Bob heard something.") && code.contains("Content of Back door"));
            assert!(as_member(acting_for(&store, &world, &bob.public()).unwrap(), || store.is_gm(&world)) == false);
            assert!(store.is_gm(&world), "and the host itself still is");
            // Back for the next round.
            document::save_now(&store, &gm, &world, &article, &code.replace("Bob heard something.", "Content of Rumours")).unwrap();
        }
    }
}
