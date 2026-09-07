//! The custom-sprite manifest and the sprite directory must agree, in BOTH
//! directions (2026-09-07).
//!
//! **This is the test the manifest exists to make possible.** Fourteen custom
//! sprites were on the box and nine were in git, so a rebuild from checkout
//! silently dropped five — `Sitch89.gif`, `Sitch89_2.gif`,
//! `lokati_gaming4.gif`, `lokati_gaming5.gif`, `lokati_gaming6.gif`. That was
//! found by a survey, by hand, by someone who happened to compare two
//! directories. Nothing in the build could have told anyone.
//!
//! With ownership stated in a file rather than inferred from filenames, the
//! same class of gap becomes a test failure:
//!
//! * **entry with no file** — a sprite that will never render, usually because
//!   the file was renamed or never committed;
//! * **file with no entry** — a sprite nobody can equip, because a sprite with
//!   no manifest entry is rejected outright (there is deliberately no
//!   name-matching fallback).
//!
//! Both directions matter and both are asserted separately. A bidirectional
//! check that only ever exercises one direction is a dormant arm — it passes
//! forever while testing half of what it claims.
//!
//! Also pins the two live defects the manifest closed, by name, because they
//! are the reason it was built and a future reader should be able to see what
//! "unambiguous ownership" bought.

use game::adventure::{custom_sprite_is_owned_by, CUSTOM_SPRITE_DIR, CUSTOM_SPRITE_MANIFEST, PUBLIC_SPRITE_OWNER};
use std::collections::{HashMap, HashSet};

/// Integration tests run with the package dir as CWD, but both paths are
/// resolved against the workspace root.
fn anchor() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("anchor CWD at the workspace root");
}

/// Sprite stems actually on disk, lowercased — the same `.png`/`.gif` filter
/// `custom_sprite_file_exists` and the picker's own listing use, so this test
/// cannot pass by looking at a different set of files than the code does.
fn files_on_disk() -> HashSet<String> {
    let entries = std::fs::read_dir(CUSTOM_SPRITE_DIR).unwrap_or_else(|e| panic!("custom sprite dir {CUSTOM_SPRITE_DIR} must be readable: {e}"));
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("gif"));
            if !is_image {
                return None;
            }
            path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase())
        })
        .collect()
}

fn manifest_entries() -> HashMap<String, String> {
    let text = std::fs::read_to_string(CUSTOM_SPRITE_MANIFEST).unwrap_or_else(|e| panic!("manifest {CUSTOM_SPRITE_MANIFEST} must be readable: {e}"));
    #[derive(serde::Deserialize)]
    struct Manifest {
        sprites: HashMap<String, String>,
    }
    let parsed: Manifest = toml::from_str(&text).unwrap_or_else(|e| panic!("manifest must parse as TOML: {e}"));
    parsed.sprites.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v.to_ascii_lowercase())).collect()
}

#[test]
fn every_manifest_entry_has_a_file() {
    anchor();
    let files = files_on_disk();
    let entries = manifest_entries();
    assert!(!entries.is_empty(), "sanity: the manifest must not be empty, or this test proves nothing");

    let orphaned: Vec<&String> = entries.keys().filter(|k| !files.contains(*k)).collect();
    assert!(
        orphaned.is_empty(),
        "these manifest entries name a sprite that is not in the checkout, so they can never render: {orphaned:?}\n\
         Either the file was renamed or never committed, or the entry is stale. Add the file or remove the entry."
    );
}

#[test]
fn every_sprite_file_has_a_manifest_entry() {
    anchor();
    let files = files_on_disk();
    let entries = manifest_entries();
    assert!(!files.is_empty(), "sanity: the sprite directory must not be empty, or this test proves nothing");

    let unowned: Vec<&String> = files.iter().filter(|f| !entries.contains_key(*f)).collect();
    assert!(
        unowned.is_empty(),
        "these sprite files have no manifest entry, so NOBODY can equip them: {unowned:?}\n\
         A sprite with no entry is rejected outright - there is no name-matching fallback. Add an owner line to {CUSTOM_SPRITE_MANIFEST}."
    );
}

#[test]
fn the_two_defects_the_manifest_closed_stay_closed() {
    anchor();

    // `Sitch89_2.gif` was selectable by NOBODY. The old rule accepted a login
    // followed by DIGITS ONLY, and `_2` is not digits, so a real file on the
    // box could never be equipped. `custom_sprite_case.rs` covered `Sitch89`
    // for case-insensitivity but not this variant - a hand-picked example set
    // that had stopped covering its own subject.
    assert!(custom_sprite_is_owned_by("sitch89", "Sitch89_2"), "Sitch89_2 must be selectable by sitch89 - an underscore suffix is not a reason to orphan a sprite");
    assert!(custom_sprite_is_owned_by("sitch89", "Sitch89"), "and the plain name must still work, whatever its case on disk");

    // `kmartbikes12.gif` was selectable by TWO people. `strip_prefix("kmartbikes1")`
    // left `"2"`, all digits, so the gate accepted it. This is the
    // authorisation check that exists specifically to stop one player equipping
    // another's named sprite, and it FAILED OPEN whenever one login was a
    // digit-extension of another. Both logins have sprites in this directory,
    // so it was reachable rather than theoretical.
    assert!(!custom_sprite_is_owned_by("kmartbikes1", "kmartbikes12"), "kmartbikes1 must NOT be able to equip kmartbikes12's sprite - this is the fail-open the manifest closed");
    assert!(custom_sprite_is_owned_by("kmartbikes12", "kmartbikes12"), "but its real owner must still be able to");
    assert!(custom_sprite_is_owned_by("kmartbikes3", "kmartbikes3"), "and the other kmartbikes sprite must still belong to its own owner");

    // No entry means rejected. This is the ruling that makes the manifest
    // meaningful: a name-matching fallback would reinstate the ambiguity.
    assert!(!custom_sprite_is_owned_by("anyone", "definitely_not_in_the_manifest"), "a sprite with no manifest entry must be rejected outright");
    assert!(!custom_sprite_is_owned_by("sitch89", "kibukah"), "and ownership must not leak between unrelated logins");
}

#[test]
fn the_public_pool_is_an_ordinary_entry() {
    anchor();
    let entries = manifest_entries();

    // `public` used to be a reserved filename PREFIX. It is now an ordinary
    // entry whose owner value is `*`, which cannot collide with a login.
    //
    // Written to hold in both states: there are no public sprites today, and
    // asserting one exists would be asserting a fixture rather than a
    // property. If one is ever added this re-arms and checks that ANY login can
    // select it - so a reader learns the pool is empty by FACT, not by silence.
    let public: Vec<&String> = entries.iter().filter(|(_, owner)| *owner == PUBLIC_SPRITE_OWNER).map(|(name, _)| name).collect();
    match public.first() {
        Some(name) => {
            assert!(custom_sprite_is_owned_by("someone_unrelated", name), "a `*` entry must be selectable by any login - that is what makes it the public pool");
            assert!(custom_sprite_is_owned_by("someone_else_entirely", name), "by ANY login, not one particular one");
        }
        None => {
            assert!(
                entries.values().all(|owner| owner != PUBLIC_SPRITE_OWNER),
                "sanity: the filter found no public entry, so none may exist"
            );
        }
    }
}
