# Elementalist Class — Implementation Progress

**Status** (2026-08-19): Stage 0 (investigation + plan) complete and
approved. Working autonomously through Stages 1-6 overnight per
explicit owner authorization — no check-ins, most-conservative-choice-
and-document rule for any decision point, branch/commit/push only, no
deploy, no master merge. This file is written so a brand-new session
with zero memory of the planning conversation can resume exactly where
things stand — read this file first, then `docs/elementalist_spec.md`
(once Stage 1 creates it) for the full class spec.

Branch: `feature/elementalist` off `master` (created at the `master`
commit that includes the boss-size overlay fix, 2026-08-19 — Phase 1
architecture refactor is complete and deployed on `master` as of this
branch point).

Baseline before any Elementalist work: **156 tests passing, 0 failed**
(`cargo test --workspace --all-targets`), confirmed 2026-08-19
immediately before Stage 1 began. This is the number every later
stage's "all existing tests still pass" check is protecting.

---

## Stages

- [x] Stage 1 — `docs/elementalist_spec.md` + tree skeleton (39 nodes,
      structurally real, `NotYetImplemented` effects) + `Elementalist`
      archetype/access (done 2026-08-19)
- [x] Stage 2 — Elemental Focus branch (proc-frequency, crit mods,
      gear-scaled elemental damage) (done 2026-08-19)
- [x] Stage 3 — Righteous Fire part 1 (damage + self-burn, Scorching
      Flames, Relentless/Cauterizing Flames, Ashes to Ashes) (done
      2026-08-19)
- [ ] Stage 4 — Righteous Fire part 2 (regen clock, Fanning Flames,
      Rising Phoenix, Shielding Flames, Cleansing Flames)
- [ ] Stage 5 — Golem foundation (summoner-death rule, dead-but-
      scheduled lifecycle state, on-death hook wrapper, Golem Master,
      `/passives` type-picker UI, Basic golem)
- [ ] Stage 6 — Golem types (Thunder/Flame/Water) + the mandatory
      Thunder Golem all-`BossKind` damage-isolation test

(Stage numbering/order per the approved plan at
`C:\Users\Administrator\.claude\plans\jaunty-pondering-waterfall.md` —
that file is the design rationale/architecture-fit record; this file
is the live execution log.)

---

## Decisions (running log — every decision made without a check-in goes here, newest last)

*(Stage 0, already approved by the owner before autonomous work began —
listed here for completeness, not decisions made unattended):*

1. **Golem overlay visibility**: invisible in v1 - golems are
   mechanically full participants (act/absorb/heal/explode, all
   correct in the combat log and fight outcome) but draw no OBS
   sprite. Visualization is an explicit future follow-up. Reason:
   keeps this feature's frontend footprint at exactly what was scoped
   (`/passives` only) - the overlay's player-formation loop has no
   mechanism for non-roster units today and building one is real,
   separate frontend work.
2. **Rising Phoenix vs. frozen `PlayerVitals` contract**: `died_at_ms`
   stays single-shot, recording only a unit's FINAL death in a fight
   (a revived-then-still-alive unit never sets it). The HP-sample
   curve and combat log show the real dip-and-recovery regardless.
   Reason: `died_at_ms` is documented as frozen for an external
   companion-app consumer; this satisfies that contract with no
   coordination needed while still delivering the real gameplay effect
   everywhere else.
3. **Thunder Golem damage absorption scope**: external damage only -
   does NOT cancel the Elementalist's own Righteous Fire self-burn.
   Reason: the spec's own design note says sustaining RF should
   require Shielding Flames/allies/gear, not "own a Thunder Golem";
   canceling self-burn would trivially defeat that intended cost.
4. **Golem/`any_player_alive` interaction (owner-mandated rule, not a
   free choice)**: golems die when the Elementalist dies, and golems
   never count toward `any_player_alive`'s fight-termination check.
   Reason (owner's own): without this, an all-real-players-dead fight
   with a reforming Thunder Golem would never terminate.

5. **Verification builds use a separate `--target-dir target-elementalist`,
   never plain `cargo build`/`cargo test`.** `target/release/{game,
   twitch-bot-rs}.exe` are the LIVE production processes (confirmed
   running throughout this work) - a plain release build tries to
   overwrite their locked files and fails with "Access is denied."
   `/target-elementalist` added to `.gitignore`. Every stage's
   verification in this log uses this target dir; production was
   reconfirmed undisturbed (same PIDs/StartTimes) after the first build
   hit this and after switching.
6. **Dropped blanket `cargo fmt` from the per-stage checklist.** No
   `rustfmt.toml` exists in this repo, and running plain `cargo fmt
   --check` against the actual crate shows the EXISTING codebase
   (checked: `affix.rs`, untouched by this work) doesn't match
   rustfmt's default output - wide struct literals stay single-line
   where default rustfmt would wrap them. This predates Stage 1
   entirely; applying `cargo fmt` for real would rewrite large amounts
   of pre-existing, unrelated code as a side effect of this feature,
   which is worse than not running it. New code in this branch is kept
   visually consistent with the file's own established conventions by
   hand (matching e.g. `WARRIOR_NODES`'s one-node-per-line-or-wrapped-
   when-long style) instead.

7. **Elemental Focus's flat bonus AND Overshock/Polar Flux/Incinerate's
   gear-scaled bonus both feed the SAME existing `fire_damage_pct`/
   `cold_damage_pct`/`lightning_damage_pct` fields** (confirmed via
   `Affix::ColdDamage`'s own doc in `affix.rs`: these fields are
   PROC CHANCE only - "fire damage dealt" is literally labeled "dmg
   reduction debuff chance" - there is no separate raw-damage stat for
   this codebase's elemental system at all). "Scaling from X damage on
   gear" read as the modifier's own magnitude times the unit's OWN gear
   roll for that element (same "scaled off the attacker's own stat"
   shape Chakra of Light already established for `increased_damage`),
   not a second independent gear roll. Extracted into a new pure
   function, `elementalist_elemental_damage_pct` (`combat.rs`), so this
   formula is unit-testable without a full `Character`/`simulate_battle`
   harness.
8. **Shocking/Chilling/Scorching Focus ("apply debuffs X% more
   frequently") implemented as a SEPARATE extra guaranteed-stack roll**
   on top of each element's own primary proc, reusing
   `roll_chakra_of_light_stacks` directly (Monk's own template for
   exactly this "second independent trigger scaled off an own-stat"
   shape) rather than inflating the primary proc chance itself - keeps
   "more damage" (Elemental Focus) and "more frequently" (the Focus
   specs) as genuinely separate, non-conflated mechanics.
9. **Lightning/Chilling/Scorching Aegis gate on the PRIMARY proc landing
   only**, not the Focus branch's own extra roll above - "every time you
   apply a debuff" read as once per hit that lands one, not once per
   extra stack an overflow roll might additionally push. One shared
   `ELEMENTAL_AEGIS_SHIELD_DURATION_MS` constant (5s, matching
   `ARCANE_SHIELD_DURATION_MS`'s own value/reasoning) covers all 3,
   rather than 3 near-identical per-element constants.
10. **Electrical Overload/Blizzard wired as ordinary `FlatStat` entries**
   into the existing generic `CritMultiplier`/`CritChance` pool (exact
   precedent: Mage's Arcane Mastery/Critical Mass/Overload/Cataclysm) -
   zero `combat.rs` changes needed for either, unlike everything else
   in this branch.
11. **Conflagration implemented as its OWN independent multiplicative
   damage layer** (`conflagration_dmg_pct`, applied as its own
   `raw_dmg *= 1.0 + X` term right next to Rogue's Backstab/Silent
   Killer), NOT folded into the shared additive `increased_damage` pool
   - the spec's explicit "MULTIPLICATIVE increased damage" wording
   (contrasted against Scorching Flames' explicit "ADDITIVE fire
   damage" a few lines above it in the same spec) reads as a deliberate,
   meaningful distinction, not incidental phrasing.
12. **`CombatSimUnit` has 4 separate construction sites, not the 1 that
   was obvious from a first read** (`impl Default`, the real
   `simulate_battle`-embedded from-`Character` constructor, a
   boss-construction site, and one further site inside boss-adds
   injection code with pre-existing, visibly inconsistent per-line
   indentation within a single struct literal - not this branch's mess,
   predates it). A find-and-replace across `chakra_of_light_pct: 0.0,`
   as the anchor briefly produced a duplicate-field compile error - a
   sloppy `replace_all` matched the same literal twice under two
   different accidental indentations, not a genuine 5th site; caught
   immediately by `cargo build` and fixed before the commit (this entry
   originally over-corrected to "5 sites" - corrected here after
   `grep -c` confirmed 4 is the real, stable count going into Stage 3).
   Noted so a later stage adding its own new `CombatSimUnit` field
   checks all 4, not assumes 1.

13. **Righteous Fire's own tick plugs into the main event loop as a new
   `NextEvent::RighteousFireTick` variant** (once-a-second cadence,
   scheduled/gated exactly like Divine Shield - player-only), dispatching
   to a new standalone `tick_righteous_fire` function - same
   "`NextEvent` arm is a one-line call to a real function" shape
   `LingeringTick`/`tick_lingering_dots` already established, NOT the
   fully-inlined shape `CurseExpiry`/Doom uses - chosen specifically so
   this stage's genuinely new logic is unit-testable in isolation with a
   crafted `units` slice, matching the "pure functions... same style as
   adventure_reply's" verification requirement.
14. **All of Righteous Fire's damage (self-burn AND enemy) is TRUE
   damage** - no crit/evasion/mitigation roll, same "a detonation, not
   an attack" convention Warlock's Doom/Apocalypse already established.
   New shared helper `apply_true_damage` (used for both halves) does NOT
   run `apply_late_stage_penalty` - that penalty is specifically "damage
   a player deals TO a boss," and applying it to Righteous Fire's own
   self-burn would be wrong. It also skips `fire_on_kill` when
   `source_idx == target_idx` (self-burn killing the caster must never
   trigger the caster's own on-kill rewards).
15. **Relentless Flames/Cauterizing Flames ride the SAME randomly-chosen
   enemy subset Righteous Fire's own damage half already picked this
   tick**, rather than rolling their own independent splash selection -
   both are spec'd with the identical "nearby enemies based on splash"
   language, read as the same aura's own reach. **Ashes to Ashes is the
   deliberate exception**: spec'd as "ANY enemy in range" (not "a
   number... based on splash"), so it sweeps every alive enemy each
   tick unconditionally, backed by its own test proving it still
   executes enemies beyond the splash-selected subset's cap.
16. **Relentless Flames' debuff (`relentlessflames_dmg_taken_pct`) rides
   the SAME target-side vulnerability slot `boss_focus_stacks` already
   established** in `resolve_hit` (`dmg *= 1.0 + def.boss_focus_stacks +
   def.relentlessflames_dmg_taken_pct` - also mirrored into the
   curse-attribution hypothetical calc just below it, to keep that
   marginal-damage logging accurate when both are present). Unlike every
   elemental debuff, it does NOT decay - the spec's own "stacking," no
   expiry stated.
17. **Cauterizing Flames reuses the existing shared
   `temp_heal_reduction_pct`/`temp_heal_reduction_expires_at_ms` slot**
   Purging Flame (Cleric) already writes to, rather than adding a new
   field - accepts the same "last write wins, doesn't stack with a
   different source" limitation every other `temp_*` debuff slot in
   this codebase already has (a Cleric and an Elementalist debuffing the
   same enemy's healing simultaneously would overwrite, not combine -
   pre-existing codebase-wide behavior for this whole family of fields,
   not something this stage introduces).
18. **Scorching Flames' fire-damage-pct bonus is folded into the SAME
   `elementalist_elemental_damage_pct` call Elemental Focus already
   uses** (added into its `elemental_focus_pct` argument, fire-channel
   only) rather than a separate addition - both are flat, additive,
   feeding the identical field.
19. Corrected Decision 12 above: re-verified via `grep -c` going into
   this stage that `CombatSimUnit` has exactly 4 construction sites, not
   the 5 that entry originally claimed (the "5th" was a duplicate from a
   sloppy `replace_all` match, not a genuine site). Left the correction
   inline in place rather than rewriting history.

*(Everything above this line was decided without a check-in, during
autonomous execution. Nothing further yet - this section grows as
Stages 4-6 proceed.)*

---

## Unresolved / open items

- Found (not fixed - out of scope, see Decision 6): Monk's tree has a
  pre-existing structural inconsistency - the "windwalker" modifier is
  parented directly to the "flowingstrikes" SKILL rather than to one of
  its 3 Specialization children. Predates this work entirely. Worth
  the owner's awareness, not this branch's to fix.

## Stage 1 summary (for the morning, or a fresh session)

- `docs/elementalist_spec.md` committed - full spec, all Stage 0
  resolutions, now the source of truth for Stages 2-6.
- `Archetype::Elementalist` added (`character.rs`): `combat_function()`
  → Ranged, `bonus()` → splash only (matches Ranger's magnitude, no
  baseline heal_power_pct - see spec doc), added to `ALL_ARCHETYPES`
  (now 12) and the passives-page icon match (🔥) in `adventure_web.rs`.
- Full 39-node tree in `passive_tree.rs` (`ELEMENTALIST_NODES`): 3
  skills → 9 specs → 27 modifiers, every node `NotYetImplemented`.
  Two node keys renamed from their spec display name to avoid a
  collision with an existing archetype's key: "Blizzard" → key
  `hoarfrost`, "Conflagration" → key `pyroclasm` (both display names
  unchanged, only the internal key differs - see the file's own
  section comment).
- 15 new tests (7 in `character.rs`'s `elementalist_tests`, 8 in
  `passive_tree.rs`'s `tree_shape_tests` - the latter is the first
  structural tree validation this codebase has ever had, generic
  enough to protect every future archetype's tree too, not just this
  one). Full suite: **171 passing, 0 failed** (was 156).
- Committed as `a14b5d4`, pushed.

## Stage 2 summary (for the morning, or a fresh session)

- Elemental Focus skill + Shocking/Chilling/Scorching Focus specs +
  their 9 modifiers (13 of the tree's 39 nodes) wired to real effects -
  see Decisions 7-12 above for the full mechanic-by-mechanic reasoning.
  Everything outside this branch (Righteous Fire, Golem Master, and all
  their children) is still `NotYetImplemented`.
- New pure function `elementalist_elemental_damage_pct` (`combat.rs`)
  and 7 new `CombatSimUnit` fields (`shockingfocus_pct`/
  `chillingfocus_pct`/`scorchingfocus_pct`/`lightningaegis_shield_pct`/
  `chillingaegis_shield_pct`/`scorchingaegis_shield_pct`/
  `conflagration_dmg_pct`), each defaulted at all 5 construction sites
  (see Decision 12).
- 13 net new tests: `passive_tree.rs`'s `tree_shape_tests` gained 5 and
  lost 1 (the old Stage-1-only blanket "every node is NotYetImplemented"
  check, replaced by one that also asserts the Elemental Focus branch's
  13 keys now have real effects), net +4; `combat.rs`'s new
  `elementalist_stage_2_tests` module added 9 (4 pure-function cases for
  `elementalist_elemental_damage_pct`, 2 Aegis, 1 Focus extra-roll, 2
  Conflagration). Full suite: **184 passing, 0 failed** (was 171).
- Committed as `f61fd22`, pushed.

## Stage 3 summary (for the morning, or a fresh session)

- Righteous Fire (skill), Scorching Flames (spec, fire-damage-pct only -
  its own modifiers Relentless/Cauterizing/Ashes to Ashes are the other
  4 nodes this stage wires) - 5 of the tree's 39 nodes now real. Healing
  Flames/Cleansing Flames and their children, plus all of Golem Master,
  remain `NotYetImplemented` for Stages 4-6.
- New once-a-second tick (`NextEvent::RighteousFireTick` ->
  `tick_righteous_fire`, a standalone testable function, NOT inlined
  into the match arm) drives: self-burn (true damage, can kill the
  caster), enemy damage to up to `PLAYER_SPLASH_MAX_TARGETS` (+overflow)
  randomly-chosen enemies, Relentless Flames' non-decaying stacking
  vulnerability on that same subset, Cauterizing Flames' reduced-healing
  debuff on that same subset, and Ashes to Ashes' unconditional
  every-alive-enemy execute sweep. New shared `apply_true_damage` helper
  (no crit/evasion/mitigation, no late-stage penalty, no self-kill
  on-kill trigger) backs both damage halves. Full mechanic-by-mechanic
  reasoning in Decisions 13-19 above.
- 6 new `CombatSimUnit` fields (`righteousfire_pct`,
  `next_righteousfire_tick_at_ms`, `relentlessflames_pct_per_stack`,
  `relentlessflames_dmg_taken_pct`, `cauterizingflames_pct`,
  `ashestoashes_pct`), each defaulted at all 4 real construction sites
  (verified count - see Decision 19's correction to Stage 2's log).
- 13 new tests (5 magnitude checks in `passive_tree.rs`, 8 in
  `combat.rs`'s new `elementalist_stage_3_tests` module covering
  self-burn, self-burn death, enemy-damage target cap, Relentless
  Flames accumulation + its real `resolve_hit` scaling, Cauterizing
  Flames' debuff application, and Ashes to Ashes' unconditional sweep
  both with and without the splash cap in play). Full suite: **197
  passing, 0 failed** (was 184).
- One correction made to Stage 2's own Decision 12 (see Decision 19) -
  "5 construction sites" was wrong, corrected to the verified 4.
- Commit is next.

---

## Hard-stop conditions (reminder to self - halt and wait if any occur)

1. A change would break loading of existing (pre-Elementalist) save
   files.
2. Something forces violating one of the 4 decisions above.
3. The same stage fails verification (`cargo build --release
   --workspace`, clippy, fmt, full test suite, save-fixture test) 3
   times in a row.

If any of these fire, write exactly why here, in this section, BEFORE
doing anything else - then stop.
