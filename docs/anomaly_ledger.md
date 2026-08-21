# Anomaly Ledger — Path of Dust

Owned by the log-parser session. Numbering here is canonical; no other
session renumbers it. This is the **first commit** of this file — it did
not exist anywhere in git history before 2026-08-21. It was reconstructed
from durable sources (auto-memory, `WIKI_IMPACT.md`, git history, this
session's own live verification) since the ledger previously lived only
in-session and had never been exported. Items whose only source is a
prior order's own summary text are marked **carried-forward, not
independently re-verified** rather than presented as freshly checked.

## Closed

**#30 — `celestial_shard_drop_chance = 0.0007`**
Ruled CLOSED 2026-08-20: the owner's own deliberate tuning, not an
un-bumped value. Do not re-flag despite `WIKI_IMPACT.md`'s own
operational warning about this field driving one roll post-merge instead
of two. *(Carried forward from memory, not re-verified today.)*

**#33 — Thunder Golem redirect / flat party `hpSamples`**
CLOSED as NOT A BUG (`WIKI_IMPACT.md:270`). The earlier owner-scoped-
absorption "fix" was itself reverted; absorption is party-wide per the
founding spec. Flat party `hpSamples` while any Thunder Golem is alive is
the correct picture of party-wide immunity, not lost data. Known gap
(not a ledger item): the §13 pinning mechanism only ever pinned the
summary tier, which never carried `hpSamples`, so a true pre-deploy
baseline comparison could not be run.

**#38 — Universal 95% damage-reduction hard cap**
CLOSED, deployed live 2026-08-20 (`WIKI_IMPACT.md:282`). No character,
golem, or enemy may be immune to any damage source through DR
specifically. Evasion, block, Intervene, Thunder Golem absorption
explicitly unaffected — separate mechanics, not DR.

**#39 — 2026-08-16 Druid rework stale-read fix**
CLOSED. Branch `fix/druid-rework-stale-reads`, commit `46f658a`, merged
`a231af2`, confirmed an ancestor of current HEAD (`380371a`). Full audit
at `docs/druid_rework_stale_read_audit.md`. Four sub-checks, verified
2026-08-21:

| Sub-check | Evidence | Result |
|---|---|---|
| `"Symbiosis"` roll-log literal absent | `own_symbiosis_dr_pct`/`symbiosis_dr_bonus`/`"Symbiosis"` all absent from `combat.rs` (grep + compiled test `the_stale_druid_rework_reads_stay_removed`); 0 occurrences across 3 sampled live fights | PASS — live + code |
| Lowest-HP members no longer pinned at 0.95 DR cap | Live: 0/38,751 `mitigation`-category rolls land in [0.945,0.955]; 0/138 real (non-zero-damage) player-target hits land in the [0.90,0.95) DR bucket, across twitchyarentwe's (Druid) fight `adventure-fights-detail/fight-0000004960.json`, snapshotted before rotation | PASS — live arithmetic |
| Temple Guardian heal ~50x lower | Audit-documented arithmetic: real contributors 0.02–0.06, stale `naturesembrace` term was adding 1.0–3.0 → ~50×. Test `the_reworked_druid_nodes_still_have_the_units_their_readers_expect` pins `naturesembrace` as a whole-number count, not a rate | PASS — code/test only. **Not exercised live**: no character in the 3 sampled fights had Nature's Embrace allocated, so no fresh live heal-amount ratio was measured this session |
| Heal-crit splash ~6x lower | Same audit, `verdantburst`: real 0.20–0.60 vs stale 1.0–3.0 → ~6×. Same test pins it as a count | PASS — code/test only, **same live-exercise gap** as above |

Recommend: close the live-exercise gap once a fight naturally allocates
Nature's Embrace/Verdant Burst and the heal-amount ratio can be measured
directly, rather than treating code/test evidence as equivalent to a
live measurement.

**#40 — `golem_slot_types` wiped by an empty-vec Memory load (NEW, assigned today)**
CLOSED. Previously tracked unnumbered in the golem-inheritance
deferred-items queue as prerequisite (d)(i). Guard lives at
`game/src/adventure/manager.rs:3110` (`if !build.golem_slot_types.is_empty()`).
No dedicated regression test existed for this specific guard (only the
non-empty round-trip case, `an_elementalist_build_round_trips_with_its_golem_slot_types`,
was covered) — added
`loading_a_memory_with_empty_golem_slot_types_does_not_wipe_the_characters_current_ones`
(`manager.rs`, same file), which exercises the real production
`load_memory` path on a fixture mirroring lokati's live loadout
(Elementalist, `golemmaster` rank 3, three Water golems), forces the
saved Memory's `golem_slot_types` empty (what a pre-existing-field save
or a genuinely golem-less Memory deserializes as), loads it, and asserts
the character's slots are preserved. **PASS**, clippy clean.

**#24 — RF self-damage emits no observable damage event**
CLOSED 2026-08-21. Live-confirmed: self-targeting `attack` events (attacker
== target) now appear in live fight data, tagged `sourceKind:"direct"` —
e.g. `colonyna` at `atMs:1000`, `damage:489187`/`unmitigatedDamage:9783733`
(`adventure-fights-detail/fight-0000004981.json`); `lokati_gaming` at
`atMs:1875`, `259664`/`5193282` (`fight-0000004982.json`). Both land at
≈5.0% of raw — consistent with `#38`'s universal 95% DR cap applying to
RF self-damage exactly as documented (`WIKI_IMPACT.md:320`), not a new
anomaly. Previously this event did not exist at all; it now does, with
real numbers. Cadence not fully characterized (only 4 RF casters and 1-2
self-damage ticks caught across the ~6s-display sampled fights), but the
core deferred claim — "an observable event now exists" — is proven.

**#29 — Shattering × inherited splash mitigability**
CLOSED 2026-08-21 via code + test, **not exercised live today** (0
Shattering/icicle occurrences across all 7 fights sampled this session —
requires a Water Golem with Shattering invested, none present in the
sample). `apply_shattering_icicle_damage` (`combat.rs:7325`) applies the
target's `damage_reduction` (respecting `defensive_stat_hard_cap`) —
genuinely mitigable, the original ask — but deliberately does **not**
route through `apply_hit`/`roll_attacker_damage`: no crit, no
evasion/block, no elemental-proc-stack compounding. Note this is a
refinement, not the literal wording of the original ruling ("normal
MITIGABLE damage") — commit `f1356fe` first tried the literal
interpretation (full `apply_hit`), which caused a documented **110,000x**
excursion (a fully-inherited golem's whole damage-multiplier stack
compounding onto "1% of a dead enemy's max HP") plus a **196x** Curse-of-
Doom leak; commit `984b9b8` (supersedes `f1356fe`, both ancestors of
current master) replaced it with the current DR-only dedicated path.
`splash + rank` targeting confirmed unchanged (separate, untouched code).
7/7 Shattering tests pass, including
`release_b_icicle_damage_is_exactly_pct_times_maxhp_times_dr_with_zero_attacker_stack_contribution`
and `release_b_doom_pool_is_unchanged_by_an_icicle_hit_on_a_cursed_target`.

**#32 — Unique-shard picker route entirely unlogged** (closed 2026-08-21)
CLOSED 2026-08-21, **live-confirmed from production server logs**
(`logs/game.log.2026-08-20`, `logs/game.log.2026-08-21` — fight JSON
files don't carry crafting actions, this required checking the tracing
log instead). 9 real `picker-apply` events across the two days, e.g.:
`2026-08-21T00:58:31Z character=fuznchill item_id=9aa40881d038104c
chosen_affix=CelestialConversion shard_balance_before=3
shard_balance_after=3 outcome_ok=true`. All 9 carry character, item_id,
chosen_affix, and shard balance before/after as required; all
`outcome_ok=true`; all show `before==after` at this specific log point,
which is documented-correct (`manager.rs:3899` doc comment — the craft
token is consumed earlier, at `craft_item_ex`'s insert time, not here).
Only the apply-time event exists — no separate "picker-open" event — but
the ledger's own deferral text only ever required "logging lands with
this release," which this satisfies; a stricter open+apply pairing was
this session's earlier memory-sourced elaboration, not the ledger's
actual requirement.

**#41 — Thunder Golem underperformance (kazesosa) — mapped to Release 1.2 items 1+2, both already-fixed and live-confirmed 2026-08-21**
CLOSED. Owner-flagged "thunder golems potentially underperforming"; resumed
investigation traced it to `game/src/adventure/combat.rs`'s own test
documentation naming kazesosa directly as the source of the original live
finding ("implied reform base = predicted x 0.7692", commit `066183b`,
Release 1.2, deployed 2026-08-19, confirmed ancestor of current HEAD
`ea2e19e`). Two sub-bugs, both fixed in that same commit, both
re-verified live today against kazesosa's own fresh fight data
(`adventure-fights-detail/fight-0000004995.json` through `-4999.json`/
`-5000.json`, sampled 2026-08-21 02:53–02:59 UTC, snapshotted before
rotation):

| Item (066183b) | Bug | Live check | Result |
|---|---|---|---|
| 1 — HP sizing | Golem reform base frozen pre-party-buff, undercounting by `1/(1+party_max_hp_pct)` every reform | Golem spawn `maxHp` = owner's reported (post-buff) `maxHp` × 0.33 × 4.0 (gigantify r3), exactly: `8116547 × 1.32 = 10713842.04` → round `10713842`, matches live `maxHp` exactly. Reform growth also exact: `10713842 × 3 = 32141526` (reform_count=2, growing r3 = 100%/reform) matches live exactly. One later fight showed a 3-unit-of-32M (0.00001%) drift from double-rounding order, not a bug signature | **PASS — live, exact** |
| 2 — damage magnitude | Golem `crit_chance`/`crit_multiplier` scaled to 33% of owner's instead of full inheritance | Owner's and golem's live `rolls[]` "Crit chance"/"Crit multiplier" magnitudes compared directly in the same fight: owner `9.3956%`/`48.296%`, golem `9.3956%`/`48.296%` — identical to the last decimal, 3330 crit rolls sampled | **PASS — live, exact** |

Both are genuinely FIXED and live, not just code/test-confirmed.

**#35 relationship: RELATED but CONFIRMED SEPARATE, not the same item.**
Kazesosa's complaint traces entirely to Release 1.2 items 1+2 above (golem
sizing/damage), now closed. `#35`'s own description (reform/redistribution
DEGRADATION, blocked on tree's not-yet-pushed observability instrumentation)
maps instead to Release 1.2 item 3 (redistribution delivery redirect-to-
alive-member fix) and `#36` (tank-credit shortfall) territory — a different
mechanic (damage redistribution on golem death, not golem sizing/damage
output). Note for whoever next touches `#35`/`#36`: item 3's own code fix
is ALSO already an ancestor of current HEAD (same `066183b` commit) — but
this session did not independently re-verify redistribution/tank-credit
live (out of today's scope, no dedicated "redistribution" event kind
exists in fight logs to check directly - only `attack`/`heal`/`shield`/
`skillCast`/`defeat`/`buffSnapshot` - which is presumably exactly the
observability gap `#35` is blocked on). `#35`/`#36` remain open, still
owned by tree, not resolved by this entry.

## Reconciliation note — 2026-08-21, later sweep

This sweep's own kickoff order seeded a ledger reconstruction from a
**stale** in-memory snapshot that predates this file's actual first
commit (`e2650e5`, today) and `ea2e19e`'s later closures. Per the
owner's mid-task correction, this file was read and maintained instead
of overwritten. Concrete disagreements found between the seed and this
file's actual state:

- Seed said **#39 OPEN** with fix commit `087a446`. Both wrong: `#39`
  is **CLOSED** (see above), and `087a446` is
  `Ignore backup-pre-stage2/ and backup-pre-stage3-chain/` — an
  unrelated housekeeping commit. The real fix is `46f658a`, merged
  `a231af2`.
- Seed said **#31/#32 OPEN**. `#32` is CLOSED (live-confirmed from
  `logs/game.log`). `#31` is WATCH, not fully open — `#32`'s new
  logging gives partial visibility.
- Seed's **#34/#37** (Shattering-related) don't exist under those
  numbers in this ledger; the corresponding item is `#29`, CLOSED via
  code+test, explicitly **not** live-exercised in any session's sample
  so far (including this one — still zero Shattering/icicle
  occurrences across everything sampled today).
- Seed never mentioned **#40** (golem_slot_types empty-vec wipe guard,
  CLOSED via new regression test) or **#41** (kazesosa Thunder Golem
  sizing/damage, CLOSED live) at all — both real, both already closed
  in this file before this sweep started.
- Seed's residual "golem slot-types LOAD" framing (verify via a live
  post-Memory-load fight showing typed Flame golems) is a **different,
  narrower check** than `#40`'s empty-vec guard — see new evidence
  below, logged as a supplement to `#40` rather than reopening it.

## New evidence — 2026-08-21, later sweep

**Step 0.** Running binary confirmed by direct SHA-256, not inferred:
live `target\release\game.exe` = `1babcc8c7e5b4fa1...` — matches
`REPORTS/deploy.md`'s recorded *old/still-live* hash exactly, and its
built-but-unswapped `target-corpus\release\game.exe` = `eebb0205...`.
**The Paladin Holy Fire fix (`16c4bd3`/`4f61c98`) and the
duplicate-unique-effects fix (`8284cc3`) are NOT in the running
binary** — the §13 live swap is BLOCKED per `REPORTS/deploy.md`
("staged reopen"), confirmed independently here via hash rather than
by trusting that report's prose. Per this order's own gating
condition, the Paladin re-measurement (order item 4) was **not
attempted** — doing so against the old binary would attribute new-code
behavior to old-code output. Splash-overcap, replay-bundle, DR-cap-38,
and the Druid fix ARE live (binary swap boundary lines up exactly with
`backup-pre-splash-overcap`'s capture, and `patch-notes.json` confirms
each by date).

**#39 supplement.** Fresh live sample (captured live off `/ws`,
fight in progress) shows `"Symbiosis"` still absent (0 occurrences,
consistent with prior closure) AND, new this sweep: **Nature's
Embrace and Verdant Burst actually fired live** (`twitchyarentwe` x4,
`fuznchill` x3 skillCast events) — the specific live-exercise gap
`#39`'s own entry flagged as outstanding. However the flat event log
has no causal link from a `heal` event back to the `skillCast` that
triggered it, and multiple heal sources land on the same
display-compressed `atMs` tick, so the exact ~50x/~6x heal-ratio
claim still could **not** be cleanly isolated this sweep either — the
gap is narrower (skills are now known to fire live) but not fully
closed.

**#40 supplement, live.** `lokati_gaming`'s 3 golem slots all show
`"golemType":"flame"` in a live fight captured this sweep (`units`
array, `__golem_lokati_gaming_0/1/2`) — post-Memory-load typed golems
are correctly represented in live simulation, not just round-tripped
in a unit test. Narrower than `#40`'s own empty-vec-wipe guard, but
the check this order actually asked for.

**#42 — Replay bundle (live since `4cb4eef`): manifest integrity,
sequencing, tiering, and public-wire exposure — CLOSED, live, exact.**
Two live production fights sampled directly off disk before rotation
(`adventure-fights-bundle/fight-0000000300.json`,
`fight-0000000304.json`, ~7,600–12,000ms real duration, 43-participant
boss stage 2500/2502), plus one live capture off the public `/ws`
socket.

| Sub-check | Method | Result |
|---|---|---|
| Manifest bytes/sha256 match members | Extracted all 6 members (`buffs`,`core`,`dot`,`playerVitals`,`replay`,`rolls`) from fight-300 by exact byte offset (derived from manifest `bytes`, cross-footed: sum of all 6 members' byte counts + manifest/structure overhead = file size exactly, 312,020,338B); hashed each | **PASS, exact** — all 6 `sha256` match the manifest digest byte-for-byte |
| `seq` strictly ordered, no inversions | Parsed `buffs`(43,729)+`dot`(407,785)+`replay`(27,439) from fight-300, checked monotonicity per-member and collision/density across the union | **PASS, exact** — 0 inversions in any member; union = 478,953 unique values, dense over `0..478952` with zero gaps or collisions |
| `playerVitals` byte-identical to legacy | fight-304 (bundle) vs. fight-5168 (legacy detail tier) — confirmed same underlying fight via the constant +4864 counter offset between the two storage tiers (bundle 300→304 tracked detail 5164→5168 in lockstep); extracted both `playerVitals` blobs independently and diffed | **PASS, exact** — byte-identical, same sha256 (`427dfbaf...`), `diff` clean |
| Tiers: `rolls`=operator, `buffs`/`dot`=participant, `core`/`replay`/`playerVitals`=public | Read directly from both fights' live manifests | **PASS, exact** — matches `MEMBER_TIERS` in code exactly |
| One plain public socket: TEXT frames, zero participant-tier keys | Connected to live `ws://127.0.0.1:4005/ws` for ~95s, captured 4 `state` + 1 full `encounter` frame (2.28MB) | **PASS** — 0 binary frames, 5/5 text. `encounter` frame's `events` array contains only `attack`/`heal`/`defeat`/`skillCast` (the public/`replay`-tier kinds) — **zero** `shield`/`buffSnapshot` records (the `buffs` member, participant-tier) and no `rolls` key anywhere. One false-positive avoided: a naive substring scan for `"dot"` matches, because `sourceKind:"dot"` is a normal, expected value on public `attack` records (3,212 of them) — not the archive's separate `dot` *member* key, which never appears here. Confirmed by inspecting the full parsed frame, not the substring alone. |

**#43 — Splash redesign (live since `2182eb7`/`824d54f`): partial,
one structural gap found — OPEN.** Same live `encounter` frame used
for `#42`.

| Sub-check | Result |
|---|---|
| Splash-roll success rate ≈ splash% | **Cannot be verified from logs, structurally** — `roll_splash()` (`combat.rs:10194`) is a bare `rng.gen_bool` call with no `RollEvent` emitted anywhere on its call path (checked all 6 callers). There is no `RollCategory::Splash` and no roll-log line for this specific decision at all. This is a different class of gap than an untriggered guard — it is **unobservable by design**, not merely unexercised this session. Flagging as a genuine open item rather than closing on code-only confidence. |
| Splash hits at full damage | `splash_damage_pct` default = `1.0` (`tunables.rs:371`, a `LiveTunable`). Live cross-check: for 6 of 13 players landing both `direct` and `splash` attacks in the sampled fight, splash:direct average-damage ratios cluster in [0.64, 1.91] (`ttfn` 1.05, `pappag4ming` 0.81, `clincl` 0.88, `zolaries` 0.98, `spicymufin` 0.64, `tarekis` 1.91) — consistent with "same order of magnitude, independent per-target rolls," not a fixed small scaling fraction. A few small-N (2-4 samples) outliers exist (`drewm1022` 0.01x, several golems 6-13x) fully explained by single-crit variance on tiny samples, not a scaling defect. **PASS, directional live evidence**, not exact (no same-hit_id pairing available to remove per-target roll variance). |
| Per-caller base preservation: cleave 1 | Grouped enemy `splash`-sourceKind attacks by (attacker, atMs), counted distinct targets per swing. ~20 of 34 enemy attackers show a clean, exclusive 1-target distribution across every splash swing they landed. **PASS, live, clean** — matches "cleave 1" exactly. |
| Per-caller base preservation: Dragon full-party | Same grouping: exactly 6 attackers show swings reaching 9-22 of the fight's 43 participants — matching the live roster's exact count of dragon boss sprites (`boss-dragon-bahamut` x4 + `boss-dragon-purple` x2 = 6). **Circumstantial PASS** — enemy IDs were not directly mapped to sprite names this sweep, so this is a strong correlation, not a confirmed 1:1 identity. |
| Per-caller base preservation: Cube 4+1 | **Not cleanly isolated.** No attacker in the sample shows an exclusive 4-or-5-target cluster; without an enemy-id→sprite mapping (6 `cube1` entries exist in this fight's roster) this can't be distinguished from partial-candidate-pool effects (fewer alive targets than the nominal base). Needs a follow-up sweep with that mapping resolved. |
| Support floor = 1 on roll-fail; zero-splash chars show no splash | Not checked this sweep — needs character build data (splash% per character) cross-referenced with attack logs, out of this sweep's time budget. |

**#44 — Duplicate equipped uniques: `apply_unique_affix` had no
commit-time guard. CLOSED 2026-08-21, deploy `3ead135` verified live
(`game.exe 5bd11caef8558f19...`, `twitch-bot-rs.exe 8d0716ae927eb832...`
- both hashed directly against the live binaries, not taken from the
report).**

**Self-correction on this entry's own earlier evidence:** the
"genuine game.log gap" claimed below (06:06Z stop, "no startup line
for any of today's later restarts") was a TIMEZONE COMPARISON ERROR,
not a real logging break - `game.log`'s timestamps are UTC, while
every file-mtime/backup-dir timestamp used elsewhere in this ledger is
local (+0800). Once converted, the 08:37/09:41/13:55 LOCAL restarts
match `00:37`/`01:41`/`05:55` UTC exactly, and all three startup lines
were sitting in the file the whole time. The log was never broken;
this session just compared two clocks without converting between
them - noted here rather than silently fixed, since the original claim
below is left intact for the record.

1. **Re-scan: 0/60, confirmed.** Same query, same field, same 5-slot
   set as before.
2. **The 6 cleaned characters' items - confirmed intact, with one
   honest nuance.** The migration's own log line exists now
   (`logs/game.log.2026-08-21`, `07:49:22.95890xZ` = `15:49:22` local,
   matching the recreated marker file's mtime exactly) and names all
   12 moved items across `drewm1022`, `gorshie`, `xDaido`, `Zolaries`,
   `qugetus_`, `xBornToKillx`, item-for-item matching this ledger's own
   earlier scan. Checked all 12 item IDs directly in the live save:
   10 sit in inventory untouched; 2 (zolaries's body "Eternal
   Cuirass", drewm1022's weapon "Celestial Dagger") are back in their
   ORIGINAL equipped slot - re-equipped by normal subsequent play
   after the migration ran, not left dangling by it. Confirmed not a
   regression: their own unique_affix/name are unchanged from what the
   log recorded, and the full re-scan above still reads 0 - each was
   the only remaining copy of its affix once its twin left for the
   bag, so re-equipping one created no new conflict. Nothing
   destroyed, nothing duplicated.
3. **Commit-time re-check: confirmed in source, not just claimed.**
   `git show 34b9afb` - `apply_unique_affix` (`character.rs`) now
   calls `has_conflicting_unique_affix_value` before mutating, same
   pattern `equip_from_inventory`/the legacy `CelestialShard` path
   already used, with a new test
   (`apply_unique_affix_rejects_when_a_second_equipped_slot_already_
   landed_first`) pinning the exact overlapping-picks race this
   ledger's own root-cause writeup described. `76e73cd` surfaces the
   rejection to players. No live rejection observed in today's log
   (note-if-seen only, not required - none of today's `picker-apply`
   lines show `outcome_ok=false` after the fix landed, but none were
   attempted against an already-conflicting target either, so absence
   here isn't itself a finding).

**Original writeup, preserved below for the record (superseded by the
close-out above where noted):**

Priority insert (2026-08-21, later still): owner observed zolaries
with `SplitPersonality` on both helm and body, live, despite the
11:07 migration and a claimed 13:57 zero-duplicates scan. Investigated
directly against the live save, not taken on the report's word.

**1. Confirmed live, exact.** `adventure-characters.json` (live, mtime
14:21), zolaries: `helm` = "Celestial Hood" (`id 6b20f628d0a57361`)
and `body` = "Eternal Cuirass" (`id 0aab5d7c637cc384`) — two DIFFERENT
physical items, both `unique_affix: "splitPersonality"`, both
currently equipped. Not a display artifact - raw save data. (His
`archetype` is `"monk"` with `secondary_archetype: "paladin"` -
irrelevant to gear, confirmed per the order's own framing.)

**2. Timing — genuinely BLOCKED, reported honestly rather than
guessed.** `logs/game.log.2026-08-21` is real but only 3,082 bytes and
stops dead at `06:06:12Z` - no startup line for ANY of today's later
restarts (08:37, 09:41, 13:55, all independently confirmed via file
mtimes/backups this session), no migration line, nothing. This is a
logging-pipeline gap, not a missing grep: the whole file was read.
`logs/bot.log.2026-08-21` has zero non-broadcast zolaries lines and
zero "duplicate" mentions anywhere today. `REPORTS/deploy.md` (mtime
11:11) was re-read in full and does not mention the 11:07 migration or
a 13:57 scan at all - its own narrative still ends at "BLOCKED - live
production deploy not performed," never updated after the swap
actually completed. `ORDERS/` does not currently exist on disk.
**Cannot independently confirm the 7-character migration scope, or
the 13:57 scan's method, or exactly when zolaries's duplicate
(re)appeared** - none of that evidence is reachable from here right
now. Items carry no creation timestamp, so item IDs can't date it
either.

**3. Full 60-character re-scan (same field, same 5-slot set as the
game's own migration logic - `EQUIP_SLOTS` in `item.rs:846`, verified
against source, not assumed).** Disagrees with the claimed 0:

| Character | Shared `unique_affix` | Slots |
|---|---|---|
| `xborntokillx` | celestialConversion | weapon, helm |
| `gorshie` | celestialConversion | body, gloves, boots |
| `zolaries` | splitPersonality | helm, body |
| `xdaido` | celestialConversion | **all 5 slots** |
| `drewm1022` | celestialConversion | weapon, body |
| `qugetus_` | splitPersonality | helm, body |

6 of 60, not 0. This is not isolated to zolaries and not a scan bug on
this end - the query matches the live game's own `has_conflicting_
unique_affix_value` grouping exactly.

**4. Verdict: a hole the live fix misses. Proven from source, not
inferred.** `Character::apply_unique_affix` (`character.rs:1767-1791`)
- the function `manager.rs:3935` calls when a player CONFIRMS a
pending Unique Shard picker choice - unconditionally executes
`item.unique_affix = Some(unique);` with **no conflict check at all**.
This directly contradicts its own codebase's doc comment
(`character.rs:1150-1158`), which claims `has_conflicting_unique_
affix_value` is "the ONE validator behind every mutation point that
can affect equipped uniques ... both unique-granting craft paths." It
isn't wired into this one. The REAL guard that exists and is tested
(`applying_to_an_equipped_item_offers_only_the_non_conflicting_
candidate`, `manager.rs:7581`) only filters candidates at PENDING-
CHOICE INSERT time (`craft_item_ex`) - a snapshot of what's equipped
*then*. Nothing re-validates at commit time. Two overlapping Unique
Shard picker flows on two different currently-equipped items (e.g.
helm and body) can each pass their own insert-time filter - neither
has committed yet, so neither sees the other as a conflict - and then
both commit unchecked, landing exactly the duplicate observed. This
reproduces on 5 more characters right now, post-swap (confirmed live
since 13:55 per this ledger's own Step 0 re-verification above), so
it is an ACTIVE gap, not 11:07-window debris specifically - though the
log gap in (2) means a pre-fix-recreation contribution can't be ruled
out either; both can be true at once.

**Remedy: code fix, not a re-run migration.** Re-running the migration
(delete marker + restart) would clean today's 6 characters but not
stop a 7th tomorrow - the commit-time gap is still open. Fix belongs
in `apply_unique_affix` itself (check `has_conflicting_unique_affix_
value` before committing, same as `equip_from_inventory`/`receive_
item` already do) or in invalidating a character's other pending
Unique Shard veils once one commits. Both are deploy-session/code
work, not parser's to fix - flagging with full evidence for that
session to act on.

## Open

**#45 — `pending_veils` holds one entry per player; a second veiled
craft silently orphans the first's spent token**
Flagged by the fix session while closing #44, untracked until now.
`PendingVeil` (manager.rs) is keyed one-per-player - starting a second
veiled craft (Unique Shard, Chancing, or any other veil type) while an
earlier one is still awaiting the player's choice overwrites the first
entry outright. The first craft's token was already consumed at
insert time (same convention every veiled craft uses - see #31's own
note on this), so the player loses that token with no error, no
choice ever offered for it, and no log line marking the loss. Not
investigated further this session - deploy/code territory, not
parser's. Needs: reproduction confirming the overwrite-not-merge
behavior, a decision on the intended fix (reject a second insert while
one is pending, vs. queue it, vs. something else), and log-observable
evidence of it actually recurring live before this can close either
way.

**#43 — Splash-roll success rate has no log-observable event**
See full write-up above (2026-08-21 later sweep). `roll_splash()` never
emits a `RollEvent` on any of its 6 call paths — structurally
unobservable from fight logs, not merely unexercised. Needs either a
new roll-log line added at the source, or acceptance that this
sub-claim can only ever be checked by re-deriving it from aggregate
splash-attack counts vs. known splash% (noisy, not exact).
**Accepted onto the post-reset backlog per owner instruction
(2026-08-21, later sweep) — no further work on this item until then.**

**Paladin Holy Fire fix — NOW LIVE (2026-08-21 ~13:55), verified by
hash + arithmetic, zolaries substituted for a different live Paladin**
Step 0 re-run per a later order, independently (not taken on the
order's own say-so): live `game.exe` sha256 = `b2412e3c982a2877...`,
file mtime 13:55:29, `game.exe` process CreationDate 13:55:38, both
`/overlay` and `/passives` return 200, `adventure-passive-overrides.toml`
confirmed to have ZERO `holyfire`/`holyfirewildfire` lines (`grep`
exit 1), git HEAD advanced to `5695295` (past the `8284cc3` baseline
the old `target-corpus` build was pinned to — explains why the live
hash differs from that older verified build rather than matching it),
patch-notes.json carries the real Paladin + duplicate-uniques entries
dated today. **Swap is real and healthy.**

**zolaries build-check (discipline: build-check before cross-fight
claims) — FAILED for this order's literal ask.** His 3 most recent
live fights (stage 2511/2512, sampled live, all consistent
`maxHp: 2961869`) all show `archetype: "monk"`, not Paladin. He does
not currently have Holy Fire available at all. Re-measuring HIM live
right now is not possible — the order's premise (that he's still on
Paladin) no longer holds; his build changed since he was last sampled.

**Substitute verification performed instead, on the 3 other live
Paladins in the same fight roster (`xcercs`, `sitch89`, `kibukah`) —
explicitly NOT zolaries, flagged as such.** One live fight
(stage 2512, `adventure-fights-detail/fight-0000005186.json`,
snapshotted before rotation), correlating each Radiant Smite's heal
total against the resulting Environmental-tagged burst against every
alive enemy (`apply_holy_fire_damage`'s own formula:
`damage = total_healed * dmg_pct`, applied flat, DR-only, to every
alive enemy).

| Sub-check | Result |
|---|---|
| Rate = 0.2535 (3/3 Holy Fire, 3/3 Wildfire, RB3) | **Exact live match**, 3 of 4 xcercs bursts: `unmitigatedDamage / totalHealed = 0.2535` to 4 decimal places. One burst (atMs 3200) read 0.0845 instead — investigated, not explained: every hit inside that burst was still internally flat/consistent (single `unmitigatedDamage` value, DR-only, no crit) as the code guarantees, so the fix's own output wasn't inconsistent; the discrepancy is most likely this sweep's own `total_healed` reconstruction picking up an extra heal event sharing that display-compressed `atMs` bucket that doesn't actually belong to that specific Smite cast (`events[].atMs` is display-compressed per this ledger's own Two Clocks discipline — not reconciled against `rolls[].atMs` this sweep). Flagged honestly rather than forced to fit. |
| Attacker stack provably absent | **PASS, exact, all bursts, all 3 casters.** Every burst shows exactly ONE distinct `unmitigatedDamage` value across every enemy hit — flat, uncorrelated with any per-target roll, matching `apply_holy_fire_damage`'s own `damage = total_healed * dmg_pct` (no attacker crit/increased-damage/multiplier stack folded in). |
| No crits, Environmental tag | **PASS, exact.** `isCrit: false` and `evaded: false` on literally every Holy Fire hit sampled (165+82+82 = 329 hits); `sourceKind` filtered to `"environmental"` throughout. |
| Reduced only by target's own DR | **PASS, live.** Each burst hit a mitigated `damage` consistent with a flat per-target DR (e.g. 9208/36833 = 0.7500 DR on every one of xcercs's 3 enemy targets in that burst) — same unmitigated base, different final damage only where a target's DR genuinely differed. |
| Burst escalation ~x1.8/step | **Not cleanly confirmed.** xcercs's clean bursts' `total_healed` went 145,297 → 163,348 → (anomalous burst) → 436,650 - ratios don't resolve to a clean ~1.8x without the anomalous burst excluded/explained first. Needs the real-time roll clock, not attempted this sweep. |
| Holy Fire share falls from 94% | **Not comparable as asked** — 94% was zolaries's own pre-fix figure; xcercs/sitch89/kibukah have no pre-fix baseline of their own to fall from. Not pursued further this sweep. |
| No Doom-pool contributions | **Not independently live-verified this sweep** — same evidentiary gap #29 already carries for Shattering (code + test only: `Environmental` tag excludes it from Doom accumulation by construction, per `apply_holy_fire_damage`'s own doc and the `holy_fire_emits_a_non_crit_non_evaded_hit` test). |

Recommend: re-run the literal zolaries ask once his Memory shows him
back on Paladin; until then this is the closest available live
evidence that the fix itself is delivering correctly in production.

**Opportunistic follow-up (2026-08-21, still later) — lokati_gaming,
note-if-seen only, no dedicated scenario built.** Build-checked first:
primary `elementalist`, `secondary_archetype: "paladin"`
(`holyfire:4, holyfirewildfire:3, risingblaze:3, smite:3` per the live
save) - "a Paladin" per the owner's note means via secondary
archetype, not primary; worth the nuance since this ledger's own
discipline flags him as the character known to swap builds.

- **Escalation (~1.8x/step): not seen.** Both sampled fights
  (`fight-0000005204`/`-5205`) show exactly ONE Holy Fire burst each -
  no consecutive casts to compare. No "Radiant Smite" skillCast event
  appears in either fight for anyone, so the burst fires without an
  explicit cast marker (consistent with how basic attacks/dots also go
  unlogged - not itself a red flag).
- **Share sanity check: clean pass.** Holy Fire is 0.0005% of his
  total damage in one fight (1,618,130 / 346,545,076,377) and ~0.00%
  in the other (122,285 / 418,131,322,956) - dramatically down from
  the pre-fix 94%, exactly the direction the fix should produce.
- **0.0845 outlier hypothesis - reproduced the ambiguity, didn't
  resolve it.** `fight-0000005205`'s one burst has TWO heal events
  sharing its exact `atMs` (lokati self 218,230 + jachiny 218,230) -
  precisely the "extra heal on a compressed tick" shape flagged
  earlier. Summing both (`total_healed=436,460`) gives an implied rate
  of 0.1267; using only the self-heal gives exactly 0.2535 (the same
  figure the earlier xcercs sweep measured, for a different rank
  combo). Neither matches the 0.338 his CURRENT save's ranks would
  predict (`0.05*4 * 1.30 * 1.30`). Not resolved - could be the same
  reconstruction ambiguity, or his ranks at fight-time genuinely
  differed from his ranks now (same caveat as always for this
  character). Flagging the shape, not forcing a verdict, per
  instruction.

**RECHECK, 2026-08-21, lokati as a genuine PRIMARY Paladin (owner
directive) - build-checked first, independently, not assumed.** Live
save confirms `archetype: "paladin"` now (`secondary: "warrior"`),
holyfire ranks unchanged (`4/3/3`). Two fresh fights actually run on
this build (`fight-0000005213`, `fight-0000005214` - a third,
`-5212`, still showed `elementalist` and was excluded).

**0.338-vs-0.2535 resolved, exactly, from source.** `holyfire`'s own
node spec (`passive_tree.rs:1056`) reads "15% at 3/3" - its `max_rank`
is 3, and the magnitude formula uses `effective_rank`
(`passive_tree.rs:474`), which clamps the stored 4 down to 3. Real
rate = `0.15 * 1.30 * 1.30 = 0.2535`, not the 0.338 this ledger wrongly
extrapolated last entry by assuming linear scaling past the node's own
cap. The 4th allocated point does nothing for this node.

**Rate, attacker-stack-absence, no-crit - reconfirmed, exact.**
`fight-0000005214`, two bursts (`atMs` 1200 and 2200): both flat,
single `unmitigatedDamage` value across every target (37 and 57 hits),
`isCrit: false` throughout. Burst 1's heal is UNAMBIGUOUS (single heal
event, no shared-tick collision): `54,882 / 216,496 = 0.2535` exact,
5 significant figures.

**0.0845 outlier: now resolved, not just reproduced.** Burst 2 shares
its `atMs` with a second heal (`roxus`, 216,496 - identical amount to
lokati's own self-heal) - the exact collision shape flagged before.
But burst 2's `unmitigatedDamage` is **54,882 - bit-for-bit identical
to burst 1's**, which is only possible if its true `total_healed` was
also 216,496, NOT the naive sum (432,992). The `roxus` heal is a
DIFFERENT skill's output landing on the same compressed tick, not a
second Holy-Fire-eligible heal: this fight's only other logged
skillCast is `"Divine Shield" x3`, and Divine Shield healing allies
(not just shielding) is the far more likely source than an unmodeled
second Smite trigger. **Confirms the original hypothesis was right**:
summing every heal event sharing a healer+atMs overcounts
`total_healed` when an unrelated heal skill fires on the same
compressed tick; only the causally-linked heal(s) belong in the
Holy Fire formula, and this ledger's earlier reconstruction method
needs that filter, not blind summation.

**Escalation (~1.8x/step): checked, genuinely flat - not confirmed.**
Two real consecutive casts, 1 second apart (`atMs` 1200 -> 2200,
squarely "rapid consecutive"), same `total_healed` (216,496 both
times) and identical resulting damage (54,882 both times). Ratio =
**1.00x, not ~1.8x.** Not force-fit: escalation requires the
`total_healed` itself to grow between casts (a stacking heal-power
source), and it didn't here - whatever escalation mechanism the
patch notes describe simply didn't engage in this window. Genuinely
open, not closed either way.

**Share sanity check - fight-dependent, reported both ways
honestly.** `fight-0000005214`: Holy Fire = 100.0000% of his damage
this fight - but only because it's his ONLY logged damage this fight
(2,112,920 total, tiny; no direct/dot/splash attacks landed at all) -
a small-denominator artifact, not renewed dominance. `fight-0000005213`:
0 damage entirely (a very quiet fight for him, one heal, no casts).
Contrast against the prior entry's two fights as secondary-Paladin
(0.0005%, ~0.00% of a MUCH larger total) - those remain the more
representative "share fell from 94%" evidence; this recheck's numbers
are real but from too small a sample to mean much on their own.

**#31 — +6 unique affixes granted vs 5 shards consumed**
Re-checked whether now verifiable, per today's order: **partially**.
`#32`'s new `picker-apply` log (see above) gives affix-grant visibility
(character, affix, timestamp) but does **not** log the shard-consumption
step itself — `craft_tokens` is decremented earlier, at
`craft_item_ex`'s insert time, and that step has no log line (confirmed
by grep; only test assertions cover it, e.g. `manager.rs:7406`). So
grant-count and spend-count still can't be cross-verified from logs
alone. Still WATCH, not re-opened — no recurrence evidence found (9/9
sampled picker-apply events this session show consistent, non-anomalous
balances). **Gap to flag**: consumption-side logging would be needed to
fully close this one.

**#35 — Thunder-reform-degradation inspection**
Description corrected today: prior entry here said "Tank-credit
observability" from a thin, unattributed carry-forward — this session's
`ORDERS/parser.md` refresh (2026-08-21) instead describes it as Thunder
Golem reform/redistribution degradation inspection, blocked behind
tree's own `#35`/`#36` observability instrumentation (not yet pushed).
That description is itself peer-relayed (order text), not independently
sourced by this session against an original ledger entry — none existed
before today. Owned by the tree session either way; not parser's to
close. See the kazesosa/thunder-golem investigation below for today's
related (but explicitly separate-tracked) work.

**#36 — Tank-credit shortfall**
Blocked on `#35` (tree's observability work), which per the order had
"not yet pushed" as of this morning. Not re-checked today.
*(Carried forward, not independently re-verified.)*

**hpSamples-collapse residual** *(unnumbered)*
Carried forward from the order's own context section with no further
detail available to this session. Not independently re-verified today.

**Guardian Spirit "exactly 2" untriggered** *(unnumbered)*
Carried forward. This is an **untriggered guard**, not a confirmed-absent
bug — per verification doctrine, that distinction stays open rather than
being closed by default. Not independently re-verified today.

**Two unexplained restarts** *(unnumbered)*
Explicitly deploy's item, not parser's — root cause tracked on deploy's
order. Not investigated here per standing instruction.

**countWs "flag retirement" — NEW finding today, likely a wrong-repo premise**
Checked today per order item 4 ("confirm it's fully retired, no dangling
references"). `countWs` does not exist anywhere in this repository's git
history — zero hits via `git log --all -S"countWs"` across all 239
commits and all branches, and zero hits in the current working tree.
It DOES exist in the separate desktop companion repo
(`C:\PathOfDust_Desktop-replay`), where it is **live, actively-called,
tested code** — `index.html:2529` (definition), `index.html:2653` (call
site), and covered by three cases in `wire.test.mjs`. This is not a
retired flag in this repository; recommend whoever authored this order
item re-check which repo/symbol was actually intended before treating it
as closed.

## Recommendation

`#24`, `#29`, `#32` closed 2026-08-21 (see above); `#40` closed
2026-08-21. That completes the golem-inheritance release's full
deferred-items set — all five are now resolved. `#31` remains WATCH,
gated on consumption-side logging that still doesn't exist. Remaining
open items (`#35`, `#36`, hpSamples-collapse, Guardian Spirit, the two
restarts) are owned by other sessions or explicitly out of parser's
scope.

**Later sweep, same day:** `#42` (replay bundle) closed live, exact,
on two production fights plus a live public-socket capture. `#43`
(splash-roll observability) opened — a real gap, not a false alarm.
Splash's other four sub-claims: cleave-1 and full-damage confirmed
live; Dragon-full-party circumstantially confirmed; Cube-4+1 and the
support-floor/zero-splash checks still need a follow-up sweep with
enemy-id-to-sprite mapping and character build data respectively. The
Paladin Holy Fire fix is pushed but NOT live — confirmed by binary
hash, not assumed from the deploy report's prose — so it was correctly
skipped rather than mis-measured against stale code.

**Final sweep, same day:** `#44` closed — deploy `3ead135` verified
live by hash (`game.exe 5bd11caef8...`, `bot 8d0716ae9...`), 60/60
characters clean, all 12 migrated items traced intact, and the
commit-time re-check confirmed directly in source with its own test.
One self-correction on the record: this ledger's own earlier "game.log
gap" for #44 was a UTC-vs-local timezone comparison error on this
session's part, not a real logging break — see the note inline above.
`#45` opened (pending_veils one-per-player token-orphan risk,
untested, deploy/code territory). Session end: this file is left
uncommitted per this session's standing instruction (the deploy
session commits it with its next release) — everything above is on
disk at `docs/anomaly_ledger.md` for that commit to pick up.
