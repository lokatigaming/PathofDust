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

/// Boss-kind stage transition after one fight resolves (win: +1, loss:
/// -2 floored at 1) - the exact arithmetic the old per-fight
/// announcement's own wording used ("Onward to stage N+1!"/"knocked
/// back to stage N!"), extracted so `aggregate_batch` below can replay
/// the same random walk across a whole batch without duplicating it.
/// Basic-kind fights never move the stage - callers only invoke this
/// for Boss-kind entries.
fn next_boss_stage(stage: u32, won: bool) -> u32 {
    if won { stage + 1 } else { stage.saturating_sub(2).max(1) }
}

/// One fight's contribution to a pending batch (fight-announcement
/// batching, 2026-08-19 - a live request to cut per-fight chat spam:
/// "party of N heroes..." lines now accumulate into one aggregated
/// summary per `LiveTunables::fight_summary_batch_size` fights instead
/// of posting individually). Captured at the moment
/// `AdventureManager::announce_encounter_result` would otherwise have
/// sent this fight's own line - a lean, purpose-built subset of
/// `EncounterResult`/`PlayerFightStats`, just what `aggregate_batch`
/// below needs, not the whole struct (a batch holds up to
/// `fight_summary_batch_size` of these in memory at once).
#[derive(Debug, Clone)]
pub(crate) struct BatchedFight {
    pub kind: super::EncounterKind,
    pub won: bool,
    /// The stage this fight was fought AT - matches `EncounterResult::stage`'s
    /// own "before this fight's own win/loss transition" semantics.
    /// Only meaningful for Boss-kind entries; a Basic fight never moves
    /// the stage.
    pub stage: u32,
    /// (display_name, damage_dealt, damage_taken, healing_done) - one
    /// row per participant, UNFILTERED (a player who did nothing in
    /// THIS specific fight can still matter once summed across the
    /// whole batch) - `aggregate_batch` applies the "only show real
    /// contributors, don't pad with fabricated zeros" filtering at the
    /// end, on the SUMMED totals, not per-fight.
    pub players: Vec<(String, u64, u64, u64)>,
}

impl BatchedFight {
    /// Builds one from a real `EncounterResult` - the only place this
    /// module reads `PlayerFightStats`'s own field names, so
    /// `aggregate_batch` itself stays decoupled from that type's exact
    /// shape.
    pub(crate) fn from_result(result: &super::EncounterResult) -> Self {
        Self {
            kind: result.kind,
            won: result.won,
            stage: result.stage,
            players: result.summary.players.iter().map(|p| (p.display_name.clone(), p.damage_dealt, p.damage_taken, p.healing_done)).collect(),
        }
    }
}

/// The aggregated numbers a batch summary is built from - win/loss
/// record, net stage movement (+ the peak reached along the way), and
/// each category's top-3-summed-across-the-batch leaderboard. Produced
/// by `aggregate_batch`, consumed by `format_batch_summary`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BatchSummaryData {
    pub fight_count: usize,
    pub win_count: usize,
    pub loss_count: usize,
    pub stage_before: u32,
    pub stage_after: u32,
    pub stage_peak: u32,
    pub top_damage_dealt: Vec<(String, u64)>,
    pub top_damage_taken: Vec<(String, u64)>,
    pub top_healing_done: Vec<(String, u64)>,
}

/// Sums each participant's damage/healing across every fight in the
/// batch (by display name, this codebase's existing chat-announcement
/// identity - see `format_batch_mvp_line`), replays the boss-stage
/// random walk across just the Boss-kind entries via `next_boss_stage`
/// (a Basic fight only contributes to the win/loss record, never moves
/// the stage), then re-ranks each category's own top 3 off the SUMMED
/// totals - a player 4th-6th in any single fight can still be a
/// batch-wide top-3 leader once summed, so this deliberately does NOT
/// reuse `fight_summary_from_snapshot`'s already-per-fight-capped view.
/// `None` only when `fights` is empty (never actually reached in
/// practice - see `AdventureManager::flush_fight_summary_batch`'s own
/// empty-batch guard - kept as a real `Option` anyway rather than
/// baking in a "never happens" assumption).
pub(crate) fn aggregate_batch(fights: &[BatchedFight]) -> Option<BatchSummaryData> {
    let first = fights.first()?;
    let mut win_count = 0usize;
    let mut loss_count = 0usize;
    let stage_before = first.stage;
    let mut stage = first.stage;
    let mut stage_peak = first.stage;
    let mut totals: std::collections::HashMap<String, (u64, u64, u64)> = std::collections::HashMap::new();
    for fight in fights {
        if fight.won {
            win_count += 1;
        } else {
            loss_count += 1;
        }
        if fight.kind == super::EncounterKind::Boss {
            stage = next_boss_stage(stage, fight.won);
            stage_peak = stage_peak.max(stage);
        }
        for (name, dealt, taken, healed) in &fight.players {
            let entry = totals.entry(name.clone()).or_insert((0, 0, 0));
            entry.0 += dealt;
            entry.1 += taken;
            entry.2 += healed;
        }
    }
    let top = |pick: fn(&(u64, u64, u64)) -> u64| -> Vec<(String, u64)> {
        let mut ranked: Vec<(String, u64)> = totals.iter().filter(|(_, t)| pick(t) > 0).map(|(name, t)| (name.clone(), pick(t))).collect();
        // Stable, deterministic tiebreak (alphabetical) - HashMap
        // iteration order isn't - so equal totals always rank the same
        // way instead of depending on hash-bucket luck.
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(super::FIGHT_SUMMARY_TOP_N);
        ranked
    };
    Some(BatchSummaryData {
        fight_count: fights.len(),
        win_count,
        loss_count,
        stage_before,
        stage_after: stage,
        stage_peak,
        top_damage_dealt: top(|t| t.0),
        top_damage_taken: top(|t| t.1),
        top_healing_done: top(|t| t.2),
    })
}

/// Twitch's own hard chat-message limit.
pub(crate) const TWITCH_MESSAGE_LIMIT: usize = 500;
/// The real target `format_batch_summary` degrades down to fit under -
/// comfortable headroom below Twitch's hard 500-char cap.
pub(crate) const FIGHT_SUMMARY_TARGET_LEN: usize = 450;

/// The record line alone ("Last N fights: WW-LL · stage X → Y (peak Z)") -
/// the unconditional first segment of every batch summary, and also its
/// own final degradation fallback (see `format_batch_summary`) - always
/// short and bounded since it never contains a username, only counts
/// and stage numbers. The peak is shown only when it's genuinely above
/// both the start and end stage (a real "pushed further, then dropped
/// back" moment) - otherwise it's redundant with the plain "X → Y" and
/// omitted to save room.
fn format_batch_record_line(data: &BatchSummaryData) -> String {
    let stage_part = if data.stage_peak > data.stage_before.max(data.stage_after) {
        format!(" · stage {} → {} (peak {})", data.stage_before, data.stage_after, data.stage_peak)
    } else if data.stage_after != data.stage_before {
        format!(" · stage {} → {}", data.stage_before, data.stage_after)
    } else {
        String::new()
    };
    format!("Last {} fight{}: {}W-{}L{stage_part}", data.fight_count, if data.fight_count == 1 { "" } else { "s" }, data.win_count, data.loss_count)
}

/// One MVP category line ("🗡️ Top DPS: Name (1.2M), ..."), capped to
/// `max_named` leaders. `None` when there's nothing to show for this
/// category (nobody contributed) OR `max_named == 0` - the latter is
/// `format_batch_summary`'s own final "drop the MVP breakdown entirely"
/// degradation rung.
fn format_batch_mvp_line(emoji_label: &str, entries: &[(String, u64)], max_named: usize) -> Option<String> {
    if entries.is_empty() || max_named == 0 {
        return None;
    }
    let shown: String =
        entries.iter().take(max_named).map(|(name, amt)| format!("{name} ({})", crate::adventure_web::format_number(*amt as f64))).collect::<Vec<_>>().join(", ");
    Some(format!("{emoji_label}: {shown}"))
}

/// Builds the aggregated batch-summary chat line, degrading gracefully
/// to stay within `FIGHT_SUMMARY_TARGET_LEN`: tries all 3 MVP categories
/// at 3 named leaders each first, then 2, then 1, then drops the MVP
/// breakdown entirely and posts just the record line - which is itself
/// always short (no usernames, only counts/stage numbers), so this
/// function can never exceed the target regardless of roster size or
/// username length. Length is measured in `.chars()` (Unicode scalar
/// count, not bytes) - a conservative-enough proxy for "how long does
/// this look in chat" given this module's own emoji-heavy vocabulary.
/// See the `fight_summary_batch_worst_case_...` test for the proof this
/// bound holds at this codebase's actual worst case (25-char usernames,
/// `format_number`'s own longest possible output).
pub(crate) fn format_batch_summary(data: &BatchSummaryData) -> String {
    let record = format_batch_record_line(data);
    for max_named in [3usize, 2, 1] {
        let mvp_parts: Vec<String> = [
            format_batch_mvp_line("🗡️ Top DPS", &data.top_damage_dealt, max_named),
            format_batch_mvp_line("🛡️ Top Tanks", &data.top_damage_taken, max_named),
            format_batch_mvp_line("💚 Top Heals", &data.top_healing_done, max_named),
        ]
        .into_iter()
        .flatten()
        .collect();
        let candidate = if mvp_parts.is_empty() { record.clone() } else { format!("{record} | {}", mvp_parts.join(" · ")) };
        if candidate.chars().count() <= FIGHT_SUMMARY_TARGET_LEN {
            debug_assert!(candidate.chars().count() < TWITCH_MESSAGE_LIMIT);
            return candidate;
        }
    }
    debug_assert!(record.chars().count() < TWITCH_MESSAGE_LIMIT, "the bare record line has no usernames and must always fit Twitch's hard cap");
    record
}

/// The loot line - `None` if there's nothing worth announcing (matches
/// main.rs: this message is only sent when non-empty). Boss fights skip
/// the loot list itself (per a live request, "it's just extra spam" on
/// top of the MVP breakdown) - a Basic encounter's loot line is still
/// its only loot feedback.
///
/// Worn-out-gear and retreat notices used to also post here (2026-08-19,
/// removed per a live "chat noise cleanup" request - chat spam only,
/// both mechanics themselves are untouched). Worn-gear still shows on
/// the /fights dashboard (`adventure_web.rs`'s own direct read of
/// `EncounterResult.broken`, unrelated to this fn) - only the chat
/// message stopped. Retreat had no other surface reading it, so
/// `EncounterResult.retreated` is now write-only from this fn's
/// perspective (still populated, just never announced) - left as-is
/// rather than ripped out, since character.rs's own retreat state
/// (`retreated_since`/`sync_retreat_status`) is the real mechanic and is
/// completely separate from this per-fight announcement list.
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

/// Divine Dust fight-drop announcement (2026-08-19,
/// docs/divine_dust_spec.md) - fight drops only, same "rare event" shape
/// as `format_unique_shard_win` above; the craft recipe's own output is
/// deliberately silent (routine, not an event).
pub(crate) fn format_divine_dust_drop(display_name: &str) -> String {
    format!("✨ {display_name} found a speck of Divine Dust!")
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
            real_duration_ms: 0,
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

    fn batched(kind: EncounterKind, won: bool, stage: u32, players: &[(&str, u64, u64, u64)]) -> BatchedFight {
        BatchedFight { kind, won, stage, players: players.iter().map(|&(name, d, t, h)| (name.to_string(), d, t, h)).collect() }
    }

    #[test]
    fn aggregate_batch_is_none_for_an_empty_batch() {
        assert_eq!(super::aggregate_batch(&[]), None);
    }

    #[test]
    fn aggregate_batch_counts_wins_and_losses_across_both_kinds() {
        let fights = vec![
            batched(EncounterKind::Boss, true, 10, &[]),
            batched(EncounterKind::Boss, false, 11, &[]),
            batched(EncounterKind::Basic, true, 10, &[]),
        ];
        let data = super::aggregate_batch(&fights).unwrap();
        assert_eq!(data.fight_count, 3);
        assert_eq!(data.win_count, 2);
        assert_eq!(data.loss_count, 1);
    }

    #[test]
    fn aggregate_batch_replays_the_boss_stage_walk_but_basic_fights_never_move_it() {
        // Boss win (10->11), Basic win (stays 11), Boss loss (11->9,
        // -2 floored... 11-2=9), Boss win (9->10).
        let fights = vec![
            batched(EncounterKind::Boss, true, 10, &[]),
            batched(EncounterKind::Basic, true, 11, &[]),
            batched(EncounterKind::Boss, false, 11, &[]),
            batched(EncounterKind::Boss, true, 9, &[]),
        ];
        let data = super::aggregate_batch(&fights).unwrap();
        assert_eq!(data.stage_before, 10);
        assert_eq!(data.stage_after, 10);
        assert_eq!(data.stage_peak, 11, "peak must reflect the highest point reached, not just start/end");
    }

    #[test]
    fn aggregate_batch_floors_a_losing_streak_at_stage_1() {
        let fights = vec![batched(EncounterKind::Boss, false, 1, &[]), batched(EncounterKind::Boss, false, 1, &[])];
        let data = super::aggregate_batch(&fights).unwrap();
        assert_eq!(data.stage_after, 1, "must never go to 0 or negative");
    }

    #[test]
    fn aggregate_batch_sums_damage_across_fights_and_reranks_top_3() {
        // Bob is 4th-6th in each individual fight but #1 once summed.
        let fights = vec![
            batched(EncounterKind::Basic, true, 5, &[("alice", 100, 0, 0), ("bob", 90, 0, 0), ("carol", 80, 0, 0), ("dave", 70, 0, 0)]),
            batched(EncounterKind::Basic, true, 5, &[("alice", 10, 0, 0), ("bob", 90, 0, 0), ("carol", 10, 0, 0), ("dave", 10, 0, 0)]),
            batched(EncounterKind::Basic, true, 5, &[("alice", 10, 0, 0), ("bob", 90, 0, 0), ("carol", 10, 0, 0), ("dave", 10, 0, 0)]),
        ];
        let data = super::aggregate_batch(&fights).unwrap();
        assert_eq!(data.top_damage_dealt[0], ("bob".to_string(), 270), "270 summed beats alice's 120 once ranked across the whole batch");
        assert_eq!(data.top_damage_dealt.len(), 3, "must still cap at the top 3");
    }

    #[test]
    fn aggregate_batch_never_pads_a_category_nobody_contributed_to() {
        let fights = vec![batched(EncounterKind::Basic, true, 5, &[("alice", 100, 0, 0)])];
        let data = super::aggregate_batch(&fights).unwrap();
        assert!(data.top_damage_taken.is_empty());
        assert!(data.top_healing_done.is_empty());
    }

    fn summary_data(fight_count: usize, win_count: usize, loss_count: usize, stage_before: u32, stage_after: u32, stage_peak: u32) -> BatchSummaryData {
        BatchSummaryData { fight_count, win_count, loss_count, stage_before, stage_after, stage_peak, top_damage_dealt: vec![], top_damage_taken: vec![], top_healing_done: vec![] }
    }

    #[test]
    fn format_batch_summary_matches_the_worked_example_shape() {
        let mut data = summary_data(10, 7, 3, 1090, 1096, 1097);
        data.top_damage_dealt = vec![("Alice".to_string(), 1_200_000), ("Bob".to_string(), 900_000)];
        let msg = super::format_batch_summary(&data);
        assert!(msg.starts_with("Last 10 fights: 7W-3L · stage 1090 → 1096 (peak 1097)"), "{msg}");
        assert!(msg.contains("🗡️ Top DPS: Alice (1.2M), Bob (900K)"), "{msg}");
    }

    #[test]
    fn format_batch_summary_omits_the_peak_when_it_matches_start_or_end() {
        let data = summary_data(5, 5, 0, 100, 105, 105);
        let msg = super::format_batch_summary(&data);
        assert!(!msg.contains("peak"), "no dip-and-recover happened, the peak segment is redundant: {msg}");
        assert!(msg.contains("stage 100 → 105"));
    }

    #[test]
    fn format_batch_summary_omits_stage_segment_entirely_when_unchanged() {
        let data = summary_data(5, 3, 2, 50, 50, 50);
        let msg = super::format_batch_summary(&data);
        assert!(!msg.contains("stage"), "an all-Basic batch never moves the stage - showing 'stage 50 -> 50' would be noise: {msg}");
    }

    #[test]
    fn format_batch_summary_singular_fight_count_wording() {
        let data = summary_data(1, 1, 0, 5, 5, 5);
        assert!(super::format_batch_summary(&data).starts_with("Last 1 fight: "), "not 'fights' for a batch of exactly 1");
    }

    #[test]
    fn format_batch_summary_degrades_from_3_to_2_to_1_named_leaders_to_fit() {
        // 20 players per category, all with distinct totals - the full
        // top-3-per-category version stays small regardless (only 3 are
        // ever shown), so degradation here is exercised directly via
        // format_batch_mvp_line at each rung instead.
        let entries: Vec<(String, u64)> = vec![("Alice".to_string(), 300), ("Bob".to_string(), 200), ("Carol".to_string(), 100)];
        assert_eq!(super::format_batch_mvp_line("🗡️ Top DPS", &entries, 3).unwrap(), "🗡️ Top DPS: Alice (300), Bob (200), Carol (100)");
        assert_eq!(super::format_batch_mvp_line("🗡️ Top DPS", &entries, 2).unwrap(), "🗡️ Top DPS: Alice (300), Bob (200)");
        assert_eq!(super::format_batch_mvp_line("🗡️ Top DPS", &entries, 1).unwrap(), "🗡️ Top DPS: Alice (300)");
        assert_eq!(super::format_batch_mvp_line("🗡️ Top DPS", &entries, 0), None, "0 named leaders is the 'drop this category' rung");
    }

    /// The hard requirement: worst-case usernames (25 chars, this
    /// codebase's own Twitch display-name ceiling) on every leader slot
    /// across all 3 categories, `format_number`'s own longest possible
    /// output (a value in the trillions, "999.9T"), and 5-digit stage
    /// numbers - the summary must STILL degrade down to fit under
    /// `FIGHT_SUMMARY_TARGET_LEN`, comfortably inside Twitch's real
    /// 500-char hard cap.
    #[test]
    fn format_batch_summary_worst_case_always_fits_the_twitch_limit() {
        let worst_name = "A".repeat(25);
        let worst_amount = 999_900_000_000_000u64; // formats as "999.9T"
        let worst_entries: Vec<(String, u64)> = (0..3).map(|i| (format!("{}{i}", &worst_name[..24]), worst_amount)).collect();
        let data = BatchSummaryData {
            fight_count: 99,
            win_count: 99,
            loss_count: 99,
            stage_before: 99999,
            stage_after: 99999,
            stage_peak: 99999,
            top_damage_dealt: worst_entries.clone(),
            top_damage_taken: worst_entries.clone(),
            top_healing_done: worst_entries,
        };
        let msg = super::format_batch_summary(&data);
        assert!(msg.chars().count() <= super::FIGHT_SUMMARY_TARGET_LEN, "worst-case message was {} chars, over the {}-char target: {msg}", msg.chars().count(), super::FIGHT_SUMMARY_TARGET_LEN);
        assert!(msg.chars().count() < super::TWITCH_MESSAGE_LIMIT, "must always stay under Twitch's real hard cap regardless of the target: {msg}");
    }

    #[test]
    fn loot_line_is_none_when_nothing_happened() {
        let result = base_result(EncounterKind::Basic, true);
        assert_eq!(super::format_loot_line(&result), None);
    }

    #[test]
    fn boss_fight_loot_list_is_suppressed() {
        let mut result = base_result(EncounterKind::Boss, true);
        result.loot = vec![LootDrop { display_name: "Alice".to_string(), item_name: "Sword".to_string(), slot: EquipSlot::Weapon, outcome: ReceiveOutcome::Equipped, tier: 1, affixes: vec![] }];
        assert_eq!(super::format_loot_line(&result), None, "boss fights must suppress the loot list itself, and nothing else announces here anymore");
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
        result.loot = (0..5)
            .map(|i| LootDrop { display_name: format!("Player{i}"), item_name: "Sword".to_string(), slot: EquipSlot::Weapon, outcome: ReceiveOutcome::Equipped, tier: 1, affixes: vec![] })
            .collect();
        let line = super::format_loot_line(&result).expect("must produce a line");
        assert!(line.contains("+2 more"), "5 loot entries capped at 3 must show '+2 more': {line}");
    }

    /// Chat noise cleanup (2026-08-19, a live request) - worn-out gear
    /// and retreat notices used to post here too; both are gone now,
    /// even when the underlying `EncounterResult` fields are populated
    /// (both mechanics themselves are untouched - see this fn's own doc).
    #[test]
    fn worn_out_gear_and_retreats_no_longer_announce_to_chat() {
        let mut result = base_result(EncounterKind::Basic, true);
        result.broken = vec![BrokenItem { display_name: "Alice".to_string(), item_name: "Sword".to_string(), slot: EquipSlot::Weapon }];
        result.retreated = vec!["Bob".to_string()];
        assert_eq!(super::format_loot_line(&result), None, "broken/retreated alone must produce no chat line at all now");

        result.loot = vec![LootDrop { display_name: "Carol".to_string(), item_name: "Shield".to_string(), slot: EquipSlot::Weapon, outcome: ReceiveOutcome::Equipped, tier: 1, affixes: vec![] }];
        let line = super::format_loot_line(&result).expect("real loot must still produce a line");
        assert!(!line.contains("Worn out") && !line.contains("Sword"), "broken items must not leak into the line even alongside real loot: {line}");
        assert!(!line.contains("Retreated") && !line.contains("Bob"), "retreats must not leak into the line even alongside real loot: {line}");
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
