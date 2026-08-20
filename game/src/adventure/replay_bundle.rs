//! Replay-bundle sequencing - the ordering key every bundle member is
//! reassembled against.
//!
//! # Why a new key at all
//!
//! Reassembling a replay by `at_ms` corrupts it: timestamps are not
//! unique, and same-`at_ms` runs are load-bearing (see
//! `build_player_vitals`, which walks the log WITHOUT re-sorting and
//! relies on last-write-wins within a 100ms bucket).
//!
//! The obvious alternative was the id already on the wire. `RollEvent`
//! carries `event_id`/`hit_id` from the shared `next_hit_id()` counter,
//! and `Attack` carries `hit_id`, so it looked like the ordering key was
//! already there and only needed serializing onto the rest.
//!
//! It was measured against a real detail-tier fight (2026-08-20) and it
//! does not work, for three independent reasons:
//!
//! - **It is not on every event.** `BuffSnapshot`, `Shield`, `Heal`,
//!   `SkillCast` and `Defeat` have no `hit_id` field at all - 30,239 of
//!   that fight's events carried none.
//! - **It is not monotonic in log order.** 1,328 inversions: the counter
//!   is drawn at hit-resolution time, not at push time, and it is shared
//!   with rolls, so an event can be pushed after an event that drew a
//!   later id.
//! - **It does not settle same-`at_ms` runs.** Sorting a same-`at_ms`
//!   run by `hit_id` reproduced the real log order in only 85.91% of
//!   runs.
//!
//! So the census flagged one reassembly trap (`at_ms`) and there were
//! two. The writer stamps its own key instead.
//!
//! # Why the array index
//!
//! `EncounterResult::events` is a `Vec` built by pushing, so its index
//! IS log order - correct by construction rather than by argument, with
//! no counter to keep in step across the 78 event push sites. The two
//! transforms that run downstream both preserve it: `compress_events`
//! maps 1:1, and `thin_events_for_overlay` filters. `seq` is therefore
//! stamped once, on the full pre-thinning log, and every later view of
//! that fight is an order-preserving subsequence of it.
//!
//! That makes the gaps meaningful. A thinned broadcast copy carries
//! `seq` values with holes in them, so a reader can see exactly what
//! thinning removed and how much - a fact, where the companion app
//! currently has to infer it (`feedTrimmed`).
//!
//! `seq` is fight-scoped, which is the only scope reassembly ever
//! operates in: a bundle describes one fight, and members are joined to
//! each other, never across fights.

use serde::Serialize;

use super::CombatEvent;

/// One replay record: a combat event plus the key fixing its place in
/// the fight's log order.
///
/// `#[serde(flatten)]` so the record keeps the event's own shape with
/// `seq` alongside it (`{"seq":0,"kind":"attack",...}`) rather than
/// nesting it - readers that already know a `CombatEvent` can ignore
/// `seq` entirely and still parse, which is what the bundle's
/// forward-compatibility rule requires of every member.
// Stage 4 (the bundle writer) is the first non-test consumer of these two.
// An explicit allow until then, rather than leaving warnings standing that
// would camouflage a real one.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SequencedEvent<'a> {
    pub(crate) seq: u32,
    #[serde(flatten)]
    pub(crate) event: &'a CombatEvent,
}

/// Stamps log order onto `events`.
///
/// MUST be called on the full, pre-thinning log - the same caller
/// contract `build_player_vitals` already documents, and for the same
/// reason: a key assigned after thinning describes the surviving subset
/// rather than the fight, and the two are not the same document.
#[allow(dead_code)]
pub(crate) fn sequence_events(events: &[CombatEvent]) -> Vec<SequencedEvent<'_>> {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| SequencedEvent { seq: index as u32, event })
        .collect()
}


// ---------------------------------------------------------------------------
// The bundle writer (Stage 4: dual-write).
// ---------------------------------------------------------------------------
//
// Written ALONGSIDE the legacy tiers, never instead of them. The legacy
// writes happen first and are untouched by anything here; `build_bundle`
// takes `&EncounterResult` so it cannot perturb what they serialize, and a
// test pins the legacy bytes as identical either side of a bundle build.
//
// Member split, per the ratified design:
//
//   core          everything that is not an event stream (<0.1% of a file)
//   replay        attack (NOT dot) / heal / defeat / skillCast
//   dot           attack where sourceKind == dot, split out because it is
//                 58% of archive event records on its own
//   buffs         shield / buffSnapshot - 25.16% of archive event bytes,
//                 deliberately off the public wire since 2026-08-20
//   rolls         66.8% of an archive file, operator tier, never public
//   playerVitals  pinned byte-for-byte
//
// replay and dot are disjoint, and both carry `seq` from the FULL log, so a
// reader merges them back into true log order by sorting on it. That is the
// whole reason the sequence key exists.

use std::collections::BTreeMap;

use serde_json::value::RawValue;
use sha2::{Digest, Sha256};

use super::{fight_storage, AttackSourceKind, BossStats, EncounterResult};

const SCHEMA_VERSION: u32 = 1;
const MIN_READER_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemberEntry {
    pub(crate) v: u32,
    pub(crate) state: &'static str,
    pub(crate) tier: &'static str,
    pub(crate) bytes: usize,
    pub(crate) sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pinned_shape: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub(crate) schema_version: u32,
    pub(crate) min_reader_version: u32,
    pub(crate) fight_id: String,
    pub(crate) started_at_unix_ms: u64,
    pub(crate) real_duration_ms: u32,
    pub(crate) display_duration_ms: u32,
    pub(crate) pinned: bool,
    pub(crate) members: BTreeMap<String, MemberEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Bundle {
    pub(crate) manifest: Manifest,
    pub(crate) members: BTreeMap<String, Box<RawValue>>,
}

/// The `core` member. Built field by field rather than by subtracting the
/// event streams from `EncounterResult`, so a field added to that struct
/// later cannot silently appear in a public-tier member without someone
/// deciding it should.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Core<'a> {
    kind: &'a super::EncounterKind,
    stage: u32,
    won: bool,
    participants: &'a [String],
    units: &'a [super::CombatUnitInfo],
    display_duration_ms: u32,
    real_duration_ms: u32,
    loot: &'a [super::LootDrop],
    broken: &'a [super::BrokenItem],
    enemy_name: &'a Option<String>,
    enemy_count: &'a Option<u32>,
    retreated: &'a [String],
    boss_sprites: &'a [String],
    summary: &'a super::FightSummarySnapshot,
    boss_stats: &'a [BossStats],
    started_at_unix_ms: u64,
}

fn is_dot(event: &CombatEvent) -> bool {
    matches!(event, CombatEvent::Attack { source_kind: AttackSourceKind::Dot, .. })
}

/// Serializes one member and records its size and digest.
///
/// The digest covers exactly the bytes a reader will receive, so it stays
/// meaningful once Stage 5 serves members as individual HTTP resources.
fn member(
    members: &mut BTreeMap<String, Box<RawValue>>,
    entries: &mut BTreeMap<String, MemberEntry>,
    name: &str,
    tier: &'static str,
    state: &'static str,
    pinned_shape: Option<bool>,
    value: &impl Serialize,
) {
    // Serialized ONCE, and those exact bytes are what the bundle carries.
    //
    // Deliberately NOT via serde_json::Value: a Value is a BTreeMap, so a
    // round-trip through one sorts every object's keys alphabetically.
    // That silently rewrote playerVitals from {id, hpSamples, diedAtMs}
    // to {diedAtMs, hpSamples, id} - still valid JSON, still parses, and
    // exactly the byte-level drift the pin on that member exists to
    // prevent. RawValue holds the bytes as serialized instead, so a member
    // is byte-identical to serializing its source type directly, and the
    // size and digest below describe precisely the bytes a reader gets.
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let bytes = serialized.len();
    let sha256 = format!("{:x}", Sha256::digest(serialized.as_bytes()));
    let raw = RawValue::from_string(serialized).unwrap_or_else(|_| RawValue::from_string("null".to_string()).expect("null is valid JSON"));
    entries.insert(
        name.to_string(),
        MemberEntry { v: 1, state, tier, bytes, sha256, pinned_shape },
    );
    members.insert(name.to_string(), raw);
}

/// Builds the bundle for one finished fight.
///
/// MUST be called on the full, pre-thinning `result` - the same caller
/// contract `build_player_vitals` documents. `save_last_fight` is that
/// point, and `thin_events_for_overlay` runs strictly after it.
pub(crate) fn build_bundle(
    result: &EncounterResult,
    boss_stats: &[BossStats],
    started_at_unix_ms: u64,
    fight_id: String,
) -> Bundle {
    let sequenced = sequence_events(&result.events);

    let mut replay = Vec::new();
    let mut dot = Vec::new();
    let mut buffs = Vec::new();
    for record in &sequenced {
        match record.event {
            CombatEvent::Shield { .. } | CombatEvent::BuffSnapshot { .. } => buffs.push(record),
            event if is_dot(event) => dot.push(record),
            _ => replay.push(record),
        }
    }

    let mut members = BTreeMap::new();
    let mut entries = BTreeMap::new();
    member(
        &mut members,
        &mut entries,
        "core",
        "public",
        "present",
        None,
        &Core {
            kind: &result.kind,
            stage: result.stage,
            won: result.won,
            participants: &result.participants,
            units: &result.units,
            display_duration_ms: result.display_duration_ms,
            real_duration_ms: result.real_duration_ms,
            loot: &result.loot,
            broken: &result.broken,
            enemy_name: &result.enemy_name,
            enemy_count: &result.enemy_count,
            retreated: &result.retreated,
            boss_sprites: &result.boss_sprites,
            summary: &result.summary,
            boss_stats,
            started_at_unix_ms,
        },
    );

    member(&mut members, &mut entries, "replay", "public", "present", None, &replay);

    // "present", not "aggregated": this writes dot at full fidelity. The
    // measured 98.34% aggregation is a separate, lossy transform and lands
    // as its own change - the manifest's `state` field exists precisely so
    // that switch needs no format change.
    member(&mut members, &mut entries, "dot", "participant", "present", None, &dot);

    member(&mut members, &mut entries, "buffs", "participant", "present", None, &buffs);

    member(&mut members, &mut entries, "rolls", "operator", "present", None, &result.rolls);

    member(&mut members, &mut entries, "playerVitals", "public", "present", Some(true), &result.player_vitals);

    Bundle {
        manifest: Manifest {
            schema_version: SCHEMA_VERSION,
            min_reader_version: MIN_READER_VERSION,
            fight_id,
            started_at_unix_ms,
            real_duration_ms: result.real_duration_ms,
            display_duration_ms: result.display_duration_ms,
            pinned: false,
            members: entries,
        },
        members,
    }
}

/// Dual-write entry point. Called from `save_last_fight` AFTER every legacy
/// tier has been written, so a failure here can never cost a fight its real
/// archive.
pub(crate) fn save_bundle(result: &EncounterResult, boss_stats: &[BossStats], started_at_unix_ms: u64) {
    fight_storage::save_bundle_fight(|seq| {
        build_bundle(result, boss_stats, started_at_unix_ms, format!("{seq:010}"))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adventure::combat::{compress_events, thin_events_for_overlay};
    use crate::adventure::{AttackSourceKind, CombatUnitInfo};

    fn unit(id: &str, is_boss: bool) -> CombatUnitInfo {
        CombatUnitInfo {
            id: id.to_string(),
            display_name: id.to_string(),
            is_boss,
            archetype: None,
            role: None,
            max_hp: 100,
            golem_summoner_id: None,
            golem_type: None,
            thunder_net_absorbed: 0,
        }
    }

    fn attack(at_ms: u32, attacker: &str, hit_id: u64) -> CombatEvent {
        CombatEvent::Attack {
            at_ms,
            attacker: attacker.to_string(),
            target: "target".to_string(),
            damage: 1,
            unmitigated_damage: 1,
            target_hp_after: 0,
            is_crit: false,
            evaded: false,
            hit_id,
            source_kind: AttackSourceKind::Direct,
        }
    }

    #[test]
    fn seq_is_dense_and_follows_log_order() {
        let events = vec![
            attack(0, "a_player", 900),
            CombatEvent::BuffSnapshot { at_ms: 0, unit: "a_player".to_string(), buffs: vec![] },
            attack(0, "a_player", 100),
            CombatEvent::Defeat { at_ms: 5, unit: "a_player".to_string() },
        ];

        let sequenced = sequence_events(&events);

        assert_eq!(sequenced.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    }

    /// The whole reason this module exists: the third record has a LOWER
    /// `hit_id` than the first, and two of the four have no `hit_id` at
    /// all, so neither `at_ms` nor `hit_id` can put these back in order.
    /// `seq` can.
    #[test]
    fn seq_settles_a_run_that_at_ms_and_hit_id_both_get_wrong() {
        let events = vec![
            attack(0, "a_player", 900),
            CombatEvent::Shield {
                at_ms: 0,
                healer: "a_player".to_string(),
                target: "a_player".to_string(),
                amount: 5,
            },
            attack(0, "a_player", 100),
        ];

        let sequenced = sequence_events(&events);

        // Every record shares one at_ms, so at_ms alone cannot order them.
        assert!(sequenced.iter().all(|e| e.event.at_ms() == 0));
        // And hit_id is both absent and out of order across the run.
        let hit_ids: Vec<Option<u64>> = events
            .iter()
            .map(|e| match e {
                CombatEvent::Attack { hit_id, .. } => Some(*hit_id),
                _ => None,
            })
            .collect();
        assert_eq!(hit_ids, vec![Some(900), None, Some(100)]);
        // seq is the only one of the three that reproduces the log.
        assert_eq!(sequenced.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn compress_events_preserves_order_so_seq_survives_it() {
        let events = vec![
            attack(0, "a_player", 1),
            attack(500, "a_player", 2),
            attack(1_000, "a_player", 3),
            attack(2_000, "a_player", 4),
        ];
        let before: Vec<u64> = events
            .iter()
            .map(|e| match e {
                CombatEvent::Attack { hit_id, .. } => *hit_id,
                _ => 0,
            })
            .collect();

        let (compressed, _display_ms) = compress_events(events);

        assert_eq!(compressed.len(), before.len(), "compress_events must map 1:1");
        let after: Vec<u64> = compressed
            .iter()
            .map(|e| match e {
                CombatEvent::Attack { hit_id, .. } => *hit_id,
                _ => 0,
            })
            .collect();
        assert_eq!(after, before, "compress_events must not reorder");
    }

    /// Thinning may only ever REMOVE records. If it could reorder or
    /// duplicate them, a thinned copy's `seq` values would no longer be
    /// a subsequence of the archive's and the two could not be compared.
    #[test]
    fn thinned_wire_copy_is_an_order_preserving_subsequence() {
        let units = vec![unit("a_player", false), unit("__enemy_0__", true)];
        // One second, well over the 500 player cap, so thinning must bite.
        let events: Vec<CombatEvent> = (0..1_200).map(|i| attack(0, "a_player", i as u64)).collect();

        let sequenced = sequence_events(&events);
        let thinned = thin_events_for_overlay(events.clone(), &units);

        assert!(thinned.len() < events.len(), "the cap must actually have applied");

        // Walk the thinned copy against the full log: every survivor must
        // appear, in order, with a strictly increasing seq.
        let mut cursor = 0usize;
        let mut last_seq: Option<u32> = None;
        for survivor in &thinned {
            let survivor_hit = match survivor {
                CombatEvent::Attack { hit_id, .. } => *hit_id,
                _ => unreachable!("this fixture is attacks only"),
            };
            let found = sequenced[cursor..]
                .iter()
                .position(|candidate| match candidate.event {
                    CombatEvent::Attack { hit_id, .. } => *hit_id == survivor_hit,
                    _ => false,
                })
                .map(|offset| cursor + offset)
                .expect("every thinned event must exist in the full log, later than the last one");
            let seq = sequenced[found].seq;
            if let Some(previous) = last_seq {
                assert!(seq > previous, "seq must strictly increase across the thinned copy");
            }
            last_seq = Some(seq);
            cursor = found + 1;
        }
    }
}

/// Conformance between the Rust types and the IDL.
///
/// The IDL (`game/schema/replay-bundle.v1.json`) is the source of truth for
/// both repos, but nothing in Rust reads it at runtime - so without these
/// tests a field could be renamed here, every Rust test would still pass,
/// and the drift would only surface as a parse failure in somebody else's
/// app. That is the failure this stage exists to make impossible.
#[cfg(test)]
mod schema_conformance {
    use super::*;
    use crate::adventure::{AttackSourceKind, PlayerVitals};
    use serde_json::Value;

    const IDL: &str = include_str!("../../schema/replay-bundle.v1.json");

    fn idl() -> Value {
        serde_json::from_str(IDL).expect("the IDL must be valid JSON")
    }

    /// Serializes one event through `SequencedEvent`, exactly as a bundle
    /// member would, and returns its JSON object.
    fn record(event: &CombatEvent) -> serde_json::Map<String, Value> {
        let sequenced = SequencedEvent { seq: 0, event };
        match serde_json::to_value(&sequenced).expect("must serialize") {
            Value::Object(map) => map,
            other => panic!("a sequenced event must serialize to an object, got {other}"),
        }
    }

    fn every_variant() -> Vec<CombatEvent> {
        vec![
            CombatEvent::Attack {
                at_ms: 0,
                attacker: "a".to_string(),
                target: "b".to_string(),
                damage: 1,
                unmitigated_damage: 2,
                target_hp_after: 3,
                is_crit: true,
                evaded: false,
                hit_id: 4,
                source_kind: AttackSourceKind::Dot,
            },
            CombatEvent::Heal {
                at_ms: 1,
                healer: "a".to_string(),
                target: "b".to_string(),
                amount: 5,
                target_hp_after: 6,
                is_revive: false,
            },
            CombatEvent::Shield { at_ms: 2, healer: "a".to_string(), target: "b".to_string(), amount: 7 },
            CombatEvent::Defeat { at_ms: 3, unit: "a".to_string() },
            CombatEvent::SkillCast { at_ms: 4, unit: "a".to_string(), skill: "Doom".to_string() },
            CombatEvent::BuffSnapshot { at_ms: 5, unit: "a".to_string(), buffs: vec![("curse".to_string(), 0.68)] },
        ]
    }

    #[test]
    fn the_idl_describes_every_event_kind_the_game_can_emit() {
        let idl = idl();
        let kinds = idl["eventKinds"].as_object().expect("eventKinds must be an object");
        for event in every_variant() {
            let json = record(&event);
            let kind = json["kind"].as_str().expect("every event serializes a kind tag").to_string();
            assert!(kinds.contains_key(&kind), "the IDL has no entry for event kind {kind:?}");
        }
    }

    #[test]
    fn every_field_the_game_emits_is_declared_in_the_idl() {
        let idl = idl();
        for event in every_variant() {
            let json = record(&event);
            let kind = json["kind"].as_str().expect("kind tag").to_string();
            let declared = idl["eventKinds"][&kind]["fields"]
                .as_object()
                .unwrap_or_else(|| panic!("the IDL declares no fields for {kind:?}"));
            for field in json.keys() {
                assert!(
                    declared.contains_key(field),
                    "{kind}.{field} is emitted by the game but absent from the IDL - add it there and regenerate the validator",
                );
            }
        }
    }

    #[test]
    fn every_field_the_idl_requires_is_actually_emitted() {
        let idl = idl();
        for event in every_variant() {
            let json = record(&event);
            let kind = json["kind"].as_str().expect("kind tag").to_string();
            let required = idl["eventKinds"][&kind]["required"]
                .as_array()
                .unwrap_or_else(|| panic!("the IDL declares no required list for {kind:?}"));
            for field in required {
                let field = field.as_str().expect("required entries are strings");
                assert!(
                    json.contains_key(field),
                    "the IDL requires {kind}.{field}, but the game does not emit it",
                );
            }
        }
    }

    #[test]
    fn the_idl_knows_every_attack_source_kind() {
        let idl = idl();
        let declared: Vec<String> = idl["eventKinds"]["attack"]["fields"]["sourceKind"]["values"]
            .as_array()
            .expect("sourceKind must declare its values")
            .iter()
            .map(|v| v.as_str().expect("values are strings").to_string())
            .collect();

        // Every variant, spelled the way serde will actually emit it.
        for variant in [
            AttackSourceKind::Direct,
            AttackSourceKind::Splash,
            AttackSourceKind::Dot,
            AttackSourceKind::Reflect,
            AttackSourceKind::CurseShare,
            AttackSourceKind::Environmental,
        ] {
            let emitted = serde_json::to_value(variant).expect("must serialize");
            let emitted = emitted.as_str().expect("source kinds serialize as strings");
            assert!(
                declared.iter().any(|d| d == emitted),
                "AttackSourceKind::{variant:?} serializes as {emitted:?}, which the IDL does not list",
            );
        }
    }

    /// playerVitals is pinned byte-for-byte, so this is the strictest check
    /// in the file: the emitted key set must equal the IDL's declared key
    /// set exactly. Not a subset either way - an added field breaks the
    /// external consumer, a removed one breaks the contract.
    #[test]
    fn pinned_player_vitals_shape_matches_the_idl_exactly() {
        let idl = idl();
        let vitals = PlayerVitals {
            id: "a_player".to_string(),
            hp_samples: vec![(0, 100), (100, 50)],
            died_at_ms: Some(100),
        };
        let json = match serde_json::to_value(&vitals).expect("must serialize") {
            Value::Object(map) => map,
            other => panic!("playerVitals entries must be objects, got {other}"),
        };

        let declared = idl["members"]["playerVitals"]["item"]["fields"]
            .as_object()
            .expect("the IDL must declare the pinned shape");

        let mut emitted_keys: Vec<&str> = json.keys().map(String::as_str).collect();
        let mut declared_keys: Vec<&str> = declared.keys().map(String::as_str).collect();
        emitted_keys.sort_unstable();
        declared_keys.sort_unstable();

        assert_eq!(
            emitted_keys, declared_keys,
            "the pinned playerVitals shape changed - this member may not evolve without an explicit migration agreed with PathOfDust_Desktop",
        );
        assert_eq!(idl["members"]["playerVitals"]["pinnedShape"], Value::Bool(true));
    }

    #[test]
    fn hp_samples_are_at_ms_hp_pairs() {
        let vitals = PlayerVitals {
            id: "a_player".to_string(),
            hp_samples: vec![(0, 790822), (300, 786612)],
            died_at_ms: None,
        };
        let json = serde_json::to_value(&vitals).expect("must serialize");
        assert_eq!(json["hpSamples"], serde_json::json!([[0, 790822], [300, 786612]]));
        // died_at_ms is Option and serializes as null rather than being
        // omitted; the IDL marks it nullable to match.
        assert_eq!(json["diedAtMs"], Value::Null);
    }
}

/// Dual-write safety.
///
/// The condition on this stage was that enabling the bundle must not move a
/// single byte of what the legacy tiers write. These tests prove that
/// against real files written by the real writer, not against values held in
/// memory, because "the file is identical" is the claim that matters.
#[cfg(test)]
mod dual_write {
    use super::*;
    use crate::adventure::{
        AttackSourceKind, BossStats, CombatUnitInfo, DetailFightSnapshot, EncounterKind,
        EncounterResult, FightSummarySnapshot, LastFightSnapshot, PlayerVitals,
        RollCategory, RollEvent,
    };

    fn attack(at_ms: u32, hit_id: u64, source_kind: AttackSourceKind) -> CombatEvent {
        CombatEvent::Attack {
            at_ms,
            attacker: "a_player".to_string(),
            target: "__enemy_0__".to_string(),
            damage: 10,
            unmitigated_damage: 20,
            target_hp_after: 90,
            is_crit: false,
            evaded: false,
            hit_id,
            source_kind,
        }
    }

    /// A fight with every member populated and, deliberately, several events
    /// sharing one at_ms across different members - the case a naive
    /// reassembly gets wrong.
    fn fixture() -> EncounterResult {
        EncounterResult {
            kind: EncounterKind::Boss,
            stage: 2056,
            won: false,
            participants: vec!["a_player".to_string()],
            units: vec![CombatUnitInfo {
                id: "a_player".to_string(),
                display_name: "A Player".to_string(),
                is_boss: false,
                archetype: None,
                role: None,
                max_hp: 100,
                golem_summoner_id: None,
                golem_type: None,
                thunder_net_absorbed: 0,
            }],
            events: vec![
                CombatEvent::SkillCast { at_ms: 0, unit: "a_player".to_string(), skill: "Doom".to_string() },
                attack(0, 1, AttackSourceKind::Direct),
                CombatEvent::BuffSnapshot { at_ms: 0, unit: "a_player".to_string(), buffs: vec![("curse".to_string(), 0.68)] },
                attack(0, 2, AttackSourceKind::Dot),
                CombatEvent::Shield { at_ms: 0, healer: "a_player".to_string(), target: "a_player".to_string(), amount: 5 },
                attack(100, 3, AttackSourceKind::Splash),
                attack(100, 4, AttackSourceKind::Dot),
                CombatEvent::Heal { at_ms: 200, healer: "a_player".to_string(), target: "a_player".to_string(), amount: 7, target_hp_after: 97, is_revive: false },
                CombatEvent::Defeat { at_ms: 300, unit: "a_player".to_string() },
            ],
            display_duration_ms: 6000,
            real_duration_ms: 2800,
            loot: vec![],
            broken: vec![],
            enemy_name: Some("The Hollow Choir".to_string()),
            enemy_count: Some(1),
            retreated: vec![],
            boss_sprites: vec!["hollow-choir".to_string()],
            rolls: vec![RollEvent {
                event_id: 10,
                hit_id: 1,
                caused_by: None,
                at_ms: 0,
                category: RollCategory::Crit,
                source: "Crit chance".into(),
                actor: "a_player".to_string(),
                target: Some("__enemy_0__".to_string()),
                probability: Some(0.25),
                succeeded: Some(false),
                magnitude: None,
            }],
            summary: FightSummarySnapshot::default(),
            player_vitals: vec![PlayerVitals {
                id: "a_player".to_string(),
                hp_samples: vec![(0, 100), (300, 0)],
                died_at_ms: Some(300),
            }],
        }
    }

    /// Members are held as raw serialized bytes, so a test that wants to
    /// inspect records parses them back first.
    fn member_array(bundle: &Bundle, name: &str) -> Vec<serde_json::Value> {
        serde_json::from_str(bundle.members[name].get()).expect("member must be valid JSON")
    }

    pub(super) fn sample_bundle() -> Bundle {
        build_bundle(&fixture(), &[BossStats::default()], 1_755_690_000_000, "0000004154".to_string())
    }

    fn legacy_files(result: &EncounterResult, tag: &str) -> (Vec<u8>, Vec<u8>) {
        // Exactly what save_last_fight writes to the coarse and detail tiers.
        let snapshot = LastFightSnapshot {
            result: result.clone(),
            boss_stats: vec![BossStats::default()],
            started_at_unix_ms: 1_755_690_000_000,
        };
        let detail = DetailFightSnapshot {
            snapshot: LastFightSnapshot {
                result: result.clone(),
                boss_stats: vec![BossStats::default()],
                started_at_unix_ms: 1_755_690_000_000,
            },
            rolls: result.rolls.clone(),
        };

        let dir = std::env::temp_dir();
        let coarse_path = dir.join(format!("pod-dualwrite-coarse-{tag}.json"));
        let detail_path = dir.join(format!("pod-dualwrite-detail-{tag}.json"));
        crate::state::save_json_compact(&coarse_path, &snapshot).expect("coarse must write");
        crate::state::save_json_compact(&detail_path, &detail).expect("detail must write");
        let coarse = std::fs::read(&coarse_path).expect("coarse must read back");
        let detail_bytes = std::fs::read(&detail_path).expect("detail must read back");
        let _ = std::fs::remove_file(&coarse_path);
        let _ = std::fs::remove_file(&detail_path);
        (coarse, detail_bytes)
    }

    /// CONDITION 1. Real files, written by the real writer, either side of a
    /// bundle build.
    #[test]
    fn legacy_bytes_are_identical_either_side_of_a_bundle_build() {
        let result = fixture();
        let boss_stats = vec![BossStats::default()];

        let (coarse_before, detail_before) = legacy_files(&result, "before");
        let bundle = build_bundle(&result, &boss_stats, 1_755_690_000_000, "0000000001".to_string());
        let (coarse_after, detail_after) = legacy_files(&result, "after");

        assert_eq!(coarse_before, coarse_after, "the coarse tier's bytes moved when the bundle was built");
        assert_eq!(detail_before, detail_after, "the detail tier's bytes moved when the bundle was built");
        assert!(!coarse_before.is_empty() && !detail_before.is_empty(), "the fixture must actually produce files");
        // And the bundle really was built - otherwise this passes vacuously.
        assert_eq!(bundle.manifest.members.len(), 6);
    }

    /// CONDITION 2. The pinned member must be the legacy shape byte for
    /// byte, not merely a shape that parses the same.
    #[test]
    fn pinned_player_vitals_member_is_byte_for_byte_the_legacy_serialization() {
        let result = fixture();
        let bundle = build_bundle(&result, &[], 0, "0000000001".to_string());

        let legacy = serde_json::to_string(&result.player_vitals).expect("legacy vitals must serialize");
        let in_bundle = bundle.members["playerVitals"].get().to_string();

        assert_eq!(in_bundle, legacy, "the pinned member diverged from the legacy serialization");
        assert_eq!(bundle.manifest.members["playerVitals"].pinned_shape, Some(true));
        assert_eq!(bundle.manifest.members["playerVitals"].tier, "public");
    }

    /// The split's core correctness: no event lost, none duplicated.
    #[test]
    fn every_event_lands_in_exactly_one_member() {
        let result = fixture();
        let bundle = build_bundle(&result, &[], 0, "0000000001".to_string());

        let count = |name: &str| member_array(&bundle, name).len();
        assert_eq!(
            count("replay") + count("dot") + count("buffs"),
            result.events.len(),
            "events were lost or duplicated across the member split",
        );

        let mut seqs: Vec<u64> = Vec::new();
        for name in ["replay", "dot", "buffs"] {
            for record in member_array(&bundle, name) {
                seqs.push(record["seq"].as_u64().expect("every record carries seq"));
            }
        }
        seqs.sort_unstable();
        seqs.dedup();
        assert_eq!(seqs.len(), result.events.len(), "seq values collided across members");
        assert_eq!(seqs, (0..result.events.len() as u64).collect::<Vec<_>>(), "seq must be dense over the whole log");
    }

    /// Merging the members back by seq must reproduce the original log
    /// exactly - including the same-at_ms run the fixture opens with, which
    /// is the case at_ms and hit_id both get wrong.
    #[test]
    fn merging_members_by_seq_reproduces_the_original_log() {
        let result = fixture();
        let bundle = build_bundle(&result, &[], 0, "0000000001".to_string());

        let mut merged: Vec<(u64, serde_json::Value)> = Vec::new();
        for name in ["replay", "dot", "buffs"] {
            for record in member_array(&bundle, name) {
                merged.push((record["seq"].as_u64().expect("seq"), record.clone()));
            }
        }
        merged.sort_by_key(|(seq, _)| *seq);

        for (index, (_, record)) in merged.iter().enumerate() {
            let original = serde_json::to_value(&result.events[index]).expect("event serializes");
            for (key, value) in original.as_object().expect("event object") {
                assert_eq!(
                    record.get(key),
                    Some(value),
                    "event {index} field {key:?} differs after a member round-trip",
                );
            }
        }
    }

    #[test]
    fn dot_is_split_out_of_replay_and_neither_leaks_into_the_other() {
        let result = fixture();
        let bundle = build_bundle(&result, &[], 0, "0000000001".to_string());

        for record in member_array(&bundle, "replay") {
            assert_ne!(record["sourceKind"].as_str(), Some("dot"), "a dot attack leaked into replay");
            let kind = record["kind"].as_str().expect("kind");
            assert!(matches!(kind, "attack" | "heal" | "defeat" | "skillCast"), "replay carries {kind}");
        }
        for record in member_array(&bundle, "dot") {
            assert_eq!(record["sourceKind"].as_str(), Some("dot"));
        }
        for record in member_array(&bundle, "buffs") {
            let kind = record["kind"].as_str().expect("kind");
            assert!(matches!(kind, "shield" | "buffSnapshot"), "buffs carries {kind}");
        }
        assert_eq!(member_array(&bundle, "dot").len(), 2);
        assert_eq!(member_array(&bundle, "buffs").len(), 2);
    }

    /// shield/buffSnapshot are off the public wire on purpose. If the split
    /// ever put them in a public-tier member they would be back on it.
    #[test]
    fn tiers_match_the_ratified_exposure_model() {
        let bundle = build_bundle(&fixture(), &[], 0, "0000000001".to_string());
        let tier = |name: &str| bundle.manifest.members[name].tier;

        assert_eq!(tier("core"), "public");
        assert_eq!(tier("replay"), "public");
        assert_eq!(tier("playerVitals"), "public");
        assert_eq!(tier("buffs"), "participant");
        assert_eq!(tier("dot"), "participant");
        assert_eq!(tier("rolls"), "operator", "full roll logs must never be public");
    }

    #[test]
    fn manifest_sizes_and_digests_describe_the_bytes_a_reader_receives() {
        use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
        let bundle = build_bundle(&fixture(), &[], 0, "0000000001".to_string());

        for (name, entry) in &bundle.manifest.members {
            let serialized = bundle.members[name].get().to_string();
            assert_eq!(entry.bytes, serialized.len(), "{name}: manifest byte count is wrong");
            let digest = format!("{:x}", Sha256::digest(bundle.members[name].get().as_bytes()));
            assert_eq!(entry.sha256, digest, "{name}: manifest digest is wrong");
            assert_eq!(entry.v, 1);
            assert_eq!(entry.state, "present");
        }
    }

    /// The bundle names itself, and that name has to be the file it is
    /// written as, or a manifest cannot be matched to its own archive entry.
    #[test]
    fn the_manifest_carries_both_clocks_and_its_own_id() {
        let bundle = build_bundle(&fixture(), &[], 1_755_690_000_000, "0000004154".to_string());
        assert_eq!(bundle.manifest.fight_id, "0000004154");
        assert_eq!(bundle.manifest.started_at_unix_ms, 1_755_690_000_000);
        // Both clocks: recovering real DoT tick times means inverting the
        // display compression, which is impossible with only one of them.
        assert_eq!(bundle.manifest.real_duration_ms, 2800);
        assert_eq!(bundle.manifest.display_duration_ms, 6000);
        assert_eq!(bundle.manifest.schema_version, 1);
        assert_eq!(bundle.manifest.min_reader_version, 1);
    }

    /// The written artifact must be the shape the JS validator and the
    /// golden fixture both expect: { manifest, members }.
    #[test]
    fn the_written_bundle_has_the_shape_the_validator_expects() {
        let bundle = build_bundle(&fixture(), &[], 0, "0000000001".to_string());
        let value = serde_json::to_value(&bundle).expect("bundle serializes");
        let object = value.as_object().expect("a bundle is an object");
        assert_eq!(object.keys().collect::<Vec<_>>(), vec!["manifest", "members"]);
        for name in ["core", "replay", "dot", "buffs", "rolls", "playerVitals"] {
            assert!(value["members"].get(name).is_some(), "member {name} missing from the payload");
            assert!(value["manifest"]["members"].get(name).is_some(), "member {name} missing from the manifest");
        }
    }
}

#[cfg(test)]
mod writer_golden {
    use super::*;

    /// The cross-repo golden contract, from this side.
    ///
    /// `writer-output.v1.json` is what `build_bundle` actually produced,
    /// committed verbatim. The JavaScript contract suite validates that same
    /// file from the reader's side, so both repos are checked against one
    /// artifact rather than against each other's prose. If the writer changes
    /// shape this fails here AND there, which is the point: a shared schema
    /// file on its own enforces nothing.
    ///
    /// This is also what pins key ORDER. Members are serialized once and
    /// carried as raw bytes precisely so that order is stable; a refactor
    /// back through serde_json::Value would sort every object's keys and
    /// break the pinned playerVitals shape without changing a single value.
    #[test]
    fn the_writer_still_produces_the_committed_golden_bundle() {
        let produced = serde_json::to_string(&super::dual_write::sample_bundle())
            .expect("the bundle must serialize");
        let committed = include_str!("../../tests/fixtures/replay_bundle/writer-output.v1.json");

        assert_eq!(
            produced, committed,
            "the writer no longer produces the committed golden bundle - if that is intended, \
             update game/tests/fixtures/replay_bundle/writer-output.v1.json and re-run the \
             JavaScript contract suite before shipping it",
        );
    }
}
