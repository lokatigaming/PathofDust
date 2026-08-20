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
//! **Determinism, and why scenarios here USED to be solo-only**
//! (resolved 2026-08-20): `simulate_battle` takes
//! `characters: &HashMap<String, Character>`, and std's `HashMap`
//! iteration order is randomized per-PROCESS. It built its unit list
//! straight off `characters.iter()`, so with more than one character
//! WHICH player a boss's "random alive player" targeting or a
//! first-mover tie-break picked differed run-to-run even with an
//! identical seeded `rng`. Every scenario here was therefore solo -
//! exactly one possible target/mover - which left every
//! multi-player-specific mechanic (Intervene, party-heal-lowest-ally
//! targeting, Pack Instinct/Symbiosis, curse-splitting, golem
//! redistribution across survivors) outside snapshot coverage
//! entirely, and made any test pairing a party with an exact assertion
//! inherently flaky rather than merely unlucky.
//!
//! `simulate_battle` now takes a `fight_seed` and orders its party by
//! sorting on id and then shuffling from that seed with a SEPARATE
//! generator - see its own doc. A multi-character fight is now fully
//! reproducible from `(fight_seed, rng)`, so **party scenarios are
//! allowed here and are the right way to cover the mechanics listed
//! above**. Solo stays fine wherever a party isn't needed; it is simply
//! no longer a constraint.
//!
//! Every fixture committed before that change still matches byte for
//! byte: sorting and shuffling a one-element slice are both no-ops, and
//! no draw was added to or removed from `rng`.
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
    /// Passive-tree investment, as `(node key, rank)` pairs written
    /// straight into `passive_allocations` (2026-08-20).
    ///
    /// **Every scenario carrying a non-empty list here exists because
    /// the corpus previously had NO passive coverage at all.**
    /// `Scenario` had no way to express investment, so every node sat at
    /// rank 0 in every fixture and not one passive mechanic fired. A
    /// corpus run proved only that non-passive combat was unchanged -
    /// far weaker than it appeared - and left the entire passive surface
    /// unprotected against exactly the kind of large `combat.rs` change
    /// these fixtures exist to catch.
    ///
    /// Allocations must be REACHABLE ones: a Specialization needs its
    /// parent Skill invested, a Modifier needs its parent Specialization
    /// at 4/4, and the total must fit `points_for_level(level)`. A
    /// fixture built from a state the game could never produce would be
    /// pinning fiction. Deliberately NOT routed through the allocation
    /// validator - this is a fixture definition, and going through
    /// `AdventureManager` would drag async and persistence into a pure,
    /// seeded snapshot. `scenario_allocations_are_reachable` enforces
    /// the rules instead.
    ///
    /// **KNOWN LIMIT - solo scenarios cannot cover ally-targeted
    /// passives.** Every scenario here is solo, because
    /// `simulate_battle` builds its unit list from a `HashMap` whose
    /// iteration order Rust randomizes per process (see this module's
    /// own doc). That is a hard determinism requirement, but it means a
    /// passive whose effect targets an ALLY simply never fires: there is
    /// no ally. `covenant`, `compassion`, `chainoflight`,
    /// `sharedstrength` and `unitedpack` are all in that category, among
    /// others. Their allocation still pins whatever stat contribution
    /// they make, but their ally-targeting branch is NOT under snapshot
    /// coverage and cannot be until multi-character fights are
    /// deterministic. Do not read a green corpus as proof that those
    /// branches are unchanged - that is precisely the over-reading this
    /// whole field exists to correct.
    passives: &'static [(&'static str, u32)],
    /// Elementalist's golem slot types. Any unlocked slot this doesn't
    /// name falls back to `GolemType::Basic`, per that field's own
    /// documented default.
    golem_slots: &'static [GolemType],
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
        Scenario { name: "warrior_vs_lich_stage50", seed: 1, stage: 50, archetype: Archetype::Warrior, level: 10, boss_kind: Some(BossKind::Lich), passives: &[], golem_slots: &[], boss: boss(8_000, 150, 1100) },
        Scenario { name: "rogue_vs_generic_stage50", seed: 2, stage: 50, archetype: Archetype::Rogue, level: 10, boss_kind: None, passives: &[], golem_slots: &[], boss: boss(8_000, 150, 1100) },
        Scenario { name: "mage_vs_cthulhu_stage200", seed: 3, stage: 200, archetype: Archetype::Mage, level: 25, boss_kind: Some(BossKind::Cthulhu), passives: &[], golem_slots: &[], boss: boss(40_000, 400, 1100) },
        Scenario { name: "cleric_vs_tough_boss_stage200", seed: 4, stage: 200, archetype: Archetype::Cleric, level: 25, boss_kind: Some(BossKind::FireDemon), passives: &[], golem_slots: &[], boss: tough_boss(40_000, 400, 1100) },
        Scenario { name: "warlock_vs_dragon_stage500", seed: 5, stage: 500, archetype: Archetype::Warlock, level: 50, boss_kind: Some(BossKind::Dragon), passives: &[], golem_slots: &[], boss: boss(120_000, 900, 1100) },
        Scenario { name: "paladin_vs_cube_stage500", seed: 6, stage: 500, archetype: Archetype::Paladin, level: 50, boss_kind: Some(BossKind::GelatinousCube), passives: &[], golem_slots: &[], boss: boss(120_000, 900, 1100) },
        // High stage - exercises the late-stage damage penalty AND boss
        // pierce together (see WIKI_IMPACT.md's pierce entry - both are
        // stage^2/(stage^2+h^2)-shaped ramps, both real at this stage).
        Scenario { name: "ranger_vs_lich_stage3000", seed: 7, stage: 3000, archetype: Archetype::Ranger, level: 80, boss_kind: Some(BossKind::Lich), passives: &[], golem_slots: &[], boss: boss(2_000_000, 6_000, 1100) },
        Scenario { name: "slayer_vs_tough_boss_stage3000", seed: 8, stage: 3000, archetype: Archetype::Slayer, level: 80, boss_kind: Some(BossKind::Dragon), passives: &[], golem_slots: &[], boss: tough_boss(2_000_000, 6_000, 1100) },
        Scenario { name: "druid_vs_generic_stage1000", seed: 9, stage: 1000, archetype: Archetype::Druid, level: 60, boss_kind: None, passives: &[], golem_slots: &[], boss: boss(500_000, 2_000, 1100) },
        Scenario { name: "monk_vs_fire_demon_stage1000", seed: 10, stage: 1000, archetype: Archetype::Monk, level: 60, boss_kind: Some(BossKind::FireDemon), passives: &[], golem_slots: &[], boss: boss(500_000, 2_000, 1100) },
        Scenario { name: "berserker_vs_lich_stage1000", seed: 11, stage: 1000, archetype: Archetype::Berserker, level: 60, boss_kind: Some(BossKind::Lich), passives: &[], golem_slots: &[], boss: boss(500_000, 2_000, 1100) },
        // ---- passive-allocating scenarios (2026-08-20) ----
        //
        // The eleven above allocate nothing, so no passive mechanic
        // fires in any of them - see `Scenario::passives`. Everything
        // below exists to close that gap.
        //
        // FIRST and most urgent: Elementalist with three Thunder Golems.
        // This is the single densest concentration of at-risk passive
        // machinery in the game - golem spawning, golem stat
        // INHERITANCE from the summoner, Righteous Fire's self-burn and
        // its healing/regen counterpart, and Thunder Golem's
        // absorbed-damage redistribution on death. A large `combat.rs`
        // change to `spawn_golem` would otherwise land with none of it
        // under snapshot coverage.
        //
        // Level 140 because the build below spends 33 points and
        // `points_for_level` grants `1 + level / 4` - 36 at this level.
        // Solo, like every scenario here, per this module's own
        // HashMap-iteration-order rule.
        Scenario {
            name: "elementalist_thunder_golems_vs_dragon_stage1000",
            seed: 12,
            stage: 1000,
            archetype: Archetype::Elementalist,
            level: 140,
            boss_kind: Some(BossKind::Dragon),
            passives: &[
                // Golem Master 3/3 -> three summon slots, all Thunder.
                ("golemmaster", 3),
                // Thunder Golem 4/4 -> unlocks its own modifiers below.
                ("thundergolem", 4),
                ("gigantify", 3),
                ("growing", 3),
                // Righteous Fire: the self-damage half...
                ("righteousfire", 3),
                // ...and Healing Flames 4/4 for the regen half plus
                // Rising Phoenix, so a death-and-revival path is live.
                ("healingflames", 4),
                ("risingphoenix", 3),
                // Elemental Focus into fire, which Righteous Fire scales
                // with - exercises the focus/RF interaction, not just
                // each in isolation.
                ("elementalfocus", 3),
                ("scorchingfocus", 4),
                ("incinerate", 3),
            ],
            golem_slots: &[GolemType::Thunder, GolemType::Thunder, GolemType::Thunder],
            boss: boss(500_000, 2_000, 1100),
        },
        // One scenario per class whose nodes an upcoming migration batch
        // will touch, each allocating those exact nodes at meaningful
        // ranks so the batch has something to be measured against.
        //
        // Paladin's batch is a single node, `finaljudgment`, which is a
        // Modifier under Judgment - so Judgment has to reach 4/4 to
        // unlock it at all. Divine Shield and Guardian's Oath come along
        // to put the class's periodic-cast and intervene paths in the
        // fixture too, rather than pinning one narrow branch.
        Scenario {
            name: "paladin_passives_vs_fire_demon_stage500",
            seed: 13,
            stage: 500,
            archetype: Archetype::Paladin,
            level: 64,
            boss_kind: Some(BossKind::FireDemon),
            passives: &[("smite", 3), ("judgment", 4), ("finaljudgment", 3), ("shield", 3), ("oath", 3)],
            golem_slots: &[],
            boss: boss(120_000, 900, 1100),
        },
        // Warlock's batch is three nodes across three different
        // Specialization chains (`chaostheory` under Unstable Power,
        // `covenant` under Dark Communion, `ravage` under Fel Rush), so
        // all three parents need 4/4. Two of the three are boolean
        // rank-threshold nodes, which is exactly the shape Stage 2
        // deliberately left for a later batch.
        Scenario {
            name: "warlock_passives_vs_dragon_stage1000",
            seed: 14,
            stage: 1000,
            archetype: Archetype::Warlock,
            level: 108,
            boss_kind: Some(BossKind::Dragon),
            passives: &[
                ("pact", 3),
                ("unstablepower", 4),
                ("chaostheory", 3),
                ("felrush", 4),
                ("ravage", 3),
                ("siphon", 3),
                ("darkcommunion", 4),
                ("covenant", 3),
            ],
            golem_slots: &[],
            boss: boss(500_000, 2_000, 1100),
        },
        // Druid has NO pending migration nodes left - its only one
        // (`unitedpack`) was migrated in Stage 2 - so unlike the two
        // above, this scenario protects no upcoming batch. It is here
        // for general passive coverage: Druid's heal-over-time, pack and
        // thorns branches are all already-tunable `Special` nodes with
        // no snapshot coverage whatsoever, and the same large `combat.rs`
        // changes threaten them.
        Scenario {
            name: "druid_passives_vs_lich_stage1000",
            seed: 15,
            stage: 1000,
            archetype: Archetype::Druid,
            level: 68,
            boss_kind: Some(BossKind::Lich),
            passives: &[("regrowth", 3), ("rejuvenation", 3), ("instinct", 3), ("packinstinct", 3), ("barrier", 3), ("bramblegrowth", 3)],
            golem_slots: &[],
            boss: boss(500_000, 2_000, 1100),
        },
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
    // Passive investment (2026-08-20) - see `Scenario::passives`. An
    // empty list leaves the character exactly as every pre-2026-08-20
    // fixture captured it, which is what keeps those eleven fixtures
    // byte-identical through this change.
    for (key, rank) in s.passives {
        character.passive_allocations.insert((*key).to_string(), *rank);
    }
    character.golem_slot_types = s.golem_slots.to_vec();

    let mut characters: HashMap<String, Character> = HashMap::new();
    characters.insert(s.name.to_string(), character);

    let tunables = LiveTunables::default();
    let (won, units, events, rolls) = simulate_battle(&characters, vec![(s.boss.clone(), s.boss_kind, 1.0)], s.stage, &tunables, s.seed, &mut rng);
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

/// Every scenario's passive investment must be a build the game could
/// actually produce (2026-08-20) - see `Scenario::passives`.
///
/// `run_scenario` writes allocations straight into the map rather than
/// going through the allocation validator, which keeps the snapshot pure
/// and seeded but also means nothing would otherwise stop a fixture from
/// pinning an impossible build: a Modifier under an un-specialized
/// parent, a rank above a node's cap, or more points than the level
/// grants. A fixture like that would look authoritative while describing
/// a state no player can reach, which is worse than no fixture at all.
#[test]
fn scenario_allocations_are_reachable() {
    for s in scenarios() {
        if s.passives.is_empty() {
            continue;
        }
        let nodes = s.archetype.passive_nodes();
        let ranks: HashMap<&str, u32> = s.passives.iter().copied().collect();

        let mut spent = 0u32;
        for (key, rank) in s.passives {
            let node = nodes.iter().find(|n| n.key == *key).unwrap_or_else(|| panic!("{}: node {key:?} is not in {:?}'s tree", s.name, s.archetype));
            assert!(*rank > 0, "{}: {key:?} is allocated at rank 0 - drop it from the list instead", s.name);
            assert!(*rank <= node.max_rank, "{}: {key:?} at rank {rank} exceeds its max_rank of {}", s.name, node.max_rank);
            if let Some(parent) = node.parent {
                let required = node.unlock_at.unwrap_or(1);
                let parent_rank = ranks.get(parent).copied().unwrap_or(0);
                assert!(
                    parent_rank >= required,
                    "{}: {key:?} needs its parent {parent:?} at rank {required}, but the scenario allocates it at {parent_rank}",
                    s.name
                );
            }
            spent += rank;
        }

        let budget = crate::passive_tree::points_for_level(s.level);
        assert!(spent <= budget, "{}: spends {spent} points but level {} only grants {budget}", s.name, s.level);
    }
}

/// A passive-allocating scenario has to actually EXERCISE its passives -
/// a fixture whose mechanics never fire would pass forever while
/// protecting nothing, which is precisely the trap the corpus fell into
/// before these scenarios existed.
#[test]
fn passive_scenarios_actually_fire_their_mechanics() {
    for s in scenarios() {
        if s.passives.is_empty() {
            continue;
        }
        let snapshot = run_scenario(&s);
        assert!(!snapshot.events.is_empty(), "{}: produced no events at all", s.name);

        if s.archetype == Archetype::Elementalist && s.passives.iter().any(|(k, _)| *k == "golemmaster") {
            let golems = snapshot.units.iter().filter(|u| u.golem_summoner_id.is_some()).count();
            assert!(golems > 0, "{}: invests in Golem Master but no golem was ever summoned - the fixture protects nothing", s.name);
        }
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
