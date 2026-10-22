use super::*;
use aw_model::Attribute;
use aw_policy::open_dek;

/// A template for the storage tests. Its content does not matter — it only
/// has to come through saving and passing on unchanged.
fn probe_template() -> Template {
    let mut t = Template {
        id: "character".into(),
        name: "Character".into(),
        icon: "🧑".into(),
        supertype: String::new(),
        group: false,
        rules: Vec::new(),
        description: String::new(),
        auth: None,
        name_pattern: aw_model::template::just_input(),
        order: Vec::new(),
        sort: Vec::new(),
        create: String::new(),
        page: String::new(),
        properties: Vec::new(),
        infobox: Default::default(),
        blocks: Vec::new(),
    };
    t.normalize();
    t
}

fn temp_store(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!("aw-store-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    (Store::new(dir.clone()), dir)
}

/// A world with its game master, keyring and the given members.
fn gm_world(store: &Store, gm: &Identity, members: Vec<Member>, attributes: &[&str]) -> String {
    let token = WorldToken::create("Eldermere").unwrap();
    let id = token.world_id().to_string();
    store.save_token(&token).unwrap();
    let mut ring = Keyring::new();
    for a in attributes {
        ring.ensure(&a.parse::<Attribute>().unwrap()).unwrap();
    }
    store.save_keyring(&id, &ring).unwrap();
    let mut all = vec![Member { identity: gm.public(), role: Role::Gm, attributes: vec!["gm".into()] }];
    all.extend(members);
    store
        .save_world_meta(&WorldMeta {
            public: token.public(),
            calendar: WorldCalendar::default(),
            icon: WORLD_ICON.to_string(),
            owner: gm.id().to_string(),
            home: String::new(),
            members: all,
            invited: Vec::new(),
        })
        .unwrap();
    id
}

fn part(view: &str, title: &str, level: Level, is_base: bool) -> NewPart {
    NewPart {
        is_base,
        level,
        view: view.parse().unwrap(),
        access: Access { section: title.into(), own: view.into(), edit: "gm".into() },
        plain: Plain { title: title.into(), text: format!("Content of {title}"), ..Plain::default() },
    }
}

fn citadel(store: &Store, id: &str) {
    store
        .create_article(
            id,
            "place".into(),
            "black-citadel".into(),
            vec![
                part("players", "Citadel", Level::Open, true),
                part("players", "Description", Level::Open, false),
                part("class:rogue", "Back door", Level::Sealed, false),
                part("gm", "Guards", Level::GmOnly, false),
            ],
            "",
        )
        .unwrap();
}

fn readable_titles(articles: &[StoredArticle], held: &HeldKeys) -> Vec<String> {
    let mut out: Vec<String> = articles
        .iter()
        .flat_map(|a| a.layers.iter())
        .filter(|l| !l.is_base)
        .filter_map(|l| l.content(held).map(|c| c.title))
        .collect();
    out.sort();
    out
}

#[test]
fn an_identity_survives_a_restart() {
    let (store, dir) = temp_store("ident");
    let bob = Identity::create("Bob").unwrap();
    store.save_identity(&bob).unwrap();
    assert!(store.has_identity());
    let again = store.load_identity().unwrap();
    assert_eq!(again.id(), bob.id());
    assert_eq!(again.name(), "Bob");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_world_with_articles_survives_a_restart() {
    let (store, dir) = temp_store("world");
    let gm = Identity::create("Marla").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players", "class:rogue"]);
    citadel(&store, &id);

    // A fresh store and no open journal: everything from disk.
    store.close(&id);
    let fresh = Store::new(dir.clone());
    assert_eq!(fresh.list_worlds().unwrap().len(), 1);
    let ring = fresh.load_keyring(&id).unwrap();
    let articles = fresh.load_articles(&id).unwrap();
    assert_eq!(readable_titles(&articles, &ring.everything()), vec!["Back door", "Description", "Guards"]);
    let player = ring.restrict_to(&["players".parse().unwrap()]);
    assert_eq!(readable_titles(&articles, &player), vec!["Description"]);
    let rogue = ring.restrict_to(&["players".parse().unwrap(), "class:rogue".parse().unwrap()]);
    assert_eq!(readable_titles(&articles, &rogue), vec!["Back door", "Description"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn nothing_sits_in_the_clear_on_disk() {
    let (store, dir) = temp_store("plaintext");
    let gm = Identity::create("Marla").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players", "class:rogue"]);
    citadel(&store, &id);
    let mut secret_type = probe_template();
    secret_type.blocks = vec![aw_model::template::TemplateBlock {
        key: "plan".into(),
        title: "The plan".into(),
        style: Default::default(),
        place: Default::default(), width: 0,
        fold: Default::default(),
        heading: 1,
        new: Vec::new(),
        parts: vec![aw_model::template::BlockPart {
            key: "plan".into(),
            text: "Poison the well.".into(),
            view: "knowledge:poison".into(),
            edit: "gm".into(),
            level: "sealed".into(),
        }],
    }];
    store.save_templates(&id, &[secret_type]).unwrap();
    // Every file of the world, wherever it lies below its folder.
    fn all_files(dir: &std::path::Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                all_files(&path, out);
            } else {
                out.push_str(&std::fs::read_to_string(path).unwrap());
            }
        }
    }
    let mut raw = String::new();
    all_files(&store.world_dir(&id), &mut raw);
    assert!(raw.contains("\"kind\":\"control\""), "the files were read");
    // Not even what a document is: it goes to everyone, so its slug is
    // sealed like its content.
    for secret in ["Back door", "Guards", "Content of", "black-citadel", "rogue", "knowledge", "Poison", "The plan"] {
        assert!(!raw.contains(secret), "{secret} must not stand in plaintext");
    }
    let articles = store.load_articles(&id).unwrap();
    assert!(articles.iter().any(|a| a.slug == "black-citadel"), "whoever opens it knows it");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gm_hands_over_the_world_player_sees_only_their_part() {
    let (gm_store, gm_dir) = temp_store("gm");
    let (player_store, player_dir) = temp_store("player");
    let gm = Identity::create("Marla").unwrap();
    let bob = Identity::create("Bob").unwrap();
    gm_store.save_identity(&gm).unwrap();
    player_store.save_identity(&bob).unwrap();
    let bob_member = Member {
        identity: bob.public(),
        role: Role::Player,
        attributes: vec!["players".into(), "class:rogue".into()],
    };
    let id = gm_world(&gm_store, &gm, vec![bob_member], &["gm", "players", "class:rogue"]);
    gm_store.save_templates(&id, &[probe_template()]).unwrap();
    let mut meta = gm_store.load_world_meta(&id).unwrap();
    meta.home = "art:home".into();
    gm_store.save_world_meta(&meta).unwrap();
    citadel(&gm_store, &id);

    let records = gm_store.all_records(&id).unwrap();
    assert!(records.iter().any(|r| r.contains("\"gm-only\"")), "everything goes to everyone — GM-only layers too, encrypted");
    assert!(player_store.receive(&id, &records).unwrap() > 0);

    assert!(!player_store.is_gm(&id), "Bob has no world token and is therefore a player");
    assert!(!player_store.private_world_dir(&id).join("owner.key").exists(), "the world token never travels");
    let meta = player_store.load_world_meta(&id).unwrap();
    assert_eq!(meta.home, "art:home", "the home page travels along");
    assert_eq!(meta.members.len(), 1, "Bob knows of himself only");
    assert!(player_store.load_templates(&id).unwrap().iter().any(|t| t.id == "character"));
    let keys = player_store.view_keys(&id, &bob).unwrap();
    assert_eq!(readable_titles(&player_store.load_articles(&id).unwrap(), &keys), vec!["Back door", "Description"]);
    assert_eq!(player_store.receive(&id, &records).unwrap(), 0, "taking the same twice changes nothing");

    std::fs::remove_dir_all(&gm_dir).ok();
    std::fs::remove_dir_all(&player_dir).ok();
}

#[test]
fn players_get_the_navigation_without_the_gms_entries() {
    use aw_model::navigation::{NavEntry, NavKind};
    let (gm_store, gm_dir) = temp_store("nav-gm");
    let (player_store, player_dir) = temp_store("nav-player");
    let gm = Identity::create("Marla").unwrap();
    let bob = Identity::create("Bob").unwrap();
    gm_store.save_identity(&gm).unwrap();
    player_store.save_identity(&bob).unwrap();
    let bob_member = Member { identity: bob.public(), role: Role::Player, attributes: vec!["players".into()] };
    let id = gm_world(&gm_store, &gm, vec![bob_member], &["gm", "players"]);
    let entry = |id: &str, gm: bool| NavEntry { id: id.into(), kind: NavKind::Type, target: "character".into(), label: id.into(), gm, children: Vec::new() };
    let nav = vec![entry("Characters", false), entry("Villains", true)];
    assert_eq!(gm_store.navigation(&id).unwrap(), None, "none until the GM sets one");
    gm_store.set_navigation(&id, &nav).unwrap();
    assert_eq!(gm_store.navigation(&id).unwrap().as_deref(), Some(&nav[..]));

    let records = gm_store.all_records(&id).unwrap();
    assert!(!records.iter().any(|r| r.contains("Villains")), "labels travel encrypted");
    player_store.receive(&id, &records).unwrap();
    assert_eq!(player_store.navigation(&id).unwrap().as_deref(), Some(&nav[..1]));
    assert!(player_store.set_navigation(&id, &nav).is_err(), "only the GM sets it");

    std::fs::remove_dir_all(&gm_dir).ok();
    std::fs::remove_dir_all(&player_dir).ok();
}

#[test]
fn a_forged_world_is_rejected() {
    let (gm_store, gm_dir) = temp_store("forge-gm");
    let (store, dir) = temp_store("forge");
    let gm = Identity::create("Mallory").unwrap();
    gm_store.save_identity(&gm).unwrap();
    let real = WorldToken::create("Eldermere").unwrap();
    let theirs = gm_world(&gm_store, &gm, vec![], &["gm"]);
    // Mallory's records, offered as the real world.
    let records = gm_store.all_records(&theirs).unwrap();
    assert!(store.receive(real.world_id().as_str(), &records).is_err());
    assert!(!store.world_dir(real.world_id().as_str()).exists(), "nothing is left behind");
    std::fs::remove_dir_all(&gm_dir).ok();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn base64_matches_the_known_vectors() {
    // Test vectors from RFC 4648.
    for (raw, expected) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64_encode(raw.as_bytes()), expected, "for {raw:?}");
    }
}

#[test]
fn base64_copes_with_binary_data_too() {
    let all: Vec<u8> = (0u8..=255).collect();
    let encoded = base64_encode(&all);
    assert_eq!(encoded.len(), 344);
    assert!(encoded.ends_with("=="));
    assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
}

#[test]
fn avatar_round_trip() {
    let (store, dir) = temp_store("avatar");
    assert_eq!(store.load_avatar().unwrap(), None);
    let png = b"\x89PNG\r\n\x1a\n-pretending-to-be-one";
    let url = store.save_avatar(png, "image/png").unwrap();
    assert!(url.starts_with("data:image/png;base64,"));
    assert_eq!(store.load_avatar().unwrap().as_deref(), Some(url.as_str()));
    store.remove_avatar().unwrap();
    assert_eq!(store.load_avatar().unwrap(), None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_new_world_has_no_templates_yet() {
    // The game master decides which article types there are.
    let (store, dir) = temp_store("empty");
    let gm = Identity::create("Marla").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm"]);
    assert!(store.load_templates(&id).unwrap().is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_base_layer_gates_the_whole_article() {
    // Whoever cannot open the base layer must not even see the article as a
    // title — not even if one of its sections would open.
    let (store, dir) = temp_store("base");
    let gm = Identity::create("Marla").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players", "class:rogue"]);
    store
        .create_article(
            &id,
            "place".into(),
            "secret-place".into(),
            vec![part("gm", "Secret place", Level::GmOnly, true), part("class:rogue", "Back door", Level::Sealed, false)],
            "",
        )
        .unwrap();
    let articles = store.load_articles(&id).unwrap();
    let ring = store.load_keyring(&id).unwrap();
    let rogue = ring.restrict_to(&["players".parse().unwrap(), "class:rogue".parse().unwrap()]);
    // The section opens …
    assert_eq!(readable_titles(&articles, &rogue), vec!["Back door"]);
    // … but the base layer does not, and that is what counts.
    let base_open = articles[0].layers.iter().find(|l| l.is_base).map(|l| open_dek(&l.header, &rogue).unwrap().is_some());
    assert_eq!(base_open, Some(false));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn deleting_a_world_clears_everything() {
    let (store, dir) = temp_store("delete");
    let gm = Identity::create("Marla").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm"]);
    assert_eq!(store.list_worlds().unwrap().len(), 1);
    store.delete_world(&id).unwrap();
    assert!(store.list_worlds().unwrap().is_empty());
    assert!(!store.world_dir(&id).exists());
    assert!(!store.private_world_dir(&id).exists());
    assert!(store.delete_world(&id).is_err(), "deleting twice reports itself");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_broken_world_does_not_break_the_list() {
    let (store, dir) = temp_store("broken");
    let gm = Identity::create("Marla").unwrap();
    store.save_identity(&gm).unwrap();
    gm_world(&store, &gm, vec![], &["gm"]);
    let broken = store.worlds_dir().join("world-broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("world.json"), b"{ this is not json").unwrap();
    let list = store.list_worlds().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].public.name, "Eldermere");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn profiles_open_the_same_world_without_sharing_keys_or_decrypted_cache() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf());
    let gm = Identity::create("GM").unwrap();
    let bob = Identity::create("Bob").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![Member {
        identity: bob.public(), role: Role::Player, attributes: vec!["players".into()]
    }], &["gm", "players", "class:rogue"]);
    citadel(&store, &id);
    let gm_open = store.journal(&id).unwrap();
    let gm_articles = store.load_articles(&id).unwrap();
    let bob_store = Store::at(dir.path().join("bob-profile"), store.worlds_dir());
    bob_store.save_identity(&bob).unwrap();
    assert!(!bob_store.is_gm(&id));
    assert!(!Arc::ptr_eq(&gm_open, &bob_store.journal(&id).unwrap()));
    let meta = bob_store.load_world_meta(&id).unwrap();
    assert_eq!(meta.members.len(), 1);
    assert_eq!(meta.members[0].identity.id, *bob.id());
    let keys = bob_store.view_keys(&id, &bob).unwrap();
    assert_eq!(readable_titles(&bob_store.load_articles(&id).unwrap(), &keys), vec!["Description"]);
    assert_eq!(readable_titles(&gm_articles, &store.load_keyring(&id).unwrap().everything()),
        vec!["Back door", "Description", "Guards"]);
    // Moving the public world directory leaves private keys usable by world ID.
    store.close(&id);
    let moved = dir.path().join("moved-worlds");
    std::fs::rename(store.worlds_dir(), &moved).unwrap();
    let moved_store = Store::at(store.profile_dir().to_path_buf(), moved);
    assert!(moved_store.is_gm(&id));
    assert_eq!(moved_store.load_articles(&id).unwrap().len(), gm_articles.len());
}

#[test]
fn multiple_roots_route_creation_and_keep_private_keys_in_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("local");
    let second = dir.path().join("mounted-drive");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let store = Store::with_roots(dir.path().join("profile"), vec![first.clone(), second.clone()]);
    let gm = Identity::create("GM").unwrap();
    store.save_identity(&gm).unwrap();
    let local_id = gm_world(&store, &gm, vec![], &["gm"]);
    let mounted_id = gm_world(&store.destination(&second).unwrap(), &gm, vec![], &["gm"]);
    assert_eq!(store.list_worlds().unwrap().len(), 2);
    for (id, root) in [(&local_id, &first), (&mounted_id, &second)] {
        assert_eq!(store.world_dir(id).parent(), Some(root.as_path()));
        assert!(store.private_world_dir(id).join("owner.key").exists());
        assert!(!store.world_dir(id).join("owner.key").exists());
        assert!(store.load_world_meta(id).is_ok());
    }
    assert!(store.destination(&dir.path().join("unconfigured")).is_err());
    // Removing a location leaves both its worlds and the profile keys intact.
    let reduced = Store::with_roots(store.profile_dir().to_path_buf(), vec![first.clone()]);
    assert_eq!(reduced.list_worlds().unwrap().len(), 1);
    assert!(second.join(dir_name(&mounted_id)).join("world.json").exists());
    assert!(reduced.private_world_dir(&mounted_id).join("owner.key").exists());
    std::fs::rename(&second, dir.path().join("disconnected")).unwrap();
    assert!(store.destination(&second).is_err());
    assert_eq!(store.list_worlds().unwrap().len(), 1);
    assert!(!second.exists(), "a missing drive is never recreated");
}

#[test]
fn old_world_formats_are_left_untouched_and_not_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf());
    store.save_identity(&Identity::create("GM").unwrap()).unwrap();
    let token = WorldToken::create("Old development world").unwrap();
    let id = token.world_id().as_str();
    let world = store.world_dir(id);
    token.save(&world.join("owner.key")).unwrap();
    let raw = serde_json::to_vec(&Local { version: 1, public: token.public(), owner: String::new(), members: vec![], invited: vec![] }).unwrap();
    std::fs::write(world.join("world.json"), &raw).unwrap();
    assert!(store.list_worlds().unwrap().is_empty());
    assert!(store.load_world_meta(id).is_err());
    assert_eq!(std::fs::read(world.join("world.json")).unwrap(), raw);
    assert!(world.join("owner.key").exists());
    assert!(!store.private_world_dir(id).exists());
}

/// Times what opening the gallery costs in a real profile:
/// `AW_PROFILE=… AW_WORLDS=… AW_WORLD=… cargo test -p aw-desktop gallery_timing -- --ignored --nocapture`
#[test]
#[ignore]
fn gallery_timing() {
    let var = |k: &str| std::env::var(k).expect(k);
    let store = Store::with_roots(PathBuf::from(var("AW_PROFILE")), vec![PathBuf::from(var("AW_WORLDS"))]);
    let world = var("AW_WORLD");
    let me = store.load_identity().unwrap();
    let t = std::time::Instant::now();
    store.with_journal(&world, |_| Ok(())).unwrap();
    println!("open journal {:?}", t.elapsed());
    let t = std::time::Instant::now();
    store.load_articles(&world).unwrap();
    println!("load_articles {:?}", t.elapsed());
    let t = std::time::Instant::now();
    store.load_articles(&world).unwrap();
    println!("load_articles again {:?}", t.elapsed());
    // The projection is written in the background: wait for it, so a next run starts from it.
    let kept = store.private_world_dir(&world).join(PROJECTION);
    let since = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
    for _ in 0..120 {
        if std::fs::metadata(&kept).and_then(|m| m.modified()).is_ok_and(|t| t > since) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    println!("projection kept: {:?} bytes", std::fs::metadata(&kept).map(|m| m.len()).ok());
    for _ in 0..3 {
        let t = std::time::Instant::now();
        store.with_journal(&world, |_| Ok(())).unwrap();
        println!("with_journal (refresh) {:?}", t.elapsed());
    }
    let t = std::time::Instant::now();
    store.with_journal(&world, |j| Ok(j.images_in_versions().len())).unwrap();
    println!("images_in_versions {:?}", t.elapsed());
    let t = std::time::Instant::now();
    store.with_journal(&world, |j| Ok(j.images_in_versions().len())).unwrap();
    println!("images_in_versions again {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let g = store.gallery(&world, &me).unwrap();
    println!("gallery {:?} ({} images)", t.elapsed(), g.len());
    let t = std::time::Instant::now();
    store.gallery(&world, &me).unwrap();
    println!("gallery again {:?}", t.elapsed());
    let t = std::time::Instant::now();
    for i in g.iter().take(20) {
        let _ = store.image_bytes(&world, &i.id, true);
    }
    println!("20 previews {:?}", t.elapsed());
}

#[test]
fn an_unencrypted_world_lies_in_the_clear_and_encrypts_again() {
    let (store, dir) = temp_store("plain");
    let gm = Identity::create("GM").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players", "class:rogue"]);
    citadel(&store, &id);
    // Someone with nothing but the world's folder.
    let stranger = Store::at(dir.join("stranger"), store.worlds_dir());
    stranger.save_identity(&Identity::create("Eve").unwrap()).unwrap();
    std::fs::create_dir_all(stranger.private_world_dir(&id)).unwrap();
    std::fs::copy(store.private_world_dir(&id).join("state.json"), stranger.private_world_dir(&id).join("state.json")).ok();
    let keys = |s: &Store| s.with_journal(&id, |j| s.device_keys(&id, j)).unwrap();
    let read = |s: &Store| readable_titles(&s.load_articles(&id).unwrap(), &keys(s));
    assert!(read(&stranger).is_empty(), "encrypted: nothing");

    store.set_encrypted(&id, false).unwrap();
    assert!(!store.is_encrypted(&id).unwrap());
    assert_eq!(read(&stranger), ["Back door", "Description", "Guards"], "unencrypted: everything, without a key");
    assert!(keys(&stranger).is_empty(), "and no key lies in the world");
    assert_eq!(store.load_templates(&id).unwrap(), stranger.load_templates(&id).unwrap(), "the types whole too");
    // What a new part holds lies in the folder as it is.
    let layer = store.load_articles(&id).unwrap()[0].layers[1].id;
    store.with_journal(&id, |j| {
        let head = j.head(layer).unwrap().clone();
        assert!(head.plain && head.seed.is_some() && head.header.slots.is_empty());
        let access: journal::Access = serde_json::from_slice(&hex::decode(&head.access).unwrap()).unwrap();
        assert_eq!(access.own, "players", "no rights count — but they stay, for encrypting again");
        Ok(())
    })
    .unwrap();

    // Anyone reads; only members write. The write key lies open, but what a
    // stranger signs with it does not count.
    let article = store.load_articles(&id).unwrap()[0].id.to_string();
    let edit = |s: &Store, who: &Identity, from: &str, to: &str| {
        let code = crate::document::code_of(s, who, &id, &article).unwrap();
        let session = format!("s-{}", who.name());
        let saved = crate::document::save(s, who, &id, &article, &code.code.replace(from, to), &code.base, &session);
        s.close_session(&id, &session).ok();
        saved
    };
    let eve = stranger.load_identity().unwrap();
    // Every journal checks it — the stranger's own too, so it is not even written.
    assert!(edit(&stranger, &eve, "Guards", "Nobody").is_err());
    store.close(&id);
    assert!(read(&store).contains(&"Guards".to_string()), "a stranger's writing does not count");
    edit(&store, &gm, "Back door", "Front door").unwrap();
    assert!(read(&stranger).contains(&"Front door".to_string()), "the game master's does");

    store.set_encrypted(&id, true).unwrap();
    assert!(store.is_encrypted(&id).unwrap());
    assert!(read(&stranger).is_empty(), "encrypted again: nothing");
    assert_eq!(read(&store).len(), 3, "the game master still reads all");
    assert!(!store.private_world_dir(&id).join(SWITCHING).exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn images_go_into_the_clear_and_back() {
    let (store, dir) = temp_store("plain-images");
    let gm = Identity::create("GM").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players"]);
    let mut raw = Vec::new();
    let img = image::RgbImage::from_pixel(40, 30, image::Rgb([200, 30, 30]));
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png).unwrap();
    let image = store.upload_image(&id, &raw, "Red", "", &gm).unwrap();
    let parts_dir = store.world_dir(&id).join("media").join("parts");
    let files = || -> Vec<Vec<u8>> { std::fs::read_dir(&parts_dir).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect() };
    let original = store.image_bytes(&id, &image, false).unwrap().0;
    assert!(!files().contains(&original), "encrypted: no part is the image");

    store.set_encrypted(&id, false).unwrap();
    store.close(&id);
    assert!(files().contains(&original), "unencrypted: the image lies there whole");
    assert_eq!(files().len(), 2, "the image and its preview — the encrypted parts went");
    assert_eq!(store.image_bytes(&id, &image, false).unwrap().0, original);

    store.set_encrypted(&id, true).unwrap();
    store.close(&id);
    assert!(!files().contains(&original), "encrypted again: the plain file went");
    assert_eq!(store.image_bytes(&id, &image, false).unwrap().0, original);

    // And once more: the plain file has the same name as the one deleted
    // before — it stays all the same.
    store.set_encrypted(&id, false).unwrap();
    store.close(&id);
    assert!(files().contains(&original), "unencrypted again: the image is there");
    assert_eq!(store.image_bytes(&id, &image, false).unwrap().0, original);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn switching_that_broke_off_is_finished_the_next_time() {
    let (store, dir) = temp_store("switching");
    let gm = Identity::create("GM").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players", "class:rogue"]);
    citadel(&store, &id);
    let nothing = HeldKeys::default();
    // Switched off — and then the app is gone before a part is in the clear.
    aw_store::durable::write(&store.private_world_dir(&id).join(SWITCHING), b"").unwrap();
    store.control(&id, Item::Encryption { on: false }, "").unwrap();
    assert!(readable_titles(&store.load_articles(&id).unwrap(), &nothing).is_empty(), "still encrypted");
    store.close(&id);
    assert_eq!(readable_titles(&store.load_articles(&id).unwrap(), &nothing).len(), 3, "opening the world finished it");
    assert!(!store.private_world_dir(&id).join(SWITCHING).exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn what_an_earlier_start_built_comes_back_only_where_nothing_changed() {
    let (store, dir) = temp_store("projection");
    let gm = Identity::create("GM").unwrap();
    store.save_identity(&gm).unwrap();
    let id = gm_world(&store, &gm, vec![], &["gm", "players", "class:rogue"]);
    citadel(&store, &id);
    let first = store.load_articles(&id).unwrap();
    assert_eq!(first[0].layers.len(), 4);
    let kept = store.private_world_dir(&id).join(PROJECTION);
    for _ in 0..100 {
        if kept.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(kept.exists(), "kept for the next start");
    // Meanwhile a part goes; then the app starts anew.
    let guards = first[0].layers[3].id;
    store.remove(&id, first[0].id, Some(guards), "").unwrap();
    store.close(&id);
    let again = store.load_articles(&id).unwrap();
    assert_eq!(again[0].layers.len(), 3, "the change counts, not what was kept");
    let held = store.with_journal(&id, |j| store.device_keys(&id, j)).unwrap();
    assert_eq!(readable_titles(&again, &held), ["Back door", "Description"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// What switching a large world's encryption costs, both ways — on a copy:
/// `AW_PROFILE=… AW_WORLDS=… AW_WORLD=… cargo test -p aw-desktop switch_timing -- --ignored --nocapture`
#[test]
#[ignore]
fn switch_timing() {
    let var = |k: &str| std::env::var(k).expect(k);
    let store = Store::with_roots(PathBuf::from(var("AW_PROFILE")), vec![PathBuf::from(var("AW_WORLDS"))]);
    let world = var("AW_WORLD");
    for on in [false, true] {
        let t = std::time::Instant::now();
        store.set_encrypted(&world, on).unwrap();
        println!("encrypted {on}: {:?}", t.elapsed());
    }

}

#[test]
fn a_folder_is_copied_file_by_file_with_how_far_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("world");
    std::fs::create_dir_all(from.join("media/ab")).unwrap();
    std::fs::write(from.join("world.json"), "{}").unwrap();
    std::fs::write(from.join("media/ab/part"), [7u8; 100]).unwrap();
    let told = std::cell::RefCell::new(Vec::new());
    copy_dir(&from, &dir.path().join("copy"), &|done, total| told.borrow_mut().push((done, total))).unwrap();
    assert_eq!(std::fs::read(dir.path().join("copy/media/ab/part")).unwrap(), [7u8; 100]);
    assert_eq!(std::fs::read_to_string(dir.path().join("copy/world.json")).unwrap(), "{}");
    assert_eq!(told.into_inner(), [(1, 2), (2, 2)]);
}
