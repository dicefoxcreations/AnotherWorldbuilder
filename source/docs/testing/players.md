# Players: a new id

As of October 2026. Code: `lib.rs` (`member_replace`, `admit`), `store.rs` (`takeovers`); interface: *Options → Players → New ID*.

A player whose device was set up anew without their key backup has a new id. The GM gives it to them: **New ID** at the player, then the new id. What is assigned to the old id — their character, types, attributes — goes to the new one, and the old id leaves the world; its keys turn over, as at any removal. The new id gets the keys of every epoch of its attributes, so it reads what was written before, as the old one did.

- Is the new id a member already, or asking to join: at once.
- Otherwise it waits as one let in ahead of time ("Takes Anna's place" under the id, "Moves to id:… as soon as it connects" at Anna) and takes over when it shows up.
- Taking the new id out again ends that.

What the old id wrote stays theirs in the versions.

**Automatic:** `network::tests::a_new_id_takes_a_players_place_with_what_was_theirs`, `who_comes_after_a_removal_reads_what_was_written_before`.

**By hand, 2026-10-23,** in the example world (demo profile): two cards made with `cargo run -p aw-crypto --example player_card -- Anna <file.awid>`; the old Anna let in *From file* with Wren assigned; *New ID* with the second card's id → waits, both notes show; the second card *From file* → Anna with the new id has Wren, the old id is gone.
