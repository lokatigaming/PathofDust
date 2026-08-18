//! Stage 0.5 golden-battle corpus (2026-08-18, architecture refactor
//! harness #1) - the safety net for the single highest silent-behavior-
//! change risk in the whole refactor project: `simulate_battle`/
//! `apply_hit`/`resolve_hit`'s many inlined, order-dependent passive-hook
//! blocks (now including the boss-pierce split, which is a genuine
//! mid-pipeline dependency, not a clean wrapper - see
//! `resolve_hit`'s own doc). Reused by every combat.rs decomposition
//! sub-stage from Stage 8+ onward: run this module, diff against the
//! committed fixtures in `tests/fixtures/golden_corpus/`, and any silent
//! change in a fight's exact outcome shows up as a failing test instead
//! of a live report weeks later.
//!
//! **Determinism, and why every scenario here is solo (one character)**:
//! `simulate_battle` takes `characters: &HashMap<String, Character>` and
//! builds its initial unit list via `characters.iter()` - std's default
//! `HashMap` iteration order is randomized per-process (a different
//! random seed each run), so with more than one character, WHICH player
//! a boss's "random alive player" targeting or first-mover-tie-break
//! picks can differ run-to-run even with an identical seeded `rng`
//! passed to `simulate_battle` itself. A solo party (exactly one
//! `Character`) sidesteps this entirely - there's only ever one possible
//! target/mover, so the whole fight is fully reproducible from the seed
//! alone. This is a real, deliberate scoping limit, not an oversight:
//! multi-player-specific mechanics (Intervene, party-heal-lowest-ally
//! targeting, Pack Instinct/Symbiosis, curse-splitting across several
//! targets) aren't covered by exact-snapshot comparison here. Making
//! them reproducible would mean either threading a custom deterministic
//! hasher through `simulate_battle`'s own signature (a bigger production
//! change than this harness-building stage should make unilaterally) or
//! accepting non-determinism - left as a follow-up, not solved here.
//!
//! Boss stats are hand-authored fixed values, NOT `boss_stats_for`/
//! `basic_enemy_stats_for` - those two apply their own un-seeded
//! `rand::thread_rng()` jitter to the roll (see `boss_stats_for`'s
//! `jitter` local), which would reintroduce the same run-to-run
//! variance this module exists to eliminate. This trades "uses the
//! exact live stat-scaling formula" fidelity for reproducibility; the
//! formula itself is simple, low-risk arithmetic, separately
//! unit-testable if that's ever needed - the real regression risk this
//! corpus targets is the SIMULATION logic, not the stat formula.
//!
//! Run `cargo test golden_corpus` to exercise every scenario. A missing
//! fixture file is treated as "first capture" and written automatically
//! (that's how this corpus was seeded, Stage 0 execution, 2026-08-18);
//! an existing fixture is compared for exact equality, failing loudly on
//! any difference.

use super::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// No `PartialEq` derive - `CombatEvent`/`CombatUnitInfo`/`RollEvent`
// don't derive it themselves (adding it purely for this test felt like
// more production-surface churn than warranted), so comparison below
// goes through `serde_json::Value` equality instead, which every
// serializable type gets for free.
#[derive(Serialize, Deserialize, Debug)]
struct GoldenSnapshot {
    won: bool,
    units: Vec<CombatUnitInfo>,
    events: Vec<CombatEvent>,
    rolls: Vec<RollEvent>,
}

struct Scenario {
    name: &'static str,
    seed: u64,
    stage: u32,
    archetype: Archetype,
    level: u32,
    boss_kind: Option<BossKind>,
    boss: BossStats,
}

fn boss(hp: u64, atk: u64, attack_interval_ms: u32) -> BossStats {
    BossStats { hp, atk, attack_interval_ms, ..Default::default() }
}

fn tough_boss(hp: u64, atk: u64, attack_interval_ms: u32) -> BossStats {
    // A boss with real secondary stats invested (evasion/block/DR/crit) -
    // exercises the defender-side mitigation combine, not just raw
    // HP/ATK racing, on top of whatever the seed's own solo character
    // brings to the attacker side.
    BossStats { hp, atk, attack_interval_ms, damage_reduction: 0.25, block_chance: 0.2, evasion: 0.15, increased_damage: 0.1, crit_chance: 0.15, crit_multiplier: 2.5, splash: 0.0 }
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "warrior_vs_lich_stage50", seed: 1, stage: 50, archetype: Archetype::Warrior, level: 10, boss_kind: Some(BossKind::Lich), boss: boss(8_000, 150, 1100) },
        Scenario { name: "rogue_vs_generic_stage50", seed: 2, stage: 50, archetype: Archetype::Rogue, level: 10, boss_kind: None, boss: boss(8_000, 150, 1100) },
        Scenario { name: "mage_vs_cthulhu_stage200", seed: 3, stage: 200, archetype: Archetype::Mage, level: 25, boss_kind: Some(BossKind::Cthulhu), boss: boss(40_000, 400, 1100) },
        Scenario { name: "cleric_vs_tough_boss_stage200", seed: 4, stage: 200, archetype: Archetype::Cleric, level: 25, boss_kind: Some(BossKind::FireDemon), boss: tough_boss(40_000, 400, 1100) },
        Scenario { name: "warlock_vs_dragon_stage500", seed: 5, stage: 500, archetype: Archetype::Warlock, level: 50, boss_kind: Some(BossKind::Dragon), boss: boss(120_000, 900, 1100) },
        Scenario { name: "paladin_vs_cube_stage500", seed: 6, stage: 500, archetype: Archetype::Paladin, level: 50, boss_kind: Some(BossKind::GelatinousCube), boss: boss(120_000, 900, 1100) },
        // High stage - exercises the late-stage damage penalty AND boss
        // pierce together (see WIKI_IMPACT.md's pierce entry - both are
        // stage^2/(stage^2+h^2)-shaped ramps, both real at this stage).
        Scenario { name: "ranger_vs_lich_stage3000", seed: 7, stage: 3000, archetype: Archetype::Ranger, level: 80, boss_kind: Some(BossKind::Lich), boss: boss(2_000_000, 6_000, 1100) },
        Scenario { name: "slayer_vs_tough_boss_stage3000", seed: 8, stage: 3000, archetype: Archetype::Slayer, level: 80, boss_kind: Some(BossKind::Dragon), boss: tough_boss(2_000_000, 6_000, 1100) },
        Scenario { name: "druid_vs_generic_stage1000", seed: 9, stage: 1000, archetype: Archetype::Druid, level: 60, boss_kind: None, boss: boss(500_000, 2_000, 1100) },
        Scenario { name: "monk_vs_fire_demon_stage1000", seed: 10, stage: 1000, archetype: Archetype::Monk, level: 60, boss_kind: Some(BossKind::FireDemon), boss: boss(500_000, 2_000, 1100) },
        Scenario { name: "berserker_vs_lich_stage1000", seed: 11, stage: 1000, archetype: Archetype::Berserker, level: 60, boss_kind: Some(BossKind::Lich), boss: boss(500_000, 2_000, 1100) },
    ]
}

fn run_scenario(s: &Scenario) -> GoldenSnapshot {
    // `Character::new` generates its own "fully kitted out" starting gear
    // internally via an UN-seeded `rand::thread_rng()` (see its own doc) -
    // the first, less obvious source of run-to-run non-determinism this
    // corpus hit (the `simulate_battle`/`boss_stats_for` sources were the
    // obvious ones). Sidestepped the same way as those: build the
    // character with empty slots, then equip deterministic gear via
    // `generate_item` directly, using the SAME seeded `rng` combat itself
    // will continue drawing from - one seed drives the entire scenario
    // end to end, gear roll included.
    let mut rng = StdRng::seed_from_u64(s.seed);
    let mut character = Character::new(s.name.to_string());
    character.archetype = s.archetype;
    character.level = s.level;
    character.weapon = Some(generate_item(EquipSlot::Weapon, s.stage, &mut rng));
    character.helm = Some(generate_item(EquipSlot::Helm, s.stage, &mut rng));
    character.body = Some(generate_item(EquipSlot::Body, s.stage, &mut rng));
    character.gloves = Some(generate_item(EquipSlot::Gloves, s.stage, &mut rng));
    character.boots = Some(generate_item(EquipSlot::Boots, s.stage, &mut rng));

    let mut characters: HashMap<String, Character> = HashMap::new();
    characters.insert(s.name.to_string(), character);

    let tunables = LiveTunables::default();
    let (won, units, events, rolls) = simulate_battle(&characters, vec![(s.boss.clone(), s.boss_kind, 1.0)], s.stage, &tunables, &mut rng);
    GoldenSnapshot { won, units, events, rolls }
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new("tests/fixtures/golden_corpus").join(format!("{name}.json"))
}

/// Structural equality with two deliberate escape hatches - everything
/// else (object keys/shape, array length/order, strings, bools, every
/// OTHER integer) stays exact:
///
/// 1. A tolerance for floating-point LEAVES. A handful of raw, un-
///    rounded `f64` fields in the full-detail roll log (`RollEvent::
///    magnitude`/`probability`, `CombatEvent::BuffSnapshot`'s stack
///    magnitudes) can land 1 ULP apart between two runs of the
///    identical seeded simulation - confirmed, empirically, NOT a real
///    behavior difference: the same pure computation
///    (`boss_defense_ignore`, tested in isolation) is perfectly
///    bit-stable across repeated process runs with fixed inputs, so the
///    1-ULP drift has to come from a difference in surrounding codegen
///    (inlining/register allocation context) when the identical
///    expression is compiled inside `simulate_battle`'s much larger
///    function body versus a tiny standalone caller - not a game-logic
///    bug. Every GAMEPLAY-facing number (`damage`, `hp`, `xp`, ...) is
///    already `.round()`-ed to an integer before it ever reaches a
///    `CombatEvent`/`CombatUnitInfo`, so this only ever matters for the
///    full-detail roll log's own raw diagnostic values.
/// 2. `eventId`/`hitId` fields are skipped entirely (any value accepted)
///    - both ride a single `static AtomicU64` shared by the WHOLE test
///    binary (see `next_hit_id`'s own doc: "a counter that keeps
///    climbing across fights/restarts is fine - nothing reads it as
///    'hit number N of this fight'"), so their ABSOLUTE value depends on
///    how many hits every OTHER test running in the same process
///    happened to roll first - meaningless to pin down, and never
///    meant to be stable even in production. Their CORRELATION role
///    (same hit_id linking an Attack event to its RollEvents) is still
///    fully verified, since it shows up as matching POSITION in these
///    already-array-order-compared event/roll lists, not as a specific
///    number.
///
/// Real regressions (a formula changing, a trigger condition flipping,
/// event ordering/count changing) still fail loudly - they move numbers
/// by far more than float noise, and touch fields other than these two
/// counters.
fn approx_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    const EPSILON: f64 = 1e-9;
    match (a, b) {
        (serde_json::Value::Number(na), serde_json::Value::Number(nb)) => match (na.as_f64(), nb.as_f64()) {
            (Some(fa), Some(fb)) => (fa - fb).abs() <= EPSILON * fa.abs().max(fb.abs()).max(1.0),
            _ => na == nb,
        },
        (serde_json::Value::Object(oa), serde_json::Value::Object(ob)) => {
            oa.len() == ob.len()
                && oa.iter().all(|(k, va)| if k == "eventId" || k == "hitId" { ob.contains_key(k) } else { ob.get(k).is_some_and(|vb| approx_eq(va, vb)) })
        }
        (serde_json::Value::Array(aa), serde_json::Value::Array(ba)) => aa.len() == ba.len() && aa.iter().zip(ba.iter()).all(|(x, y)| approx_eq(x, y)),
        _ => a == b,
    }
}

#[test]
fn golden_corpus_matches_committed_fixtures() {
    let mut first_capture: Vec<&str> = Vec::new();
    let mut mismatched: Vec<String> = Vec::new();

    for scenario in scenarios() {
        let fresh = run_scenario(&scenario);
        let path = fixture_path(scenario.name);

        if !path.exists() {
            std::fs::create_dir_all(path.parent().unwrap()).expect("failed to create tests/fixtures/golden_corpus");
            let json = serde_json::to_string_pretty(&fresh).expect("GoldenSnapshot must serialize");
            std::fs::write(&path, json).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
            first_capture.push(scenario.name);
            continue;
        }

        let existing_json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let existing: serde_json::Value = serde_json::from_str(&existing_json).unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
        let fresh_value = serde_json::to_value(&fresh).expect("GoldenSnapshot must serialize to a Value");

        if !approx_eq(&existing, &fresh_value) {
            mismatched.push(scenario.name.to_string());
        }
    }

    if !first_capture.is_empty() {
        println!("golden_corpus: captured {} new baseline fixture(s): {:?}", first_capture.len(), first_capture);
    }
    assert!(
        mismatched.is_empty(),
        "golden_corpus: {} scenario(s) diverged from their committed fixture - a deliberate balance/mechanic \
         change should regenerate the fixture (delete it under tests/fixtures/golden_corpus/ and rerun), \
         an UNINTENDED change during refactor should NOT: {:?}",
        mismatched.len(),
        mismatched
    );
}
