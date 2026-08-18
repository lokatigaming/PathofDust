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

---

## Class specification

**Class: Elementalist**
Base class effect: splash (number of additional targets affected)
scaling with level — implemented via the existing `PassiveStat::Splash`
mechanism (`Archetype::bonus()`), the same shape Ranger's own root
bonus already uses (`b.splash = 0.15 * mult`). Splash's existing
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
- **Scorching Flames** — gain 10/20/30% additive fire damage per
  level.
  - *Relentless Flames* — a number of nearby enemies (based on
    splash) take 1/2/3% increased damage per second for every second
    they remain in the Elementalist's presence (stacking).
  - *Cauterizing Flames* — a number of nearby enemies (based on
    splash) receive 5/10/15% multiplicative reduced healing.
  - *Ashes to Ashes, Dust to Dust* — any enemy in range, including
    bosses, instantly bursts into flame and dies when its health
    drops below 100/200/300% of the Elementalist's health.

### Base passive 2: Elemental Focus
Gain 5/10/15% additive elemental damage (lightning/cold/fire) per
level.

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
Elementalist's stats. The Elementalist deals 33% less damage per
summoned golem, additive — at 3 golems the Elementalist deals 1% of
their normal damage. Golems attack with a basic unified hit as if they
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
    each time they reform (stacking within a combat).
  - *Terrifying* — when a Thunder Golem dies, it explodes dealing
    33/66/100% of its health as damage to enemies.
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
