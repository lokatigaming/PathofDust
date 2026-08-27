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
**#49 — Migrating a node from structure-only to override-aware ACTIVATES
any pre-existing entry for that key in the override store**
The 2026-08-27 incident write-up is in `docs/session_journal.md` under
the `DEPLOY-PASSIVE-TUNABLES-STAGE3` entry; the migration mechanics are
in the Stage 3 record in `docs/passive_tunables_spec.md`. Missing from
this ledger until backfilled 2026-08-28.

When a passive node is migrated from rank-fed (structure-only)
consumption onto a declared per-rank magnitude read through
`magnitude_at_rank` → `passive_override_for`, the override store begins
feeding it. An override written while that node was inert is stored
silently and applied silently the moment the node goes live — no warning
at write time, none at migration time, none at the swap.

Three shipped this way on the 2026-08-27 passive-tunables stage 3 release:

| node | pre-swap | went live as | affected |
| --- | --- | --- | --- |
| chakraoflife | 1000/2000/3000 ms | 330/660/1000 ms (~3x cut to Monk cheat-death immunity) | 4 monks |
| unyieldingspirit | 0.35/0.45/0.55 | 0.33/0.66/1.0 (Monk Last Stand effectively always-on at rank 3) | 8 monks |
| shattering | 1/2/3 targets | 2/4/6 targets (Elementalist icicle targets doubled) | 2 elementalists |

Live for roughly 20 minutes. All three reverted to their declared
defaults — which reproduce the old call-site values bit-exact — and
confirmed back at pre-swap behaviour.

**This is the MIRROR of the declaration-drift class.** Declaration drift
is "the declared per-rank table disagrees with what the call site
actually computes", and every stage of this feature has checked for it by
reading the call site. Migration makes BOTH halves live at once: the
declaration AND whatever the store already holds for that key. Only the
declaration was checked. The test suite cannot see the other half,
because every migration test pins DEFAULTS and the live store is by
definition not at defaults — which is exactly why a green suite and a
byte-identical golden corpus both passed while three live values moved.

**Prevention rule — `docs/passive_tunables_spec.md`, "Required
pre-migration step (2026-08-28, BINDING)".** That section is the
authoritative copy; it was moved there from `docs/session_journal.md` on
2026-08-28, on the reasoning that the journal is a narrative log while
the spec is what a migration session is actually told to read. The
journal keeps a one-line pointer, not a duplicate. The rule: *any
migration moving a node from rank-fed consumption onto a declared
per-rank magnitude MUST diff the LIVE override store against that
batch's node list BEFORE the swap, not after; every key in both is a
value about to change in production.* Nine nodes are still un-migrated
(six on `PENDING_MIGRATION_NODES`, three on `PARTIALLY_TUNABLE_NODES`),
so the rule is live, not historical. Note for whoever automates it:
procedural only — nothing in code or CI enforces it today.

Closed as an incident (remediated, store audited: the 33 remaining keys
were intersected against the 25 migrated, intersection empty, so nothing
else was activated). The CLASS is prevented by rule, not by a guard.

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

**#35 — Thunder Golem reform-cleanse (does a golem get WEAKER with every
reform?) — CLOSED 2026-08-22, code + dedicated test + confirmed live
deploy; live arithmetic on the HP-growth half, live-inconclusive on the
lifespan half.**

**Step 0.** Live `game.exe` sha256 `ab3a2b6551845f84...` = order's
`AB3A2B65`; live `twitch-bot-rs.exe` sha256 `9123860f0ffdc8bd...` =
order's `9123860F`; both mtime 2026-08-22 04:04. Local HEAD = `a1b285d`
= order's stated master. `patch-notes.json`'s newest entry (`"date":
"August 22, 2026"`) is headlined "Fixed: Thunder Golem Reform
Cleansing," confirming this exact fix shipped in the running binary, not
inferred from git history alone. All 6 fights sampled below
(`fight-0000006336` through `-6341`, detail tier) have file mtimes
10:30-10:41, well after the 04:04 build — genuinely post-deploy.

**Code fix, read directly.** `reform_thunder_golem` (`combat.rs:7078`)
now opens with `cleanse_player_debuffs(&mut units[golem_idx])` before
touching `max_hp`/`hp`/`alive` — `cleanse_player_debuffs`
(`combat.rs:7515`) zeroes `boss_focus_stacks`, `cube_shred_stacks` +
`cube_shred_expires_at_ms`, and `wound_stacks` + `wound_expires_at_ms`.
This is exactly the owner-reported mechanism: Gelatinous Cube's shred
(up to -50% DR, `CUBE_SHRED_DURATION_MS` 3000ms) and Festering Wound
used to ride into a fresh incarnation because reform mutated the same
unit in place and never touched these fields. A dedicated test,
`reforming_clears_cube_shred_wound_and_boss_focus_stacks_from_the_dead_
incarnation` (`combat.rs:17848`), calls the real production
`reform_thunder_golem` on a golem seeded with all three debuffs active
and asserts all five fields land back at zero — pins the fix exactly,
not a paraphrase. **Not independently re-run in isolation this
session** (relying on source inspection + the merge's own test-suite
gate per `REPORTS/deploy.md`'s prior merges, not a fresh `cargo test`
this sweep) — flagging that honestly rather than claiming a live test
run that didn't happen.

**Live sample.** Two characters currently run Thunder Golems
(`golem_slot_types` check against `adventure-characters.json`):
`kazesosa` (3 slots) and `roxus` (3 slots). Only `kazesosa` had active
golems in the 6 fights sampled — `roxus` summoned none in this window.
All 6 fights are the same "endless/horde" stage format (stage
2971-2973, 42-43 simultaneous enemies mixing `cube1` with
dragon/cthulhu/demon-fire/lich sprites, boss `hp` in the trillions,
`atk` ~0.6-1.9M every 74ms) — 9 golem-incarnation-sequences total.

| Fight | Golem | Incarnation | absorbed | redistributed | maxHp | lifespanMs |
|---|---|---|---:|---:|---:|---:|
| 6336 | \_0 | 0/1/2 | 24818942 / 60582086 / 79182455 | 12409471 / 30291043 / 39591228 | 19460618 / 48651545 / 77842472 | 200 / 0 / 0 |
| 6336 | \_1 | 0/1/2 | 34198216 / 58326663 / 82300930 | 17099108 / 29163332 / 41150465 | 19460618 / 48651545 / 77842472 | 200 / 0 / 0 |
| 6336 | \_2 | 0/1 | 25700966 / 53360076 | 12850483 / 26680038 | 19460618 / 48651545 | 1200 / 0 |
| 6337 | \_0 | 0/1/2 | 19584952 / 50257642 / 77860844 | 9792476 / 25128821 / 38930422 | 19460618 / 48651545 / 77842472 | 200 / 22 / 52 |
| 6337 | \_1 | 0/1/2 | 21756089 / 53183730 / 83677298 | 10878045 / 26591865 / 41838649 | 19460618 / 48651545 / 77842472 | 200 / 22 / 52 |
| 6337 | \_2 | 0/1/2 | 24866848 / 50060846 / 79736721 | 12433424 / 25030423 / 39868361 | 19460618 / 48651545 / 77842472 | 200 / 74 / 0 |
| 6338 | \_0 | 0/1 | 21497065 / 49526915 | 10748533 / 24763458 | 19460618 / 48651545 | 200 / 170 |
| 6338 | \_1 | 0/1 | 22161770 / 49843768 | 11080885 / 24921884 | 19460618 / 48651545 | 200 / 192 |
| 6338 | \_2 | 0/1 | 21713200 / 49394551 | 10856600 / 24697276 | 19460618 / 48651545 | 200 / 244 |
| 6339 | \_0 | 0/1 | 20797742 / 50311961 | 10398871 / 25155981 | 19460618 / 48651545 | 200 / 0 |
| 6339 | \_1 | 0 | 19513427 | 9756714 | 19460618 | 1200 |
| 6339 | \_2 | 0 | 22409057 | 11204529 | 19460618 | 2200 |
| 6340 | \_0 | 0/1/2 | 20657687 / 52421316 / 87284837 | 10328844 / 26210658 / 43642419 | 19462565 / 48656413 / 77850260 | 200 / 0 / 0 |
| 6340 | \_1 | 0/1/2 | 19679359 / 51423816 / 79708086 | 9839680 / 25711908 / 39854043 | 19462565 / 48656413 / 77850260 | 200 / 0 / 0 |
| 6340 | \_2 | 0/1/2 | 29177844 / 49744032 / 78726840 | 14588922 / 24872016 / 39363420 | 19462565 / 48656413 / 77850260 | 200 / 0 / 0 |
| 6341 | \_0 | 0/1/2 | 24408111 / 50280506 / 78000602 | 12204056 / 25140253 / 39000301 | 19462565 / 48656413 / 77850260 | 200 / 0 / 72 |
| 6341 | \_1 | 0/1/2 | 21029276 / 49180289 / 77990988 | 10514638 / 24590145 / 38995494 | 19462565 / 48656413 / 77850260 | 200 / 16 / 56 |
| 6341 | \_2 | 0/1/2 | 24937580 / 54068818 / 83631671 | 12468790 / 27034409 / 41815836 | 19462565 / 48656413 / 77850260 | 200 / 72 / 72 |

**HP growth (does the golem get bigger, not smaller): PASS, exact, all
18 incarnation-pairs.** `max_hp` steps UP every reform in every single
sequence above, matching `base × (1 + growing_pct × reform_count)`
exactly — e.g. fight 6340's `_0`: `19462565 × (1+g×1) = 48656413` and
`19462565 × (1+g×2) = 77850260` both solve to `g = 1.500000...` to 7
significant figures, both reforms, independently. That `g=1.5` (150%)
does NOT match the "growing" node's compiled rank-3 spec
(`passive_tree.rs:2132`: `0.33 + 2×0.335 = 1.00`, i.e. 100%) — checked
this as a possible new bug, then resolved it as NOT one:
`adventure-passive-overrides.toml` carries a live-tunable override
`growing = [0.5, 1.0, 1.5]`, and kazesosa's own `passive_allocations`
(`adventure-characters.json`) confirms `growing: 3` (at cap, no
over-allocation) — rank 3's override value is exactly `1.5`, matching
the live golem data to 7 significant figures. Owner's own deliberate
tuning via `/admin/passives`, same shape as ledger `#30` — not
re-flagging as an anomaly.

**Lifespan trend: live-inconclusive, reported honestly rather than
forced.** No clean shrink/hold/grow pattern across the 18 pairs above —
some sequences shrink to a 0ms floor (6336 `_0`/`_1`, 6340 all three),
some shrink then partially recover (6337 `_0`/`_1`: 200→22→52), some
grow monotonically (6338, 6341 `_1`/`_2`). Root cause: at this specific
live stage (2971-2973), incoming burst from 42-43 simultaneous enemies
overwhelms even the largest incarnation (77.8M `maxHp`) within
0-250ms of real sim time regardless of reform count — a floor/ceiling
effect from raw incoming damage, not a signal of the golem's own
defensive state. This stage is not a useful venue to detect the
owner's original symptom (which described a longer, sustained fight);
it's evidence for the HP-growth claim above but not for the
DR-carryover claim.

**Cube-vs-non-Cube split: NOT POSSIBLE from current live data, structurally.**
The order's premise (discrete Cube fights vs discrete non-Cube fights)
doesn't hold at kazesosa's current stage — every one of the 6 sampled
fights is the same mixed horde format with `cube1` sprites present
*alongside* dragon/cthulhu/demon-fire/lich sprites simultaneously
(`bossSprites` arrays checked directly). There is no live non-Cube fight
to contrast against right now. Not a finding either way — a genuine gap
in what's currently being played, not something this session can
manufacture.

**Direct shred-carryover check, attempted, inconclusive.** "Gelatinous
Cube shred" mitigation rolls (`category:"mitigation"`,
`source:"Gelatinous Cube shred"`, `magnitude:-0.1`) were found hitting
kazesosa's golems in 3 of 6 fights (6340 `_0`×1, 6340 `_1`×3, 6341
`_1`×1) — confirms Cube shred is genuinely being exercised against
these golems live, not merely a code path that never fires. Per this
ledger's own Two Clocks discipline, `rolls[].atMs` is real and
`thunder_incarnations[].lifespanMs` is ALSO real (both come from the
same pre-`compress_events` `unit_infos` builder in `combat.rs`, unlike
`events[].atMs` which IS display-compressed by that later pass) —
reconciled by reconstructing each incarnation's real-clock window from
its own `lifespanMs` plus the confirmed rank-3 reform delay
(`thundergolem:4` allocated, clamped to rank 3 via
`PassiveNode::effective_rank`'s Specialization floor at 3, giving
2000ms per `passive_tree.rs:2543`'s own test). Fight 6340 `_0`'s shred
roll (`atMs=4200`) reconstructs to the FIRST instant of that
incarnation's window; fight 6341 `_1`'s shred roll (`atMs=4272`)
reconstructs to essentially the LAST instant of its incarnation. Two
data points landing at opposite ends of the incarnation lifecycle
argues against a systematic carryover pattern (which would cluster
every shred detection right after every reform) but isn't proof either
way without an enemy-id-to-sprite mapping to confirm which specific
attacker (Cube or not) is credited each time — same structural gap
`#43` already flagged for splash. Reporting the shape, not forcing a
verdict, per this file's own discipline.

**Verdict: CLOSED**, same basis as `#29`'s prior closure (code fix read
directly + a dedicated regression test pinning the exact reported
mechanism + confirmed live deployment by hash) — the specific thing the
owner reported (Cube shred/Festering Wound surviving a reform) is
provably removed from the code path, and the HP-growth half of "weaker
every reform" is live-confirmed correct and non-regressing. The
lifespan half could not be cleanly isolated live this sweep (floor
effect at the only stage currently being played, no Cube-free contrast
fight, no enemy-id map) — flagged as the live-exercise gap rather than
papered over.

**#36 — Tank-credit shortfall (sum of per-incarnation net absorption
must equal `thunderNetAbsorbed`) — CLOSED 2026-08-22, live, exact/near-exact.**

| Fight | Golem | sum(absorbed−redistributed) | thunderNetAbsorbed | diff | tolerance (`n×2+1`) |
|---|---|---:|---:|---:|---:|
| 6336 | \_0 | 82291741 | 82291742 | −1 | 7 |
| 6336 | \_1 | 87412904 | 87412905 | −1 | 7 |
| 6336 | \_2 | 39530521 | 39530521 | 0 | 5 |
| 6337 | \_0 | 73851719 | 73851719 | 0 | 7 |
| 6337 | \_1 | 79308558 | 79308559 | −1 | 7 |
| 6337 | \_2 | 77332207 | 77332208 | −1 | 7 |
| 6338 | \_0 | 35511989 | 35511990 | −1 | 5 |
| 6338 | \_1 | 36002769 | 36002769 | 0 | 5 |
| 6338 | \_2 | 35553875 | 35553876 | −1 | 5 |
| 6339 | \_0 | 35554851 | 35554852 | −1 | 5 |
| 6339 | \_1 | 9756713 | 9756714 | −1 | 3 |
| 6339 | \_2 | 11204528 | 11204529 | −1 | 3 |
| 6340 | \_0 | 80181919 | 80181920 | −1 | 7 |
| 6340 | \_1 | 75405630 | 75405631 | −1 | 7 |
| 6340 | \_2 | 78824358 | 78824358 | 0 | 7 |
| 6341 | \_0 | 76344609 | 76344610 | −1 | 7 |
| 6341 | \_1 | 74100276 | 74100277 | −1 | 7 |
| 6341 | \_2 | 81319034 | 81319035 | −1 | 7 |

18/18 golem-fight instances: 4 exact, 14 off by exactly −1 (f64→u64
`.round()` accumulation across 1-3 incarnations, consistent with the
production code's own documented tolerance formula
`(incarnations.len() as i64) * 2 + 1`, `combat.rs:18569` — no instance
exceeds it). Arithmetic holds cleanly; no shortfall anywhere in this
sample. The known confound (a DoT armed near fight end that never gets
to deliver) wasn't a factor here — every recipient list was non-empty
and every redistribution completed within-incarnation. **Verdict:
CLOSED, exact.**

## Open

**#51 — `/admin/tunables/save`, `/admin/passives/save` and
`/admin/passives/revert` answer a non-admin POST with a bare redirect and
no status code, indistinguishable from success**
Recorded by the deploy session 2026-08-28 during the World 2 Stage 2
release verification, per the order. All three admin POST handlers gate
on `current_session` + a plain `ADMIN_TUNABLES_LOGIN` equality check, and
a session that fails that gate falls through to the same redirect the
success path returns:

- `do_save_passive_override` (`adventure_web.rs:2751`) — both the
  no-session and the wrong-login branch `return redirect()`, and that
  redirect is `/admin/passives?class={slug}&saved=1`. A refused request
  is answered with a URL that literally asserts `saved=1`.
- `do_revert_passive_override` (`adventure_web.rs:2836`) — the admin work
  sits inside `if let Some(..) { if login == ADMIN { .. } }` with no else,
  so a non-admin falls straight through to the tail redirect.
- `do_save_tunables` (`adventure_web.rs:3125`) — same shape, tail
  `Redirect::to("/admin/tunables?saved=1")`.

Nothing is written in any of these cases, so this is not a privilege
escalation — the store is untouched. The defect is that the caller cannot
tell a refusal from a success: no 401, no 403, no message, and a
`saved=1` query parameter that the receiving page renders as a
confirmation. Same **outcome-does-not-match-response class** as `#46`,
`#47` and `#50`.

**`/admin/ops/next-encounter`, shipped in this release, establishes the
correct pattern** and is the reference to bring the three older routes
to: every outcome, refusal included, returns a visible page naming the
exact condition with a status code that matches it — `403 Forbidden`
"Refused - not the operator", `409 Conflict` "Refused - a fight is in
progress" / "Refused - operator action already running", `400` "Refused -
unrecognized boss". Both refusal shapes were exercised live during this
deploy's checks 13 and 15 and returned the documented codes.

Verified in source and live: a non-admin POST to
`/admin/ops/next-encounter` returned `403` with the refusal text, while
the same session's POST to the three older routes hit their form
extractors first (`422`, a body-shape rejection) — the bare redirect is
the behaviour reached once a well-formed body gets past the extractor,
which is what the handler source above shows.

Status: **OPEN, not fixed.** Scope is the refusal branch of three
handlers; no change to what any of them does on the success path.

**#52 — rampage removal-stage deletion candidates (pending work, not a
defect)**
Recorded by the deploy session 2026-08-28 on the World 2 Stage 2 release,
as reported by the feature session. `permanent_rampage` becomes the only
rampage state; everything below exists only to serve the retired
countdown/vote flow and comes out at the removal stage:

- State and tunables: `rampage_remaining`, `rampage_notify`,
  `rampage_votes`, `RAMPAGE_VOTE_THRESHOLD`, `RAMPAGE_ENCOUNTER_COUNT`.
- Types and functions: `RampageVoteOutcome`, `start_rampage`,
  `register_rampage_vote`, `persist_rampage_remaining`,
  `announce_rampage_complete`.
- The countdown branch of `spawn_rampage_loop`.
- Persistence: `RAMPAGE_STATE_PATH` and `adventure-rampage-state.json`,
  together with that file's entry in the backup script's allow-list
  (`backup-game-data.ps1`, `$CoreFiles`) — the allow-list entry must come
  out with the file, or the backup's own manifest-drift check starts
  reporting a missing core file on every run.

Status: **PENDING REMOVAL-STAGE WORK.** Not a defect and nothing to fix
now; recorded so the removal stage has the list it was derived from
rather than re-deriving it.

**#50 — `/admin/passives` Revert DELETES the override instead of
restoring a prior value; deliberate tuning is destroyed with no undo**
Found by the deploy session 2026-08-27 during the identity+units release
verification, recorded not fixed, per the order. The Revert control on a
node row calls `do_revert_passive_override`, which removes that key from
`adventure-passive-overrides.toml` outright and returns the node to its
compiled-in declared default. There is no prior-value history, so on a
node the owner has deliberately tuned, one click discards the tuning
permanently — the only recovery is a backup snapshot or remembering the
numbers.

Same **silent-destruction class** as the two save-path defects fixed in
`#46`/`#47`: an admin control whose visible outcome does not match what
it actually did to stored state. The save path lied by reporting success
for a value it dropped; Revert reads as "undo my change" and instead
performs "delete the tuning". The distinction only matters for a node
whose stored value IS the intended one — which is the normal case for a
tuned node, and precisely the situation in which the control is most
tempting to click.

Concrete consequence, observed: during check 16 of the identity+units
deploy the verification needed `volley`'s above-1 fraction warn/confirm
path exercised and then undone. Reverting via the control would have
deleted the owner's stored `0.5 / 1.0 / 1.5` and dropped the node to its
declared default, so the value was restored by hand (re-saving 1.5
through the same warn/confirm path) rather than using Revert. A
verification step having to route around a control to avoid destroying
live data is the finding.

The question put to the owner was the intended shape: a confirm prompt
naming what will be lost, a "restore previous value" keeping one step of
history, or accepted-as-is with the control relabelled. Revert-to-default
is a legitimate operation and may be exactly what is wanted; the defect
is that the control does not say so. Answered below.

**RULED 2026-08-28 (owner) — NOT FIXED.** The ruling, so the eventual fix
is unambiguous:

> **Revert is not to become an undo.** Dropping the override IS the
> correct behaviour — it returns the node to its declared default. The
> defect is the LABEL and the ABSENCE OF CONFIRMATION, nothing else.

The fix, when scheduled:

1. **Relabel** the control to state plainly that it clears the override
   and returns the node to its default — not "Revert", which reads as
   "undo my last change".
2. **Require an explicit confirm** before acting, using the
   warn-then-confirm pattern already shipped for above-1 fraction values
   in `#46` (a warning naming what is about to happen, and a second POST
   carrying `confirm=1`). Reuse that path rather than inventing a second
   confirmation mechanism.

**Explicitly ruled OUT: no value history, no restore-previous.** Do not
add a prior-value store, an undo stack, or a "restore last value"
control. The deletion semantics stay exactly as they are.

Status: **RULED, not fixed.** Scope is a label and a confirm gate on
`do_revert_passive_override` plus its page control; no change to what the
handler does to the store.

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

**#35 — Thunder-reform-degradation inspection — CLOSED 2026-08-22, see
Closed section above (new entry, this date).** The blocking observability
(`CombatUnitInfo.thunder_incarnations`) shipped and is live; verified
against it directly.

**#36 — Tank-credit shortfall — CLOSED 2026-08-22, see Closed section
above (new entry, this date).** Arithmetic checked exact/near-exact
across 9 live golem-incarnation sequences.

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

## Deploy record — 2026-08-23, dynamic pacing release

Entered by the deploy session at merge of `feature/dynamic-pacing`.
These are **known consequences recorded ahead of time, not bugs and not
parser findings** — they carry no `#NN` because the log parser owns this
file's numbering. Each exists so a future investigation starts here
instead of rediscovering it.

**Overlay broadcast worst-case event volume up ~50%**
A maximally dense fight's worst-case event count rises from roughly
52.5k to roughly 78.8k. Cause is known and structural, not a leak: event
thinning budgets per DISPLAY-second, and the display timeline got longer
(the playback ceiling moved 35s → 45s, derived from the pacing window
rather than a constant). More display-seconds at the same per-second
budget is more events. If overlay lag is ever reported after this
release, start from this line — the volume increase is expected and the
question is whether the consumer keeps up with it, not where the extra
events came from.

**Fixture regeneration moved TIMING as well as damage, in 8 of 14**
The deploy order predicted damage would change and timing would not.
Timing moved: `party_ally_targeted_passives_stage500` 41032 → 46207 ms,
`ranger_vs_lich_stage3000` 6000 → 9800 ms, and
`berserker_vs_lich_stage1000` 8300 → **7800** ms. All attributable to
ADDITION 4's top layer: enemies take less damage, so they survive longer
and keep acting — wins lengthen, and the one loss shortens because the
enemy lives long enough to finish the player sooner. `won` is unchanged
in all 14. Recorded because the order's prediction was wrong, and the
next person to diff these fixtures should not read moved timestamps as
an unexplained second cause.

**Stage 3000: the top layer alone stretched a scenario ~2.4x in event
count** — `ranger_vs_lich_stage3000` went 87 → 211 events (rolls 485 →
1236) at layer 0.400. The golden corpus runs with hand-authored boss
stats and no live pacing controller, so this is the RAW top-layer effect
with nothing compensating. Live, Controller A is supposed to absorb it
by pulling HP down. **This is the high-stage case to watch after
release**: if fight duration at stage 3000+ does not settle back into
the 30–45s window within the pacing window's 20 fights, the top layer
and Controller A are fighting each other and arbitration (the known next
step per the spec's own "known limit") is what to reach for.

## Deploy record — 2026-08-23, ops scripts release (`eab210a`)

Entered by the deploy session at merge of `fix/ops-backup-and-watchdog`.
Standing-condition records, **not bugs and not parser findings** — no
`#NN`, since the log parser owns this file's numbering. Same convention
as the dynamic-pacing record above.

**Until today the game had NO scheduled backup of character data. None.**
Not degraded, not partial — the mechanism did not exist. Everything that
had ever functioned as a backup was incidental to some other action:

1. **Migration-time** `.pre-*-backup` copies, written only by whichever
   one-time migration happened to be pending on a given start. Those are
   the `adventure-characters.json.pre-*` files sitting at repo root.
2. **Deploy-time** `backup-pre-<name>/` directories, written by hand as
   REFACTOR_PLAN.md §13 step 4.

Both are tied to a **deploy**. The consequence worth writing down, because
it is the thing nobody had said out loud: **a world that is not being
deployed to receives no backups at all.** That is precisely the condition
a frozen legacy world would be in — the scoping work that found this was
scoping a second deployment in which the existing world stops receiving
code changes. Freezing a world would have silently removed the only
backup mechanism its save data had.

**The August 2026 UTF-8 BOM incident was recoverable only because a deploy
had just happened to make a copy.** A BOM on `adventure-characters.json`
took out all 60 characters. The recovery source was a deploy-time
`backup-pre-*` directory that existed by coincidence of timing, not by
design. Had the same corruption landed a week into a quiet period with no
release, there would have been nothing to restore from. The roster was
saved by luck. Recorded so that no future investigation reads that
recovery as evidence that a backup system worked — there was no system.

Two related facts from the same day, for whoever next audits data safety:

- The game persists with `std::fs::write` (`game/src/state.rs`), which
  truncates and then writes. Any copy taken inside that window is a
  truncated file that is perfectly valid on disk and useless as a backup.
  Any future backup tooling must parse what it copied before trusting it;
  `backup-game-data.ps1` does, and refuses to prune on a failed verify.
- The BOM itself is no longer fatal — `state.rs:61` strips it with a
  warning, and `load_json_fail_loud` refuses to start rather than
  overwriting good data with an empty default. The *absence of backups*
  was the unaddressed half, and it stayed unaddressed for four months
  after the incident.

**Shipped today:** `backup-game-data.ps1`, hourly-for-24h then
earliest-of-day-for-30d, verified-before-prune, safe against the live
process. **The scheduled task that drives it was NOT registered this
session** — `Register-ScheduledTask` and `schtasks /Create` both return
Access denied from the deploy session's non-elevated token. Until an
elevated operator registers `GameDataBackup` (exact command in
`docs/ops_backup_and_watchdog.md`), **the backup exists but nothing is
running it**, and every statement above about having no scheduled backup
remains true of production. This is the open item.

## Deploy record — 2026-08-23, pacing control-loop fixes (`0110be6`)

Entered by the deploy session at merge of `fix/pacing-controller-loop`.
No `#NN` — the log parser owns this file's numbering.

**CAUSE FOUND: fixed-step control on the damage axis caused the
ten-win/ten-loss oscillation observed in production on 2026-08-23.**
Controller B requested a FIXED `cur * (1 + dmg_max_step_per_fight)` up or
`cur / (1 + step)` down — the same size move whether the rolling win:loss
ratio was 2.1:1 or 20:1. Fixed steps near equilibrium hunt by
construction: the smallest correction available is also the largest one,
so the controller cannot settle onto its target, only step over it and
back. That is the mechanism behind the swings, and it is structural
rather than a tuning miss — no value of `dmg_max_step_per_fight` removes
it, because the defect is that the step does not depend on the error at
all.

Measured on one alternating outcome stream driven through both control
laws (`controller_b_oscillates_less_than_the_fixed_step_law`, which keeps
the retired law as its in-test baseline): **14.23x peak-to-trough under
the old fixed-step law against 11.16x under the new proportional one.**
B now scales step MAGNITUDE by `|tanh(ln(observed / target))|` with the
existing rate limit still applied as a cap on the request. Near target
the step approaches zero.

Two further notes from the same investigation, so a future reader does
not re-derive them:

- **Controller A was never the fixed-step one.** The order that opened
  this work attributed the fixed-step defect to A. A has always requested
  `mean_dps x midpoint / base_pool` — the closed-form correction, i.e.
  the error itself — with the rate limit as a cap. Two regression tests
  now pin that. The live symptom attributed to A (1.0 → 30x over ~15
  wins) is `1.25^15 ~= 28.4`: A taking its maximum *capped* step every
  fight because the honest proportional request was far above it. A was
  converging correctly on a duration target; what made the party
  unwinnable is the duration/lethality outcome coupling the spec already
  flags as a known limit.
- **A could not descend during a losing streak, and the stage walk made
  it worse.** A samples wins only (correct — a wipe reads as a short
  fight). But a loss walks the stage back 2, which SHRINKS the organic
  pool, which makes A's required multiplier RISE. So a losing streak
  actively pushed A further up. Both live incidents needed a manual
  override to break out. A relaxation path now decays A back toward
  neutral after consecutive losses.

**Deploy-process findings, both about scripts deployed earlier the same
day — neither blocked this release:**

- `Disable-ScheduledTask -TaskName 'GameProcess-Watchdog'` returns
  **Access denied** from the deploy session's non-elevated token, the
  same limitation that blocked `Register-ScheduledTask` in the ops
  release above. §13 step 4's "disable the watchdog" step therefore did
  not happen: the stop/swap window ran with `GameProcess-Watchdog`
  enabled. It did not fire (the swap took seconds against a ~2-minute
  repetition interval) and the deploy is unaffected, but the step is
  currently unperformable as written and a future deploy with a slower
  swap could race it. Either an elevated operator performs the toggle or
  §13 should stop claiming a non-elevated session can.
- `game-watchdog.ps1` resolves **`$ExpectedPathRoot` to EMPTY** under the
  exact invocation its scheduled task uses
  (`powershell.exe -NoProfile -ExecutionPolicy Bypass -File ...`). The
  default is `[string] $ExpectedPathRoot = $PSScriptRoot` in the *param
  block*, where `$PSScriptRoot` is not yet populated; the `$LogPath`
  default avoids this by resolving in the body instead and does work.
  **No impact today** — at `RunLevel = Limited` the listener's image path
  is unreadable anyway, so the verdict is `unverifiable` either way, and
  `-RequireOwnPath` is off. It matters the moment someone raises the task
  to `RunLevel = Highest` expecting the path check to start confirming:
  with an empty root, `Test-UnderRoot` returns `$null` for every
  candidate, so every listener stays `unverifiable` and the check they
  raised the run level to obtain silently never happens. Verified live by
  dry run against the restarted game this release: `expected root :` came
  back blank while `log :` resolved correctly to
  `C:\PathofDust\game-watchdog.log`.

## Deploy record — 2026-08-24, watchdog maintenance gate (`2cf4cfd`)

Entered by the deploy session at merge of `fix/watchdog-maintenance-gate`.
No `#NN` — the log parser owns this file's numbering. Scripts and docs
only: no Rust, no binary swap, neither the game nor the bot stopped or
restarted at any point.

**REFACTOR_PLAN.md §13 step 4 was unperformable from a non-elevated
deploy session, and had been silently skipped on every deploy to date.**
The step instructed "disable `GameProcess-Watchdog`" before the binary
swap and "re-enable" it after. `Disable-ScheduledTask` requires an
elevated token; a deploy session does not have one and gets
`Access denied`. Because the failure was a denied cmdlet rather than a
crash, nothing downstream noticed — the deploy simply carried on with the
step not done.

**Consequence, stated plainly: the 2026-08-23 pacing deploy ran its whole
stop/swap window with `GameProcess-Watchdog` live.** It did not fire, and
the deploy was unaffected — but only because the swap finished well
inside the task's ~2 minute repetition interval. That is luck, not
protection. A slower swap (a large backup, a retried copy, a stalled
disk) races it, and the watchdog would have restarted the game off a
half-written `game.exe`. The same was true of every earlier deploy in
this repo's history; the 2026-08-23 one is simply where it was noticed
and written down. The bot half of the step (`TwitchBotRS-Watchdog`) had
the identical defect and the same silent skip.

Fixed by suppression that a non-elevated session CAN perform: a flag
file. `maintenance-flag.ps1 -Target Game|Bot -Set/-Status/-Clear` writes
and removes it; both watchdogs honour their own. The flag is a **lease,
not a switch** — one older than 30 minutes (or unreadable, undated, or
dated in the future) is IGNORED, the fact logged loudly, and protection
resumes. A forgotten flag disabling protection indefinitely would be a
worse and quieter failure than the one being fixed, so every ambiguous
case fails toward protecting the world. §13 step 4 and its conditional
bot branch were rewritten to match.

**Both watchdogs now detect by LISTENING PORT rather than by image
name.** The game side moved on 2026-08-23; the bot side moved with this
release (`watchdog.ps1` had still been using
`Get-Process -Name "twitch-bot-rs"`). Image-name detection cannot
distinguish two deployments: the check returns non-empty whenever EITHER
process is alive, so standing up a second deployment would have silently
un-protected the first — each watchdog reading the other deployment's
process as proof its own was alive. Port detection is per-deployment by
construction and uses the same port-to-PID resolution CLAUDE.md's
PRODUCTION SAFETY rule already requires. Neither script terminates
anything, by image name or otherwise.

The bot's port was chosen from the code, not assumed: 4001 (alerts) is
unconditional and the earliest of its three, and `start_alert_server`
awaits `TcpListener::bind` before returning. 4002 was disqualified as
CONDITIONAL — it only binds when `config.youtube_api_keys` is non-empty,
so clearing the YouTube keys would have made a watchdog keyed to it
restart a healthy bot forever. 4003 binds last, behind a Twitch
round-trip that would read as death when slow.

**Two false-confidence defects found and fixed alongside**, both of the
same shape — a safety check that reports success while doing nothing:

- `game-watchdog.ps1`'s `$ExpectedPathRoot` defaulted to `$PSScriptRoot`
  in the PARAM BLOCK, where that variable is empty under the `-File`
  invocation the scheduled task uses. The root arrived empty,
  `Test-UnderRoot` returned `$null` for every candidate, and every
  listener resolved `unverifiable` regardless of where it lived — so
  `-RequireOwnPath` silently did nothing. Harmless at today's
  `RunLevel = Limited` (paths are unreadable anyway), but raising the
  task to `Highest` — the ruled prerequisite for a second deployment —
  would have bought false confidence instead of the check it was raised
  for. `watchdog.ps1` never had this defect: it had no param block at
  all. Resolved in the body, as `$LogPath` always was.
- The first draft of `maintenance-flag.ps1` defaulted its flag path off
  its own `$PSScriptRoot`, so running the helper from a worktree — where
  a deploy session naturally has a shell open — wrote a flag the live
  watchdog never reads while `-Status` still reported `SUPPRESSED`.
  Caught in review by a second session before it shipped. `-Set` now
  resolves the authoritative root from the scheduled task's own action
  and refuses to write anywhere else; `-Status` always ends with a
  `scope :` line stating whether the flag it just described is the one
  that task actually reads.

**The two flags are separate files** (`game-watchdog-maintenance.flag`,
`bot-watchdog-maintenance.flag`) and that is load-bearing: §13 deploys
the game unconditionally and the bot only when the diff says so, so a
single shared flag would have suppressed the BOT's watchdog through every
game-only deploy — exactly the window in which §13 says the bot runs
untouched. A bot crash there would have gone unrecovered.

**Still open, deliberately:** neither watchdog task's `RunLevel` was
changed by this release. Elevation and `-RequireOwnPath` remain gated on
the second deployment, per the owner's ruling. Until then both listeners'
image paths stay unreadable and both watchdogs resolve
`listening-unverifiable` on a healthy process — which is treated as
healthy, and is why the restart decision deliberately hinges only on the
port.

## Deploy record — 2026-08-24, passive-tunables release (`487aaf4`) — BACKFILLED

Entered retroactively by the divinity-and-item-locks deploy session
(2026-08-24, later the same day) because this record was never written
at deploy time — the last record on file was the watchdog-maintenance-
gate entry above, itself docs/scripts-only with no binary swap. This is
a genuine gap: `487aaf4` DID swap the live `game.exe` (Stage 0 admin-
passives-honesty + Stage 1 overflow-economy `LiveTunables`), and that
was the binary the divinity deploy found live and hashed against when
it started.

**What shipped:** Stage 0 (`d246ee6`) made `/admin/passives` honest — it
now tracks rank-only-consumed nodes and half-tunable notes, plus a CI
guard against future drift. Stage 1 (`35cccba`) added the overflow-
economy `LiveTunables`: one cap on how much any single passive
conversion node can output per rank, and four more setting where
Evasion, Block, Damage Reduction and Intervene saturate and start
feeding conversions. Every new default reproduces the previous
hardcoded numbers exactly — patch notes correctly filed this
`Internal:` (no player-facing change at deploy; nothing moves until an
admin deliberately retunes a cap).

**Live incident during this deploy — the reason §13 step 4a exists.**
The first `GameProcess` restart raced the binary copy and died
immediately, `LastTaskResult=1` — the scheduled task tried to start the
new exe before the copy had finished landing. The retry cost roughly 90
seconds of live downtime before the game came back healthy. This is the
deploy that motivated writing the numbered swap recipe now documented as
§13 step 4a (`docs/section-13-swap-recipe`, merged into master by the
divinity-and-item-locks deploy immediately ahead of this backfill) — its
ordering (rename-not-overwrite, poll the port before renaming, copy-
then-start never start-then-copy) exists specifically to close the race
that caused this incident.

**Not independently re-verified** — reconstructed from REFACTOR_PLAN.md
§13 step 4a's own account of the incident (written by the session that
fixed it) plus the merge commit's diffstat, not from this session's own
live observation of the 2026-08-24 11:55 deploy. *(Carried forward, not
independently re-verified, per this file's own convention for entries
built from a prior session's summary rather than fresh verification.)*

## Deploy record — 2026-08-24, Divinity and item locks (`ce32085`)

Entered by the deploy session at merge of `docs/section-13-swap-recipe`
(`b64c27d`) followed by `feature/divinity-and-item-locks` (`ce32085`)
into master, base `d730be7`.

**What shipped:** a new whole-bag crafting action, Divinity (1 Unique
Shard, runs the full Hideout Warrior chain over every eligible bag item
at once, no Dust cost, Krangled items auto-named "From Divinity"); the
Keep lock now blocks ALL item modification, not just disenchant (crafts,
Polish, Reforge, Divine Dust, unique apply, Recombine as either input —
Repair and Krangle level-growth still apply); every JSON persist path
(both `game/src/state.rs` and the bot's own mirrored `src/state.rs`) now
writes via temp-file + fsync + rename instead of a direct `fs::write`,
closing the crash-mid-write truncation risk. Also carries §13's own new
step 4a, the numbered binary-swap recipe — this release is its first
live use.

**Fixtures:** `golden_corpus_matches_committed_fixtures` passed with no
divergence, as predicted — this release is crafting-side only, nothing
touches `combat.rs`'s simulation logic. No regeneration needed.

**Verification:** full workspace suite, `cargo test --release
--workspace --quiet --target-dir target-deploy-divinity` — 707 passed, 0
failed. Clippy clean: default lints, zero diagnostics on any of the 10
files this merge actually touched. (`-D warnings` over the whole
workspace was tried and rejected as a signal — it turns ~250 pre-
existing pedantic warnings in untouched `pacing.rs` into hard failures
unrelated to this merge.) Real-config smoke test: a fresh `game.exe`
started clean against copies of production's three live config files
(`adventure-item-balance.toml`, `adventure-live-tunables.toml`,
`adventure-passive-overrides.toml`) via `GAME_DATA_DIR`, served
`/passives` and `/` at HTTP 200, and a crafted-session `/join` call
produced a valid, non-truncated `adventure-characters.json` with zero
`.tmp` residue anywhere in the scratch directory — confirms the Stage 0
atomic-save path against a real running binary, not just the unit tests
already covering it.

**Bot redeploy determination: NOT diff-clean.** `git diff --name-only
487aaf4..ce32085` touches `src/state.rs` (the bot's own atomic-save
mirror) — squarely inside the bot's dependency set (`src/**`) per §13's
post-severance definition (confirmed directly: root `Cargo.toml` carries
no `game = { path = "game" }` dependency any more). Bot deployed
alongside the game, each under its own maintenance-flag window; the bot
was started only after `GameProcess` was confirmed healthy.

**4a swap recipe — first live use, worked as written, both targets
(Game then Bot).** One note for whoever owns the doc next: step 6's
literal check ("`LastTaskResult` is `0`") never actually reads `0` for a
healthy long-running task in this environment — the observed steady
state for both scheduled tasks, both before touching anything and again
right after a clean restart, was `267009` (`SCHED_S_TASK_RUNNING`),
because a game/bot process that stays alive by definition never lets the
task instance complete. `0` only appears once a run has already finished
(the `487aaf4` incident's `LastTaskResult=1` fits this shape — that
process died immediately, so its run WAS a complete, failed instance).
The recipe's real health signal — port-listening plus a real page/curl
check — already covers this correctly; only the "is 0" wording is
misleading and should probably read "not 1 (or any other non-running
failure code)" instead. Not blocking, and it didn't affect this
deploy's outcome — flagged for the next edit of the recipe.

## Deploy record — 2026-08-24, per-node conversion caps (`81ec2ae`)

Entered by the deploy session at merge of
`feature/per-node-conversion-cap` (`7e68cc7`) into master, base
`2759648`.

**What shipped:** each `OverflowConversion` node's own
per-rank contribution cap is now individually overridable — a new
additive `[conversion_caps]` table in `adventure-passive-overrides.toml`
(`HashMap<String, f64>`; legacy files without it parse unchanged), and a
"Cap / rank" input rendered on `/admin/passives` beside the magnitude,
on every conversion row. NOTE the count: the docs have long said 13
conversion nodes, but the tree holds **14** today (Warrior 1, Rogue 3,
Monk 4 — stonefist/graniteskin/earthenwill/risingdefiance, Paladin 2,
Ranger 1, Druid 3); the implementation matches the effect generically,
so all 14 are covered. Blank follows the global
`LiveTunables::overflow_conversion_cap_per_rank`, which remains the
fallback for every node; `has_override`/`revert` cover both axes so a
cap-only override still earns the tuned marker and Revert clears both.
HOT via the same swap-on-save store the magnitudes use. Admin-tooling +
override-plumbing only — no default value moves.

**Fixtures:** expected NO divergence (behaviour-neutral at defaults:
the no-entry fallback IS the Stage-1 math byte-for-byte, pinned by
`a_per_node_cap_override_tunes_one_channel_without_touching_its_siblings`);
`golden_corpus` passed clean, nothing regenerated.

**Verification:** full workspace suite on the merged state, isolated
target dir — 711 passed, 0 failed. Clippy clean on touched lines.
Real-config smoke: fresh `game.exe` against copies of production's three
live config files via `GAME_DATA_DIR`, isolated scratch cwd + seeded
admin session — all 12 class pages of `/admin/passives` render; the
Cap / rank inputs total 14 (per-class counts above); a page-shaped save
round-tripped into the scratch overrides file (303 + `[conversion_caps]`
entry), a blank field cleared it again (empty table left behind, same
serialization convention as `[nodes]`), and `adventure-item-balance.toml`
/ `adventure-live-tunables.toml` SHA-256s were byte-identical before and
after the run — no tunable value moved.

**Bot redeploy determination: diff-clean** — `git diff --name-only
2759648..7e68cc7` touches only `game/**` and `WIKI_IMPACT.md`; nothing
under `src/**` or root `Cargo.toml`/`Cargo.lock`. Bot: unchanged, not
redeployed.

## Deploy record — 2026-08-25, passive-tunables stage 2 drift batch (`fb921b3`)

Entered by the deploy session at merge of `feature/passive-tunables-stage2`
(`9127fe0`) into master, base `879f586`.

**What shipped:** tunable_audit.md §3 Groups B+C — 17 nodes moved off raw-
rank reads onto their own declared values (16 out of PENDING_MIGRATION_NODES,
47 ? 31; Slayer `unrelenting`'s rank-3 bonus folded into a SpecialPerRank
table, out of PARTIALLY_TUNABLE_NODES 7 ? 3), plus seven count nodes
(golemmaster/risingphoenix/virulence/cursedblood/livingbond/naturesembrace/
verdantburst charges) now read via `passive_node_count` and added to
INTEGER_COUNT_NODES 21 ? 28. golemmaster's three call sites (combat spawn /
manager slot-unlock check / web picker) read one count; the two bespoke
lookup fns (`healing_flames_regen_pct`, `blazing_attack_speed_pct`) deleted.
Behaviour-neutral at defaults: every default reproduces the old rank-fed
value exactly (pinned by tests); overrides on these nodes now actually reach
the game.

**Doc restoration:** master's tree had dropped Stage 1's doc-record lines
from `LIVE_TUNABLES_PROGRESS.md` and `docs/passive_tunables_spec.md`
relative to `35cccba` (the branch tip `487aaf4` merged); the substance was
backfilled via `c9f4cbe` instead. This merge restores those doc sections on
current master (both files applied cleanly in `9127fe0`).

**Fixtures:** expected NO divergence (behaviour-neutral at defaults);
`golden_corpus` passed clean inside the suite, nothing regenerated.

**Verification:** full workspace suite on the merged state (`fb921b3`),
isolated target dir `target-deploy-stage2` — `cargo test --release
--workspace --quiet --target-dir target-deploy-stage2`: 712 passed, 0
failed across 24 suites, exit code 0 captured separately. Clippy exit 0,
zero diagnostics on any touched file. Real-config smoke: fresh `game.exe`
from the deploy build against copies of production's three live config
files (`adventure-item-balance.toml`, `adventure-live-tunables.toml`,
`adventure-passive-overrides.toml`) via `GAME_DATA_DIR`, isolated scratch
dir + ports 4199/4198 + seeded admin session — `/passives` 200;
`/admin/passives` renders (warrior default page; elementalist page shows
the golemmaster/healingflames rows); a `bulwark` save POST round-tripped
303 into the scratch overrides copy with tuned badge + Revert offered;
production TOMLs never touched (no tunable value changed).

**Bot redeploy determination: diff-clean** — `git diff --name-only
879f586..9127fe0` touches only `game/**` plus docs/`WIKI_IMPACT.md`;
nothing under `src/**` or root `Cargo.toml`/`Cargo.lock`. Bot: unchanged,
not redeployed (diff-clean).

**4a swap:** maintenance flag SET (`scope` line verified against the
GameProcess-Watchdog task root) BEFORE the stop; `Stop-ScheduledTask`; port
4005 confirmed free; old `game.exe` SHA-256
`8EE025EAB94BCCE3D4F56A9BBEACC0157BECD66544BB30EB09CFB046BB3C9BB6` renamed
aside AND copied into `backup-pre-passive-tunables-stage2/` with the pinned
200-file `adventure-fights-summary` snapshot; new binary SHA-256
`5361D4AD714742D3156C002996492026FB1EF404EAED19FF4DDD0EB5895C360F` copied
in; task restarted, `LastTaskResult` `267009` (running — see the Divinity
record's step-6 note), `/passives` and `/` both HTTP 200; flag CLEARED
after the health check. Downtime ~ stop-to-start window only.

## Deploy record — 2026-08-27, passive-tunables stage 3 (`7ddfd8d`) — BACKFILLED

Backfilled 2026-08-28 by the deploy session: every release from
2026-08-23 onward carries a record here, and this one was missing. Merge
of `feature/passive-tunables-stage3` (`86fb5b0`/`ad3f026`/`e503f2e`) into
master as `7ddfd8d`, base `234a487`; gitignore commit `23e2b32`; deploy
docs `6fbf7c6` and `2fbdde2`. Pushed.

**What shipped:** the rank-fed backlog closes — 25 nodes moved off
`passive_node_rank`-only consumption onto declared per-rank tables read
through `magnitude_at_rank` → `passive_override_for`, so overrides on
them now actually reach the game. `PENDING_MIGRATION_NODES` 31 → 6;
`INTEGER_COUNT_NODES` 28 → 40. Eight old declarations disagreed with the
game and were corrected to the GAME's values, read off each call site one
at a time (payback, secondwind, crush, vitalstrike, gloriousdeath,
undying, doubletap, lastrites) — displayed numbers changed, fight
behaviour did not. `lastrites` is the one node whose declared MEANING
changed: its advertised 33/66/100% chance was never read, and its
magnitude now carries the deterministic charge count it actually uses.
Left behind with reasons on the list itself: `clarity`, `lastlaugh`,
`neverending`, `sanctifiedtouch` (structure-only, own no rank-fed
number); `reckless`, `deathwish` (need a second per-rank value slot —
the schema change described in the Stage 3 record in
`docs/passive_tunables_spec.md`). Full mechanics in that record.

**Verification:** `cargo test --release --workspace --quiet --target-dir
target-deploy-stage3` → 716 passed, 0 failed (exit 0). Clippy exit 0, no
new warnings on touched lines (blame confirms the doc-indentation hits
pre-date the branch).

**Golden corpus:** REGENERATED at merge per house rule. 14 of 17 fixtures
rewrote, but the only changed keys across all of them were `hitId` and
`eventId` (20,008 + 17,118 occurrences), zero combat values — the
process-global counters `approx_eq` skips by design. Committed fixtures
restored; no semantic diff.

**Bot redeploy determination: diff-clean.** Nothing under `src/**` or
root `Cargo.toml`/`Cargo.lock`. Bot: unchanged, not redeployed.

**4a swap:** maintenance flag set before the stop and cleared after the
health check; old `game.exe` SHA-256
`5361D4AD714742D3156C002996492026FB1EF404EAED19FF4DDD0EB5895C360F` →
new `5F3B595A4EEBB8095289D2E45277F80528CA3DEE7A35A02859BA1C4C13D8741E`.
Rollback at `backup-pre-passive-tunables-stage3/` (old binary plus the
200-file pinned `adventure-fights-summary` snapshot) and
`target/release/game.exe.pre-passive-tunables-stage3`. Downtime a few
seconds.

**INCIDENT — three stale overrides went live at the swap. See `#49`.**
`chakraoflife`, `unyieldingspirit` and `shattering` had sat inert in
`adventure-passive-overrides.toml` (all three were on the OLD pending
list, so `/admin/passives` never offered them — generic seed values, not
owner tuning) and activated the moment the binary swapped: Monk
cheat-death immunity cut roughly 3x for 4 players, Monk Last Stand
effectively always-on at rank 3 for 8 players, Elementalist icicle
targets doubled for 2 players. Live approximately 20 minutes across ~20
boss fights. Caught post-deploy by the `current ≠ default` columns on
`/admin/passives`, not by the suite — every migration test pins DEFAULTS
and the live store is not at defaults. All three reverted to declared
defaults (bit-exact reproductions of the old call-site values) and
confirmed back at pre-swap values. Store audit ordered as follow-up: the
33 remaining keys intersected against the 25 migrated, intersection
empty, so nothing else was activated by this deploy.

**Step 8 of the deploy order** (confirm a stored override actually
reaches combat) — VERIFIED, but proven by the incident above rather than
by the ordered deliberate-value test, which the owner ruled on
2026-08-27 must NOT be re-run. Three overrides changing observable
combat for 14 players the moment the binary began reading them is
conclusive.

**FOUND at this deploy, fixed in the next one:** `/admin/passives` rows
rendered no unit word and save validation was "known key + finite" with
no range clamp — became `#46`, shipped in the 2026-08-27 identity+units
release below.

## Deploy record — 2026-08-27, local identity + passive-override units (`0f7f754`)

Merges `3ef0651` (`feature/local-identity`) and `7af21b2`
(`fix/passive-override-units`). Numbers below are assigned by the deploy
session at the owner's explicit instruction; the log-parser session owns
the sequence and may renumber.

**Commit citation, reconciled (2026-08-28).** This record cites
`0f7f754`; the deploy report gave the pushed master head as `8d09913`.
Both are correct about different things, and `0f7f754` is the one that
corresponds to the shipped binary:

| commit | what it is |
| --- | --- |
| `c855aa8` | HEAD when `cargo build --release --workspace` produced the binary that shipped |
| `d65d2f6`, `0f7f754` | `WIKI_IMPACT.md` and `.gitignore` only — no compiled input; `0f7f754` was HEAD at the moment of the swap and is the SHA recorded in the maintenance-flag reason string |
| `8d09913` | `docs/anomaly_ledger.md` only, committed AFTER the swap; the pushed master head |

`git diff --name-only c855aa8..0f7f754` returns `.gitignore` and
`WIKI_IMPACT.md`; `git diff --name-only 0f7f754..8d09913` returns
`docs/anomaly_ledger.md`. No source file changed after the build, so the
binary's source tree is identical across all four commits.
**`0f7f754` is the deployed commit — the heading stays as it is.**
`8d09913` is post-deploy documentation and ships no behaviour.


**#46 — `/admin/passives` save validation: "accept any finite number" is
SUPERSEDED by typed per-unit validation**
The earlier ruling — that the save path should accept any finite number,
since the admin is trusted and the consuming code clamps what it needs to
— is superseded as of this release. Saves are now range-checked against a
per-node UNIT derived from the code that consumes the value: fraction,
count, seconds, milliseconds, multiplier. All 463 editable nodes are
classified, zero left unconfirmed. A bounded fraction rejects out-of-range
input naming the field and the expected range (verified live: `payback`
rank 2 = 45 → `⛔ Not saved — Rank 2 on payback is above what the code
that reads it accepts — got 45, expected a fraction from 0 to 1 — 1 means
100%. If you meant 45%, enter 0.45.`, nothing written). The six
legitimately-above-1 fraction keys (`cutthroat`, `finalcut`, `volley`,
`growing`, `echo`, `chainshot`) warn and require an explicit confirm
rather than being refused. **The superseded doc block was REPLACED, not
left alongside the new one — deliberately, so the file carries exactly
one ruling on this question and no future session has to guess which of
two co-resident blocks is current.**

**#47 — Unparseable conversion cap redirected as if saved (same
silent-accept class as #46); now rejected**
A conversion-cap value that failed to parse used to redirect with
`saved=1` while the value went nowhere but a `tracing` warning — the
operator saw a success page and no change. Identical failure class to the
validation gap above: the save path reporting success for input it did
not store. Now refused with an error. The fix landed outside the ordered
scope of `fix/passive-override-units`; **the owner ruled it stays**.

**#48 — Backup allow-list gap: `backup-game-data.ps1` enumerates, it does
not glob**
The backup manifest is an explicit allow-list of filenames derived from
the code, not a `adventure-*.json` glob. A new persisted state file is
therefore INVISIBLE to backups — no error, no drift warning at the moment
it is created — until a human adds it by hand.
`adventure-accounts.json`, the local-account password-hash store, was
missed on `feature/local-identity`'s first delivery and caught in review;
it ships in the allow-list (with a shape-validation arm, since a lost
password hash has no external identity provider to re-authenticate
against). **Standing rule, adopted here: any new persisted state file
must be added to the backup allow-list in the SAME branch that creates
it.** Verified this deploy: the post-swap run lists
`adventure-accounts.json` as `absent (skipped)` — legitimately absent,
because no account has been registered yet.

**Bot redeploy determination: DEPLOYED.** `git diff --name-only
2fbdde2..0f7f754` touches `Cargo.lock` (argon2 dependencies for the game
crate), which is inside the bot's declared dependency set per §13. No
`src/**` or root `Cargo.toml` change, but the §13 rule is objective and
the diff is authoritative — so the bot deployed rather than being skipped
on a judgment that the added crates are game-only.

**4a swap:** game maintenance flag SET with `scope : this IS the flag
'GameProcess-Watchdog' reads` verified BEFORE the stop; `Stop-ScheduledTask
GameProcess`; port 4005 confirmed free; old `game.exe` SHA-256
`5F3B595A4EEBB8095289D2E45277F80528CA3DEE7A35A02859BA1C4C13D8741E` renamed
aside to `game.exe.pre-identity-units` AND copied into
`backup-pre-identity-units/` with the pinned 200-file
`adventure-fights-summary` snapshot; new `game.exe` SHA-256
`71F524832CD20FE3EC5F46CE0C381C343C1941279A3C2985C5FF3FEEE075DE28`; task
restarted, `LastTaskResult` `267009` (running), `/passives` 200; flag
CLEARED after the health check. Bot then swapped under its own separate
flag (`scope : this IS the flag 'TwitchBotRS-Watchdog' reads`), port 4001,
old `105D7740183E24B98D2E0AB2B3F34BA1A084C13E85B969081122145CA2DE72E2` →
new `3DED1C682B6470D5DD681380D66EBE4C9BC87ED8032F537A45813A177041A435`,
started only after the game was confirmed healthy; bot flag CLEARED.

**Identity — nobody logged out.** A session token minted before the swap
(`created_at` 1787725400) returned the authenticated `/passives` page at
129,541 bytes both before and after the swap, with no login prompt in the
body. `/login` still 303s to `id.twitch.tv/oauth2/authorize` with the
production client id and `adventure.lokati.net/auth/callback`.
`/account/register` and `/account/login` render 200. Collision guard
verified in production WITHOUT creating an account: `username=xcercs`
(an existing character key) with a valid 12-character password → 400,
"That username is already taken."; a 7-character password on an unused
name → 400, "Passwords must be at least 8 characters long."
`adventure-accounts.json` does not exist on disk after either attempt.
No account was registered — the owner creates the first one himself.

**No live override value changed.** `adventure-passive-overrides.toml`
holds the same 34 keys with the same values as its pre-deploy snapshot
(SHA-256 of the pre-deploy copy
`68CBB642D29814CF488CC539A3F5DB23883CEE6D0930656AF42841ACC29FAD52`;
compared value-for-value, since the writer re-serializes from a HashMap
and key ORDER is not stable between writes). The check-16 warn/confirm
exercise moved `volley` rank 3 1.5 → 1.6 and back to 1.5 through the same
warn-and-confirm path; it was reverted by RESTORING THE PRIOR VALUE rather
than by the page's Revert button, because Revert drops the override
entirely and would have discarded the owner's tuning of that node.

**Golden corpus:** regenerated pre-merge on master (delete + rerun). All
14 changed fixtures differed on exactly 37,126 lines, of which 20,008 were
`hitId` and 17,118 `eventId` and none were anything else — zero combat
values moved. `next_hit_id()` is a process-global atomic counter, so those
ids depend on what else ran in the test process and churn on every run;
`approx_eq` ignores both keys by design. The churn was therefore reverted
rather than committed, and the committed fixtures stand unchanged.

## Deploy record — 2026-08-28, World 2 Stage 2 (`ac5573a` + `f5c38f8`)

Two additive branches merged in the ordered sequence and shipped as one
game-only release: `feature/announcement-feed` (`e94774e`) then
`feature/operator-levers` (`6c7d3f2`).

| item | value |
| --- | --- |
| merge 1 (announcement feed) | `ac5573a` |
| merge 2 (operator levers) | `f5c38f8` |
| source state the binary was built from | `988fbb7` |
| old `game.exe` SHA-256 | `71F524832CD20FE3EC5F46CE0C381C343C1941279A3C2985C5FF3FEEE075DE28` |
| new `game.exe` SHA-256 | `B8D9B45969F58DA13C9D2CA7948A3CCAD29970F9C849889C189F5F72D5D9A93C` |
| rollback | `backup-pre-world2-stage2/game.exe` + `target/release/game.exe.pre-world2-stage2` |
| bot | unchanged, not redeployed (diff-clean) |

**Backup.** `backup-game-data.ps1` run before anything else:
`pod-backup-20260828-013322`, 252 files, 11.31 MB, `verdict=clean`, 33
snapshots verified. The 200-file `adventure-fights-summary` corpus was
pinned into `backup-pre-world2-stage2/` inside the stop window.

**Bot determination.** `git diff --name-only 0f7f754..HEAD` touches
`WIKI_IMPACT.md`, `docs/**`, `game/src/**`, `game/tests/**` and
`templates/base.html`. Nothing under the bot's dependency set (root
`src/**`, root `Cargo.toml`, `Cargo.lock`), so the bot ran untouched
through the whole window — no flag, no stop, no binary copy.

**Conflict.** One, in `manager.rs`, and mechanical: both branches
appended a new `#[cfg(test)]` module at end of file. Kept both in merge
order (`announcement_feed_ring_tests`, then `operator_boss_select_tests`).
No behaviour was chosen. `adventure_web.rs` auto-merged.

**Verification.** `cargo build --release --workspace --target-dir
target-stage2` exit 0; `cargo test --release --workspace --quiet
--target-dir target-stage2` → **741 passed, 0 failed** across 24 suites,
exit code captured separately. Clippy exit 0; three
`doc_lazy_continuation` warnings landed on new lines of
`render_announcement_feed`'s doc comment and were fixed in `988fbb7`
(prose only), after which no clippy diagnostic falls inside any line this
release added.

**Golden corpus.** Passed clean on master before the merge (4 tests, no
fixture drift). Regenerated anyway per the order (delete + rerun): 7
fixtures changed, 6,464 lines, of which **3,450 were `hitId` and 3,014
`eventId` and none were anything else — zero combat values moved.**
`approx_eq` ignores both keys by design
(`golden_corpus.rs:508`), so the churn is invisible to the test; it was
reverted rather than committed, and the committed fixtures stand
unchanged. Same handling as the stage-3 release above.

**Swap.** Maintenance flag set 01:58:15 with `scope : this IS the flag
'GameProcess-Watchdog' reads` confirmed, `Stop-ScheduledTask GameProcess`,
port 4005 free on the first 500 ms poll, old binary renamed aside to
`game.exe.pre-world2-stage2`, new binary copied in, task restarted,
`LastTaskResult=267009` (`SCHED_S_TASK_RUNNING`, the healthy steady
state), `/passives` and `/` both 200. **Flag cleared after the health
check passed**, `-Status` confirming `watchdog : NOT suppressed`.

**Post-deploy checks.** The load-bearing one first:

| # | check | result |
| --- | --- | --- |
| 7 | Twitch chat still receives announcements | **PASS** — `Last 6 fights: 4W-2L · stage 5035 → 5035` at 18:04:03Z, and a second at 18:09:18Z. The tee did not become a reroute. |
| 8 | Feed card renders server-side | **PASS** — `ul#announcement-feed` present; the empty-state placeholder immediately post-restart, then the real line once the ring warmed. |
| 9 | New announcement appears live, no reload | **PASS** — pushed over `/ws` at 02:09:18 local; the *same line* reached chat at 18:09:18.142Z. One `announce`, both destinations, same second. |
| 10 | Fresh connection receives the backlog | **PASS** — a socket opened at 02:06:45 got `{"type":"announcements","lines":[..]}` with the ring's content, not an empty list. |
| 11 | Overlay / desktop client unaffected | **PASS (overlay)** — `/overlay` serves 200; its `handleOverlayMessage` is an `if/else if` on `state`/`encounter` with no `else`, so both new types fall through ignored, and both of its own envelope types were observed still flowing on the live socket. **Desktop client: not reachable** — nothing listening besides game (4004/4005) and bot (4001-4003). |
| 12 | Operator card on `/admin/tunables` | **PASS** — form posts to `/admin/ops/next-encounter`, 8 boss choices rendered from `FORCED_CHOICES`. |
| 13 | Non-admin POST refused visibly | **PASS** — `403 Forbidden`, "Refused - not the operator", "Nothing was triggered." Not a bare redirect. See `#51`. |
| 14 | Admin fires next-encounter once, no boss | **PASS** — `200`, "Encounter triggered / Ran the next encounter right now." |
| 15 | Second press during a fight | **PASS** — `409 Conflict`, "Refused - a fight is in progress ... Nothing was queued." No second fight ran. |
| 16 | Boss select in production | **NOT TESTED, by order.** Covered by `operator_boss_select_tests`. |

**Exactly one operator-triggered encounter reached the game.** The first
attempt at the 14/15 pair failed in the client (a `Start-Process`
argument-quoting fault meant that press never reached the server,
`http_code=000`), so the successful trigger was its sibling press at
02:03:45. The pair was then re-run as two genuinely concurrent requests;
both landed while the SCHEDULED loop already held the fight gate, so both
returned the `FightInProgress` 409 and neither fired anything. Check 15 is
therefore satisfied against a real in-flight fight rather than against one
this session caused, which is the stronger reading of it.

**Patch notes.** One "August 28, 2026" block at the top of the array,
section "New: The Adventure Feed On Your Dashboard" — three items, plainly
stating that announcements now appear on the dashboard, that the card
updates live and arrives populated, and that chat is unchanged. Nothing
about the operator control: it is admin-only and not player-facing.

**`CLAUDE.md` unchanged this release**, so the `.clinerules` mirror needed
no refresh (§13 step 6 is conditional on that file moving).
