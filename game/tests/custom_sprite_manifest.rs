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

use game::adventure::{custom_sprite_dir, custom_sprite_is_owned_by, custom_sprite_manifest_path, PUBLIC_SPRITE_OWNER};
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
    let entries = std::fs::read_dir(custom_sprite_dir()).unwrap_or_else(|e| panic!("custom sprite dir {} must be readable: {e}", custom_sprite_dir().display()));
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
    let text = std::fs::read_to_string(custom_sprite_manifest_path()).unwrap_or_else(|e| panic!("manifest {} must be readable: {e}", custom_sprite_manifest_path().display()));
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
         A sprite with no entry is rejected outright - there is no name-matching fallback. Add an owner line to {}.", custom_sprite_manifest_path().display()
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

    // CORRECTED 2026-09-07. This first asserted that `kmartbikes1` must NOT
    // reach `kmartbikes12`, on the belief that a player of that name existed
    // and the old gate had been failing open into their sprite.
    //
    // **There is no player `kmartbikes12`.** World 1's roster holds exactly one
    // kmart login, `kmartbikes1`, and that account had `custom/kmartbikes12`
    // equipped — legitimately, because "login followed by digits" made it their
    // SECOND sprite. The original assertion would have taken a sprite away from
    // the only person who has ever used it, and the test would have guarded the
    // theft.
    //
    // The manifest still fixes the underlying ambiguity: ownership is now
    // stated, so if a `kmartbikes12` ever registers, `kmartbikes12.gif` does not
    // silently become theirs — and it does not silently stop being
    // `kmartbikes1`'s either. That is the property worth having, and it is not
    // the same as "kmartbikes1 is locked out".
    assert!(custom_sprite_is_owned_by("kmartbikes1", "kmartbikes12"), "kmartbikes12 is kmartbikes1's second sprite - the only kmart account there has ever been, and the one that had it equipped");
    assert!(custom_sprite_is_owned_by("kmartbikes1", "kmartbikes3"), "and kmartbikes3 is theirs on the same evidence - under the old rule it needed a login `kmartbikes3` or `kmartbikes`, so it was selectable by nobody");
    assert!(!custom_sprite_is_owned_by("kmartbikes12", "kmartbikes12"), "a login that has never existed must not own it - ownership is stated now, not inferred from the name");

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
