//! No-op passive overrides: the page must not lie about them, and the save
//! path must stop creating them (2026-09-05).
//!
//! **The defect, in two halves.** `do_save_passive_override` used to insert
//! unconditionally, so opening a row and pressing Save without editing anything
//! wrote an override whose values equal the compiled defaults. Two consequences:
//!
//! 1. **Display.** The page decided the "differs from default" badge and the
//!    class nav's `(n)` count with `PassiveOverrides::has_override` — *"does an
//!    entry exist"* — so that row claimed to differ forever. One boolean was
//!    answering two questions, which is why it survived review: `has_override`
//!    is the *correct* predicate for **Revert** (is there an entry to delete?)
//!    and the wrong one for the badge (did the numbers move?).
//! 2. **Storage (R3).** The entry is invisible while the compiled default is
//!    unchanged, and then silently wins over a rebalance of that default. A
//!    World 2 rebalance would miss exactly the nodes someone had looked at and
//!    left alone.
//!
//! Both are fixed, and this pins both — plus the mutation guard that matters
//! more than either: **a genuine change must still write an entry and still be
//! badged.** A predicate that never fires, or a save path that never writes,
//! would satisfy every "must not" assertion here while telling a different lie.
//!
//! Part B constructs a no-op override **directly in the store** rather than
//! through the save route, because the save route no longer produces one. That
//! is not contrivance: files written by older binaries — and the World 1 data,
//! where at least 5 of 34 entries were bit-identical to their defaults — still
//! contain exactly this, so the display fix remains load-bearing for legacy
//! data even though the save path can no longer create it.
//!
//! Same disposable-instance setup as the other admin HTTP tests: OS-assigned
//! ephemeral port, scratch data dir, nothing reaching the live game.

use game::adventure::AdventureManager;
use std::path::PathBuf;

const ADMIN_LOGIN: &str = "lokati_gaming";

/// Warrior's Bulwark, read off `WARRIOR_NODES` in `passive_tree.rs`:
/// `FlatStat { at_rank_1: 0.08, per_additional_rank: 0.06 }` — compiled ranks
/// 0.08 / 0.14 / 0.20. Saving exactly these is the no-op case.
const NODE: &str = "bulwark";
const DEFAULT_R1: &str = "0.08";
const DEFAULT_R2: &str = "0.14";
const DEFAULT_R3: &str = "0.2";

#[tokio::test]
async fn no_op_overrides_are_neither_written_nor_claimed() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("anchor CWD at workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_passives_no_op_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}}}}"#)).expect("seed sessions");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("server must start");
    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("client");

    let page = |client: reqwest::Client, base: String, class: &'static str| async move {
        client
            .get(format!("{base}/admin/passives?class={class}"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .send()
            .await
            .expect("GET /admin/passives")
            .text()
            .await
            .expect("body")
    };
    let save = |client: reqwest::Client, base: String, r1: &'static str, r2: &'static str, r3: &'static str| async move {
        client
            .post(format!("{base}/admin/passives/save"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .form(&[("class", "warrior"), ("node_key", NODE), ("r1", r1), ("r2", r2), ("r3", r3)])
            .send()
            .await
            .expect("POST failed")
    };

    // --- sanity: a clean instance claims nothing ------------------------
    let before = page(client.clone(), base.clone(), "warrior").await;
    assert!(!before.contains("differs from default"), "sanity: no overrides yet, so nothing may claim to differ");
    assert!(!before.contains("/admin/passives/revert"), "sanity: nothing to revert on a fresh instance");

    // --- PART A (R3): a no-op save writes NOTHING ------------------------
    let saved = save(client.clone(), base.clone(), DEFAULT_R1, DEFAULT_R2, DEFAULT_R3).await;
    assert!(saved.status().is_redirection(), "a no-op save is still a successful save and must redirect, got {}", saved.status());
    assert!(
        game::adventure::passive_override_for(NODE, 1).is_none(),
        "a save equal to the compiled defaults must write NO entry - a stored no-op is invisible until the default is rebalanced, and then silently wins over it"
    );

    let after = page(client.clone(), base.clone(), "warrior").await;
    assert!(!after.contains("differs from default"), "and nothing may claim to differ");
    assert!(!after.contains("passive-state-head\">Modified"), "and no Modified section may appear");
    assert!(!after.contains("/admin/passives/revert"), "and there is nothing to revert, because nothing was stored");

    // --- MUTATION GUARD: a genuine change still writes and still shows ---
    // This is the assertion that stops the two "must not" halves above from
    // being satisfied by a save path that never writes anything at all.
    let changed = save(client.clone(), base.clone(), "0.11", DEFAULT_R2, DEFAULT_R3).await;
    assert!(changed.status().is_redirection(), "a real save must redirect, got {}", changed.status());
    assert!(
        game::adventure::passive_override_for(NODE, 1).is_some(),
        "a GENUINE change must still write an entry - otherwise the no-op fix has broken saving outright while passing every assertion above"
    );
    let moved = page(client.clone(), base.clone(), "warrior").await;
    assert!(moved.contains("differs from default"), "a genuine change must still be badged");
    assert!(moved.contains("passive-state-head\">Modified"), "and must appear under Modified");
    assert!(moved.contains("/admin/passives/revert"), "and must offer Revert");

    // --- and typing the defaults back in CLEARS that entry ---------------
    // Removal rather than "skip the insert": the operator is undoing their own
    // edit, and leaving the previous override standing would ignore the save.
    let undone = save(client.clone(), base.clone(), DEFAULT_R1, DEFAULT_R2, DEFAULT_R3).await;
    assert!(undone.status().is_redirection(), "undoing an edit is a successful save too");
    assert!(
        game::adventure::passive_override_for(NODE, 1).is_none(),
        "typing the defaults back in must CLEAR the existing override, not leave the previous one in place"
    );

    // --- PART B: legacy no-op data must still not be claimed as modified --
    // Written straight into the store, because the save path can no longer
    // create one. Files from older binaries still contain exactly this.
    let mut legacy = game::adventure::passive_overrides();
    legacy.nodes.insert(NODE.to_string(), vec![0.08, 0.14, 0.2]);
    game::adventure::save_passive_overrides(legacy).expect("write legacy no-op override");
    assert!(
        game::adventure::passive_override_for(NODE, 1).is_some(),
        "precondition: the legacy no-op entry really is in the store"
    );

    let legacy_page = page(client.clone(), base.clone(), "warrior").await;
    assert!(
        !legacy_page.contains("differs from default"),
        "a STORED no-op override must not be badged as differing - the badge asks whether the numbers moved, not whether an entry exists"
    );
    assert!(!legacy_page.contains("passive-state-head\">Modified"), "nor filed under Modified");
    assert!(
        legacy_page.contains("/admin/passives/revert"),
        "but Revert MUST be offered - an entry exists and it is exactly the one worth deleting. `has_override` is the right predicate for THIS question"
    );

    // --- the collapsed "Not tunable yet" section actually renders ---------
    // An absent section and a broken section look identical in a rendered page,
    // so this asserts it POSITIVELY on a class that actually has such a node.
    //
    // SEARCHED, not hardcoded (2026-09-07). This named Paladin's
    // `sacredoverflow` until item 14 deleted that node outright, at which point
    // this assertion failed — correctly, and exactly as its own comment had
    // predicted it would. Hardcoding a class here means the test's subject can
    // be removed by an unrelated change in another window, and neither side can
    // see the other coming: the deletion predates this assertion existing, and
    // this branch predates the deletion.
    //
    // Written to hold in BOTH states, the shape
    // `a_not_yet_implemented_node_is_shown_but_not_editable` already uses in
    // adventure_web.rs. The section's own bucket is `not_yet || pending`, so the
    // search matches that predicate rather than only `NotYetImplemented` — a
    // class with a pending-migration node renders the section just the same, and
    // testing a narrower condition than the code uses is how this drifts again.
    //
    // When no such node exists anywhere, the absence is asserted explicitly, so
    // a reader learns the arm is dormant by FACT rather than by silence — and it
    // re-arms itself the moment any node enters either state.
    let untunable_class = game::adventure::ALL_ARCHETYPES.iter().find_map(|&a| {
        a.passive_nodes()
            .iter()
            .find(|n| matches!(n.effect, game::passive_tree::PassiveEffect::NotYetImplemented) || !game::adventure::node_is_tunable(n.key))
            .map(|n| (a, n.key))
    });

    match untunable_class {
        Some((archetype, node_key)) => {
            let slug: &'static str = Box::leak(format!("{archetype:?}").to_lowercase().into_boxed_str());
            let with_untunable = page(client.clone(), base.clone(), slug).await;
            assert!(
                with_untunable.contains("Not tunable yet ("),
                "{archetype:?} has untunable node {node_key}, so its page MUST render the collapsed section - an absent section and a broken one are indistinguishable without this"
            );
            assert!(with_untunable.contains(node_key), "and {node_key} must be listed inside it");
        }
        None => {
            let total: usize = game::adventure::ALL_ARCHETYPES
                .iter()
                .map(|a| {
                    a.passive_nodes()
                        .iter()
                        .filter(|n| matches!(n.effect, game::passive_tree::PassiveEffect::NotYetImplemented) || !game::adventure::node_is_tunable(n.key))
                        .count()
                })
                .sum();
            assert_eq!(total, 0, "sanity: the search found no untunable node, so the count must agree");
        }
    }

    // The empty case must stay empty rather than emitting a zero-count header.
    // Warrior is checked directly because `legacy_page` IS a Warrior page, and
    // Warrior's own untunable count is asserted here rather than assumed.
    let warrior_untunable = game::adventure::Archetype::Warrior
        .passive_nodes()
        .iter()
        .filter(|n| matches!(n.effect, game::passive_tree::PassiveEffect::NotYetImplemented) || !game::adventure::node_is_tunable(n.key))
        .count();
    if warrior_untunable == 0 {
        assert!(
            !legacy_page.contains("Not tunable yet ("),
            "Warrior has no untunable nodes, so its page must NOT render the section"
        );
    }

    std::fs::remove_dir_all(&scratch).ok();
}
