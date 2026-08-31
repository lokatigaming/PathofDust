//! Optional Twitch credentials (2026-08-31) over real HTTP.
//!
//! `game/src/main.rs` used to abort startup without `TWITCH_CLIENT_ID` and
//! `TWITCH_CLIENT_SECRET`. A standalone game that will not start without a
//! Twitch app is wrong - the Linux staging instance had to be given
//! placeholder values just to boot. Absent now means the Twitch login path
//! is simply not registered, the same `None`-disables-the-mount shape
//! `ADVENTURE_API_SECRET` uses for the `/api` nest.
//!
//! Two servers in ONE process, deliberately - the whole claim is a
//! comparison, and asserting it across two test binaries would let the
//! two halves drift:
//!
//! * **absent** - the process starts, `/login` and `/auth/callback` are
//!   404 (never mounted), the logged-out page does not offer a Twitch
//!   link, and local accounts still work.
//! * **present** - byte-for-byte what shipped before: `/login` redirects
//!   to `id.twitch.tv` carrying the configured `client_id`, and the link
//!   is offered. This is the half that protects the live Windows game,
//!   which has the credentials and must not change.
//!
//! Nothing is DELETED here. This is optionality, not removal - removal is
//! Stage 3b's job.

use std::path::PathBuf;
use std::sync::Arc;

use game::adventure::AdventureManager;

const CLIENT_ID: &str = "test-client-id";

async fn serve(twitch: bool, manager: Arc<AdventureManager>, sessions_path: PathBuf) -> String {
    let (id, secret) = if twitch { (Some(CLIENT_ID.to_string()), Some("test-client-secret".to_string())) } else { (None, None) };
    let bound = game::adventure_web::start_adventure_web_server(0, "http://localhost".to_string(), id, secret, manager, sessions_path, None)
        .await
        .expect("the server must start whether or not Twitch is configured - that is the entire point");
    format!("http://127.0.0.1:{}", bound.port())
}

#[tokio::test]
async fn twitch_credentials_are_optional_and_change_nothing_when_present() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("twitch_optional_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, "{}").expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // --- absent: the process starts and the Twitch path is gone --------
    let base = serve(false, manager.clone(), sessions_path.clone()).await;

    for path in ["/login", "/auth/callback"] {
        let resp = client.get(format!("{base}{path}")).send().await.expect("GET failed");
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND, "{path} must not be mounted without the Twitch credentials, got {}", resp.status());
    }

    let home = client.get(format!("{base}/")).send().await.expect("GET failed");
    assert_eq!(home.status(), reqwest::StatusCode::OK, "the dashboard must still serve");
    let home_body = home.text().await.expect("body");
    assert!(!home_body.contains("Login with Twitch"), "a link to an unmounted route must not be offered");
    assert!(!home_body.contains("href=\"/login\""), "and nothing else may link to it either");

    // Local identity is the whole point of still being up.
    let local = client.get(format!("{base}/account/login")).send().await.expect("GET failed");
    assert_eq!(local.status(), reqwest::StatusCode::OK, "local accounts must work without Twitch");
    assert!(home_body.contains("/account/register"), "and must still be offered on the logged-out page");

    // --- present: byte-identical to what shipped before -----------------
    let base = serve(true, manager.clone(), sessions_path.clone()).await;

    let login = client.get(format!("{base}/login")).send().await.expect("GET failed");
    assert_eq!(login.status(), reqwest::StatusCode::SEE_OTHER, "with the credentials present /login must still redirect exactly as it always did");
    let location = login.headers().get(reqwest::header::LOCATION).expect("a redirect must carry a Location").to_str().expect("ascii location");
    assert!(location.starts_with("https://id.twitch.tv/oauth2/authorize"), "the redirect target must be unchanged, got {location}");
    assert!(location.contains(&format!("client_id={CLIENT_ID}")), "the configured client_id must still be sent, got {location}");

    let home = client.get(format!("{base}/")).send().await.expect("GET failed");
    let home_body = home.text().await.expect("body");
    assert!(home_body.contains("Login with Twitch"), "the link must still be offered when the route exists");

    // `/auth/callback` is mounted: it answers (with its own failure card
    // for a call carrying no code), rather than 404ing as an unmounted
    // route does.
    let callback = client.get(format!("{base}/auth/callback")).send().await.expect("GET failed");
    assert_ne!(callback.status(), reqwest::StatusCode::NOT_FOUND, "/auth/callback must be mounted when the credentials are present");

    let _ = std::fs::remove_dir_all(&scratch);
}
