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
