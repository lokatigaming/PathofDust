# Pacing baseline anchors — measurement against the live affix curve

**Status:** measurement only. **Nothing was changed.** No value was tuned,
no tunable was written, no code was touched.
**Session:** PACING-ANCHORS · **Branch:** `feature/pacing-anchors` off
`origin/master` `ea5ef88` · **Measured:** 2026-09-03

Answers `docs/affix_curve_spec.md` §5.1, which flagged
`BASELINE_STAGE_ANCHORS` / `BASELINE_HP_ANCHORS` / `BASELINE_ATK_ANCHORS`
as derived against the old linear affix scaling and never re-derived.

---

## 0. Source of truth, and a trap that caught this session too

**Production is `/var/lib/pathofdust` on the Debian box. It is the only
valid source.** `C:\PathofDust` is the frozen pre-cutover Windows
snapshot. It is mtime-fresh, internally consistent, and completely wrong:
it reads **stage 7380** against production's **stage 8**, and its
tunables differ from production's on nine pacing fields —
`hp_max_step_per_fight` 0.05 vs 0.25, `dmg_max_step_per_fight` 0.05 vs
0.15, `hp_multiplier_ceiling` 1000 vs 6.0, `baseline_atk_anchors`
terminal 0.45 vs 0.62, `permanent_rampage` true vs false.

This session measured the frozen file first and produced a full, coherent,
entirely invalid set of numbers before catching it. That is **three
sessions** misled by this file (`session_journal.md:1198` records the
first two). A local file being recently modified is not evidence that it
is live.

---

## 1. Where the anchors sit vs where the controllers operate

Live window: 200 fights (47 boss, 153 basic) over 7.85 h. Boss win rate
**31/47 = 66.0%** — matches the reported figure exactly.

World: `stage 8`, `highest_stage 14`, observed range **2–14**,
oscillating with no net climb.

| axis | baseline floor @ stage 8 | controller | ceiling | pinned to floor? |
|---|---|---|---|---|
| HP (A) | 0.99872 | **6.0000** | **6.0** | **NO** — 6.01× above |
| ATK (B) | 0.99904 | 2.1135 | 4.0 | **NO** — 2.12× above |

`hp_pinned()` = `false`. `dmg_pinned()` = `false`. Neither floor binds.

**At what stage does the floor start binding? None — and it gets less
likely with stage, not more.** The anchors *decrease* monotonically
(1.0 → 0.55/0.62 across stage 0 → 3000), so the floor is tightest at
stage 0 and loosest deep. Production sits at the table's maximum
(~0.999, i.e. "never easier than the organic curve") and both controllers
are asking for **2×–6× harder** than organic. The gap is not marginal.

### The actual binding constraint is the CEILING, not the floor

`hp_pacing_mult` = **6.0** is exactly `hp_multiplier_ceiling` = 6.0.
Controller A is saturated at the top and still failing its target.

A is a closed-form solve: `required = mean_dps × 37.5s / base_pool`, then
clamped. Inverting the observed result gives A's honest request:

| | value |
|---|---|
| target band | 30–45 s (mid-target 37.5 s) |
| observed boss **win** durations | median **17.4 s**, mean 19.5 s |
| distribution | **87% under 30 s**, 0% over 45 s, max 34.4 s |
| A's honest request `37.5 × 6.0 / dur` | **11.5 – 12.9** |
| A's permitted ceiling | **6.0** |
| **shortfall** | **1.9× – 2.2×** |

`boss_losses_since_win` = 1, below `hp_relax_after_losses` = 3, so the
relaxation path is not engaged. A is pressed against the ceiling by
fights that are less than half as long as the system wants.

### What the party is experiencing, plainly

Bosses die in about **17 seconds** when the pacing system is trying to
make them take **30–45**. Fights are roughly **half to a third** the
intended length, and the system has no dial left — it is already
requesting the maximum it is allowed and is short by about a factor of
two.

Meanwhile the win rate is exactly on setpoint (66.0% vs the 2:1 target).
That is not the floor doing its job; it is **Controller B compensating
for A's saturation.** B has raised boss *damage* to 2.11× to hold the
win rate where A can no longer hold the duration. The two controllers are
covering for each other, and the result the player sees is a **burst
race**: bosses that evaporate in seconds but hit hard enough to wipe the
party one fight in three. Short and swingy, not the 30–45 s battle the
design intends.

B is currently descending (window 12W/8L = 1.5 ratio, below the 2.0
target; next step 2.1135 → 2.0284, −4.2%).

---

## 2. What the anchors were derived from, and the equivalent today

**Originally:** hand-authored, never computed. The doc comment states the
premise — party power "has historically outrun the LINEAR stage curve
increasingly at higher stages" (the old rubber-band reached 3.9× and once
pinned a 5.0 ceiling), so the room granted below organic widens with
stage.

**The equivalent derivation under the live curve is a no-op, and this is
a code fact, not a judgement.** `pacing.rs` contains **zero** references
to affixes or crit. The anchors are dimensionless *fractions of the
organic enemy stage/level/party formula*, and that formula has no affix
term. The affix cut changed the *numerator* (player power); the anchors'
*denominator* is untouched. There is nothing to re-derive arithmetically
— the units are unchanged.

What the affix cut can change is only *where the controllers settle*
relative to the floor, and that is measured, not derived: A at 6.0
(ceiling), B at 2.11. Both far above.

The original premise's *direction* also survives: it claimed parties
over-perform against the stage curve, and at the one stage we can observe
they over-perform so hard that A is jammed at its ceiling. §5.1's concern
that the floor would be "wrong in both directions" is not borne out at
the stage actually being played — the floor's value there (~0.999,
"never easier than organic") is correct and harmless.

**The rest of the table is untested and unreachable this season** (§3).

---

## 3. Is floor-binding reachable this season?

**Mechanically closer than World 1 suggested, but the direction of travel
is away from it.**

| | distance to floor | worst-case fights | at 6.0 boss/h |
|---|---|---|---|
| A (÷1.25 max step) | 6.01× | **8.0** | 1.3 h |
| B (÷1.15 max step) | 2.12× | **5.4** | 0.9 h |

Those are *worst-case* counts assuming maximum downward pressure every
fight. That pressure does not exist: A needs fights to run **over 45 s**
to descend and **0 of 31 wins did** (max 34.4 s); A is pinned at the
opposite rail. B needs a sustained all-loss window against the current
12W/8L.

**Stage rate is irrelevant to the answer.** The world moved 2 → 14 in
7.85 h and is oscillating around 8 with no net climb; `highest_stage` is
14. The first anchor where the floor departs from ~1.0 by more than 8% is
stage 500. At the observed trajectory that is not this season, and the
floor's value is ~0.999 everywhere the world will actually go.

---

## 4. Anything else calibrated the same way

`pacing.rs` has **no direct affix dependency at all** — nothing in it was
calibrated against affix magnitudes, so nothing else carries §5.1's
defect in its literal form.

One constant carries the same *structural* defect — a stage-shaped curve
read far from its knee — in the **opposite direction**:

**`top_layer_half_stage = 1500.0`** (`top_layer_for_stage` =
`cap × s/(s+half)`, `cap` = 0.6):

| stage | 8 (live) | 14 (peak) | 500 | 1500 (knee) |
|---|---|---|---|---|
| mitigation | **0.32%** | 0.55% | 15.0% | 30.0% |

The world is at **0.5% of the way to this curve's knee**, so the
stage-tied top layer is contributing essentially **nothing** to live
fights. The anchors are read *past the end* of their table; the top layer
is read at its very *start*. Both are stage-shaped curves whose shape is
inert at the stage actually being played. Noted, not changed — it is
outside this order.

---

## 5. Verdict

**The anchors do not need changing, and they are not this season's
problem.** They are not binding, not close to binding in the direction
travel is going, and their value at live stages (~0.999) is correct. §5.1
is real but **latent** — it concerns the 500→3000 region, which this
season will not reach.

**The live problem the measurement surfaced instead is
`hp_multiplier_ceiling = 6.0`.** Controller A is saturated against it and
short by roughly 2×, bosses are dying in ~17 s against a 30–45 s target,
and Controller B is masking it by raising boss damage to hold the win
rate. That is a ceiling question, not an anchor question, and it is a
**LiveTunable — a dial on `/admin/tunables`, no deploy and no restart**
(`adventure-live-tunables.toml` is `RwLock`, re-read every fight).

Not changed here. This order was measure-and-report, and raising a
ceiling that A will immediately consume is a balance decision for the
owner, not a cleanup.
