//! Bug reports end to end (2026-09-02) - the player files one over real
//! HTTP, it lands in the file, and the operator can read it back.
//!
//! Same shape and same reasoning as `divinity_ui_http.rs`: the mechanic
//! can be entirely correct and still be unreachable from a browser, and
//! only a real GET of the real page plus a real POST through the real
//! `Form` extractor can see that gap. The entry point matters as much as
//! the store here - a report form nothing links to is a feature nobody
//! files a report with.
//!
//! One `#[tokio::test]`, deliberately - `adventure::set_data_dir` is a
//! process-wide `OnceLock`, so a second test function in this file would
//! race this one for who calls it first.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, BugReport, Character};

#[tokio::test]
async fn a_player_can_file_a_bug_report_and_the_operator_can_read_it() {
    // Integration tests run with their PACKAGE dir as CWD, but the
    // template loader resolves "templates/" against CWD and that
    // directory belongs to the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("bug_reports_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "bug-reporter";
    const OPERATOR: &str = "bug-operator";

    // `ADMIN_TUNABLES_LOGIN` is a `LazyLock` over this variable, so it has
    // to be set before the first read - i.e. before the server starts.
    // Same reasoning `operator_bootstrap_http.rs` spells out. Pointed at a
    // login that is NOT the reporter, so the refusal and the read-back are
    // both real rather than the same session twice.
    std::env::set_var("OPERATOR_LOGIN", OPERATOR);

    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"BugReporter","created_at":{now_secs}}},"op-token":{{"login":"{OPERATOR}","display_name":"BugOperator","created_at":{now_secs}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    // Must run before anything that could touch `data_path` - even
    // constructing a `Character` reaches it transitively via item
    // generation. See `divine_dust_ui_http.rs` for the full note. It is
    // also what puts `adventure-bugreports.json` in the scratch dir
    // rather than beside production's.
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), Character::new("BugReporter".to_string()));
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path, PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path)
        .await
        .expect("disposable adventure_web server must start");
    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    let cookie = "adv_session=test-token";

    // --- Step 1: a player can FIND the form. An unlinked page is not a
    // feature, and the nav is the only entry point. ---
    let resp = client.get(format!("{base}/")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET / failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let dashboard = resp.text().await.expect("failed to read dashboard body");
    assert!(dashboard.contains("href=\"/bugs\""), "top_nav must link to the bug report page - it is the only way a player reaches it:\n{dashboard}");

    // --- Step 2: the form renders, with a field the handler can read. ---
    let resp = client.get(format!("{base}/bugs")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /bugs failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.expect("failed to read /bugs body");
    assert!(body.contains("action=\"/bugs\""), "the page must post back to /bugs:\n{body}");
    assert!(body.contains("name=\"text\""), "the report field must be named `text`, which is what BugReportForm reads");

    // --- Step 3: POST exactly the fields the page renders, derived from
    // the rendered form rather than a hand-written list (the drift guard
    // `admin_tunables_splash_http.rs` established - a hand-maintained
    // body can never catch a field the page stopped rendering). ---
    let form_html = {
        let start = body.find("<form method=\"post\" action=\"/bugs\"").expect("the report form must be on the page");
        let end = start + body[start..].find("</form>").expect("the report form must be closed");
        &body[start..end]
    };
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert_eq!(rendered, vec!["text"], "the form's field set must be exactly what BugReportForm expects, got {rendered:?}");

    const REPORT: &str = "Krangle confirmed nothing & then locked <b>my</b> sword";
    let exact: Vec<(&str, &str)> = rendered.iter().map(|name| (*name, REPORT)).collect();
    let resp = client.post(format!("{base}/bugs")).header(reqwest::header::COOKIE, cookie).form(&exact).send().await.expect("POST /bugs failed at the transport level");
    assert_ne!(
        resp.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "posting exactly the fields the form renders must extract cleanly - a 422 means BugReportForm and the page disagree"
    );
    assert!(resp.status().is_redirection(), "a filed report must redirect (POST/redirect/GET, so a refresh cannot re-file it), got {}", resp.status());
    let location = resp.headers().get(reqwest::header::LOCATION).expect("a redirect must carry a Location").to_str().expect("Location must be ASCII").to_string();
    assert_eq!(location, "/bugs?filed=1", "the redirect must carry the new report's id so the page can name it, got {location}");

    // --- Step 4: the confirmation is VISIBLE, not just in the URL. ---
    let resp = client.get(format!("{base}{location}")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET of the redirect target failed");
    let confirmed = resp.text().await.expect("failed to read confirmation body");
    assert!(confirmed.contains("Report #1 received"), "the player must see that the report landed, and its number:\n{confirmed}");

    // --- Step 5: it really is on disk, under the reporter's login rather
    // than anything they typed. ---
    let raw = std::fs::read_to_string(scratch.join("adventure-bugreports.json")).expect("the report file must exist after a successful submission");
    let saved: Vec<BugReport> = serde_json::from_str(&raw).expect("the report file must parse");
    assert_eq!(saved.len(), 1, "exactly one report must have been recorded");
    assert_eq!(saved[0].id, 1);
    assert_eq!(saved[0].user, TEST_LOGIN, "the reporter comes from the session, never from the form - it cannot be spoofed");
    assert_eq!(saved[0].text, REPORT, "the report is stored verbatim; escaping happens at render time, not on the way in");
    assert!(saved[0].at_unix_secs > 0, "a report must be timestamped");

    // --- Step 6: the cooldown is real. The second submission inside the
    // window must refuse and must NOT write. ---
    let resp = client.post(format!("{base}/bugs")).header(reqwest::header::COOKIE, cookie).form(&exact).send().await.expect("second POST /bugs failed");
    let location = resp.headers().get(reqwest::header::LOCATION).expect("a redirect must carry a Location").to_str().expect("Location must be ASCII").to_string();
    assert!(location.starts_with("/bugs?error=cooldown"), "a second report inside PER_USER_COOLDOWN must be refused, got {location}");
    let raw = std::fs::read_to_string(scratch.join("adventure-bugreports.json")).expect("the report file must still exist");
    let saved: Vec<BugReport> = serde_json::from_str(&raw).expect("the report file must parse");
    assert_eq!(saved.len(), 1, "a refused submission must not be written");

    // --- Step 7: not logged in means no report and no anonymous flood. ---
    let resp = client.post(format!("{base}/bugs")).form(&exact).send().await.expect("logged-out POST /bugs failed");
    assert!(resp.status().is_redirection(), "a logged-out submission must be bounced to login, got {}", resp.status());
    let location = resp.headers().get(reqwest::header::LOCATION).expect("a redirect must carry a Location").to_str().expect("Location must be ASCII").to_string();
    assert_eq!(location, "/account/login", "a logged-out submission goes to login, got {location}");
    let saved: Vec<BugReport> = serde_json::from_str(&std::fs::read_to_string(scratch.join("adventure-bugreports.json")).expect("file")).expect("parse");
    assert_eq!(saved.len(), 1, "a logged-out submission must not be written");

    // --- Step 8: /admin/bugs is operator-only, and says so rather than
    // rendering an empty page a non-operator would misread as "no bugs". ---
    let resp = client.get(format!("{base}/admin/bugs")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /admin/bugs failed");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN, "a non-operator must be refused with a status that says so, not shown an empty list");
    let refused = resp.text().await.expect("failed to read refusal body");
    assert!(refused.contains("not the operator"), "the refusal must name the reason:\n{refused}");
    assert!(!refused.contains("Krangle confirmed nothing"), "a refused request must not leak report contents");

    // --- Step 9: the operator CAN read it back. This is the half that
    // makes the feature worth anything - a report nobody can read is a
    // write-only file. ---
    let resp = client.get(format!("{base}/admin/bugs")).header(reqwest::header::COOKIE, "adv_session=op-token").send().await.expect("operator GET /admin/bugs failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK, "the operator must be able to open the page");
    let listed = resp.text().await.expect("failed to read /admin/bugs body");
    assert!(listed.contains("Krangle confirmed nothing"), "the operator must see the report text:\n{listed}");
    assert!(listed.contains(TEST_LOGIN), "the operator must see who filed it");
    assert!(listed.contains("#1"), "the operator must see the report's number, which is what the player was told");
    // The report is stored verbatim and escaped at render time - a player
    // must not be able to put markup on the operator's own page.
    assert!(!listed.contains("<b>my</b>"), "report text must be escaped where it is rendered:\n{listed}");
    assert!(listed.contains("&lt;b&gt;"), "the markup must survive as escaped text rather than being stripped");

    let _ = std::fs::remove_dir_all(&scratch);
}
