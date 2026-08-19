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
- [x] **Stage 2** — 20 count nodes migrated onto the tunable path;
      5 reclassified after reading their real call sites. 442 → 449
      tests. Commit `dc1b8ec`.
- [ ] **Stage 3** — buckets B, C, D (24 nodes), batched per class, plus
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

**449 passing, 0 failed.** Golden corpus green — that is the
behavior-neutrality proof for this stage. Clippy and release build
clean. No character data touched.

20 nodes migrated from `passive_node_rank` to the new
`Character::passive_node_count`, which reads the magnitude (and so the
override hook) and converts to `u32` in one documented place.

### The batch was 20, not the 36 the Stage 1 audit projected

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

### Open decision: `chainoflight`

`(1 + c.passive_node_rank("chainoflight") + …).min(5)`. A
Specialization can hold rank 4, so today a 4/4 investment gives **5**
Prayer of Mending targets. Migrating to magnitude would give 4, because
`effective_rank` floors a Spec at 3.

Three facts point the same way: the node's own description says "up to 4
at rank 3"; `passive_tree.rs` documents the 4th point as unlock-only,
adding no further increment; and every other Spec obeys that. So today's
5 looks like a latent bug rather than intent.

But correcting it is a **player-facing nerf**, not a neutral migration,
so it was deliberately left alone. Needs an explicit call:

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

- `PENDING_MIGRATION_NODES`: **38** (was 60)
- `INTEGER_COUNT_NODES`: **20**, each confirmed a plain arithmetic count
  at its own call site
- `UNWIRED_NODES`: **2**
- Tunable nodes overall: **371** of 471 (was 351)
