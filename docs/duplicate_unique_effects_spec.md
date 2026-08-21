# Duplicate Unique Effects Fix

**Source of truth for this fix.** Committed here rather than left in
chat history specifically so a fresh session with no memory of the
planning conversation can pick it up correctly. Read this file in full
before touching `UniqueAffix`/equip code again.

Branch: `fix/duplicate-unique-effects` off `master`.

---

## The bug

A live report: some players had multiple equipped items carrying the
SAME `UniqueAffix` at once (2x Split Personality, 2x Celestial
Conversion). Equip-time already enforced "one of each unique equipped
at a time" (`Character::has_conflicting_unique_affix`, called from
`receive_item`/`equip_from_inventory`) — but TWO other mutation points
granted a `UniqueAffix` directly onto an item without ever re-running
that check, so applying a shard to an already-equipped item could
silently create a duplicate:

1. `Character::apply_unique_affix` — the commit step of `CraftAction::UniqueShard`'s
   apply-time picker (`AdventureManager::choose_veil_outcome`).
2. The legacy `CraftAction::CelestialShard` branch in `craft_inner` —
   same shape, structurally identical hole. Confirmed unreachable in
   live data (every character's `CelestialShard` tokens had already
   migrated to `UniqueShard`), fixed anyway for defense in depth.

Every OTHER mutation point that can touch equipped uniques was audited
and confirmed safe: `apply_recombine_roll` (places its result through
`receive_item`, which already checks), `reforge_equipped_item`
(same-slot replacement only, can't introduce a new duplicate), the
one-time starter-kit backfill (only ever equips a fresh, non-unique
item), and every `ITEM_MIGRATIONS` entry (none touch `unique_affix` at
all). **Memories loads are NOT a vector** — `Memory`/`apply_memory` only
ever write `archetype`/`passive_allocations`/`secondary_*`/
`golem_slot_types`; gear is never read or written by a Memory
save/load, verified by reading `AdventureManager::load_memory`'s full
body, not assumed.

## The fix

One validator, not two: `Character::has_conflicting_unique_affix`
(item-shaped, used by the equip-time call sites) is now a thin wrapper
over `Character::has_conflicting_unique_affix_value` (takes the
`UniqueAffix` value directly, for a caller deciding whether to GRANT one
that isn't on the item yet).

- **`CraftAction::UniqueShard`'s picker** (`craft_item_ex`): if the
  target item is currently EQUIPPED, candidates are filtered at
  INSERT time to exclude any `UniqueAffix` that would conflict with
  another already-equipped slot. If every candidate would conflict, the
  whole action rejects with `CraftError::ConflictingUniqueAffix`,
  BEFORE the token is consumed — same "insert-time validates,
  commit-time trusts it" convention `ItemLocked`/`AlreadyUnique` already
  use, so `choose_veil_outcome`/`apply_unique_affix` never need their
  own re-check: every candidate ever offered is guaranteed
  conflict-free by construction.
- **The legacy CelestialShard branch**: same idea, single check (only
  one possible affix) — rejects with the same error if the target is
  equipped and would conflict.
- **A BAGGED target item is never filtered or rejected**, even with the
  identical conflict — a conflict there is only ever an equip-time
  concern, exactly matching how any other unique-bearing item can
  already sit unequipped in a bag today.

## The one-time cleanup

Extends `CHARACTER_MIGRATIONS` (migrations.rs) — same shape as
`migrate_celestial_shard_into_unique_shard`, a new marker file, `fn(&mut
Character)`. This IS a startup sweep (runs once, over every currently-
loaded character, gated by one marker), but that's the right read of
"avoid requiring the bot down": it rides the next normal deploy
restart, not extra downtime, and it's the codebase's own established
pattern rather than new lazy per-load machinery.

For every `UniqueAffix` currently duplicated across 2+ equipped slots:
**every copy** is unequipped (not "keep one, drop the rest" — no silent
winner-picking) into the bag, intact. Nothing destroyed, nothing
refunded, no stat changes. The player re-equips whichever one they
want. Naturally idempotent — after the first run no equipped group has
2+ items sharing a unique. Logs one `tracing::info!` line per AFFECTED
character (silent for a clean one), naming every slot and item moved.

## Scope (read-only scan, 2026-08-21, against live `adventure-characters.json`)

**7 of 60 characters, 18 items total**, split 3 characters/
`splitPersonality`, 4 characters/`celestialConversion`. One outlier
(xDaido) had all 5 equipped slots carrying `celestialConversion`
simultaneously. Reproduced exactly by
`migrations::duplicate_unique_effects_cleanup_tests::reproduces_the_live_scan_figures_from_a_seven_character_fixture`
against a synthetic fixture shaped identically to the real findings —
proves the reported blast radius is a property of the migration's own
logic, not a one-off manual read.

## Decisions log

1. Suspected cause (equip-time enforces the rule, shard-apply doesn't
   re-check) confirmed exactly, plus a second, structurally identical
   hole found in the legacy CelestialShard path — included in the fit
   report and fixed alongside the first.
2. Reject (not require-unequip-first) for an equipped-item shard apply
   that would conflict — approved, checked BEFORE token consumption,
   same convention `ItemLocked`/`AlreadyUnique` already use. This
   resolved into insert-time CANDIDATE FILTERING for the picker (since
   the conflict is per-candidate, not a single up-front boolean like
   `ItemLocked`), with a full reject only when every candidate would
   conflict.
3. Unequip-ALL-copies cleanup (not keep-one) — approved as specified,
   explicitly to avoid the migration silently picking a winner for the
   player.
4. `CHARACTER_MIGRATIONS` extension (not new per-load lazy machinery) —
   approved; riding the next normal deploy restart already avoids extra
   downtime, and this is the codebase's own established pattern for
   this exact category of one-time correction.
5. Memories verification (no gear writes, not a vector for this bug) —
   accepted as evidence-based refutation of the assignment's own
   premise that Memories loads needed enumerating as a mutation point.

## Verification

- `cargo build --release --workspace --target-dir target-duplicate-unique-effects`
  (`--workspace` required; a separate target dir is mandatory —
  `target/release/` holds live, file-locked production binaries).
- `cargo test --workspace --target-dir target-duplicate-unique-effects`.
- **No `cargo fmt`.**
- Golden-corpus fixtures: unaffected, none diverged (this fix touches
  equip/craft rules only, no combat math).
- New tests: equip-time blocking directly exercised for the first time
  (`character.rs::duplicate_unique_effects_tests`, 6 tests — previously
  untested despite already being correct); the picker's insert-time
  filter, both partial-filter and full-reject cases, plus the bag-is-
  never-filtered case (`manager.rs::unique_shard_tests`, 3 new tests);
  the legacy CelestialShard branch's equipped-vs-bagged behavior
  (2 tests); a real Memories-load inertness test against a pre-existing
  duplicate (`manager.rs::memory_manager_tests`, 1 test); the cleanup
  migration's own behavior, idempotence, the 5-slot case, and the
  7-character/18-item scan reproduction
  (`migrations.rs::duplicate_unique_effects_cleanup_tests`, 7 tests).
