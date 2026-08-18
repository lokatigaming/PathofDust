// The `/wiki` page - extracted verbatim from adventure_web.rs (2026-08-18,
// a pure code-motion refactor: zero behavior change, no renames, no
// cleanup). Every helper it leans on (top_nav, render_page,
// current_session, escape_html, compute_passive_layout, root_node_html,
// passive_archetype_icon_role, ...) is shared with other pages and
// deliberately stayed in the parent - reached here through `use super::*`,
// the same convention src/adventure/*.rs already uses to see adventure.rs.

use super::*;
use regex::Regex;
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

/// Base directory the prose sections (bosses/crafting/healing/landing)
/// are authored in as real Markdown - relative to CWD, same convention
/// `patch-notes.json` already uses (see `patch_notes` in adventure_web.rs)
/// so it resolves the same way in dev and prod without an absolute path
/// baked in. Passives are the deliberate exception - that section is
/// fully code-generated (see `render_wiki_passives`) and has no .md file.
const WIKI_MD_DIR: &str = "wiki";

struct CachedMdPage {
    mtime: SystemTime,
    rendered: String,
}

/// One cache slot per prose page, keyed by its markdown filename (no
/// extension). Guarded by mtime, not a TTL: editing a .md file on disk
/// takes effect on the very next request that notices the new mtime, no
/// rebuild or restart - the whole point of moving this content out of
/// Rust string literals. Reset for free on process restart along with
/// everything else, so there's no explicit invalidation path to maintain.
static WIKI_MD_CACHE: LazyLock<Mutex<HashMap<&'static str, CachedMdPage>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Loads, Markdown-renders, and placeholder-substitutes `wiki/{name}.md`,
/// caching the rendered HTML until the file's mtime changes. `name` has
/// no path separators or extension (e.g. `"bosses"`) - always a literal
/// from this module, never anything request-derived, so there's no path
/// traversal surface here despite building a filesystem path from it.
fn render_markdown_page(name: &'static str) -> String {
    let path = format!("{WIKI_MD_DIR}/{name}.md");
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);

    let mut cache = WIKI_MD_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(name) {
        if cached.mtime == mtime {
            return cached.rendered.clone();
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

    cache.insert(name, CachedMdPage { mtime, rendered: html_out.clone() });
    html_out
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
        // Chat-command cooldowns/thresholds - these three were private
        // consts in their own top-level modules (not adventure::) until
        // this pass flipped them to pub(crate) specifically so they could
        // be wired here; `song_requests`'s vote-volume bounds were already
        // pub in a pub mod, so no visibility change was needed for those.
        ("BUILTIN_COOLDOWN_S", crate::commands::BUILTIN_COOLDOWN.as_secs().to_string()),
        ("BUGREPORT_COOLDOWN_S", crate::bug_reports::PER_USER_COOLDOWN.as_secs().to_string()),
        ("VOTESKIP_COOLDOWN_S", crate::song_requests::SKIP_ACTION_COOLDOWN.as_secs().to_string()),
        ("MIN_VOTE_VOLUME", crate::song_requests::MIN_VOTE_VOLUME.to_string()),
        ("MAX_VOTE_VOLUME", crate::song_requests::MAX_VOTE_VOLUME.to_string()),
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
    ])
}

/// Public - no login needed, same "pure reference content, same for
/// everyone" reasoning as patch-notes above (the two deliberate
/// exceptions to this dashboard's usual login gate). `/wiki` itself is
/// now just a landing page/table of contents - the actual sections live
/// at `/wiki/bosses`, `/wiki/crafting`, `/wiki/healing`, `/wiki/passives`
/// (see the handlers below), split out so the passives section (which
/// draws all 11 classes' full trees) doesn't have to render on every
/// visit to any other section. Still resolves the session (if any) now,
/// purely so `top_nav`'s stat summary can show for a logged-in visitor -
/// the page itself stays fully viewable logged out; every sub-page below
/// does the same for the same reason.
///
/// Old links used `/wiki#slug` fragments (chat's boss-alert links still
/// send these - see `BossKind::wiki_slug`, manager.rs). Since a fragment
/// never reaches the server, this page carries a small inline script
/// (`wiki_hash_redirect_script`) that forwards a recognized `#slug` to
/// the sub-page that now actually contains that anchor.
pub(super) async fn wiki_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/", "Back to your character"),
        wiki_hash_redirect_script(),
        wiki_subnav("landing"),
        render_wiki_toc(),
    )))
}

pub(super) async fn wiki_bosses_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("bosses"),
        render_wiki_bosses(),
    )))
}

pub(super) async fn wiki_crafting_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("crafting"),
        render_wiki_crafting(),
    )))
}

pub(super) async fn wiki_healing_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("healing"),
        render_wiki_healing(),
    )))
}

pub(super) async fn wiki_passives_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("passives"),
        render_wiki_passives(),
    )))
}

pub(super) async fn wiki_commands_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("commands"),
        render_markdown_page("commands"),
    )))
}

pub(super) async fn wiki_combat_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("combat"),
        render_markdown_page("combat"),
    )))
}

pub(super) async fn wiki_getting_started_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = resolve_wiki_character(&headers, &state).await;
    Html(render_page(&format!(
        "{}{}{}{}",
        top_nav(character.as_ref()),
        wiki_crumb("/wiki", "All wiki sections"),
        wiki_subnav("getting-started"),
        render_markdown_page("getting-started"),
    )))
}

async fn resolve_wiki_character(headers: &HeaderMap, state: &AppState) -> Option<Character> {
    match current_session(headers, state).await {
        Some((login, _)) => state.adventure.character(&login).await,
        None => None,
    }
}

/// Shared "Wiki" breadcrumb card every wiki page (landing + sub-pages)
/// opens with - `back_href`/`back_label` vary since the landing page
/// backs out to `/` while every sub-page backs out to `/wiki`.
fn wiki_crumb(back_href: &str, back_label: &str) -> String {
    format!("<div class=\"card\"><h1>Wiki</h1><p class=\"muted\"><a href=\"{back_href}\">&larr; {back_label}</a></p></div>")
}

/// Small nav between the 4 wiki sections, shown on every wiki page
/// (landing included, so it doubles as a shortcut past the ToC). Reuses
/// `top_nav`'s own `.top-nav-links`/`.top-nav-link` classes verbatim
/// instead of introducing new CSS - the active page renders as plain
/// bold text (no href) rather than a new "active link" style.
fn wiki_subnav(active: &str) -> String {
    let entry = |path: &str, label: &str, key: &str| -> String {
        if key == active {
            format!("<strong class=\"top-nav-link\">{label}</strong>")
        } else {
            format!("<a class=\"top-nav-link\" href=\"{path}\">{label}</a>")
        }
    };
    format!(
        "<div class=\"top-nav-links\">{}{}{}{}{}{}{}</div>",
        entry("/wiki/getting-started", "🚀 Getting Started", "getting-started"),
        entry("/wiki/combat", "⚔️ Combat", "combat"),
        entry("/wiki/bosses", "🐲 Bosses", "bosses"),
        entry("/wiki/crafting", "⚒️ Crafting", "crafting"),
        entry("/wiki/healing", "✨ Healing", "healing"),
        entry("/wiki/passives", "🌳 Passives", "passives"),
        entry("/wiki/commands", "💬 Commands", "commands"),
    )
}

/// `/wiki` landing page's table of contents - content lives in
/// `wiki/landing.md` (see `render_markdown_page`).
fn render_wiki_toc() -> String {
    render_markdown_page("landing")
}

/// Forwards an old `/wiki#slug` fragment link to whichever sub-page now
/// owns that anchor. The map's keys are exactly the `id=\"...\"` values
/// `render_wiki_bosses`/`render_wiki_crafting`/`render_wiki_healing`
/// define - keep in sync if a section ever adds/renames an anchor.
/// Fragments never reach the server, so this has to run client-side.
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
/// (SVG connectors and all) on every single visit to any wiki sub-page.
static WIKI_PASSIVES_HTML: LazyLock<String> = LazyLock::new(|| {
    let sections: String = ALL_ARCHETYPES.iter().map(|&archetype| render_wiki_archetype_graph(archetype)).collect();
    format!(
        "<div class=\"card\">\
          <h2>Class Passive Trees</h2>\
          <p>Every class has its own tree: 1 root passive (always active, scales with level), 3 Skills you can rank up freely, \
          9 Specializations (3 per Skill, gated behind their Skill), and 27 Modifiers (3 per Specialization, gated behind pushing \
          that Specialization to 4/4 - the 4th point \"specializes\" it without adding another point of its own stat, but unlocks \
          its 3 Modifiers below). Shown fully unlocked below so every node is visible at once - a node flagged \
          <span class=\"muted\">(inactive)</span> doesn't do anything yet, though points spent there are banked, not wasted, and \
          will start working the day that mechanic ships.</p>\
        </div>{sections}"
    )
});

fn render_wiki_passives() -> String {
    WIKI_PASSIVES_HTML.clone()
}

/// One class's full tree, drawn with the exact same graph renderer the
/// interactive `/passives` page uses - see `render_wiki_passives`'s doc.
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

/// Hand-written, deliberately non-technical - a curious viewer or a
/// player deciding whether to !join should be able to read this without
/// knowing anything about how the game's actually coded. Content lives
/// in `wiki/bosses.md` (see `render_markdown_page`); numbers there
/// (thresholds, add counts, percentages) are pulled from adventure.rs's
/// own constants (TWO_BOSS_STAGE/THREE_BOSS_STAGE, LICH_MAX_ADDS, the
/// Fire Demon/Dragon aura magnitudes, Cthulhu's -90%) - keep the .md in
/// sync if any of those ever change (`{{PLACEHOLDER}}`s aside).
fn render_wiki_bosses() -> String {
    render_markdown_page("bosses")
}

/// Second wiki section - the crafting/item system, ported from the
/// standalone "Crafting Codex" reference doc into the in-game wiki so
/// players don't need a separate link to find it. Content lives in
/// `wiki/crafting.md` (see `render_markdown_page`); numbers there (dust
/// costs, crit chances, disenchant multipliers) are transcribed from the
/// actual constants/formulas in `item.rs`/`character.rs`/`manager.rs`,
/// same maintenance contract as the boss section above, except where
/// they're wired up as `{{PLACEHOLDER}}`s instead.
fn render_wiki_crafting() -> String {
    render_markdown_page("crafting")
}

/// Third wiki section - general combat mechanics that apply across every
/// archetype, positioned right before Passives so a reader hits the
/// shared rules before running into each archetype's own shield-granting
/// skills (Divine Shield, Overflowing Grace, Arcane Shield, Consecration,
/// Seed of Life) down there. Content lives in `wiki/healing.md` (see
/// `render_markdown_page`).
fn render_wiki_healing() -> String {
    render_markdown_page("healing")
}
