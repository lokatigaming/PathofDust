// The `/wiki` page - extracted verbatim from adventure_web.rs (2026-08-18,
// a pure code-motion refactor: zero behavior change, no renames, no
// cleanup). Every helper it leans on (top_nav, render_page,
// current_session, escape_html, compute_passive_layout, root_node_html,
// passive_archetype_icon_role, ...) is shared with other pages and
// deliberately stayed in the parent - reached here through `use super::*`,
// the same convention src/adventure/*.rs already uses to see adventure.rs.
//
// 2026-08-19: routing went from one registered route per page to a single
// `/wiki/:page` dynamic route (`wiki_dynamic_page`) that serves any
// `wiki/{page}.md` that exists on disk - adding a new prose page no longer
// needs a rebuild, matching how *editing* a page already worked. `/wiki`
// (the landing ToC) and `/wiki/passives` (code-generated, no .md file)
// stay their own explicit routes since axum's router always prefers a
// literal path segment over a `:param` one, regardless of registration
// order - no ambiguity between the two.

use super::*;
use regex::Regex;
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

/// Base directory the prose pages are authored in as real Markdown -
/// relative to CWD, same convention `patch-notes.json` already uses (see
/// `patch_notes` in adventure_web.rs) so it resolves the same way in dev
/// and prod without an absolute path baked in. Passives are the deliberate
/// exception - that section is fully code-generated (see
/// `WIKI_PASSIVES_HTML`) and has no .md file.
const WIKI_MD_DIR: &str = "wiki";

struct CachedMdPage {
    mtime: SystemTime,
    rendered: String,
    headings: Vec<WikiHeading>,
}

/// One cache slot per prose page, keyed by its markdown filename (no
/// extension) - a plain `String` key now rather than `&'static str`, since
/// `wiki_dynamic_page` hands this a request-derived page name that can't
/// be `'static`. Guarded by mtime, not a TTL: editing a .md file on disk
/// takes effect on the very next request that notices the new mtime, no
/// rebuild or restart - the whole point of moving this content out of
/// Rust string literals. Reset for free on process restart along with
/// everything else, so there's no explicit invalidation path to maintain.
static WIKI_MD_CACHE: LazyLock<Mutex<HashMap<String, CachedMdPage>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Loads, Markdown-renders, and placeholder-substitutes `wiki/{name}.md`,
/// caching the rendered HTML (and its extracted heading list, for the
/// sidebar's in-page nav - see `extract_and_inject_heading_ids`) until the
/// file's mtime changes. `name` is validated by `is_valid_wiki_slug` before
/// this is ever called with a request-derived value (see
/// `wiki_dynamic_page`) - ASCII lowercase-alphanumeric-and-hyphen only, so
/// there's no path-traversal surface despite building a filesystem path
/// from it, and no way to reach a file outside `wiki/` or one without a
/// `.md` extension.
fn render_markdown_page(name: &str) -> (String, Vec<WikiHeading>) {
    let path = format!("{WIKI_MD_DIR}/{name}.md");
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);

    let mut cache = WIKI_MD_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(name) {
        if cached.mtime == mtime {
            return (cached.rendered.clone(), cached.headings.clone());
        }
    }

    let source = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        tracing::error!("Failed to read {path}: {err}");
        format!("<p class=\"muted\">[wiki content missing: {name}]</p>")
    });
    let substituted = substitute_wiki_placeholders(&source);
    let mut html_out = String::new();
    let parser = pulldown_cmark::Parser::new_ext(&substituted, pulldown_cmark::Options::ENABLE_TABLES);
    pulldown_cmark::html::push_html(&mut html_out, parser);
    let (html_out, headings) = extract_and_inject_heading_ids(&html_out);

    cache.insert(name.to_string(), CachedMdPage { mtime, rendered: html_out.clone(), headings: headings.clone() });
    (html_out, headings)
}

/// One `<h2>`/`<h3>` found in a rendered wiki page - what the sidebar's
/// per-page "on this page" sub-list is built from.
#[derive(Clone)]
struct WikiHeading {
    level: u8,
    id: String,
    /// Tag-stripped inner text (emoji survive, since they aren't HTML
    /// tags) - safe to drop straight into the sidebar link's text.
    text: String,
}

/// Scans rendered HTML for `<h2>`/`<h3>` tags and returns (a) the same HTML
/// with an `id="..."` added to any heading that didn't already have one,
/// and (b) the ordered list of every heading found, existing-id or newly
/// assigned. Existing ids (hand-authored for the legacy `/wiki#slug`
/// redirect map - see `wiki_hash_redirect_script`) are never touched, only
/// ever read - this only ever *adds* anchors, so old links can't break.
///
/// A page is assumed to use at most one `<h2>` (its own title) followed by
/// any number of `<h3>`s - true of every page today - so processing `<h2>`
/// then `<h3>` and concatenating is document-order-correct without needing
/// a single combined pass (Rust's `regex` crate has no backreferences, so
/// one pattern matching "some heading level, then the SAME level's closing
/// tag" isn't expressible anyway - two simple per-level patterns sidestep
/// that entirely).
fn extract_and_inject_heading_ids(html: &str) -> (String, Vec<WikiHeading>) {
    static H2_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<h2(?:\s+id="([^"]*)")?>(.*?)</h2>"#).unwrap());
    static H3_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<h3(?:\s+id="([^"]*)")?>(.*?)</h3>"#).unwrap());

    let mut used_ids: HashSet<String> = HashSet::new();
    let mut headings: Vec<WikiHeading> = Vec::new();
    let after_h2 = inject_heading_ids(html, 2, &H2_RE, &mut used_ids, &mut headings);
    let after_h3 = inject_heading_ids(&after_h2, 3, &H3_RE, &mut used_ids, &mut headings);
    (after_h3, headings)
}

fn inject_heading_ids(html: &str, level: u8, re: &Regex, used_ids: &mut HashSet<String>, headings: &mut Vec<WikiHeading>) -> String {
    static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
    re.replace_all(html, |caps: &regex::Captures| {
        let inner = &caps[2];
        let text = TAG_RE.replace_all(inner, "").trim().to_string();
        let id = match caps.get(1).map(|m| m.as_str()).filter(|s| !s.is_empty()) {
            Some(existing) => {
                used_ids.insert(existing.to_string());
                existing.to_string()
            }
            None => {
                let base = slugify(&text);
                let mut candidate = base.clone();
                let mut n = 2;
                while used_ids.contains(&candidate) {
                    candidate = format!("{base}-{n}");
                    n += 1;
                }
                used_ids.insert(candidate.clone());
                candidate
            }
        };
        headings.push(WikiHeading { level, id: id.clone(), text: text.clone() });
        format!("<h{level} id=\"{id}\">{inner}</h{level}>")
    })
    .into_owned()
}

/// Lowercase ASCII alphanumerics joined by single hyphens - deliberately
/// the same shape as the hand-authored ids already scattered through
/// `wiki/*.md` (`"dragon"`, `"fire-demon"`), so an auto-generated id looks
/// no different from one an author typed by hand.
fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = true;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

/// Substitutes `{{CONSTANT_NAME}}` tokens against the real game constants
/// in `wiki_placeholder_map` - a typo or a renamed/removed constant shows
/// up loudly as `[MISSING: NAME]` in the rendered page instead of silently
/// vanishing, so drift between the wiki and the code it quotes gets
/// noticed immediately rather than needing a manual re-audit.
fn substitute_wiki_placeholders(source: &str) -> String {
    static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{([A-Z0-9_]+)\}\}").unwrap());
    let values = wiki_placeholder_map();
    PLACEHOLDER_RE
        .replace_all(source, |caps: &regex::Captures| {
            let name = &caps[1];
            values.get(name).cloned().unwrap_or_else(|| format!("[MISSING: {name}]"))
        })
        .into_owned()
}

/// The single source of truth every `{{PLACEHOLDER}}` in `wiki/*.md`
/// resolves against - real constants from the game's own code, not
/// numbers transcribed by hand. Every entry here is either a named
/// constant/`pub` accessor already reachable from this module (all of
/// `combat.rs`/`item.rs`/`craft.rs`'s `pub(crate)` items come along for
/// free via `adventure.rs`'s `pub use {combat,item,craft}::*;`, so none
/// of this needed any visibility changes), or a small derived value
/// (seconds instead of ms, a percentage instead of a fraction) computed
/// from one. `CraftAction::X.base_cost()` is deliberately used instead of
/// `craft_action_def(X).default_cost` for the six token-craft costs -
/// `base_cost()` is the one that actually applies
/// `adventure-item-balance.toml` overrides, so the wiki tracks whatever
/// price is really live, not just the compiled-in default.
///
/// The handful of numbers this audit verified accurate but couldn't wire
/// (Dragon's slow, Lich's wave size/cadence/cap, Fire Demon's heal cut,
/// Cthulhu's cap, Recombine's crit, Crafting-Panel Reforge's per-tier
/// rate, Polishing's costs) were bare literals with no name to import as
/// of Phase 4 - the primary session has since hoisted all of them into
/// named `pub(crate)` constants (see `WIKI_IMPACT.md`'s "Pure refactor"
/// entry), so every one of them is wired below too now.
fn wiki_placeholder_map() -> HashMap<&'static str, String> {
    let pct = |fraction: f64| -> String { format!("{}", (fraction * 100.0).round() as i64) };
    let crit_pct = |quality: f64, perfect: bool| -> String { format!("{:.1}", crate::adventure::reforge_crit_chance(quality, perfect) * 100.0) };
    // Bot->game published constants (2026-08-18, architecture refactor
    // Stage 2 - the owner's ruling on wiki.rs's crate placement: game-
    // side, full stop, so these 5 - the only bot-side reads this file
    // ever had - invert into a small published file instead of blocking
    // the move. See `crate::adventure::PublishedConstants`'s own doc for
    // the full mechanism. `None` (file missing/unparseable - the bot has
    // never run since this deploy, or hasn't started yet in whatever
    // order the two processes come up in) renders "varies" rather than a
    // real number - a stale-but-plausible cooldown would be worse than
    // an honest "we don't know right now."
    let published = crate::state::load_json::<crate::adventure::PublishedConstants>(crate::adventure::published_constants_path());
    let varies = || "varies".to_string();

    HashMap::from([
        // Currency-crafting dust costs - live, override-aware.
        ("TRANSMUTE_COST", CraftAction::Transmute.base_cost().to_string()),
        ("AUGMENT_COST", CraftAction::Augment.base_cost().to_string()),
        ("REGAL_COST", CraftAction::Regal.base_cost().to_string()),
        ("EXALT_COST", CraftAction::Exalt.base_cost().to_string()),
        ("SCOUR_COST", CraftAction::Scour.base_cost().to_string()),
        ("KRANGLE_COST", CraftAction::Krangle.base_cost().to_string()),
        ("ANNULMENT_COST", CraftAction::Annulment.base_cost().to_string()),
        ("CHANCING_COST", CraftAction::Chancing.base_cost().to_string()),
        ("TIER_CRAFT_DUST_COST", crate::adventure::TIER_CRAFT_DUST_COST.to_string()),
        ("WEB_REFORGE_DUST_COST", crate::adventure::WEB_REFORGE_DUST_COST.to_string()),
        ("VEIL_EXTRA_COST", crate::adventure::VEIL_EXTRA_COST.to_string()),
        // Reforge's quality-scaled bonus-affix crit chance - computed at
        // the same four sample points the wiki's table shows, straight
        // from the real formula instead of hand-copied numbers.
        ("REFORGE_CRIT_AT_0", crit_pct(0.0, false)),
        ("REFORGE_CRIT_AT_50", crit_pct(50.0, false)),
        ("REFORGE_CRIT_AT_100", crit_pct(100.0, false)),
        ("REFORGE_CRIT_AT_PERFECT", crit_pct(100.0, true)),
        ("PERFECT_QUALITY_BONUS_PCT", pct(crate::adventure::PERFECT_QUALITY_MULT - 1.0)),
        ("SACRED_STAGE_THRESHOLD", crate::adventure::SACRED_STAGE_THRESHOLD.to_string()),
        ("CELESTIAL_CONVERSION_PCT", pct(crate::adventure::CELESTIAL_CONVERSION_PCT)),
        // Cthulhu's Bubble.
        ("CTHULHU_DEBUFF_CADENCE_S", (crate::adventure::CTHULHU_DEBUFF_CADENCE_MS / 1000).to_string()),
        // Gelatinous Cube.
        ("CUBE_CAPTURE_CADENCE_S", (crate::adventure::CUBE_CAPTURE_CADENCE_MS / 1000).to_string()),
        ("CUBE_CAPTURE_PCT", pct(crate::adventure::CUBE_CAPTURE_PCT)),
        ("CUBE_SHRED_PCT_PER_STACK", pct(crate::adventure::CUBE_SHRED_PCT_PER_STACK)),
        ("CUBE_SHRED_MAX_STACKS", crate::adventure::CUBE_SHRED_MAX_STACKS.to_string()),
        ("CUBE_SHRED_MAX_PCT", pct(crate::adventure::CUBE_SHRED_PCT_PER_STACK * crate::adventure::CUBE_SHRED_MAX_STACKS as f64)),
        ("CUBE_SHRED_DURATION_S", (crate::adventure::CUBE_SHRED_DURATION_MS / 1000).to_string()),
        ("CUBE_SPLASH_TOTAL_TARGETS", (crate::adventure::CUBE_SPLASH_MAX_TARGETS + 1).to_string()),
        // Hoisted by the primary session (see WIKI_IMPACT.md) specifically
        // so this audit could wire them.
        ("DRAGON_SLOW_PCT", pct(crate::adventure::DRAGON_SLOW_MULT - 1.0)),
        ("FIRE_DEMON_HEAL_MULT_PCT", pct(1.0 - crate::adventure::FIRE_DEMON_HEAL_MULT)),
        ("CTHULHU_DEBUFF_CAP_PCT", pct(crate::adventure::CTHULHU_DEBUFF_CAP)),
        ("LICH_SUMMON_CADENCE_S", (crate::adventure::LICH_SUMMON_CADENCE_MS / 1000).to_string()),
        ("LICH_ADDS_PER_SUMMON", crate::adventure::LICH_ADDS_PER_SUMMON.to_string()),
        ("LICH_MAX_ADDS", crate::adventure::LICH_MAX_ADDS.to_string()),
        ("RECOMBINE_CRIT_CHANCE_PCT", pct(crate::adventure::RECOMBINE_CRIT_CHANCE)),
        ("PANEL_REFORGE_DUST_PER_TIER", crate::adventure::PANEL_REFORGE_DUST_PER_TIER.to_string()),
        ("POLISH_PERFECT_SAND_COST", crate::adventure::POLISH_PERFECT_SAND_COST.to_string()),
        ("POLISH_SAND_COST_PER_QUALITY_PCT", (crate::adventure::POLISH_SAND_COST_PER_QUALITY_PCT as i64).to_string()),
        // Max normal-item polish cost is ceil(100% / divisor) - derived,
        // not a separate constant, so it stays correct if the divisor
        // above ever changes.
        ("POLISH_MAX_SAND_COST", ((100.0 / crate::adventure::POLISH_SAND_COST_PER_QUALITY_PCT).ceil() as u64).to_string()),
        // Chat-command cooldowns/thresholds - bot-side values, read from
        // `published` above rather than directly (see that binding's own
        // comment) since Stage 2 moved this file game-side.
        ("BUILTIN_COOLDOWN_S", published.as_ref().map(|p| p.builtin_cooldown_secs.to_string()).unwrap_or_else(varies)),
        ("BUGREPORT_COOLDOWN_S", published.as_ref().map(|p| p.bug_report_cooldown_secs.to_string()).unwrap_or_else(varies)),
        ("VOTESKIP_COOLDOWN_S", published.as_ref().map(|p| p.song_skip_cooldown_secs.to_string()).unwrap_or_else(varies)),
        ("MIN_VOTE_VOLUME", published.as_ref().map(|p| p.min_vote_volume.to_string()).unwrap_or_else(varies)),
        ("MAX_VOTE_VOLUME", published.as_ref().map(|p| p.max_vote_volume.to_string()).unwrap_or_else(varies)),
        ("RAMPAGE_VOTE_THRESHOLD", crate::adventure::RAMPAGE_VOTE_THRESHOLD.to_string()),
        // Core combat mechanics (wiki/combat.md).
        ("CRIT_BONUS_MULT_PCT", pct(crate::adventure::CRIT_BONUS_MULT)),
        ("OVERCRIT_CURVE_A", format!("{}", crate::adventure::OVERCRIT_CURVE_A)),
        ("CRIT_CHANCE_CAP_PCT", pct(crate::adventure::CRIT_CHANCE_CAP)),
        ("BLOCK_DAMAGE_REDUCTION_PCT", pct(crate::adventure::BLOCK_DAMAGE_REDUCTION)),
        ("LIFE_LEECH_CAP_PER_SEC_PCT", pct(crate::adventure::LIFE_LEECH_CAP_PER_SEC)),
        ("PLAYER_SPLASH_MAX_TARGETS", crate::adventure::PLAYER_SPLASH_MAX_TARGETS.to_string()),
        ("ENEMY_SPLASH_MAX_TARGETS", crate::adventure::ENEMY_SPLASH_MAX_TARGETS.to_string()),
        ("SPLASH_OVERFLOW_BONUS_TARGETS", crate::adventure::SPLASH_OVERFLOW_BONUS_TARGETS.to_string()),
        ("HEAL_SPLASH_MAX_TARGETS", crate::adventure::HEAL_SPLASH_MAX_TARGETS.to_string()),
        ("ELEMENTAL_PROC_CHANCE_DIVISOR", (crate::adventure::ELEMENTAL_PROC_CHANCE_DIVISOR as i64).to_string()),
        ("ELEMENTAL_PROC_DURATION_S", (crate::adventure::ELEMENTAL_PROC_DURATION_MS / 1000).to_string()),
        ("ELEMENTAL_DEFENSE_FLOOR_PCT", pct(crate::adventure::ELEMENTAL_DEFENSE_FLOOR)),
        ("ELEMENTAL_DEFENSE_CEILING_PCT", pct(crate::adventure::ELEMENTAL_DEFENSE_CEILING)),
        ("ELEMENTAL_LIGHTNING_MAX_STACKS", crate::adventure::ELEMENTAL_LIGHTNING_MAX_STACKS.to_string()),
        ("ELEMENTAL_DIVINE_ENEMY_MAX_STACKS", crate::adventure::ELEMENTAL_DIVINE_ENEMY_MAX_STACKS.to_string()),
        ("LINGERING_EFFECT_TICK_INTERVAL_MS", crate::adventure::LINGERING_EFFECT_TICK_INTERVAL_MS.to_string()),
        ("LINGERING_EFFECT_TICKS", crate::adventure::LINGERING_EFFECT_TICKS.to_string()),
        (
            "LINGERING_EFFECT_DURATION_S",
            ((crate::adventure::LINGERING_EFFECT_TICK_INTERVAL_MS * crate::adventure::LINGERING_EFFECT_TICKS) / 1000).to_string(),
        ),
        ("MAX_FIGHT_DURATION_S", (crate::adventure::MAX_FIGHT_DURATION_MS / 1000).to_string()),
        ("REVIVE_DURATION_S", crate::adventure::REVIVE_DURATION.as_secs().to_string()),
        // Character lifecycle & progression (wiki/getting-started.md).
        ("ACTIVITY_XP_COOLDOWN_S", crate::adventure::ACTIVITY_XP_COOLDOWN.as_secs().to_string()),
        ("ACTIVITY_XP_AMOUNT", crate::adventure::ACTIVITY_XP_AMOUNT.to_string()),
        ("XP_TO_LEVEL_2", Character::xp_to_next_level(1).to_string()),
        ("XP_TO_LEVEL_11", Character::xp_to_next_level(10).to_string()),
        ("XP_TO_LEVEL_26", Character::xp_to_next_level(25).to_string()),
        ("XP_TO_LEVEL_51", Character::xp_to_next_level(50).to_string()),
        ("RETREAT_REPAIR_DURATION_MIN", (crate::adventure::RETREAT_REPAIR_DURATION.as_secs() / 60).to_string()),
        ("ARCHETYPE_CHANGE_COST", crate::adventure::ARCHETYPE_CHANGE_COST.to_string()),
        ("PASSIVE_RESPEC_COST", crate::adventure::PASSIVE_RESPEC_COST.to_string()),
        ("MODEL_CHANGE_COST", crate::adventure::MODEL_CHANGE_COST.to_string()),
        ("MODEL_CHANGES_FREE_FOR_ALL", if crate::adventure::MODEL_CHANGES_FREE_FOR_ALL { "currently free for everyone" } else { "not currently free" }.to_string()),
        ("WINGS_COST", crate::adventure::WINGS_COST.to_string()),
        ("INVENTORY_CAPACITY", crate::adventure::INVENTORY_CAPACITY.to_string()),
        ("ENCOUNTER_INTERVAL_MIN", (crate::adventure::ENCOUNTER_INTERVAL.as_secs() / 60).to_string()),
        ("BASIC_ENCOUNTER_INTERVAL_MIN", (crate::adventure::BASIC_ENCOUNTER_INTERVAL.as_secs() / 60).to_string()),
        ("FORCE_BOSS_MAX_PER_CYCLE", crate::adventure::FORCE_BOSS_MAX_PER_CYCLE.to_string()),
        ("RAMPAGE_ENCOUNTER_COUNT", crate::adventure::RAMPAGE_ENCOUNTER_COUNT.to_string()),
        ("RAMPAGE_MIN_INTERVAL_S", crate::adventure::RAMPAGE_MIN_INTERVAL.as_secs().to_string()),
        // Items & loot (wiki/items.md).
        ("EQUIP_SLOTS_COUNT", crate::adventure::EQUIP_SLOTS.len().to_string()),
        ("POWER_ROLL_MIN_PCT", pct(crate::adventure::POWER_ROLL_RANGE.start)),
        ("POWER_ROLL_MAX_PCT", pct(crate::adventure::POWER_ROLL_RANGE.end)),
        ("WEAPON_BASE_POWER", crate::adventure::base_power_for_slot(EquipSlot::Weapon).to_string()),
        ("HELM_BASE_POWER", crate::adventure::base_power_for_slot(EquipSlot::Helm).to_string()),
        ("BODY_BASE_POWER", crate::adventure::base_power_for_slot(EquipSlot::Body).to_string()),
        ("GLOVES_BASE_POWER", crate::adventure::base_power_for_slot(EquipSlot::Gloves).to_string()),
        ("BOOTS_BASE_POWER", crate::adventure::base_power_for_slot(EquipSlot::Boots).to_string()),
        ("ALL_AFFIXES_COUNT", crate::adventure::ALL_AFFIXES.len().to_string()),
        (
            "LEECH_RARITY_DIVISOR",
            (crate::adventure::affix_weight(Affix::DamageReduction) / crate::adventure::affix_weight(Affix::Leech)).round().to_string(),
        ),
        ("BOSS_ITEM_PITY_GAIN_PCT", pct(crate::adventure::BOSS_ITEM_PITY_GAIN)),
        ("BASIC_ITEM_PITY_GAIN_PCT", pct(crate::adventure::BASIC_ITEM_PITY_GAIN)),
        ("BOSS_CRAFT_PITY_GAIN_PCT", pct(crate::adventure::BOSS_CRAFT_PITY_GAIN)),
        ("BASIC_CRAFT_PITY_GAIN_PCT", pct(crate::adventure::BASIC_CRAFT_PITY_GAIN)),
        // Classes & Passives (wiki/classes.md).
        ("ARCHETYPE_COUNT", ALL_ARCHETYPES.len().to_string()),
        ("POINTS_AT_LEVEL_1", crate::passive_tree::points_for_level(1).to_string()),
        ("POINTS_AT_LEVEL_20", crate::passive_tree::points_for_level(20).to_string()),
        ("POINTS_AT_LEVEL_50", crate::passive_tree::points_for_level(50).to_string()),
    ])
}

/// Public - no login needed, same "pure reference content, same for
/// everyone" reasoning as patch-notes above (the two deliberate
/// exceptions to this dashboard's usual login gate). `/wiki` itself is
/// just a landing page/table of contents - every prose section lives at
/// `/wiki/{page}`, served by `wiki_dynamic_page` below, and Passives lives
/// at `/wiki/passives`, its own route since it's code-generated rather
/// than a `.md` file. Still resolves the session (if any) now, purely so
/// `top_nav`'s stat summary can show for a logged-in visitor - the page
/// itself stays fully viewable logged out; every page below does the same
/// for the same reason.
///
/// Old links used `/wiki#slug` fragments (chat's boss-alert links still
/// send these - see `BossKind::wiki_slug`, manager.rs). Since a fragment
/// never reaches the server, this page carries a small inline script
/// (`wiki_hash_redirect_script`) that forwards a recognized `#slug` to
/// the page that now actually contains that anchor.
pub(super) async fn wiki_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    let (content, headings) = render_markdown_page("landing");
    Html(render_page(&format!(
        "{}{}{}",
        top_nav(character.as_ref()),
        wiki_hash_redirect_script(),
        wiki_shell("landing", &headings, &content),
    )))
}

pub(super) async fn wiki_passives_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    let (content, headings) = WIKI_PASSIVES_HTML.clone();
    Html(render_page(&format!("{}{}", top_nav(character.as_ref()), wiki_shell("passives", &headings, &content))))
}

/// Serves any `wiki/{page}.md` that exists, for any `page` matching
/// `is_valid_wiki_slug` - this is the whole reason a new wiki page no
/// longer needs a route added and a rebuild deployed, only a new `.md`
/// file (and, to actually show up in nav, a line in `wiki/nav.json` - see
/// `wiki_sidebar`). 404s on an invalid slug or a missing file, never 500s
/// or falls through to something else - `is_valid_wiki_slug`'s character
/// restriction doubles as a guard against ever accidentally web-serving
/// something in `wiki/` that isn't a lowercase-hyphenated page file (the
/// dev-facing `WIKI_SYSTEM.md`/`QUESTIONS_FOR_OWNER.md` docs, for
/// instance, fail the slug check on their own uppercase/underscore names,
/// with no separate exclusion list to maintain).
pub(super) async fn wiki_dynamic_page(State(state): State<AppState>, headers: HeaderMap, Path(page): Path<String>) -> (StatusCode, Html<String>) {
    if !is_valid_wiki_slug(&page) || !std::path::Path::new(&format!("{WIKI_MD_DIR}/{page}.md")).exists() {
        let not_found = "<div class=\"card\"><h1>Wiki</h1><p>That wiki page doesn't exist.</p><p class=\"muted\"><a href=\"/wiki\">&larr; All wiki sections</a></p></div>";
        return (StatusCode::NOT_FOUND, Html(render_page(&wiki_shell("", &[], not_found))));
    }
    let character = resolve_wiki_character(&headers, &state).await;
    let (content, headings) = render_markdown_page(&page);
    (StatusCode::OK, Html(render_page(&format!("{}{}", top_nav(character.as_ref()), wiki_shell(&page, &headings, &content)))))
}

/// ASCII lowercase letters, digits, and hyphens only - matches every real
/// page filename in `wiki/` (`getting-started`, `bosses`, ...) and nothing
/// else. No `.`/`/`/`\` means no path traversal is expressible at all,
/// not just "checked for" - `format!("{WIKI_MD_DIR}/{name}.md")` in
/// `render_markdown_page` can only ever resolve inside `wiki/`.
fn is_valid_wiki_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

async fn resolve_wiki_character(headers: &HeaderMap, state: &AppState) -> Option<Character> {
    match current_session(headers, state).await {
        Some((login, _)) => state.adventure.character(&login).await,
        None => None,
    }
}

/// One nav entry, deserialized straight from `wiki/nav.json` - `label`
/// carries its own emoji prefix (e.g. `"🚀 Getting Started"`) so the
/// manifest is the one place both the icon and the text live. Sidebar
/// order is manifest order - reordering pages is a `nav.json` edit, not a
/// code change (requirement from the 2026-08-19 sidebar redesign: "order
/// comes from nav.json so I can reorder by editing the file, no rebuild").
#[derive(serde::Deserialize)]
struct WikiNavEntry {
    slug: String,
    label: String,
}

/// The persistent left-hand docs sidebar every wiki page shares - driven
/// by `wiki/nav.json`, current page highlighted, with that page's own
/// `<h2>`/`<h3>` headings (see `extract_and_inject_heading_ids`) rendered
/// as a collapsible `<details>` sub-list right under its entry. Only the
/// ACTIVE entry gets a heading sub-list - the other pages' headings aren't
/// even computed for this request, only the one actually being rendered
/// (see `wiki_dynamic_page`/`wiki_page`/`wiki_passives_page`, which each
/// already have their own page's `(content, headings)` in hand).
///
/// Missing/unparseable manifest just means an empty nav list, not a 500 -
/// `load_json` already logs a parse error. Includes the mobile hamburger
/// toggle button and its backdrop - both `display: none` above the
/// `.wiki-shell` mobile breakpoint (see `templates/base.html`), so they're
/// inert dead weight on desktop rather than a second code path.
fn wiki_sidebar(active: &str, headings: &[WikiHeading]) -> String {
    let entries: Vec<WikiNavEntry> = crate::state::load_json(format!("{WIKI_MD_DIR}/nav.json")).unwrap_or_default();
    let items: String = entries
        .iter()
        .map(|e| {
            if e.slug == active {
                let toc: String = headings
                    .iter()
                    .map(|h| {
                        let class = if h.level == 2 { " class=\"wiki-toc-h2\"" } else { "" };
                        format!("<li{class}><a href=\"#{}\">{}</a></li>", h.id, h.text)
                    })
                    .collect();
                format!("<li><details class=\"wiki-toc\" open><summary>{}</summary><ul class=\"wiki-toc-list\">{toc}</ul></details></li>", e.label)
            } else {
                format!("<li><a class=\"wiki-nav-link\" href=\"/wiki/{}\">{}</a></li>", e.slug, e.label)
            }
        })
        .collect();
    format!(
        "<button type=\"button\" class=\"wiki-sidebar-toggle\" aria-label=\"Toggle wiki menu\" onclick=\"\
          document.getElementById('wiki-sidebar').classList.toggle('wiki-sidebar--open');\
          document.getElementById('wiki-sidebar-backdrop').classList.toggle('wiki-sidebar--open');\
        \">&#9776; Wiki Menu</button>\
        <div class=\"wiki-sidebar-backdrop\" id=\"wiki-sidebar-backdrop\" onclick=\"\
          this.classList.remove('wiki-sidebar--open');\
          document.getElementById('wiki-sidebar').classList.remove('wiki-sidebar--open');\
        \"></div>\
        <nav class=\"wiki-sidebar\" id=\"wiki-sidebar\">\
          <div class=\"wiki-sidebar-home\"><a href=\"/wiki\">&#128214; Wiki</a> &middot; <a href=\"/\">&larr; Your character</a></div>\
          <ul class=\"wiki-nav-list\">{items}</ul>\
        </nav>"
    )
}

/// Wraps a page's rendered content with the sidebar into the
/// `.wiki-shell` two-column layout (see `templates/base.html` for the
/// CSS) - every wiki page (landing, passives, and every dynamic `.md`
/// page) goes through this one function, so the shell markup only exists
/// in one place.
fn wiki_shell(active: &str, headings: &[WikiHeading], content_html: &str) -> String {
    format!("<div class=\"wiki-shell\">{}<div class=\"wiki-content\">{}</div></div>", wiki_sidebar(active, headings), content_html)
}

/// Forwards an old `/wiki#slug` fragment link to whichever page now owns
/// that anchor. The map's keys are exactly the `id="..."` values
/// `wiki/bosses.md`/`wiki/crafting.md`/`wiki/healing.md` define - keep in
/// sync if a page ever adds/renames an anchor. Fragments never reach the
/// server, so this has to run client-side.
fn wiki_hash_redirect_script() -> String {
    "<script>(function(){\
      var slug = location.hash.slice(1);\
      if (!slug) return;\
      var section = ({\
        dragon:'bosses',cthulhu:'bosses',lich:'bosses','fire-demon':'bosses','gelatinous-cube':'bosses',\
        currencies:'crafting',ceiling:'crafting','currency-crafting':'crafting',reforge:'crafting',recombine:'crafting',\
        polishing:'crafting','celestial-shard':'crafting','item-tiers':'crafting',veiling:'crafting',\
        disenchanting:'crafting','quick-reference':'crafting',\
        shields:'healing'\
      })[slug];\
      if (section) location.replace('/wiki/' + section + location.hash);\
    })();</script>"
        .to_string()
}

/// Public, read-only reference for every class's full passive tree - the
/// interactive `/passives` page (`render_passive_tree_page`) needs a
/// logged-in character (it's driven by THEIR current ranks/unlock state),
/// so it can't double as a "just let me look at the whole tree" page for
/// a logged-out visitor or someone comparing classes before picking one.
/// This instead renders the SAME node-graph component (`compute_passive_layout`/
/// `root_node_html`, the exact boxes-and-SVG-connectors tree `/passives`
/// itself draws) per class, one collapsible section each - not a
/// hand-duplicated text summary, so the two views can never drift apart.
///
/// Unlike the prose sections (which live in `wiki/*.md` and are re-read
/// per request, see `render_markdown_page`), this has no external input
/// at all - every class's `passive_nodes()` and `compute_passive_layout`
/// are pure functions of compiled-in data, so the output is identical on
/// every call for the life of the process. `WIKI_PASSIVES_HTML` builds it
/// once, on first request, instead of recomputing 11 full tree layouts
/// (SVG connectors and all) on every single visit to any wiki page - the
/// heading extraction (for the sidebar's in-page nav, same as every
/// markdown page gets) rides along in that same one-time cost. Real
/// headings here: the intro card's "Class Passive Trees" `<h2>` and the
/// "Memories" `<h2>` (2026-08-19) - archetype sections themselves are
/// `<details>`/`<summary>`, not headings, so they don't add more.
static WIKI_PASSIVES_HTML: LazyLock<(String, Vec<WikiHeading>)> = LazyLock::new(|| {
    let sections: String = ALL_ARCHETYPES.iter().map(|&archetype| render_wiki_archetype_graph(archetype)).collect();
    let html = format!(
        "<div class=\"card\">\
          <h2>Class Passive Trees</h2>\
          <p>Every class has its own tree: 1 root passive (always active, scales with level), 3 Skills you can rank up freely, \
          9 Specializations (3 per Skill, gated behind their Skill), and 27 Modifiers (3 per Specialization, gated behind pushing \
          that Specialization to 4/4 - the 4th point \"specializes\" it without adding another point of its own stat, but unlocks \
          its 3 Modifiers below). Shown fully unlocked below so every node is visible at once - a node flagged \
          <span class=\"muted\">(inactive)</span> doesn't do anything yet, though points spent there are banked, not wasted, and \
          will start working the day that mechanic ships.</p>\
          <p class=\"muted\">Node values can also be tuned live by the streamer without a redeploy. A node currently running a \
          different number than its own description text is flagged with a \"Tuned: X (default Y)\" note beside that description - \
          treat that note as the real, current number whenever the two disagree.</p>\
        </div>\
        <div class=\"card\">\
          <h2>Memories: Save &amp; Swap Full Builds</h2>\
          <p>Every character gets {memory_slots} Memory slots, managed right here on this page. A Memory saves your entire build - \
          archetype, full passive allocation, Split Personality's second archetype and tree if you have one, and (for Elementalists) \
          your golem loadout - so you can save a build, respec into something else, and load the saved one back exactly as it was.</p>\
          <p>Saving and loading are both completely free, bypassing the normal {archetype_cost} dust archetype-change cost and \
          {respec_cost} dust passive-respec cost entirely once a build is saved.</p>\
          <p>Loading only works out of combat - no mid-fight swaps. If you've earned levels since a Memory was saved, the extra \
          points it doesn't spend are left for you to place yourself, never auto-spent.</p>\
        </div>{sections}",
        memory_slots = crate::adventure::STARTING_MEMORY_SLOTS,
        archetype_cost = crate::adventure::ARCHETYPE_CHANGE_COST,
        respec_cost = crate::adventure::PASSIVE_RESPEC_COST,
    );
    extract_and_inject_heading_ids(&html)
});

/// One class's full tree, drawn with the exact same graph renderer the
/// interactive `/passives` page uses - see `WIKI_PASSIVES_HTML`'s doc.
/// Since this has no real character behind it, `allocations` is a
/// synthetic "every node maxed" map (every skill/spec/modifier's own
/// `max_rank`) rather than anyone's real ranks - the ONLY purpose here is
/// to make `compute_passive_layout` treat every Specialization as
/// unlocked (so every Modifier row actually gets drawn instead of hidden
/// behind its normal in-game gate), and it doubles as a natural "here's
/// what a fully-invested tree looks like" reference view. Read-only, same
/// "no forms, plain rank text" shape as `render_passive_tree_readonly`'s
/// own node_html.
fn render_wiki_archetype_graph(archetype: Archetype) -> String {
    let (icon, role) = passive_archetype_icon_role(archetype);
    let role_class = archetype.css_class();
    let nodes = archetype.passive_nodes();
    let allocations: HashMap<String, u32> = nodes.iter().map(|n| (n.key.to_string(), n.max_rank)).collect();

    let layout = compute_passive_layout(archetype, &allocations);
    let PassiveLayout { positioned, root_x, svg_lines, stage_w, stage_h } = layout;

    let node_html = |p: &PosNode| -> String {
        let n = &p.node;
        let rank = n.max_rank;
        let not_yet = matches!(n.effect, crate::passive_tree::PassiveEffect::NotYetImplemented);
        let tip = if not_yet {
            format!("{} Not yet active - allocating still banks the point for when this mechanic ships.", escape_html(n.description))
        } else {
            escape_html(n.description)
        };
        let tip = match passive_override_note(n) {
            Some(note) => format!("{tip} {note}"),
            None => tip,
        };
        let (kind_class, kind_label) = match n.tier {
            PassiveTier::Skill => ("node-skill", "Tier 1"),
            PassiveTier::Specialization => ("node-spec", "Specialization"),
            PassiveTier::Modifier => ("node-mod", ""),
        };
        let state_class = if n.max_rank == 4 { " node--specialized" } else { " node--maxed" };
        let dots: String = (0..n.max_rank)
            .map(|i| {
                let gold = n.max_rank == 4 && i == 3;
                format!("<span class=\"dot filled{}\"></span>", if gold { " dot-spec" } else { "" })
            })
            .collect();
        let kind_label_html = if kind_label.is_empty() { String::new() } else { format!("<div class=\"node-kind\">{kind_label}</div>") };
        format!(
            "<div class=\"node {kind_class}{state_class}\" style=\"left:{x}px;top:{y}px;width:{w}px;\" data-tip=\"{tip}\">\
              {kind_label_html}\
              <div class=\"node-name\">{name}{flag}</div>\
              <div class=\"dots\">{dots}</div>\
              <div class=\"node-buttons\"><span class=\"node-rank\">{rank}/{max_rank}</span></div>\
            </div>",
            x = p.x,
            y = p.y,
            w = p.w,
            name = escape_html(n.name),
            flag = if not_yet { " <span class=\"muted\">(inactive)</span>" } else { "" },
            max_rank = n.max_rank,
        )
    };

    let root_desc = escape_html(&archetype.description(1));
    let root_html = root_node_html(archetype, root_x, &root_desc);
    let nodes_html: String = positioned.iter().map(node_html).collect();

    format!(
        "<details class=\"card bag-row wiki-archetype\">\
          <summary>{icon} {archetype:?} <span class=\"role-badge role-{role_class}\">{role}</span></summary>\
          <p class=\"wiki-root-desc\"><strong>Root passive (always active):</strong> {root_desc} <span class=\"muted\">- shown at level 1; grows with level.</span></p>\
          <div class=\"ptree-page\">\
            <div class=\"tree-wrap\"><div style=\"width:{stage_w}px;height:{stage_h}px;position:relative;\">\
              <svg class=\"connectors\" width=\"{stage_w}\" height=\"{stage_h}\">{svg_lines}</svg>\
              {root_html}{nodes_html}\
            </div></div>\
          </div>\
        </details>",
    )
}
