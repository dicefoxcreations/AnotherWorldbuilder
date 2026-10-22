//! The hard invariant of the security boundary.
//!
//! For every policy and every set of attributes it must hold: **the data key
//! opens exactly when the policy is satisfied.** Not more often, not less.
//!
//! The test is exhaustive rather than random. Over a universe of four
//! attributes there are 16 subsets and 55 policies in disjunctive normal form
//! with up to two groups of up to two attributes each — all 880 combinations
//! are checked. A passing run is therefore a proof for this universe, not a
//! sample.

use aw_crypto::ContentKey;
use aw_model::{Attribute, Conjunction, Policy};
use aw_policy::{open_dek, wrap_dek, HeldKeys, Keyring};
use std::collections::BTreeSet;

const UNIVERSE: [&str; 4] = ["class:rogue", "background:noble", "faction:guild", "players"];

fn attr(text: &str) -> Attribute {
    text.parse().unwrap()
}

/// All subsets of the universe.
fn all_subsets() -> Vec<BTreeSet<Attribute>> {
    (0..(1u32 << UNIVERSE.len()))
        .map(|mask| {
            UNIVERSE
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, name)| attr(name))
                .collect()
        })
        .collect()
}

/// All AND groups with one or two attributes.
fn all_groups() -> Vec<Conjunction> {
    let mut out = Vec::new();
    for (i, a) in UNIVERSE.iter().enumerate() {
        out.push(Conjunction::single(attr(a)));
        for b in UNIVERSE.iter().skip(i + 1) {
            out.push(Conjunction::new([attr(a), attr(b)]).unwrap());
        }
    }
    out
}

/// All policies with one or two OR groups.
fn all_policies() -> Vec<Policy> {
    let groups = all_groups();
    let mut out = Vec::new();
    for (i, g) in groups.iter().enumerate() {
        out.push(Policy::new([g.clone()]).unwrap());
        for h in groups.iter().skip(i + 1) {
            out.push(Policy::new([g.clone(), h.clone()]).unwrap());
        }
    }
    out
}

fn full_keyring() -> Keyring {
    let mut ring = Keyring::new();
    for name in UNIVERSE {
        ring.ensure(&attr(name)).unwrap();
    }
    ring
}

fn keys_for(ring: &Keyring, attribute: &BTreeSet<Attribute>) -> HeldKeys {
    let mut held = HeldKeys::new();
    for a in attribute {
        if let Some(key) = ring.current(a) {
            held.insert(key.at, key.key);
        }
    }
    held
}

#[test]
fn decryptable_exactly_when_the_policy_holds() {
    let ring = full_keyring();
    let subsets = all_subsets();
    let policies = all_policies();

    assert_eq!(subsets.len(), 16);
    assert_eq!(policies.len(), 55);

    let mut checked = 0usize;
    let mut openings = 0usize;

    for policy in &policies {
        let dek = ContentKey::generate().unwrap();
        let header = wrap_dek(policy, 1, &ring, &dek).unwrap();

        for held_set in &subsets {
            let held = keys_for(&ring, held_set);
            let allowed = policy.satisfied_by(held_set);
            let opened = open_dek(&header, &held).unwrap();

            assert_eq!(
                opened.is_some(),
                allowed,
                "policy '{policy}' with attributes {:?}: allowed={allowed}, opened={}",
                held_set.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                opened.is_some()
            );

            // If opened, then with the right key too.
            if let Some(key) = opened {
                assert_eq!(key.as_bytes(), dek.as_bytes(), "wrong data key");
                openings += 1;
            }
            checked += 1;
        }
    }

    assert_eq!(checked, 880);
    assert!(openings > 0, "otherwise the test would prove nothing");
}

#[test]
fn without_a_key_nothing_ever_opens() {
    let ring = full_keyring();
    let empty = HeldKeys::new();

    for policy in all_policies() {
        let header = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();
        assert!(
            open_dek(&header, &empty).unwrap().is_none(),
            "policy '{policy}' opened without any key"
        );
    }
}

#[test]
fn keys_of_a_foreign_world_open_nothing() {
    // Two worlds, the same attribute names, different keys.
    let world_a = full_keyring();
    let world_b = full_keyring();
    let everything: BTreeSet<Attribute> = UNIVERSE.iter().map(|n| attr(n)).collect();
    let foreign_keys = keys_for(&world_b, &everything);

    for policy in all_policies() {
        let header = wrap_dek(&policy, 1, &world_a, &ContentKey::generate().unwrap()).unwrap();
        assert!(
            open_dek(&header, &foreign_keys).unwrap().is_none(),
            "policy '{policy}' opened with the keys of a foreign world"
        );
    }
}

#[test]
fn one_attribute_more_never_opens_less() {
    // Monotonicity: whoever gains something loses nothing. A violation would
    // mean a grant takes something away from someone — a nasty surprise.
    let ring = full_keyring();

    for policy in all_policies() {
        let header = wrap_dek(&policy, 1, &ring, &ContentKey::generate().unwrap()).unwrap();

        for held_set in all_subsets() {
            if open_dek(&header, &keys_for(&ring, &held_set))
                .unwrap()
                .is_none()
            {
                continue;
            }
            for extra in UNIVERSE {
                let mut larger = held_set.clone();
                larger.insert(attr(extra));
                assert!(
                    open_dek(&header, &keys_for(&ring, &larger))
                        .unwrap()
                        .is_some(),
                    "policy '{policy}' closed because of an extra attribute"
                );
            }
        }
    }
}
