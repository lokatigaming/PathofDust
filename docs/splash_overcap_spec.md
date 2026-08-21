# Splash Redesign (Overcap)

**Source of truth for this feature.** Committed here rather than left in
chat history specifically so a fresh session with no memory of the
planning conversation can pick it up correctly. Read this file in full
before touching Splash code. If an implementation needs to deviate from
anything here, document why in the commit message and add a numbered
entry to the Decisions log below.

Branch: `feature/splash-overcap` off `master`.

---

## What it is

A full redesign of `Affix::Splash`/`Character::combat_splash`, not the
originally-requested "+1 extra target on overcap" tweak — the ordering
process surfaced that the described starting premise (splash was a
guaranteed-hit, damage-scaling fraction; overcap already granted +2
targets via a hardcoded constant) didn't match the code, and several
rounds of owner rulings replaced it with the design below. All 6
independent call sites that used to reimplement their own splash-count
logic now route through one shared function, `roll_splash` (`combat.rs`).

## The table (owner-ratified, do not re-derive)

Splash % is now a **CHANCE**, not a damage-scaling fraction. Every
splash-hit target takes the FULL primary hit/heal value (`LiveTunables::splash_damage_pct`,
default 1.0) — never a fraction of it.

**ATTACK splash** (`apply_splash`, `apply_heal_splash` — effects with
their own primary target, splash is additional targets on top):

| Splash % | Outcome |
|---|---|
| 0% | primary target only — no splash, ever |
| 1–100% | one roll (`= splash %`), all-or-nothing: success = the caller's own base extra-target count; failure = 0 |
| 101–999% (overcap) | guaranteed — base + `splash_overcap_bonus_targets` |
| 1000%+ (ladder) | + `splash_ladder_targets_per_step` per full `splash_ladder_step_pct` of splash, uncapped |

**SUPPORT splash** (Radiant Smite heal, Relentless Flames, Cauterizing
Flames, Cleansing Flames' cleanse-count AND buff-refresh — effects
whose targeting IS the splash count, no separate primary):

Identical table, EXCEPT a miss/0% falls back to `splash_support_floor_targets`
(default 1) instead of 0 — these four **never do nothing**. This is the
one deliberate difference between the two shapes; both floors live
inside `roll_splash` itself so the distinction is structural, not
per-site drift.

## Every caller keeps its own base (Option A — additive, caller-neutral)

`roll_splash(splash_fraction, base_extra_targets, floor_targets, tunables, rng)`
never overwrites a caller's own target count with a flat tunable value —
it only decides the roll/floor outcome and the overcap/ladder addition,
both layered ON TOP of whatever `base_extra_targets` the caller passed
in. Concretely, unchanged from before this redesign:

- Gelatinous Cube: `CUBE_SPLASH_MAX_TARGETS` (4) — "hits 5 random
  players" stays exactly that.
- The Dragon: `units.len()` — the full-party breath sweep stays a
  full-party sweep.
- Default enemy cleave: `ENEMY_SPLASH_MAX_TARGETS` (1).
- Volley/Chain-Lightning (player): `LiveTunables::splash_extra_targets` +
  `extra_splash_targets` (Storm of Arrows/Wider Burst/Stormcaller's own
  additive bonus) — those three passives stay fully effective.
- Radiant Smite heal: `HEAL_SPLASH_MAX_TARGETS` + `smite_extra_targets`
  (Zealotry's own bonus) — same.

Cube and the Dragon force `splash_fraction` to exactly `1.0` — a
guaranteed roll (no RNG draw at all, so their own splash decision never
perturbs the RNG sequence) that never crosses into the `> 1.0`
overcap/ladder branch either. Their neutrality under this redesign holds
**by construction**, not by coincidence — see `roll_splash`'s own doc.

## The one deliberate non-`roll_splash` exception

The Volley/Chain-Lightning damage-bonus SIZING line (inside
`simulate_battle`, sets `splash_target_dmg_bonus`) sizes a flat PASSIVE
damage bonus, not a real target pick — there is no discrete splash event
to roll for, so it deliberately does not call `roll_splash`'s chance
gate. It still inherits the identical overcap/ladder COUNT formula via
the shared `splash_overcap_target_count` helper (factored out
specifically so this line can never drift from `roll_splash`'s own
math), and it includes `extra_splash_targets` in its own base, matching
the real `apply_splash` call it's pricing.

## Retired constants kept for the wiki module only

`PLAYER_SPLASH_MAX_TARGETS`, `HEAL_SPLASH_MAX_TARGETS`,
`SPLASH_OVERFLOW_BONUS_TARGETS` are no longer read by any live combat
code — the real numbers are the `LiveTunables` fields below — but stay
defined at their old values because `adventure_web/wiki.rs`'s
placeholder substitution still reads them by name, and that file is out
of scope for this branch (parallel wiki-overhaul session's workspace).
See `WIKI_IMPACT.md`. `ENEMY_SPLASH_MAX_TARGETS`/`CUBE_SPLASH_MAX_TARGETS`
are NOT stale — those two stay live-read as each boss's own base.

## Tunables (`/admin/tunables`, "Splash" section)

- `splash_extra_targets` (default 2) — player-side base extra targets on
  a successful roll.
- `splash_support_floor_targets` (default 1) — the four support sites'
  floor on a miss/0%.
- `splash_overcap_bonus_targets` (default 1) — added to the caller's
  base once splash exceeds 100%.
- `splash_ladder_step_pct` (default 1000) / `splash_ladder_targets_per_step`
  (default 1) — the 1000%+ ladder. `splash_ladder_step_pct = 0` disables
  the ladder term entirely.
- `splash_damage_pct` (default 1.0) — fraction of the primary hit/heal
  each ATTACK splash target takes; irrelevant to the four support sites
  (they apply their own already-full-value effect regardless).

## Decisions log

1. **Original premise was factually wrong on both halves** (fit report,
   round 1) — splash was never a chance, and overcap already granted +2
   guaranteed targets via a hardcoded constant. Reported and stopped per
   explicit instruction before building anything.
2. **Full redesign ruling** (round 2) superseded the original request:
   splash becomes a chance, base 2 targets at FULL damage on success,
   overcap = +1 (not the old +2), a documented-only 1000%+ ladder,
   centralize via one shared function, boss splash inherits the
   redesign (`apply_splash` is shared with boss cleave/Cube/Dragon).
3. **Addendum 1**: wiki-constant handling (keep, don't delete, flag via
   `WIKI_IMPACT.md`, following the pre-existing `pierce_cap`/`pierce_h`
   pattern); boss inheritance confirmed intentional; the Volley/
   Chain-Lightning sizing line's exception (inherits the count formula,
   skips the chance-roll) accepted as a judgment call.
4. **Option B, literal** (an intermediate ruling, since superseded for
   the four support sites — kept here for history): the chance-gate
   applied uniformly to all 6 sites, no floor, no exception — a 0%-splash
   character got 0 extra targets everywhere, including the 4 support
   sites that had previously been unconditional.
5. **FINAL SPLASH TABLE**, which is what's actually built: introduced
   the two-floor structure (attack = 0, support = the tunable floor) and
   the 1000%+ ladder, both superseding item 4's "no floor, no
   exception" and the earlier "document only, don't build" ladder
   deferral respectively.
6. **Option A, additive/caller-neutral** — a fit-report stop mid-build:
   the FINAL TABLE's flat "2 additional targets on success" language
   would have silently collapsed Cube's 4, the Dragon's full-party
   sweep, Storm of Arrows/Wider Burst/Stormcaller's bonus, and Zealotry's
   bonus onto one flat number. Owner ruling: every caller keeps its own
   base; only the roll/floor/overcap/ladder layer is shared. This is the
   shipped design.

## Verification

- `cargo build --release --workspace --target-dir target-splash-overcap`
  (`--workspace` required; a separate target dir is mandatory —
  `target/release/` holds live, file-locked production binaries).
- `cargo test --workspace --target-dir target-splash-overcap`.
- **No `cargo fmt`.**
- Golden-corpus fixtures are neither regenerated nor deleted — report
  divergence, regen happens at merge time. One fixture diverged this
  pass (`ranger_vs_lich_stage3000`) — plausibly the Volley sizing fix
  (previously omitted `extra_splash_targets`) and/or the attack-splash
  chance/full-damage change acting on that character's own gear splash.
- New tests: `roll_splash`'s own boundary table (0/1/55/100/101/999/
  1000/1999/2000/3000%, both floor categories, RNG-non-consumption on a
  guaranteed roll, seeded determinism, every tunable overridden
  end-to-end), per-caller base preservation (Cube, default cleave,
  Volley+Storm of Arrows, Radiant Smite+Zealotry), each of the four
  newly-gated support sites exercised at 0% and guaranteed splash, one
  boss-side splash consumer, and a real end-to-end HTTP test through the
  actual `Form<TunablesForm>` extractor for the new `/admin/tunables`
  fields (`tests/admin_tunables_splash_http.rs`).
