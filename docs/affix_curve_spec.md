# Affix Tier Curve

**Authority document for the affix tier rebalance.** Owner-ratified.
Committed here rather than left in chat history so a fresh session with
no memory of the planning conversation can implement it correctly. Read
this file in full before touching `affix_base_value`. If an
implementation needs to deviate from anything here, document why in the
commit message and add a numbered entry to the Decisions log below.

Branch: `docs/affix-curve-spec` off `master`. **Docs only — no code
changes ship on this branch.** The curve below is not implemented; this
file records what was ratified, so that the implementation pass has a
single source of truth to build against.

---

## The curve

```
f(T) = sqrt(T)                    for T <= 100
f(T) = 10 * (T / 100)^0.289       for T > 100
```

It replaces the bare `per_tier * tier` in `affix_base_value`
(`game/src/adventure/affix.rs:385-387`):

```rust
// today
pub(crate) fn affix_base_value(affix: Affix, tier: u32) -> f64 {
    affix_balance(affix).0 * tier as f64
}
```

**Per-affix `per_tier` coefficients are unchanged.** Nothing in the
`affix_def` table (`affix.rs:211-275`) moves, and nothing in
`adventure-item-balance.toml` moves. The entire change is the tier
term. Every affix keeps its relative weight against every other affix at
every tier — the ratio between `CritMultiplier` and `DivineDamage` is
2.22 today at every tier and stays 2.22 at every tier under the curve.

---

## 1. Anchors, and why each one is where it is

| Anchor | Value | Why |
|---|---|---|
| `f(1) = 1` | exact | Every T=1 starting value is **identical to today**. A new character's first drop reads exactly as it always has; nothing about the opening experience changes. |
| `f(100) = 10` | exact | T=100 lands on **today's T=10 values**. This is the deliberate compression point: the first hundred tiers now deliver what the first ten used to. |
| exponent `0.289` | — | Makes the first 1,000-tier step **exactly a doubling**: `f(1100)/f(100) = 11^0.289 = 2`. |
| continuity at T=100 | exact | `sqrt(100) = 10` and `10 * (100/100)^0.289 = 10`. The two halves meet with no seam — no discontinuity, no jump, no special-case at the boundary. |

Verified: `sqrt(100) = 10.000000` and `10 * 1^0.289 = 10.0000000000`.
The halves are continuous in value. They are **not** continuous in
first derivative (slope 0.05 approaching from the left, 0.0289 leaving
to the right) — that is intended and harmless, since nothing in the game
reads the derivative of this function.

### Why sqrt below 100

The sub-100 half is where a fresh character actually lives, and `sqrt`
is the mildest curve that still bends: it holds T=1 exactly, and by T=100
has only cut to one-tenth. Anything steeper would make the first hour
feel worse than today's game; anything shallower would leave the
compression point too high to matter.

### Why 0.289 and not ln2/ln11

The exponent that yields an *exactly* exact doubling is
`ln(2)/ln(11) = 0.2890648263`. The ratified constant is the rounded
`0.289`, which gives `11^0.289 = 1.99968913` — short of a true doubling
by **0.0155%**.

That rounding is ratified and should be implemented as written. The
divergence between the two never becomes material:

| T | `0.289` | `ln2/ln11` | divergence |
|---|---|---|---|
| 100 | 10.0000 | 10.0000 | 0.000% |
| 1,000 | 19.4536 | 19.4565 | 0.015% |
| 1,300 | 20.9860 | 20.9895 | 0.017% |
| 10,000 | 37.8443 | 37.8556 | 0.030% |
| 100,000 | 73.6207 | 73.6537 | 0.045% |

A literal `0.289` is also readable in the source and reproducible by
hand, which an implementer checking the curve against this document will
want. Use `0.289`.

---

## 2. Growth decay

Per-1,000-tier growth from any tier T is:

```
f(T + 1000) / f(T) = (1 + 1000/T)^0.289
```

This is a pure function of T — it does not depend on the per-affix
coefficient — so it applies identically to every affix in the game.

| T | growth over the next 1,000 tiers | f(T) → f(T+1000) |
|---|---|---|
| 100 | **2.00x** | 10.00 → 20.00 |
| 200 | 1.68x | 12.22 → 20.51 |
| 300 | 1.53x | 13.74 → 20.99 |
| 400 | 1.44x | 14.93 → 21.44 |
| 500 | **1.37x** | 15.92 → 21.87 |
| 700 | 1.29x | 17.55 → 22.68 |
| 1,000 | 1.22x | 19.45 → 23.77 |
| 1,300 | **1.18x** | 20.99 → 24.75 |
| 2,000 | 1.12x | 23.77 → 26.72 |
| 3,000 | 1.09x | 26.72 → 29.04 |
| 5,000 | 1.05x | 30.97 → 32.65 |
| 10,000 | **1.03x** | 37.84 → 38.90 |

Each thousand tiers is worth strictly less than the thousand before it,
forever, with no floor and no cliff. The curve never stops rewarding
tier — it just stops rewarding it much.

> **Correction to the ordering figures.** The order gave the T=500 decay
> as 1.42x. The computed value is **1.37x**; 1.42x is the decay at
> T ≈ 423, not T = 500. The order also gave T=1,300 as 1.19x; the
> computed value is 1.1793, which rounds to **1.18x**. The T=100 (2.00x)
> and T=10,000 (1.03x) figures are correct as given. The curve itself is
> unaffected — only these two reported sample points were off. Table
> above is computed, not transcribed.

### Sublinear at every tier; never crosses back

`f(T) < T` for all `T > 1`, and the ratio `f(T)/T` is **monotonically
decreasing** across the entire domain:

| T | 1 | 2 | 10 | 50 | 100 | 101 | 500 | 1,000 | 10,000 | 100,000 | 1,000,000 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `f(T)/T` | 1.0 | 0.707 | 0.316 | 0.141 | 0.100 | 0.0993 | 0.0318 | 0.0195 | 0.00378 | 0.000736 | 0.000143 |

For `T > 100` the ratio is `2.6423 * T^(-0.711)` — strictly decreasing
in T, with no root. **The curve can never cross back over the old linear
line at any tier, at any point in the game's future.** This is a
structural property, not a property of the sampled range.

### Rejected alternative: doubling at fixed intervals

A curve that doubles every fixed number of tiers — `h(T) = 10 * 2^((T-100)/1000)`,
matched to the same `f(100) = 10` anchor — was considered and
**rejected**, because it is exponential, not sublinear. It grows slower
than linear only at first and then overtakes it:

| T | `h(T)` | vs linear T |
|---|---|---|
| 5,000 | 299 | below |
| 9,000 | 4,777 | below |
| 10,000 | 9,554 | below |
| **10,077** | **10,077** | **crossover** |
| 10,400 | 12,607 | **above** |
| 11,000 | 19,109 | **above** |

Past T ≈ 10,077 a fixed-interval doubling curve is *worse than doing
nothing* — it hands back everything the rebalance bought and then
compounds past it. Given that live equipped tiers already run 1,200–1,800
and grow at +1 per craft action with no cap, T = 10,000 is a reachable
number, not a hypothetical one. Rejected.

> The order gave the crossover as "near T=10,400". The computed
> crossover is **T ≈ 10,077** (`h(10,050) = 9,891`, below;
> `h(10,100) = 10,240`, above). The rejection stands either way — the
> figure is recorded here corrected so nobody re-derives it and thinks
> the document is wrong.

---

## 3. Effect on effective power

Party DPS was measured at **T^4.95** — see the affix scaling analysis
that preceded this spec. That exponent is the product of six
independently-linear layers (weapon flat damage, the shared INCREASED
bucket, uncapped crit multiplier, gloves attack rate, Echo repeats,
Splash target count), each of which is linear in tier and each of which
opens its own multiplicative factor.

Past T = 100 the curve makes the effective tier term `T^0.289`, so:

| | today | under the curve |
|---|---|---|
| Party DPS power law | **T^4.95** | **T^1.43** |
| Doubling item tier | **x30.9** | **x2.70** |
| 10x item tier | x89,000 | **x27.0** |

`0.289 * 4.95 = 1.4305`, and `2^1.4305 = 2.695`.

The exponent is cut by a factor of 3.46 — the same factor by which the
tier term itself is compressed. **No layer is removed and no affix is
capped.** All six multiplicative layers still exist and still compound;
they simply run on a tier term that grows as `T^0.289` instead of `T^1`.
This is deliberate: it preserves every build's identity and every
affix's relative worth, and changes only how fast the whole system
travels.

---

## 4. Ships with a FULL RESTART — what that removes

This curve ships as part of a **full restart with fresh characters.**
Every character begins at T=1, where `f(1) = 1` and the curve is exactly
today's value. Nothing is retrofitted onto anything.

That context removes four requirements that would otherwise be
mandatory. **They are recorded here anyway, because every one of them
becomes mandatory if this curve is ever applied to a live population
instead of a fresh one.**

### 4.1 Migration of stored affix values — NOT required

**Why it is not needed now:** no character carries a pre-curve item.
Every affix on every item is rolled fresh through the new
`affix_base_value`.

**Why it would be mandatory on a live population — and this is the
important part:** changing `affix_base_value` alone would be **very
nearly a no-op for existing gear.**

`affix_base_value` is called at *roll* time only. The two functions that
actually move a live item's affixes to a higher tier never call it:

- `Item::sync_tier_to` (`item.rs:439-459`) multiplies each stored value
  by `new_tier / old_tier` — an exact ratio rescale, deliberately so, to
  preserve the item's original jitter.
- `Character::roll_recombine` (`character.rs:1337-1345`) does the same
  with its own `a_ratio` / `b_ratio`.

Live equipped tiers run 1,200–1,800. The stage-derived drop tier at
stage 3,586 is 718. **Every high-tier live item reached its tier through
ratio rescaling, not through a fresh roll at that tier.** A curve applied
without a migration would therefore bite only below tier ~718 and leave
the entire top of the live population growing linearly forever — the
exact opposite of the intent.

A live application would need **both**:

1. A one-time migration rewriting every stored affix value to
   `per_tier * f(item.tier) * preserved_jitter`. Precedent for the shape:
   `migrate_crit_value_nerf` (`migrations.rs:67-73`) and
   `migrate_gloves_speed_rebalance` (`migrations.rs:99-113`) — note the
   latter is deliberately **unconditional** rather than `.max()`-gated,
   because a rebalance migration must be able to *lower* a value. Note
   also that it reapplies `PERFECT_QUALITY_MULT` by hand; a bare
   recompute silently shaves the 20% off a Perfect/Sacred item.
2. A change to `sync_tier_to` and `roll_recombine` so future tier growth
   re-derives against the curve instead of ratio-rescaling. Without this
   the migration is a one-time cut that immediately starts growing
   linearly again from wherever it landed.

### 4.2 Landing-stage calculation / mass stage reset — NOT required

**Why it is not needed now:** the world restarts at stage 0 alongside
the characters. There is no gap between party power and world stage to
close, because both start from nothing.

**Why it would be mandatory on a live population:** the curve is a
~50x cut to effective damage at live tiers
(`f(1300)/1300 = 0.0195`, raised to the 4.95 power against the six
compounding layers). Stage 3,586 content is tuned — via
`hp_pacing_mult`, `boss_power_mult`, and the baseline anchors — against
a party that no longer exists. Someone would have to compute the stage
at which the post-curve party actually lands and reset the world there,
or the first post-deploy boss fight is an unwinnable wall.

Relevant live state at time of writing: `hp_pacing_mult` is pinned at
its `6.0` ceiling (Controller A is saturated by a factor of ~4.0), and
`recent_boss_outcomes` is already 10 losses deep. There is no slack in
the pacing system to absorb a cut of this size.

### 4.3 Player compensation, respecs — NOT required

**Why it is not needed now:** nobody loses anything. There is no
"before" to compare against.

**Why it would be mandatory on a live population:** the cut is not
uniform across builds. It falls hardest on exactly the builds that
stacked the most tier-linear layers — a character with high
`CritMultiplier` *and* high `Splash` *and* high `Echo` loses far more
than one that concentrated in a single stat. That is an unannounced
retroactive re-tuning of build decisions players made under different
rules. Precedent exists for both compensation
(`adventure-kibukah-compensation-marker.json`) and free respecs
(`Character::free_passive_respecs`).

### 4.4 Quality% drift handling — NOT required

**Why it is not needed now:** every item is rolled under the curve, so
every displayed quality% is correct from birth.

**Why it would be mandatory on a live population:** `affix_quality_percent`
and `craft_affix_value_range` (`affix.rs:396-428`) recompute an item's
displayed roll quality **live** from `stored_value / affix_base_value(tier)`
rather than storing it. This is called out as a KNOWN SIDE EFFECT in
`affix_base_value`'s own doc comment. Changing the tier function
retroactively rewrites the displayed quality% of **every existing item
in the game** the moment the bot restarts.

The effect is cosmetic — combat math reads `value` directly — but it is
highly visible: every item's quality reading would jump, and with the
denominator now ~50x smaller at live tiers, essentially every existing
affix would clamp to the 1.15 jitter ceiling and display as a flat 100%.
Players would see every item they own become "perfect" overnight while
also hitting for a fiftieth as much.

---

## 5. Still required at implementation time

These are **not** waived by the restart. All four apply to the
implementation pass regardless.

### 5.1 Baseline anchors must be re-derived against the new curve

`pacing::defaults::BASELINE_STAGE_ANCHORS` / `BASELINE_HP_ANCHORS` /
`BASELINE_ATK_ANCHORS` (`pacing.rs:128-130`) were hand-authored against
the *linear* power curve. Their own doc comment states the reasoning
explicitly: *"party power has historically outrun the LINEAR stage curve
increasingly at higher stages... so the room the floor grants below the
organic curve widens with stage."*

Under `T^1.43` that premise no longer holds — party power will track the
stage curve very differently, and a floor shaped for runaway linear
growth will be wrong in both directions. Re-derive all three anchor
lists against the new curve before the restart goes live. They are
already dashboard-shapable live tunables, so this can be tuned after
launch, but the shipped defaults should not be the old ones.

### 5.2 All 17 golden fixtures regenerate at merge

`golden_corpus.rs:414-418` builds every scenario character with real
`generate_item(slot, stage, rng)`, which flows through `roll_affixes` →
`affix_base_value`. Changing the tier function changes every fight's
exact outcome, so **all 17 fixtures in `game/tests/fixtures/golden_corpus/`
will mismatch.**

Per BRANCH DISCIPLINE this is report-with-attributed-cause on the
feature branch; regeneration happens at merge, never on the branch.
Expected cause for every one of the 17: *"affix tier curve — every
rolled affix value changed."*

### 5.3 The rng draw count inside `roll_affixes` must stay identical

This is the trap that will waste a day if it is missed.

`roll_affixes` (`affix.rs:510-531`) draws in a fixed order: one
`gen_range(0.0..1.0)` for the affix count, then `weighted_affix_pick`'s
draws, then one `gen_range(0.85..1.15)` jitter per affix. The curve must
change **only the value computed from those draws** — never how many
draws happen, never their order, never their ranges.

If the draw count changes, every downstream fixture diverges for a
**second, unrelated reason**: the whole rng stream shifts, so every
subsequent decision in the fight (targeting, crit rolls, splash picks)
changes too. The 17 fixture diffs stop being readable as "affix values
moved" and become "everything moved," and there is then no way to
confirm the curve did what it was supposed to do.

Implement as `affix_balance(affix).0 * f(tier)` and nothing else.

### 5.4 This needs code — the TOML cannot do it

`adventure-item-balance.toml` can override a per-affix `per_tier`
coefficient and a per-slot `base_power` / `power_cap` (`affix.rs:305-372`,
`item.rs:930-956`). It **cannot** change the shape of the tier function —
there is no override hook for the `* tier as f64` term, and adding one is
out of scope here.

So the level-cut half of a rebalance is testable live with no deploy,
but this curve is not. It ships as a code change or it does not ship.

---

## 6. Computed affix table

`value = per_tier * f(T)`, one instance of the affix, before the
±15% jitter (`roll_affixes`) and before `PERFECT_QUALITY_MULT` (×1.20).
`per_tier` values are the live code defaults from `affix_def`
(`affix.rs:211-275`); every `[affixes.*]` field in
`adventure-item-balance.toml` is commented out, so those defaults are
the live values.

| Affix | per_tier | T=1 | T=10 | T=100 | T=1,000 | T=10,000 |
|---|---|---|---|---|---|---|
| `CritMultiplier` | 0.05 | 5% | 15.81% | 50% | 97.27% | 189.22% |
| `IncreasedDamage` | 0.03 | 3% | 9.49% | 30% | 58.36% | 113.53% |
| `Splash` | 0.03 | 3% | 9.49% | 30% | 58.36% | 113.53% |
| `IncreasedLife` | 0.03 | 3% | 9.49% | 30% | 58.36% | 113.53% |
| `ColdDamage` | 0.0225 | 2.25% | 7.12% | 22.5% | 43.77% | 85.15% |
| `FireDamage` | 0.0225 | 2.25% | 7.12% | 22.5% | 43.77% | 85.15% |
| `LightningDamage` | 0.0225 | 2.25% | 7.12% | 22.5% | 43.77% | 85.15% |
| `DivineDamage` | 0.0225 | 2.25% | 7.12% | 22.5% | 43.77% | 85.15% |
| `ChaosDamage` | 0.0225 | 2.25% | 7.12% | 22.5% | 43.77% | 85.15% |
| `DamageReduction` | 0.02 | 2% | 6.32% | 20% | 38.91% | 75.69% |
| `BlockChance` | 0.02 | 2% | 6.32% | 20% | 38.91% | 75.69% |
| `Evasion` | 0.016 | 1.6% | 5.06% | 16% | 31.13% | 60.55% |
| `CritChance` | 0.01 | 1% | 3.16% | 10% | 19.45% | 37.84% |
| `Intervene` | 0.01 | 1% | 3.16% | 10% | 19.45% | 37.84% |
| `Leech` | 0.001 | 0.1% | 0.31623% | 1% | 1.95% | 3.78% |
| `Echo` | 0.000125 | 0.0125% | 0.03953% | 0.125% | 0.24317% | 0.47305% |
| `FlatLife` *(raw hp, not a %)* | 5.0 | 5 | 15.81 | 50 | 97.27 | 189.22 |
| `LingeringEffect` *(retired, weight 0.0)* | 0.00025 | 0.025% | 0.07906% | 0.25% | 0.48634% | 0.94611% |
| **f(T)** | — | **1.0000** | **3.1623** | **10.0000** | **19.4536** | **37.8443** |
| **cut vs today** | — | **×1.000** | **×0.3162** | **×0.1000** | **×0.01945** | **×0.003784** |

`LingeringEffect` is retired (2026-08-21, roll weight 0.0, excluded from
`ALL_AFFIXES`) and is listed only because `affix_def` still carries its
arm. It is never rolled, so the curve never touches it.

### What the curve does and does not change about the caps

Every hard cap and threshold in the system is expressed as an absolute
fraction and is **unchanged by this curve** — only the tier at which each
is reached moves.

**The tier at which a threshold is reached does NOT scale the way
intuition suggests.** Because the curve is sublinear, stacking an affix
across six instances divides `f(T)` by 6 — but that divides *T* by
`6^(1/0.289) = 6^3.46 ≈ 511`, not by 6. Investment is worth far more,
in tier terms, under the curve than it is today. Both columns below are
computed:

| Threshold | today, 1 roll | curve, 1 roll | today, 6 instances | curve, 6 instances |
|---|---|---|---|---|
| `DamageReduction` 75% cap | T = 38 | T ≈ 9,689 | T = 7 | **T ≈ 39** |
| `BlockChance` 75% cap | T = 38 | T ≈ 9,689 | T = 7 | **T ≈ 39** |
| `Evasion` 75% cap | T = 47 | T ≈ 20,970 | T = 8 | **T ≈ 61** |
| `Intervene` 50% cap | T = 50 | T ≈ 26,217 | T = 9 | **T ≈ 69** |
| `Splash` 100% overcap | T = 34 | T ≈ 6,446 | T = 6 | **T ≈ 31** |
| `Echo` 100% (first repeat) | T = 8,000 | T ≈ 1.1e12 | T = 1,334 | **T ≈ 2.3e9** |

("6 instances" = the affix rolled on all five equipped slots plus a
sacred implicit — an achievable but heavily-invested build.)

Two consequences worth stating plainly, both **known consequences, not
ratified intents:**

**The four defensive caps stay reachable for invested builds.** A lone
`DamageReduction` roll no longer caps until ~T=9,689, but a six-instance
build still caps at T≈39 — barely later than today's T=7 in absolute
terms, and reached well inside normal progression. So
`defensive_overflow` (`character.rs:2875-2884`) — currently ~22.5% of the
whole gear-side increased-damage bucket — **remains a live damage source
for anyone who invests in it.** What the curve removes is the case where
one lucky roll caps a defensive stat and dumps the remainder into
offense; it now takes real investment across slots. That is arguably an
improvement in its own right, and it is not something this curve needed
to solve.

**`Echo` becomes structurally unreachable, and it is a MORE layer.**
Today a heavily-invested build reaches its first guaranteed repeat at
T≈1,334 — within live tier range. Under the curve that moves to T≈2.3e9,
which is not a number this game will ever see. Echo is one of the six
compounding layers, so **the curve does not merely slow that layer down;
it deletes it in practice.** The effective post-curve power law is
therefore likely to sit slightly *below* the T^1.43 in §3, which assumes
all six layers survive. This is flagged for the implementation pass to
measure rather than assume, and for the owner to rule on separately if
a permanently-inert Echo affix is not wanted.

---

## 7. CritMultiplier coefficient cut (owner ruling)

`Affix::CritMultiplier`'s `per_tier` coefficient is **halved across the
board, 0.05 → 0.025**. This applies everywhere the coefficient is read,
including the new Amulet base power in §8.

This is a change to the `affix_def` table (`affix.rs:225`) and is
therefore an explicit, ratified exception to Decision 3 ("per-affix
coefficients are not part of this change"). It is the only one.

### Why this affix

`CritMultiplier` is the single worst offender in the pre-curve table on
two counts at once:

1. **Highest coefficient of any affix.** 0.05/tier, ahead of
   `IncreasedDamage`/`Splash`/`IncreasedLife` at 0.03 and the five
   elementals at 0.0225.
2. **The only affix opening an uncapped MORE layer.** Every other
   large-coefficient affix either lands in a shared additive bucket
   (`IncreasedDamage`, the elementals — `combat_increased_damage`,
   `character.rs:3015-3072`) or has a brake somewhere. `CritMultiplier`
   feeds `combat_crit_multiplier` (`character.rs:3153-3157`), which is
   `BASE_CRIT_MULTIPLIER + sum + archetype`, floored at 1.0 and capped
   nowhere.

The asymmetry with `CritChance` is the specific reason this was
singled out. `crit_stack_bonus` (`combat.rs:3897-3900`) is:

```
(min(stacks, 1) + overcrit_curve(stacks - 1)) * (crit_multiplier - 1.0) * CRIT_BONUS_MULT
```

`overcrit_curve` (`combat.rs:3880-3882`) asymptotes at
`OVERCRIT_CURVE_A = 1.5`, so the left-hand bracket is hard-bounded at
2.5 no matter how high crit chance climbs — crit chance was already
given a real ceiling. But that bounded bracket then multiplies
`(crit_multiplier - 1.0)`, which has no ceiling at all. **The existing
brake is on the wrong half of the product.** Halving the coefficient
does not fix that asymmetry; it halves the rate at which the unbraked
half grows.

### Precedent

`migrate_crit_value_nerf` (`migrations.rs:67-73`) already halved stored
`CritChance` and `CritMultiplier` values once before, in place, exactly
1:1. Its doc records why halving the *stored value* rather than
recomputing from `affix_base_value` was correct there — it preserves
each item's original jitter exactly.

That precedent is informative but **not needed here**: this ships with
the fresh restart (§4), so no stored value exists to halve. It is
recorded because a live application of this cut would use exactly that
shape.

### Resulting values under the curve

`value = 0.025 * f(T)`, one instance, before jitter and
`PERFECT_QUALITY_MULT`:

| | T=1 | T=10 | T=100 | T=1,000 | T=10,000 |
|---|---|---|---|---|---|
| **new — 0.025 × f(T)** | **2.5%** | **7.91%** | **25%** | **48.63%** | **94.61%** |
| old coefficient, same curve — 0.05 × f(T) | 5% | 15.81% | 50% | 97.27% | 189.22% |
| today — 0.05 × T, no curve | 5% | 50% | 500% | 5,000% | 50,000% |

The combined effect against today at T=10,000 is a factor of **528**.
The curve does 264 of that; the halving does the remaining 2.

### Measured effect — and a caveat worth reading

On a build with average affix luck the halving is **very close to
inert**, because `combat_crit_multiplier` is dominated by its
`BASE_CRIT_MULTIPLIER = 2.0` baseline once the curve has compressed the
rolled contribution:

| T | crit mult @ 0.05 | @ 0.025 | DPS effect |
|---|---|---|---|
| 10 | 2.060 | 2.030 | ×0.999 |
| 100 | 2.191 | 2.095 | ×0.996 |
| 1,000 | 2.372 | 2.186 | ×0.989 |
| 20,000 | 2.883 | 2.442 | ×0.959 |
| 100,000 | 3.406 | 2.703 | ×0.917 |

**The curve, not the halving, is what actually disarms this affix.**
The halving's real target is the concentration build — the live
character carrying 83,370% crit multiplier across five slots plus a
sacred implicit — where it remains a true 2x cut. On an average build
it is worth under 1% until five figures of tier.

Recorded as a ratified ruling, not questioned. But if the intent was
"meaningfully reduce crit multiplier for a typical player," the curve
had already done that and the halving adds little; if the intent was
"close the concentration ceiling," it does that. Flagged so the
distinction is on record.

---

## 8. Four new gear slots

Four new equipment slots ship alongside the curve, as part of the fresh
restart, to offset the affix scarcity the curve creates. Each carries a
base-power implicit in the same fashion as the existing five —
`base_power_for_slot(slot) × tier-term × power_roll` — differing only in
which stat it grants.

| Slot | Base power grants | `per_tier` |
|---|---|---|
| `Ring1` | crit chance | 0.01 |
| `Ring2` | crit chance | 0.01 |
| `Amulet` | crit multiplier | **0.025** (post-cut, §7) |
| `Pants` | % increased life | 0.03 |

**Owner ruling on coefficients:** a slot's base power equals exactly
**one affix of that type at the item's tier.** The coefficients above
are therefore identical to the corresponding `affix_def` `per_tier`
values, with the Amulet using the post-cut 0.025. They are not to be
re-derived.

**The code can express "one affix's worth" exactly, with one caveat.**
Both `affix_base_value` and `compute_power` are
`coefficient × tier-term × roll`, so at equal roll the two are
byte-identical. The caveat is that the two roll ranges differ:

| | range | cite |
|---|---|---|
| affix jitter | `0.85 .. 1.15` | `affix.rs:527` |
| `POWER_ROLL_RANGE` | `0.85 .. 1.20` | `item.rs:31` |

An implicit can therefore roll **4.3% higher at the top end** than the
affix it is meant to equal (Amulet at T=100: affix max 28.75%, implicit
max 30.00%). Floors match exactly. This is a real, small deviation from
"exactly one affix's worth" — flagged, not decided. Either accept the
4.3%, or roll the four new slots' power against the affix jitter band
instead of `POWER_ROLL_RANGE`.

---

### 8.1 How base power works today, and what must change

**Today.** `compute_power` (`item.rs:967-974`) is the single source of
truth for every slot's primary stat:

```rust
pub(crate) fn compute_power(slot: EquipSlot, tier: u32, power_roll: f64) -> f64 {
    let raw = base_power_for_slot(slot) * tier as f64 * power_roll;
    match slot_power(slot).1 { Some(cap) => raw.min(cap), None => raw }
}
```

`base_power_for_slot` (`item.rs:883-905`) supplies the coefficient:
Weapon 12.0, Helm 4.0, Body 12.0, Gloves 0.009, Boots 13.0.
`default_slot_power_cap` (`item.rs:907-925`) returns **`None` for every
slot** — no cap exists anywhere, and its doc records why the one former
cap (Gloves, flat 0.55) was removed on 2026-08-16: it made the stat
"dead-stop scaling entirely once reached."

**Does the existing mechanism support a base power that grants a
percentage stat rather than a raw number? Yes — with no extension
needed.** `EquipSlot::Gloves` is already exactly that: its `power` is
0.009/tier and is read as an **attack-speed fraction** by
`attack_interval_ms` (`character.rs:2438`), and rendered as
`+{:.0}% speed` rather than a raw number (`adventure_web.rs:5843`).
Storage, generation, wear decay (`effective_power`, `item.rs:496-498`),
tier-sync and reforge all treat `power` as an opaque f64 and do not care
about its units.

So the generation side needs **only** four new `base_power_for_slot`
arms. What genuinely must be added is the **consumption** side — one
site per new slot, because nothing today reads a slot's power into a
`combat_*` aggregate as an affix-equivalent:

| Slot | Must be summed into | Where |
|---|---|---|
| `Ring1`, `Ring2` | `gear_total` in `combat_crit_chance` | `character.rs:3129-3146` |
| `Amulet` | `gear_total` in `combat_crit_multiplier` | `character.rs:3153-3157` |
| `Pants` | `gear_increased` in `combat_max_hp` | `character.rs:2385-2391` |

Adding each to the **existing `gear_total`** (rather than as a new
multiplicative layer) is what makes the implicit behave as exactly one
affix: it lands in the same additive pool `sum_affix` already feeds, and
the passive tree's `(1 + tree)` layer then multiplies the combined
result exactly as it does today. Any other placement would make the
implicit worth more or less than one affix, contradicting the ruling.

**Structural changes required beyond that:**

| # | Change | Note |
|---|---|---|
| 1 | 4 new `EquipSlot` variants | `item.rs:9-15`. `#[serde(rename_all = "lowercase")]` → wire strings `ring1`, `ring2`, `amulet`, `pants` |
| 2 | `EQUIP_SLOTS` 5 → 9 | `item.rs:864`. **This array's length is read by the drop roll — see 8.4** |
| 3 | 4 new `Character` fields | `character.rs:361-369`, plus arms in `equipped`, `equipped_mut`, `equip` (`character.rs:1126-1185`) |
| 4 | 4 arms in `base_power_for_slot` | `item.rs:883-905` |
| 5 | 4 arms in `noun_pool` | `generate_item_at_tier_with_roll`, `item.rs:1054+` — exhaustive match, compiler-caught |
| 6 | 4 arms in `item_stat_line` | `adventure_web.rs:5835-5844` — exhaustive match, compiler-caught. Follow the Gloves precedent: render as a percentage |
| 7 | 6 hardcoded 5-slot lists in `adventure_web.rs` | **Not compiler-caught — see 8.5** |

`Item::cooldown_ms` (`item.rs:468-472`) gives every non-Helm slot the
shared `default` curve. The four new slots inherit it and never read it
(only Helm and Boots have skill procs). Harmless; no change needed.

---

### 8.2 Base power at fresh-start tiers

`per_tier × f(T)`, at `power_roll = 1.0`. This is the table for
sanity-checking how early gear feels:

| Slot | per_tier | T=1 | T=25 | T=50 | T=100 | T=200 |
|---|---|---|---|---|---|---|
| `Ring1` | 0.01 | 1.00% | 5.00% | 7.07% | 10.00% | 12.22% |
| `Ring2` | 0.01 | 1.00% | 5.00% | 7.07% | 10.00% | 12.22% |
| `Amulet` | 0.025 | 2.50% | 12.50% | 17.68% | 25.00% | 30.54% |
| `Pants` | 0.03 | 3.00% | 15.00% | 21.21% | 30.00% | 36.65% |
| **f(T)** | — | **1.0000** | **5.0000** | **7.0711** | **10.0000** | **12.2179** |

At `power_roll = 1.20` (the `POWER_ROLL_RANGE` ceiling) multiply by 1.2;
at 0.85, by 0.85.

Read for feel: a fresh character with both rings and an amulet at T=50
carries roughly **+14% crit chance and +18% crit multiplier from
implicits alone**, before any rolled affix. At T=200 that is +24% and
+31%. Pants at T=200 is +37% max hp — a meaningful but not dominant
share of `combat_max_hp`'s `(1 + gear_increased)` term.

---

### 8.3 Ring1 / Ring2 identity, and the duplicate-unique guard

They need **distinct `EquipSlot` variants** — `Ring1` and `Ring2`, not
one `Ring` variant occupying two fields. Every slot-keyed mechanism in
the codebase is keyed by the enum value: `equipped(slot)`,
`equipped_mut(slot)`, `equip(item)` dispatching on `item.slot`, the
`SLOT_POWER` HashMap, and the drop roll's slot pick. A shared variant
would make "which ring is this" unrepresentable and would break
`equip()` outright — it dispatches purely on `item.slot` and would
overwrite whichever ring was already there.

**The duplicate-unique prevention from `fix/duplicate-unique-effects`
already covers "the same unique equipped in both ring slots" — by
construction, with no change required.** The validator is:

```rust
// character.rs:1160
pub(crate) fn has_conflicting_unique_affix_value(&self, unique: UniqueAffix, excluding_slot: EquipSlot) -> bool {
    EQUIP_SLOTS.iter().filter(|&&s| s != excluding_slot)
        .filter_map(|&s| self.equipped(s).as_ref())
        .any(|other| other.unique_affix == Some(unique))
}
```

It iterates `EQUIP_SLOTS` and excludes only the destination slot by
enum equality. With `Ring1 != Ring2` as distinct variants, equipping a
unique into `Ring2` while the same unique sits in `Ring1` finds
`Ring1 != Ring2`, sees the conflict, and blocks. Its own doc records
that it is "the ONE validator behind every mutation point that can
affect equipped uniques" — equip, receive, and both unique-granting
craft paths.

**Two prerequisites, both mandatory:**

1. `EQUIP_SLOTS` must actually contain all nine variants. The validator
   iterates that array, not the enum — a slot omitted from
   `EQUIP_SLOTS` is invisible to the guard *and* to `sum_affix`,
   `count_affix`, wear decay, and the repair-cost calculation. This is
   the highest-risk single line in the whole change.
2. `Ring1` and `Ring2` must be distinct variants, per above.

Nothing else in that fix needs to change.

---

### 8.4 Power impact across the fresh range

Modelled with the DPS model behind §3 (`combat_atk × crit EV ×
(1 + increased bucket) × hits/sec × echo × splash targets`), using
expected affix counts rather than a specific build: average 1.23 affixes
per drop, drawn from a 16.1-weight pool, so **0.0764 expected instances
of any given affix per equipped slot** — 0.382 across 5 slots, 0.688
across 9.

Net effect at `power_roll = 1.0`, curve applied:

| T | +4 slots alone | crit halving alone | **NET** |
|---|---|---|---|
| 1 | ×1.05 | ×1.000 | **×1.05** |
| 5 | ×1.12 | ×0.999 | **×1.12** |
| 10 | ×1.17 | ×0.999 | **×1.16** |
| 25 | ×1.26 | ×0.998 | **×1.25** |
| 50 | ×1.37 | ×0.998 | **×1.35** |
| 100 | ×1.53 | ×0.996 | **×1.49** |
| 150 | ×1.60 | ×0.995 | **×1.56** |
| 200 | ×1.65 | ×0.995 | **×1.60** |

**One number: ×1.31 net over T = 1 to 200** (geometric mean).

The compensation and the cut are **not** in balance — they are barely in
the same conversation. The four slots are worth ×1.05 to ×1.65 and
growing with tier; the crit halving is worth ×0.995 to ×1.000 and is
effectively invisible in this range for the reason given in §7. **The
new slots are a net buff of roughly 31% across the fresh-start range,
not an offset.** Whether that is the intended amount of compensation for
the curve's scarcity is an owner call; it is flagged, not adjusted.

---

### 8.5 The `compute_power` question — flagged, needs a ruling

The order states the new slots' base power "scales through the same f(T)
curve as every other tier-scaled value." Read literally, "every other
tier-scaled value" would include `compute_power` for the **existing**
five slots — Weapon's flat damage and Gloves' attack-speed fraction.
§1 of this document ratified the curve as replacing the tier term in
`affix_base_value` only.

**This is not a cosmetic ambiguity. It changes the outcome by a factor
of four, and one reading contradicts the already-ratified §3 target.**

Measured over the fresh range, full package (4 new slots + crit
halving), new-slot implicits always curved:

| `compute_power` ruling | T^ (25→200) | T^ (100→400) | doubling tier |
|---|---|---|---|
| **affix curve only — existing `compute_power` stays LINEAR** | **T^1.44** | T^1.79 | **×2.72** |
| affix **and** `compute_power` both curved | T^0.42 | T^0.39 | ×1.34 |
| *§3 ratified target* | *T^1.43* | — | *×2.70* |

**Leaving `compute_power` linear lands on the ratified target almost
exactly. Curving it as well overshoots by ~4× and produces a game where
doubling every item's tier is worth 34% more damage** — flat enough that
tier progression would likely read as broken.

The recommendation, for the owner to accept or reject: **curve
`affix_base_value` and the four new implicits; leave `compute_power` for
the existing five slots linear.** The four new slots use the curve
because their implicits are affix-equivalents by ruling, not because
they are slot powers.

**A second correction falls out of the same measurement, and it applies
to §3 regardless of this ruling.** §3 derives T^1.43 as `0.289 × 4.95`.
That multiplication is only valid in the asymptotic limit where every
compounding layer is already ≫ 1. It is not, post-curve: at T=1400 under
the curve the increased-damage bucket sits at ≈1.17, so `(1 + bucket)`
is ≈2.17 rather than the ≈77 it reaches under the linear curve. A layer
near 1 contributes almost nothing to the exponent. Direct measurement
over real windows:

| window | status quo | affix curve only | both curved |
|---|---|---|---|
| 25 → 200 | T^2.58 | T^1.33 | T^0.31 |
| 100 → 400 | T^3.55 | T^1.68 | T^0.28 |
| 700 → 1400 | T^4.33 | T^2.05 | T^0.37 |
| 2,000 → 4,000 | T^4.11 | T^1.75 | T^0.43 |

**§3's T^1.43 should be read as an order-of-magnitude target, not a
constant — the real exponent varies with tier window.** The affix-only
column brackets it (1.33–2.05) across the range the game will actually
occupy, which is a further reason to prefer that ruling. Measure at
implementation; do not assume.

---

### 8.5b Crit concentration

Every character now receives one CritMultiplier's worth from the Amulet
and two CritChance's worth from the rings — **guaranteed, not by luck.**
That is a structural change to how crit is acquired, separate from how
much of it exists.

**The implicit share is constant at every tier.** Both the implicit and
the rolled contribution scale with the same `f(T)`, so the ratio never
moves:

| | implicit | rolled (9 slots, expected) | **implicit share** |
|---|---|---|---|
| crit chance | `2 × 0.01 × f(T)` | `0.0069 × f(T)` | **74%** |
| crit multiplier | `1 × 0.025 × f(T)` | `0.0172 × f(T)` | **59%** |

Three-quarters of a typical character's crit chance and nearly
two-thirds of their crit multiplier now arrive automatically with a
filled ring/amulet slot, at every tier from 1 upward.

**Effect on the exponent — small, and upward.** Isolating the implicits
under the recommended affix-only ruling:

| window | 5 slots | 9 slots, no implicits | 9 slots + implicits |
|---|---|---|---|
| 25 → 200 | T^1.33 | T^1.39 | **T^1.44** |
| 100 → 400 | T^1.68 | T^1.73 | **T^1.79** |
| 700 → 1400 | T^2.04 | T^2.10 | **T^2.21** |

The implicits add roughly **+0.05 to +0.11** to the exponent. That is
not a rounding error — because they scale with `f(T)` rather than being
flat grants, they are a genuine additional scaling layer, and they push
in the *opposite* direction from the curve. Worth noting alongside §8.4:
the new slots are a growth buff, not only a level buff.

**Effect on build diversity — this is the real cost.** Crit stops being
a build *choice* and becomes a floor everyone stands on. Two knock-on
consequences:

1. A "crit build" is far less differentiated. The gap between someone
   who deliberately stacked crit and someone who ignored it narrows to
   the 26% / 41% that is still rolled.
2. **Rolling `CritChance` or `CritMultiplier` on gear becomes a
   comparatively worse outcome** than rolling almost anything else,
   because it tops up a stat the character already has a large
   guaranteed base in, while every other affix starts from zero. That is
   a real drop-quality change nobody asked for, and it is the strongest
   argument for reconsidering *which* stats the implicits grant.

**Does crit multiplier still need its own saturation curve? No — not
while the curve is in force.** Under `f(T)` with the halved coefficient,
the reachable ceiling is modest:

| T | crit chance | crit mult | overcrit bracket (cap 2.5) | crit EV |
|---|---|---|---|---|
| 100 | 31.9% | 2.42 | 0.32 | ×1.23 |
| 1,000 | 57.3% | 2.82 | 0.57 | ×1.52 |
| 10,000 | 106.7% | 3.60 | 1.09 | ×2.36 |
| 100,000 | 202.9% | 5.11 | 1.76 | ×4.61 |
| 1,000,000 | 389.9% | 8.04 | 2.12 | ×8.44 |

Crit multiplier reaches **8.04 at tier one million**, against the
**835** measured on live linear gear today. The curve has already done
what a saturation curve would have been for, and `overcrit_curve`'s
existing 2.5 bracket handles the other half. Adding a second asymptote
on top would be redundant and would make crit feel dead.

**Record this as conditional, not settled.** The conclusion holds
*because* `f(T)` is in force on `affix_base_value`. It does not survive
either of these:

- the §8.5 ruling going the other way in a manner that re-linearises
  crit's inputs;
- the curve later being softened or removed without revisiting crit.

If either happens, the §7 asymmetry returns immediately — a bounded
bracket multiplying an unbounded multiplier — and a real saturation
curve on `crit_multiplier` becomes necessary. Flagged rather than
decided.

---

### 8.6 Everything else this touches

**Drop tables — behaviour change, not just a count.** Seven sites pick a
slot as `EQUIP_SLOTS[rng.gen_range(0..EQUIP_SLOTS.len())]`
(`manager.rs:2736, 4877, 4894, 4998, 5081, 5450, 5493`). Extending
`EQUIP_SLOTS` to 9 automatically:

- dilutes each existing slot's drop share from 20% to **11.1%** (a 44%
  cut in how often a weapon drops);
- more than doubles the time to fill every slot once — coupon-collector
  expectation rises from **11.4 to 25.5** drops;
- **changes the `gen_range` bound, which shifts the rng stream** at all
  seven sites. See 8.7.

Whether the loot rate needs raising to compensate is an owner call. It
is not automatic, and it compounds with the affix scarcity the curve
already creates.

**Crafting and reforge.** Slot-agnostic throughout. `reforge_item`
(`character.rs:1998-2027`) and `roll_recombine` (`character.rs:1296+`)
key on `item.slot` only for `is_eligible_for_slot`, which returns `true`
for every affix on every slot today (`affix.rs:137-139`,
`eligible_slots: None` on all 17 arms). The new slots inherit the full
17-affix pool with no change. `craft_action_cost`'s per-tier surcharge
is slot-agnostic. Recombine requires same-slot inputs, so rings can only
recombine with rings — correct by construction, and `Ring1`/`Ring2`
being distinct variants means a Ring1 cannot recombine with a Ring2.
**Flag: is that intended?** It is the automatic consequence, not a
decision anyone made.

**Disenchant.** Fully slot-agnostic — `disenchant_multiplier`
(`item.rs:552+`) counts affixes and Perfect/Sacred state,
`meets_auto_disenchant_floor` (`item.rs:410-416`) reads tier/quality
only. No change. Note the second-order effect: nine slots means more
drops land in the bag and more get auto-disenchanted, raising dust
income relative to a five-slot game.

**Character sheet display order.** Six hardcoded five-element slot lists
in `adventure_web.rs` — lines **3155, 3903, 4019, 5128, 5236, 5748** —
none of which read `EQUIP_SLOTS`. **None are compiler-caught**; they
will silently render a nine-slot character as five slots, and the new
gear will be invisible on the dashboard while working correctly in
combat. Note 5748 uses a *different* order from the others
(Helm/Weapon/Gloves/Body/Boots). Converting all six to iterate
`EQUIP_SLOTS` is the safer fix and makes future slot additions
automatic; if display order must differ from `EQUIP_SLOTS` order, keep
one named ordering constant rather than six literals.

**Wiki — coordination required, do not edit.** `adventure_web/wiki.rs`
carries 5 `EquipSlot` references and belongs to the wiki session per
CLAUDE.md. The new variants will affect it. Per rule 3, the slot
addition is *additive* to shared helpers, but `EQUIP_SLOTS` changing
length is a behaviour change to a shared constant the wiki reads.
**Tell the owner before implementing so it can be sequenced with the
wiki session.** Do not touch that file.

**`WIKI_IMPACT.md`.** This branch is docs-only and ships no
player-facing change, so no line is appended here. The implementation
pass **must** append at minimum:

```
affix.rs:affix_base_value — tier term now f(T)=sqrt(T)/10*(T/100)^0.289 — affects crafting, passives, bosses
affix.rs:affix_def CritMultiplier — per_tier 0.05 -> 0.025 — affects crafting
item.rs:EQUIP_SLOTS — 4 new slots (ring1/ring2/amulet/pants), drop share per slot 20% -> 11.1% — affects crafting, commands
item.rs:base_power_for_slot — 4 new implicit stats granted by base power — affects crafting
```

**Golden corpus.** Already flagged in §5.2 for the curve. The slot
addition makes it strictly worse: see 8.7.

---

### 8.7 The rng hazard, restated for the slot change

§5.3 requires the draw count inside `roll_affixes` to stay identical.
**The slot addition breaks the equivalent guarantee one level up, and
there is no way to avoid it.**

`EQUIP_SLOTS.len()` going 5 → 9 changes `rng.gen_range(0..5)` to
`rng.gen_range(0..9)` at seven loot sites. That is a different draw from
the same generator state, so every subsequent value in the stream
diverges. Combined with the curve's own value changes, the 17 golden
fixtures will differ for **two independent reasons at once**.

Mitigation, for the implementation pass:

1. Land the curve and the crit cut **first**, on their own commit, and
   regenerate the fixtures at that merge. Diffs are then attributable to
   "affix values changed" alone.
2. Land the slot addition **second**, as its own commit, and regenerate
   again. Diffs are then attributable to "loot rng stream shifted."

Doing both in one commit produces 17 unreadable fixture diffs and no way
to confirm either change did what it was supposed to. Per BRANCH
DISCIPLINE, regeneration happens at merge in both cases, never on the
branch.

---

### 8.8 Cross-repo: the desktop companion

`C:\PathOfDust_Desktop-replay` — **separate repo, separate maintainer.
Read read-only for this report; not edited. Naming the risk only.**

**Its equipment rendering is not data-driven.** It hardcodes the
five-slot list in six places plus a five-key emoji map in three:

| File | What |
|---|---|
| `bag.html:655` | `const SLOTS = ['weapon','helm','body','gloves','boots']` |
| `builds.html:317` | same literal |
| `solver/advisor-core.mjs:416` | same literal |
| `item-codec.js:50` | same literal |
| `extension/shared/item-codec.js:50` | same literal (duplicate of the above) |
| `bag.html:215`, `character.html:116`, `extension/card.js:6` | `SLOT_EMOJI` — five keys |

Three distinct risks, in descending severity:

**1. The codec encodes slot as a positional index.** `item-codec.js`
`shrink()` does `SLOTS.indexOf(it.s)` and stores the integer. Two
consequences:

- **Appending** `ring1`/`ring2`/`amulet`/`pants` to that array is safe
  for old links, and until it is appended the code already degrades
  gracefully — `s < 0 ? it.s : s` falls back to storing the raw slot
  string. An unknown slot produces a longer payload, not a wrong one.
- **Inserting or reordering** silently decodes every previously-shared
  v2 item link to the *wrong slot*, with no error and no version bump.
  The payload format is `"<version>." + base64url(...)` and the version
  is chosen by size, not by schema (`item-codec.js:23-24, 92-95`).

This is the same silent-version-collision class already known in this
cross-repo pairing. **The mitigation to communicate: append only, never
reorder.**

**2. The solver models the damage formula independently.**
`solver/advisor-core.mjs` carries its own gear model
(`weaponPower`, `bodyPower`, `helmPower`, `helmCooldownMs`,
`attackSpeed` — line 59) and its own damage reconstruction
(line 271: `role.mult * (baseAtk[0] + baseAtk[1]*level) + role.flat + g.weaponPower`).
It will keep producing confident, wrong advice after the curve ships —
it has no knowledge of `f(T)`, of the halved crit coefficient, or of
four slots' worth of implicits. Line 456 also still assumes
*"Elemental affixes only fit weapon/helm,"* which has been false since
the 2026-08-19 widen — so it is already drifted.

**3. `LABELS` in the same codec is a positional affix-label array** and
already lacks `echo` (it still lists `lingering effect`). Same
append-only constraint applies.

**Recommended action: notify that repo's maintainer before the restart
ships, with the four new slot strings (`ring1`, `ring2`, `amulet`,
`pants` — `EquipSlot` is `#[serde(rename_all = "lowercase")]`) and the
append-only warning.** No code change is proposed here and none should
be made by this session.

---

## Decisions log

1. **Curve shape ratified as piecewise `sqrt` below T=100, power-0.289
   above.** Continuous at the boundary. Not a single smooth function —
   two halves, chosen for their anchors rather than for elegance.
2. **Exponent is the rounded `0.289`, not `ln2/ln11 = 0.2890648`.**
   Divergence peaks at 0.045% at T=100,000. Readability and hand-
   checkability win over exactness.
3. **Per-affix `per_tier` coefficients are explicitly NOT part of this
   change.** Relative affix weights are preserved exactly. Any
   reweighting (e.g. the five elemental types collectively out-sloping
   `IncreasedDamage` by 4.75x, or defensive overflow feeding the
   offensive bucket) is a separate decision and a separate document.
4. **Fixed-interval doubling rejected** — exponential, overtakes linear
   at T ≈ 10,077, which is a reachable tier given +1-per-craft-action
   tier growth.
5. **Ships with a full restart.** The four waived requirements in §4 are
   waived *by that context only* and are documented as mandatory for any
   live application.
6. **No caps added, no layers removed.** The curve changes the rate of
   travel, not the structure. All six compounding layers survive intact.
7. **Corrections to the ordering figures, recorded for the record:**
   T=500 growth decay is 1.37x, not 1.42x (1.42x lands at T ≈ 423);
   T=1,300 is 1.18x, not 1.19x; the rejected doubling curve's crossover
   is T ≈ 10,077, not 10,400. Every table in this document is computed,
   not transcribed. The curve, its anchors, and the T^1.43 / x2.70
   effect figures are all confirmed correct as ordered.
8. **OPEN — not ratified, surfaced by this document.** The curve pushes
   `Echo`'s first guaranteed repeat to T ≈ 2.3e9 even on a six-instance
   build, which removes one of the six compounding layers entirely
   rather than slowing it. See §6. Either accept Echo as a
   permanently-fractional affix, or give it a `per_tier` of its own
   under Decision 3's exemption. Needs an owner ruling before
   implementation.
9. **`Affix::CritMultiplier` per_tier halved, 0.05 → 0.025** (§7).
   Owner-ratified. The single explicit exception to Decision 3.
   Rationale: highest coefficient in the table AND the only affix
   opening an uncapped MORE layer, while `CritChance` already has
   `overcrit_curve` as an asymptote.
10. **Four new gear slots** — `Ring1`/`Ring2` (crit chance, 0.01),
    `Amulet` (crit multiplier, 0.025 post-cut), `Pants` (% increased
    life, 0.03) (§8). Owner-ratified. Each slot's base power equals
    exactly one affix of that type at the item's tier; coefficients are
    not to be re-derived. The existing `compute_power` mechanism
    supports a percentage-granting base power with no extension —
    `Gloves` is already precedent.
11. **OPEN — the `compute_power` question, and it is the important one**
    (§8.5). "Every other tier-scaled value" is ambiguous about whether
    the existing five slots' `compute_power` is curved too. Measured:
    leaving it linear gives T^1.44 (matching §3's ratified target);
    curving it too gives T^0.42, four times too flat. **Recommendation:
    leave `compute_power` linear.** Needs an owner ruling before
    implementation.
12. **CORRECTION to §3, independent of Decision 11.** `0.289 × 4.95` is
    valid only in the asymptotic limit where every compounding layer is
    ≫ 1, which is not true post-curve. Direct measurement gives T^1.33
    to T^2.05 across the tier windows the game will occupy. **§3's
    T^1.43 is a target, not a constant.** Measure at implementation.
13. **OPEN — implicit roll range** (§8). `POWER_ROLL_RANGE` is
    `0.85..1.20` while affix jitter is `0.85..1.15`, so an implicit
    tops out 4.3% above the affix it is defined to equal. Either accept
    or roll the new slots against the affix band. Needs a ruling.
14. **The new slots are a net buff, not an offset** (§8.4). Measured
    ×1.31 over T = 1–200. The crit halving contributes ×0.995–1.000 in
    that range and does not meaningfully offset anything until five
    figures of tier. Recorded as a consequence for the owner to accept
    or re-tune, not as a defect.
15. **Two-commit sequencing is mandatory at implementation** (§8.7).
    The curve and the slot addition each perturb the golden fixtures for
    an independent reason — the slot count changes `gen_range(0..N)` at
    seven loot sites and shifts the rng stream. Land them as separate
    commits with a fixture regeneration at each merge, or the 17 diffs
    become unattributable.
16. **Cross-repo: append-only** (§8.8). The desktop companion's item
    codec encodes slot as a positional index into a hardcoded
    five-element array. Appending the four new slot strings is safe and
    already degrades gracefully; reordering silently decodes every
    previously-shared v2 link to the wrong slot. Its maintainer needs
    notice before the restart ships. Not this session's repo to change.
