//! `/admin/ops/next-encounter` over real HTTP (2026-08-28) - the web
//! operator control added in Stage 2 of the standalone plan, the
//! equivalent of the bot's mod-only `!nextencounter`.
//!
//! Everything this file asserts is about the things a Rust-level call
//! into `operator_trigger_encounter` could not see: the admin gate on a
//! real cookie, the `axum::Form` extraction of the boss select, the
//! status code and body each refusal actually produces, and - the point
//! of the control's whole guarding design - that a second press while
//! the first is still running is REFUSED rather than queued.
//!
//! Same disposable-instance setup as `admin_tunables_splash_http.rs`: an
//! OS-assigned ephemeral port and a scratch data directory, so nothing
//! here can reach the live game's files or ports.

use game::adventure::AdventureManager;
use std::path::PathBuf;
use std::sync::Arc;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
const OTHER_LOGIN: &str = "someone_else";

/// The boss the select is asked to force, and how `WorldState` persists
/// it. Reading the world file back is how "did the force actually take"
/// is answered with what the fight USED rather than what the form asked
/// for.
const FORCED_BOSS: &str = "cthulhu";
const FORCED_BOSS_SERIALIZED: &str = "\"last_boss_kind\":\"Cthulhu\"";

#[tokio::test]
async fn admin_ops_next_encounter_gates_reports_and_refuses_a_double_press() {
    // Integration tests run with their PACKAGE dir as CWD; the template
    // loader resolves "templates/" against CWD and that directory belongs
    // to the workspace root. Same anchor every other *_http.rs test uses.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_ops_next_encounter_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
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
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        manager.clone(),
        sessions_path,
        None,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let url = format!("{base}/admin/ops/next-encounter");
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    let world_path = scratch.join("adventure-world.json");

    let post = |token: Option<&'static str>, boss: &'static str| {
        let client = client.clone();
        let url = url.clone();
        async move {
            let mut req = client.post(&url).form(&[("boss", boss)]);
            if let Some(token) = token {
                req = req.header(reqwest::header::COOKIE, format!("adv_session={token}"));
            }
            let response = req.send().await.expect("POST /admin/ops/next-encounter failed");
            (response.status(), response.text().await.expect("body must read"))
        }
    };

    // --- refused without admin authentication, VISIBLY -----------------
    // The existing admin POSTs (`/admin/tunables/save`,
    // `/admin/passives/save`) answer a non-admin with a bare redirect and
    // no status - indistinguishable from success. This control must not:
    // it fires a fight against a live party, so a silent no-op reads as
    // "the button is broken" at exactly the wrong moment.
    for token in [None, Some("other-token"), Some("nonsense-token")] {
        let (status, body) = post(token, "").await;
        assert_eq!(status, reqwest::StatusCode::FORBIDDEN, "a non-admin POST must be refused with 403, not silently redirected ({token:?})");
        assert!(body.contains("Refused - not the operator"), "the refusal must NAME the condition, not just fail ({token:?}) - got: {body}");
        assert!(body.contains("Nothing was triggered"), "the refusal must say plainly that no fight happened ({token:?})");
    }

    // Nothing above may have run a fight. Nobody has joined yet either,
    // but proving the gate held means proving the world never moved.
    assert!(!world_path.exists(), "a refused POST must not have reached run_encounter at all");

    // --- admin auth, but nobody eligible -------------------------------
    // Still a refusal, and still a NAMED one: "nothing happened" and
    // "nothing could happen" are different answers and the operator has
    // to be able to tell them apart.
    let (status, body) = post(Some("admin-token"), "").await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "an admin POST with an empty battlefield must report the refusal, not 200");
    assert!(body.contains("Refused - nobody is eligible to fight"), "got: {body}");

    // --- a boss name the select never offers ---------------------------
    let (status, body) = post(Some("admin-token"), "not-a-boss").await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "an unrecognized boss must be a 400, not a silent random fight");
    assert!(body.contains("Refused - unrecognized boss"), "got: {body}");

    // --- performs its action with admin authentication -----------------
    manager.join("tester", "Tester").await;
    let (status, body) = post(Some("admin-token"), FORCED_BOSS).await;
    assert_eq!(status, reqwest::StatusCode::OK, "an admin POST with a joined character must actually trigger - got: {body}");
    assert!(body.contains("Encounter triggered"), "success must be reported as plainly as a refusal - got: {body}");

    // --- the boss select forces the named boss -------------------------
    // `run_encounter` writes the boss it picked into the persisted world
    // file (see `WorldState::last_boss_kind`), so this reads back what
    // the fight ACTUALLY used, not what the form claimed to ask for.
    // The world file is pretty-printed, so whitespace is stripped
    // before matching rather than pinning this test to its layout.
    let bare = |json: &str| json.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let world = bare(&std::fs::read_to_string(&world_path).expect("a fight must have persisted the world file"));
    assert!(world.contains(FORCED_BOSS_SERIALIZED), "the select must force the boss it names - POSTed {FORCED_BOSS}, world file says: {world}");
    assert_eq!(world.matches("last_boss_kind").count(), 1, "sanity: the world file holds exactly one last_boss_kind, so reading it back identifies the LAST fight");

    // --- double submission is refused and queues nothing ---------------
    // Two presses fired concurrently, which is what an impatient operator
    // produces. `run_encounter`'s own `fight_gate` would happily
    // serialize these into TWO fights back to back; `operator_action_gate`
    // is the entire reason the second is refused instead. The assertion
    // that matters is not the status - it is that the boss the second
    // press asked for never fought.
    let (first, second) = tokio::join!(post(Some("admin-token"), "lich"), post(Some("admin-token"), "purple"));
    let mut outcomes = [first, second];
    // Which press wins the gate is a genuine race, so sort by status
    // (200 before 409) rather than assuming an order.
    outcomes.sort_by_key(|(status, _)| status.as_u16());
    let (winner_status, _) = &outcomes[0];
    let (loser_status, loser_body) = &outcomes[1];
    assert_eq!(*loser_status, reqwest::StatusCode::CONFLICT, "the second concurrent press must be refused, not queued behind the first");
    assert!(
        loser_body.contains("Refused - operator action already running") || loser_body.contains("Refused - a fight is in progress"),
        "the refusal must name which guard stopped it - got: {loser_body}"
    );
    assert!(loser_body.contains("Nothing was queued"), "the operator must be told explicitly that no delayed fight is now pending - got: {loser_body}");
    assert!(
        *winner_status == reqwest::StatusCode::OK || *winner_status == reqwest::StatusCode::CONFLICT,
        "the first press either ran or was itself refused - never anything else, got {winner_status}"
    );
    // At most ONE of the two bosses may have fought. Both appearing is
    // impossible with a single `last_boss_kind`, so the real check is
    // that the world settled on one of them (or neither, if the winner
    // was itself refused) and then stopped moving.
    let after_race = bare(&std::fs::read_to_string(&world_path).expect("world file must exist"));
    let settled = |w: &str| (w.contains("\"last_boss_kind\":\"Lich\"") as u8) + (w.contains("\"last_boss_kind\":\"Dragon\"") as u8);
    assert!(settled(&after_race) <= 1, "only one of the two concurrent presses may have run a fight - world file says: {after_race}");
    // And no fight is pending: a moment later the world must be
    // byte-identical. A queued second fight would land here.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let later = bare(&std::fs::read_to_string(&world_path).expect("world file must exist"));
    assert_eq!(after_race, later, "a refused press must not have queued a fight that lands afterwards");

    // --- a press while a fight is in progress is refused ---------------
    // Driven through the BOT's own entry point, so this also proves the
    // two paths see each other: `fight_in_progress` reads the same
    // `fight_gate` a bot-triggered fight holds.
    let manager_for_fight: Arc<AdventureManager> = manager.clone();
    let holding = tokio::spawn(async move {
        manager_for_fight.trigger_encounter_now(Some("bahamut")).await;
    });
    // `fight_in_progress` reads the LOCK, not a timestamp, so pressing
    // before the spawned fight has actually taken it would test nothing.
    let mut refusal = None;
    for _ in 0..100 {
        if manager.fight_in_progress().await {
            refusal = Some(post(Some("admin-token"), "").await);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let (status, body) = refusal.expect("the spawned bot-path fight must hold fight_gate long enough to press against");
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "a press during a live fight must be refused - got: {body}");
    assert!(
        body.contains("Refused - a fight is in progress") || body.contains("Refused - operator action already running"),
        "a press during a live fight must name the condition - got: {body}"
    );
    assert!(body.contains("Nothing was queued"), "got: {body}");
    holding.await.expect("the spawned fight must not panic");

    let _ = std::fs::remove_dir_all(&scratch);
}
