//! `BASIC_ENEMY_SPRITES` must name files that really exist, with exactly
//! the casing written down (2026-09-02).
//!
//! This is the `Sitch89.gif` class of bug (see `custom_sprite_case.rs`) and
//! it is nastier here, because nothing in Rust ever opens these files. The
//! server only puts the NAME on the wire; a browser on someone else's
//! machine turns it into a URL. So a wrong name cannot fail a build, fail
//! a request, or log anything - it renders as `drawEnemy`'s red
//! placeholder circle, on the live stream, and nowhere else.
//!
//! The dev boxes are Windows (case-insensitive) and production is Linux
//! (case-sensitive), so `basicenemy/01-Goblin-Warrior` would look correct
//! in every local check and be a hole in production only. `.exists()`
//! cannot see that difference on Windows - it answers the host
//! filesystem's question, not the web server's. This test therefore
//! compares against the byte-exact names `read_dir` reports, which are the
//! real on-disk ones on both platforms, so a mis-cased entry fails HERE
//! rather than on stream.
//!
//! Checked in BOTH directions on purpose. The list is generated from a
//! directory listing (`ls *.png | sed 's/\.png$//' | sort`) and is only
//! ever correct while it still matches that listing - so a sprite dropped
//! in without regenerating it fails too, rather than quietly never
//! appearing in a fight.
//!
//! Asserted against the REAL sprite directory rather than a fixture, for
//! the reason `custom_sprite_case.rs` gives: that directory is what varies
//! per deployment, and a fixture would prove the list only against itself.
//! Same CWD anchoring, same reason - `cargo test`'s CWD is the PACKAGE
//! root (`game/`) while the sprite path is resolved from the workspace
//! root in production.

use std::collections::HashSet;

use game::adventure::BASIC_ENEMY_SPRITES;

/// Where the overlay resolves these names against - `getOrLoadSprite`
/// builds `sprites/{name}.png`, so a name of `basicenemy/01-goblin-warrior`
/// is the file `public_adventure_overlay/sprites/basicenemy/01-goblin-warrior.png`.
const SPRITE_ROOT: &str = "public_adventure_overlay/sprites";

fn anchor_cwd_at_workspace_root() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
}

/// The byte-exact file names `read_dir` reports for the basic-enemy
/// directory. Byte-exact is the whole point: on Windows this returns the
/// real stored casing even though the filesystem would have matched any
/// other, which is what lets a Windows run catch a Linux-only break.
fn png_names_on_disk() -> Vec<String> {
    let dir = format!("{SPRITE_ROOT}/basicenemy");
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("the basic-enemy sprite directory must exist at {dir} (CWD is the workspace root): {e}"));
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            name.ends_with(".png").then_some(name)
        })
        .collect();
    names.sort();
    names
}

#[test]
fn every_basic_enemy_sprite_exists_on_disk_with_exactly_that_casing() {
    anchor_cwd_at_workspace_root();
    let on_disk: HashSet<String> = png_names_on_disk().into_iter().collect();
    assert!(!on_disk.is_empty(), "no .png files found under {SPRITE_ROOT}/basicenemy - the art is missing from the checkout entirely");

    for name in BASIC_ENEMY_SPRITES {
        let file = name.strip_prefix("basicenemy/").unwrap_or_else(|| panic!("every entry must live under basicenemy/ so the overlay resolves it correctly, got {name:?}"));
        let file = format!("{file}.png");
        assert!(
            on_disk.contains(&file),
            "BASIC_ENEMY_SPRITES names {name:?}, so the overlay will request sprites/{name}.png, but the directory holds no file called exactly {file:?}. \
             If the file is there under a different case, this is the Linux-only break: it would render correctly on the Windows dev box and as a red placeholder circle in production"
        );
    }
}

#[test]
fn no_sprite_on_disk_is_missing_from_the_list() {
    anchor_cwd_at_workspace_root();
    let listed: HashSet<String> = BASIC_ENEMY_SPRITES.iter().map(|name| format!("{}.png", name.trim_start_matches("basicenemy/"))).collect();

    for file in png_names_on_disk() {
        assert!(
            listed.contains(&file),
            "{file:?} is in the basic-enemy sprite directory but not in BASIC_ENEMY_SPRITES, so it can never be rolled and no fight will ever show it. \
             The list is generated from the directory - regenerate it: ls *.png | sed 's/\\.png$//' | sort"
        );
    }
}

#[test]
fn the_list_has_no_duplicates() {
    // A duplicate is not a crash, it is a silently weighted roll - the
    // repeated mob would show up twice as often as every other. Cheap to
    // check, invisible if it happens.
    let unique: HashSet<&&str> = BASIC_ENEMY_SPRITES.iter().collect();
    assert_eq!(unique.len(), BASIC_ENEMY_SPRITES.len(), "BASIC_ENEMY_SPRITES contains a duplicate, which would weight that sprite's roll above the rest");
}

#[test]
fn the_overlay_preloads_exactly_this_list() {
    // The overlay carries the same names so it can preload the art up
    // front (see its own BASIC_ENEMY_SPRITES, and BOSS_SPRITES above it
    // doing the same for bosses). Two hand-maintained copies of one list
    // is precisely the drift this project keeps getting bitten by, so the
    // copies are pinned to each other here. A name only in the overlay
    // preloads art nothing rolls; a name only in Rust gets rolled and
    // then pops in late on its first-ever fight.
    anchor_cwd_at_workspace_root();
    let overlay = std::fs::read_to_string("public_adventure_overlay/overlay.html").expect("the overlay must be readable from the workspace root");
    let start = overlay.find("const BASIC_ENEMY_SPRITES = [").expect("overlay.html must still declare BASIC_ENEMY_SPRITES");
    let body = &overlay[start..];
    let body = &body[..body.find("];").expect("that array must be closed")];

    let mut in_overlay: Vec<&str> = Vec::new();
    for piece in body.split('\'').skip(1).step_by(2) {
        in_overlay.push(piece);
    }

    assert_eq!(
        in_overlay, BASIC_ENEMY_SPRITES,
        "overlay.html's BASIC_ENEMY_SPRITES and manager.rs's must be identical - both are generated from the same directory listing"
    );
}
