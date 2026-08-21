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

## Open

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
