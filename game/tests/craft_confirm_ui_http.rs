//! The confirm-before-submit gate on the irreversible craft actions
//! (2026-09-02).
//!
//! WHY THIS FILE EXISTS. `base.html`'s confirm handler used to bind with
//! `document.querySelector('.craft-actions').closest('form')` - the FIRST
//! form on the page containing a `.craft-actions` row. On 2026-08-19
//! (2834e80, Divine Dust Stage 5) a standalone `<form>` carrying its own
//! `.craft-actions` div was inserted ABOVE the crafting form. From that
//! commit the listener attached to a form containing no `data-confirm`
//! button at all, and every confirmation in the game went silent for two
//! weeks: Krangle, Scour, Annulment, Chancing, Hideout Warrior, and
//! Divinity, which shipped after the break and never confirmed once.
//!
//! No test caught it because the only test that looked (`divinity_ui_http`)
//! asserted the SERVER emitted `data-confirm`. It always did. The server
//! side never broke. What broke was whether anything was listening.
//!
//! WHAT THIS TEST CAN AND CANNOT DO. The workspace suite has no browser
//! and no JS engine, so it cannot click a button and observe a real
//! `window.confirm` dialog. It checks the two halves that, together,
//! decide whether the dialog appears at all:
//!
//!   1. COVERAGE - every irreversible action really does render a
//!      `data-confirm` button on one real page, over real HTTP.
//!   2. WIRING - the handler is attached where it is guaranteed to see
//!      them: `document`. A submit event bubbles to the document from any
//!      form anywhere on the page, so a delegated listener keyed off the
//!      submitter's own attribute cannot be stolen by an inserted form,
//!      cannot be outranked by a later element, and does not care what
//!      order anything renders in. An element-scoped binding can only
//!      ever cover the one element it resolved to, which is exactly the
//!      failure above - so this test REJECTS one outright.
//!
//! Check 2 fails on the 2026-08-19 code. That is the point: it is the
//! assertion whose absence let a dead confirmation ship.
//!
//! One `#[tokio::test]` per file, deliberately - `adventure::set_data_dir`
//! is a process-wide `OnceLock`. See `divinity_ui_http.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character, CraftAction};

/// Every action whose button must carry `data-confirm`. A new
/// irreversible action is registered here or this test fails - which is
/// the only way the coverage half stays honest as the card grows.
const CONFIRMED_ACTIONS: &[&str] = &["krangle", "scour", "annulment orb", "chancing", "hideout warrior", "divinity"];

/// Pulls the confirm block out of `templates/base.html` by its own code
/// rather than by line number, so ordinary edits above it cannot silently
/// re-point this test at the wrong `<script>` block.
///
/// `//` comments are stripped: the block's own comment explains the
/// binding it must never go back to, quoting it verbatim, and a scan that
/// read prose would fail on the explanation of the bug rather than on the
/// bug. (No string literal in this block contains `//`.)
fn confirm_block(base_html: &str) -> String {
    let anchor = base_html.find("hasAttribute('data-confirm')").expect("base.html must still contain the data-confirm handler");
    let start = base_html[..anchor].rfind("try { (function() {").expect("the handler must live inside one of the isolated try-blocks");
    let end = start + base_html[start..].find("})(); } catch").expect("that try-block must be closed");
    base_html[start..end]
        .lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `<button ...>` on the page that carries `data-confirm`, returned
/// as its `name="action"` value.
fn confirm_button_actions(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for piece in body.split("<button").skip(1) {
        let tag = match piece.find('>') {
            Some(i) => &piece[..i],
            None => continue,
        };
        if !tag.contains("data-confirm=\"1\"") {
            continue;
        }
        let value = tag.split("value=\"").nth(1).and_then(|v| v.split('"').next()).unwrap_or("<no value attribute>");
        found.push(value.to_string());
    }
    found
}

#[tokio::test]
async fn every_irreversible_craft_button_is_confirmed_and_the_handler_is_delegated() {
    // Integration tests run with their PACKAGE dir as CWD, but the
    // template loader resolves "templates/" against CWD and that
    // directory belongs to the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("craft_confirm_ui_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "confirm-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"ConfirmTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    // Must run before anything that could touch `data_path` - even
    // constructing a `Character` reaches it transitively via item
    // generation. See `divine_dust_ui_http.rs` for the full note.
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("ConfirmTester".to_string());
    // A held Unique Shard is what reveals the Divinity row, and a
    // non-empty bag is what keeps it enabled - both are needed for all
    // six buttons to be on ONE page at once, which is the whole point.
    character.craft_tokens.push((CraftAction::UniqueShard, 1));
    for slot in [&mut character.weapon, &mut character.helm, &mut character.gloves] {
        let item = slot.take().expect("starter kit must equip this slot");
        character.inventory.push(item);
    }

    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), character);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path)
        .await
        .expect("disposable adventure_web server must start");
    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let resp = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, "adv_session=test-token").send().await.expect("GET /inventory failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.expect("failed to read /inventory body");

    // --- 1. Coverage: every irreversible action carries the flag. ---
    let flagged = confirm_button_actions(&body);
    for action in CONFIRMED_ACTIONS {
        assert!(
            flagged.iter().any(|f| f == action),
            "'{action}' is irreversible and must render with data-confirm; the page flagged {flagged:?}"
        );
    }
    assert!(
        body.contains("data-confirm-msg=\""),
        "Divinity is a whole-bag action and must override the per-item confirm text, which would name an item it ignores"
    );

    // --- 2. Wiring: the handler must be delegated on the document. ---
    let base_html = std::fs::read_to_string("templates/base.html").expect("templates/base.html must be readable from the workspace root");
    let block = confirm_block(&base_html);
    let block = block.as_str();
    assert!(
        block.contains("document.addEventListener('submit'"),
        "the confirm handler must listen on `document`. Submit events bubble, so a delegated listener sees every form on the page no matter how many exist or what order they render in. Handler as written:\n{block}"
    );
    assert!(
        !block.contains(".closest('form')") && !block.contains(".closest(\"form\")"),
        "the confirm handler must NOT resolve its binding to one form by element lookup. That is precisely how it broke on 2026-08-19: it bound to the first form containing a .craft-actions row, a standalone form was inserted above the crafting form, and all six confirmations went silent for two weeks. Handler as written:\n{block}"
    );
    // Any listener attached to something other than `document` is
    // element-scoped by definition and can only cover that one element.
    for scoped in ["craftForm.addEventListener", "form.addEventListener"] {
        assert!(
            !block.contains(scoped),
            "`{scoped}` scopes the confirm gate to a single form; buttons in any other form submit unconfirmed. Delegate on `document` instead"
        );
    }

    // --- 3. The gate must key off the SUBMITTER, not the form. The
    // crafting form's other buttons (Transmute, Polish, Recombine...)
    // share it and must submit without a dialog. ---
    assert!(
        block.contains("e.submitter") && block.contains("hasAttribute('data-confirm')"),
        "the gate must fire on the clicked button's own data-confirm, so the non-destructive buttons sharing the form stay one-click:\n{block}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
