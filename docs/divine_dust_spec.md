# Divine Dust

**Source of truth for this feature.** Committed here rather than left in
chat history specifically so a fresh session with no memory of the
planning conversation can pick it up correctly. Read this file in full
before touching Divine Dust code. If an implementation needs to deviate
from anything here, document why in the commit message and add a
numbered entry to the Decisions log below.

Branch: `feature/divine-dust` off `master`. Companion execution log:
`DIVINE_DUST_PROGRESS.md`.

---

## What it is

A per-character currency (`Character::divine_dust: u64`, additive
`#[serde(default)]`) for making an item Sacred and rerolling its sacred
affix, on top of its own dust+sand craft recipe. Displayed wherever
`sand` is displayed (public character profile, dashboard, `top_nav`,
crafting card header).

Sacred items and sacred affixes already existed (`Item::sacred_affix`,
`make_item_sacred`) before this feature — Divine Dust is a new currency
layered on top of that existing mechanic, not a new item state.

## Design decisions already made — do not re-litigate

1. **Acquisition, three sources, all additive:**
   - Fight drops: `LiveTunables::divine_dust_drop_chance` (default 0.1),
     rolled once per fighting character on every WIN (boss or basic —
     the same eligibility `sand`'s own grant uses), granting exactly 1.
     Originally announced in chat (rare-event framing); the chat
     announcement was removed 2026-08-19 (a live request, chat-noise
     reduction) - `announce_divine_dust_drop`/`format_divine_dust_drop`
     no longer exist. The grant itself is completely unaffected, silent
     now like the disenchant/craft sources below.
   - Disenchanting a SACRED item: `LiveTunables::divine_dust_disenchant_chance`
     (default 0.1), rolled once per Sacred item MANUALLY disenchanted
     (`disenchant_from_inventory`/`disenchant_all_from_inventory` only —
     a Sacred item can never reach the auto-disenchant path, since it
     always meets every `AutoDisenchantTier` floor). Non-sacred
     disenchants always yield 0. Silent (no announcement) — same
     "routine, not an event" treatment auto-disenchant loot already
     gets.
   - Craft recipe: `LiveTunables::divine_dust_craft_dust_cost`/
     `_sand_cost`/`_craft_output` (defaults 1000/10/1), a standalone
     `/craft` recipe (`AdventureManager::craft_divine_dust`), x1/x10/x50
     batchable via a dedicated pseudo-action (`"divine dust craft"` in
     `do_craft`), following `do_craft_batch`'s exact stop-on-shortfall
     convention. Silent — crafted output is routine, not an event.
2. **Rate derivation.** Both drop-chance tunables default to 0.1 —
   1/10th of sand's own equivalent rate. Sand has no literal probability
   constant (its fight-grant is unconditional; its disenchant-grant's
   chance is `quality_percent()/100`, which for a Sacred item — always
   `perfect`, hence 100% quality — is *also* unconditional). Both are
   therefore treated as an implicit rate of 1.0, and Divine Dust's
   default is 1/10th of that: 0.1. Owner-blessed derivation (2026-08-19)
   — if the real drop rate ever needs different tuning, change the
   `LiveTunables` value; don't re-derive from sand at runtime.
3. **Usage — apply/reroll, `CraftAction::DivineDust`:** costs
   `2 × item.tier` Divine Dust, computed and deducted by
   `AdventureManager::craft_item_ex`'s own early branch (same split as
   Polishing/Reforge — the pure fn only validates/mutates).
   - Not yet Sacred: sacralizes in place — see Decision 6 below for why
     this also applies `make_item_perfect`'s effect, not just the affix.
   - Already Sacred: rerolls to a different random affix, current
     excluded. Empty-pool guard (`CraftError::NoValidRerollTarget`)
     implemented and unit-tested, though unreachable at the real
     17-variant `ALL_AFFIXES` size.
   - Respects `item.locked` (Krangled items reject, same as every other
     crafting action).
   - Exactly one sacred affix per item, always — never multi-affix.
4. **Economy note (accepted, recorded deliberately).** The recipe's
   dust cost (1000, default) is deliberately CHEAP relative to veteran
   dust holdings — dust income scales with stage/loot_mult and
   compounds over a long-lived character's whole history. **Sand is the
   intended pacing constraint** on how fast a player can convert their
   held dust into Divine Dust: sand income is comparatively low and
   flatter (flat per-win grants, `sand_mult`-scaled, not stage-scaled
   the way dust is), so the sand cost (10, default) is what actually
   throttles this recipe in practice, not the dust cost. Recorded here
   so a future economy review reads this as a deliberate choice, not an
   oversight — same spirit as `memories_spec.md`'s own economy note for
   free Memory swaps.
5. **UI placement.** Both the recipe and the apply/reroll action live on
   the existing `/craft` page. The recipe row is its OWN `<form>`,
   separate from the main item-crafting form, and deliberately NOT
   gated behind the character owning any items — it's a pure currency
   conversion, unlike every other action on this card. The apply/reroll
   button lives inside the main item-crafting form (it needs `item_a`),
   priced client-side off the selected item's tier, same pattern
   Polishing/Reforge already use.
6. **Sacralizing via Divine Dust also makes the item Perfect (judgment
   call).** The spec text as given only said "becomes sacred, gains one
   random sacred affix" — it didn't explicitly say the item also
   becomes Perfect. Implemented so it DOES, because `Sacred implies
   Perfect` is a load-bearing invariant elsewhere in this codebase:
   - `Item::meets_auto_disenchant_floor`'s `Quality% < Perfect < Sacred`
     ordering (a Sacred item is always `perfect: true`).
   - `disenchant_multiplier`'s `Sacred(N) == Perfect(N+1)` equivalence
     (the sacred implicit counts as one more affix toward the same
     disenchant-value step Perfect climbs).
   - This very feature's OWN Decision 2 above: `divine_dust_disenchant_chance`'s
     default derivation assumes a Sacred item's `quality_percent()` is
     always exactly 100.
   A player-mintable "Sacred but not Perfect" item would quietly break
   all three. So `Character::apply_divine_dust`'s sacralize path
   reuses `make_item_perfect`'s exact effect (power_roll maxed, every
   existing affix value bumped by `PERFECT_QUALITY_MULT`) when the item
   isn't already Perfect, before rolling the sacred affix — identical
   to what a natural Sacred drop (`make_item_sacred`) already does.
   Guarded against double-applying the multiplier if the item was
   already Perfect (checks `item.perfect` first). This is a real,
   non-obvious secondary effect worth knowing about: applying Divine
   Dust to sacralize a non-Perfect item is "make Perfect + gain a
   sacred affix" in one action. Flagged explicitly in the Stage 4
   completion report; not re-litigated here, just recorded.

## Rules the implementation must keep

- **Persistence is additive.** `divine_dust` is `#[serde(default)]` on
  `Character`. Old saves load unchanged — proved by
  `a_character_saved_before_divine_dust_existed_still_loads_with_zero`
  and by `tests/character_fixture_roundtrip.rs` staying green.
- **Every currency-consuming action is atomic.** Insufficient funds
  reject cleanly with `CraftError::InsufficientDivineDust`/
  `DivineDustCraftError::InsufficientDust`/`InsufficientSand` and
  consume nothing — proved by
  `craft_divine_dust_insufficient_dust_consumes_nothing`,
  `craft_divine_dust_insufficient_sand_consumes_nothing_even_with_plenty_of_dust`
  (the recipe's two currencies are checked together before either is
  deducted — a sand shortfall never partially spends dust), and
  `applying_divine_dust_with_insufficient_balance_consumes_nothing`.
- **Batch semantics match `do_craft_batch` exactly.** One atomic unit
  per iteration, stop at the first failure, whatever already landed
  stays applied — proved by
  `repeated_calls_match_the_batch_stop_on_shortfall_convention`.
- **No golden-corpus fixture changes.** Divine Dust is a currency/craft
  feature, not a combat mechanic — nothing under
  `game/tests/fixtures/golden_corpus/` should ever need touching for
  this feature.

## Decisions log (newest last)

1. **Rate derivation blessed as 1/10th of sand's implicit rate=1.0**
   (both fight-drop and disenchant chance) — see "Design decisions"
   item 2 above. Owner-approved 2026-08-19 after the fit-report stage,
   explicitly as-proposed.
2. **Craft-recipe route is a third string-matched pseudo-action**
   (`"divine dust craft"`) in `do_craft`, alongside the existing
   `"recombine"`/`"hideout warrior"`, rather than forcing the
   currency-only recipe through `parse_craft_action`/`craft_item`'s
   item-targeted shape. Owner-approved structural choice.
3. **Apply/reroll is a new `CraftAction::DivineDust` variant** with its
   own `DivineDustOutcome`/`CraftResult::DivineDustApplied` (not folded
   into `CraftOutcome`), Polishing-style early branch in
   `craft_item_ex`, respecting `item.locked`. Owner-approved structural
   choice.
4. **Sacralizing also applies Perfect** — see "Design decisions" item 6.
   Judgment call, made to preserve the `Sacred implies Perfect`
   invariant; not explicitly specified either way in the original
   request.
5. **`divine_dust_reroll_pool` factored into its own free function**
   so the empty-pool guard (`CraftError::NoValidRerollTarget`,
   unreachable at the real 17-variant `ALL_AFFIXES` size) is directly
   unit-testable against a degenerate single-entry pool, rather than
   left as an assumed-safe but untested guard.
6. **`DisenchantOutcome` gained a `divine_dust` field** so the
   single-item disenchant popup can show "+N Divine Dust!" when a
   Sacred item's roll hits — a small UX addition beyond the literal
   spec text, consistent with "displayed wherever sand is displayed"
   (the existing popup already shows the dust roll).

## Verification

- `cargo build --release --workspace --target-dir target-divinedust`
  (`--workspace` is required; a separate target dir is mandatory —
  `target/release/` holds live, file-locked production binaries).
- `cargo test --workspace --target-dir target-divinedust`.
- `cargo clippy` clean on touched code.
- **No `cargo fmt`** — there is no `rustfmt.toml` and a blanket run
  rewrites unrelated code.
- Golden-corpus fixtures are neither regenerated nor deleted.
- New pure-domain tests, by stage: currency round-trip/migration
  (Stage 1); `maybe_drop_divine_dust`/`roll_divine_dust_disenchant`
  (Stage 2); `AdventureManager::craft_divine_dust` cost/atomicity/batch
  (Stage 3); `Character::apply_divine_dust` sacralize/reroll/locked/
  empty-pool (Stage 4); a real disposable-server HTTP test asserting
  the actual rendered `/inventory` markup (Stage 5,
  `tests/divine_dust_ui_http.rs`).
