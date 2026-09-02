# Affix Tier Curve

> **RECOVERY NOTE — added 2026-09-02, not part of the original document.**
>
> This file was written 2026-08-23 by Lokati on the branch
> `docs/affix-curve-spec` (commits `8bdb30d`, `691f050`, `9d7ba14`,
> `e552f89`, `cc8da41`, `c9eeedf`, `4d3b147`). That branch was marked
> **"branch CLOSED"** in its final commit message and was never merged.
> The spec was therefore absent from master for ten days while the World 2
> build plan carried only a four-word stub pointing at it. Recovered to
> master 2026-09-02, **verbatim** — nothing below this note was
> summarised, condensed or modernised.
>
> **Still unbuilt as of 2026-09-02.** Verified against master `a2d75fa`:
> `EQUIP_SLOTS` is still `[EquipSlot; 5]` (`game/src/adventure/item.rs:907`) with
> no Ring, Amulet or Pants variant anywhere in the tree; `CritMultiplier`
> still carries `default_per_tier: 0.05` (`game/src/adventure/affix.rs:225`),
> so the §7 halving has not been applied; no affix-curve code exists; and
> no test under `game/tests/` references `base_power`, so R5 was never
> written. Line numbers cited throughout this document are as of the
> 2026-08-23 tree and may not resolve against current master.
>
> **The one open ruling in this document is now closed** — see §8.5.

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
already dashboard-shapable `LiveTunables`, so this **can** genuinely be
tuned after launch without a restart (see §10.7 — `LiveTunables` is the
hot config surface; `adventure-item-balance.toml` is not). The shipped
defaults should still not be the old ones.

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

So the level-cut half of a rebalance can be changed **without a
rebuild**, but this curve cannot. It ships as a code change or it does
not ship.

> **Corrected (§10, R7).** An earlier revision of this paragraph read
> "testable live with no deploy," which implies hot tuning. It is not
> hot: `adventure-item-balance.toml` is read through `OnceLock`s and
> **requires a process restart**. See §10.7 — the two config files in
> this system have opposite reload semantics, and the distinction is
> load-bearing.

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

> **RULED 2026-09-02 — CLOSED. `compute_power` STAYS LINEAR.**
> Added on recovery; not part of the original 2026-08-23 document, whose
> text follows unchanged below.
>
> The affix curve replaces the tier term in `affix_base_value` only.
> `compute_power` for the existing five slots — Weapon's flat damage and
> Gloves' attack-speed fraction — is **not** curved, and the four new
> slots' implicits follow the same rule.
>
> **The ruling rests on this document's own measurement**, re-read
> 2026-09-02 from the table below: leaving `compute_power` linear yields
> **T^1.44** over the fresh range against §3's ratified target of
> **T^1.43** — a match. Curving it too yields **T^0.42**, roughly four
> times too flat, a game in which doubling every item's tier is worth
> only ×1.34. The recommendation this section recorded was correct and is
> now adopted as the ruling.
>
> §8.5's own closing caution still stands and is not overridden: T^1.43
> is an order-of-magnitude target, not a constant, and the real exponent
> varies with the tier window. Measure at implementation; do not assume.

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

## 9. Echo — owner ruling on Decision 8

**Ruling: Echo does not ship as a dead affix, and this needs no code.**
Every `per_tier` is already overridable in
`adventure-item-balance.toml` (`affix.rs:305-372`,
`balance.rs:32-52`). Echo's coefficient is set there at world launch.

**Provisional value: `per_tier = 0.00857`**, derived so a six-instance
build reaches its first guaranteed repeat at T = 1,000:

```
6 × 0.00857 × f(1000) = 6 × 0.00857 × 19.4536 = 1.0003
```

The value is **confirmed** — see 9.2. What did not survive verification
is the premise behind it; see 9.4.

---

### 9.1 The problem, restated precisely

At the code default `per_tier = 0.000125`, the curve pushes Echo's first
guaranteed repeat to T ≈ 2.25e9 even on a six-instance build (§6). At the
tiers this game will ever occupy, Echo contributes:

| | T=100 | T=500 | T=1,000 | T=3,000 |
|---|---|---|---|---|
| 1 instance | 0.125% | 0.199% | 0.243% | 0.334% |
| 6 instances | 0.750% | 1.194% | 1.459% | 2.004% |

**Correction to my own §6 / Decision 8 wording.** I described this as the
curve "deleting the layer." That is the right conclusion but the wrong
mechanism, and the distinction matters for an authority document:
**there is no cliff at 100%.** `roll_echo` (`combat.rs:10148-10156`) is

```rust
let guaranteed = pct.floor() as u32;
let remainder  = pct - guaranteed as f64;
let succeeded  = remainder > 0.0 && rng.gen_bool(remainder.min(1.0));
(guaranteed + if succeeded { 1 } else { 0 }, remainder, succeeded)
```

so `E[repeats] = floor(pct) + remainder = pct` **exactly**, for every
`pct`. Echo below 100% is not inert — it is a linear-in-expectation
damage multiplier of `(1 + pct)`, continuously. The 100% mark is a
*readability landmark* ("you now always get one extra hit"), not a
mechanical threshold.

So the real problem is **magnitude, not death**: at 0.000125 the layer
is worth ×1.015 on a six-instance build at T=1,000, which rounds to
nothing. The fix is the same either way; the reasoning recorded for it
should be accurate.

---

### 9.2 Arithmetic verification

| | |
|---|---|
| `f(1000)` = `10 × 10^0.289` | **19.45360082** |
| `6 × 0.00857 × 19.45360082` | **1.00030415** |
| coefficient for *exactly* 1.0 — `1 / (6 × f(1000))` | **0.0085673942** |
| `0.00857` overshoots by | **0.0304%** |

**`0.00857` is confirmed, and the rounding direction is load-bearing.**
`0.00856` gives `6 × 0.00856 × 19.4536 = 0.999137`, which does **not**
reach 1.0 at T=1,000 — a six-instance build would sit one hair below its
first guaranteed repeat at exactly the tier the value was derived to
hit. Rounding up is required. Use `0.00857`, not the truncation.

(This is the same class of check as `0.289` vs `ln2/ln11` in §1, and
lands the same way: the rounded literal is correct and readable, the
deviation is under a tenth of a percent, and the direction was chosen
rather than accidental.)

**Scale.** `0.000125 → 0.00857` is a **×68.6 increase**. This is not a
tweak — it moves Echo from the smallest coefficient in the table to
11th of 13, between `Intervene` (0.01) and `Leech` (0.001):

| rank | affix | per_tier |
|---|---|---|
| 1–3 | `IncreasedDamage`, `Splash`, `IncreasedLife` | 0.03 |
| 4 | `CritMultiplier` (post-cut) | 0.025 |
| 5 | each elemental | 0.0225 |
| 6–7 | `DamageReduction`, `BlockChance` | 0.02 |
| 8 | `Evasion` | 0.016 |
| 9–10 | `CritChance`, `Intervene` | 0.01 |
| **11** | **`Echo` (new)** | **0.00857** |
| 12 | `Leech` | 0.001 |
| — | *`Echo` (old)* | *0.000125* |

That placement is defensible for a MORE layer, and arguably still
conservative: at six instances and T=1,000 Echo is worth ×2.00, while
six instances of `IncreasedDamage` at the same tier put 350% into the
shared bucket for ×4.50. Echo repeats splash along with the primary hit
(`Affix::Echo`'s doc), so the ×2.00 is a true pack-damage doubling, not
a single-target-only figure — and it is still less than the additive
affix it sits beside.

---

### 9.3 Echo's share of total damage

Share = `pct / (1 + pct)`, since the layer multiplies by `(1 + pct)`:

| T | f(T) | 1 instance | share | 6 instances | share |
|---|---|---|---|---|---|
| 100 | 10.0000 | 8.6% | **7.89%** | 51.4% | **33.96%** |
| 500 | 15.9222 | 13.6% | **12.01%** | 81.9% | **45.02%** |
| 1,000 | 19.4536 | 16.7% | **14.29%** | 100.0% | **50.01%** |
| 3,000 | 26.7232 | 22.9% | **18.63%** | 137.4% | **57.88%** |

Compare the same table at the old coefficient — 0.744% to 2.004% at six
instances across the whole range.

A six-instance Echo build at T=3,000 draws **58% of its damage from
Echo**. That is a large share for one affix, but it is the correct
consequence of a six-slot commitment to a single MORE layer, and it is
the same shape a six-instance `Splash` or `IncreasedDamage` build
produces. Flagged for visibility, not as an objection.

---

### 9.4 Exponent with Echo dead vs Echo at 0.00857 — and the contradiction

Measured under the §8.5 recommended ruling (affix curve only,
`compute_power` linear, 9 slots, `CritMultiplier` 0.025):

| window | Echo 0.000125 | Echo 0.00857 | delta |
|---|---|---|---|
| 25 → 200 | T^1.445 | T^1.464 | **+0.019** |
| 100 → 400 | T^1.791 | T^1.810 | +0.019 |
| 200 → 800 | T^2.003 | T^2.026 | +0.023 |
| 700 → 1,400 | T^2.205 | T^2.234 | +0.029 |
| 1,000 → 3,000 | T^2.289 | T^2.323 | +0.034 |

Doubling item tier over the 25→200 window: **×2.722 with Echo dead,
×2.759 with Echo at 0.00857.**

**The value is confirmed. The premise behind it is contradicted.**

The order states "T^1.43 assumed all six layers survive," implying
Echo's death was costing a meaningful share of the exponent. It was not.
**Reviving Echo moves the exponent by +0.019 to +0.034 — between 1% and
2%.** Both the dead and the revived figures sit on §3's T^1.43 target.

The reason is the same one §8.5 already established for §3 generally: a
layer's exponent contribution scales with how far above 1 it sits, not
with whether it exists. A layer at `(1 + 0.015)` and a layer at
`(1 + 1.0)` are both far below the `(1 + 76)` regime where a layer
contributes a full factor of T. Echo was never carrying a sixth of the
exponent under the curve — it was carrying about 2% of it.

**What this changes: nothing about the ruling, everything about its
justification.** Set Echo to 0.00857 because an affix that contributes
1.5% of damage on a full six-slot commitment is a dud that makes drops
feel bad — that is a real and sufficient reason. Do **not** set it
expecting to recover exponent; there is no exponent to recover, and if
the implementation pass measures against a T^1.43 target it will find
the target met either way. Decision 8 is amended accordingly.

---

### 9.5 Every threshold and breakpoint in the system

The order asks whether any non-obvious ones exist. **Yes — two, plus one
that matters more than Echo did.** Full sweep, computed:

| Affix | Threshold | 1 inst | 6 inst | today, 6 inst |
|---|---|---|---|---|
| `DamageReduction` | 75% hard cap | 9,689 | 39 | 6 |
| `BlockChance` | 75% hard cap | 9,689 | 39 | 6 |
| `Evasion` | 75% hard cap | 20,970 | 61 | 8 |
| `Intervene` | 50% hard cap (per-char **and** party pool) | 26,217 | 69 | 8 |
| `CritChance` | 200% = half the overcrit asymptote | 3,175,659 | 6,446 | 33 |
| `CritChance` | 1000% = 90% of the asymptote | 8.33e8 | 1,689,859 | 167 |
| `Splash` | 100% = overcap, guaranteed + bonus target | 6,446 | 31 | 6 |
| **`Splash`** | **1000% = first ladder rung (+1 target)** | **1.86e7** | **37,750** | **56** |
| `Echo` @ 0.00857 | 100% = 1st guaranteed repeat | 492,162 | **999** | — |
| *`Echo` @ 0.000125* | *100% = 1st guaranteed repeat* | *1.11e12* | *2.25e9* | *1,333* |
| **each elemental** | **1000% = proc chance clamps at 100%** | **5.03e7** | **102,147** | **74** |
| **`DivineDamage`** | **100% = 1st guaranteed heal-power self-buff stack** | **17,441** | **55** | **7** |

**Non-obvious #1 — `DivineDamage` has a second, Echo-shaped threshold.**
`roll_divine_heal_power_proc` (`combat.rs:7631-7645`) is the one
elemental proc that does **not** go through
`ELEMENTAL_PROC_CHANCE_DIVISOR`. It reads the rolled fraction *directly*
as a stack count — `guaranteed = raw_pct.floor()`, plus
`Bernoulli(remainder)` — byte-for-byte the same shape as `roll_echo`.
**Verdict: not a problem.** Six instances reach the first guaranteed
stack at T ≈ 55 (today: T ≈ 7), and like Echo it has no cliff —
`E[stacks] = raw_pct` continuously below 1.0. Solo is pushed to
T ≈ 17,441, which is far, but a Cleric/Druid investing in Divine will be
running multiple instances by design. No override needed.

**Non-obvious #2 — the elemental proc chance clamps at 1000%, not
100%.** `ELEMENTAL_PROC_CHANCE_DIVISOR = 10.0` (`combat.rs:5354`), and
`fire_damage_pct` etc. are the raw `sum_affix` fractions
(`combat.rs:2567-2577`, `combat.rs:11317-11325`), so
`chance = fraction / 10` clamped to 1.0 at `fraction = 10.0`. Today a
six-instance build clamps at T ≈ 74; under the curve, T ≈ 102,147.
**Verdict: not a problem, and not a death condition** — the clamp is a
*ceiling*, not a floor. Proc chance scales linearly from zero with no
threshold anywhere below it, so elementals degrade gracefully; they
simply stop reaching guaranteed-proc status. Arguably an improvement:
a stat that maxed out at tier 74 now has room to grow. Note also that
elementals feed the increased-damage bucket in parallel
(`character.rs:3015-3072`), which is uncapped, so they can never become
dead regardless.

**The one that matters more than Echo did — `Splash`'s ladder.** Splash
has two thresholds, and the curve treats them very differently. The
**100% overcap** (guaranteed + `splash_overcap_bonus_targets`) stays
reachable at T ≈ 31 on six instances. The **1000% ladder rung**
(`splash_overcap_target_count`, `combat.rs:10254-10262` — +1 target per
full 1000%, uncapped) moves from **T ≈ 56 today to T ≈ 37,750**. That is
not a landmark being pushed out; **it is an entire live mechanic
becoming structurally unreachable**, in exactly the way Decision 8
described for Echo — and unlike Echo, the splash ladder genuinely *is*
discrete, so there is no continuous fallback. Below the first rung the
ladder contributes literally zero extra targets, always.

**This is flagged, not decided.** It is the same class of problem the
Echo ruling just solved, it was not in the order, and it has the same
zero-code fix available: `[affixes.splash].per_tier`, or the
`splash_ladder_step_pct` live tunable (default 1000, `tunables.rs:413`),
which is a *separate* config surface and can be lowered without touching
the affix at all. Lowering the ladder step is the more surgical of the
two — it leaves Splash's 100% overcap behaviour exactly where it is.
**Needs an owner ruling before launch.**

Everything else was checked and has no tier-magnitude threshold:
`IncreasedDamage` (floored at −0.9, unreachable upward),
`IncreasedLife`/`FlatLife` (no cap in `combat_max_hp`), `Leech` (no cap
on the gear path; `combat.rs:3618`'s ceiling is on Slayer's archetype
bonus, not the affix), `CritMultiplier` (no threshold — crit *stacks*
come from crit chance). `ELEMENTAL_LIGHTNING_MAX_STACKS = 200` and
`ELEMENTAL_DIVINE_ENEMY_MAX_STACKS = 100` are caps on accumulated
in-fight stacks, reached through proc *frequency* over time, not through
affix magnitude — unaffected by the curve.

One non-affix breakpoint worth recording because it bears on Decision
11: the **50 ms `attack_interval_ms` floor** sits at T ≈ 2,500 if
`compute_power` stays linear, and at T ≈ 2.0e10 if it is curved. If the
existing five slots are curved, gloves lose their only brake — one more
argument for leaving `compute_power` linear.

---

### 9.6 The launch configuration, as a single reviewed list

`adventure-item-balance.toml`, to be set once before the world opens.
Keys are `Affix`'s camelCase serde names and `EquipSlot`'s lowercase
serde names (`balance.rs:32-52`).

```toml
# --- ratified overrides, world launch ---

[affixes.echo]
per_tier = 0.00857          # sec 9. 6 instances reach 1 guaranteed repeat at T=1000.
                            # Do NOT round to 0.00856 - it misses the anchor.

[affixes.critMultiplier]
per_tier = 0.025            # sec 7. See the coupling warning below.

[slot_base_power]
ring1  = 0.01               # MUST equal [affixes.critChance].per_tier
ring2  = 0.01               # MUST equal [affixes.critChance].per_tier
amulet = 0.025              # MUST equal [affixes.critMultiplier].per_tier
pants  = 0.03               # MUST equal [affixes.increasedLife].per_tier
```

Nothing else needs setting. Deliberately left at code defaults:
`[slot_power_cap]` (no caps — `default_slot_power_cap` returns `None`
for every slot, and the 2026-08-16 removal of the Gloves cap is on
record as deliberate), `[cooldown.*]` (the four new slots inherit the
`default` curve and never read it), `[craft_action_cost]`, and every
other affix's `per_tier` and `weight`.

**Four things about this file that will bite if not recorded:**

**1. The base-power coupling is an invariant that nothing enforces.**
§8's ruling — "a slot's base power equals exactly one affix of that type"
— spans two independent TOML sections. `[affixes.critMultiplier]` and
`[slot_base_power].amulet` must move together, always; likewise
`critChance` ↔ both rings and `increasedLife` ↔ pants. There is no code
check, no test, and no warning if they drift. **Anyone tuning a crit
number in this file must edit two keys.** A test asserting the four
pairings would be cheap insurance and is recommended for the
implementation pass.

**2. Decide crit's home: code or TOML.** The §7 halving is a ratified
permanent design change, not a launch knob — it belongs in
`affix_def` (`affix.rs:225`) as the new code default, with no TOML entry
at all. Echo's 0.00857 is explicitly a config value by this ruling and
belongs in TOML. Putting the crit cut in TOML instead works identically
but makes the code default a lie and doubles the number of places a
future reader has to check. **Recommendation: crit in code, Echo in
TOML.** If crit goes in code, delete the `[affixes.critMultiplier]`
block above and keep only `[slot_base_power].amulet = 0.025`.

**3. "No deploy" is right; "no restart" is not.** `AFFIX_BALANCE` and
`SLOT_POWER` are both `OnceLock`s (`affix.rs:303`, `item.rs:930`),
initialised lazily on first read and never re-read. A TOML edit needs a
**process restart** to take effect — no rebuild, no deploy, but not hot.

**4. Strip the retired `[affixes.lingeringEffect]` header.** It is
sitting in the live file today. It is harmless *now* — `affix_balance`
gained an explicit guard for exactly this case — but that guard exists
because this precise situation **panicked every request in production
for ~2.5 minutes before rollback on 2026-08-21** (`affix.rs:318-331`).
A fresh world should not carry the header that caused it. Start the new
file clean.

---

## 10. Ratified rulings (R1–R8)

Eight owner rulings, each recorded as ratified. Where a ruling carries a
number, the number was independently verified; where it carries
reasoning, the reasoning is preserved so a later reader can tell what
the decision was *for*.

---

### 10.1 R1 — Echo `per_tier = 0.00857`, rounding UP

**RATIFIED.** `0.00856` gives `6 × 0.00856 × f(1000) = 0.999137` and
misses the anchor the value was derived to hit — a six-instance build
would sit one hair below its first guaranteed repeat at exactly the tier
chosen for it. `0.00857` gives `1.00030415`.

Same discipline as `0.289` over `ln2/ln11` (§1): prefer the short
readable literal, verify which side of the anchor it lands on, and
choose the rounding direction deliberately rather than by default. In
both cases the deviation is under a tenth of a percent and the direction
is the whole point.

**Note the mirror image in R3.** Echo's coefficient rounds *up*; the
splash ladder step rounds *down*. Same underlying reason: both anchors
are `floor()` thresholds, and the derived quantity sits on opposite
sides of the division. Rule of thumb worth carrying — **when a value is
derived to hit a `floor()` boundary, round in whichever direction clears
the boundary, and state which direction that was.**

---

### 10.2 R2 — the acceptance test for Echo

**RATIFIED, and recorded here specifically so the implementation pass
cannot pick the wrong criterion.**

**The acceptance test for the Echo change is NOT "exponent recovered."**
That test passes whether or not the change ships: measured, Echo's
revival moves the exponent by +0.019 to +0.034 — roughly 1–2% — and both
the dead and the revived figures sit on §3's T^1.43 target (§9.4). An
implementation pass measuring exponent will conclude either that the
change was unnecessary, or that it worked when it was never the
mechanism.

**The justification is that a 1.5%-of-damage affix on a full six-slot
commitment is a dud.** At the code default, six instances of Echo at
T=1,000 are worth ×1.015. A player who rolls Echo on every slot has
committed an entire gear set to a stat that does approximately nothing,
and every Echo drop reads as a wasted roll. That is a drop-quality and
player-experience problem, and it is a complete and sufficient reason on
its own.

**The correct acceptance test** is the damage-share table in §9.3: at
`0.00857`, a six-instance build draws **33.96% / 45.02% / 50.01% /
57.88%** of its damage from Echo at T = 100 / 500 / 1,000 / 3,000.
Verify those. Do not verify the exponent.

---

### 10.3 R3 — splash ladder via `splash_ladder_step_pct = 350`

**RATIFIED. Value verified; two qualifications and one side effect
recorded below.**

The fix goes through the `splash_ladder_step_pct` `LiveTunable`
(`tunables.rs:413`, default 1000), **not** `[affixes.splash].per_tier`.
This is the surgical option: it leaves Splash's 100% overcap behaviour —
the chance-to-guaranteed transition and `splash_overcap_bonus_targets` —
exactly where it is, and touches only rung spacing.

**Arithmetic.**

| | |
|---|---|
| `f(1000)` | 19.45360082 |
| `6 × 0.03 × f(1000)` | 3.50164815 → **350.1648%** |
| `floor(350.1648 / 350)` | **1 rung** ✓ |
| largest integer step still giving 1 rung at T=1,000 | **350** |
| step 351 → `floor(350.1648 / 351)` | **0 rungs — misses** |

**Rounding DOWN is required**, the mirror image of R1. `350` is the
largest integer clearing the anchor. Value confirmed.

**Rungs by instance count and tier, at step 350:**

| instances | T=100 | T=500 | T=1,000 | T=3,000 | T=10,000 |
|---|---|---|---|---|---|
| 1 | — (30%) | — (48%) | — (58%) | — (80%) | 0 rungs (114%) |
| 2 | — (60%) | — (96%) | 0 rungs (117%) | 0 rungs (160%) | 0 rungs (227%) |
| 4 | 0 rungs (120%) | 0 rungs (191%) | 0 rungs (233%) | 0 rungs (321%) | **1 rung** (454%) |
| 6 | 0 rungs (180%) | 0 rungs (287%) | **1 rung** (350%) | **1 rung** (481%) | **1 rung** (681%) |

"—" means the splash fraction is ≤ 1.0, so `roll_splash` never enters
the overcap branch at all — splash is still a chance roll there and no
ladder is reachable by construction.

**Qualification 1 — post-curve the "ladder" is one step, not a ladder.**
For a six-instance build the rungs land at:

| rung | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|
| T at step 350 | **998** | 10,988 | 44,692 | 120,933 | 261,742 |
| T at old step 1000 | 37,750 | 415,468 | — | — | — |

Step 350 brings the first rung from T ≈ 37,750 to T ≈ 998 — a ~37×
improvement in reachability, which is the point. But rung two still sits
at T ≈ 10,988. In any realistic tier range this is **a single +1 target,
once.** That is the sublinear curve behaving correctly and is not an
argument against the value; it *is* an argument for not calling the
mechanic a "ladder" in the wiki or patch notes, because players will
look for rung two and not find it.

**Qualification 2 — it is a 4+ instance mechanic only.** A 1-instance
build does not reach overcap until T = 10,000, and a 2-instance build
never reaches a rung anywhere up to T = 10,000 (peaking at 227% against
a 350% requirement). Only 4- and 6-instance builds ever see a rung.
Whether the ladder should be reachable on lighter investment is an owner
call; flagged, not adjusted.

**Side effect, and it is a buff.** `splash_overcap_target_count` has a
**second consumer**: Ranger's Volley / Chain Lightning damage-bonus
sizing line (`combat.rs:14307`), which deliberately reuses the same
count formula so it can never drift from `roll_splash`'s overcap math.
Lowering the step raises `max_targets_reachable`, and
`splash_target_dmg_bonus = volley_per_target × max_targets_reachable`
scales directly with it. For a six-instance splash build at T=1,000 the
reachable count goes 4 → 5 — **a +25% bump to that passive.** The
coupling is intended by construction (the shared helper exists precisely
so the two stay in step), but it is not neutral and belongs in the patch
note.

**Enemies are unaffected, by construction.** Gelatinous Cube's splash
and the Dragon's full-party sweep force `splash_fraction` to exactly
`1.0`, and `roll_splash` gates the overcap branch on
`splash_fraction > 1.0` — strictly. `1.0` is not `> 1.0`, so neither
reaches the ladder at any step value.

---

### 10.4 R4 — the crit multiplier halving goes in CODE

**RATIFIED.** `Affix::CritMultiplier`'s `per_tier` becomes `0.025` as
the `affix_def` default (`affix.rs:225`). **No `[affixes.critMultiplier]`
block in the launch TOML.**

The reasoning, recorded because it generalises: **a TOML override that
permanently contradicts the code default makes the code default a lie.**
Every future reader of `affix_def` would see `0.05`, believe it, and be
wrong — and the only thing telling them otherwise is a data file that is
not in the repository. A ratified permanent design change belongs in the
source; a launch-time config value belongs in the config. The halving is
the former; Echo's `0.00857` is the latter.

`[slot_base_power].amulet = 0.025` **stays in the TOML**, because the
four new slots' base powers are launch configuration for a world that
does not exist yet — and because R5's test catches it if the two drift.

Supersedes §9.6's launch block; the corrected block is §10.9.

---

### 10.5 R5 — a test asserting the four base-power pairings

**RATIFIED.** The implementation pass adds a test asserting:

| slot | must equal |
|---|---|
| `slot_base_power.ring1` | `affix_balance(Affix::CritChance).0` |
| `slot_base_power.ring2` | `affix_balance(Affix::CritChance).0` |
| `slot_base_power.amulet` | `affix_balance(Affix::CritMultiplier).0` |
| `slot_base_power.pants` | `affix_balance(Affix::IncreasedLife).0` |

§8's ruling — "a slot's base power equals exactly one affix of that type
at the item's tier" — is an invariant spanning two independent config
sections (`[slot_base_power]` and `[affixes.*]`, plus the `affix_def`
code defaults after R4), with no check of any kind between them. It will
drift the first time someone tunes a crit number, and the drift is
silent: the amulet simply stops being worth one affix and nothing
reports it.

**The test must read through the resolved accessors** —
`base_power_for_slot()` and `affix_balance()` — **not** the raw
`AffixDef` constants. Reading the constants would pass while the live
game was mismatched, which is the exact failure the test exists to
prevent.

This is a *design* invariant, not a physical one: if a future ruling
deliberately breaks a pairing, the test is updated alongside it. Its job
is to make the break visible and deliberate.

---

### 10.6 R6 — strip `[affixes.lingeringEffect]` from the launch file

**RATIFIED.** The retired header is not carried into the new world's
config.

**Why, recorded in full because the guard makes it look harmless:** on
2026-08-21 this exact header — an `[affixes.*]` block naming an affix
that deserializes to a real `Affix` variant but is absent from
`ALL_AFFIXES` — **panicked every request in production for roughly 2.5
minutes before rollback.** `affix_balance` now handles it
(`affix.rs:318-331`): `resolved.get_mut(&affix)` returns `None`, a
warning is logged, the entry is skipped.

That guard is **scar tissue from the incident, not a licence to keep the
header.** Carrying it forward means every startup logs a warning about
an affix that has not existed since 2026-08-21, and it leaves a live
example of the shape that caused the outage sitting in the file where
the next person to copy a block will find it. A fresh world starts with
a clean file.

The `LingeringEffect` variant itself stays in the `Affix` enum and in
`affix_def` — it must, or unmigrated saved items fail to deserialize
(see the variant's own doc). This ruling is about the *config file*
only.

---

### 10.7 R7 — reload semantics, recorded prominently

**RATIFIED, and this section is the authority for it. Any doc, patch
note, or comment claiming `adventure-item-balance.toml` is hot-tunable
is wrong.**

**The two config files in this system have opposite reload semantics,
and conflating them is the failure mode this ruling exists to prevent:**

| File | Held as | Re-read | Restart needed? |
|---|---|---|---|
| `adventure-item-balance.toml` — affix `per_tier`/`weight`, `slot_base_power`, `slot_power_cap`, `cooldown`, `craft_action_cost` | `OnceLock` (`AFFIX_BALANCE` `affix.rs:303`, `SLOT_POWER` `item.rs:930`, `COOLDOWN_CURVES`) | **never** — initialised lazily on first read | **YES** |
| `adventure-live-tunables.toml` — `LiveTunables`, incl. `splash_ladder_step_pct`, the pacing controllers, the top layer | `std::sync::RwLock` in `AdventureManager` | **every fight** | **NO — genuinely hot** |

`LiveTunables`' own doc states the contrast explicitly
(`tunables.rs:8-9`): *"unlike ... `OnceLock`, this is held live in
`AdventureManager` behind a `std::sync::RwLock` and re-read on every
fight, so a saved change takes effect"* without a restart.

**The practical consequence for the two rulings above:**

- **R1 (Echo, `[affixes.echo].per_tier`) → `adventure-item-balance.toml`
  → no rebuild, no deploy, but a RESTART IS REQUIRED.**
- **R3 (splash, `splash_ladder_step_pct`) → `LiveTunables` → no rebuild,
  no deploy, no restart. Genuinely hot, dashboard-editable, effective
  from the next fight.**

Both were described as "zero-code fixes," and both are — but only one
can be changed on a running game. Saying "no deploy" for both is
accurate and, on its own, actively misleading.

**Corrections applied to this document under this ruling:**

1. **§5.4** said the level-cut half of a rebalance was "testable live
   with no deploy." Corrected to "without a rebuild," with a pointer
   here. `adventure-item-balance.toml` is not hot.
2. **§5.1** said the pacing baseline anchors are "dashboard-shapable
   live tunables, so this can be tuned after launch." That claim is
   **correct and stays** — they are `LiveTunables`. Annotated with a
   pointer here so a later reader does not "fix" a true statement while
   correcting the false one.
3. **§9.6 point 3** already stated the `OnceLock` restart requirement
   and is unchanged.

Patch notes for the launch must not describe Echo's coefficient as
hot-tunable.

---

### 10.8 R8 — never reset pushed work to a SHA quoted in an order

**RATIFIED AS HOUSE PRACTICE.**

**When an order names a base SHA that is behind the branch head, append
to HEAD and flag the discrepancy. Never reset, never force-push, never
rewrite pushed history to match the quoted SHA.**

The precedent: the order that produced §9 said "append to
`docs/affix-curve-spec` branch at `8bdb30d`" while the branch was at
`691f050` — `8bdb30d` was simply where the Decision being ruled on had
been *written*, two commits earlier. Resetting to it would have silently
discarded the crit-cut and new-slots work from the immediately preceding
order.

The general shape: **a SHA in an order is usually a citation, not an
instruction.** It identifies where the thing being discussed lives. The
cost asymmetry is decisive — appending to a head the owner did not
expect is trivially corrected in the next message, while discarding
pushed work is not always recoverable, and is never recoverable *by the
owner without being told it happened.* Append, then state plainly in the
report which SHA you built on and which one the order named.

This sits alongside the existing BRANCH DISCIPLINE rule in `CLAUDE.md`
("don't rebase/reset/stash shared state — another session may have work
in flight") and extends it: the hazard is not only *concurrent*
sessions, it is also *sequential* orders whose quoted SHAs have gone
stale.

---

### 10.9 The launch configuration, corrected

Supersedes §9.6's block.

```toml
# --- world launch: adventure-item-balance.toml ---
# RESTART REQUIRED for any change here (OnceLock - see 10.7).
# NOT hot. Do not describe these as live-tunable.

[affixes.echo]
per_tier = 0.00857      # R1. 6 instances -> 1 guaranteed repeat at T=1000.
                        # Round UP: 0.00856 = 0.999137, misses the anchor.

[slot_base_power]
ring1  = 0.01           # R5-tested: must equal affixes.critChance per_tier
ring2  = 0.01           # R5-tested: must equal affixes.critChance per_tier
amulet = 0.025          # R5-tested: must equal affixes.critMultiplier per_tier
pants  = 0.03           # R5-tested: must equal affixes.increasedLife per_tier

# NO [affixes.critMultiplier] block - R4 puts the 0.05 -> 0.025 halving
# in affix_def (affix.rs:225) as the code default instead.
# NO [affixes.lingeringEffect] block - R6.
```

And separately — **a different file, with different reload semantics,
hot from the next fight**:

```toml
# --- world launch: adventure-live-tunables.toml ---
# Hot (RwLock, re-read every fight). No restart needed.

splash_ladder_step_pct = 350    # R3, from 1000. 6 instances -> rung 1 at
                                # T=1000. Round DOWN: 351 misses.
```

Everything else stays at its code default: `[slot_power_cap]`,
`[cooldown.*]`, `[craft_action_cost]`, every other affix's `per_tier`
and `weight`, and `splash_overcap_bonus_targets` /
`splash_ladder_targets_per_step` / `splash_damage_pct` /
`splash_extra_targets`.

---

### 10.10 R9 — post-curve the splash ladder is one rung

**RATIFIED.** For a six-instance build at step 350 the rungs land at
T ≈ 998 / 10,988 / 44,692 / 120,933 / 261,742. Only the first sits
inside any tier range this game will occupy.

This was already recorded as Qualification 1 inside §10.3; it is
promoted to a numbered ruling here so it cannot be read as a caveat on
R3 rather than a decision in its own right. **The mechanic is a single
+1 target, once. That is the intended shape, not a shortfall.**

### 10.11 R10 — naming constraint: do not call it a "ladder" to players

**RATIFIED.** Wiki text and patch notes must not describe the splash
overcap as a ladder, a staircase, or anything else implying repeated
rungs. A player who reads "ladder" will look for rung two, and at
T ≈ 10,988 will not find it inside the life of the world.

Describe it as what it is: **passing 350% splash grants one additional
target.** The internal names (`splash_ladder_step_pct`,
`splash_ladder_targets_per_step`) stay — renaming a `LiveTunable` key
for no mechanical gain is pure churn — but the player-facing wording is
constrained.

### 10.12 R11 — rung 2 is deferred, not broken

**RATIFIED.** Rung 2 at T ≈ 10,988 is left where it falls. It is not a
bug to be tuned out, and the step is **not** to be lowered further to
pull it into range — see R12 for why the floor is where it is.

If a future world ever reaches those tiers, rung 2 activates on its own
with no change required. Recorded so a later session does not read the
single-rung outcome as an incomplete implementation.

### 10.13 R12 — the Ranger coupling is a buff, and ships as one

**RATIFIED.** `splash_overcap_target_count` has a second consumer —
Ranger's Volley / Chain Lightning damage-bonus sizing line
(`combat.rs:14307`) — which reuses the same count formula by design so
the two can never drift. Lowering the step from 1000 to 350 raises
`max_targets_reachable` from 4 to 5 for a six-instance splash build at
T=1,000: **a +25% bump to `splash_target_dmg_bonus`.**

This is not a side effect to be suppressed. The shared helper exists
precisely so a change to the count formula reaches both call sites, and
decoupling them to keep Volley neutral would reintroduce exactly the
drift the helper was factored out to prevent. **It ships as a buff and
it goes in the patch note as a buff** — per COMMITS & DOCS, patch notes
are honest in both directions.

### 10.14 R13 — the `floor()` rounding rule

**RATIFIED AS GENERAL PRACTICE.** Promoted here from a note inside
§10.1, where it was easy to miss.

> **When a value is derived to hit a `floor()` boundary, round in
> whichever direction clears the boundary, and state in the spec which
> direction that was and why.**

The rule exists because the correct direction is not fixed — it depends
on which side of the division the derived quantity sits:

| Ruling | Quantity | Direction | The wrong value | What it does |
|---|---|---|---|---|
| R1 | Echo `per_tier` | **UP** → 0.00857 | 0.00856 → 0.999137 | falls short of 1 repeat |
| R3 | `splash_ladder_step_pct` | **DOWN** → 350 | 351 → `floor(350.16/351) = 0` | falls short of 1 rung |

Both anchors are `floor()` thresholds. Echo's coefficient is the
*numerator* of the comparison and the step is the *denominator*, so
clearing the boundary means rounding in opposite directions. Truncating
by habit would have broken both.

The same discipline covers §1's `0.289` over `ln2/ln11 = 0.2890648` —
there the deviation is nowhere near a `floor()` boundary, so readability
wins and the rounding is free. **Check whether a boundary is in play
before deciding that rounding is cosmetic.**

---

## 11. The five "dead" passive nodes (D35)

**RATIFIED, and verified against the model.** Recorded here rather than
only in the crit-saturation forensics report, because it changes what
the passive rebalance should do.

**Pressure Point, Nerve Strike, Stone Fist, Granite Skin and Overgrown
Reach are not weak nodes. They are correctly-sized nodes drowned by
three orders of magnitude of gear inflation.**

### 11.1 What the five nodes actually are

| Node | Key | Effect at 3/3 |
|---|---|---|
| Pressure Point | `pressurepoint` | `Special` — Flowing Strikes' stacks grant **+6% crit chance per stack** |
| Nerve Strike | `nervestrike` | `Special` — **+0.30 to `crit_multiplier`** |
| Stone Fist | `stonefist` | `OverflowConversion` Evasion → IncreasedDamage, `spec` max_rank **4**, capped **+0.40** |
| Granite Skin | `graniteskin` | `OverflowConversion` Evasion → IncreasedDamage, modifier max_rank 3, capped +0.30 |
| Overgrown Reach | `risingdefiance` | `OverflowConversion` Evasion → IncreasedDamage, modifier max_rank 3, capped +0.30 |

Overgrown Reach's node key is `risingdefiance` — renamed 2026-08-17, key
unchanged (`passive_tree.rs:986-1000`). Worth knowing before anyone
greps for it.

**Three of the five are hard-capped into the same tree conversion pool —
a combined +1.000, not +0.90.** Stone Fist is a `spec` node with
`max_rank: 4`, so its cap is `OVERFLOW_CONVERSION_CAP_PER_RANK × 4 =
0.40`; the other two are `modifier_with_effect` at max_rank 3, capped
0.30 each. **0.40 + 0.30 + 0.30 = 1.000.** Flowing Strikes caps at 5
stacks, or 8 with Flow like Water at 3/3, so Pressure Point tops out at
+0.30 or +0.48 crit chance.

> **Corrected 2026-08-23 (§13).** An earlier revision of this table said
> "capped +0.30" for all three and "+0.90 into the same **additive**
> bucket." Both were wrong: Stone Fist's cap is 0.40 at rank 4, and the
> pool is `tree_total`, which is **multiplicative**, not additive. See
> §13.

### 11.2 ~~Verified at live scale — the ×1.003 is correct~~ **SUPERSEDED BY §13**

> ## ⚠ SECTIONS 11.2 AND 11.4 ARE SUPERSEDED
>
> The ×1.003 figure below was computed on a **refuted model** — it added
> the three overflow nodes' output into the *gear* bucket, when the code
> puts it in `tree_total`, which is its own multiplicative layer.
> **The correct figure for those three nodes is ×2.000, and the
> damage-forensics session is right.** §13 carries the reconciliation
> and the corrected conclusions. The tables below are retained only so
> the error stays legible; **they must not be cited.**
>
> The two crit nodes (Nerve Strike, Pressure Point) *were* modelled
> correctly and their figures stand — see §13.4.

Measured across the live roster, real equipped gear, gear-only bucket:

| character | crit chance | crit mult | bucket | 5 stacks | 8 stacks |
|---|---|---|---|---|---|
| merkosh | 5,973% | 375 | 766 | ×1.00203 | ×1.00206 |
| galquin | 7,983% | 100 | 759 | ×1.00421 | ×1.00423 |
| sitch89 | 1,788% | 453 | 726 | ×1.00247 | ×1.00280 |
| yo_pony | 5,285% | 522 | 674 | ×1.00197 | ×1.00201 |
| zolaries | 11,242% | 647 | 602 | ×1.00197 | ×1.00198 |
| clincl | 8,227% | 496 | 596 | ×1.00214 | ×1.00216 |

**×1.002 to ×1.004 — the ruling's ×1.003 is confirmed** for the typical
geared character.

**The strongest evidence for the thesis is the outlier.** `kazesosa`
carries **zero** crit affixes — crit chance 5% (the bare
`BASE_CRIT_CHANCE`), crit multiplier 2.0 (the bare baseline). On that
character the same five nodes at the same ranks are worth **×1.199**.
Same nodes, same ranks, gear inflation removed: **200× the effect.**
The thesis demonstrated on live data rather than argued.

### 11.3 Verified under the curve

| claim | verdict |
|---|---|
| a CritChance affix at T=1,300 is worth ~21% | **20.99%** — `0.01 × f(1300)`, `f(1300) = 20.986` ✓ |
| five instances ≈ 1.05 crit stacks | **1.0493** ✓ — gear affixes alone, excluding the 5% base |
| overcrit bracket ≈ 1.07 of a possible 2.5 | **1.0705** naive ✓ — but corrected below |
| the bucket falls to single digits | **2.06 – 6.35** at T=1,300 ✓ |

**The 1.05 excludes two real sources.** Adding `BASE_CRIT_CHANCE` gives
**1.0993**; adding the two new ring implicits from §8 gives **1.5190**.
A real post-curve character sits nearer 1.52 stacks than 1.05. The
bracket at 1.5190 is 1.5125 — still only 61% of the 2.5 ceiling, so
"far from saturation" holds at all three figures.

**CORRECTION — the true bracket is ~1.04, not ~1.07.**
`crit_stack_bonus` is evaluated at a whole number of stacks, and real
stacks are a two-point distribution (`floor(cc)` or `floor(cc)+1`), so
the expectation is the probability-weighted average of the function at
both — **not** the function at the average. The function is concave past
the first stack, so by Jensen's inequality the naive figure overstates:

| crit chance | naive `bracket(E[stacks])` | **exact `E[bracket]`** |
|---|---|---|
| 1.0493 | 1.0705 | **1.0370** |
| 1.0993 | 1.1355 | **1.0745** |
| 1.5190 | 1.5125 | **1.3893** |

`Character::combat_total_output_per_sec` already does this correctly and
its own doc calls out the Jensen trap explicitly
(`character.rs:3337-3349`). **This makes the conclusion stronger, not
weaker — the build is even further from saturation than stated.**

On the bucket: the ruling's **~1205** is the full
`combat_increased_damage` including the passive-tree multiplicative
layer. The **gear-only** bucket on the same top character is **766**.
Both are correct readings of different quantities, and this spec's own
tables from §4 onward are gear-only, so the two should not be compared
directly. Either reading supports the claim — under the curve at
T=1,300 the gear bucket runs 2.06 (average affix luck) to 6.35 (eleven
damage-bucket affixes).

### 11.4 ~~The revival, quantified~~ **SUPERSEDED BY §13** — the overflow half of this section is computed on the refuted model; the evasion-gate table itself is correct and is carried forward into §13.5

**New finding, not in the ruling: the three overflow nodes are gated on
evasion actually exceeding the 75% cap, and the curve moves that gate.**

At T=1,300 one Evasion affix is worth 33.58%, so **2.23 instances are
needed before any overflow exists at all.** Average affix luck across
nine slots is 0.69 instances → evasion 0.231 → **zero overflow, and all
three nodes pay exactly nothing.**

| Evasion instances | evasion | overflow | the 3 nodes pay |
|---|---|---|---|
| 1 | 0.336 | 0.000 | **0.000** |
| 2 | 0.672 | 0.000 | **0.000** |
| 3 | 1.007 | 0.257 | 0.489 |
| 4 | 1.343 | 0.593 | 0.834 |
| 5 | 1.679 | 0.929 | **0.900** (all capped) |
| 6 | 2.015 | 1.265 | **0.900** |

So the revival is real, but **conditional on the build the nodes were
written for**:

| build at T=1,300, under the curve | five nodes worth |
|---|---|
| average luck, no evasion investment (Pressure Point + Nerve Strike only) | **×1.266** |
| 3 Evasion instances | **×1.469** |
| 5 crit + 5 Evasion instances, 5 stacks | **×1.549** |
| 5 crit + 5 Evasion instances, 8 stacks | **×1.632** |
| *the same 5-crit / 5-evasion build at live linear scale* | *×1.024* |

**From ×1.003 to ×1.55.** The curve restores these nodes by roughly two
orders of magnitude without touching a single line of
`passive_tree.rs`.

### 11.5 Consequence for the passive rebalance — **see §13.6 for the corrected version**

**DO NOT BUFF THESE FIVE.** Ratified. The curve restores them; buffing
them now leaves them overtuned in the new world, and the correction
would then have to be a nerf to nodes players had just been told were
improved.

Three things to carry into that rebalance, all flagged rather than
decided:

**The opposite risk is now live.** Three nodes contributing a flat +0.90
into a bucket of ~2.06 is **+44% from three nodes.** The caps (+0.30
each) were sized against a linear world where they were rounding errors,
and nothing has re-derived them for a world where the bucket is single
digits. "Do not buff" is settled; **"do these now need a nerf" is a
real question**, and it is the same class of finding as this ruling
pointing the other way.

**The gate is the saving grace and should stay.** The overflow nodes pay
nothing below ~2.23 Evasion instances, which keeps them a genuine
evasion-build reward rather than a free +0.90 for everyone. Any change
to `Evasion`'s `per_tier`, or to the 75% cap, moves that gate — check it
before touching either.

**The sweep has not been run.** These five were found by review, not by
a systematic pass. Every flat-magnitude `Special` and every capped
`OverflowConversion` in the tree has the same shape and is subject to
the same drowning-then-revival. That sweep should happen **before** any
node is retuned, or the rebalance will buff nodes the curve was about to
fix.

---

## 12. The last three open items, closed

Decisions 11, 13 and 20's sibling were the only items this spec still
listed as awaiting a ruling. All three are now closed. **With these, the
spec is complete unless something contradicts it.**

### 12.1 D11 — `compute_power` stays linear

**RATIFIED. The five existing slots' base power is NOT curved.**
`compute_power` (`item.rs:967-974`) keeps `base_power_for_slot(slot) ×
tier as f64 × power_roll` exactly as it is today.

The measurement that decides it, over the fresh-start range with the
full package (four new slots, `CritMultiplier` at 0.025):

| ruling | T^ (25→200) | doubling item tier |
|---|---|---|
| **`compute_power` LINEAR** | **T^1.44** | **×2.72** |
| `compute_power` curved | T^0.42 | ×1.34 |
| *§3 ratified target* | *T^1.43* | *×2.70* |

**Doubling every item's tier for 34% more damage is not a progression
game.** Linear lands on the ratified target essentially exactly.

**Scope clarification, now authoritative:** "every other tier-scaled
value" in §8's slot ruling means **AFFIX values, not slot base power.**
The curve applies to `affix_base_value` and to the four new slots'
implicits — the latter because those implicits are affix-equivalents by
the "one affix's worth" ruling, *not* because they are slot powers.

Three consequences worth recording:

- **The 50 ms `attack_interval_ms` floor survives.** It sits at
  T ≈ 2,500 with `compute_power` linear, and at T ≈ 2.0e10 if curved.
  Gloves keep their only brake, which was an argument for this ruling
  and is now a property of it.
- **Weapon and Body power stay linear and uncapped.** They remain the
  two layers of the original six that are *not* slowed by the curve.
  §8.5's measurement already accounts for this — T^1.44 is the number
  *with* them linear — so this is not a hidden cost, but it does mean
  weapon power is the single largest untouched growth term in the new
  world. If the post-launch exponent drifts high, that is the first
  place to look.
- **`[slot_base_power]` for the existing five stays at code defaults.**
  Nothing in §10.9's launch file changes.

### 12.2 D13 — the new slots use the affix jitter band

**RATIFIED. The four new slots roll their base power against
`0.85..1.15` — the affix jitter band — NOT `POWER_ROLL_RANGE`
(`0.85..1.20`).**

The reasoning: §8's ruling is that a slot's base power **equals exactly
one affix of that type at the item's tier.** An invariant that holds at
the floor and breaks at the ceiling is not an invariant. Under
`POWER_ROLL_RANGE` the implicit tops out **4.3% above** the affix it is
defined to equal — at T=100 the Amulet's ceiling is 30.00% against a
28.75% affix maximum. Small, but it is a permanent, structural lie about
what the slot is.

**Record explicitly: this makes the four new slots the first equipment
in the game that does not share the existing slots' roll range.** That
is a deliberate divergence, not an oversight, and it has consequences a
later reader will otherwise trip over:

| Touch point | Consequence |
|---|---|
| `generate_item_at_tier` / `generate_item_at_tier_with_roll` (`item.rs:1033`, `1054`) | draw the new slots' `power_roll` from `0.85..1.15`; the existing five keep `POWER_ROLL_RANGE` |
| `Item::power_roll` field | stores a value that is now range-dependent on `slot`. Its doc says the roll is "fixed for that item's whole lifetime" — still true, but the *range* it came from now varies |
| `Item::quality_percent` | measures the roll against `POWER_ROLL_RANGE`'s span. **On a new slot this will read wrong** — a maxed ring would show as ~86% quality, never 100% — unless it is made range-aware. This is the one that will produce a live bug report if missed |
| `make_item_perfect` / `apply_divine_dust` (`character.rs:2078`) | set `power_roll = POWER_ROLL_RANGE.end`. On a new slot that is 1.20, **above its own 1.15 ceiling** — must clamp to the slot's own range or a Perfect ring exceeds one affix's worth by exactly the 4.3% this ruling exists to remove |
| `QUALITY_STEP` polish math (`character.rs:1908`, `1941-1956`) | computes `jitter_span` from `POWER_ROLL_RANGE`; needs the same range-awareness |
| R5's pairing test (§10.5) | still passes — it tests the *coefficient*, not the roll. But the ruling's intent is now only fully enforced if the test also checks that the ranges match. Recommended addition |

**The cleanest implementation shape** is a `roll_range_for_slot(slot)`
helper beside `base_power_for_slot`, returning `POWER_ROLL_RANGE` for
the existing five and `0.85..1.15` for the new four, with every site
above reading through it. A bare constant swap will miss
`quality_percent` and `make_item_perfect`, and both fail silently.

**This does not change the rng draw count.** `gen_range` is called
exactly once either way, just against a different range — so §5.3's
constraint is respected and this contributes no additional golden-fixture
divergence beyond the slot addition already accounted for in §8.7.

### 12.3 D20-sibling — four-plus instances is the right floor

**RATIFIED AND CLOSED. The step is not lowered below 350 to bring
2-instance builds into reach.**

At step 350 the splash overcap ladder is reachable by a six-instance
build at T = 1,000 and by a four-instance build as a long tail at
T = 10,000. A two-instance build never reaches it, peaking at 227%
against the 350% requirement.

**That is the intended shape.** The splash overcap is a heavy build
commitment — six slots, or four slots and patience. **A threshold a
casual allocation reaches is not a commitment**, and lowering the step
to make it one would delete the only thing that distinguishes a splash
build from a build that happened to roll splash twice.

Closed. Do not re-open on the observation that 2-instance builds miss
it; that observation is the ruling, not an objection to it.

---

## 13. Reconciliation — the forensics session is right, §11 was wrong

**Resolution: the damage-forensics session's ×2.000 stands. My
×1.002–×1.004 was computed on a refuted model and is withdrawn.** §11.2
and §11.4 are marked superseded above. Only one figure now stands.

### 13.1 Are we measuring the same quantity?

Nominally yes — both claim to be "what these nodes multiply total damage
by." The difference is not one of scope. **One of them was computed
against the wrong formula.**

| | what it is a multiplier ON |
|---|---|
| **Forensics ×2.000** | the `(1 + tree_total)` factor in `combat_increased_damage` (`character.rs:3015-3072`), which multiplies the **entire** `(1 + gear_total)` product |
| **My ×1.003** | the ratio of `critEV × (1 + bucket)` with the three nodes' output **added into `bucket`** — i.e. into `gear_total` |

### 13.2 Does my model treat the tree as multiplicative? No — and that is the error

**It treated the tree as additive into the gear bucket. The figure is
therefore computed on a refuted model and is withdrawn.**

The code is unambiguous:

```rust
// character.rs:3015-3072, combat_increased_damage
let gear_total = sum_affix(IncreasedDamage) + damage_type_bonus
               + archetype.bonus.increased_damage + self.defensive_overflow();
let tree_total = self.passive_bonus().increased_damage
               + self.passive_overflow_bonus().increased_damage;
((1.0 + gear_total) * (1.0 + tree_total) * … - 1.0).max(-0.9)
```

`OverflowConversion` nodes land in `passive_overflow_bonus()`
(`character.rs:2616-2644`), therefore in **`tree_total`**, therefore in
their **own multiplicative factor**. They never touch `gear_total`.

- Additive into a gear bucket of 766: `(1 + 766.9)/(1 + 766)` = **×1.001**
- Multiplicative as `(1 + 1.000)`: **×2.000**

That is the entire discrepancy. Three orders of magnitude of gear
inflation is exactly what makes the two answers so far apart — which is
also why the error was invisible in the output. ×1.003 *looked* like the
drowning thesis confirming itself.

### 13.3 The forensics arithmetic reproduces exactly, including the ranks

**`(1 + 601.61) × 2.000 − 1 = 1204.22`** ✓ against the logged 1204.22.

And the `tree_total` of exactly **1.000** is not the 0.90 the node
descriptions imply. The reason is a code detail worth recording on its
own:

```rust
// character.rs:2639-2640
let raw    = overflow * node.magnitude_at_rank(rank);                  // effective_rank = min(rank, 3)
let capped = raw.min(OVERFLOW_CONVERSION_CAP_PER_RANK * rank as f64);  // RAW rank
```

`magnitude_at_rank` clamps a `Specialization` node to
`effective_rank = min(rank, 3)` (`passive_tree.rs:531-537`), but the cap
uses the **raw** rank. `stonefist` is a `spec()` node with
`max_rank: 4`, so at rank 4 its **efficiency saturates at the rank-3
value (1.00) while its cap keeps climbing to 0.40**:

| node | kind | max_rank | efficiency at max | cap |
|---|---|---|---|---|
| `stonefist` | `spec` | **4** | 1.00 (rank-3 value) | **0.40** |
| `graniteskin` | modifier | 3 | 0.45 | 0.30 |
| `risingdefiance` | modifier | 3 | 0.45 | 0.30 |
| | | | | **1.000** |

**Rank 4 of a Specialization `OverflowConversion` buys +0.10 of cap and
zero efficiency, while the node's own description still reads "up to
+30% at 3/3."** Flagged as undocumented behaviour for the passive
rebalance; not this branch's to change.

**Live allocations confirm the prediction exactly.** Every character the
forensics measured at ×2.000 carries `stonefist=4, graniteskin=3,
risingdefiance=3` — `olympiclarry` and `gorshie` as primary Monks;
`yo_pony`, `roxus` and `kmartbikes1` through a Monk **secondary** tree
via Split Personality. Every character it measured at ×1.000 — `ttfn`,
`clincl`, `pappag4ming` — has none of the three invested. Zero
exceptions across the sample.

### 13.4 What survives from §11, and what does not

**The five nodes were never one group. They split cleanly in two, and
D35's thesis is true of one half and false of the other.**

| | Nerve Strike, Pressure Point | Stone Fist, Granite Skin, Overgrown Reach |
|---|---|---|
| Where the magnitude lands | **added** into `crit_multiplier` / `crit_chance` alongside gear (`combat.rs:4041-4042`, `4053`) | **`tree_total`** — its own multiplicative factor |
| Drowned by gear inflation? | **YES** | **NO** |
| Worth at live scale | **×1.0004 – ×4.04** | **×2.000** |
| Under the curve | revives to **×1.27 – ×1.37** | **unchanged at ×2.000** — if the gate is cleared |
| D35's thesis | **correct** | **refuted** |

My additive treatment of the two crit nodes was right — they genuinely
are summed into pools gear fills at scale. Recomputed cleanly across the
37 characters with real crit gear, the two nodes alone at 3/3 are worth
**×1.00039 (`ttfn`, huge crit gear) to ×4.04 (`pc_glory`, minimal crit
gear)** — a 4,000× spread driven by nothing but how much crit gear sits
underneath them. That spread *is* the drowning thesis, cleanly
demonstrated, and it is the part of §11 that survives intact.

The `kazesosa` datapoint survives too: with zero crit affixes (crit
chance 5%, crit multiplier 2.0 — both bare baselines) the two crit nodes
at 3/3 would be worth **×1.198**. That figure came from the crit half of
the model, which was correct. *(Recorded as the hypothetical it is —
`kazesosa` actually has `pressurepoint=1, nervestrike=0`.)*

### 13.5 The saturation hypothesis — right observation, wrong reconciliation

The working hypothesis offered was that live builds sit far past the 75%
evasion threshold and are saturated, while under the curve most never
clear it. **The observation is correct and important. It is not the
reconciliation** — the two figures differ because one was computed on
the wrong formula, not because they describe different conditions. Said
plainly, so the record does not imply a tidier story than the one that
happened.

The observation itself, with numbers. All three nodes reach their caps
once evasion overflow ≥ 0.6667:

| tier | instances needed, **linear** | instances needed, **curve** | factor |
|---|---|---|---|
| 100 | 0.885 | 8.85 | 10× |
| 500 | 0.177 | 5.56 | 31× |
| **1,300** | **0.068** | **4.22** | **62×** |
| 3,000 | 0.030 | 3.31 | 112× |
| 10,000 | 0.009 | 2.34 | 264× |

**Today it takes 7% of one Evasion affix to fully saturate all three
nodes.** Any Monk with any evasion at all gets the full ×2.000 for free
— precisely why the forensics found it uniform across every character
that had them invested. Under the curve at T=1,300 it takes **4.22
instances**: a real five-slot commitment.

Behaviour under the curve at T=1,300:

| Evasion instances | evasion | overflow | tree_total | multiplier |
|---|---|---|---|---|
| 1 | 0.336 | 0.000 | 0.000 | ×1.000 |
| 2 | 0.672 | 0.000 | 0.000 | ×1.000 |
| 3 | 1.007 | 0.257 | 0.489 | ×1.489 |
| 4 | 1.343 | 0.593 | 0.934 | ×1.934 |
| **4.22** | 1.417 | 0.667 | **1.000** | **×2.000** |
| 6 | 2.015 | 1.265 | 1.000 | ×2.000 |

**The curve does not weaken these three nodes at all. It makes them
conditional.** A dedicated evasion build gets exactly what it gets
today; a build with no evasion investment gets nothing where today it
got a free doubling.

### 13.6 Corrected consequence for the passive rebalance

**"DO NOT BUFF" still holds for all five — but for opposite reasons, and
the reasons matter more than the conclusion.**

- **Nerve Strike, Pressure Point** — do not buff because **the curve
  revives them** (×1.003 → ×1.27–1.37). §11.5's original reasoning was
  correct for these two.
- **Stone Fist, Granite Skin, Overgrown Reach** — do not buff because
  **they are already the largest multiplicative layer a Monk has.** A
  flat ×2.000 from three nodes is not a drowned node needing help.
  §11.5 was wrong about these; this is the corrected reasoning.

**The nerf question is now far more pointed than §11.5 framed it.**
Three nodes granting a flat, unconditional ×2.000 — reachable today at
7% of one affix — is the single largest bespoke multiplier in the game
outside the gear bucket itself. The curve makes it conditional but does
not make it smaller. That is D42, deliberately deferred to D41's sweep.

---

## 14. D41 — the sweep

**RATIFIED: this sweep gates the passive rebalance. No node is retuned
until it completes.** Retuning first means buffing nodes the curve was
about to fix and missing nodes the curve is about to break — §13 is a
worked example of exactly how wrong a by-review judgement can be.

### 14.1 The classification rule

§13 produced the rule the whole sweep turns on. **A node's exposure to
gear inflation is decided by whether its magnitude is ADDED into a pool
gear also fills, or opens its OWN multiplicative factor.**

| Class | Where the magnitude lands | Drowned by gear? | What the curve does |
|---|---|---|---|
| **A — additive** | summed into `gear_total`, `crit_chance`, `crit_multiplier`, or any pool gear fills at scale | **YES** — worth `(pool + x)/pool` | **revives it** |
| **B — multiplicative** | its own `(1 + x)` factor: `tree_total`, or a bespoke layer in `combat_increased_damage` | **NO** — worth `(1 + x)` at any gear scale | **nothing.** Only the gate moves |

Class B nodes were never drowned and cannot be revived. **For them the
only questions are whether the input gate is still reachable, and
whether a fixed `(1 + cap)` sized for a world where that gate was free
is still correctly sized in a world where it costs five slots.**

### 14.2 Every `OverflowConversion` node — COMPLETE, 14 of 14

All 14 are **Class B**. Caps are `OVERFLOW_CONVERSION_CAP_PER_RANK ×
max_rank`; efficiency saturates at effective rank 3 (§13.3).

| Node | Key | Conversion | max_rank | cap | eff |
|---|---|---|---|---|---|
| Unbreakable | `unbreakable` | Block → IncreasedDamage | 4 | 0.40 | 1.00 |
| Stone Fist | `stonefist` | Evasion → IncreasedDamage | 4 | 0.40 | 1.00 |
| Granite Skin | `graniteskin` | Evasion → IncreasedDamage | 3 | 0.30 | 0.45 |
| Overgrown Reach | `risingdefiance` | Evasion → IncreasedDamage | 3 | 0.30 | 0.45 |
| Shifting Form | `shiftingform` | Evasion → IncreasedDamage | 4 | 0.40 | 1.00 |
| Primal Shift | `primalshift` | Evasion → IncreasedDamage | 3 | 0.30 | 0.45 |
| Elusive | `elusive` | Evasion → CritChance | 4 | 0.40 | 0.75 |
| Phantom | `phantom` | Evasion → CritChance | 3 | 0.30 | 0.30 |
| Claw Strike | `clawstrike` | Evasion → CritChance | 3 | 0.30 | 0.60 |
| Duskveil | `duskveil` | Evasion → AttackSpeed | 3 | 0.30 | 0.75 |
| Lightfoot | `lightfoot` | Evasion → AttackSpeed | 3 | 0.30 | 1.00 |
| Earthen Will | `earthenwill` | Evasion → MaxHpPct | 3 | 0.30 | 0.75 |
| Aegis Ward | `aegisward` | Intervene → DamageReduction | 4 | 0.40 | 1.00 |
| Sanctified Armor | `sanctifiedarmor` | Intervene → DamageReduction | 3 | 0.30 | 0.45 |

**Combined caps per output stat — the number that matters, since each
output is a single pooled `tree_total`:**

| Output | nodes | max combined | lands in |
|---|---|---|---|
| **IncreasedDamage** | 6 | **+2.10 → ×3.10** | `tree_total`, `combat_increased_damage` |
| **CritChance** | 3 | **+1.00 → ×2.00** | `tree_total`, `combat_crit_chance` |
| **DamageReduction** | 2 | +0.70 | `combine_reduction_sources` — capped, behaves differently |
| **AttackSpeed** | 2 | +0.60 → ×1.60 | `tree_bonus`, `attack_interval_ms` |
| **MaxHpPct** | 1 | +0.30 → ×1.30 | `tree_increased`, `combat_max_hp` |

**Verdict for all 14: NONE revive, NONE are dead, and the six
IncreasedDamage nodes are the OVERTUNED candidates.** No single
character reaches +2.10 — the six split across Warrior (`unbreakable`),
Monk (`stonefist`/`graniteskin`/`risingdefiance`) and Druid
(`shiftingform`/`primalshift`) — but Split Personality makes
cross-archetype combinations real, and **`merkosh` already runs
`unbreakable=4` primary plus the full Monk trio on a secondary tree:
0.40 + 1.000 = +1.40 → ×2.40 today.**

**The gate, per input stat** — affix instances needed to begin
overflowing, linear / curve:

| Input | cap | per_tier | T=100 | T=500 | T=1,300 | T=3,000 |
|---|---|---|---|---|---|---|
| Evasion | 0.75 | 0.016 | 0.47 / 4.69 | 0.09 / 2.94 | **0.04 / 2.23** | 0.02 / 1.75 |
| BlockChance | 0.75 | 0.02 | 0.38 / 3.75 | 0.07 / 2.36 | 0.03 / 1.79 | 0.01 / 1.40 |
| Intervene | 0.50 | 0.01 | 0.50 / 5.00 | 0.10 / 3.14 | 0.04 / 2.38 | 0.02 / 1.87 |

Under the curve every gate costs 2–5 affix instances instead of a
rounding error. **That is the curve's entire effect on Class B: it turns
fourteen free bonuses into fourteen build commitments.**

### 14.3 The `Special` half — OUT OF SCOPE FOR THIS BRANCH (D43)

> **D43 — RATIFIED. This is a scoped, ready-to-resume task owned by the
> PASSIVE REBALANCE, not an open item on this spec.** 407 sites
> classified by read-site rather than declaration is a project in its
> own right. It is **not to be resumed on this branch.** Everything
> below is the starting kit for that project, and is kept for exactly
> that purpose.



**Status: HANDED OFF, not blocked.** `passive_tree.rs` contains **407**
`Special` sites. Classification cannot be done from `passive_tree.rs`
alone, because a `Special` node's class is decided by its **read site in
`combat.rs` / `character.rs`**, not by its declaration — `nervestrike`
and `stonefist` are both declared with flat magnitudes and land in
opposite classes. Each of the 407 needs its `passive_node_magnitude` /
`passive_node_rank` consumer located and classified. That is a real
sweep, not a grep, and it did not fit this pass.

**What is already known, so the sweep does not restart from zero:**

**Class B (multiplicative — will NOT revive)** — named explicitly in
`combat_increased_damage` as "compounds separately": Titan's Grip,
Overwhelming Force (+ Grim Resolve), Momentous Blow, Reckless Swing,
Death Wish (+ Glory Hound), Life Tap (+ Soul Exchange). Six bespoke
layers, each its own `(1 + x)`. Treat exactly like the Class B
conversions above.

**Class A (additive — WILL revive)** — confirmed: Nerve Strike
(→ `crit_multiplier`), Pressure Point (→ `crit_chance`), plus every
generic `FlatStat` node pooled through `passive_bonus()` whose output
stat gear also fills. The **51** `FlatStat` sites are the first place to
look.

**The method, so whoever runs it does not re-derive it:**

1. Enumerate every `Special` and `FlatStat` node key.
2. For each, grep `passive_node_magnitude("<key>")` /
   `passive_node_rank("<key>")` in `combat.rs` and `character.rs`.
3. Classify by the **read site**: added into an existing sum → **Class
   A**; its own `(1 + x)` factor, or into `tree_total` → **Class B**.
4. Class A: report `(pool + x)/pool` at live scale and under the curve.
   That ratio is the revival.
5. Class B: report `(1 + x)` — unchanged by the curve — plus the gate
   cost if it has an input threshold. **These are the nerf candidates.**

**Do not retune any node before steps 1–5 complete for all of them.**

---

## 15. Final rulings (D43–D45)

### 15.1 D43 — the `Special` sweep belongs to the passive rebalance

**RATIFIED.** Applied to §14.3 above: that section is now marked as a
scoped, ready-to-resume task **owned by the passive rebalance**, not as
an open item on this spec. It is not to be resumed on this branch.

Kept deliberately, because it is what that project needs to start from:
the Class A / Class B classification rule (§14.1), the six known Class B
bespoke layers named in `combat_increased_damage`, the confirmed Class A
nodes, the 51 `FlatStat` starting points, and the five-step method.

**This is a handoff, not a gap.** §14.2's `OverflowConversion` half is
complete and stands on its own; the `Special` half was never in this
spec's remit — it surfaced here only because §13's correction made the
classification rule visible.

### 15.2 D44 — the Stone Fist rank-4 behaviour is a DEFECT

**RATIFIED AS A DEFECT, NOT A DESIGN.**

```rust
// character.rs:2639-2640
let raw    = overflow * node.magnitude_at_rank(rank);                  // effective_rank = min(rank, 3)
let capped = raw.min(OVERFLOW_CONVERSION_CAP_PER_RANK * rank as f64);  // RAW rank
```

`magnitude_at_rank` clamps a `Specialization` node to
`effective_rank = min(rank, 3)` (`passive_tree.rs:531-537`) while the
cap uses the **raw** rank. `stonefist` is a `spec()` node with
`max_rank: 4`. Therefore **rank 4 buys +0.10 of cap and zero
efficiency**, while the node's own description still reads *"up to +30%
at 3/3."*

Three things follow, all recorded as consequences of the defect rather
than of any decision:

1. **It is why the trio sums to exactly 1.000 rather than 0.90**
   (0.40 + 0.30 + 0.30), which is the number the forensics session
   back-solved from the live logs (§13.3).
2. **Every player holding `stonefist=4` spent a passive point on cap
   headroom they cannot use** unless their overflow is large enough to
   be clipped by it — which, at live scale, it always is. So the point
   is not wasted today; it is worth exactly +0.10 of flat increased
   damage and nothing else, which is not what the node text promises.
3. **It affects every `spec()`-tier `OverflowConversion` node**, not
   just Stone Fist — `unbreakable`, `elusive`, `shiftingform` and
   `aegisward` share the shape (§14.2's `max_rank: 4` rows).

**Not fixed here — docs only.** Whether the fix is to clamp the cap to
`effective_rank`, to let efficiency continue to rank 4, or to correct
the description, is a passive-rebalance decision, and it changes live
player power either way.

**Cross-reference: anomaly ledger #54.**

> ⚠ **The cross-reference could not be verified from this branch.**
> `docs/anomaly_ledger.md` at this branch's base (`3b1dea1`, last
> touched by `b24163e`) has a highest entry of **#45**; there is no #54.
> Either the log-parser session has advanced the ledger past this
> branch's base, or the number needs confirming. **The ledger's
> numbering is canonical and owned by the log-parser session
> (`CLAUDE.md`), so this branch has neither created an entry nor
> renumbered anything** — the reference is recorded as given and flagged
> for that session to confirm or correct.

### 15.3 D45 — headline result: what the curve is actually for

**RATIFIED AS A HEADLINE RESULT.**

> **Saturating Stone Fist, Granite Skin and Overgrown Reach costs
> 0.068 Evasion affix instances today. Under the curve at T = 1,300 it
> costs 4.22. That is a 62× change.**
>
> **The curve does not weaken these nodes. It converts a free doubling
> into a five-slot commitment.**

The full progression, because the factor grows with tier:

| tier | instances, **linear** | instances, **curve** | factor |
|---|---|---|---|
| 100 | 0.885 | 8.85 | 10× |
| 500 | 0.177 | 5.56 | 31× |
| **1,300** | **0.068** | **4.22** | **62×** |
| 3,000 | 0.030 | 3.31 | 112× |
| 10,000 | 0.009 | 2.34 | 264× |

**This is the single clearest demonstration in this document of what the
curve is for.** Every other result here is a number moving — an exponent
falling from T^4.95 to T^1.44, a bucket shrinking from 766 to 2.06, a
coefficient halving. This one is a *decision* coming back into
existence. Today, "should I invest in evasion?" is not a question any
Monk asks: 7% of one affix instance buys the entire ×2.000, so the nodes
are free and the choice is not a choice. Under the curve the same
×2.000 costs five slots, and the player has to weigh it against
everything else those slots could carry.

The curve's purpose was never only to slow numbers down. It was to make
the numbers small enough that choosing between them means something
again. These three nodes are where that shows up most legibly, and they
are worth citing whenever the rebalance needs to explain itself.

---

## 16. Closing — scope, non-scope, and what happens first

**NOTHING IN THIS DOCUMENT HAS BEEN IMPLEMENTED.** This branch is
docs-only from first commit to last. No code was changed, no test was
added, no configuration file was edited, no fixture was regenerated, and
nothing was merged or deployed. Every number here was computed against
the live code as it stands at `3b1dea1` and against live production data
read read-only. **This spec is a record of decisions, not a record of
work done.**

### 16.1 What this spec covers

| | |
|---|---|
| **The curve** | `f(T) = sqrt(T)` for T ≤ 100, `10 × (T/100)^0.289` above — replacing the tier term in `affix_base_value` only (§1–§3) |
| **Scope of the curve** | `affix_base_value` and the four new slots' implicits. **`compute_power` stays linear** (D11, §12.1) |
| **CritMultiplier** | `per_tier` halved 0.05 → 0.025, in code, as the `affix_def` default (§7, R4) |
| **Four new slots** | `Ring1`, `Ring2` (crit chance 0.01), `Amulet` (crit multiplier 0.025), `Pants` (increased life 0.03), each worth exactly one affix of that type (§8) |
| **New-slot roll range** | the **affix** band `0.85..1.15`, not `POWER_ROLL_RANGE` (D13, §12.2) |
| **Echo** | `per_tier = 0.00857` via `adventure-item-balance.toml` (R1, §9) |
| **Splash ladder** | `splash_ladder_step_pct = 350` via `LiveTunables` (R3, §10.3) |
| **Reload semantics** | the two config files behave oppositely — item-balance needs a restart, live-tunables does not (R7, §10.7) |
| **The passive-node findings** | the ×2.000 reconciliation, D35's split, the complete 14-node `OverflowConversion` sweep (§13, §14.2) |

### 16.2 What this spec deliberately does NOT cover

- **The `Special` half of the D41 sweep** — 407 sites, handed to the
  passive rebalance (D43, §14.3). Starting kit kept; work not begun.
- **D42, "do these nodes now need a nerf"** — open by design, deferred
  until that sweep completes (§13.6, ledger entry 57).
- **The Stone Fist rank-4 defect's fix** — recorded as a defect (D44),
  not fixed. The fix changes live player power and belongs to the
  rebalance.
- **Any reweighting of affixes against each other.** Relative affix
  weights are preserved exactly; the sole exception is
  `CritMultiplier` (Decision 3, Decision 9). The five elementals
  collectively out-sloping `IncreasedDamage` by 4.75×, and defensive
  overflow supplying ~22.5% of the offensive bucket, are both recorded
  and both untouched.
- **The pacing controllers, the top layer, and the enemy side generally.**
  §5.1 requires the baseline anchors be re-derived, but no new values are
  proposed here.
- **Anything about a live population.** This ships with a full restart;
  §4 records the four requirements that context waives and why each
  becomes mandatory without it.

### 16.3 What an implementation pass must do, in order

**Before writing code:**

1. **Fit report first**, per `CLAUDE.md` PROCESS — verify these premises
   against the code as it then stands, enumerate touch points, propose a
   staged plan, stop for approval. This document is a year of decisions,
   not a licence to skip that step.
2. **Tell the owner before touching `EQUIP_SLOTS`.** Its length is read
   by `adventure_web/wiki.rs`, which belongs to the wiki session. Per
   `CLAUDE.md` rule 3 this must be sequenced with them, not merely
   announced.
3. **Notify the desktop companion's maintainer** — `ring1`, `ring2`,
   `amulet`, `pants`, and the append-only warning (§8.8). Their item
   codec encodes slot as a positional index; appending is safe,
   reordering silently corrupts every previously-shared v2 link.

**Then, as two separate commits — this ordering is mandatory (§8.7):**

4. **Commit one — the curve and the crit cut.** `affix_base_value` gains
   `f(T)`; `affix_def`'s `CritMultiplier` becomes 0.025. Keep the `rng`
   draw count inside `roll_affixes` byte-identical (§5.3). Regenerate
   the 17 golden fixtures **at merge**, attributed to "affix values
   changed."
5. **Commit two — the four slots.** Variants, `EQUIP_SLOTS`, `Character`
   fields, `base_power_for_slot`, `noun_pool`, `item_stat_line`, the
   four consumption sites, and the **six hardcoded five-slot lists in
   `adventure_web.rs`** (3155, 3903, 4019, 5128, 5236, 5748 — none
   compiler-caught). Add `roll_range_for_slot(slot)` and route
   `quality_percent`, `make_item_perfect` and `apply_divine_dust`
   through it (D13 — both fail silently otherwise). Regenerate fixtures
   again **at merge**, attributed to "loot rng stream shifted."

**Then:**

6. **Add R5's pairing test**, reading through `base_power_for_slot()` /
   `affix_balance()`, never the raw `AffixDef` constants (§10.5).
7. **Write the launch config** (§10.9): `[affixes.echo] per_tier =
   0.00857` and the four `[slot_base_power]` keys —
   **restart required**; `splash_ladder_step_pct = 350` in
   `LiveTunables` — hot. No `[affixes.critMultiplier]` block (R4), no
   `[affixes.lingeringEffect]` block (R6).
8. **Re-derive the pacing baseline anchors** against the new curve
   (§5.1). Genuinely tunable after launch, but the shipped defaults
   should not be the old ones.
9. **Append the `WIKI_IMPACT.md` lines** listed in §8.6.
10. **Write the patch note honestly** — the curve is a nerf and says so;
    the Ranger Volley / Chain Lightning +25% is a buff and says so
    (R12); Echo's coefficient is **not** described as hot-tunable (R7).

**Acceptance tests — use the right ones:**

- Echo: **the damage-share table in §9.3**, not the exponent (R2).
- The curve: measure the exponent over a real tier window and expect
  T^1.33–T^2.05 depending on the window, not a constant T^1.43
  (Decision 12).
- The five passive nodes: nothing. They are not being changed
  (§13.6).

### 16.4 Branch status

**CLOSED.** Sixty-three decisions, §1–§16. Every item is either ratified
or explicitly deferred with its owner named. The one deferred item
(D42) and the one handed-off task (D43) both belong to the passive
rebalance.

Nothing here should change unless something contradicts it — as the
damage-forensics session's ×2.000 contradicted §11's ×1.003, and was
right to.

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
8. ~~**OPEN**~~ **RESOLVED (§9) — `Affix::Echo` `per_tier` set to
   0.00857 via `adventure-item-balance.toml`, no code.** Owner ruling.
   **Amended in two places by §9:** (a) Echo has no cliff at 100% —
   `E[repeats] = pct` exactly and continuously, so the problem was
   magnitude, not death; (b) reviving Echo moves the exponent by only
   +0.019 to +0.034, so it does not "restore a sixth of the exponent."
   Set it because a 1.5%-of-damage affix is a dud, not to recover
   scaling.
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
11. ~~**OPEN**~~ **RESOLVED — D11 (§12.1): `compute_power` STAYS
    LINEAR.** The five existing slots' base power is not curved.
    Measured, leaving it linear gives T^1.44, matching §3's ratified
    target; curving it gives T^0.42 — doubling every item's tier for 34%
    more damage is not a progression game. **"Every other tier-scaled
    value" in §8's ruling means AFFIX values, not slot base power.**
12. **CORRECTION to §3, independent of Decision 11.** `0.289 × 4.95` is
    valid only in the asymptotic limit where every compounding layer is
    ≫ 1, which is not true post-curve. Direct measurement gives T^1.33
    to T^2.05 across the tier windows the game will occupy. **§3's
    T^1.43 is a target, not a constant.** Measure at implementation.
13. ~~**OPEN**~~ **RESOLVED — D13 (§12.2): the four new slots roll
    against the AFFIX jitter band `0.85..1.15`, NOT `POWER_ROLL_RANGE`
    `0.85..1.20`.** "Equals exactly one affix of that type" must hold at
    both ends of the roll, not only at the floor. This makes the new
    slots the first equipment in the game that does **not** share the
    existing slots' roll range — see §12.2 for what that touches.
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
17. **`Affix::Echo` `per_tier = 0.00857`, set in
    `adventure-item-balance.toml`** (§9). Owner-ratified. Value verified
    exactly: `6 × 0.00857 × f(1000) = 1.00030`. The rounding direction
    is load-bearing — `0.00856` yields `0.99914` and misses the anchor.
18. **CORRECTION to §6 and Decision 8, recorded plainly.** `roll_echo`
    gives `floor(pct)` guaranteed repeats **plus** `Bernoulli(remainder)`,
    so `E[repeats] = pct` exactly, for every `pct`. Echo below 100% is a
    continuous linear multiplier, not an inert affix. §6's "the curve
    deletes the layer" reached the right conclusion by the wrong
    mechanism; the real problem was that the layer was worth ×1.015 on a
    full six-slot commitment. Same correction applies to
    `roll_divine_heal_power_proc`, which has the identical shape.
19. **CONTRADICTED — Echo's death was not costing meaningful exponent**
    (§9.4). Reviving Echo moves the measured exponent by **+0.019 to
    +0.034** (1–2%), not by a sixth. Both the dead and revived figures
    sit on §3's T^1.43 target. This does not change the ruling; it
    changes its justification, and it means the implementation pass must
    not use "exponent recovered" as the acceptance test for this change.
20. ~~**OPEN**~~ **RESOLVED — R3 (§10.3): fixed via
    `splash_ladder_step_pct = 350`**, the `LiveTunable`, leaving the
    100% overcap behaviour untouched. Its sibling question — should the
    step go lower to bring 2-instance builds into reach — is **CLOSED by
    D20-sibling (§12.3): no. Four-plus instances is the right floor.**
21. **No other affix has a death condition** (§9.5). Full sweep run.
    Two non-obvious thresholds exist and both are fine: `DivineDamage`'s
    heal-power self-buff has an Echo-shaped `floor()` threshold reached
    at T ≈ 55 on six instances, with the same continuous fallback; and
    the elemental proc chance clamps at a rolled fraction of 10.0
    (`ELEMENTAL_PROC_CHANCE_DIVISOR = 10.0`), which is a ceiling rather
    than a floor and becomes unreachable harmlessly. `IncreasedDamage`,
    `IncreasedLife`, `FlatLife`, `Leech` and `CritMultiplier` have no
    tier-magnitude threshold at all.
22. **The base-power coupling is an unenforced invariant** (§9.6).
    `[affixes.critChance]` ↔ `[slot_base_power].ring1`/`ring2`,
    `[affixes.critMultiplier]` ↔ `.amulet`, `[affixes.increasedLife]` ↔
    `.pants` must always match per §8's ruling, and they live in
    separate TOML sections with no code check between them. A test
    asserting the four pairings is recommended.
23. **Crit belongs in code, Echo in TOML** (§9.6). The §7 halving is a
    permanent ratified design change and should become the `affix_def`
    default; Echo's coefficient is explicitly a launch config value by
    Decision 17. Splitting them this way keeps the code default honest.
24. **A TOML edit needs a process restart** (§9.6). `AFFIX_BALANCE` and
    `SLOT_POWER` are `OnceLock`s. "No deploy" is accurate; "no restart"
    is not.
25. **R1 — Echo `per_tier = 0.00857`, rounding UP** (§10.1). Ratified.
    `0.00856` = 0.999137 and misses the anchor. General rule adopted:
    when a value is derived to hit a `floor()` boundary, round in
    whichever direction clears it, and state which direction that was.
26. **R2 — Echo's acceptance test is the damage-share table, NOT the
    exponent** (§10.2). Ratified. Exponent passes either way (+0.019 to
    +0.034). The justification is that a 1.5%-of-damage affix on a full
    six-slot commitment is a dud. Verify §9.3's shares; do not verify
    exponent.
27. **R3 — `splash_ladder_step_pct = 350`** (§10.3). Ratified via the
    `LiveTunable`, not `[affixes.splash].per_tier`, so the 100% overcap
    behaviour is untouched. Verified: `floor(350.1648/350) = 1` rung at
    T=1,000 on six instances; **step 351 gives 0 and misses**, so this
    one rounds DOWN — the mirror image of R1.
28. **R3 qualifications, recorded** (§10.3). Post-curve the ladder is
    effectively ONE rung — rung 2 sits at T ≈ 10,988 — so it should not
    be described as a "ladder" to players. And it is a **4+ instance
    mechanic only**: a 2-instance build never reaches a rung anywhere up
    to T = 10,000. Neither changes the value; both need saying.
29. **R3 side effect: Ranger's Volley / Chain Lightning gains ~25%**
    (§10.3). `splash_overcap_target_count` has a second consumer at
    `combat.rs:14307` that sizes `splash_target_dmg_bonus` off the same
    count. Intended coupling, but not neutral — belongs in the patch
    note. Enemies (Cube/Dragon) are unaffected by construction: they
    force `splash_fraction` to exactly 1.0 and the overcap branch is
    gated on `> 1.0`, strictly.
30. **R4 — the CritMultiplier halving goes in CODE, not TOML** (§10.4).
    Ratified. A TOML override that permanently contradicts the code
    default makes the code default a lie. `[affixes.critMultiplier]` is
    dropped from the launch file; `[slot_base_power].amulet = 0.025`
    stays.
31. **R5 — test the four base-power/affix pairings** (§10.5). Ratified.
    Must read through `base_power_for_slot()` / `affix_balance()`, not
    the raw `AffixDef` constants — reading the constants would pass
    while the live game was mismatched.
32. **R6 — strip `[affixes.lingeringEffect]` from the launch file**
    (§10.6). Ratified. The guard that makes it survivable is scar tissue
    from the 2026-08-21 outage, not a licence to keep it. The enum
    variant stays; only the config block goes.
33. **R7 — the two config files have OPPOSITE reload semantics**
    (§10.7). Ratified and now the authority for it.
    `adventure-item-balance.toml` is `OnceLock` — **restart required.**
    `adventure-live-tunables.toml` is `RwLock`, re-read every fight —
    **genuinely hot.** So R1 (Echo) needs a restart and R3 (splash) does
    not, despite both being "zero-code." §5.4 corrected; §5.1's claim
    verified correct and annotated so nobody "fixes" it.
34. **R8 — never reset pushed work to a SHA quoted in an order** (§10.8).
    Ratified as house practice. A SHA in an order is usually a citation,
    not an instruction. Append to HEAD, then state plainly which SHA you
    built on and which the order named. Extends `CLAUDE.md`'s BRANCH
    DISCIPLINE rule from concurrent sessions to sequential orders with
    stale SHAs.
35. **R9 — post-curve the splash ladder is ONE rung** (§10.10). Ratified.
    Rungs at T ≈ 998 / 10,988 / 44,692 / 120,933 / 261,742; only the
    first is reachable. The intended shape, not a shortfall. Promoted
    from a qualification inside §10.3 to a ruling in its own right.
36. **R10 — do not call it a "ladder" to players** (§10.11). Ratified.
    Wiki and patch-note wording is constrained to "passing 350% splash
    grants one additional target." The internal `LiveTunable` key names
    stay as they are.
37. **R11 — rung 2 is deferred, not broken** (§10.12). Ratified. Left
    where it falls; activates on its own if a world ever reaches those
    tiers. Not an incomplete implementation.
38. **R12 — the Ranger coupling ships as a buff** (§10.13). Ratified.
    +25% to `splash_target_dmg_bonus` for a six-instance splash build at
    T=1,000. Not suppressed — decoupling would reintroduce the drift the
    shared helper was factored out to prevent. Goes in the patch note as
    a buff.
39. **R13 — the `floor()` rounding rule, promoted to its own subsection**
    (§10.14). Ratified as general practice. The direction is not fixed:
    Echo's coefficient is the numerator and rounds UP; the splash step is
    the denominator and rounds DOWN. Truncating by habit breaks both.
    Check whether a boundary is in play before deciding rounding is
    cosmetic.
40. **D11 — `compute_power` STAYS LINEAR** (§12.1). Ratified, closing
    Decision 11. Linear gives T^1.44 against the ratified T^1.43 target;
    curved gives T^0.42. "Every other tier-scaled value" means AFFIX
    values, not slot base power. Consequences: the 50 ms attack-interval
    floor survives at T ≈ 2,500, and weapon/body power remain the largest
    untouched growth terms in the new world — first place to look if the
    post-launch exponent drifts high.
41. **D13 — the four new slots roll against the AFFIX band 0.85..1.15**
    (§12.2). Ratified, closing Decision 13. "Equals exactly one affix"
    must hold at both ends, not just the floor. **This makes them the
    first equipment that does not share the existing slots' roll range.**
    Six touch points enumerated; `Item::quality_percent` and
    `make_item_perfect` both fail SILENTLY if missed — the latter would
    set `power_roll = 1.20` on a slot whose ceiling is 1.15, reinstating
    the exact 4.3% overshoot this ruling removes. Implement via a
    `roll_range_for_slot(slot)` helper, not a bare constant swap. No
    change to the rng draw count.
42. **D20-sibling — four-plus instances is the right floor, CLOSED**
    (§12.3). Ratified. The step is not lowered below 350. A threshold a
    casual allocation reaches is not a commitment. Do not re-open on the
    observation that 2-instance builds miss it — that observation IS the
    ruling.
43. **D35 — the five "dead" nodes revive under the curve; DO NOT BUFF
    THEM** (§11). Ratified and verified. ×1.003 at live scale confirmed
    (measured ×1.002–×1.004 across the geared roster); ~21% CritChance
    affix at T=1,300 confirmed exactly (20.99%); 1.05 crit stacks
    confirmed (1.0493, gear affixes excluding the 5% base); bucket falls
    to single digits confirmed (2.06–6.35). The curve restores them from
    ×1.003 to ×1.55 without touching `passive_tree.rs`.
44. **CORRECTIONS and additions to D35, none weakening it** (§11.3,
    §11.4). (a) The overcrit bracket at 1.0493 stacks is **~1.04, not
    ~1.07** — `crit_stack_bonus` is concave past the first stack and real
    stacks are a two-point distribution, so `E[bracket] < bracket(E[cc])`
    by Jensen; `combat_total_output_per_sec` already does this correctly
    (`character.rs:3337-3349`). The build is even further from
    saturation than stated. (b) The ~1205 bucket figure is the FULL
    `combat_increased_damage` including the tree layer; the gear-only
    figure on the same character is 766 — this spec's tables are
    gear-only, so do not compare them directly. (c) **NEW: the three
    overflow nodes are gated on evasion exceeding 75%, needing ~2.23
    Evasion instances at T=1,300.** Average affix luck yields ZERO
    overflow and they pay nothing — the revival is conditional on the
    build they were written for, which is the correct outcome. (d) Live
    proof of the thesis: `kazesosa`, who carries no crit gear at all,
    already gets ×1.199 from the same five nodes today — 200× the
    geared-character figure, with gear inflation the only variable.
45. **Two follow-ups for the passive rebalance, flagged not decided**
    (§11.5). (a) **The opposite risk is now live** — three nodes adding a
    flat +0.90 into a bucket of ~2.06 is +44% from three nodes, and the
    +0.30 caps were sized against a linear world. "Do not buff" is
    settled; "do these now need a NERF" is a real open question. (b)
    **The sweep has not been run** — these five were found by review, and
    every flat-magnitude `Special` and capped `OverflowConversion` in the
    tree has the same shape. Run that sweep BEFORE retuning any node, or
    the rebalance will buff nodes the curve was about to fix.
46. **Spec status: COMPLETE.** With Decisions 11, 13 and 20's sibling
    closed, no item in this document awaits a ruling. Further changes
    should come only from something contradicting what is recorded here.
47. **R9-R13 promotion confirmed correct** (§10.10-10.14). The
    caveats-vs-rulings distinction is ratified as the reason: a
    qualification attached to R3 reads as a caveat on that ruling, not
    as a decision standing on its own, and R11 in particular would
    otherwise have been read as an incomplete implementation.
48. **D13's `roll_range_for_slot(slot)` helper is RATIFIED over a bare
    constant swap** (§12.2). Both silent failures are now IMPLEMENTATION
    REQUIREMENTS, not notes: (a) `Item::quality_percent` measures
    against `POWER_ROLL_RANGE`'s span, so a maxed ring would cap at ~86%
    quality and never reach 100%; (b) `make_item_perfect` /
    `apply_divine_dust` write `POWER_ROLL_RANGE.end` = 1.20 onto a slot
    whose ceiling is 1.15, reinstating the exact 4.3% overshoot D13
    exists to remove. Neither is compiler-caught.
49. **CARRY FORWARD: weapon and body power are the largest untouched
    growth terms in the new world** (§12.1). Both stay linear and
    uncapped under D11. First place to look if the post-launch exponent
    drifts high.
50. **CONTRADICTION RESOLVED — the damage-forensics session is right;
    §11's x1.003 is WITHDRAWN** (§13). The figure was computed by adding
    the three overflow nodes into the GEAR bucket, when the code puts
    them in `tree_total`, which is its own multiplicative layer. Correct
    figure: **x2.000**. `(1 + 601.61) * 2.000 - 1 = 1204.22` reproduces
    the logged value exactly, and live allocations confirm the split
    with zero exceptions: every x2.000 character carries
    `stonefist=4, graniteskin=3, risingdefiance=3` (primary Monk or via
    Split Personality); every x1.000 character carries none. §11.2 and
    §11.4 are marked SUPERSEDED; only one figure now stands.
51. **The tree_total of exactly 1.000 comes from a rank-4 cap/efficiency
    mismatch** (§13.3). `magnitude_at_rank` clamps a Specialization node
    to `effective_rank = min(rank,3)` while the cap uses the RAW rank,
    so `stonefist` at rank 4 gets efficiency 1.00 (the rank-3 value) and
    a cap of 0.40. 0.40 + 0.30 + 0.30 = 1.000. **Rank 4 buys +0.10 of
    cap and zero efficiency, while the node description still reads "up
    to +30% at 3/3."** Flagged as undocumented behaviour for the passive
    rebalance.
52. **D35 is TRUE of two of the five nodes and REFUTED for the other
    three** (§13.4). Nerve Strike and Pressure Point are additive into
    pools gear fills and ARE drowned — measured x1.00039 (`ttfn`) to
    x4.04 (`pc_glory`), a 4,000x spread driven by nothing but underlying
    crit gear. Stone Fist / Granite Skin / Overgrown Reach are their own
    multiplicative layer and were never drowned. The five were never one
    group.
53. **The saturation hypothesis is the right OBSERVATION but the wrong
    RECONCILIATION** (§13.5). The two figures differ because one used
    the wrong formula, not because they describe different conditions.
    The observation stands on its own: saturating all three nodes costs
    0.068 Evasion instances today and 4.22 under the curve at T=1,300 —
    a 62x rise. **The curve does not weaken these nodes; it makes them
    conditional.**
54. **"DO NOT BUFF" survives for all five, for OPPOSITE reasons**
    (§13.6). The two crit nodes because the curve revives them; the
    three overflow nodes because they are already the largest
    multiplicative layer a Monk has.
55. **D41 sweep — the `OverflowConversion` half is COMPLETE, 14 of 14**
    (§14.2). All 14 are Class B (multiplicative): none revive, none are
    dead, and the six IncreasedDamage nodes are the overtuned
    candidates at a combined cap of +2.10. `merkosh` already runs
    +1.40 → x2.40 today by pairing `unbreakable=4` with the full Monk
    trio through Split Personality.
56. **D41 sweep — the `Special` half is BLOCKED on scope** (§14.3). 407
    `Special` sites; class is decided by the combat.rs read site, not
    the declaration, so each needs its consumer located. The
    classification rule, the six known Class B bespoke layers, the
    confirmed Class A nodes, the 51 `FlatStat` starting points and the
    five-step method are all recorded so the sweep does not restart from
    zero. **No node is retuned until it completes.**
57. **D42 — "do these now need a NERF" is OPEN, deliberately deferred
    until D41's sweep completes.** Reason: the nerf candidates are
    Class B nodes, and Class B membership is exactly what the incomplete
    half of the sweep determines. Deciding now would repeat §13's error
    in the opposite direction — acting on a partial classification. The
    question is sharper than §11.5 framed it: three nodes granting a
    flat x2.000, reachable today at 7% of one affix, is the largest
    bespoke multiplier in the game outside the gear bucket, and the
    curve makes it conditional without making it smaller.
58. **Spec status: COMPLETE except D42, which is deferred BY DESIGN to
    D41's sweep.** Supersedes Decision 46. No other item awaits a
    ruling.
59. **D43 — the `Special` half of the D41 sweep is OUT OF SCOPE for this
    branch and is not to be resumed here** (§14.3, §15.1). It is a
    scoped, ready-to-resume task **owned by the passive rebalance**, not
    an open item on this spec. The classification rule, the six known
    Class B bespoke layers, the confirmed Class A nodes, the 51
    `FlatStat` starting points and the five-step method are kept
    deliberately as that project's starting kit. A handoff, not a gap —
    §14.2's `OverflowConversion` half is complete and stands alone.
60. **D44 — the Stone Fist rank-4 behaviour is a DEFECT, not a design**
    (§15.2). `magnitude_at_rank` clamps a Specialization to
    `effective_rank = min(rank,3)` while the cap uses the raw rank, so
    rank 4 buys +0.10 of cap and zero efficiency while the node text
    reads "up to +30% at 3/3". It is why the trio sums to exactly 1.000
    rather than 0.90. **It affects every `spec()`-tier
    `OverflowConversion` node** — `unbreakable`, `elusive`,
    `shiftingform`, `aegisward` share the shape. Not fixed here; the fix
    changes live player power and belongs to the rebalance.
    Cross-references anomaly ledger #54.
61. **FLAG — anomaly ledger #54 could not be verified from this branch.**
    `docs/anomaly_ledger.md` at base `3b1dea1` (last touched by
    `b24163e`) has a highest entry of **#45**. Either the log-parser
    session has advanced the ledger past this branch's base, or the
    number needs confirming. The ledger's numbering is canonical and
    owned by that session per `CLAUDE.md`, so **this branch created no
    entry and renumbered nothing** — the reference is recorded as given
    and flagged for that session to confirm or correct.
62. **D45 — HEADLINE RESULT** (§15.3). Saturating Stone Fist, Granite
    Skin and Overgrown Reach costs **0.068 Evasion instances today
    versus 4.22 under the curve at T=1,300 — a 62x change**, rising to
    264x at T=10,000. **The curve does not weaken these nodes; it
    converts a free doubling into a five-slot commitment.** Recorded as
    the clearest demonstration in this spec of what the curve is for:
    every other result is a number moving, this one is a decision coming
    back into existence.
63. **BRANCH CLOSED** (§16). Nothing in this spec has been implemented —
    docs only from first commit to last, no code, no config, no
    fixtures, no merge, no deploy. §16 records what the spec covers,
    what it deliberately does not, and the ordered implementation
    sequence including the mandatory two-commit split, the wiki-session
    and desktop-maintainer prerequisites, and which acceptance test
    belongs to which change. Supersedes Decision 58 as the final status
    entry.
