# Live-tunable passive values — Implementation Progress

**Status** (2026-08-19): **Stage 1 complete, built and verified** on
branch `feature/live-tunables`. Not merged, not deployed — that happens
through the release queue on the owner's go-ahead.

This file is the live execution log; `docs/passive_tunables_spec.md` is
the design source of truth. Read the spec first, then this.

Branch: `feature/live-tunables` off `master` at `45ca8a4` (the commit
containing the Memories merge), in worktree `..\PathofDust-tunables`.
The main checkout was never touched and its branch never switched.

**Test baseline before any work on this branch: 374 passing, 0 failed.**

---

## Stages

- [x] **Stage 1** — override store, the `magnitude_at_rank` hook,
      `/admin/passives`, and the tuned-value display line.
      374 → 403 tests. Commits `453155e`, `8ad16e1`.
- [x] **Stage 2** — 21 count nodes migrated onto the tunable path;
      4 reclassified after reading their real call sites. 442 → 450
      tests. Commits `dc1b8ec`, +chainoflight.
- [ ] **Stage 3** — buckets B, C, D (23 nodes), batched per class, plus
      an Elementalist golden-corpus scenario as a fixture ADDITION.

**Ask the owner before starting each migration batch** — they may
reorder for balancing appetite. Approved risk order: Druid, Paladin,
Warlock → Monk, Ranger, Mage → Rogue, Slayer, Warrior, Cleric →
Berserker last.

---

## Stage 1 summary (for a fresh session)

**403 passing, 0 failed.** Release build clean; clippy clean on touched
code. Golden-corpus fixtures untouched and unregenerated. No character
data touched, so the save-compat fixture is unaffected by construction.

| File | Change |
|---|---|
| `game/src/adventure/passive_overrides.rs` | **New.** `PassiveOverrides`, the `LazyLock<RwLock<_>>` global, TOML load/save, `passive_override_for` (the hook target), `PENDING_MIGRATION_NODES`, `INTEGER_COUNT_NODES`. |
| `game/src/passive_tree.rs` | `magnitude_at_rank` now consults the store; new `magnitude_at_rank_with` (pure, for tests) and `effective_rank` (one definition of the rank an override is keyed by). |
| `game/src/adventure_web.rs` | 3 routes, `render_admin_passives_page`, save/revert handlers, `passive_override_note`, `trim_float`. |
| `templates/base.html` | `.passive-*` CSS for the admin page and the tuned line. |
| `tests/admin_passives_http.rs` | **New.** Gate + round-trip + reaches-the-game, over real HTTP. |

### What makes this safe to ship dark

With no override file on disk, **every node at every rank reads exactly
what it read before** — asserted across all 471 nodes rather than
argued (`with_no_overrides_file_every_node_in_every_tree_is_byte_identical_to_before`).
At the UI layer, no badge and no revert button appears anywhere, and no
tooltip gains a tuned line.

### Testing note worth preserving

The override store is a **process-global** `RwLock`, and this crate runs
its whole suite in one process. So no in-crate test writes it — they all
go through `magnitude_at_rank_with` and hand-built `PassiveOverrides`.
The single test that touches the global only *reads* it, to assert it is
empty. The integration test in `tests/` may write it freely: Rust gives
each `tests/*.rs` file its own process.

---

## Decisions

All numbered design decisions live in `docs/passive_tunables_spec.md`'s
own Decisions log rather than being split across two files.

Two are worth calling out here because they were nearly got wrong:

- **`INTEGER_COUNT_NODES` ships empty** (Decision 9). Seeding it from
  the 36 nodes declaring `1.0 / 1.0` looks obviously right and is wrong
  for 12 of them — `lastlaugh`, `compassion`, `quickdraw` and 9 others
  declare `1.0 / 1.0` but are implemented as boolean thresholds
  (`rank >= 2` flips a flag), not counts of anything. Caught by checking
  the hand-written list against the audit data before committing it.
- **Two test assertions were passing for the wrong reason.** Once
  `base.html` carries a `.passive-tuned` CSS rule, every rendered page
  contains the substring `passive-tuned` whether or not the note was
  ever emitted. Both assertions now match `class="passive-tuned"`, i.e.
  the markup rather than the stylesheet. Any future check on
  page-level HTML has the same trap.

---

## Still open

- The wiki side of the tuned-value line. `passive_override_note` is
  `pub(crate)` and free-standing specifically so
  `adventure_web/wiki.rs` can call it, but that file belongs to the
  parallel wiki session and was not edited here. Requested via
  `WIKI_IMPACT.md`; until adopted, `/wiki/passives` will not show that a
  node has been retuned.
- No human has looked at `/admin/passives` yet. It is verified
  structurally (render tests) and end to end (HTTP test), not by eye.

---

## Stage 2 summary (2026-08-20)

**450 passing, 0 failed** (excluding one pre-existing flaky test — see
below). Clippy and release build clean. No character data touched.

### CORRECTION: the golden corpus does NOT protect passive migrations

An earlier draft of this file, and the Stage 2 commit message, called
the golden corpus this stage's behavior-neutrality proof. **That was
wrong**, and the error is worth recording because the whole Stage 3 plan
was built on it.

`golden_corpus.rs::run_scenario` builds each scenario character with an
archetype, a level and deterministic gear, and **never populates
`passive_allocations`**. Every node sits at rank 0 in every fixture, so
no passive mechanic fires in any of them. A corpus run therefore proves
only that non-passive combat is unchanged; it could not detect a passive
regression if one existed.

**What actually proves Stage 2 neutral** is the algebraic property,
asserted directly in
`at_default_values_every_migrated_count_still_equals_its_rank`: every
migrated node declares `1.0 / 1.0`, so `1.0 + 1.0 * (rank - 1) == rank`
exactly, and swapping a rank read for a magnitude read cannot move a
number. That test walks all 21 nodes at every rank.

**Consequence for Stage 3** (buckets B/C/D, which change real values):
the corpus will not protect those either. Closing this needs corpus
scenarios that actually allocate passives — a fixture **ADDITION**,
never a regeneration, and therefore permitted. That should land before
the first value-changing batch, not after.

### Pre-existing flaky test (not introduced here)

`elementalist_stage_6_thunder_golem_isolation_tests::every_golem_death_gets_handle_golem_death_even_on_the_fights_final_tick`
fails intermittently. Measured on **unmodified master**: 4 failures in
14 runs (~29%). On this branch: comparable. **Not caused by Stage 2** —
all three characters in that test have empty `passive_allocations`, so
every migrated node reads 0 through either accessor.

Cause: the test uses a 3-character party in a `HashMap`, and
`simulate_battle` iterates `characters.iter()`. Rust randomizes
`HashMap` order per process, so targeting and first-mover tie-breaks
differ run to run even with a seeded RNG — precisely the hazard
`golden_corpus.rs`'s own doc documents as its reason for keeping every
scenario solo. Reported to the owner; belongs to the Elementalist
session, not touched here.

21 nodes migrated from `passive_node_rank` to the new
`Character::passive_node_count`, which reads the magnitude (and so the
override hook) and converts to `u32` in one documented place.

### The batch was 21, not the 36 the Stage 1 audit projected

Stage 1 classified nodes by their *declaration* (`1.0 / 1.0` ⇒
magnitude equals rank ⇒ mechanical swap). Dumping every candidate's
actual **call site** before editing found five the declaration-shaped
view got wrong. Worth reading before Stage 3, because the same trap
applies there:

| Node | What the declaration implied | What the code actually does |
|---|---|---|
| `chainoflight` | trivial count swap | Spec read as `(1 + rank).min(5)`; at rank 4 yields **5**, magnitude would yield 4 — **held, see below** |
| `bloodsac` | risky (Spec, rank 4) | safe — `.max(2000.0)` floor makes rank 3 and 4 identical |
| `onehundredhands` | risky (Spec, rank 4) | safe — already `.min(3)` by hand |
| `risingblaze` | pending migration | **no consumer anywhere** — nothing to migrate |
| `stillwater` | pending migration | **no consumer anywhere** — nothing to migrate |
| `undyingwill` | trivial count swap | feeds a non-linear `match` table — stays pending for Stage 3 |

**The general lesson: a node's declared shape does not predict its call
site.** Read the call site first. This is the second time the same
assumption has produced a wrong list (Stage 1's `INTEGER_COUNT_NODES`
was the first).

### RESOLVED: `chainoflight` — migrated, nerf accepted

`(1 + c.passive_node_rank("chainoflight") + …).min(5)`. A
Specialization can hold rank 4, so today a 4/4 investment gives **5**
Prayer of Mending targets. Migrating to magnitude would give 4, because
`effective_rank` floors a Spec at 3.

Three facts point the same way: the node's own description says "up to 4
at rank 3"; `passive_tree.rs` documents the 4th point as unlock-only,
adding no further increment; and every other Spec obeys that. So today's
5 looks like a latent bug rather than intent.

Correcting it is a **player-facing nerf**, not a neutral migration, so
it was raised rather than assumed. **Owner decision (2026-08-20):
migrate and accept.** A 4/4 Chain of Light now gives 4 bounce targets
instead of 5, the node becomes tunable, and its description becomes
accurate rather than aspirational. Pinned by
`chain_of_light_at_four_four_now_matches_its_own_description`, which
exists because the corpus cannot catch it. The options considered were:

- **Migrate and accept the change** (4/4 drops 5 → 4 targets) — makes
  the node tunable and brings it in line with the documented rule.
- **Preserve today's behavior** — keep reading the raw rank, and leave
  the node permanently non-tunable.
- **Preserve and make tunable** — read the count but re-add the +1 at
  rank 4 explicitly, which encodes the anomaly in code forever.

### New classification: `UNWIRED_NODES`

`risingblaze` and `stillwater` declare real per-rank values that
**nothing in the codebase reads** — no `passive_node_magnitude`, no
`passive_node_rank`, no call site at all. That is a third state,
distinct from "pending migration" (values do reach the game, via
hardcoded constants) and from `NotYetImplemented` (declares no value).
`/admin/passives` now says the accurate thing for each via
`node_untunable_reason` rather than promising a migration batch that
would have nothing to do.

### Counts after Stage 2

- `PENDING_MIGRATION_NODES`: **37** (was 60)
- `INTEGER_COUNT_NODES`: **21**, each confirmed a plain arithmetic count
  at its own call site
- `UNWIRED_NODES`: **2**
- Tunable nodes overall: **372** of 471 (was 351)

## Drift batch (2026-08-25): tunable_audit.md §3 Groups B+C land

Branch `feature/passive-tunables-stage1`, stacked on Stage 1's spec.
Behavior-neutral at defaults, golden corpus untouched. 17 nodes migrated
off raw-rank reads onto their own declared values (16 out of
PENDING_MIGRATION_NODES → 31; unrelenting came out of
PARTIALLY_TUNABLE_NODES by folding its rank-3 bonus into a
SpecialPerRank table). INTEGER_COUNT_NODES 21 → 28 with golemmaster,
risingphoenix, virulence, cursedblood, livingbond, naturesembrace and
verdantburst. Three mixed nodes stay honestly half-listed
(bloomingfield/reaperscall/sacrifice: second value, no second slot);
ravage/endlessthirst/naturesblessing left the partial list as fully-wired
(only structural unlock-gates still read rank), matching empoweredbolt's
shipped shape; mercifultouch confirmed wired, never listed. Full record:
docs/passive_tunables_spec.md "Drift-batch record".

## Stage 3 (2026-08-27): the rank-fed backlog closes

Branch `feature/passive-tunables-stage3`, off master 234a487.
Behavior-neutral at defaults, golden corpus untouched. 25 nodes migrated
off raw-rank reads onto their own declared per-rank tables:
PENDING_MIGRATION_NODES 31 → 6, INTEGER_COUNT_NODES 28 → 40.

Eight of those nodes' old declarations disagreed with what combat.rs
actually used (payback, secondwind, crush, vitalstrike, gloriousdeath,
undying, doubletap, lastrites) — the declared tables are now the game's
real values, pinned bit-exactly by test.

The six still listed are not waiting on a batch: four are structure-only
(clarity, lastlaugh, neverending, sanctifiedtouch — every rank read is an
unlock gate) and two need a second per-node value slot (reckless,
deathwish — dealt AND taken ladders), the same blocker as
sacrifice/bloomingfield/reaperscall. `node_untunable_reason` was reworded
to stop promising them a migration. Full record + the schema change those
five need: docs/passive_tunables_spec.md "Stage 3 record".
