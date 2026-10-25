//! Who is in a world, with which roles, and the keys that go with them.
//!
//! The game master keeps it in the profile; a supporter knows it from the
//! game master's keys sealed for them (`Item::Keys`) and what supporters did
//! since (`journal::Support`). Both change it alike — let a player in, give
//! or take their roles, take them out; what is taken turns over — and write
//! it down differently: the game master as their word, a supporter as acts
//! the game master's device takes in as its own.
//!
//! A host showing a world to its members itself acts for a supporter too:
//! the game master's device as the game master, whose list it holds; a
//! server — in an unencrypted world, whose grants name everyone openly — with
//! acts it signs in the supporter's name.

use super::*;
use aw_model::Attribute;
use std::collections::BTreeSet;

/// The members of a world and its keyring.
#[derive(Clone)]
pub struct Roster {
    pub members: Vec<Member>,
    pub ring: Keyring,
}

/// What every player has.
const EVERY_PLAYER: [&str; 2] = ["players", "all"];

fn parsed(attribute: &str) -> Result<Attribute> {
    attribute.parse().map_err(|_| StoreError::Journal(format!("“{attribute}” is no role.")))
}

impl Roster {
    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.identity.id.as_str() == id)
    }

    /// Lets someone in as a player: with what every player has, and
    /// `attributes`.
    pub fn admit(&mut self, identity: IdentityPublic, attributes: &[String]) -> Result<()> {
        let mut all: Vec<String> = EVERY_PLAYER.iter().map(|a| a.to_string()).collect();
        for a in attributes {
            if a.parse::<Attribute>().is_ok() && !all.contains(a) {
                all.push(a.clone());
            }
        }
        for a in &all {
            self.ring.ensure(&parsed(a)?)?;
        }
        self.members.retain(|m| m.identity.id != identity.id);
        self.members.push(Member { identity, role: Role::Player, attributes: all });
        Ok(())
    }

    /// What a member is assigned to besides what their role gives. What is
    /// taken away turns over: what is written from then on is closed to
    /// them; what they had, they keep.
    pub fn assign(&mut self, id: &str, attributes: &[String]) -> Result<()> {
        let m = self.members.iter_mut().find(|m| m.identity.id.as_str() == id).ok_or_else(|| StoreError::Journal("There is no such member.".into()))?;
        let given: &[&str] = if m.role == Role::Gm { &["gm", "players", "all"] } else { &EVERY_PLAYER };
        let mut wanted: Vec<String> = given.iter().map(|a| a.to_string()).collect();
        for a in attributes {
            let a = a.trim().to_string();
            parsed(&a)?;
            if !wanted.contains(&a) {
                wanted.push(a);
            }
        }
        for a in &wanted {
            self.ring.ensure(&parsed(a)?)?;
        }
        let gone: Vec<String> = m.attributes.iter().filter(|a| !wanted.contains(a)).cloned().collect();
        m.attributes = wanted;
        for a in gone.iter().filter_map(|a| a.parse::<Attribute>().ok()) {
            self.ring.rotate(&a)?;
        }
        Ok(())
    }

    /// Takes a player out: every key they held turns over.
    pub fn remove(&mut self, id: &str) -> Result<Option<Member>> {
        let Some(at) = self.members.iter().position(|m| m.identity.id.as_str() == id) else { return Ok(None) };
        if self.members[at].role == Role::Gm {
            return Err(StoreError::Journal("The game master cannot be removed — hand the world over first.".into()));
        }
        let gone = self.members.remove(at);
        for a in gone.attributes.iter().filter_map(|a| a.parse::<Attribute>().ok()) {
            if self.ring.current_epoch(&a).is_some() {
                self.ring.rotate(&a)?;
            }
        }
        Ok(Some(gone))
    }

    /// Takes in what supporters did since the game master's word, as far as
    /// it is sealed for `me`.
    pub(super) fn take_in(&mut self, journal: &Journal, me: &Identity) {
        let secret = me.seal_secret();
        self.ring.merge(&journal.turned_keys(secret));
        for (id, inside) in journal.acted_on() {
            if !inside {
                self.members.retain(|m| m.identity.id.as_str() != id);
                continue;
            }
            let (Some((identity, _)), Some(content)) = (journal.grant(&id), journal.grant_content(&id, secret)) else { continue };
            match self.members.iter_mut().find(|m| m.identity.id.as_str() == id) {
                Some(m) => m.attributes = content.attributes,
                None => self.members.push(Member { identity: identity.clone(), role: Role::Player, attributes: content.attributes }),
            }
        }
    }

    /// Whether keys turned over since `before` — what was sealed with the
    /// old ones is sealed anew.
    fn turned_since(&self, before: &Roster) -> bool {
        self.ring.new_since(&before.ring).everything().iter().any(|(at, _)| before.ring.current_epoch(&at.attribute).is_some())
    }
}

impl Store {
    /// The roster as this device may change it: the game master's own — or,
    /// where the game master's keys are sealed for this device, a
    /// supporter's.
    pub fn roster(&self, world_id: &str) -> Result<Roster> {
        let me = self.load_identity()?;
        if self.holds_token(world_id) {
            let mut roster = Roster { members: self.load_local(world_id)?.members, ring: self.load_keyring(world_id)? };
            self.with_journal(world_id, |j| {
                roster.take_in(j, &me);
                Ok(())
            })?;
            return Ok(roster);
        }
        if crate::host::acting().is_some() {
            return self.with_journal(world_id, |j| {
                if j.encrypted() {
                    return Err(StoreError::Journal("A server holds no keys of this world.".into()));
                }
                let gm = j.world_item().map(|(.., gm)| gm.clone());
                let mut members: Vec<Member> = gm.into_iter().map(|identity| Member { identity, role: Role::Gm, attributes: vec!["gm".into(), "players".into(), "all".into()] }).collect();
                for identity in j.granted_members() {
                    if let Some(grant) = j.grant_content(identity.id.as_str(), me.seal_secret()) {
                        members.push(Member { identity, role: Role::Player, attributes: grant.attributes });
                    }
                }
                Ok(Roster { members, ring: Keyring::new() })
            });
        }
        self.with_journal(world_id, |j| {
            let keys = sealed_keys(j, &me)?;
            let mut roster = Roster { members: keys.members, ring: Keyring::from_json(&keys.keyring)? };
            roster.take_in(j, &me);
            Ok(roster)
        })
    }

    /// Writes down what changed between `before` and `after`: the game
    /// master's word, or a supporter's acts.
    pub fn keep_roster(&self, world_id: &str, before: &Roster, after: &Roster) -> Result<()> {
        let gone: Vec<String> = before.members.iter().map(|m| m.identity.id.to_string()).filter(|id| after.member(id).is_none()).collect();
        let mut requests = self.load_requests(world_id)?;
        let asked = requests.len();
        requests.retain(|r| after.member(r.identity.id.as_str()).is_none());
        if requests.len() != asked {
            self.save_requests(world_id, &requests)?;
        }
        // The game master's device writes its own word — for whomever it acts.
        if self.holds_token(world_id) {
            return crate::host::as_nobody(|| self.keep_as_gm(world_id, before, after, &gone));
        }
        let me = self.load_identity()?;
        // A server signs in the name of the supporter it acts for.
        let by = crate::host::acting().map(|a| a.member).unwrap_or_else(|| me.public());
        let admins = self.load_local(world_id)?.admins;
        let encrypted = self.is_encrypted(world_id)?;
        let new = after.ring.new_since(&before.ring);
        let turned: BTreeSet<Attribute> = new.attributes().cloned().collect();
        let open = self.journal(world_id)?;
        let mut guard = open.lock().map_err(poisoned)?;
        let journal = &mut guard.journal;
        // The other supporters, and whom the world is being handed to: they
        // change it too, and need to know.
        let others: Vec<IdentityPublic> = journal
            .supporters()
            .into_iter()
            .chain(journal.handing_to())
            .filter(|id| id != by.id.as_str())
            .filter_map(|id| after.member(&id).map(|m| m.identity.clone()).or_else(|| admins.iter().find(|a| a.id.as_str() == id).cloned()))
            .collect();
        let at = now();
        if encrypted && !new.is_empty() {
            journal.support_keys(&me, &new, &others, at)?;
        }
        for id in &gone {
            journal.support_revoke(&me, &by, id, at)?;
        }
        for m in after.members.iter().filter(|m| m.role == Role::Player) {
            let granted = m.granted_attributes();
            // A key the ring lacks — a player's own name, say — is left out.
            let attributes: Vec<Attribute> = granted.iter().filter_map(|a| a.parse().ok()).filter(|a| after.ring.current_epoch(a).is_some()).collect();
            let same = before.member(m.identity.id.as_str()).is_some_and(|b| b.attributes == m.attributes);
            if same && !attributes.iter().any(|a| turned.contains(a)) {
                continue;
            }
            let keys = if encrypted { after.ring.grant(&m.identity, &attributes)? } else { Vec::new() };
            journal.support_grant(&me, &by, &m.identity, &GrantContent { attributes: granted, keys }, &others, at)?;
        }
        Ok(())
    }

    /// The game master's word on what changed.
    fn keep_as_gm(&self, world_id: &str, before: &Roster, after: &Roster, gone: &[String]) -> Result<()> {
        {
            let mut meta = self.load_world_meta(world_id)?;
            meta.members = after.members.clone();
            meta.invited.retain(|i| after.member(i).is_none());
            meta.supporters.retain(|s| after.member(s).is_some());
            self.write_keyring(world_id, &after.ring)?;
            // Saving the members hands out the keys anew.
            self.save_world_meta(&meta)?;
            for id in gone {
                self.gone(world_id, id)?;
            }
            if after.turned_since(before) {
                self.reseal(world_id)?;
            }
        }
        Ok(())
    }

    /// Someone the game master took out: the journal says so, and nobody
    /// takes their place, nor they anyone's.
    fn gone(&self, world_id: &str, id: &str) -> Result<()> {
        self.no_takeovers(world_id, id)?;
        if self.handing_to(world_id)?.as_deref() == Some(id) {
            self.hand_over(world_id, None)?;
        }
        // Every device learns of it: the others' stop passing the world on.
        if self.with_journal(world_id, |j| Ok(j.word_on(id) == Some(true) || j.acted_by(id).is_some()))? {
            self.control(world_id, Item::Revoked { member: id.to_string() }, "")?;
        }
        Ok(())
    }

    fn no_takeovers(&self, world_id: &str, id: &str) -> Result<()> {
        for (new, old) in self.takeovers(world_id)? {
            if new == id || old == id {
                self.set_takeover(world_id, &new, None)?;
            }
        }
        Ok(())
    }

    /// Takes in what supporters did — the game master's device: their acts
    /// become its own word. How many members they changed.
    pub fn adopt_support(&self, world_id: &str) -> Result<usize> {
        if !self.is_gm(world_id) {
            return Ok(0);
        }
        let me = self.load_identity()?;
        let (acts, turned) = self.with_journal(world_id, |j| Ok((j.acted_on().len(), j.turned_keys(me.seal_secret()))))?;
        if acts == 0 && !self.load_keyring(world_id)?.merge(&turned) {
            return Ok(0);
        }
        let before = Roster { members: self.load_local(world_id)?.members, ring: self.load_keyring(world_id)? };
        let after = self.roster(world_id)?;
        self.keep_roster(world_id, &before, &after)?;
        Ok(acts)
    }

    /// Takes a member out — or, the game master's, an id let in ahead of
    /// time. A supporter takes out players, not other supporters.
    pub fn remove_member(&self, world_id: &str, id: &str) -> Result<WorldMeta> {
        let gm = self.is_gm(world_id);
        if !gm && !self.supports(world_id)? {
            return Err(StoreError::Journal("Only the GM or a supporter takes someone out.".into()));
        }
        if !gm && self.with_journal(world_id, |j| Ok(j.supporters().iter().any(|s| s == id)))? {
            return Err(StoreError::Journal("Only the GM takes a supporter out.".into()));
        }
        if gm {
            let mut meta = self.load_world_meta(world_id)?;
            if meta.invited.iter().any(|i| i == id) {
                meta.invited.retain(|i| i != id);
                self.save_world_meta(&meta)?;
            }
            self.no_takeovers(world_id, id)?;
        }
        let before = self.roster(world_id)?;
        let mut after = before.clone();
        after.remove(id)?;
        self.keep_roster(world_id, &before, &after)?;
        self.load_world_meta(world_id)
    }
}

/// The game master's keys sealed for `me`.
pub(super) fn sealed_keys(journal: &Journal, me: &Identity) -> Result<GmKeys> {
    let sealed = journal.keys_for(me.id().as_str()).ok_or_else(|| StoreError::Journal("There are no game master's keys for you in this world.".into()))?;
    let plain = sealed.open(me.seal_secret()).ok_or_else(|| StoreError::Journal("The game master's keys do not open for you.".into()))?;
    Ok(serde_json::from_slice(&plain)?)
}
