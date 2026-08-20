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
