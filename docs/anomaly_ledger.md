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

## Open

**#31 — +6 unique affixes granted vs 5 shards consumed**
Downgraded to WATCH 2026-08-20: accept the launch-giveaway explanation
unless it recurs after the giveaway window. Not re-checked today — out
of this order's scope. *(Carried forward, not independently re-verified.)*

**#32 — Unique-shard picker route entirely unlogged**
Was deferred pending the golem-inheritance release, which was going to
add picker-open/picker-apply logging. **That release has since shipped**
— commit `82b1785` (`Merge feature/golem-inheritance-mechanism`) is
confirmed an ancestor of current master `380371a`. Per the deferral's
own trigger condition ("verify them together in one pass once it
deploys"), this is now due for re-verification. Not checked today — out
of this order's explicit scope (order listed it as "context only, not
new work this order"). **Recommend as the next parser task.**

**#24 — RF self-damage emits no observable damage event**
Same status as #32: deferred pending the golem-inheritance release,
which has now shipped (see #32). Due for re-verification, not done
today, out of this order's scope.

**#29 — Shattering × inherited splash mitigability**
Same status as #32/#24: release-gated, release has shipped, due for
re-verification, not done today, out of this order's scope.

**#35 — Tank-credit observability**
Owned by the tree session, not parser. `#36` below is blocked on it.
*(Carried forward, no detail available to this session beyond the
name.)*

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

`#24`, `#29`, `#32`, and the golem-durability item now closed as `#40`
were the golem-inheritance release's full deferred-items set. With `#40`
closed and the release confirmed shipped, `#24`/`#29`/`#32` are the only
remaining open items gated on that release — bundle them into one
verification pass next, per the original deferral's own instruction.
