//! `/admin/passives` over real HTTP (2026-08-19) - Stage 1 of the
//! live-tunable passive values build. Same disposable-instance setup as
//! `http_golden_responses.rs` and `memories_http.rs`: an OS-assigned
//! ephemeral port, a scratch data directory, and seeded fake sessions,
//! so nothing here can reach the live game's files or ports.
//!
//! Covers the three things no in-crate test can:
//!
//! 1. **The admin gate actually holds** on the read and both writes.
//!    This page can change every player's combat numbers, so "a
//!    non-admin gets nothing" has to be proven over real HTTP against
//!    the real session layer, not asserted from reading the handler.
//! 2. **A save round-trips through the real store to the real file**,
//!    and a revert removes it.
//! 3. **An override actually reaches the game**, observed through
//!    `Character::passive_node_magnitude` - the accessor combat itself
//!    uses - rather than only through the admin page that wrote it.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir`
//! is a process-wide `OnceLock`, and the override store is a
//! process-wide `RwLock`. Rust gives each `tests/*.rs` file its own
//! process, so this file is isolated from the rest of the suite, but
//! two `#[tokio::test]`s inside THIS file would share both and race.

use std::path::PathBuf;
use game::adventure::{AdventureManager, Archetype};

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
const OTHER_LOGIN: &str = "someone_else";

#[tokio::test]
async fn admin_passives_gates_writes_and_a_saved_override_reaches_the_game() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_passives_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}},"other-token":{{"login":"{OTHER_LOGIN}","display_name":"SomeoneElse","created_at":{now}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        Some("test-client-id".to_string()),
        Some("test-client-secret".to_string()),
        manager.clone(),
        sessions_path,
        None,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // A real Warrior with a real allocation, so the override's effect is
    // observable through the same accessor combat reads.
    manager.join(ADMIN_LOGIN, "Lokati").await;
    manager.change_archetype(ADMIN_LOGIN, Archetype::Warrior).await.expect("class change must succeed");
    // One rank only - a fresh level-1 character has exactly one passive
    // point (`points_for_level(1)` = 1), so the override this test
    // observes is the RANK 1 value.
    manager.preview_allocate_passive(ADMIN_LOGIN, "bulwark", 1, false).await.expect("allocation must succeed");
    manager.save_passive_tree(ADMIN_LOGIN).await.expect("saving the tree must succeed");

    let baseline = manager.character(ADMIN_LOGIN).await.expect("joined").passive_node_magnitude("bulwark");
    assert!(baseline > 0.0, "sanity: an allocated node must have a non-zero magnitude, got {baseline}");

    // --- the gate ----------------------------------------------------
    // The refusal is a generic "Not Found" card - the BODY hides that a
    // restricted page exists here at all, and that is deliberate. The
    // STATUS must nonetheless be a real 404 (2026-08-31): it used to be
    // 200, so this very assertion passed for the admin too and proved
    // nothing.
    let anon = client.get(format!("{base}/admin/passives")).send().await.expect("GET failed");
    assert_eq!(anon.status(), reqwest::StatusCode::NOT_FOUND, "a logged-out visitor must be refused with a real 404, not a 200 that says Not Found");
    let anon_body = anon.text().await.expect("body");
    assert!(anon_body.contains("Not Found"), "a logged-out visitor must get the generic fallback");
    assert!(!anon_body.contains("Passive Values"), "and must not see the page itself");

    let other = client.get(format!("{base}/admin/passives")).header(reqwest::header::COOKIE, "adv_session=other-token").send().await.expect("GET failed");
    assert_eq!(other.status(), reqwest::StatusCode::NOT_FOUND, "a logged-in NON-admin must be refused with a real 404 too");
    let other_body = other.text().await.expect("body");
    assert!(other_body.contains("Not Found"), "a logged-in NON-admin must get the generic fallback too");
    assert!(!other_body.contains("Passive Values"));

    let admin = client.get(format!("{base}/admin/passives")).header(reqwest::header::COOKIE, "adv_session=admin-token").send().await.expect("GET failed");
    assert_eq!(admin.status(), reqwest::StatusCode::OK, "the admin must get a 200 - this is what makes the 404s above discriminate");
    let admin_body = admin.text().await.expect("body");
    assert!(admin_body.contains("Passive Values"), "the admin must see the page");
    assert!(admin_body.contains("bulwark"), "and its nodes");
    assert!(!admin_body.contains("differs from default"), "nothing is overridden yet");

    // --- a NON-admin write must not take effect ----------------------
    let sneaky = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=other-token")
        .form(&[("class", "warrior"), ("node_key", "bulwark"), ("r1", "0.9"), ("r2", "0.9"), ("r3", "0.9")])
        .send()
        .await
        .expect("POST failed");
    // Ledger #51, fixed 2026-08-31: this used to answer with the same
    // `?saved=1` redirect a real save gets - a refusal reported as a
    // success. It is now the same generic 404 the GET page returns.
    assert_eq!(sneaky.status(), reqwest::StatusCode::NOT_FOUND, "a non-admin write must be refused with a 404, never a redirect claiming it saved");
    assert_eq!(
        manager.character(ADMIN_LOGIN).await.unwrap().passive_node_magnitude("bulwark"),
        baseline,
        "a non-admin POST must not change any value"
    );
    assert!(!scratch.join("adventure-passive-overrides.toml").exists(), "and must not create the overrides file");

    // --- the admin write ---------------------------------------------
    let save = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "bulwark"), ("r1", "0.11"), ("r2", "0.22"), ("r3", "0.33")])
        .send()
        .await
        .expect("POST failed");
    assert!(save.status().is_redirection());

    // It reached the file...
    let overrides_file = scratch.join("adventure-passive-overrides.toml");
    assert!(overrides_file.exists(), "the override must be persisted so it survives a restart");
    let contents = std::fs::read_to_string(&overrides_file).expect("readable");
    assert!(contents.contains("bulwark"), "the file must name the node, got:\n{contents}");

    // ...and it reached the GAME, through the accessor combat uses.
    let tuned = manager.character(ADMIN_LOGIN).await.unwrap().passive_node_magnitude("bulwark");
    assert_eq!(tuned, 0.11, "the character sits at rank 1, so it must now read the rank 1 override");
    assert_ne!(tuned, baseline, "sanity: the override must actually differ");

    // The admin page reflects it, and so does the player-facing tree.
    let admin_body = client
        .get(format!("{base}/admin/passives?class=warrior"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    assert!(admin_body.contains("differs from default"), "the tuned node must be marked");
    assert!(admin_body.contains("/admin/passives/revert"), "and must offer a revert");

    let player_page =
        client.get(format!("{base}/passives")).header(reqwest::header::COOKIE, "adv_session=admin-token").send().await.expect("GET failed").text().await.expect("body");
    // Matched on the MARKUP, not the bare class name: `base.html` now
    // carries a `.passive-tuned` CSS rule, which every rendered page
    // embeds, so a substring check for `passive-tuned` alone would pass
    // whether or not the note was ever emitted.
    assert!(
        player_page.contains("class=\"passive-tuned\""),
        "a retuned node must say so on the player's own tree rather than silently diverging"
    );
    assert!(player_page.contains("Tuned: 0.11"), "and must state the real numbers");

    // --- a bad node key is refused -----------------------------------
    client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "not_a_real_node"), ("r1", "1"), ("r2", "1"), ("r3", "1")])
        .send()
        .await
        .expect("POST failed");
    let contents = std::fs::read_to_string(&overrides_file).expect("readable");
    assert!(!contents.contains("not_a_real_node"), "a key that isn't in the class being edited must never be stored, got:\n{contents}");

    // --- revert -------------------------------------------------------
    let revert = client
        .post(format!("{base}/admin/passives/revert"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "bulwark")])
        .send()
        .await
        .expect("POST failed");
    assert!(revert.status().is_redirection());
    assert_eq!(
        manager.character(ADMIN_LOGIN).await.unwrap().passive_node_magnitude("bulwark"),
        baseline,
        "revert must return the node to its compiled-in value"
    );

    let player_page =
        client.get(format!("{base}/passives")).header(reqwest::header::COOKIE, "adv_session=admin-token").send().await.expect("GET failed").text().await.expect("body");
    assert!(!player_page.contains("class=\"passive-tuned\""), "and the tuned note must disappear with it");

    // --- per-node conversion caps (OverflowConversion rows only) ------
    // Both POSTs below are built from the field set SCRAPED off the
    // rendered page, per the house trap rule - never from a hand-
    // maintained list, so drift in either direction fails here.
    let admin_body = client
        .get(format!("{base}/admin/passives?class=warrior"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    let unbreakable_fields = save_form_field_names(&admin_body, "unbreakable");
    assert!(
        unbreakable_fields.contains(&"conversion_cap".to_string()),
        "an OverflowConversion row must render the per-node cap beside its magnitude, got {unbreakable_fields:?}"
    );
    let bulwark_fields = save_form_field_names(&admin_body, "bulwark");
    assert!(
        !bulwark_fields.contains(&"conversion_cap".to_string()),
        "a non-conversion row must NOT offer the cap, got {bulwark_fields:?}"
    );

    // Saving a cap persists it to the file...
    let cap_save = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[
            ("class", "warrior"),
            ("node_key", "unbreakable"),
            ("r1", "0.5"),
            ("r2", "0.75"),
            ("r3", "1.0"),
            ("conversion_cap", "0.05"),
        ])
        .send()
        .await
        .expect("POST failed");
    assert!(cap_save.status().is_redirection());
    let contents = std::fs::read_to_string(&overrides_file).expect("readable");
    assert!(contents.contains("[conversion_caps]") && contents.contains("unbreakable"), "the cap must land in its own table, got:\n{contents}");

    // ...swap into the live store HOT...
    assert_eq!(
        game::adventure::passive_conversion_cap_override("unbreakable"),
        Some(0.05),
        "the saved cap must be readable through the accessor combat resolves caps through"
    );
    assert_eq!(game::adventure::passive_conversion_cap_override("bulwark"), None, "nodes without an entry fall back to the global");
    // ...and mark the node as retuned on the page.
    let marked = client
        .get(format!("{base}/admin/passives?class=warrior"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    assert!(marked.contains("differs from default"), "a cap-only override must earn the tuned marker");

    // A blank cap field means "follow the global again".
    client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[
            ("class", "warrior"),
            ("node_key", "unbreakable"),
            ("r1", "0.5"),
            ("r2", "0.75"),
            ("r3", "1.0"),
            ("conversion_cap", ""),
        ])
        .send()
        .await
        .expect("POST failed");
    assert_eq!(game::adventure::passive_conversion_cap_override("unbreakable"), None, "blank must clear the per-node cap");


    // --- units and range validation (2026-08-27) ----------------------
    // The page used to render three bare numbers per row with nothing
    // saying what they meant, and accepted any finite value. Payback is
    // the worked example: its threshold is compared against
    // `hp / max_hp`, so `45` meaning "45 percent" is an always-true
    // threshold - it used to persist silently.
    let admin_body = client
        .get(format!("{base}/admin/passives?class=warrior"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");

    // Every rendered row says what its numbers are, or says outright
    // that it has none.
    let mut rows = 0;
    let mut editable_rows = 0;
    // `passive-row-head` shares the prefix, so it is renamed out of the
    // way before splitting - each piece is then exactly one row, ending
    // where the next begins.
    let row_scan = admin_body.replace("<div class=\"passive-row-head\"", "<div class=\"rowhead\"");
    for row in row_scan.split("<div class=\"passive-row").skip(1) {
        rows += 1;
        let declares_no_value = row.contains("declares no value");
        assert!(
            row.contains("class=\"passive-unit\"") || declares_no_value,
            "every rendered row must carry a unit chip (or say it declares no value at all): {row}"
        );
        if row.contains("name=\"r1\"") {
            editable_rows += 1;
            assert!(row.contains("class=\"passive-unit-note\""), "every editable row must print its unit and expected range beside the inputs: {row}");
        }
    }
    assert!(rows > 30 && editable_rows > 30, "sanity: the Warrior page must have rendered real rows, got {rows} rows / {editable_rows} editable");
    assert!(
        admin_body.contains("fraction") && admin_body.contains("a fraction from 0 to 1"),
        "the bounded-fraction rows must state their range in words"
    );

    // The 422 trap, the direction no superset-body test can see: the
    // ordinary edit form must not render `confirm`, and the form struct
    // must not require it.
    let payback_fields = save_form_field_names(&admin_body, "payback");
    assert!(!payback_fields.contains(&"confirm".to_string()), "the ordinary edit form must not render the confirm field, got {payback_fields:?}");
    assert_eq!(payback_fields, vec!["class", "node_key", "r1", "r2", "r3"], "the scraped field set is what a real browser save posts");

    // A value the consuming code cannot use is REFUSED, inline, naming
    // the field and the range - and never written.
    let rejected = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "payback"), ("r1", "0"), ("r2", "45"), ("r3", "0.45")])
        .send()
        .await
        .expect("POST failed");
    assert_eq!(rejected.status(), reqwest::StatusCode::OK, "a rejection re-renders the page rather than redirecting away from it");
    let rejected_body = rejected.text().await.expect("body");
    assert!(rejected_body.contains("Not saved"), "the rejection must be visible on the page: {}", &rejected_body[..rejected_body.len().min(400)]);
    assert!(rejected_body.contains("Rank 2"), "and must name the field");
    assert!(rejected_body.contains("payback"), "and the node");
    assert!(rejected_body.contains("a fraction from 0 to 1"), "and the expected range");
    let contents = std::fs::read_to_string(&overrides_file).expect("readable");
    assert!(!contents.contains("payback"), "a rejected value must NOT be persisted, got:\n{contents}");
    assert_eq!(game::adventure::passive_override_for("payback", 2), None, "and must not reach the live store either");

    // The value they meant saves cleanly - a valid save is still a
    // redirect, not a 422.
    let accepted = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "payback"), ("r1", "0"), ("r2", "0.45"), ("r3", "0.6")])
        .send()
        .await
        .expect("POST failed");
    assert!(accepted.status().is_redirection(), "a valid save must still redirect, got {}", accepted.status());
    assert_eq!(game::adventure::passive_override_for("payback", 2), Some(0.45));

    // A borderline value - Juggernaut's max-HP fraction has no upper
    // bound in the code that reads it, so 45 is a plausible slip rather
    // than an impossible value. It WARNS and waits.
    let warned = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "juggernaut"), ("r1", "45"), ("r2", "0.16"), ("r3", "0.24")])
        .send()
        .await
        .expect("POST failed");
    assert_eq!(warned.status(), reqwest::StatusCode::OK);
    let warned_body = warned.text().await.expect("body");
    assert!(warned_body.contains("Not saved yet"), "a borderline value must warn rather than reject");
    assert!(warned_body.contains("Save anyway"), "and must offer an explicit confirm");
    assert!(warned_body.contains("Rank 1") && warned_body.contains("juggernaut"), "naming the field and node");
    assert_eq!(game::adventure::passive_override_for("juggernaut", 1), None, "nothing may be written before the operator confirms");

    // Confirming persists it - the operator is never blocked from a
    // value the code would genuinely accept.
    let confirmed = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "juggernaut"), ("r1", "45"), ("r2", "0.16"), ("r3", "0.24"), ("confirm", "1")])
        .send()
        .await
        .expect("POST failed");
    assert!(confirmed.status().is_redirection(), "a confirmed save must redirect like any other");
    assert_eq!(game::adventure::passive_override_for("juggernaut", 1), Some(45.0));

    // A count is cast to u32 by its consumer, so a fraction is refused
    // outright rather than silently rounded.
    let fractional_count = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "undyingwill"), ("r1", "0"), ("r2", "1.5"), ("r3", "2")])
        .send()
        .await
        .expect("POST failed");
    assert_eq!(fractional_count.status(), reqwest::StatusCode::OK);
    let fractional_body = fractional_count.text().await.expect("body");
    assert!(fractional_body.contains("must be a whole number"), "a count must refuse a fraction");
    assert_eq!(game::adventure::passive_override_for("undyingwill", 2), None, "and must not persist it");
    std::fs::remove_dir_all(&scratch).ok();
}

/// Names of every input inside the save form whose hidden node_key
/// carries `key` - the rendered-page-derived field set the house trap
/// rule demands a POST be built from.
fn save_form_field_names(page: &str, key: &str) -> Vec<String> {
    let anchor = format!("value=\"{key}\"");
    let row = page.find(&anchor).unwrap_or_else(|| panic!("node {key} must render on the page"));
    let start = page[..row].rfind("<form method=\"post\" action=\"/admin/passives/save\"").unwrap_or_else(|| panic!("node {key}'s save form must precede its hidden key"));
    let end = start + page[start..].find("</form>").expect("the form must close");
    let mut names = Vec::new();
    let mut rest = &page[start..end];
    while let Some(i) = rest.find("name=\"") {
        rest = &rest[i + 6..];
        let close = rest.find('"').expect("a name attribute must close");
        names.push(rest[..close].to_string());
        rest = &rest[close + 1..];
    }
    names.sort();
    names
}
