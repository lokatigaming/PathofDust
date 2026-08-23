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
