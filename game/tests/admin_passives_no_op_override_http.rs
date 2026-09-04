//! A save that changes nothing must not claim the node was changed
//! (2026-09-05).
//!
//! `do_save_passive_override` inserts unconditionally, so saving a row without
//! editing it writes an override whose values equal the compiled defaults. The
//! page then read `PassiveOverrides::has_override` — *"does an entry exist"* —
//! to decide the **"differs from default"** badge and the class nav's `(n)`
//! count, so that row claimed to differ forever and the count was inflated by
//! every no-op save.
//!
//! One boolean was answering two questions, which is why it survived review:
//! `has_override` is the *correct* answer for **Revert** (is there an entry to
//! delete?) and the *wrong* answer for the badge (did the numbers move?).
//!
//! This pins both halves at once, because fixing the badge by making Revert
//! disappear would be a worse bug than the one being fixed — the no-op override
//! is precisely the entry that most needs deleting.
//!
//! **Note what this does NOT test.** The no-op override is still written to
//! `adventure-passive-overrides.toml`; only the page's claim about it is now
//! honest. A node pinned at its default still silently escapes a rebalance of
//! its compiled value. That is R3, a separate change.
//!
//! Same disposable-instance setup as the other admin HTTP tests: OS-assigned
//! ephemeral port, scratch data dir, nothing reaching the live game.

use game::adventure::AdventureManager;
use std::path::PathBuf;

const ADMIN_LOGIN: &str = "lokati_gaming";

/// Warrior's Bulwark, read off `WARRIOR_NODES` in `passive_tree.rs`:
/// `FlatStat { at_rank_1: 0.08, per_additional_rank: 0.06 }` — so the compiled
/// ranks are 0.08 / 0.14 / 0.20. Saving exactly these is the no-op case.
const NODE: &str = "bulwark";
const DEFAULT_R1: &str = "0.08";
const DEFAULT_R2: &str = "0.14";
const DEFAULT_R3: &str = "0.2";

#[tokio::test]
async fn a_save_that_changes_nothing_does_not_claim_the_node_differs() {
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

    let page = |client: reqwest::Client, base: String| async move {
        client
            .get(format!("{base}/admin/passives?class=warrior"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .send()
            .await
            .expect("GET /admin/passives")
            .text()
            .await
            .expect("body")
    };

    // --- before: a clean instance claims nothing differs ----------------
    let before = page(client.clone(), base.clone()).await;
    assert!(
        !before.contains("differs from default"),
        "sanity: a fresh instance has no overrides, so nothing may claim to differ"
    );
    assert!(!before.contains("/admin/passives/revert"), "sanity: nothing to revert on a fresh instance");

    // --- the no-op save: exactly the compiled defaults -------------------
    let saved = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", NODE), ("r1", DEFAULT_R1), ("r2", DEFAULT_R2), ("r3", DEFAULT_R3)])
        .send()
        .await
        .expect("POST failed");
    assert!(saved.status().is_redirection(), "a valid save must redirect, got {}", saved.status());

    // The entry really was written - this is the defect's precondition, not
    // an incidental detail. If this ever stops being true (R3), this test's
    // premise is gone and it should be revisited rather than deleted.
    assert!(
        game::adventure::passive_override_for(NODE, 1).is_some(),
        "precondition: the save writes an override entry even though nothing changed"
    );

    let after = page(client.clone(), base.clone()).await;

    // --- the badge must NOT claim a difference ---------------------------
    assert!(
        !after.contains("differs from default"),
        "a save equal to the compiled defaults must not brand the node as differing - the badge asks whether the NUMBERS moved, not whether an entry exists"
    );

    // --- the state section must not file it under Modified ---------------
    assert!(
        !after.contains("passive-state-head\">Modified"),
        "a no-op override must not put the row in the Modified section - the section is the working set, and filling it with unchanged nodes is the same lie as the badge"
    );

    // --- but Revert must still be offered --------------------------------
    assert!(
        after.contains("/admin/passives/revert"),
        "Revert must still be offered: an entry exists and it is exactly the one worth deleting. `has_override` is the right predicate for THIS question"
    );
    assert!(after.contains(NODE), "the node itself must still render");

    // --- and a real change must still register ---------------------------
    let changed = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", NODE), ("r1", "0.11"), ("r2", DEFAULT_R2), ("r3", DEFAULT_R3)])
        .send()
        .await
        .expect("POST failed");
    assert!(changed.status().is_redirection(), "a valid save must redirect, got {}", changed.status());

    let moved = page(client, base).await;
    assert!(
        moved.contains("differs from default"),
        "a genuine change MUST still be badged - a predicate that never fires would pass every assertion above while telling a different lie"
    );
    assert!(moved.contains("passive-state-head\">Modified"), "and it must appear under Modified");

    std::fs::remove_dir_all(&scratch).ok();
}
