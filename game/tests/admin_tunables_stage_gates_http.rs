//! The four world-stage drop gates on `/admin/tunables`, over real HTTP
//! (2026-09-02).
//!
//! Polishing sand, Perfect items, Divine Dust and Sacred items each became
//! a `LiveTunable` in the same change. Two of them had no stage gate at all
//! before it; Perfect's lived on the retired `late_content_stage` (default
//! 100, now 150 under its own name) and Sacred's was the hardcoded
//! `SACRED_STAGE_THRESHOLD`.
//!
//! **Unlike the pool cap, these defaults are NOT inert.** They are active
//! on the very first boot, so what this file proves is the operator half of
//! a live behaviour change: that each control renders with its unit and its
//! bounds, that a save reaches the tunables the fight loop reads and
//! survives a restart, that an out-of-range value cannot get through by
//! bypassing the browser, and - the assertion this feature exists to
//! protect - that a body which OMITS a gate falls back to the shipped
//! constant rather than to `0`, which would silently open the gate at stage
//! 0 and undo the whole feature while still reporting a successful save.
//! That last defect has been found twice in this codebase.
//!
//! Same disposable-instance setup as `admin_tunables_pool_cap_http.rs`, and
//! the same house rule about the POST body: the field set is derived from
//! the rendered page, never hand-written.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// The four gates: form field name, shipped default, and a distinctive
/// non-default value to save it to. Spelled out here on purpose - if
/// someone edits the constants without meaning to change live behaviour,
/// this file fails and says so.
const GATES: &[(&str, u32, &str)] = &[
    ("sand_drop_stage", 100, "111"),
    ("perfect_item_stage", 150, "222"),
    ("divine_dust_drop_stage", 300, "333"),
    ("sacred_item_stage", 300, "444"),
];

/// Must match `adventure::DROP_STAGE_MIN` / `DROP_STAGE_MAX`.
const STAGE_MIN: u32 = 0;
const STAGE_MAX: u32 = 100_000;

#[tokio::test]
async fn the_four_stage_gates_render_with_bounds_and_round_trip_through_a_real_form_post() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_stage_gates_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        manager.clone(),
        sessions_path,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let page = |client: reqwest::Client, base: String| async move {
        client
            .get(format!("{base}/admin/tunables"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .send()
            .await
            .expect("GET /admin/tunables failed")
            .text()
            .await
            .expect("body must read")
    };

    // --- the shipped configuration, as the game actually reads it -------
    // These are LIVE defaults, not inert ones: a fresh instance gates on
    // exactly these numbers from its first fight.
    let live = manager.live_tunables();
    let read_gate = |t: &game::adventure::LiveTunables, name: &str| -> u32 {
        match name {
            "sand_drop_stage" => t.sand_drop_stage,
            "perfect_item_stage" => t.perfect_item_stage,
            "divine_dust_drop_stage" => t.divine_dust_drop_stage,
            "sacred_item_stage" => t.sacred_item_stage,
            other => panic!("unknown gate {other}"),
        }
    };
    for &(name, shipped, _) in GATES {
        assert_eq!(read_gate(&live, name), shipped, "{name} must ship at {shipped} - anything else is a live behaviour change nobody asked for");
    }

    let admin_page = page(client.clone(), base.clone()).await;

    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        &admin_page[start..end]
    };

    // --- each control carries its unit and its range --------------------
    // The house standard for a numeric tunable is a typed input with
    // min/max (which is what actually reports an out-of-range value to the
    // operator, in the browser, before the POST is made) plus a hint
    // stating the unit. A bare unlabelled number field is banned.
    for &(name, shipped, _) in GATES {
        let row = {
            let start = form_html.find(&format!("for=\"{name}\"")).unwrap_or_else(|| panic!("the {name} control must render inside the save form"));
            let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
            &form_html[start..end]
        };
        assert!(row.contains("type=\"number\""), "{name} must be a typed numeric input, not free text: {row}");
        assert!(row.contains(&format!("min=\"{STAGE_MIN}\"")), "{name} must carry its lower bound so the browser rejects a low value visibly: {row}");
        assert!(row.contains(&format!("max=\"{STAGE_MAX}\"")), "{name} must carry its upper bound so the browser rejects a high value visibly: {row}");
        assert!(row.contains("required"), "{name}: an empty submission must be rejected rather than read as 0: {row}");
        assert!(row.to_lowercase().contains("world stage"), "{name}'s label or hint must state the UNIT - a bare number field is banned: {row}");
        assert!(row.contains(&format!("value=\"{shipped}\"")), "{name} must render the live value back: {row}");
    }

    // The field set comes off the rendered page, per the house rule - a
    // hand-written body can only catch drift in one direction, and it is
    // the other direction that shipped a dead Save button in production.
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    for &(name, _, _) in GATES {
        assert!(rendered.contains(&name), "{name} must be inside the save form, not merely on the page");
    }
    assert!(
        !rendered.contains(&"late_content_stage"),
        "late_content_stage was retired by this change and must not still render - a dial that does nothing is worse than no dial"
    );

    // --- THE MERGE CHECK ------------------------------------------------
    // This branch and `feature/win-based-xp` added their fields to the SAME
    // four places (the struct, its `Default`, `TunablesForm`, the render
    // block) and landed one after the other, so the conflict resolution was
    // textual and a keep-both that quietly dropped one side would still
    // compile and still pass every test that only looks at its own fields.
    // Asserting the UNION renders is what actually catches that. If this
    // fails after a merge, someone resolved a conflict by choosing a side.
    for name in ["win_xp_flat", "win_xp_level_pct", "win_xp_mult", "win_xp_cooldown_secs", "win_xp_catchup_enabled"] {
        assert!(rendered.contains(&name), "{name} (from feature/win-based-xp) must still render alongside this branch's four gates - nine new fields total");
    }

    // --- a save reaches the tunables the fight loop actually reads -------
    // All four are moved in ONE post, which is also what proves they are
    // four independent fields rather than one value wired up four times.
    let body: Vec<(&str, &str)> = rendered
        .iter()
        .map(|name| match GATES.iter().find(|(gate, _, _)| gate == name) {
            Some(&(_, _, raised)) => (*name, raised),
            None if *name == "enemy_hp_pool_hard_cap" => (*name, "1000000000000000"),
            None => (*name, "1"),
        })
        .collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&body).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());

    let after = manager.live_tunables();
    for &(name, _, raised) in GATES {
        assert_eq!(read_gate(&after, name), raised.parse::<u32>().unwrap(), "a save must move {name} to the value the operator typed");
    }

    let admin_page = page(client.clone(), base.clone()).await;
    for &(name, _, raised) in GATES {
        assert!(admin_page.contains(&format!("value=\"{raised}\"")), "{name}'s saved value must render back, or the operator cannot see the state they set");
    }

    // A restart must not silently revert a drop gate.
    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the gates must persist to the live tunables file");
    for &(name, _, raised) in GATES {
        assert!(on_disk.contains(&format!("{name} = {raised}")), "{name} must survive a restart - got: {on_disk}");
    }

    // --- out of range cannot get through by bypassing the browser -------
    // The form's min/max is what an operator sees. A hand-crafted POST has
    // no such gate, so the handler rejects rather than clamps: 400, the
    // field and its range named, and the live value left where it was.
    for &(name, _, raised) in GATES {
        let before = read_gate(&manager.live_tunables(), name);
        assert_eq!(before, raised.parse::<u32>().unwrap(), "sanity: the save above is what must survive every rejected POST below");
        for attempt in ["100001", "4294967295"] {
            let body: Vec<(&str, &str)> = rendered
                .iter()
                .map(|field| {
                    if *field == name {
                        (*field, attempt)
                    } else if let Some(&(_, _, other)) = GATES.iter().find(|(gate, _, _)| gate == field) {
                        (*field, other)
                    } else if *field == "enemy_hp_pool_hard_cap" {
                        (*field, "1000000000000000")
                    } else {
                        (*field, "1")
                    }
                })
                .collect();
            let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&body).send().await.expect("POST failed");
            assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "{name}: the out-of-range POST {attempt} must be REJECTED, not clamped and reported as saved");
            let text = saved.text().await.expect("body");
            assert!(text.contains("NOT SAVED"), "{name}: the refusal must say plainly that nothing was written ({attempt})");
            assert!(text.contains(name), "the refusal must name the offending field: {name} ({attempt})");
            assert!(text.contains(&format!("{STAGE_MAX}")), "{name}: the refusal must name the accepted range so the operator knows what to type ({attempt})");
            assert_eq!(read_gate(&manager.live_tunables(), name), before, "{name}: a rejected POST must leave the live value untouched ({attempt})");
        }
    }

    // --- THE DEFECT THIS FEATURE HAS TO NOT REPEAT ----------------------
    // A body that omits a gate entirely - an older client, or any test
    // still posting a pre-existing field set - must neither 422 nor
    // collapse the gate to `u32::default()` == 0. Zero is not a harmless
    // fallback here: it means "this drop is ungated at every stage", i.e.
    // the whole feature silently off while the page says "Saved".
    for &(name, shipped, _) in GATES {
        let without: Vec<(&str, &str)> = rendered
            .iter()
            .filter(|field| **field != name)
            .map(|field| {
                if let Some(&(_, _, other)) = GATES.iter().find(|(gate, _, _)| gate == field) {
                    (*field, other)
                } else if *field == "enemy_hp_pool_hard_cap" {
                    (*field, "1000000000000000")
                } else {
                    (*field, "1")
                }
            })
            .collect();
        assert_eq!(without.len(), rendered.len() - 1, "sanity: exactly {name} was dropped from the body");
        let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&without).send().await.expect("POST failed");
        assert!(saved.status().is_redirection(), "a body omitting {name} must still extract - got {}. A 422 here means the field is required rather than defaulted", saved.status());
        assert_eq!(
            read_gate(&manager.live_tunables(), name),
            shipped,
            "an omitted {name} must fall back to the SHIPPED CONSTANT ({shipped}), not to 0 - a serde default of 0 would open the gate at stage 0 and silently disable the whole feature"
        );
    }

    let _ = std::fs::remove_dir_all(&scratch);
}
