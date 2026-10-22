//! A complete run through every level: create a world, admit players,
//! assign a character, write an article, check the view.
//!
//! This is the flow the application will later drive through the interface.

use aw_crypto::{decrypt, encrypt, ContentKey, Identity, WorldToken};
use aw_model::{Article, Attribute, Layer, Policy, Level};
use aw_policy::{open_dek, wrap_dek, GrantedKey, HeldKeys, Keyring};

fn attr(text: &str) -> Attribute {
    text.parse().unwrap()
}

/// What a player can actually read: they try to open every layer and keep
/// what opens. No visibility check in the UI — the keys decide.
fn readable_titles(
    article: &Article,
    header: &[(usize, aw_policy::LayerHeader, Vec<u8>)],
    held: &HeldKeys,
) -> Vec<String> {
    let mut out = Vec::new();
    for (index, head, ciphertext) in header {
        if let Some(dek) = open_dek(head, held).unwrap() {
            let plaintext = decrypt(&dek, ciphertext).unwrap();
            assert_eq!(
                String::from_utf8(plaintext).unwrap(),
                article.layers[*index].title
            );
            out.push(article.layers[*index].title.clone());
        }
    }
    out
}

#[test]
fn gm_creates_a_world_player_sees_only_their_part() {
    // --- The GM sets up ------------------------------------------------
    let gm = Identity::create("Marla").unwrap();
    let world = WorldToken::create("Eldermere").unwrap();
    assert_eq!(world.world_id().prefix(), "world");

    let mut ring = Keyring::new();
    for a in ["players", "gm", "class:rogue", "class:mage", "player:bob"] {
        ring.ensure(&attr(a)).unwrap();
    }

    // --- Bob joins -----------------------------------------------------
    let bob = Identity::create("Bob").unwrap();
    let anna = Identity::create("Anna").unwrap();

    // The GM confirms Bob's fingerprint and signs the grant.
    let grant = format!("character:eleanel -> {} ; class:rogue", bob.public().id);
    let signature = world.sign(grant.as_bytes());
    world.public()
        .verify(grant.as_bytes(), &signature)
        .expect("the player must be able to check the grant");

    // Bob gets: himself, all players, and his character's class.
    let bobs_grants: Vec<GrantedKey> = ring
        .grant(
            &bob.public(),
            &[attr("player:bob"), attr("players"), attr("class:rogue")],
        )
        .unwrap();
    // Anna plays a mage.
    let annas_grants = ring
        .grant(&anna.public(), &[attr("players"), attr("class:mage")])
        .unwrap();

    // The control document holds the grants of all members.
    let control_document: Vec<GrantedKey> = bobs_grants
        .iter()
        .chain(annas_grants.iter())
        .cloned()
        .collect();

    // The GM gets their own keys like any other member.
    let gm_grants = ring
        .grant(&gm.public(), &[attr("gm"), attr("players"), attr("class:rogue"), attr("class:mage"), attr("player:bob")])
        .unwrap();

    let bob_keys = HeldKeys::unseal(bob.seal_secret(), &control_document);
    let anna_keys = HeldKeys::unseal(anna.seal_secret(), &control_document);
    let gm_keys = HeldKeys::unseal(gm.seal_secret(), &gm_grants);
    assert_eq!(bob_keys.len(), 3);
    assert_eq!(anna_keys.len(), 2);
    assert_eq!(gm_keys.len(), 5);

    // --- The GM writes an article --------------------------------------
    let mut citadel = Article::new(
        "place",
        "black-citadel",
        Layer::new("Description", Policy::just(Attribute::Players)).with_level(Level::Open),
    );
    citadel.push_layer(
        Layer::new("Back door", "class:rogue".parse().unwrap()).with_level(Level::Sealed),
    );
    citadel.push_layer(
        Layer::new("Letter to Bob", "player:bob".parse().unwrap()).with_level(Level::Sealed),
    );
    citadel
        .push_layer(Layer::new("Guards", Policy::just(Attribute::Gm)).with_level(Level::GmOnly));

    // Every layer gets a data key of its own.
    let mut packed = Vec::new();
    for (index, layer) in citadel.layers.iter().enumerate() {
        let dek = ContentKey::generate().unwrap();
        let head = wrap_dek(&layer.policy, layer.epoch, &ring, &dek).unwrap();
        let ciphertext = encrypt(&dek, layer.title.as_bytes()).unwrap();
        packed.push((index, head, ciphertext));
    }

    // --- What is meant for players ---------------------------------------
    let delivered: Vec<_> = packed
        .iter()
        .filter(|(i, _, _)| citadel.layers[*i].level.for_players())
        .cloned()
        .collect();
    assert_eq!(delivered.len(), 3, "the GM-only layer is not among them");

    // --- And what the players can open of it ---------------------------
    assert_eq!(
        readable_titles(&citadel, &delivered, &bob_keys),
        vec!["Description", "Back door", "Letter to Bob"]
    );
    assert_eq!(
        readable_titles(&citadel, &delivered, &anna_keys),
        vec!["Description"],
        "Anna is a mage and is not called Bob"
    );

    // The GM sees everything on their own device, the GM-only layer too.
    assert_eq!(
        readable_titles(&citadel, &packed, &gm_keys),
        vec!["Description", "Back door", "Letter to Bob", "Guards"]
    );

    // Even with the complete ciphertext of every layer it stays that way.
    assert_eq!(
        readable_titles(&citadel, &packed, &anna_keys),
        vec!["Description"],
        "the GM-only layer does not open either"
    );
}

#[test]
fn revealing_is_only_handing_over_a_key() {
    // The ciphertext has long been on the device; revealing means handing
    // over the attribute key. No network needed.
    let mut ring = Keyring::new();
    ring.ensure(&attr("knowledge:crypt-found")).unwrap();

    let bob = Identity::create("Bob").unwrap();

    let policy: Policy = "knowledge:crypt-found".parse().unwrap();
    let dek = ContentKey::generate().unwrap();
    let head = wrap_dek(&policy, 1, &ring, &dek).unwrap();
    let ciphertext = encrypt(&dek, b"Behind the altar a staircase leads down.").unwrap();

    // Before: Bob has the ciphertext but no key.
    let before = HeldKeys::unseal(bob.seal_secret(), &[]);
    assert!(open_dek(&head, &before).unwrap().is_none());

    // At the table: the GM hands over the key, a few hundred bytes.
    let handover = ring
        .grant(&bob.public(), &[attr("knowledge:crypt-found")])
        .unwrap();
    let after = HeldKeys::unseal(bob.seal_secret(), &handover);

    let key = open_dek(&head, &after).unwrap().unwrap();
    assert_eq!(
        String::from_utf8(decrypt(&key, &ciphertext).unwrap()).unwrap(),
        "Behind the altar a staircase leads down."
    );
}

#[test]
fn changing_class_locks_future_layers() {
    let mut ring = Keyring::new();
    let rogue = attr("class:rogue");
    ring.ensure(&rogue).unwrap();

    let bob = Identity::create("Bob").unwrap();
    let anna = Identity::create("Anna").unwrap();

    let bob_keys = HeldKeys::unseal(
        bob.seal_secret(),
        &ring.grant(&bob.public(), std::slice::from_ref(&rogue)).unwrap(),
    );

    let policy: Policy = "class:rogue".parse().unwrap();
    let old = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();

    // Bob becomes a mage. New epoch, only for the remaining rogues.
    ring.rotate(&rogue).unwrap();
    let _anna_keys = HeldKeys::unseal(
        anna.seal_secret(),
        &ring.grant(&anna.public(), std::slice::from_ref(&rogue)).unwrap(),
    );
    let new = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();

    assert!(open_dek(&old, &bob_keys).unwrap().is_some(), "Eleanel knows what she learned");
    assert!(open_dek(&new, &bob_keys).unwrap().is_none(), "anything new stays shut");
}
