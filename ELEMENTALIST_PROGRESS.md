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
- [x] Stage 4 — Righteous Fire part 2 (regen clock, Fanning Flames,
      Rising Phoenix, Shielding Flames, Cleansing Flames) (done
      2026-08-19) — completes the ENTIRE Righteous Fire branch (13/13
      nodes now real, including Cleansing Flames' own 3 modifiers,
      Enshrouded/Guardian/Shielding Fire - see Decision 20 below on why
      those 3 are counted as part of this stage)
- [x] Stage 5 — Golem foundation (summoner-death rule, dead-but-
      scheduled lifecycle state, on-death hook wrapper, Golem Master,
      `/passives` type-picker UI, Basic golem) (done 2026-08-19) — the
      "dead but scheduled" state and on-death hook wrapper both turned
      out to already exist from Stage 4's Rising Phoenix work, reused
      directly rather than rebuilt (see Decision 30)
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

20. **Stage 4 scope includes Cleansing Flames' own 3 modifiers**
   (Enshrouded Fire/Guardian Fire/Shielding Fire), even though the
   approved plan's own Stage 4 line only explicitly named "Cleansing
   Flames + its enumerated debuff-field list" without spelling those 3
   out by name. Judgment call: Stage 4 is "Righteous Fire branch, part
   2" and Stage 5 moves on to a completely different branch (Golem
   foundation) - leaving 3 nodes permanently `NotYetImplemented` in the
   middle of a branch every later stage has moved past would be a
   dangling gap, not a deferral. The node count confirms this reading:
   Righteous Fire's full branch is 13 nodes (1 skill + 3 specs + 9
   modifiers); Stage 3 did 5, so Stage 4 needed the remaining 8 to
   actually finish "part 2."
21. **Enshrouded Fire/Guardian Fire/Shielding Fire ride Cleansing
   Flames' own 4-second tick as an UNCONDITIONAL refresh**, independent
   of whether that tick's own probabilistic cleanse roll succeeds. None
   of the 3 are spec'd with their own chance or interval ("grants a
   number of allies... X%," no "chance every N seconds" language like
   their parent has) - reusing the parent's cadence as a flat periodic
   reapplication was the most conservative reading. Backed by a test
   proving they refresh even at `cleansingflames_chance: 0.0`.
22. **Enshrouded Fire/Guardian Fire reuse the EXISTING shared
   `temp_evasion_buff`/`temp_damage_reduction_bonus` slots** (Mage's
   Vanish and Dreadful Death's shred, respectively) rather than adding
   new fields - same "shared temp_* slot, last write wins" acceptance
   as Cauterizing Flames (Stage 3) and Guardian Fire's own dual-purpose
   reuse of `temp_damage_reduction_bonus` (Dreadful Death writes
   negative values to ENEMIES only; Guardian Fire writes positive
   values to ALLIES only - no real collision risk in practice). Shielding
   Fire's improved-block override had no existing equivalent, so it gets
   one new pair (`temp_shieldingfire_block_pct`/`_expires_at_ms`), read
   via `.max()` against the target's own `block_damage_reduction_pct` so
   it can never DOWNGRADE an ally who already has a naturally higher
   personal override - backed by a dedicated test.
23. **Rising Phoenix's death detection is a per-iteration alive-state
   sweep at the top of the main event loop**, NOT an audit of every
   individual damage-application call site. A snapshot of who was alive
   as of the previous iteration (`prev_alive`) is compared against
   current state at the top of every iteration; any real player who
   flips from alive to dead is offered a revival via
   `try_schedule_rising_phoenix_revival`. This correctly catches a death
   from ANY source (normal attacks, boss abilities, Righteous Fire's own
   self-burn, anything) with zero changes to any existing damage site -
   avoided the originally-anticipated "~9 site audit" (which the plan
   assigned to Stage 5's on-death hook instead) entirely for this
   mechanic. The main loop's own `if !u.alive { continue; }` skip is
   bypassed specifically for `revive_at_ms` (checked one line earlier),
   and `any_player_alive`'s fight-termination check now also counts a
   unit with `revive_at_ms` scheduled as "still in the fight," so an
   all-real-players-dead instant with someone due back in a second
   doesn't end the fight early.
24. **`PlayerVitals::died_at_ms`'s OWN building logic
   (`build_player_vitals` in manager.rs) needed a real fix**, not just
   documentation - it was using `get_or_insert` (first-Defeat-wins),
   which is WRONG per Stage 0's own resolution #2 ("records only a
   unit's FINAL death"). Fixed to overwrite on every `Defeat` (tracks
   the latest) and clear whenever a later `Heal`/`Attack` event shows
   `target_hp_after > 0` for that unit (a revival always surfaces this
   way - see Decision 25). This is a genuine behavior change to
   pre-existing code, but a provably safe one: before Rising Phoenix, no
   unit could ever have more than one `Defeat` event in a fight, so
   `get_or_insert` and "overwrite + clear-on-revival" are IDENTICAL for
   every fight that existed before this stage - confirmed via 2 new
   tests (`survivors_have_no_died_at_ms`/`exact_defeat_timestamp_is_
   preserved_even_mid_bucket`, both pre-existing, still pass unchanged)
   plus 2 new revival-specific tests. The field's wire SHAPE
   (`Option<u32>`, `#[serde(default)]`) is untouched - only this
   internal computation changed - so this does not violate the frozen-
   contract hard-stop condition.
25. **A revival is surfaced as existing `SkillCast`/`Heal` events, NOT a
   new `CombatEvent` variant** - `CombatEvent` is the same wire contract
   `PlayerVitals` builds from, and Stage 0's own resolution #2 already
   anticipated this exact approach ("the HP-sample curve and combat log
   show the real dip-and-recovery accurately regardless"). `NextEvent::Revive`
   sets `hp`/`alive` directly rather than calling `apply_heal` -
   reduced-healing debuffs should never apply to "coming back from
   death," and `apply_heal` isn't built to operate on an already-dead
   unit anyway.
26. **Rising Phoenix's revival HP amount (25% of max HP) is a judgment
   call** - the spec states "revive and rejoin the battle" with no HP
   number given. Chose a modest-but-meaningful fraction rather than a
   full heal, consistent with this being a safety net, not a free full
   recovery; `RISING_PHOENIX_REVIVE_HP_PCT` is a single named constant,
   easy to retune later if this reads wrong in play.
27. **Cleansing Flames' "remove all debuffs" cleanses a hand-enumerated,
   NOT exhaustive, list**: `boss_focus_stacks`, `cube_shred_stacks`
   (+ its expiry), and `wound_stacks` (+ its expiry). This is the set
   with DIRECT EVIDENCE (their own doc comments) of actually landing on
   a PLAYER unit - the real boss's own survivability-focus debuff,
   Gelatinous Cube's per-hit shred, and Festering Wound (applicable to
   either side via `apply_hit`'s `applies_wound` parameter). Deliberately
   excludes the 5 elemental on-hit debuffs (`fire_dr_debuff` etc.) and
   every `temp_*_debuff` pair (Frost Nova, Static Field, Poison Thorns) -
   their own doc comments show those are only ever applied by a PLAYER
   source against an ENEMY, never the reverse, so clearing them on an
   ally is always a no-op today. This is NOT a full audit of every
   debuff-shaped field in the ~700-field `CombatSimUnit` struct - flagged
   explicitly per the owner's own instruction that this judgment call be
   documented.
28. **Healing Flames' irregular 3/6/10% progression is a small local
   lookup (`healing_flames_regen_pct(rank)`), not a new `PassiveEffect`
   variant.** The node's own `Special{0.03, 0.035}` stays for structural
   consistency but is never actually consulted for the real value -
   confirmed nothing in the UI reads `magnitude_at_rank` for display
   (tooltips are the hand-written `description` strings, already correct
   since Stage 1). Extending the shared `PassiveEffect` enum would touch
   every archetype's node type for a need only this one node has -
   flagged back in Stage 1's own header comment as the expected
   resolution once this stage was reached.
29. **Rising Phoenix's "survived at least 3 seconds" check uses an
   approximate death timestamp** (`prev_at_ms`, the last processed
   event's own `at_ms` - see Decision 23) rather than the EXACT
   millisecond a unit's hp hit 0, since nothing centrally records that
   without the same site-by-site audit Decision 23 avoided. In practice
   this is off by at most one event's worth of time (typically well
   under a second in a fast-ticking fight), acceptable slack against a
   3-second gameplay threshold.

30. **Stage 5's own "dead but scheduled to return" lifecycle state and
   general on-death hook wrapper both turned out to already exist**,
   built in Stage 4 for Rising Phoenix - reused directly rather than
   rebuilt. Specifically: `revive_at_ms`/the main loop's per-iteration
   alive-state sweep (`prev_alive` comparison) already generalize to
   "detect a death from any source, do X" - the summoner-death rule
   just needed its OWN sweep branch (`kill_golems_of_dead_summoners`)
   riding the SAME snapshot Rising Phoenix's own branch already
   computes, not a parallel mechanism. This meaningfully de-risked
   Stage 5 relative to the plan's own original framing (which expected
   "a general on-death hook wrapper, touching ~9 Defeat-emitting call
   sites" as NEW work) - confirms the plan's own "if not already built"
   hedge on this exact machinery.
31. **`CombatSimUnit` has no real `Default` in production code -
   deliberately** (`impl Default for CombatSimUnit` is `#[cfg(test)]`-
   gated, with its own doc explaining why: "every REAL construction
   site must be explicit about every stat"). `spawn_golem` needed a
   zeroed base and, per that same philosophy, could not simply use
   `..Default::default()` - discovered as a compile error, not
   anticipated in the plan. Resolution: extracted a new
   `zeroed_combat_unit()` function (production code, NOT test-gated)
   holding a byte-for-byte copy of the test-only Default impl's own
   ~465-field literal, kept in sync by hand. This respects the
   deliberate "no accidental production zero-inheritance" design rather
   than weakening it - `zeroed_combat_unit()` is its own explicit,
   visible opt-in, used by exactly one call site so far.
32. **Golem stat scaling (33%) applies to core magnitude/rate stats
   (max hp, atk, crit chance/multiplier, evasion, damage reduction,
   block chance) - attack cadence (`attack_interval_ms`) is COPIED, not
   scaled.** The spec's "as if they were a player with 33% of your
   stats" reads naturally as scaling OUTPUT/survivability numbers, not
   attack speed - nothing in the spec suggests golems act 33% as often.
   Every other field defaults to zero/off via `zeroed_combat_unit()` -
   "a basic unified hit," none of the Elementalist's own tree bonuses
   (elemental procs, splash, Righteous Fire, etc.) carry over to a
   golem.
33. **The 33%-less-damage-per-golem penalty is its own independent
   multiplicative term** (`golem_summon_dmg_penalty`, same shape as
   Conflagration/`conflagration_dmg_pct`), NOT folded into the shared
   additive `increased_damage` pool - required for the spec's own "1%
   of normal damage at 3 golems" to compute exactly (`1.0 - 0.33*3 =
   0.01`); mixing it into `increased_damage` would interact
   unpredictably with the caster's own other bonuses instead of always
   landing on the exact spec'd number.
34. **Golem ids use a new `__golem_` prefix, deliberately never
   matching any real roster username** - this is the ENTIRE mechanism
   keeping golems invisible in the OBS overlay (Stage 0 resolution #1):
   the player-formation loop is keyed off the real `characters` Map, so
   an id that can't appear in that map is automatically skipped, no
   explicit "invisible" flag or frontend change needed anywhere.
35. **`/passives/set-golem-type` mirrors `set-secondary`'s existing
   pattern exactly** (form, manager method, error enum, silent-redirect-
   with-popup-on-error) - one dropdown per Golem-Master-unlocked slot,
   entirely absent from the page for a non-Elementalist or an
   Elementalist with 0 points in Golem Master, matching the "hidden,
   not disabled" convention Split Personality's own section already
   established. Always free (same as every other passive-tree choice
   action), takes effect on the character's next fight (golems are
   spawned fresh every `simulate_battle` call - no "already summoned"
   live state to migrate).
36. **Basic golems get nothing beyond `spawn_golem`'s flat scaling** -
   Thunder/Flame/Water's own bespoke sub-passives (all 9 of their
   modifiers) remain `NotYetImplemented` on the tree and unread by
   `spawn_golem`/anywhere else, correctly deferred to Stage 6 per the
   original plan.

*(Everything above this line was decided without a check-in, during
autonomous execution. Nothing further yet - this section grows as
Stage 6 proceeds.)*

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
- Committed as `d5923c0`, pushed.

## Stage 4 summary (for the morning, or a fresh session)

- The ENTIRE Righteous Fire branch is now real: all 13 nodes (skill +
  3 specs + 9 modifiers). Only Golem Master's whole branch (12 nodes)
  remains `NotYetImplemented`, for Stages 5-6.
- Healing Flames/Fanning Flames/Shielding Flames all ride Righteous
  Fire's own once-a-second tick (extended `tick_righteous_fire`
  directly) - self-regen (irregular 3/6/10%, via the new
  `healing_flames_regen_pct` lookup), a splash-shared portion via the
  existing `apply_heal_splash`, and a shield via the existing
  `grant_shield`.
- Cleansing Flames got its OWN new periodic tick
  (`NextEvent::CleansingFlamesTick` -> `tick_cleansing_flames`, 4s
  cadence, independent of Righteous Fire's 1s one) - a probabilistic
  cleanse of a hand-enumerated debuff list (see Decision 27) plus an
  unconditional refresh of Enshrouded/Guardian/Shielding Fire on nearby
  allies (see Decision 21).
- Rising Phoenix required genuinely new cross-cutting machinery: a
  per-iteration death-detection sweep in the main event loop (Decision
  23), a `revive_at_ms`/`NextEvent::Revive` scheduling pair, and a real
  fix to `PlayerVitals::died_at_ms`'s own building logic in manager.rs
  (Decision 24) - the field's doc previously said "nothing revives a
  unit mid-replay," which stopped being true this stage.
- 14 new `CombatSimUnit` fields, each defaulted at all 4 construction
  sites (verified count from Stage 3's own correction still holds).
- 19 new tests (4 magnitude checks in `passive_tree.rs`, 13 in
  `combat.rs`'s new `elementalist_stage_4_tests` module, 2 in
  `manager.rs`'s existing `player_vitals_tests` module proving the
  revive-then-survive and revive-then-die-again `died_at_ms` cases).
  Full suite: **216 passing, 0 failed** (was 197).
- Committed as `ce82cca`, pushed.

## Stage 5 summary (for the morning, or a fresh session)

- Golem Master's foundation is real: `spawn_golem` builds 1-3 Basic
  golems per fight for an Elementalist with the skill invested (33% of
  the caster's core stats, attack cadence copied not scaled - see
  Decision 32), the caster's own damage drops via a new independent
  `golem_summon_dmg_penalty` term (Decision 33), and a new
  `/passives/set-golem-type` picker lets the player assign a type per
  unlocked slot (`Character::golem_slot_types`, a new additive-schema
  field - Decision 35).
- The owner-mandated summoner-death rule is real and tested: golems die
  the instant their summoner dies (`kill_golems_of_dead_summoners`) and
  never count toward the fight's own alive-party check
  (`any_real_player_alive`) - both extracted as standalone functions
  specifically so the REQUIRED test (owner's own 3rd Stage-0 addition)
  could exercise them directly, plus one end-to-end test mirroring the
  main loop's actual termination check.
- Both of Stage 5's anticipated "hard new machinery" items (dead-but-
  scheduled lifecycle state, general on-death hook wrapper) turned out
  to already exist from Stage 4's Rising Phoenix work and were reused
  directly - see Decision 30. The one genuinely new piece of
  infrastructure this stage needed was `zeroed_combat_unit()` (Decision
  31), working around `CombatSimUnit`'s deliberately test-only
  `Default` impl.
- Thunder/Flame/Water's own bespoke behavior is still entirely
  `NotYetImplemented`/unread - correctly deferred to Stage 6. Golems
  are invisible in the OBS overlay purely because their id
  (`__golem_`-prefixed) can never match the roster-keyed `characters`
  Map the player-formation loop reads (Decision 34) - no frontend
  change was needed for this at all, confirming Stage 0's own
  resolution #1.
- 11 new tests (8 in `combat.rs`'s new `elementalist_stage_5_tests`
  module covering stat scaling, the damage penalty, and the summoner-
  death rule from 3 angles; 3 in `character.rs`'s `elementalist_tests`
  covering `GolemType`'s default and `golem_slot_types`' JSON
  round-trip/pre-existing-save migration). Also caught up
  `WIKI_IMPACT.md` with one consolidated entry per stage done so far
  (Stages 1-5), since CLAUDE.md's standing rule applies regardless of
  this branch not being merged/deployed yet. Full suite: **227
  passing, 0 failed** (was 216).
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
