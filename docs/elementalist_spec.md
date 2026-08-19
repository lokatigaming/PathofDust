# Elementalist Class Specification

**Source of truth for the Elementalist implementation (Stages 1-6,
`feature/elementalist` branch).** Committed here rather than left in
chat history specifically so a fresh session with no memory of the
planning conversation can implement any remaining stage correctly.
Read this file in full before touching any stage's code. Numbers/names
below are authoritative; if a later stage's implementation needs to
deviate from anything here, document why in that stage's commit
message and in `ELEMENTALIST_PROGRESS.md`.

See `ELEMENTALIST_PROGRESS.md` (repo root) for current stage status
and the running decisions log. See
`C:\Users\Administrator\.claude\plans\jaunty-pondering-waterfall.md`
for the full architecture-fit investigation and staged plan this spec
supports.

---

## Design decisions already made — do not re-litigate

- **Golem count and type are chosen on the passive tree** via the
  `/passives` interface. Investing in Golem Master grants golem slots
  (1/2/3). There are four golem types: **Basic, Thunder, Flame, and
  Water**. Basic is an explicit type with no sub-tree and no bonuses —
  just the standard 33%-stats golem attack. Thunder/Flame/Water are
  the typed sub-trees below Golem Master. The tree UI lets the player
  explicitly assign a type (including Basic) to each slot, and the
  selection is visible. Frontend changes for this interface are in
  scope for this feature (an exception to frontend work otherwise
  being parked) — kept minimal and localized to the passives page.
- **Ashes to Ashes, Dust to Dust affects ALL enemies, bosses
  included.** The cull threshold (enemy health below 100/200/300% of
  the Elementalist's health) applies universally — this is intentional
  capstone power, no boss exemption.
- **Righteous Fire's net self-burn is intentional.** Max rank burns
  30%/s against Healing Flames' max 10%/s regen — sustaining it is
  supposed to require Shielding Flames, allies, or gear. Do not "fix"
  the numbers.
- **Class access works exactly like existing classes** — mirrors
  however characters currently acquire/choose a class. No new
  acquisition mechanism.
- Existing characters are untouched; nothing about this feature
  migrates or modifies existing save data beyond additive schema.

## Handling spec ambiguity

Where a mechanic is underspecified (exact splash formula interaction,
timing/stacking edge cases, what "nearby" means numerically), match
the conventions the codebase already uses for similar mechanics, and
list every such judgment call in the stage's commit message and in
`ELEMENTALIST_PROGRESS.md`'s Decisions log. Per the owner's autonomous-
execution authorization: if something is genuinely unresolvable or two
spec lines contradict, make the most conservative choice consistent
with this document and the approved plan, document it, and continue —
do not stop to ask.

---

## Stage 0 resolutions (architecture-fit questions, already decided)

1. **Golem overlay visibility: invisible in v1.** Golems are
   mechanically full participants (act, absorb damage, heal, explode —
   all correctly reflected in the combat log and fight outcome) but
   draw no sprite in the OBS overlay. The player-side formation loop
   in `overlay.html` is hard-coded to the joined-roster `characters`
   Map (unlike the enemy side, which already reads live off the
   fight's unit list) — teaching it to draw non-roster units is real,
   separate frontend work, out of the "minimal, localized to the
   passives page" scope for this feature. Visualization is an
   explicit future follow-up, not part of Stages 1-6.
2. **Rising Phoenix vs. the frozen `PlayerVitals` contract:
   `died_at_ms` stays single-shot.** It records only a unit's FINAL
   death in a fight, if any — a revived-then-still-alive unit never
   sets it. The HP-sample curve and combat log show the real
   dip-and-recovery accurately regardless; only this one external-
   facing field (documented as frozen because "an external companion
   app builds against this shape") stays single-shot.
3. **Thunder Golem absorbs external damage only** — it does not cancel
   the Elementalist's own Righteous Fire self-burn. RF's sustain cost
   is deliberate (see "do not fix the numbers" above); a Golem
   trivially answering it would defeat that.
4. **Golem/`any_player_alive` interaction (owner-mandated rule)**:
   golems die when the Elementalist dies, and golems never count
   toward `any_player_alive`'s fight-termination check
   (`combat.rs:9342`, `if !boss_alive || !any_player_alive { break; }`).
   Without this, an all-real-players-dead fight with a reforming
   Thunder Golem would never terminate. This rule must be tested in
   Stage 5 (a fight where every real player dies but a Golem is still
   alive/reforming still ends promptly), not just asserted.
5. **Thunder Golem's damage-absorption mechanism is NOT a taunt.**
   Confirmed via direct investigation: there is no centralized "apply
   damage" function in `combat.rs` — HP mutation happens at multiple
   direct `unit.hp = ...` sites across `apply_hit`, `apply_splash`,
   `tick_lingering_dots`, `apply_reflect_damage`, and inline
   boss-ability code in the main event loop (every `BossKind` has its
   own bespoke special-ability damage code). A taunt-style redirect
   (Druid's Unyielding Roots) only affects a normal attack's *primary
   target selection* — it does nothing for splash's independently-
   selected secondary targets, DoT ticks already active on a party
   member, or any boss's own special-ability damage. Implementation:
   a small helper `thunder_golem_redirect(units, target_idx) -> usize`
   returning either the original target or an alive Thunder Golem's
   index on that side, called at the top of every enemy-damages-party
   site before mitigation runs — a guard clause repeated at each site,
   not a pipeline refactor. Requires an audit pass through every
   `BossKind`'s ability code. For DoTs already ticking on a party
   member when a Golem is summoned: redirect the *damage* at
   tick-resolution time to the Golem's HP, without migrating the DoT's
   own storage off the original unit. Stage 6 must back this audit
   with a seeded-fight test against every `BossKind`, not just careful
   reading (see Stage 6 below).

   **2026-08-19 bugfix — non-Thunder golem immunity made real, not
   accidental.** Live fight-log analysis confirmed Basic/Flame/Water
   golems' immunity to damage only ever held as a SIDE EFFECT of
   `thunder_golem_redirect` steering hits away from real players — it
   did nothing if a non-Thunder golem was itself already the selected
   target (the redirect's own early-return treated "already IS a
   golem" as "leave it alone," not distinguishing type), and offered
   no protection at all during a Thunder Golem's own reform gap.
   Fixed with two layers: (1) targeting — every candidate-selection
   pool an enemy attack, splash secondary, or Volatile-Magic-style
   true-damage AoE draws from now excludes protected (non-Thunder)
   golems outright (`is_protected_golem`), and `thunder_golem_redirect`
   itself now also redirects a golem-aimed hit to a random real player
   when no Thunder Golem is alive to absorb it, instead of leaving it
   on the golem; (2) damage application — every direct HP-mutation
   site that doesn't funnel through `apply_hit` (DoT ticks, Doom
   detonation, reflect, Righteous Fire/Terrifying's own true damage)
   now runs a blanket `is_damage_immune` check, the same shape as the
   existing Chakra of Life immunity guard, so a protected golem takes
   zero damage from ANY source regardless of how it was targeted.
   Known, accepted consequence: during a Thunder Golem's reform gap,
   attacks that would have leaked onto a non-Thunder golem now land on
   real players instead — that gap IS Thunder Golem's intended
   weakness, this fix just stops it from bleeding onto the wrong unit
   type. See `is_protected_golem`/`is_damage_immune`'s own doc in
   combat.rs, and the `protected_golems_take_no_damage_and_are_never_targeted_against_every_boss_kind`
   test (same seeded-fight-per-`BossKind` harness as the original
   Thunder absorption test).

---

## Class specification

**Class: Elementalist**
Base class effect: splash (number of additional targets affected)
scaling with level — implemented via the existing `PassiveStat::Splash`
mechanism (`Archetype::bonus()`), the same shape Ranger's own root
bonus already uses (`b.splash = 0.15 * mult`, `mult = 1.0 + level *
0.10`). (2026-08-19: reverified against a spec-owner ruling that
questioned whether this scaled correctly, alongside the separate
Elemental Focus/Scorching Flames per-level bugfix below — confirmed
correct and unchanged, already using the same `Archetype::bonus()`
convention every other class's base effect does; see the
`root_bonus_grants_splash_scaling_with_level_like_ranger` test in
character.rs, which already covered this.) Splash's existing
codebase convention is a *fraction* applied to a *fixed* target-count
cap (`HEAL_SPLASH_MAX_TARGETS`, `PLAYER_SPLASH_MAX_TARGETS`, plus
`SPLASH_OVERFLOW_BONUS_TARGETS` once the fraction exceeds 100%) — not a
continuously-scaling target count. Every "target count based on
splash" passive below (Fanning Flames, Enshrouded Fire, Guardian Fire,
Shielding Fire) uses this existing fixed-cap-plus-overflow convention.

**Ranks and tier gating:** all passives have 3 ranks, shown as X/Y/Z
below. Tier-2 passives (the middle tier: Healing Flames, Cleansing
Flames, Scorching Flames, Shocking/Chilling/Scorching Focus, and the
golem types) additionally accept a 4th point that grants no stat value
and exists only to unlock the tier-3 passives beneath that node. This
is the standard convention for every class already (`spec()`'s
`max_rank: 4`, `magnitude_at_rank` capping the effective rank at 3) —
reuse it directly, do not build a parallel mechanism.

### Base passive 1: Righteous Fire
Deals damage equal to 10/20/30% of the Elementalist's maximum health
to a number of enemies based on splash. The Elementalist takes
10/20/30% of their health as damage per second while it's active.

- **Healing Flames** — regenerate 3/6/10% of your health per second.
  - *Fanning Flames* — share 33/66/100% of your Healing Flames
    regeneration with nearby allies, target count based on splash.
  - *Rising Phoenix* — when nearby allies die, up to 1/2/3 of them
    revive and rejoin the battle 1 second after death. Only applies to
    allies that had survived at least 3 seconds. The 1/2/3 count is a
    per-combat limit.
  - *Shielding Flames* — 33/66/100% of your Healing Flames
    regeneration is also added as a shield on you, in addition to the
    healing.
- **Cleansing Flames** — 33/66/100% chance every 4 seconds to remove
  all debuffs from yourself and nearby allies, target count based on
  splash.
  - *Enshrouded Fire* — grants a number of allies (based on splash)
    3/6/9% multiplicative evasion.
  - *Guardian Fire* — grants a number of allies (based on splash)
    3/6/9% multiplicative reduced damage taken.
  - *Shielding Fire* — grants a number of allies (based on splash)
    improved block: blocked attacks reduce damage by 55/60/65%
    instead of the standard 50%.
- **Scorching Flames** — gain 10/20/30% additive fire damage ×
  CHARACTER LEVEL — e.g. at level 194, rank 3 (30%) is 5,820%
  additive fire, stacking on top of Elemental Focus's own fire
  contribution. (2026-08-19: same implementation bugfix as Elemental
  Focus above — see `elementalist_per_level_elemental_pct` in
  combat.rs.)
  - *Relentless Flames* — a number of nearby enemies (based on
    splash) take 1/2/3% increased damage per second for every second
    they remain in the Elementalist's presence (stacking).
  - *Cauterizing Flames* — a number of nearby enemies (based on
    splash) receive 5/10/15% multiplicative reduced healing.
  - *Ashes to Ashes, Dust to Dust* — any enemy in range, including
    bosses, instantly bursts into flame and dies when its health
    drops below 100/200/300% of the Elementalist's health.

### Base passive 2: Elemental Focus
Gain 5/10/15% additive elemental damage (lightning/cold/fire) ×
CHARACTER LEVEL, applied to each element separately (not one pool
split three ways) — e.g. at level 194, rank 3 (15%) is 2,910%
additive to lightning, AND to cold, AND to fire, each independently.
(2026-08-19: implementation bugfix — the code never actually
multiplied by level despite this spec text always saying "per level";
see `elementalist_per_level_elemental_pct` in combat.rs.)

**Balance note, confirmed at fix time:** `fire_damage_pct`/
`cold_damage_pct`/`lightning_damage_pct` are NOT raw damage
multipliers — every read site (`apply_hit`'s 5 elemental-proc rolls,
`apply_heal`'s on-heal buff rolls) feeds them through
`roll_elemental_proc`, which divides by `ELEMENTAL_PROC_CHANCE_DIVISOR`
(10.0) and hard-clamps the result to `[0.0, 1.0]` — a raw value of
10.0 (1000%) already guarantees a 100% proc chance, and every point
above that is inert. Reaching that clamp only takes rank 3 Elemental
Focus alone around level ~67 (10.0 / 0.15), or fire specifically much
sooner once Scorching Flames and/or gear rolls stack in. This fix does
NOT inflate the Elementalist's raw damage-per-hit at all — increased_damage/
crit/base attack damage are untouched — it only affects how reliably
the elemental on-hit DEBUFFS (Fire DR reduction, Cold evasion
reduction, Lightning damage-taken stacks) land, and only for a
character below that saturation level; anyone already near or at max
level was very likely already at or near the 100% clamp before this
fix too (via gear rolls alone, independent of Elemental Focus).

- **Shocking Focus** — you apply lightning damage debuffs 33/66/100%
  more frequently.
  - *Overshock* — 15/30/45% more lightning damage, scaling from
    **lightning** damage on gear.
  - *Electrical Overload* — gain 10/20/30% more critical strike
    damage.
  - *Lightning Aegis* — gain 1/2/3% of your health as shield every
    time you apply a lightning debuff.
- **Chilling Focus** — you apply cold damage debuffs 33/66/100% more
  frequently.
  - *Polar Flux* — 15/30/45% more cold damage, scaling from **cold**
    damage on gear.
  - *Blizzard* — gain 10/20/30% more critical strike chance.
  - *Chilling Aegis* — gain 1/2/3% of your health as shield every time
    you apply a cold debuff.
- **Scorching Focus** — you apply fire damage debuffs 33/66/100% more
  frequently.
  - *Incinerate* — 15/30/45% more fire damage, scaling from **fire**
    damage on gear.
  - *Conflagration* — gain 10/20/30% multiplicative increased damage.
  - *Scorching Aegis* — gain 1/2/3% of your health as shield every
    time you apply a fire debuff.

### Base passive 3: Golem Master
Grants the ability to summon 1/2/3 golems. Golems have 33% of the
Elementalist's FULLY-BUFFED EFFECTIVE stats — the Elementalist's real,
post-buff numbers at the moment of summon (level, all tree passives
including per-level Elemental Focus/Scorching Flames, and gear), "as if
they were a player," not a partial or base-only snapshot. (2026-08-19
bugfix: the original implementation copied base max hp/atk/crit/evasion/
DR/block at 33% correctly, but left increased_damage and
conflagration_dmg_pct at 0 entirely — a real build's own gear/tree
damage multipliers never reached the golem at all. Fixed by inheriting
those two at FULL value, not scaled to 33% like the base stats — the
arithmetic only lands at the intended ~33% per-hit-damage RATIO if a
multiplicative term passes through whole: `golem_dmg = (atk × 0.33) ×
(1 + full_increased_damage) = 0.33 × owner_dmg`, exactly, regardless of
how large that multiplier gets. Scaling the multiplier down to 33% too
compounds against the already-scaled base stat instead of canceling out,
converging toward an ~11% ratio at a large dominant multiplier — this
is what the live audit's own "~3% instead of 33%, an 11x gap" finding
measured. See `spawn_golem`'s own doc in combat.rs.) The Elementalist
deals 33% less damage per summoned golem, additive — at 3 golems the
Elementalist deals 1% of their normal damage. Golems attack with a basic
unified hit as if they
were a player with 33% of the Elementalist's stats. Golems take no
damage unless a golem type specifies otherwise. There are four golem
types — Basic, Thunder, Flame, Water — assigned per slot via the
passive tree. Basic golems have no sub-tree and no bonuses: pure base
behavior and stats.

- **Thunder Golem** — absorbs all damage the party takes until it
  dies (external damage only — see Stage 0 resolution #3). Cannot be
  shielded or healed by any means. Reforms 4/3/2 seconds after dying
  and rejoins combat.
  - *Gigantify* — Thunder Golems get 100/200/300% more contribution
    from your health pool (base 33% of your health → 66/99/132%).
  - *Growing* — Thunder Golems gain 33/66/100% more maximum health
    each time they reform, ADDITIVELY off their ORIGINAL spawn-time
    max hp, stacking within a combat: `max_hp = reform_base * (1.0 +
    rank_pct * reform_count)`. NOT compounding onto the
    already-grown value — 12 reforms at rank 3 (100%/reform) lands
    at exactly 13x the original base, never 2^12x. (2026-08-19: the
    original implementation compounded onto the current, already-
    grown max_hp every reform, misreading "stacking within a
    combat" as authorizing that; confirmed unintended via live
    fight-log analysis and fixed the same day — see
    `thundergolem_growing_pct`'s own doc in combat.rs.)
  - *Terrifying* — when a Thunder Golem dies, it explodes dealing
    33/66/100% of its health as damage to enemies.

  **Final sizing formula, traced and confirmed correct against the
  code (2026-08-19, golem integrity audit item A3):**
  `max_hp_at_spawn = summoner.max_hp × 0.33 × (1.0 + gigantify_pct)`,
  where `summoner.max_hp` is the Elementalist's own fully-buffed
  effective max hp at the moment of summon and `gigantify_pct` is
  0.0/1.0/2.0/3.0 at rank 0/1/2/3 (so rank 3 gives `0.33 × 4.0 = 1.32×`
  the owner's max hp - matches the "66/99/132%" wording above exactly).
  On each reform, `max_hp = max_hp_at_spawn × (1.0 + growing_pct ×
  reform_count)` (see Growing's own entry above for the additive-not-
  compounding fix). Both halves are covered by their own passing unit
  tests (`gigantify_raises_thunder_golem_hp_contribution`,
  `golem_reform_growing_is_additive_not_compounding_across_many_reforms`)
  proving the formula is internally correct for a KNOWN rank/reform
  count. A live audit found an apparent ~23% shortfall against a
  "predicted" 1.32× for one specific character - traced as far as
  possible without that character's own real `passive_allocations`:
  the predicted figure assumes rank-3 Gigantify AND rank-3 Growing:
  if the real character's investment in either is actually lower, the
  "predicted" comparison figure itself would be wrong, not the code -
  a rank-2 Growing golem that's reformed several times would
  legitimately sit well below a rank-3 assumption's own prediction.
  **Not resolved as a confirmed code bug** - re-flag with the actual
  character's real Gigantify/Growing ranks if the shortfall persists.
- **Flame Golem** — base behavior is the standard golem attack; the
  sub-passives are its identity.
  - *Volcanic Ash* — Flame Golems inherit 33/66/100% of the
    Elementalist's multiplicative increased fire damage.
  - *Blazing* — Flame Golems gain 6/9/18% multiplicative attack speed.
  - *Surging* — Flame Golems deal 10/20/30% multiplicative damage.
- **Water Golem** — base behavior is the standard golem attack; the
  sub-passives are its identity.
  - *Replenishing* — Water Golems convert all damage they deal into
    healing for the party at a 100/200/300% rate.
  - *Singing* — all allies gain 10/20/30% more effect from shields and
    heals applied to them.
  - *Shattering* — when an enemy dies in the Water Golem's presence,
    it explodes, sending icicles at (splash + 1/2/3) nearby enemies,
    each dealing damage equal to 1% of the dead enemy's health.

---

## Verification requirements (every stage)

- `cargo build --release --workspace` (plain `cargo build` misses
  `game`'s own binary target), clippy clean on touched code, `cargo
  fmt` applied.
- All existing tests pass (156-test baseline as of Stage 0 completion,
  2026-08-19), especially the save-fixture and redemption-table tests.
- New domain logic (splash targeting, RF burn/regen interaction, golem
  stat derivation and damage penalty, revival rules, debuff
  application) covered by unit tests as pure functions, same style as
  `adventure_reply`'s (Phase 1 precedent — plain functions, no I/O,
  exhaustive over the stage's own policy table).
- A round-trip test for any new persisted fields, and a test proving a
  pre-Elementalist save file still loads.
- Stage 5 specifically: a test proving the golem/`any_player_alive`
  rule (all real players dead + a Golem alive/reforming still
  terminates the fight promptly).
- Stage 6 specifically: a seeded fight simulation run against every
  `BossKind` variant, asserting that while a Thunder Golem is alive no
  non-golem party member takes any externally-sourced damage
  (Righteous Fire's own self-burn exempt).

## Deploy

Do not deploy. Branch commits and pushes to `feature/elementalist`
only — no merge to master, no touching production tasks/processes.
Merging and deploying happens only after explicit owner go-ahead, after
review of the passives-page tree/golem-picker UI specifically.

(The above "do not deploy" line is the original Stage 1-6 branch-only
instruction, kept as written for history — every stage has since been
merged and deployed; see `ELEMENTALIST_PROGRESS.md` for the actual
deploy record and every post-deploy bugfix release since.)

---

## Balance projection: combined (self + golem) output vs. the party
(2026-08-19, written for the golem-stat-inheritance fix above — see
that entry for the specific bug this projection accounts for)

**Expected band: an Elementalist's TOTAL output (their own attacks +
every summoned golem's) should land close to 100% of what they'd deal
running solo with no golems at the same level/build — not a clear top
performer on the party's damage meters purely from Golem Master, and
not a clear underperformer either.** This falls out of the design
being complementary by construction, now that both halves are correct:

- Golem Master's own penalty: the Elementalist deals `1 - 0.33 * golem_count`
  of their normal damage (33% less per golem, additive) — 67% at 1
  golem, 34% at 2, 1% at 3.
- Each golem's own per-hit output (post-fix): ~33% of what the
  Elementalist would deal per hit, unreduced — see `spawn_golem`'s own
  doc in combat.rs for why this requires inheriting increased_damage/
  conflagration_dmg_pct at FULL value, not scaled down too.
- Summed: 1 golem → 67% + 33% ≈ 100%; 2 golems → 34% + 66% ≈ 100%; 3
  golems → 1% + 99% ≈ 100%. The two halves are designed to cancel out
  to roughly the SAME total regardless of golem count — Golem Master
  trades some of the Elementalist's own damage for build diversity/
  Thunder Golem's tanking utility, it doesn't inflate or deflate total
  party contribution.

**Expect real numbers to land a few points BELOW 100%, not above**, for
a build with meaningful crit investment specifically: golem
`crit_chance`/`crit_multiplier` are still scaled to 33% each (unchanged
by this fix, not part of the ruling that prompted it) - at that
combined scale a golem's own crits contribute little to nothing extra
over a non-crit hit, unlike the owner's real crit investment, so a
crit-heavy Elementalist's combined total will trend a bit under the
clean ~100% figure above. This is a real, currently-unaddressed
follow-up if it turns out to matter in practice, not something this
fix attempts to solve.

**Thunder Golem's absorbed damage is separate, additional value on
top of this projection** - it's tanking contribution, credited to the
owner's own damage_taken stat per the golem-attribution work, and has
no bearing on the damage-dealt total above. Expect an Elementalist
running Thunder Golem to rank disproportionately high on Top Tanks
specifically (compared to their Top DPS standing) - that's the
mechanic working as intended, not a balance concern.

**What this projection does NOT cover**: healing output (Water Golem's
Replenishing, separately rolled up per the golem-attribution work,
follows its own `heal_power` economy independent of this damage
projection) and the still-open Thunder-redistribution change queued
behind this release, whose own tank-credit formula depends on this
fix's new absorption magnitudes and is out of scope here.

---

## Release 1, Part A — golem integrity fixes (2026-08-19)

A verification pass over the shipped golem work found four real gaps,
fixed together before Part B below (which builds on a Thunder Golem's
now-accurate absorbed-damage bookkeeping):

- **A1 — golems could self-heal via Festering Wound's leech.** Root
  cause: `wound_leech_bonus` is a TARGET-side field (Festering Wound's
  leech applies to WHOEVER attacks a wounded target), independent of
  the attacker's own `life_leech_pct` (always 0 for a golem, which
  never inherits leech stats). Not something the earlier stat-
  inheritance fix caused — golems never had leech fields copied to
  them — but a real leak regardless. Fixed by gating both
  `wound_leech_bonus` and the general `effective_leech_pct` leech
  calculation on `!attacker.is_golem`, and adding `golem_may_produce_heals`
  as defense-in-depth at every other self-heal site a golem could
  theoretically reach (Wound Explosion self-leech, Bloodpact Shared
  Pain/refund) even though those are structurally unreachable for a
  golem today.
- **A2 — Thunder Golem's heal/shield immunity only lived inside
  `apply_heal`.** The existing immunity comment claimed `apply_heal`/
  `grant_shield` were "the two fully centralized application points
  every shield/heal source in this file already funnels through" —
  true for `grant_shield`, false for `apply_heal`: several hand-rolled
  heal-write sites bypass it entirely, most notably the Guardian
  Spirit/Undying Fury/Verdant Burst/Soul Stone/Chakra of Life
  would-kill-save cascade. This exactly matches a live audit finding
  ("healed a Thunder golem for 865,341 twice"). Fixed with a shared
  `is_heal_immune`/`golem_may_produce_heals` pair of helpers, applied
  as a target-side backstop at every direct HP-restoration site in the
  file (the would-kill save cascade, `apply_heal_splash`,
  `apply_radiant_smite_heal`, Nature's Embrace splash, `apply_heal_bounce`,
  Unbroken Prayer bounce, the unified-action heal-lowest-ally pool),
  not just the two "shared" functions.
- **A3 — golem HP sizing.** Traced and confirmed mathematically
  correct against the code — see the sizing-formula callout in Thunder
  Golem's own entry above. Not a confirmed bug.
- **A4 — zero-amount heal event bloat.** The Lingering Effect
  heal-flavor tick (50ms cadence) pushed a `Heal` event unconditionally
  on every tick, including when the target was already at full hp
  (`healed == 0`) — unlike `apply_heal`'s own `if healed > 0` push
  convention. Confirmed as the live audit's own "2,296 of ~2,444 heal
  events... amount:0" finding. Fixed by nesting the event push inside
  `if healed > 0`, same as every other heal site. **Not golem-specific**
  — this fixes event bloat for every heal-flavor Lingering Effect tick
  regardless of target, so the golden-corpus regression fixtures for
  every archetype needed regenerating alongside this release, not just
  the Elementalist ones.

---

## Release 1, Part B — Thunder Golem absorbed-damage redistribution (2026-08-19)

**B1 — the mechanic.** Every incarnation of a Thunder Golem tracks its
own `thundergolem_absorbed_this_incarnation`: the total damage actually
applied to its own hp (after its own mitigation), since spawn or since
its most recent reform, whichever is later. When that incarnation dies,
`thunder_redistribution_pct` (a live tunable, default 50%) of that total
splits evenly across every currently-alive REAL party member (never
another golem — B2) as unmitigated true damage (no evasion/block/DR
roll), delivered as 2 equal ticks spread across
`thunder_redistribution_window_secs` (a live tunable, default 2.0s —
tick 1 at half the window, tick 2 at the full window). A lethal tick
resolves exactly like any other killing blow (hp hits 0, `Defeat`
fires, Rising Phoenix eligible) — no special-casing, since the main
loop's own generic "who died since last iteration" sweep picks it up
the same as any other death. Zero absorbed damage or a 0%
`thunder_redistribution_pct` schedules nothing at all. The per-
incarnation tally resets to 0 on spawn and on every reform — a
long-lived Thunder Golem's later incarnations only ever redistribute
what THAT incarnation itself absorbed, not its predecessors'.

**B2 — never lands on a golem.** The recipient pool is built directly
from currently-alive real party members (`!is_boss && !is_golem`), so
this bypasses `thunder_golem_redirect` by construction — there is
nothing to redirect away from, since a golem was never a candidate
recipient in the first place, even if a second Thunder Golem is alive
at the same moment.

**B3 — Terrifying's explosion is unchanged and separate.** It fires
from the same `handle_golem_death` call (outgoing damage to enemies,
based on `max_hp`) with no interaction with the incoming-damage
redistribution mechanic below it in the same function.

**B4 — attribution: environmental, not the Elementalist's own.** Every
redistribution tick's `Attack` event carries a sentinel `attacker` id
(`__thunder_redistribution__`, no real unit is ever built with that
shape) tagged with a new `AttackSourceKind::Environmental` variant.
`full_player_fight_stats`'s `stats.get_mut(attacker)` never finds a
match for the sentinel, so nobody's own `damage_dealt` is credited —
it is deliberately unattributed, "not enemy damage, not the
Elementalist's own." Each death that triggers a redistribution also
pushes a distinct `SkillCast { skill: "Thunder Golem Redistribution" }`
observability marker, same convention as the Reform/Righteous
Fire/Healing Flames markers.

**B5 — tunables.** `LiveTunables::thunder_redistribution_pct` (0.0-1.0,
default 0.50) and `LiveTunables::thunder_redistribution_window_secs`
(default 2.0), both live-editable at `/admin/tunables` under a new
"Elementalist" section, re-read on every death (not cached), so a
change takes effect on the very next fight.

**B6 — tank-credit formula.** A Thunder Golem's own lifetime
`thundergolem_net_absorbed` running total is what
`full_player_fight_stats` rolls up into the owner's `damage_taken`
stat, NOT the plain event-log sum every other golem type still uses —
crediting both the full raw absorbed total AND the redistributed
ticks' own separate `damage_taken` (already counted on each
recipient's own row) would double-count the redistributed portion.
Computed incrementally: every absorbed hit adds to
`thundergolem_net_absorbed` (mirroring `thundergolem_absorbed_this_incarnation`,
but never reset by a reform — it's a lifetime total), and
`handle_golem_death` immediately subtracts whatever fraction just got
redistributed away. Net effect: an incarnation that dies mid-fight
leaves `(1 - thunder_redistribution_pct) × absorbed` credited to the
owner's tank stat; an incarnation still alive at fight end keeps its
full absorbed amount, since it was never redistributed at all.
