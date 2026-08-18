# Questions for the owner

Found during the wiki completeness pass (2026-08-18 onward). Not resolved here —
either routed to the primary session as bugs, or left as open design questions
the wiki works around by documenting current behavior rather than guessing.

## BUGS — routing to the primary session

Behavior confirmed against source; these look like real defects, not design
choices. The wiki documents *current actual behavior* for the minor ones (so
players aren't misled by an out-of-date page), except where noted.

1. ~~**Echoing Power (Mage) tooltip disagrees with its own data.**~~ **FIXED** by the primary session (see `WIKI_IMPACT.md` entry) — description text corrected to match the real, unchanged 95%-at-3/3 data. Wiki now states 95% directly.
2. ~~**Intervene's per-character cap doesn't actually hold.**~~ **FIXED** by the primary session — `combat_intervene()` now has the same post-combine `.min(0.5)` hard cap that evasion/DR/block already had.
3. **Leech's cap window resets instead of sliding.** `src/adventure/combat.rs:6543-6546`: `if at_ms - window_start >= 1000 { reset }`. A leech-heavy build can burst close to 2× `LIFE_LEECH_CAP_PER_SEC` across a window boundary (near-cap right before reset, near-cap again right after).
4. **`/fights` HTML page reads a much smaller history pool than `/fights.json`.** The HTML page draws from `COARSE_FIGHTS_CAPACITY = 5` (fights server-wide, `src/adventure/fight_storage.rs:42`), while `/fights.json` draws from `SUMMARY_FIGHTS_CAPACITY = 200`. A non-streamer player who wasn't in one of the last 5 fights logged *across the whole game* sees "no recent fights" even having just played. **Wiki treatment (per ruling): document `/fights` as a real feature, note the history window is short and under review, don't publish exact fight counts.**
5. **`/fights` craft-error popup dismiss button redirects to the wrong page.** `render_craft_error_popup` (`src/adventure_web.rs:432-444`) is shown on `/inventory`, but its dismiss button does `history.replaceState(null, '', '/')` instead of `/inventory` — every sibling popup (craft success, disenchant) correctly restores `/inventory`. Looks like a leftover from the dashboard/inventory page split.
6. **`!clearbattlefield`/`!resetbattlefield` missing from two commands.rs lists.** Not in `BUILTIN_NAMES` (`commands.rs:1499-1507`, the collision guard for `!command add`) — a mod could shadow it with a dead custom command. Also missing from `hand_written_public_entries()` (`commands.rs:126-181`), so it never appears on the public commands page.
7. **`!price vessels` cooldown comment contradicts its own code.** Comment at `commands.rs:478-480` says `!price vessels` isn't rate-limited since `!vesselprice` itself isn't — but the code unconditionally rate-limits every `!price <x>` call via the outer `matches!` (`commands.rs:467-472`), so `!price vessels` **is** throttled, sharing a bucket with plain `!price`.
8. **Stale code comments (behavior is correct, only the comment is wrong)** — flagging in case anything else was hand-verified against these:
   - `manager.rs:3544, 3642` — say `late_content_stage` is "(90)"; the real compiled default and live config are both **100**.
   - `manager.rs:3543, 3657` — say Sacred gating is "`SACRED_STAGE_THRESHOLD` (200)"; the real constant is **300**.
   - `manager.rs` (loot-roll comment near the drop logic) — says the bag caps at "50"; `INVENTORY_CAPACITY` is actually **150**.
   - `combat.rs:3556-3559` — describes `late_stage_damage_penalty_pct` as capped by "how far past its tuned stage range" a boss is; the actual formula (`combat.rs:7538`, `stage / (stage + 2000)`) has no per-boss tuned range at all, just the raw absolute world stage. Either aspirational or stale — worth confirming before anyone relies on the comment.
   - `item.rs:3-4` — `EquipSlot`'s doc comment says "no inventory/manual !equip for now"; a full 150-slot bag and manual equip-from-inventory both exist and are load-bearing.

## OPEN DESIGN QUESTIONS — not blocking, wiki documents current behavior

1. **`SACRED_STAGE_THRESHOLD` (300) is a hardcoded constant; `late_content_stage` (Perfect gate, default 100) is admin-tunable via `/admin/tunables`.** Intentional asymmetry, or should Sacred's threshold be tunable too?
2. **Unique Shard's drop chance is literally aliased to Celestial Shard's rate** (`manager.rs:4610-4614`, same `celestial_shard_drop_chance` tunable feeds both). Intentional-for-now, or should it get its own rate?
3. **`respec_passive_tree` only clears the primary tree.** The secondary (Split Personality) tree has no respec path except unequipping the item, which fully refunds it. Is that the intended "respec," or is a secondary-tree respec missing?
4. **`!nextencounter` silently accepts undocumented boss aliases** (`demon`, `fire`, `gelatinouscube` — `manager.rs:4254-4265`) beyond the ones listed in its own usage text and the public commands page (`lich, firedemon, cthulhu, dragon, bahamut, purple, cube`). Should the wiki list the extra aliases, or should they be dropped from code?
5. **A `!recentsongs` command is referenced in a doc comment** (`song_requests.rs:440`) but never registered anywhere in `commands.rs`. Planned-and-never-shipped, or a stale comment?
6. **Several frequently-useful commands have zero rate limiting** (`!join`, `!character`, `!party`, `!queue`, `!nowplaying`, `!playlist <username>` — which queues songs on every call — and all four vote commands). Likely fine (cheap lookups), but `!playlist <username>` dumping into the live queue with no throttle seemed worth a second look.
7. **Paladin is the only other innately-hybrid archetype (heal power baked in outside `combat_function()`), but `Archetype::description()`'s "innately hybrid" text only fires for `combat_function() == Heal`** — so Paladin never gets that line client-side despite qualifying by the same logic Cleric/Druid do. Intentional omission, or should Paladin's description say so too?
8. **No dust/sand economy cap exists anywhere** — both are unclamped `u64` accumulators. Assumed intentional (no ceiling needed given the sink list), flagging only for confirmation.
