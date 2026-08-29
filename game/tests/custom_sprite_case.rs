//! Linux-readiness (2026-08-29) - `is_valid_custom_sprite` must give the
//! same answer on a case-insensitive and a case-sensitive filesystem.
//!
//! It did not. The ownership gate lowercased (`custom_sprite_is_owned_by`)
//! but the file probe was a bare `dir.join("{name}.png").exists()`, which
//! silently inherited the host's case rules: on NTFS a stored
//! `custom/Sitch89` matched the on-disk `Sitch89.gif` whatever case it was
//! written in, and on ext4 only a byte-exact match would - so a character
//! whose model was already stored in one case would have fallen back to
//! its hash-default sprite the moment production moved to Linux. The probe
//! now resolves by listing the directory and comparing without case.
//!
//! Asserted against the REAL drop-in directory rather than a fixture,
//! because that directory is exactly what varies per deployment and a
//! fixture would prove the fix only against itself. That means anchoring
//! this test binary's CWD at the workspace root, the same thing
//! `http_golden_responses.rs` does and for the same reason: `cargo test`'s
//! CWD is the PACKAGE root (`game/`), while `CUSTOM_SPRITE_DIR` is a bare
//! relative literal resolved against the workspace root in production.
//! This is its own test binary specifically so that CWD change cannot
//! affect anything else.
//!
//! The uppercase axis is the one that matters and it needs no mixed-case
//! file on disk: for a lowercase `kibukah.png`, `custom/KIBUKAH` and
//! `custom/kibukah` name the same file, and before this fix they disagreed
//! on Linux.

use game::adventure::{custom_sprite_is_owned_by, is_valid_custom_sprite, CUSTOM_SPRITE_DIR};

fn anchor_cwd_at_workspace_root() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
}

/// Every real `.png`/`.gif` in the drop-in directory, as `(stem, owner)`.
fn sprites_on_disk() -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(CUSTOM_SPRITE_DIR) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_sprite = path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("gif"));
            let stem = path.file_stem().and_then(|stem| stem.to_str())?.to_string();
            // `owner_id` is always the lowercased character key, so the
            // ownership gate is held constant and only the MODEL's case
            // varies - precisely the axis the old probe was blind to.
            is_sprite.then(|| (stem.to_ascii_lowercase(), stem))
        })
        .map(|(owner, stem)| (stem, owner))
        .collect()
}

#[test]
fn every_case_variant_of_a_real_sprite_validates_identically() {
    anchor_cwd_at_workspace_root();
    let sprites = sprites_on_disk();
    assert!(!sprites.is_empty(), "the custom sprite directory must hold at least one .png/.gif for this test to mean anything - looked in {CUSTOM_SPRITE_DIR}");

    for (stem, owner) in sprites {
        assert!(is_valid_custom_sprite(&owner, &format!("custom/{stem}")), "{stem} exists on disk and must validate as stored");
        for variant in [stem.to_ascii_uppercase(), stem.to_ascii_lowercase(), stem.clone()] {
            assert!(
                is_valid_custom_sprite(&owner, &format!("custom/{variant}")),
                "custom/{variant} and custom/{stem} name the same file - they must validate identically on every filesystem"
            );
        }
    }
}

/// The reject side of the same coin. Case-insensitive lookup must not have
/// become a wildcard: a name with no file behind it is refused in every
/// case, and the ownership gate still refuses someone else's sprite.
#[test]
fn a_name_with_no_file_is_rejected_in_every_case() {
    anchor_cwd_at_workspace_root();
    for model in ["custom/nosuchsprite", "custom/NoSuchSprite", "custom/NOSUCHSPRITE"] {
        assert!(!is_valid_custom_sprite("nosuchsprite", model), "{model} has no file behind it and must be rejected");
    }
}

/// The two halves that used to disagree, asserted side by side so they
/// cannot drift apart again: the ownership gate has always ignored case,
/// and now the file probe does too.
#[test]
fn the_ownership_gate_ignores_case_but_still_gates() {
    assert!(custom_sprite_is_owned_by("sitch89", "Sitch89"), "an owner must match their own sprite whatever case it is stored in");
    assert!(custom_sprite_is_owned_by("sitch89", "sitch89"));
    assert!(!custom_sprite_is_owned_by("sitch89", "Kibukah"), "ignoring case must not turn into matching someone else's sprite");
    assert!(!custom_sprite_is_owned_by("sitch89", "KIBUKAH"));
}

/// Path-escape rejections are unchanged by the case fix - listing the
/// directory must not have made a traversal reachable that probing was not.
#[test]
fn escapes_are_still_rejected() {
    anchor_cwd_at_workspace_root();
    for model in ["custom/../kibukah", "custom/sub/kibukah", "custom/sub\\kibukah", "custom/", "kibukah"] {
        assert!(!is_valid_custom_sprite("kibukah", model), "{model} must be rejected");
    }
}
