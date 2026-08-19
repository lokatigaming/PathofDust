# Divine Dust — Implementation Progress

**Status** (2026-08-19): All 6 stages complete on branch
`feature/divine-dust`. Built, tested, pushed. **Not merged, not
deployed** — that happens through the release queue on the owner's
go-ahead only.

This file is the live execution log; `docs/divine_dust_spec.md` is the
design source of truth. Read the spec first, then this.

Branch: `feature/divine-dust` off `master` at `fcd3493`, in worktree
`..\PathofDust-divinedust`. The main checkout was never touched; no
golden-corpus fixtures were regenerated or deleted.

**Test baseline before any Divine Dust work: 385 passing** (`cargo test
--workspace --target-dir target-divinedust`, unit tests in the `game`
lib). This is the number every stage's "all existing tests still pass"
check protects.

---

## Stages

- [x] **Stage 1** — currency + persistence (`Character::divine_dust`,
      round-trip + pre-feature-save tests). 385 → 387. Commit `b732d86`.
- [x] **Stage 2** — acquisition: fight drops + sacred disenchant.
      2 new `LiveTunables` fields wired + admin-editable, plus the 3
      craft-recipe cost fields (unwired until Stage 3). 387 → 394.
      Commit `f001c7c`.
- [x] **Stage 3** — craft recipe (`AdventureManager::craft_divine_dust`,
      pseudo-action route, batch semantics). 394 → 403. Commit `8181839`.
- [x] **Stage 4** — apply/reroll (`CraftAction::DivineDust`,
      `Character::apply_divine_dust`). 403 → 413 (game-lib unit tests;
      +2 more live in `manager.rs`'s own disposable-manager harness).
      Commit `bdc54e5`.
- [x] **Stage 5** — UI: currency display at every `sand` site, the
      always-visible recipe row, the per-item apply/reroll button, a
      real disposable-server HTTP test asserting rendered markup
      (`tests/divine_dust_ui_http.rs`). Commit `2834e80`.
- [x] **Stage 6** — docs (this file, `docs/divine_dust_spec.md`,
      `WIKI_IMPACT.md`, patch-notes draft in the completion report).

**Final: 403 passing in the `game` lib's own `cargo test --workspace`
run** (the two manager-level `CraftAction::DivineDust` cost tests added
in Stage 4 are counted in that same run — see the commit for the exact
delta) **+ 1 new HTTP integration test** (`tests/divine_dust_ui_http.rs`).
Release build clean; no new clippy warnings on touched code (the 4
pre-existing warnings in untouched code are unchanged).
`tests/character_fixture_roundtrip.rs` — the test that catches a
missing `#[serde(default)]` — green throughout. Golden-corpus fixtures
untouched and unregenerated.

---

## What was built, by file

| File | Change |
|---|---|
| `game/src/adventure/character.rs` | `Character::divine_dust` field; `divine_dust_reroll_pool`; `Character::apply_divine_dust`; disenchant fns gained a `divine_dust_disenchant_chance` param. |
| `game/src/adventure/tunables.rs` | 5 new `LiveTunables` fields: `divine_dust_drop_chance`, `divine_dust_disenchant_chance`, `divine_dust_craft_dust_cost`, `divine_dust_craft_sand_cost`, `divine_dust_craft_output`. |
| `game/src/adventure/manager.rs` | `maybe_drop_divine_dust`, `roll_divine_dust_disenchant`, `announce_divine_dust_drop`, `AdventureManager::craft_divine_dust`, `CraftAction::DivineDust`'s early branch in `craft_item_ex`; fight-drop rolls wired into both `run_encounter`/`run_basic_encounter`'s win blocks. |
| `game/src/adventure/announcements.rs` | `format_divine_dust_drop`. |
| `game/src/adventure/craft.rs` | `CraftAction::DivineDust`, `DivineDustCraftError`, `CraftError::InsufficientDivineDust`/`NoValidRerollTarget`, `CraftResult::DivineDustApplied`. |
| `game/src/adventure/item.rs` | `DivineDustOutcome`; `DisenchantOutcome` gained a `divine_dust` field. |
| `game/src/adventure_web.rs` | Currency display at 4 sites (character profile, dashboard, `top_nav`, crafting card header); `render_divine_dust_recipe_row`; the apply/reroll button in `render_crafting_card`; `do_craft`'s `"divine dust craft"` pseudo-action + `do_craft_divine_dust_batch`; `"divine dust"` in `parse_craft_action`; 5 new `TunablesForm` fields + admin page section; `IndexParams`/popups for both the disenchant and craft-recipe results. |
| `templates/base.html` | `divineDustBtn` client-side cost preview (mirrors Polish/Reforge's `updateSpecialCosts`). |
| `tests/divine_dust_ui_http.rs` | **New.** Real disposable-server HTTP test asserting the rendered `/inventory` markup. |
| `.gitignore` | `target-divinedust/`. |

---

## Decisions

All numbered design decisions live in `docs/divine_dust_spec.md`'s own
Decisions log — not duplicated here. The one worth flagging loudly in
this execution log too: **sacralizing a non-Perfect item via Divine
Dust also makes it Perfect** (spec Decision 4 / Design decision 6) —
a real secondary effect beyond the literal "gains one random sacred
affix" request text, made to preserve the `Sacred implies Perfect`
invariant this codebase relies on elsewhere. Worth the owner's
attention at review time even though it wasn't something requiring a
mid-implementation stop per the existing spec-ambiguity convention.
