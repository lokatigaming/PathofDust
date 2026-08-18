// Stage 3 API seam (2026-08-19, architecture refactor) - PURE formatting
// for every game-initiated chat announcement (§4b of REFACTOR_PLAN.md).
// Deliberately free functions, no `&self` - the state-mutating half
// (Celestial Shard / launch-giveaway grants, which need real
// AdventureManager access) lives in manager.rs's own `announce_*`
// methods, which call these to build the actual text. Splitting it this
// way makes the formatting genuinely unit-testable for the first time -
// the ORIGINAL version of this logic lived entirely inside a main.rs
// closure and had zero test coverage.
//
// This is a PORT, not a rewrite - every message here is meant to be
// byte-for-byte identical to what main.rs's own broadcast subscribers
// already produce today. Per REFACTOR_PLAN.md's own Stage 3 instruction
// ("build the seam alongside the existing path... no cutover until
// Stage 4"), main.rs's subscribers are UNTOUCHED and still the only
// thing actually announcing in production - this is new, parallel code,
// not a replacement yet.

/// Joins up to `max` items with ", ", appending "+N more" if there were
/// more - keeps a condensed summary line from growing unbounded when a
/// big roster all loot/break/retreat at once.
fn join_capped(items: &[String], max: usize) -> String {
    if items.len() <= max {
        items.join(", ")
    } else {
        format!("{}, +{} more", items[..max].join(", "), items.len() - max)
    }
}

/// The main win/loss outcome line, with the boss-fight-only MVP
/// breakdown appended - everything main.rs's own subscriber builds
/// before the Celestial Shard/launch-giveaway side effects.
pub(crate) fn format_encounter_outcome(result: &super::EncounterResult) -> String {
    let mut msg = match (result.kind, result.won) {
        (super::EncounterKind::Boss, true) => format!(
            "⚔️ Victory! The party ({} heroes) bested the stage {} enemy! Onward to stage {}!",
            result.participants.len(),
            result.stage,
            result.stage + 1
        ),
        (super::EncounterKind::Boss, false) => format!(
            "⚔️ The party ({} heroes) was defeated by the stage {} enemy... knocked back to stage {}!",
            result.participants.len(),
            result.stage,
            result.stage.saturating_sub(2).max(1)
        ),
        (super::EncounterKind::Basic, true) => format!(
            "⚔️ The party ({} heroes) fought off {} ({})!",
            result.participants.len(),
            result.enemy_name.as_deref().unwrap_or("a group of enemies"),
            result.enemy_count.map(|n| format!("{n} enemies")).unwrap_or_default()
        ),
        (super::EncounterKind::Basic, false) => format!(
            "⚔️ The party ({} heroes) was overwhelmed by {} ({})...",
            result.participants.len(),
            result.enemy_name.as_deref().unwrap_or("a group of enemies"),
            result.enemy_count.map(|n| format!("{n} enemies")).unwrap_or_default()
        ),
    };

    if result.kind == super::EncounterKind::Boss {
        let summary = super::fight_summary_from_snapshot(&result.summary);
        let per_person = |entries: &[(String, u64)]| {
            entries.iter().map(|(name, amt)| format!("{name} ({})", crate::adventure_web::format_number(*amt as f64))).collect::<Vec<_>>().join(", ")
        };
        let mut parts = Vec::new();
        if !summary.top_damage_dealt.is_empty() {
            parts.push(format!("🗡️ Top DPS: {}", per_person(&summary.top_damage_dealt)));
        }
        if !summary.top_damage_taken.is_empty() {
            parts.push(format!("🛡️ Top Tanks: {}", per_person(&summary.top_damage_taken)));
        }
        if !summary.top_healing_done.is_empty() {
            parts.push(format!("💚 Top Heals: {}", per_person(&summary.top_healing_done)));
        }
        if let Some(name) = &summary.first_to_die {
            parts.push(format!("💀 {name} first down"));
        }
        if !parts.is_empty() {
            msg.push_str(&format!(" | {}", parts.join(" · ")));
        }
    }

    msg
}

/// The separate loot/broken/retreated line - `None` if there's nothing
/// worth announcing (matches main.rs: this message is only sent when
/// non-empty). Boss fights skip the loot list itself (per a live
/// request, "it's just extra spam" on top of the MVP breakdown) - a
/// Basic encounter's loot line is still its only loot feedback.
pub(crate) fn format_loot_line(result: &super::EncounterResult) -> Option<String> {
    let mut loot_msg = String::new();

    if !result.loot.is_empty() && result.kind != super::EncounterKind::Boss {
        let parts: Vec<String> = result
            .loot
            .iter()
            .filter_map(|loot| match loot.outcome {
                super::ReceiveOutcome::Equipped => Some(format!("{} equipped a {}", loot.display_name, loot.item_name)),
                super::ReceiveOutcome::AddedToBag => Some(format!("{} bagged a {}", loot.display_name, loot.item_name)),
                super::ReceiveOutcome::BagFull => Some(format!("{} lost a {} (bag full)", loot.display_name, loot.item_name)),
                super::ReceiveOutcome::AutoDisenchanted { .. } => None,
            })
            .collect();
        if !parts.is_empty() {
            loot_msg.push_str(&format!("🎁 {}", join_capped(&parts, 3)));
        }
    }

    if !result.broken.is_empty() {
        let parts: Vec<String> = result.broken.iter().map(|item| format!("{}'s {}", item.display_name, item.item_name)).collect();
        if !loot_msg.is_empty() {
            loot_msg.push_str(" · ");
        }
        loot_msg.push_str(&format!("💔 Worn out: {}", join_capped(&parts, 3)));
    }

    if !result.retreated.is_empty() {
        if !loot_msg.is_empty() {
            loot_msg.push_str(" · ");
        }
        loot_msg.push_str(&format!("🏳️ Retreated (repair on the dashboard!): {}", join_capped(&result.retreated, 3)));
    }

    if loot_msg.is_empty() {
        None
    } else {
        Some(loot_msg)
    }
}

pub(crate) fn format_gear_crit(event: &super::GearCritEvent) -> String {
    let verb = match event.source {
        super::GearCritSource::Reforge => "reforge",
        super::GearCritSource::Recombine => "recombination",
    };
    format!(
        "🎲✨ {}'s {verb} CRIT! Their new {} ({:?}, tier {}) rolled a bonus {} modifier!",
        event.display_name,
        event.item_name,
        event.slot,
        event.tier,
        super::affix_name(event.affix)
    )
}

pub(crate) const RAMPAGE_COMPLETE_MESSAGE: &str = "🔥 Rampage complete! Things have settled back down... for now.";

pub(crate) fn format_unique_shard_win(event: &super::UniqueShardEvent) -> String {
    format!("💎 {} just found a rare Unique Shard!", event.display_name)
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn player(id: &str, damage_dealt: u64, damage_taken: u64, healing_done: u64) -> PlayerFightStats {
        PlayerFightStats { id: id.to_string(), display_name: id.to_string(), damage_dealt, damage_taken, healing_done, ..Default::default() }
    }

    fn base_result(kind: EncounterKind, won: bool) -> EncounterResult {
        EncounterResult {
            kind,
            stage: 5,
            won,
            participants: vec!["alice".to_string(), "bob".to_string()],
            units: vec![],
            events: vec![],
            display_duration_ms: 0,
            loot: vec![],
            broken: vec![],
            enemy_name: None,
            enemy_count: None,
            retreated: vec![],
            boss_sprites: vec![],
            rolls: vec![],
            summary: FightSummarySnapshot::default(),
            player_vitals: vec![],
        }
    }

    #[test]
    fn boss_victory_message_matches_the_original_wording() {
        let result = base_result(EncounterKind::Boss, true);
        let msg = super::format_encounter_outcome(&result);
        assert_eq!(msg, "⚔️ Victory! The party (2 heroes) bested the stage 5 enemy! Onward to stage 6!");
    }

    #[test]
    fn boss_defeat_message_knocks_back_two_stages_floored_at_one() {
        let mut result = base_result(EncounterKind::Boss, false);
        result.stage = 1;
        let msg = super::format_encounter_outcome(&result);
        assert!(msg.contains("knocked back to stage 1"), "must floor at stage 1, not go to 0 or negative: {msg}");
    }

    #[test]
    fn boss_fight_appends_mvp_breakdown_when_summary_has_data() {
        let mut result = base_result(EncounterKind::Boss, true);
        result.summary = FightSummarySnapshot { players: vec![player("alice", 500, 100, 0), player("bob", 100, 500, 300)], ..Default::default() };
        let msg = super::format_encounter_outcome(&result);
        assert!(msg.contains("🗡️ Top DPS:"), "must include a DPS breakdown when someone dealt damage: {msg}");
        assert!(msg.contains("🛡️ Top Tanks:"), "must include a tanking breakdown when someone took damage: {msg}");
        assert!(msg.contains("💚 Top Heals:"), "must include a healing breakdown when someone healed: {msg}");
    }

    #[test]
    fn basic_encounter_never_gets_an_mvp_breakdown() {
        let mut result = base_result(EncounterKind::Basic, true);
        result.enemy_name = Some("a pack of Goblin Raiders".to_string());
        result.enemy_count = Some(4);
        result.summary = FightSummarySnapshot { players: vec![player("alice", 500, 100, 0)], ..Default::default() };
        let msg = super::format_encounter_outcome(&result);
        assert!(!msg.contains("Top DPS"), "a Basic encounter's MVP breakdown must never render, even with real summary data: {msg}");
        assert!(msg.contains("a pack of Goblin Raiders"));
    }

    #[test]
    fn loot_line_is_none_when_nothing_happened() {
        let result = base_result(EncounterKind::Basic, true);
        assert_eq!(super::format_loot_line(&result), None);
    }

    #[test]
    fn boss_fight_loot_list_is_suppressed_but_broken_and_retreated_still_show() {
        let mut result = base_result(EncounterKind::Boss, true);
        result.loot = vec![LootDrop { display_name: "Alice".to_string(), item_name: "Sword".to_string(), slot: EquipSlot::Weapon, outcome: ReceiveOutcome::Equipped, tier: 1, affixes: vec![] }];
        result.retreated = vec!["Bob".to_string()];
        let line = super::format_loot_line(&result).expect("retreated alone must still produce a line");
        assert!(!line.contains("Sword"), "boss fights must suppress the loot list itself: {line}");
        assert!(line.contains("🏳️ Retreated"));
    }

    #[test]
    fn basic_encounter_loot_list_still_shows() {
        let mut result = base_result(EncounterKind::Basic, true);
        result.loot = vec![LootDrop { display_name: "Alice".to_string(), item_name: "Sword".to_string(), slot: EquipSlot::Weapon, outcome: ReceiveOutcome::Equipped, tier: 1, affixes: vec![] }];
        let line = super::format_loot_line(&result).expect("a Basic encounter's loot must produce a line");
        assert!(line.contains("Alice equipped a Sword"));
    }

    #[test]
    fn loot_line_caps_long_lists_with_a_plus_n_more_suffix() {
        let mut result = base_result(EncounterKind::Basic, true);
        result.retreated = (0..5).map(|i| format!("Player{i}")).collect();
        let line = super::format_loot_line(&result).expect("must produce a line");
        assert!(line.contains("+2 more"), "5 retreated names capped at 3 must show '+2 more': {line}");
    }

    #[test]
    fn auto_disenchanted_loot_is_silently_excluded() {
        let mut result = base_result(EncounterKind::Basic, true);
        result.loot = vec![LootDrop {
            display_name: "Alice".to_string(),
            item_name: "Sword".to_string(),
            slot: EquipSlot::Weapon,
            outcome: ReceiveOutcome::AutoDisenchanted { dust: 3 },
            tier: 1,
            affixes: vec![],
        }];
        assert_eq!(super::format_loot_line(&result), None, "an auto-disenchanted-only loot list must produce no announcement at all");
    }
}
