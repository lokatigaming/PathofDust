//! A refused sprite change must say so (2026-09-08).
//!
//! **The defect.** `do_change_model` was `let _ = change_model(..).await`
//! followed by `Redirect::to("/")`. Its own comment argued the updated
//! sprite on the next page load was "confirmation enough" - true only
//! while the action cannot fail. On a refusal the player got a reload,
//! their old sprite, and nothing else: indistinguishable from the game
//! ignoring the click.
//!
//! That is the same silent-failure shape `do_craft` was fixed for, where
//! the doc records the live report in the player's own words - "the game
//! did nothing". So the remedy is the same one rather than a new one:
//! redirect carrying the reason, popup on arrival.
//!
//! **What this test can and cannot reach.** While
//! `MODEL_CHANGES_FREE_FOR_ALL` is `true` nothing is charged, so
//! `InsufficientDust` - the variant that becomes ordinary the day that
//! flag flips back - is not reachable through the route at all.
//! `InvalidChoice` is, and it is a real player path: a stale page, or a
//! custom sprite whose file went away between render and submit. The
//! `InsufficientDust` wording is covered in-crate by
//! `adventure_web::change_model_error_tests`, which does not need the
//! route.
//!
//! This exists as its own `tests/*.rs` for the reason
//! `admin_passives_http.rs` documents: `set_data_dir` is a process-wide
//! `OnceLock`, and Rust gives each integration test file its own
//! process.

use game::adventure::AdventureManager;
use std::path::PathBuf;

const LOGIN: &str = "picky";

#[tokio::test]
async fn a_refused_sprite_change_tells_the_player_why() {
    // Integration tests run with the PACKAGE dir as CWD, but the
    // template loader resolves "templates/" against the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("change_model_failure_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"tok":{{"login":"{LOGIN}","display_name":"Picky","created_at":{now}}}}}"#)).expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");
    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    manager.join(LOGIN, "Picky").await;

    // --- a sprite that does not exist ---------------------------------
    let refused = client
        .post(format!("{base}/change-model"))
        .header(reqwest::header::COOKIE, "adv_session=tok")
        .form(&[("model", "definitely_not_a_real_sprite")])
        .send()
        .await
        .expect("POST failed");
    assert!(refused.status().is_redirection(), "the handler still redirects on refusal, as every other action here does");

    let location = refused.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).expect("a redirect must carry a Location").to_string();
    assert!(
        location.starts_with("/?model_failed="),
        "a REFUSAL must redirect somewhere that can explain itself. Landing on a bare \"/\" is the defect: the player gets their old sprite back and no reason, which reads as the game ignoring them. Got: {location}"
    );

    // The sprite really did not change - so the popup is not papering
    // over a change that silently happened anyway.
    let stored = manager.character(LOGIN).await.expect("joined").model;
    assert_eq!(stored, None, "an invalid sprite must not be stored, popup or no popup");

    // --- and the page the player lands on actually says it ------------
    let page = client
        .get(format!("{base}{location}"))
        .header(reqwest::header::COOKIE, "adv_session=tok")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    assert!(page.contains("Sprite Not Changed"), "the dashboard must render the refusal popup, not just carry the parameter - a param nothing reads is the same silence with extra steps");
    assert!(page.contains("model-error-modal"), "and it must be the model popup specifically");
    assert!(page.contains("Reload and pick again"), "and must carry the REASON text through, not render an empty modal - the whole point is that the player learns what went wrong");

    // --- the success path stays silent, as it always was ---------------
    let ok = client
        .post(format!("{base}/change-model"))
        .header(reqwest::header::COOKIE, "adv_session=tok")
        .form(&[("model", game::adventure::ALL_SPRITES[0])])
        .send()
        .await
        .expect("POST failed");
    let ok_location = ok.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).expect("a redirect must carry a Location").to_string();
    assert_eq!(
        ok_location, "/",
        "SUCCESS must stay silent and land on a clean dashboard. Without this arm the test above would pass just as well if every change were reported as a failure"
    );
    assert_eq!(manager.character(LOGIN).await.expect("joined").model.as_deref(), Some(game::adventure::ALL_SPRITES[0]), "and the sprite must actually have changed");

    let _ = std::fs::remove_dir_all(&scratch);
}
