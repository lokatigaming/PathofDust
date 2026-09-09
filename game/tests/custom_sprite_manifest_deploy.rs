//! The manifest must be where the RUNNING GAME looks, not only where the
//! checkout has it (2026-09-09).
//!
//! **The defect this exists for, in the words of the deploy that caught
//! it.** The manifest was read from a bare relative path, so it resolved
//! against the service's working directory - `/var/lib/pathofdust`. The
//! sprites are there. The manifest was not: `deploy-linux.sh` copies the
//! binary, `owners.toml` is git-tracked in the checkout, and nothing
//! carried it across. Missing file means an empty owner map, which
//! rejects every custom sprite - so the release shipped the enforcement
//! without the data, and the two players wearing a custom sprite
//! (`kibukah`, `sitch89`) would have lost it. Fail-safe by design, and
//! still the exact inverse of the feature.
//!
//! **Why the existing tests could not catch it, and this one can.**
//! `custom_sprite_manifest.rs` and `custom_sprite_case.rs` anchor their
//! CWD at the workspace root, where the checkout's copy sits exactly
//! where the path resolves. They check that the manifest and the sprite
//! directory AGREE - a real property, and one that is true on the box
//! too. What they cannot express is the box's actual condition: sprites
//! present, manifest absent. This file makes that condition
//! representable by pointing the resolver at a directory that has the
//! one and not the other.
//!
//! Its own process, because `set_data_dir` is a process-wide `OnceLock`
//! and this file is the only caller in it - the same reason
//! `admin_passives_http.rs` is one file with one test.

use game::adventure::{custom_sprite_is_owned_by, custom_sprite_manifest_path, is_valid_custom_sprite, CUSTOM_SPRITE_DIR};

#[test]
fn a_manifest_missing_from_the_data_directory_is_visible_here_and_recoverable_in_place() {
    let scratch = std::env::temp_dir().join(format!("custom_sprite_manifest_deploy_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let custom = scratch.join(CUSTOM_SPRITE_DIR);
    std::fs::create_dir_all(&custom).expect("scratch sprite dir must be creatable");

    // The box's shape: the sprite file present, the manifest absent.
    std::fs::write(custom.join("kibukah.png"), b"not really a png").expect("sprite file must be writable");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this file is the only caller in its process");
    assert_eq!(custom_sprite_manifest_path(), custom.join("owners.toml"), "the manifest must resolve BESIDE the sprites, inside the configured data directory - not against the checkout");

    // --- the deployed-without-the-manifest condition -------------------
    assert!(!custom_sprite_manifest_path().exists(), "sanity: this arm is only meaningful while the manifest is genuinely absent");
    assert!(
        !custom_sprite_is_owned_by("kibukah", "kibukah"),
        "with no manifest at the resolved path, NOBODY owns anything - this is the condition the deploy hit, and the reason the release was held: the sprite file is right there and its owner still cannot equip it"
    );
    assert!(!is_valid_custom_sprite("kibukah", "custom/kibukah"), "and the full validation path refuses it too, which is what the player would actually have experienced");

    // --- the same directory, one file later ---------------------------
    // Written in place rather than restarting anything, because the
    // manifest is read per call: the fix for a box in this state is to
    // put the file there, and it takes effect immediately. That is the
    // property the operator note in `docs/custom_sprites.md` promises.
    std::fs::write(custom.join("owners.toml"), "[sprites]\n\"kibukah\" = \"kibukah\"\n").expect("manifest must be writable");
    assert!(
        custom_sprite_is_owned_by("kibukah", "kibukah"),
        "dropping the manifest into the data directory must fix it with no restart - if this fails the absence arm above proves nothing, because it would be failing for some other reason"
    );
    assert!(is_valid_custom_sprite("kibukah", "custom/kibukah"), "and the sprite becomes equippable, which is the whole feature");

    // And ownership is still gated once the manifest is there - the
    // recovery must not be "everyone can equip everything".
    assert!(!custom_sprite_is_owned_by("someone_else", "kibukah"), "restoring the manifest must restore the GATE, not just access");

    let _ = std::fs::remove_dir_all(&scratch);
}
