use super::*;
use aw_crypto::WriteKey;
use aw_model::Attribute;

struct Table {
    _dir: tempfile::TempDir,
    token: WorldToken,
    ring: Keyring,
    gm: Identity,
    gm_journal: Journal,
    bob: Identity,
    bob_journal: Journal,
}

fn attr(a: &str) -> Attribute {
    a.parse().unwrap()
}

/// A world with a game master and Bob, a player with `players` and `all`.
fn table() -> Table {
    let dir = tempfile::tempdir().unwrap();
    let token = WorldToken::create("Eldermere").unwrap();
    let mut ring = Keyring::new();
    for a in ["gm", "players", "all"] {
        ring.ensure(&attr(a)).unwrap();
    }
    let gm = Identity::create("Marla").unwrap();
    let bob = Identity::create("Bob").unwrap();
    let mut gm_journal = open(dir.path(), "gm", &token);
    gm_journal.set_keys(ring.everything());
    gm_journal
        .control(
            &token,
            Item::World {
                public: token.public(),
                calendar: WorldCalendar::default(),
                icon: "🏰".into(),
                home: String::new(),
                gm: gm.public(),
            },
            "",
            1,
        )
        .unwrap();
    let keys = ring.grant(&bob.public(), &[attr("players"), attr("all")]).unwrap();
    gm_journal.control(&token, grant(&bob, &gm, &["players", "all"], &keys), "", 1).unwrap();
    gm_journal.set_me(&gm);
    let mut bob_journal = open(dir.path(), "bob", &token);
    bob_journal.set_me(&bob);
    bob_journal.set_keys(HeldKeys::unseal(bob.seal_secret(), &keys));
    Table { _dir: dir, token, ring, gm, gm_journal, bob, bob_journal }
}

/// A device's journal: its world folder and its drafts.
fn open(base: &Path, who: &str, token: &WorldToken) -> Journal {
    Journal::open(&base.join(who).join("world"), &base.join(who).join("private"), &token.public(), Mode::Shared).unwrap()
}

/// A grant, sealed for its member and the game master.
fn grant(member: &Identity, gm: &Identity, attributes: &[&str], keys: &[GrantedKey]) -> Item {
    let content = GrantContent { attributes: attributes.iter().map(|a| a.to_string()).collect(), keys: keys.to_vec() };
    let sealed = Sealed::new(&[&member.public(), &gm.public()], &serde_json::to_vec(&content).unwrap()).unwrap();
    Item::Grant { member: member.public(), sealed }
}

fn plain(title: &str, text: &str) -> Plain {
    Plain { title: title.into(), text: text.into(), ..Plain::default() }
}

impl Table {
    /// An article with a base layer and one part.
    fn article(&mut self, part: (&str, &str, &str, Level)) -> (ArticleId, LayerId, LayerId) {
        let article = ArticleId::new();
        let (view, edit, text, level) = part;
        self.gm_journal
            .control(&self.token, Item::Article { article }, "", 2)
            .unwrap();
        let base = LayerId::new();
        let head = new_head(
            &mut self.ring,
            article,
            base,
            true,
            Level::Open,
            1,
            &"all".parse().unwrap(),
            &Access { section: String::new(), own: "all".into(), edit: "gm".into() },
            &plain("Tavern", ""),
            Some(&About { article_type: "place".into(), slug: "tavern".into() }),
            true,
        )
        .unwrap();
        self.gm_journal.control(&self.token, Item::Layer(head), "", 2).unwrap();
        let layer = LayerId::new();
        let head = new_head(
            &mut self.ring,
            article,
            layer,
            false,
            level,
            1,
            &view.parse().unwrap(),
            &Access { section: "story".into(), own: view.into(), edit: edit.into() },
            &plain("", text),
            None,
            true,
        )
        .unwrap();
        self.gm_journal.control(&self.token, Item::Layer(head), "", 2).unwrap();
        self.gm_journal.set_keys(self.ring.everything());
        (article, base, layer)
    }

    /// Everything the game master may hand to Bob, handed.
    fn to_bob(&mut self) -> usize {
        let to = self.bob.id().to_string();
        let _ = to;
        let ids = self.gm_journal.in_order(&self.gm_journal.inventory());
        let mut new = 0;
        for id in ids {
            if let Added::New(_) = self.bob_journal.add(&self.gm_journal.raw(&id).unwrap()).unwrap() {
                new += 1;
            }
        }
        new
    }

    /// Everything Bob has, handed to the game master.
    fn to_gm(&mut self) {
        let ids = self.bob_journal.in_order(&self.bob_journal.inventory());
        for id in ids {
            self.gm_journal.add(&self.bob_journal.raw(&id).unwrap()).unwrap();
        }
    }
}

fn text_of(journal: &mut Journal, layer: LayerId) -> String {
    journal.plain(layer).unwrap().text
}

#[test]
fn what_the_game_master_writes_reaches_bob_and_his_part_comes_back() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    assert!(t.to_bob() > 0);
    assert_eq!(text_of(&mut t.bob_journal, story), "The inn is old.");

    // Bob may edit the story — his change reaches the game master.
    let (generation, heads) = t.bob_journal.state(story).unwrap();
    t.bob_journal
        .edit(story, generation, &heads, &plain("", "The inn is old. It rains."), &t.bob, "s-bob", 10)
        .unwrap()
        .expect("a change");
    t.to_gm();
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old. It rains.");
    assert_eq!(t.to_bob(), 0, "nothing new for Bob: he wrote it");
}

#[test]
fn bob_holds_what_only_the_game_master_reads_and_cannot_write_what_he_may_not() {
    let mut t = table();
    let (_, _, secret) = t.article(("gm", "gm", "The innkeeper is a spy.", Level::GmOnly));
    t.to_bob();
    assert!(t.bob_journal.head(secret).is_some(), "everything goes to everyone");
    assert!(t.bob_journal.plain(secret).is_err(), "but only the keys decide who reads");

    let (_, _, story) = t.article(("players", "gm", "The inn is old.", Level::Open));
    t.to_bob();
    let (generation, heads) = t.bob_journal.state(story).unwrap();
    let err = t.bob_journal.edit(story, generation, &heads, &plain("", "Mine."), &t.bob, "s", 1).unwrap_err();
    assert!(err.to_string().contains("may not"), "{err}");
}

#[test]
fn a_change_without_the_write_key_is_refused() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "gm", "The inn is old.", Level::Open));
    t.to_bob();
    // Bob forges: a change signed by himself and a write key of his own.
    let (generation, heads) = t.bob_journal.state(story).unwrap();
    let head = t.bob_journal.head(story).unwrap().clone();
    let opened = t.bob_journal.open_layer(story, generation).unwrap();
    let dek = opened.dek.clone();
    let changes = opened.doc.edit(&layer::hashes(&heads).unwrap(), &plain("", "Mine."), t.bob.id().as_str(), "s", 1).unwrap();
    let list: Vec<String> = changes.iter().map(|c| hex::encode(c.raw_bytes())).collect();
    let mut forged = Draft {
        world: t.token.world_id().to_string(),
        article: head.article,
        layer: story,
        generation,
        author: t.bob.public(),
        session: "s".into(),
        seq: 1,
        time: 1,
        changes: dek.seal(&serde_json::to_vec(&list).unwrap()).unwrap(),
        seal: None,
        write_signature: String::new(),
        signature: String::new(),
    };
    let bytes = forged.signed_bytes().unwrap();
    let (own, _) = WriteKey::generate().unwrap();
    forged.write_signature = hex::encode(own.sign(&bytes));
    forged.signature = hex::encode(t.bob.sign(&bytes));
    let raw = serde_json::to_vec(&Record::Draft(forged)).unwrap();
    assert!(t.gm_journal.add(&raw).is_err(), "the game master's device refuses it");
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old.");
}

#[test]
fn offline_edits_merge_and_overlaps_are_named() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "one\ntwo\nthree\nfour", Level::Open));
    t.to_bob();
    let (g, gm_heads) = t.gm_journal.state(story).unwrap();
    let (_, bob_heads) = t.bob_journal.state(story).unwrap();
    // Apart from each other: different lines …
    t.gm_journal.edit(story, g, &gm_heads, &plain("", "ONE\ntwo\nthree\nfour"), &t.gm, "s-gm", 5).unwrap();
    t.bob_journal.edit(story, g, &bob_heads, &plain("", "one\ntwo\nthree\nFOUR"), &t.bob, "s-bob", 5).unwrap();
    t.to_gm();
    t.to_bob();
    assert_eq!(text_of(&mut t.gm_journal, story), "ONE\ntwo\nthree\nFOUR");
    assert_eq!(text_of(&mut t.bob_journal, story), "ONE\ntwo\nthree\nFOUR");
    assert!(t.gm_journal.sessions(article).iter().all(|s| s.overlaps.is_empty()), "no overlap");

    // … then both on the same line.
    let (_, gm_heads) = t.gm_journal.state(story).unwrap();
    let (_, bob_heads) = t.bob_journal.state(story).unwrap();
    t.gm_journal.edit(story, g, &gm_heads, &plain("", "ONE\nzwei\nthree\nFOUR"), &t.gm, "s-gm-2", 6).unwrap();
    t.bob_journal.edit(story, g, &bob_heads, &plain("", "ONE\ndeux\nthree\nFOUR"), &t.bob, "s-bob-2", 6).unwrap();
    t.to_gm();
    let sessions = t.gm_journal.sessions(article);
    let gm2 = sessions.iter().find(|s| s.session == "s-gm-2").unwrap();
    assert_eq!(gm2.overlaps, vec!["s-bob-2".to_string()], "{sessions:?}");
    assert_eq!(sessions[0].author.name, "Bob", "newest first — both at 6, Bob's came in later: {sessions:?}");
}

#[test]
fn a_session_is_a_version_that_can_be_taken_back() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.to_bob();
    let (g, heads) = t.bob_journal.state(story).unwrap();
    t.bob_journal.edit(story, g, &heads, &plain("", "The inn is old and dark."), &t.bob, "s-bob", 3).unwrap();
    t.bob_journal.edit(story, g, &[], &plain("", "The inn is old and dark and loud."), &t.bob, "s-bob", 4).unwrap();
    t.to_gm();
    let (_, heads) = t.gm_journal.state(story).unwrap();
    t.gm_journal.edit(story, g, &heads, &plain("", "Rain. The inn is old and dark and loud."), &t.gm, "s-gm", 5).unwrap();

    let sessions = t.gm_journal.sessions(article);
    let bobs = sessions.iter().find(|s| s.session == "s-bob").unwrap();
    assert_eq!((bobs.from, bobs.to, bobs.layers.clone()), (3, 4, vec![story]));
    let (before, after) = t.gm_journal.compare(story, "s-bob").unwrap();
    assert_eq!((before.text.as_str(), after.text.as_str()), ("The inn is old.", "The inn is old and dark and loud."));

    t.gm_journal.revert(story, "s-bob", &t.gm, "s-gm-revert", 6).unwrap();
    assert_eq!(text_of(&mut t.gm_journal, story), "Rain. The inn is old.");
    t.to_bob();
    assert_eq!(text_of(&mut t.bob_journal, story), "Rain. The inn is old.");
}

#[test]
fn a_new_generation_locks_out_who_lost_the_right() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "players", "For all of you.", Level::Open));
    t.to_bob();
    // The game master narrows the view to rogues: a new generation.
    let head = t.gm_journal.head(story).unwrap().clone();
    let now = t.gm_journal.plain(story).unwrap();
    let next = new_head(
        &mut t.ring,
        head.article,
        story,
        false,
        Level::Sealed,
        head.generation + 1,
        &"class:rogue".parse().unwrap(),
        &Access { section: "story".into(), own: "class:rogue".into(), edit: "gm".into() },
        &now,
        None,
        true,
    )
    .unwrap();
    t.gm_journal.control(&t.token, Item::Layer(next), "s-gm", 7).unwrap();
    t.gm_journal.set_keys(t.ring.everything());
    t.gm_journal.edit(story, 2, &[], &plain("", "Only for rogues now."), &t.gm, "s-gm", 8).unwrap();
    t.to_bob();
    assert!(t.bob_journal.plain(story).is_err(), "Bob cannot open the new generation");
    assert_eq!(text_of(&mut t.gm_journal, story), "Only for rogues now.");
}

#[test]
fn the_journal_reads_itself_back_from_disk() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.gm_journal.edit(story, 1, &[], &plain("", "The inn is new."), &t.gm, "s", 1).unwrap();
    t.gm_journal.close("s", &t.gm).unwrap();
    t.gm_journal.edit(story, 1, &[], &plain("", "The inn is new. Still open."), &t.gm, "s2", 2).unwrap();
    let mut again = Journal::open(&t.gm_journal.dir.clone(), &t.gm_journal.private.clone(), &t.token.public(), Mode::Shared).unwrap();
    again.set_keys(t.ring.everything());
    assert_eq!(text_of(&mut again, story), "The inn is new. Still open.");
    assert_eq!(again.inventory(), t.gm_journal.inventory());
}

#[test]
fn what_bob_wrote_offline_reaches_the_text_even_when_access_changed_meanwhile() {
    let mut t = table();
    let day = 24 * 3600;
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.to_bob();

    // Day 0: Bob writes, offline.
    t.bob_journal.edit(story, 1, &[], &plain("", "The inn is old. It rains."), &t.bob, "s-bob", day).unwrap();

    // Day 2: the game master changes who may see the part — a new generation —
    // and writes too, not knowing of Bob's lines.
    let head = t.gm_journal.head(story).unwrap().clone();
    let now = t.gm_journal.plain(story).unwrap();
    let next = new_head(
        &mut t.ring,
        head.article,
        story,
        false,
        Level::Open,
        2,
        &"players".parse().unwrap(),
        &Access { section: "story".into(), own: "players".into(), edit: "players".into() },
        &now,
        None,
        true,
    )
    .unwrap();
    t.gm_journal.control(&t.token, Item::Layer(next), "s-gm", 3 * day).unwrap();
    t.gm_journal.set_keys(t.ring.everything());
    t.gm_journal.edit(story, 2, &[], &plain("", "Night. The inn is old."), &t.gm, "s-gm", 3 * day).unwrap();

    // Day 6: they meet. Bob's lines arrive at the game master — for the old
    // generation, so not in the text yet, and marked so.
    t.to_gm();
    let sessions = t.gm_journal.sessions(article);
    let bobs = sessions.iter().find(|s| s.session == "s-bob").unwrap();
    assert!(bobs.stale, "{sessions:?}");
    assert!(bobs.arrived > bobs.to + day, "written long before it arrived");
    assert!(!text_of(&mut t.gm_journal, story).contains("rains"));

    // Bob's device learns of the new generation and carries his lines over.
    t.to_bob();
    for (layer, session) in t.bob_journal.stale_of(t.bob.id().as_str()) {
        t.bob_journal.carry(layer, &session, &t.bob, 6 * day).unwrap();
    }
    t.to_gm();
    let text = text_of(&mut t.gm_journal, story);
    assert_eq!(text, "Night. The inn is old. It rains.");
    let bobs = t.gm_journal.sessions(article).into_iter().find(|s| s.session == "s-bob").unwrap();
    assert!(!bobs.stale, "carried over — in the text now");
    assert_eq!(bobs.author.name, "Bob", "still Bob's version");

    // And the document as it stood with each version.
    assert_eq!(t.gm_journal.state_at(story, "s-gm", 3 * day).unwrap().text, "Night. The inn is old.");
    // By the time it was written, Bob's line stood on day 2 already — even
    // though the game master only learned of it on day 6.
    assert_eq!(t.gm_journal.state_at(story, "none", 2 * day).unwrap().text, "The inn is old. It rains.");
    assert_eq!(t.gm_journal.state_at(story, "none", day / 2).unwrap().text, "The inn is old.");
}

impl Table {
    /// Bob writes the story anew, as one version.
    fn bob_writes(&mut self, story: LayerId, text: &str, session: &str) {
        let (generation, heads) = self.bob_journal.state(story).unwrap();
        self.bob_journal.edit(story, generation, &heads, &plain("", text), &self.bob, session, 10).unwrap().expect("a change");
        self.bob_journal.close(session, &self.bob).unwrap();
    }

    fn gm_says(&mut self, item: Item) {
        self.gm_journal.control(&self.token, item, "", 20).unwrap();
    }

    /// The game master decides about a version.
    fn verdict(&mut self, article: ArticleId, session: &str, accepted: bool) {
        let versions = self.gm_journal.waiting_versions(session);
        let keys = if accepted { self.gm_journal.reveal(&versions).unwrap() } else { BTreeMap::new() };
        self.gm_says(Item::Verdict { article, accepted, versions, keys });
    }

    /// Cara, a second player with `players` and `all`, and everything the
    /// game master has.
    fn cara(&mut self) -> (Identity, Journal) {
        let cara = Identity::create("Cara").unwrap();
        let keys = self.ring.grant(&cara.public(), &[attr("players"), attr("all")]).unwrap();
        self.gm_says(grant(&cara, &self.gm, &["players", "all"], &keys));
        let mut journal = open(self._dir.path(), "cara", &self.token);
        journal.set_me(&cara);
        journal.set_keys(HeldKeys::unseal(cara.seal_secret(), &keys));
        self.to(&mut journal);
        (cara, journal)
    }

    fn to(&self, journal: &mut Journal) {
        for id in self.gm_journal.in_order(&self.gm_journal.inventory()) {
            journal.add(&self.gm_journal.raw(&id).unwrap()).unwrap();
        }
    }
}

/// A world where the game master moderates, with a story Bob may edit.
fn moderated() -> (Table, ArticleId, LayerId) {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.gm_says(Item::Moderation { on: true, settled: Vec::new() });
    t.to_bob();
    (t, article, story)
}

#[test]
fn a_moderated_version_waits_for_the_game_master() {
    let (mut t, article, story) = moderated();
    t.bob_writes(story, "The inn is old. It rains.", "s-bob");
    assert_eq!(text_of(&mut t.bob_journal, story), "The inn is old. It rains.", "Bob sees what he wrote");
    t.to_gm();
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old.", "not in the text yet");
    let waiting = t.gm_journal.sessions(article).into_iter().find(|s| s.session == "s-bob").unwrap();
    assert_eq!(waiting.review, Review::Waiting);
    // As it would be taken in: the text with it.
    assert_eq!(t.gm_journal.state_at(story, "s-bob", 99).unwrap().text, "The inn is old. It rains.");
    // Cara gets it like everything — and cannot read it: it is sealed.
    let (_, mut cara) = t.cara();
    assert!(cara.versions.values().any(|v| v.session == "s-bob" && v.blocks.iter().any(|(.., sealed)| *sealed)));
    assert_eq!(text_of(&mut cara, story), "The inn is old.");

    // The game master writes meanwhile — on the text without it.
    let (g, heads) = t.gm_journal.state(story).unwrap();
    t.gm_journal.edit(story, g, &heads, &plain("", "Night. The inn is old."), &t.gm, "s-gm", 30).unwrap();
    assert_eq!(text_of(&mut t.gm_journal, story), "Night. The inn is old.");

    t.verdict(article, "s-bob", true);
    assert_eq!(text_of(&mut t.gm_journal, story), "Night. The inn is old. It rains.");
    t.to(&mut cara);
    assert_eq!(text_of(&mut cara, story), "Night. The inn is old. It rains.", "taken in, its key came along");
}

#[test]
fn a_rejected_version_leaves_the_text_also_for_its_author() {
    let (mut t, article, story) = moderated();
    t.bob_writes(story, "The inn is a ruin.", "s-bob");
    t.to_gm();
    t.verdict(article, "s-bob", false);
    t.to_bob();
    assert_eq!(text_of(&mut t.bob_journal, story), "The inn is old.");
    let mine = t.bob_journal.sessions(article).into_iter().find(|s| s.session == "s-bob").unwrap();
    assert_eq!(mine.review, Review::Rejected);
}

#[test]
fn trusted_and_settled_versions_count_at_once() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.to_bob();
    t.bob_writes(story, "The inn is old. Before.", "s-before");
    t.to_gm();
    // Switched on later: what stood then stays as it was.
    let settled = t.gm_journal.sessions_by(None);
    t.gm_says(Item::Moderation { on: true, settled });
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old. Before.");
    t.gm_says(Item::Trust { member: t.bob.id().to_string(), trusted: true, settled: Vec::new() });
    t.to_bob();
    t.bob_writes(story, "The inn is old. Before. Trusted.", "s-trusted");
    t.to_gm();
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old. Before. Trusted.");
    assert!(t.gm_journal.sessions(article).iter().all(|s| s.review == Review::Counted));
}

#[test]
fn a_version_built_on_another_goes_with_it() {
    let (mut t, _, story) = moderated();
    t.bob_writes(story, "The inn is old. One.", "s-1");
    t.bob_writes(story, "The inn is old. One. Two.", "s-2");
    t.to_gm();
    assert_eq!(t.gm_journal.bound("s-2", true), vec!["s-1".to_string()], "taking in the second brings the first");
    assert_eq!(t.gm_journal.bound("s-1", false), vec!["s-2".to_string()], "turning down the first takes the second");
}

#[test]
fn a_session_taken_in_does_not_take_in_what_comes_after() {
    let (mut t, article, story) = moderated();
    t.bob_writes(story, "The inn is old. One.", "s-bob");
    t.to_gm();
    t.verdict(article, "s-bob", true);
    t.to_bob();
    // Still in the same session, Bob writes on: that waits again.
    t.bob_writes(story, "The inn is old. One. Two.", "s-bob");
    t.to_gm();
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old. One.");
    assert_eq!(t.gm_journal.session_review("s-bob"), Review::Waiting);
}

#[test]
fn what_a_document_is_and_who_has_which_keys_is_sealed() {
    let mut t = table();
    let (article, _, _) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.to_bob();
    assert_eq!(t.bob_journal.about(article).unwrap().slug, "tavern", "Bob may open it");
    // Only for the game master: Bob holds it, but does not learn what it is.
    let secret = ArticleId::new();
    t.gm_says(Item::Article { article: secret });
    let head = new_head(
        &mut t.ring,
        secret,
        LayerId::new(),
        true,
        Level::GmOnly,
        1,
        &"gm".parse().unwrap(),
        &Access { section: String::new(), own: "gm".into(), edit: "gm".into() },
        &plain("The traitor", ""),
        Some(&About { article_type: "npc".into(), slug: "traitor-aldric".into() }),
        true,
    )
    .unwrap();
    t.gm_says(Item::Layer(head));
    t.gm_journal.set_keys(t.ring.everything());
    t.to_bob();
    assert!(t.bob_journal.head(t.bob_journal.layers_of(secret)[0].layer).is_some(), "it reached Bob");
    assert!(t.bob_journal.about(secret).is_none());
    assert_eq!(t.gm_journal.about(secret).unwrap().slug, "traitor-aldric");
    let raw: Vec<String> = t.bob_journal.inventory().iter().map(|id| String::from_utf8_lossy(&t.bob_journal.raw(id).unwrap()).to_string()).collect();
    assert!(raw.iter().all(|r| !r.contains("traitor")), "no slug in plain text");
    let grants: Vec<&String> = raw.iter().filter(|r| r.contains("\"item\":\"grant\"")).collect();
    assert!(!grants.is_empty() && grants.iter().all(|r| !r.contains("players")), "no attributes in a grant");
    // Bob's grant opens for Bob and the game master, not for Cara.
    let (cara, _) = t.cara();
    assert!(t.bob_journal.grant_content(t.bob.id().as_str(), t.bob.seal_secret()).is_some());
    assert!(t.gm_journal.grant_content(t.bob.id().as_str(), t.gm.seal_secret()).is_some());
    assert!(t.gm_journal.grant_content(t.bob.id().as_str(), cara.seal_secret()).is_none());
}

// --- Versions -----------------------------------------------------------------

impl Table {
    /// A part only the game master sees, in `article`.
    fn secret_part(&mut self, article: ArticleId, text: &str) -> LayerId {
        let layer = LayerId::new();
        let head = new_head(
            &mut self.ring,
            article,
            layer,
            false,
            Level::GmOnly,
            1,
            &"gm".parse().unwrap(),
            &Access { section: "secret".into(), own: "gm".into(), edit: "gm".into() },
            &plain("", text),
            None,
            true,
        )
        .unwrap();
        self.gm_says(Item::Layer(head));
        self.gm_journal.set_keys(self.ring.everything());
        layer
    }
}

/// What a shared folder carries from one journal to another: control and
/// versions, no drafts.
fn through_folder(from: &Journal, to: &mut Journal) {
    for id in from.in_order(&from.shared()) {
        to.add(&from.raw(&id).unwrap()).unwrap();
    }
}

fn drafts_of(journal: &Journal) -> usize {
    journal.drafts_shelf.all(|_| true).unwrap().len()
}

#[test]
fn a_session_ends_in_one_version_holding_the_whole_document() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    let secret = t.secret_part(article, "The innkeeper is a spy.");
    t.gm_journal.edit(secret, 1, &[], &plain("", "The innkeeper is a spy. And a cook."), &t.gm, "s-gm", 1).unwrap();
    assert_eq!(drafts_of(&t.gm_journal), 1, "a draft while the session lasts");
    let first = t.gm_journal.close("s-gm", &t.gm).unwrap().remove(0);
    assert_eq!(drafts_of(&t.gm_journal), 0, "the version holds it now");
    // Kept in the world's folder, for whoever shares it.
    assert!(t.gm_journal.shelf.get(&first).is_some());

    // Bob writes the story: his version takes the secret over as it is.
    t.to_bob();
    t.bob_writes(story, "The inn is old. It rains.", "s-bob");
    let id = t.bob_journal.versions.iter().find(|(_, v)| v.session == "s-bob").map(|(i, _)| i.clone()).unwrap();
    let mine = t.bob_journal.version_full(&id).unwrap();
    assert_eq!(mine.parents, vec![first.clone()]);
    let theirs = t.bob_journal.version_full(&first).unwrap();
    let kept = |v: &Version| v.blocks.iter().find(|b| b.layer == secret).cloned();
    assert_eq!(kept(&mine), kept(&theirs), "the secret, unread, the same bytes");
    assert_eq!(drafts_of(&t.bob_journal), 0);

    // The newest version alone shows the document — to someone with all keys.
    let mut dana = open(t._dir.path(), "dana", &t.token);
    dana.set_keys(t.ring.everything());
    through_folder(&t.bob_journal, &mut dana);
    assert_eq!(dana.tips(article), vec![id.clone()]);
    let opened = dana.open_layer(secret, 1).unwrap();
    let from_newest = opened.applied[&id].len();
    assert_eq!(from_newest, opened.doc.doc.get_changes(&[]).len(), "every change of the secret is in Bob's version");
    assert_eq!(text_of(&mut dana, secret), "The innkeeper is a spy. And a cook.");
    assert_eq!(text_of(&mut dana, story), "The inn is old. It rains.");
}

#[test]
fn writing_together_the_others_lines_come_along_in_the_version() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.to_bob();
    // Bob types; over the network his draft reaches the game master …
    t.bob_journal.edit(story, 1, &[], &plain("", "The inn is old. It rains."), &t.bob, "s-bob", 5).unwrap();
    t.to_gm();
    // … who writes on and leaves first.
    t.gm_journal.edit(story, 1, &[], &plain("", "Night. The inn is old. It rains."), &t.gm, "s-gm", 6).unwrap();
    let version = t.gm_journal.close("s-gm", &t.gm).unwrap().remove(0);
    assert_eq!(t.gm_journal.versions[&version].drafts.len(), 1, "Bob's draft comes along");

    // Cara has only the shared folder — no drafts — and reads both.
    let (_, mut cara) = t.cara();
    let mut fresh = open(t._dir.path(), "cara-folder", &t.token);
    fresh.set_keys(cara.keys.clone().unwrap());
    through_folder(&t.gm_journal, &mut fresh);
    assert_eq!(text_of(&mut fresh, story), "Night. The inn is old. It rains.");
    let sessions = fresh.sessions(article);
    assert!(sessions.iter().any(|s| s.session == "s-bob" && s.author.name == "Bob"), "{sessions:?}");
    assert_eq!(text_of(&mut cara, story), "Night. The inn is old. It rains.");
}

#[test]
fn a_version_cannot_put_words_in_someone_elses_mouth() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.to_bob();
    // Bob may write the story — but makes a change in the game master's name.
    let head = t.bob_journal.head(story).unwrap().clone();
    let seed = open_dek(&head.write, t.bob_journal.keys.as_ref().unwrap()).unwrap().unwrap();
    let heads = t.bob_journal.shown_heads(story, 1).unwrap();
    let opened = t.bob_journal.open_layer(story, 1).unwrap();
    opened.doc.edit(&heads, &plain("", "The inn is old. The game master says so."), t.gm.id().as_str(), "s-bob", 9).unwrap();
    let now = opened.doc.doc.get_heads();
    let state = opened.doc.save_at(&now).unwrap();
    let mut block = Block {
        layer: story,
        generation: 1,
        state: opened.dek.seal(&state).unwrap(),
        seal: None,
        write_signature: String::new(),
    };
    block.write_signature =
        hex::encode(aw_crypto::WriteKey::from_seed(&seed).sign(&block.signed_bytes(t.token.world_id().as_str(), &article).unwrap()));
    let mut forged = Version {
        world: t.token.world_id().to_string(),
        article,
        author: t.bob.public(),
        session: "s-bob".into(),
        from: 9,
        until: 9,
        parents: Vec::new(),
        wrote: vec![(story, 1)],
        blocks: vec![block],
        drafts: Vec::new(),
        signature: String::new(),
    };
    forged.signature = hex::encode(t.bob.sign(&forged.signed_bytes().unwrap()));
    t.gm_journal.add(&serde_json::to_vec(&Record::Version(forged)).unwrap()).unwrap();
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old.", "the block stays out");
}

#[test]
fn a_version_waits_for_what_it_builds_on() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    t.gm_journal.edit(story, 1, &[], &plain("", "One."), &t.gm, "s-1", 1).unwrap();
    let first = t.gm_journal.close("s-1", &t.gm).unwrap().remove(0);
    t.gm_journal.edit(story, 1, &[], &plain("", "One. Two."), &t.gm, "s-2", 2).unwrap();
    let second = t.gm_journal.close("s-2", &t.gm).unwrap().remove(0);
    // The second comes first: it waits, then the first brings it in.
    let controls: Vec<String> = t.gm_journal.shared().into_iter().filter(|id| t.gm_journal.controls.contains_key(id)).collect();
    for id in t.gm_journal.in_order(&controls) {
        t.bob_journal.add(&t.gm_journal.raw(&id).unwrap()).unwrap();
    }
    assert_eq!(t.bob_journal.add(&t.gm_journal.raw(&second).unwrap()).unwrap(), Added::Waiting);
    assert!(matches!(t.bob_journal.add(&t.gm_journal.raw(&first).unwrap()).unwrap(), Added::New(_)));
    assert_eq!(text_of(&mut t.bob_journal, story), "One. Two.");
}

#[test]
fn opened_again_only_the_newest_versions_are_read() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "Zero.", Level::Open));
    let mut text = String::from("Zero.");
    for i in 1..=5 {
        text.push_str(&format!(" {i}."));
        t.gm_journal.edit(story, 1, &[], &plain("", &text), &t.gm, &format!("s-{i}"), i).unwrap();
        t.gm_journal.close(&format!("s-{i}"), &t.gm).unwrap();
    }
    assert_eq!(text_of(&mut t.gm_journal, story), text, "checked once, every version in order");
    // The app looks in at every step — and writes down what it checked.
    t.gm_journal.refresh().unwrap();
    // Opened again: the index knows the versions, the newest shows the text.
    let mut again = Journal::open(&t.gm_journal.dir.clone(), &t.gm_journal.private.clone(), &t.token.public(), Mode::Shared).unwrap();
    again.set_keys(t.ring.everything());
    assert_eq!(again.versions.len(), 5);
    assert_eq!(text_of(&mut again, story), text);
    assert_eq!(again.full.borrow().len(), 1, "only the newest was read: {:?}", again.full.borrow().keys());
    // The history reads the rest when asked.
    let sessions = again.sessions(article);
    assert_eq!(sessions.len(), 5);
    let (before, after) = again.compare(story, "s-3").unwrap();
    assert_eq!((before.text.as_str(), after.text.as_str()), ("Zero. 1. 2.", "Zero. 1. 2. 3."));
    // Without the index everything is read and checked anew.
    std::fs::remove_file(t.gm_journal.private.join("index.json")).unwrap();
    let mut fresh = Journal::open(&t.gm_journal.dir.clone(), &t.gm_journal.private.clone(), &t.token.public(), Mode::Shared).unwrap();
    fresh.set_keys(t.ring.everything());
    assert_eq!(text_of(&mut fresh, story), text);
    assert_eq!(fresh.full.borrow().len(), 5);
}

#[test]
fn what_the_game_master_deletes_is_gone_everywhere_and_stays_gone() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "Zero.", Level::Open));
    let mut ids = Vec::new();
    for i in 1..=3 {
        t.gm_journal.edit(story, 1, &[], &plain("", &format!("Zero. {i}.")), &t.gm, &format!("s-{i}"), i).unwrap();
        ids.extend(t.gm_journal.close(&format!("s-{i}"), &t.gm).unwrap());
    }
    t.to_bob();
    assert_eq!(t.gm_journal.removable(article), ids[..2].to_vec(), "the newest stays");
    let old = t.bob_journal.raw(&ids[0]).unwrap();
    t.gm_says(Item::Deleted { records: ids[..2].to_vec() });
    assert_eq!(text_of(&mut t.gm_journal, story), "Zero. 3.");
    // The next state leaves it out.
    assert!(t.gm_journal.consolidate().unwrap());
    assert!(t.gm_journal.shelf.get(&ids[0]).is_none() && t.gm_journal.shelf.get(&ids[2]).is_some());

    t.to_bob();
    t.bob_journal.consolidate().unwrap();
    assert!(t.bob_journal.shelf.get(&ids[0]).is_none());
    assert_eq!(text_of(&mut t.bob_journal, story), "Zero. 3.");
    // Passed on again by someone who still has it: not taken.
    assert_eq!(t.bob_journal.add(&old).unwrap(), Added::Known);
    assert!(!t.bob_journal.inventory().contains(&ids[0]));
    // And after a restart, too.
    let mut again = Journal::open(&t.bob_journal.dir.clone(), &t.bob_journal.private.clone(), &t.token.public(), Mode::Shared).unwrap();
    again.set_keys(t.bob_journal.keys.clone().unwrap());
    assert_eq!(again.versions.len(), 1);
    assert_eq!(text_of(&mut again, story), "Zero. 3.");
    assert_eq!(t.gm_journal.sessions(article).len(), 1);
}

#[test]
fn superseded_control_records_can_go() {
    let mut t = table();
    let first = t.gm_journal.shared();
    t.gm_says(Item::Versions { max: 10 });
    t.gm_says(Item::Versions { max: 20 });
    let gone = t.gm_journal.superseded();
    assert_eq!(gone.len(), 1, "the earlier limit");
    assert!(!first.contains(&gone[0]));
    t.gm_says(Item::Deleted { records: gone });
    assert_eq!(t.gm_journal.version_limit(), 20);
    assert!(t.gm_journal.superseded().is_empty());
}

#[test]
fn what_a_change_of_access_took_along_is_not_carried_again() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "", Level::Open));
    // The game master writes and, in the same session, changes who may edit.
    t.gm_journal.edit(story, 1, &[], &plain("", "The inn is old."), &t.gm, "s-gm", 1).unwrap();
    let head = t.gm_journal.head(story).unwrap().clone();
    let now = t.gm_journal.plain(story).unwrap();
    let mut next = new_head(
        &mut t.ring,
        head.article,
        story,
        false,
        Level::Open,
        2,
        &"players".parse().unwrap(),
        &Access { section: "story".into(), own: "players".into(), edit: "gm".into() },
        &now,
        None,
        true,
    )
    .unwrap();
    next.from = t.gm_journal.state(story).unwrap().1;
    t.gm_says(Item::Layer(next));
    t.gm_journal.set_keys(t.ring.everything());
    t.gm_journal.close("s-gm", &t.gm).unwrap();
    assert!(t.gm_journal.stale_of(t.gm.id().as_str()).is_empty(), "the text went along");
    assert!(!t.gm_journal.sessions(article).iter().any(|s| s.stale));
    assert_eq!(text_of(&mut t.gm_journal, story), "The inn is old.");
}

#[test]
fn a_members_proposal_reaches_the_gm_sealed_for_the_two_of_them() {
    let mut t = table();
    t.to_bob();
    let id = t.bob_journal.propose(&t.bob, b"{\"name\":\"Goblin\"}", 7).unwrap();
    // It travels like everything else.
    assert!(t.bob_journal.shelf.get(&id).is_some());
    t.to_gm();
    let (got, proposal) = t.gm_journal.proposals().next().expect("the GM has it");
    assert_eq!(got, &id);
    assert_eq!(proposal.open(t.gm.seal_secret()).as_deref(), Some(&b"{\"name\":\"Goblin\"}"[..]));
    assert_eq!(proposal.open(t.bob.seal_secret()).as_deref(), Some(&b"{\"name\":\"Goblin\"}"[..]));
    let stranger = Identity::create("Eve").unwrap();
    assert!(proposal.open(stranger.seal_secret()).is_none(), "nobody else reads it");

    // Someone who is no member: it waits, and is never taken in.
    let gm = t.gm.public();
    let theirs = Proposal::new(t.token.world_id().as_str(), &stranger, &gm, b"{}", 8, true).unwrap();
    let raw = serde_json::to_vec(&Record::Proposal(theirs.clone())).unwrap();
    assert!(matches!(t.gm_journal.add(&raw).unwrap(), Added::Waiting));
    assert_eq!(t.gm_journal.proposals().count(), 1);
    // Signed by someone else than it says: refused.
    let mut forged = theirs;
    forged.author = t.bob.public();
    assert!(t.gm_journal.add(&serde_json::to_vec(&Record::Proposal(forged)).unwrap()).is_err());

    // Done with, the GM deletes it: gone everywhere.
    t.gm_says(Item::Deleted { records: vec![id.clone()] });
    assert_eq!(t.gm_journal.proposals().count(), 0);
    t.to_bob();
    assert_eq!(t.bob_journal.proposals().count(), 0);
    t.bob_journal.consolidate().unwrap();
    assert!(t.bob_journal.shelf.get(&id).is_none());
}

#[test]
fn what_another_device_writes_into_a_known_folder_is_picked_up() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "players", "Zero.", Level::Open));
    let mut other = Journal::open(&t.gm_journal.dir.clone(), &t.gm_journal.private.join("other"), &t.token.public(), Mode::Shared).unwrap();
    other.set_keys(t.ring.everything());
    assert_eq!(text_of(&mut other, story), "Zero.");
    assert!(!other.refresh().unwrap(), "nothing changed, nothing to pick up");
    // Written beside it, into the folder it already listed.
    t.gm_journal.edit(story, 1, &[], &plain("", "Zero. One."), &t.gm, "s-1", 1).unwrap();
    let version = t.gm_journal.close("s-1", &t.gm).unwrap().remove(0);
    assert!(other.refresh().unwrap());
    assert_eq!(text_of(&mut other, story), "Zero. One.");
    // Consolidated there: nothing new here, nothing lost.
    assert!(t.gm_journal.consolidate().unwrap());
    assert!(!other.refresh().unwrap());
    assert!(other.versions.contains_key(&version));
    let again = Journal::open(&t.gm_journal.dir.clone(), &t.gm_journal.private.join("third"), &t.token.public(), Mode::Shared).unwrap();
    assert!(again.versions.contains_key(&version));
}

#[test]
fn generations_nothing_was_written_in_can_go_and_the_part_stays_as_it_was() {
    let mut t = table();
    let (article, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    let order = |j: &Journal| j.parts_of(article).into_iter().map(|(l, _)| l).collect::<Vec<_>>();
    let before = order(&t.gm_journal);
    // Who may edit changes three times: three new generations, the second
    // one written in.
    let mut heads = Vec::new();
    for (generation, text) in [(2, None), (3, Some("Written in the third.")), (4, None)] {
        let head = t.gm_journal.head(story).unwrap().clone();
        let now = t.gm_journal.plain(story).unwrap();
        let next = new_head(
            &mut t.ring, head.article, story, false, Level::Open, generation, &"players".parse().unwrap(),
            &Access { section: "story".into(), own: "players".into(), edit: if generation % 2 == 0 { "gm" } else { "players" }.into() },
            &now, None, true,
        )
        .unwrap();
        t.gm_journal.control(&t.token, Item::Layer(next), "s-gm", 10 + generation as u64).unwrap();
        t.gm_journal.set_keys(t.ring.everything());
        heads.push(t.gm_journal.latest.get(&format!("layer {story}")).unwrap().1.clone());
        if let Some(text) = text {
            t.gm_journal.edit(story, generation, &[], &plain("", text), &t.gm, &format!("s-{generation}"), 20 + generation as u64).unwrap();
        }
    }
    let gone = t.gm_journal.superseded();
    // The second generation: nothing written, between the first and the current one.
    assert!(gone.contains(&heads[0]), "the second generation goes");
    assert!(!gone.contains(&heads[1]), "the third was written in");
    assert!(!gone.contains(&heads[2]), "the current one stays");
    t.gm_says(Item::Deleted { records: gone });
    assert_eq!(text_of(&mut t.gm_journal, story), "Written in the third.");
    assert_eq!(order(&t.gm_journal), before, "the parts stand as they did");
    assert!(t.gm_journal.superseded().is_empty());
}

#[test]
fn a_tidied_world_still_sends_its_own_record_first() {
    let mut t = table();
    // Many records before the world's record is written anew — then tidied:
    // the first world record goes, the newest stands far down the count.
    for n in 0..30 {
        t.gm_says(Item::Versions { max: 10 + n });
    }
    let (public, calendar, icon, home, gm) = {
        let (p, c, i, h, g) = t.gm_journal.world_item().unwrap();
        (p.clone(), c.clone(), i.to_string(), h.to_string(), g.clone())
    };
    t.gm_says(Item::World { public, calendar, icon: format!("{icon}!"), home, gm });
    let gone = t.gm_journal.superseded();
    t.gm_says(Item::Deleted { records: gone });
    let order = t.gm_journal.in_order(&t.gm_journal.inventory());
    let first = vec![String::from_utf8(t.gm_journal.raw(&order[0]).unwrap()).unwrap()];
    assert!(world_of(&first, t.token.public().world_id.as_str()).is_ok(), "the world's own record comes first");
}

#[test]
fn a_supporter_lets_a_player_in_with_the_keys_for_everyone_and_a_removal_after_it_counts() {
    let mut t = table();
    let (_, _, story) = t.article(("players", "players", "The inn is old.", Level::Open));
    let (_, _, secret) = t.article(("gm", "gm", "The innkeeper is a spy.", Level::Open));
    let carla = Identity::create("Carla").unwrap();
    let held = HeldKeys::unseal(t.bob.seal_secret(), &t.ring.grant(&t.bob.public(), &[attr("players"), attr("all")]).unwrap());
    let content = |member: &Identity| GrantContent {
        attributes: vec!["players".into(), "all".into()],
        keys: held.grant_to(&member.public(), &[attr("players"), attr("all")]).unwrap(),
    };
    t.to_bob();

    // Not a supporter yet: his act waits, and lets nobody in.
    let id = t.bob_journal.support_grant(&t.bob, &t.bob.public(), &carla.public(), &content(&carla), &[], 5);
    assert!(id.is_err(), "his own device does not take it either");
    t.gm_says(Item::Supporters { ids: vec![t.bob.id().to_string()] });
    t.to_bob();
    t.bob_journal.support_grant(&t.bob, &t.bob.public(), &carla.public(), &content(&carla), &[], 5).unwrap();
    t.to_gm();
    assert!(t.gm_journal.grant(carla.id().as_str()).is_some(), "in, on every device");
    assert_eq!(t.gm_journal.acted_by(carla.id().as_str()).map(|b| b.id.clone()), Some(t.bob.public().id));
    assert_eq!(t.gm_journal.acted_on(), vec![(carla.id().to_string(), true)], "for the game master's device to take in");

    // Carla reads what every player reads — and nothing of the GM's.
    let mut cara = open(t._dir.path(), "carla", &t.token);
    cara.set_me(&carla);
    t.to(&mut cara);
    let grant = cara.grant_content(carla.id().as_str(), carla.seal_secret()).unwrap();
    cara.set_keys(HeldKeys::unseal(carla.seal_secret(), &grant.keys));
    assert_eq!(text_of(&mut cara, story), "The inn is old.");
    assert!(cara.plain(secret).is_err(), "the GM's part stays closed");

    // The game master takes her out: the act before it counts no more.
    t.gm_says(Item::Revoked { member: carla.id().to_string() });
    assert!(t.gm_journal.grant(carla.id().as_str()).is_none());
    assert!(!t.gm_journal.superseded().is_empty(), "the act the GM's word took over goes");
    // Let in again by the supporter, after the removal: in again.
    t.to_bob();
    t.bob_journal.support_grant(&t.bob, &t.bob.public(), &carla.public(), &content(&carla), &[], 9).unwrap();
    t.to_gm();
    assert!(t.gm_journal.grant(carla.id().as_str()).is_some());
    // No supporter any more: what he alone let in is out.
    t.gm_says(Item::Supporters { ids: Vec::new() });
    assert!(t.gm_journal.grant(carla.id().as_str()).is_none());
}

#[test]
fn a_supporter_takes_a_player_out_and_hands_on_the_new_keys_but_never_touches_the_gm_or_a_supporter() {
    let mut t = table();
    let dana = Identity::create("Dana").unwrap();
    let keys = t.ring.grant(&dana.public(), &[attr("players"), attr("all")]).unwrap();
    t.gm_says(grant(&dana, &t.gm, &["players", "all"], &keys));
    t.gm_says(Item::Supporters { ids: vec![t.bob.id().to_string()] });
    t.to_bob();

    // Bob takes Dana out, and turns the players' key over for those who stay.
    let mut turned = Keyring::from_json(&t.ring.to_json().unwrap()).unwrap();
    turned.rotate(&attr("players")).unwrap();
    let new = turned.new_since(&t.ring);
    t.bob_journal.support_revoke(&t.bob, &t.bob.public(), dana.id().as_str(), 7).unwrap();
    t.bob_journal.support_keys(&t.bob, &new, &[], 7).unwrap();
    t.to_gm();
    assert!(t.gm_journal.grant(dana.id().as_str()).is_none(), "out, on every device");
    assert_eq!(t.gm_journal.acted_on(), vec![(dana.id().to_string(), false)]);
    let theirs = t.gm_journal.turned_keys(t.gm.seal_secret());
    assert_eq!(theirs.current(&attr("players")).unwrap().key, turned.current(&attr("players")).unwrap().key, "the GM gets the new key");
    assert!(t.gm_journal.turned_keys(dana.seal_secret()).is_empty(), "Dana does not");

    // The game master writes their own word: the act is taken over — the keys stay.
    t.gm_says(Item::Revoked { member: dana.id().to_string() });
    assert!(t.gm_journal.acted_on().is_empty());
    let gone = t.gm_journal.superseded();
    t.gm_says(Item::Deleted { records: gone });
    assert!(!t.gm_journal.turned_keys(t.gm.seal_secret()).is_empty(), "what was written with them still opens");

    // Neither the game master nor a supporter is his to take out.
    t.to_bob();
    t.bob_journal.support_revoke(&t.bob, &t.bob.public(), t.gm.id().as_str(), 8).unwrap();
    t.bob_journal.support_revoke(&t.bob, &t.bob.public(), t.bob.id().as_str(), 9).unwrap();
    t.to_gm();
    assert!(t.gm_journal.grant(t.bob.id().as_str()).is_some(), "a supporter stays in");
    assert!(t.gm_journal.acted_on().is_empty());
}

#[test]
fn a_world_comes_as_its_frame_first_and_each_document_whole_when_asked() {
    let mut t = table();
    let (tavern, tavern_base, rumours) = t.article(("players", "players", "The inn is old.", Level::Open));
    let (harbour, _, tides) = t.article(("players", "players", "The tide is high.", Level::Open));
    t.to_bob();
    t.bob_writes(rumours, "The inn is old. It rains.", "s-bob");
    t.to_gm();

    // The frame: everything but the parts besides the documents' bases.
    let (total, all) = t.gm_journal.frame_after(None);
    assert_eq!(total, all.len());
    let ids: Vec<&String> = all.iter().map(|(_, id)| id).collect();
    let layer_of = |id: &String| match t.gm_journal.controls.get(id).map(|c| &c.item) {
        Some(Item::Layer(h)) => Some(h.layer),
        _ => None,
    };
    let layers: Vec<LayerId> = ids.iter().filter_map(|id| layer_of(id)).collect();
    assert!(layers.contains(&tavern_base), "the base is in it");
    assert!(!layers.contains(&rumours) && !layers.contains(&tides), "the other parts are not");
    assert!(!ids.iter().any(|id| t.gm_journal.drafts.get(*id).is_some_and(|d| d.layer == rumours)), "nor what is written there");

    // A piece at a time: from where the last one ended, nothing twice.
    let (_, rest) = t.gm_journal.frame_after(Some(&all[2].0));
    assert_eq!(rest.len(), all.len() - 3);
    assert_eq!(rest[0], all[3]);

    // A new device takes the frame in: the names, not the parts.
    let mut carla = open(t._dir.path(), "carla", &t.token);
    carla.set_keys(t.ring.everything());
    for (_, id) in &all {
        carla.add(&t.gm_journal.raw(id).unwrap()).unwrap();
    }
    assert!(carla.plain(rumours).is_err(), "not here yet");
    // Then a document, whole.
    let whole = t.gm_journal.rest_of(&[tavern, harbour]);
    assert_eq!(whole.len(), 2);
    let tavern_rest = &whole.iter().find(|(a, _)| *a == tavern).unwrap().1;
    let raws: Vec<Vec<u8>> = tavern_rest.iter().map(|id| t.gm_journal.raw(id).unwrap()).collect();
    carla.add_all(&raws.iter().map(|r| r.as_slice()).collect::<Vec<_>>()).unwrap();
    assert_eq!(text_of(&mut carla, rumours), "The inn is old. It rains.");
    assert!(carla.plain(tides).is_err(), "the harbour still waits");
}
