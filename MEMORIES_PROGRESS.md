# Memories — Implementation Progress

**Status** (2026-08-19): All 4 stages complete on branch
`feature/memories`. Built, tested, pushed. **Not merged, not deployed** —
that happens through the release queue on the owner's go-ahead only.

This file is the live execution log; `docs/memories_spec.md` is the
design source of truth. Read the spec first, then this.

Branch: `feature/memories` off `master` at `362d717`, in worktree
`..\PathofDust-memories`. The main checkout was never touched and its
branch never switched — another session had work in flight there
(`combat.rs`, `WIKI_IMPACT.md`, `docs/elementalist_spec.md`).

**Test baseline before any Memories work: 296 passing, 0 failed**
(`cargo test --workspace --all-targets --target-dir target-memories`).
This is the number every stage's "all existing tests still pass" check
protects.

---

## Stages

- [x] **Stage A** — domain + persistence (`memory.rs`, `Character::{memories,
      memory_slots}`, name filter, snapshot replay) + the
      `validate_allocation_step` extraction. 296 → 334 tests.
      Commit `23943bd`.
- [x] **Stage B** — manager layer (`save`/`load`/`rename`/`delete_memory`,
      `fight_in_progress`). 334 → 347 tests. Commit `233b196`.
- [x] **Stage C** — `/passives` UI, 4 routes, CSS, and the stale
      points-per-level copy fix. 347 → 357 tests. Commit `eb7b420`.
- [x] **Stage C.1** — end-to-end HTTP route coverage. 357 → 358 tests.
      Commit `f6d9ed4`.
- [x] **Stage D** — docs (this file, `docs/memories_spec.md`,
      `WIKI_IMPACT.md`).

**Final: 358 passing, 0 failed.** Release build clean; clippy clean on
touched code (the warnings that remain are all pre-existing, in
untouched files). `tests/character_fixture_roundtrip.rs` — the test that
catches a missing `#[serde(default)]` — green throughout. Golden-corpus
fixtures untouched and unregenerated.

---

## What was built, by file

| File | Change |
|---|---|
| `game/src/adventure/memory.rs` | **New.** `Memory`, `MemoryError`, `NameRejection`, `DroppedAllocation`, `DropReason`, `MemoryLoadReport`, `AppliedBuild`, `validate_memory_name`, `default_memory_name`, `replay_snapshot`, `trim_to_budget`, `apply_memory`. All pure functions, no I/O. |
| `game/src/passive_tree.rs` | `validate_allocation_step` extracted (see below). |
| `game/src/adventure/character.rs` | `memories`, `memory_slots` fields; `memories_padded`, `memory_slot`, `memory_slot_mut`, `snapshot_build`. |
| `game/src/adventure/manager.rs` | `fight_in_progress`, `save_memory`, `load_memory`, `rename_memory`, `delete_memory`; `preview_allocate_passive` refactored onto the shared validator. |
| `game/src/adventure_web.rs` | 4 routes, 3 form structs, 4 handlers, `memory_error_text`, `memory_load_note`, `render_memory_note_popup`, `render_memories_section`; points-per-level copy fix. |
| `templates/base.html` | `.ptree-memories` CSS. |
| `tests/memories_http.rs` | **New.** Route-level coverage against a disposable instance. |

### The one structural change: `validate_allocation_step`

The node-local half of `preview_allocate_passive` (node existence,
`max_rank`, the parent/`unlock_at` gate) moved into
`passive_tree::validate_allocation_step`. `preview_allocate_passive` now
calls it, and so does the Memory replay.

This is what makes "a Memory can never produce a tree state the normal
UI couldn't have built" a property of the code rather than a claim in a
comment — the assertion is only worth making if there is exactly one
implementation of "could have built". The point *budget* check
deliberately stayed in the manager: it is character-scoped (one pool
shared across both trees), not a property of a node.

Behaviour-preserving. One ordering detail was kept deliberately: the
early node-existence lookup stays in `preview_allocate_passive` *before*
the preview map is touched, so asking about a nonexistent node still
can't create a preview entry as a side effect.

---

## Decisions

All numbered design decisions live in `docs/memories_spec.md`'s own
Decisions log rather than being split across two files. Three were
owner-ratified during the fit report rather than being made
unilaterally: strict orphan replay, filter scope limited to Memories,
and folding in the points-per-level copy fix.

---

## Found along the way (reported, not fixed here)

Each of these is its own small release through the queue, never bundled.
In the owner's stated priority order:

1. **Item nicknames reach Twitch chat unfiltered.** `name_item`
   (`manager.rs`) only trims and truncates; nicknames render into chat
   announcements and the OBS overlay via `Item::display_name()`. Live
   ToS exposure. `validate_memory_name` is written to be reusable here,
   though it needs a rejection path (nicknames currently truncate
   silently rather than reject).
2. **`respec_passive_tree` clears only the primary tree** while charging
   the full free-token/1000-dust cost, so Split Personality points stay
   spent against the shared budget. Owner-ruled a bug; intended
   behaviour is that a paid respec clears both. A comment marks the site
   in `manager.rs`.
3. **De-allocation does not cascade.** Dropping a parent to rank 0
   leaves its children allocated and still paying out — reachable from
   the normal UI. Needs a migration decision for saves already carrying
   orphans. This is the gap Decision 3 works around.
4. **`templates/base.html` duplicates every page's body.** Line 685
   contains `{{ body }}` inside a `//` JavaScript comment (in the
   scroll-restore block's own doc comment), so minijinja substitutes the
   whole page body there too. Two consequences: every page is roughly
   twice its necessary size, and **any newline in the body terminates
   the `//` comment, leaving the remainder to be parsed as JavaScript** —
   a syntax error that kills the entire inline script block, which is
   what performs scroll restoration. Player-reachable today via item
   nicknames, which are not newline-stripped (finding 1's vector, second
   consequence). Confirmed empirically with a throwaway probe
   (`render_page("<p>alpha\nbeta</p>")` → body text appears twice, and
   `beta</p> closes, so every element it needs already exists;` lands
   outside the comment inside `<script>`), not by reading alone. Found
   while writing this feature's HTTP test, whose slot-count assertions
   are written as `N * 2` with a note rather than baking the doubled
   count in as if intended.
5. **Doc-only**: `passive_tree.rs`'s module doc says "429 allocatable
   nodes"; the real count is 471 (Monk +3 from its irregular
   Skill-parented Modifiers, Elementalist +39).

---

## For a fresh session picking this up

- Read `docs/memories_spec.md` first — especially the Decisions log, so
  you don't re-litigate settled calls.
- The whole policy surface is pure functions in
  `game/src/adventure/memory.rs`. If you're changing behaviour, that's
  almost certainly where, and it's testable with no manager, no save
  file and no server.
- `manager.rs`'s Memory methods are deliberately thin: lock, call in,
  persist, broadcast. Resist putting policy there.
- The UI has **not** had a human visual review yet — it was verified
  structurally (render tests) and end to end (HTTP test), not by eye.
  The Elementalist precedent was that the owner reviews the
  `/passives` UI specifically before merge.
