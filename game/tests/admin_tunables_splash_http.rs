//! `/admin/tunables/save` over real HTTP (2026-08-20) - the splash
//! redesign's 6 new `LiveTunables` fields (`splash_extra_targets`,
//! `splash_support_floor_targets`, `splash_overcap_bonus_targets`,
//! `splash_ladder_step_pct`, `splash_ladder_targets_per_step`,
//! `splash_damage_pct`) added `TunablesForm` fields with NO
//! `#[serde(default)]` (same as every other numeric tunable on this
//! form) - a real `axum::Form` extraction is the only thing that can
//! catch a name mismatch between the form's `<input name="...">`
//! attributes and the struct fields deserializing them; an in-crate
//! call straight into `do_save_tunables`'s Rust types would sail past
//! that exact class of bug (see `divine_dust_craft_http.rs`'s own doc
//! for the live 422 this house rule exists because of).
//!
//! Same disposable-instance setup as `admin_passives_http.rs`: an
//! OS-assigned ephemeral port and a scratch data directory, so nothing
//! here can reach the live game's files or ports.

use std::path::PathBuf;
use game::adventure::AdventureManager;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
const OTHER_LOGIN: &str = "someone_else";

#[tokio::test]
async fn admin_tunables_save_gates_writes_and_the_splash_fields_round_trip() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_splash_http_{}", std::process::id()));
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
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let baseline = manager.live_tunables();
    assert_eq!(baseline.splash_extra_targets, 2, "sanity: fresh tunables start at LiveTunables::default()");

    // Every field `TunablesForm` requires (no `#[serde(default)]` on the
    // numeric ones) - a missing one here would 422 instead of silently
    // defaulting, exactly the failure mode this test exists to catch.
    // The 6 splash fields are set to distinctive, non-default values so
    // a name mismatch (a typo in either the `<input name>` or the
    // struct field) shows up as a wrong number, not a false pass.
    let form: Vec<(&str, String)> = vec![
        ("loot_mult", baseline.loot_mult.to_string()),
        ("sand_mult", baseline.sand_mult.to_string()),
        ("wings_drop_chance", baseline.wings_drop_chance.to_string()),
        ("celestial_shard_drop_chance", baseline.celestial_shard_drop_chance.to_string()),
        ("boss_health", baseline.boss_health.to_string()),
        ("boss_power", baseline.boss_power.to_string()),
        ("dynamic_scaling_mult", baseline.dynamic_scaling_mult.to_string()),
        ("boss_count_tier_stages", baseline.boss_count_tier_stages.to_string()),
        ("boss_count_cap_mult", baseline.boss_count_cap_mult.to_string()),
        ("late_content_stage", baseline.late_content_stage.to_string()),
        ("pierce_cap", baseline.pierce_cap.to_string()),
        ("pierce_h", baseline.pierce_h.to_string()),
        ("fight_summary_batch_size", baseline.fight_summary_batch_size.to_string()),
        ("thunder_redistribution_pct", baseline.thunder_redistribution_pct.to_string()),
        ("thunder_redistribution_window_secs", baseline.thunder_redistribution_window_secs.to_string()),
        ("reactive_proc_cap_ms", baseline.reactive_proc_cap_ms.to_string()),
        ("divine_dust_drop_chance", baseline.divine_dust_drop_chance.to_string()),
        ("divine_dust_disenchant_chance", baseline.divine_dust_disenchant_chance.to_string()),
        ("divine_dust_craft_dust_cost", baseline.divine_dust_craft_dust_cost.to_string()),
        ("divine_dust_craft_sand_cost", baseline.divine_dust_craft_sand_cost.to_string()),
        ("divine_dust_craft_output", baseline.divine_dust_craft_output.to_string()),
        ("rf_self_damage_pct_rank1", baseline.rf_self_damage_pct_rank1.to_string()),
        ("rf_self_damage_pct_rank2", baseline.rf_self_damage_pct_rank2.to_string()),
        ("rf_self_damage_pct_rank3", baseline.rf_self_damage_pct_rank3.to_string()),
        ("haloedsteps_per_instance_pct_rank1", baseline.haloedsteps_per_instance_pct_rank1.to_string()),
        ("haloedsteps_per_instance_pct_rank2", baseline.haloedsteps_per_instance_pct_rank2.to_string()),
        ("haloedsteps_per_instance_pct_rank3", baseline.haloedsteps_per_instance_pct_rank3.to_string()),
        ("shattering_damage_pct_rank1", baseline.shattering_damage_pct_rank1.to_string()),
        ("shattering_damage_pct_rank2", baseline.shattering_damage_pct_rank2.to_string()),
        ("shattering_damage_pct_rank3", baseline.shattering_damage_pct_rank3.to_string()),
        ("defensive_stat_hard_cap", baseline.defensive_stat_hard_cap.to_string()),
        // Stage 1 overflow-economy caps - distinctive non-default values,
        // same reasoning as the splash block below: a name mismatch must
        // show up as a wrong number, not a false pass.
        ("overflow_conversion_cap_per_rank", "0.07".to_string()),
        ("evasion_overflow_cap", "0.60".to_string()),
        ("block_overflow_cap", "0.70".to_string()),
        ("dr_overflow_cap", "0.65".to_string()),
        ("intervene_overflow_cap", "0.40".to_string()),
        ("verdantburst_echo_threshold_pct", baseline.verdantburst_echo_threshold_pct.to_string()),
        ("buffsnapshot_dedupe_window_ms", baseline.buffsnapshot_dedupe_window_ms.to_string()),
        // Distinctive, non-default splash values - the actual point of this test.
        ("splash_extra_targets", "7".to_string()),
        ("splash_support_floor_targets", "4".to_string()),
        ("splash_overcap_bonus_targets", "9".to_string()),
        ("splash_ladder_step_pct", "250".to_string()),
        ("splash_ladder_targets_per_step", "3".to_string()),
        ("splash_damage_pct", "0.42".to_string()),
        ("boss_power_mult_override", String::new()),
    ];
    let form_refs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();

    // --- a non-admin write must not take effect -----------------------
    let sneaky = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=other-token").form(&form_refs).send().await.expect("POST failed");
    assert!(sneaky.status().is_redirection(), "the handler redirects regardless, to avoid confirming the page exists");
    assert_eq!(manager.live_tunables().splash_extra_targets, 2, "a non-admin POST must not change any value");

    // --- the admin write ------------------------------------------------
    let save = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&form_refs).send().await.expect("POST failed");
    assert!(save.status().is_redirection(), "a real Form<TunablesForm> extraction must succeed with every field present - a 4xx here means a name mismatch");

    let saved = manager.live_tunables();
    assert_eq!(saved.splash_extra_targets, 7);
    assert_eq!(saved.splash_support_floor_targets, 4);
    assert_eq!(saved.splash_overcap_bonus_targets, 9);
    assert_eq!(saved.splash_ladder_step_pct, 250);
    assert_eq!(saved.splash_ladder_targets_per_step, 3);
    assert!((saved.splash_damage_pct - 0.42).abs() < 1e-9);

    // Stage 1 (2026-08-24): the five overflow-economy dials round-trip
    // too. These have #[serde(default)] on TunablesForm, so the failure
    // mode this guards is the OTHER direction - the page stops rendering
    // one of them (or its name drifts) and a real browser save silently
    // resets it to Default, which the distinctive values here turn into a
    // loud wrong number instead of a quiet pass.
    assert!((saved.overflow_conversion_cap_per_rank - 0.07).abs() < 1e-9);
    assert!((saved.evasion_overflow_cap - 0.60).abs() < 1e-9);
    assert!((saved.block_overflow_cap - 0.70).abs() < 1e-9);
    assert!((saved.dr_overflow_cap - 0.65).abs() < 1e-9);
    assert!((saved.intervene_overflow_cap - 0.40).abs() < 1e-9);

    // It reached the file, so it survives a restart.
    let tunables_file = scratch.join("adventure-live-tunables.toml");
    assert!(tunables_file.exists(), "the save must persist to disk");
    let contents = std::fs::read_to_string(&tunables_file).expect("readable");
    assert!(contents.contains("splash_extra_targets"), "the persisted file must name every new field, got:\n{contents}");
    assert!(contents.contains("splash_ladder_step_pct"), "got:\n{contents}");

    // --- the GET page reflects the saved values ------------------------
    let admin_page = client
        .get(format!("{base}/admin/tunables"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    assert!(admin_page.contains("value=\"7\""), "the retuned splash_extra_targets must render back into its own input");
    assert!(admin_page.contains("name=\"splash_ladder_step_pct\""), "the new ladder field must actually be in the form");
    // Stage 1: the overflow-economy group must render its heading and all
    // five dials, or the page-derived drift guard below would silently
    // stop covering them.
    assert!(admin_page.contains("Overflow Economy (cross-class caps)"), "the Stage 1 group heading must render");
    for field in ["overflow_conversion_cap_per_rank", "evasion_overflow_cap", "block_overflow_cap", "dr_overflow_cap", "intervene_overflow_cap"] {
        assert!(admin_page.contains(&format!("name=\"{field}\"")), "{field} must render as a form input");
    }
    // Both pacing controllers must show the multiplier ACTUALLY in force
    // (the max of the controller's own value and the stage baseline) -
    // without it, a controller pinned to the baseline renders a "current"
    // number that generation never used. See render_tunables_page.
    assert!(admin_page.contains("Controller A (HP / duration)"), "Controller A's pacing readout must render on the admin page");
    assert!(admin_page.contains("Controller B (damage / lethality)"), "Controller B's pacing readout must render on the admin page");
    assert_eq!(admin_page.matches("in force").count(), 2, "both controllers must render their 'in force' multiplier, got:\n{}", admin_page.matches("in force").count());

    // --- form/struct drift guard (2026-08-23) --------------------------
    // Every assertion above posts a hand-written SUPERSET body, which is
    // exactly why they all kept passing while `/admin/tunables` Save was
    // dead in production: the dynamic-pacing branch dropped the retired
    // `dynamic_scaling_mult` <input> from the page but left the field on
    // `TunablesForm` with no `#[serde(default)]`, so a real browser save
    // - which posts only what the page renders - 422'd on every attempt.
    // A superset body can never catch that. Derive the field set from the
    // rendered page and post EXACTLY it, so any future drift in either
    // direction fails here instead of shipping.
    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        &admin_page[start..end]
    };
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert!(rendered.len() > 40, "sanity: the tunables form renders many inputs, found {}", rendered.len());
    let exact: Vec<(&str, &str)> = rendered.iter().map(|name| (*name, "1")).collect();
    let exact_save =
        client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&exact).send().await.expect("POST failed");
    assert!(
        exact_save.status().is_redirection(),
        "posting exactly the {} fields the page renders must extract cleanly - got {}. A 422 here means `TunablesForm` requires a field the form no longer renders (or renders one it does not accept)",
        rendered.len(),
        exact_save.status()
    );

    std::fs::remove_dir_all(&scratch).ok();
}
