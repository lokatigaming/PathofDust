//! THE THREE-WAY AGREEMENT: declared, charged, and DISPLAYED (2026-09-09).
//!
//! WHY THIS FILE EXISTS. A player was shown "3.1k" on the crafting panel's
//! Reforge button, pressed it, and was refused with "15k needed".
//! `templates/base.html` carried a hardcoded `var dustCost = 30 * tier *
//! times` - the flat curve retired on 2026-09-02 - while the server
//! charged `PriceRule::MultipleOfStandard { times: 5, base: 60 }` through
//! `dust_at`. At tier 101 that is 3,030 against 15,260. The affordability
//! gate read the same wrong variable, so the button looked affordable and
//! the click bounced. Veiled Recombine had the same defect twice over:
//! an UNSCALED veil surcharge and a hardcoded `500 * pool` against a real
//! `50 + pool x (250 + surcharge)`.
//!
//! The existing agreement test (`craft::price_rule_tests`) compares
//! DECLARED against CHARGED and both were right the whole time. **The
//! label was a third number, and nothing compared anything to it.**
//!
//! WHAT THIS TEST DOES. It GETs the real crafting panel over real HTTP,
//! scrapes each priced button's own attributes, and evaluates the SAME
//! expression `templates/base.html` evaluates - then asserts the result
//! equals `PriceRule::dust_at`, at every tier, for every action.
//!
//! Deriving the inputs from the rendered page rather than from a
//! hand-written list is the point, and it is the shape
//! `admin_tunables_splash_http.rs` established: a button that stops
//! emitting `data-times` fails here, not just one whose arithmetic drifts.
//!
//! WHAT IT CANNOT DO. There is no JS engine in the workspace suite, so
//! this re-implements the browser's expression in Rust rather than
//! running it. That is why the second half asserts on `base.html`'s SOURCE
//! - that the retired formulas are gone and that the script computes from
//! attributes. A test that only re-implemented the expression would still
//! pass if the page went back to `30 * tier` tomorrow.
//!
//! One `#[tokio::test]` per file, deliberately - `adventure::set_data_dir`
//! is a process-wide `OnceLock`. See `divinity_ui_http.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{composite_price, AdventureManager, Character, CraftAction, PriceRule, CRAFT_BASE_COST_MULT, CRAFT_TIER_EXPONENT, TIER_CRAFT_DUST_COST};

/// Tiers to check. 3 is a live dropped item at stage 10; 35 is roughly the
/// top of the live band; 101 is the tier the reported incident happened
/// at, kept by name; 201 and 1000 are ahead of the game, so a rule that
/// only misbehaves at scale still fails here.
const TIERS: [u32; 6] = [1, 3, 35, 101, 201, 1000];

/// One priced button, as the page rendered it.
#[derive(Debug)]
struct RenderedPrice {
    action: String,
    base: u64,
    times: u64,
    flat: u64,
    per_unit: bool,
    tier_mult: f64,
    tier_exp: f64,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    tag.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()).map(str::to_string)
}

/// Every button on the page that carries the price parameters, parsed per
/// `<button>` tag so an attribute belonging to a different button can
/// never be read as this one's.
fn rendered_prices(body: &str) -> Vec<RenderedPrice> {
    let mut found = Vec::new();
    for piece in body.split("<button").skip(1) {
        let Some(end) = piece.find('>') else { continue };
        let tag = &piece[..end];
        let (Some(base), Some(action)) = (attr(tag, "data-base"), attr(tag, "value")) else { continue };
        let (Some(tier_mult), Some(tier_exp)) = (attr(tag, "data-tier-mult"), attr(tag, "data-tier-exp")) else { continue };
        found.push(RenderedPrice {
            action,
            base: base.parse().expect("data-base must be an integer"),
            times: attr(tag, "data-times").map_or(1, |v| v.parse().expect("data-times must be an integer")),
            flat: attr(tag, "data-flat").map_or(0, |v| v.parse().expect("data-flat must be an integer")),
            per_unit: tag.contains("data-per-unit"),
            tier_mult: tier_mult.parse().expect("data-tier-mult must be a number"),
            tier_exp: tier_exp.parse().expect("data-tier-exp must be a number"),
        });
    }
    found
}

impl RenderedPrice {
    /// `templates/base.html`'s `ruleCostOf`, in Rust. Kept term-for-term
    /// with the JS - `Math.ceil` per term, never one ceil over the sum -
    /// because a difference in rounding here would hide exactly the class
    /// of defect this file exists for.
    fn displayed_at(&self, tier: u32, units: u64) -> u64 {
        let tier_cost = if tier == 0 { 0 } else { (self.tier_mult * (tier as f64).powf(self.tier_exp)).ceil() as u64 };
        let n = if self.per_unit { units } else { 1 };
        self.flat + n * self.times * (self.base + tier_cost)
    }
}

#[tokio::test]
async fn the_displayed_price_equals_the_declared_and_charged_price_at_every_tier() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("craft_price_label_agreement_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "price-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"PriceTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("PriceTester".to_string());
    // A banked token renders the FREE branch of `action_btn`, which emits
    // no price attributes at all - every assertion below would then pass
    // vacuously. Cleared so the buttons render on the PRICED path, same
    // reasoning as `craft_confirm_ui_http.rs`.
    character.craft_tokens.clear();
    // A banked free recombine renders the "Free - N tokens" branch, which
    // likewise carries no price attributes at all.
    character.free_recombines = 0;
    for marker in ["adventure-craft-token-backfill-marker.json", "adventure-craft-token-backfill-v2-marker.json"] {
        std::fs::write(scratch.join(marker), "true").expect("failed to seed a migration marker");
    }
    // Two items in the bag: Recombine needs a second one to render at all.
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

    let rendered = rendered_prices(&body);
    // The six standard currency buttons, plus Reforge and Recombine - the
    // two that were NOT on this path until today.
    assert!(rendered.len() >= 8, "expected at least the six currency buttons plus Reforge and Recombine to carry price parameters, found {}: {rendered:?}", rendered.len());
    for required in ["reforge", "recombine"] {
        assert!(
            rendered.iter().any(|r| r.action == required),
            "'{required}' must render the price parameters every other priced button carries - it was a bespoke branch with a hardcoded formula until 2026-09-09, which is the defect. Found: {:?}",
            rendered.iter().map(|r| &r.action).collect::<Vec<_>>()
        );
    }

    for r in &rendered {
        assert_eq!(r.tier_exp, CRAFT_TIER_EXPONENT, "{} must render the LIVE craft_tier_exponent, not a compiled figure", r.action);
        assert_eq!(r.tier_mult, TIER_CRAFT_DUST_COST as f64, "{} must render the live per-tier multiplier", r.action);

        // The rule the CHARGE reads, looked up the same way the charge
        // site looks it up.
        //
        // **A rendered priced button whose rule cannot be resolved is a
        // FAILURE, never a skip.** The first version of this test used
        // `ALL_CRAFT_ACTIONS` alone and `continue`d when it found nothing
        // - and `ALL_CRAFT_ACTIONS` does not contain Reforge, which is
        // exactly the action this whole file was written about. The
        // numeric half passed vacuously for it: a mutation that dropped
        // `data-times` from Reforge's button went completely undetected.
        // A silent `continue` in a coverage test is the same defect as a
        // label nobody compares to a charge.
        let (rule, units): (PriceRule, u64) = match r.action.as_str() {
            "recombine" => (composite_price("recombine_veiled"), 4),
            other => {
                let action = game::adventure::ALL_CRAFT_ACTIONS
                    .iter()
                    .copied()
                    .chain([CraftAction::Reforge, CraftAction::Polishing, CraftAction::DivineDust, CraftAction::CelestialShard, CraftAction::UniqueShard])
                    .find(|a| a.label().to_lowercase() == other)
                    .unwrap_or_else(|| panic!("'{other}' renders price parameters but no CraftAction has that label - this test cannot check a price it cannot look up, and must not pretend it did"));
                (action.price_rule(), 1)
            }
        };
        let Some(_) = rule.display_params(CRAFT_BASE_COST_MULT) else {
            panic!("'{}' renders price parameters but its rule is not dust-denominated - a button that shows a dust price must have a dust rule behind it", r.action)
        };

        for tier in TIERS {
            // Recombine is the only per-unit rule, so it is checked at
            // every pool size it can actually be charged at - one modifier
            // through the four-modifier cap - not just one.
            let unit_range = if r.per_unit { 1..=units } else { 1..=1 };
            for u in unit_range {
                let declared = rule.dust_at(tier, u, CRAFT_BASE_COST_MULT, CRAFT_TIER_EXPONENT).expect("dust-denominated");
                let displayed = r.displayed_at(tier, u);
                assert_eq!(
                    displayed, declared,
                    "{} at tier {tier} with {u} unit(s): the PAGE shows {displayed} and the RULE charges {declared}. \
                     A label the charge does not agree with is the 2026-09-09 defect - the player is shown one number, \
                     refused with another, and the affordability gate believes the label",
                    r.action
                );
            }
        }
    }

    // --- The page must not carry a price FORMULA, only parameters. ---
    let base_html = std::fs::read_to_string("templates/base.html").expect("templates/base.html must be readable from the workspace root");
    // `//` comments stripped first, the same reason
    // `craft_confirm_ui_http.rs` strips them: the block's own comment
    // quotes the retired formulas verbatim to explain what went wrong, and
    // a scan that read prose would fail on the explanation of the bug
    // rather than on the bug.
    let base_html: String = base_html
        .lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let base_html = base_html.as_str();
    assert!(
        !base_html.contains("30 * tier"),
        "`30 * tier` is the flat panel-Reforge curve retired on 2026-09-02. It survived in base.html for a week and showed 3,030 where the server charged 15,260. The price must come from the button's own data-times/data-base/data-flat"
    );
    assert!(
        !base_html.contains("500 * pool"),
        "`500 * pool` is veiled Recombine's retired per-modifier price. The real rule is one Krangle per combined modifier, which scales with tier"
    );
    assert!(
        base_html.contains("data-times") && base_html.contains("data-flat") && base_html.contains("data-per-unit"),
        "the preview must read the whole rule off the button. Reading only some of it is how Reforge's 5x multiple went missing from the label while the charge applied it"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
