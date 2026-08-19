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
- [ ] **Stage 2** — bucket A, the 36 nodes where magnitude equals rank
      by construction.
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
