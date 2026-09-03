# Dynamic pacing — design pass

**Status:** proposal. **No game code written.** Awaiting owner ruling.
**Session:** PACING-ANCHORS (design) · **Branch:** `design/dynamic-pacing`
off `origin/master` `ea5ef88` · **Written:** 2026-09-03

Builds on `docs/pacing_anchor_measurement.md` (same session). Live numbers
there are not re-derived here.

**Live state at time of writing:** `hp_multiplier_ceiling` = **50.0** (the
owner's patch, applied on the admin page), which unpins Controller A.
`hp_pacing_mult` = 6.0, `boss_power_mult` = 2.1135, stage 8.

---

## 0. Controller A's signal path, stated plainly

| # | where | what |
|---|---|---|
| 1 | `manager.rs:5256` | `simulate_battle` returns `events`, `units` |
| 2 | `manager.rs:5262` | `real_duration_ms` = max event `at_ms`, floored at 1 — the **simulated** clock, never wall-clock |
| 3 | `manager.rs:5272-5275` | `enemy_pool` = Σ boss `max_hp`; `dealt_to_enemies` = Σ `Attack` damage on boss targets |
| 4 | `manager.rs:5276` | `pacing_sample_dps = min(dealt, pool) / real_duration_s` — overkill-capped |
| 5 | `manager.rs:5277` | **`compress_events` runs on the NEXT line** — the sample is taken first, deliberately |
| 6 | `manager.rs:5694` | `push_dps_sample` — wins only, finite only, trimmed to window |
| 7 | `pacing.rs:553` | `required = mean_dps × 37.5 s / base_pool`, then rate-limit + clamp |

A is a **closed-form solve**, not a hill-climber: it computes the
multiplier that would put kill time exactly at the band midpoint, then
rate-limits the move. This matters for everything below — A does not need
to search, so its errors are errors in its *inputs*, not in its search.

### Failure modes

**F1 — display/real divergence. The one that has been wrong before, and
the reason it is subtle.** `compress_events` returns scale exactly 1.0
whenever the fight sits inside `[target_duration_min_s,
target_duration_max_s]` — "the display window contains the whole pacing
window," by construction. So display and real are **identical exactly
while pacing is working**, and diverge **only when the fight overruns the
band** — precisely when A is failing. Sampling the display value would
therefore be self-concealing: the worse A performed, the more correct its
own signal would look. The current code samples real and is correct.
Verified live: 9/9 boss wins had `displayDurationMs == realDurationMs`.
**There is no test pinning the ordering.** A regression test asserting
"sample before compress" is cheap, and its absence is why this can regress
a second time.

**F2 — wins-only sampling plus stage regression: the documented upward
spiral.** A loss walks stage −2, which shrinks `base_pool`, which *raises*
`required`, while the wins-only window still describes the party that used
to win. `relax_hp_pacing_mult` is the release valve and correctly takes
precedence over the ordinary update. Understood, handled, still live.

**F3 — the denominator and the numerator are from different stages.
Unaddressed.** `base_pool` is *this fight's* organic pool; `mean_dps` is
the last 20 fights', taken across a stage walk of +1/−2 per fight. Organic
HP moves **15% per stage**, so a three-stage drift is a ~20% error in the
denominator (stage 8→11 = ×1.205; 8→6 = ×0.864). The error is worst at low
stage — where the world now lives. A is solving a precise equation with a
mismatched term.

**F4 — the overkill cap biases A downward.** `min(dealt, pool)` caps
measured throughput at the pool, so a party that deletes a boss reads as
exactly `pool / duration` and its true DPS is invisible. The cap is
correct for its own purpose (an overkill finisher must not inflate
throughput) but it means **A systematically understates how overpowered a
steamrolling party is** — biasing A *down* exactly when it needs to go up.
This is a contributing cause of the ~17 s bosses, not just the ceiling.

**F5 — no cost telemetry exists at all.** Nothing records event count,
unit count, or simulation wall-clock. World 1's 1.9 GB fights were found
by looking at the disk.

---

## 1. The ceiling is the wrong shape for its job

**Measured, and it inverts the ceiling's purpose.** A's `required` is
*highest at low stage*, because `base_pool` is tiny there (organic HP is
`74 × boss_health × party × level_mult × (1 + 0.15·stage)`) while player
capability is not. At stage 8, `required` ≈ 12.9 against a pool of ~9,700
HP — a fight that costs nothing. As stage climbs, the +15%/stage organic
term catches up and `required` falls.

So a **fixed multiplier ceiling binds hardest exactly where a fight is
cheapest, and is loosest exactly where cost is real.** That is precisely
backwards, and it is why 6.0 blocked the controller at stage 8 while 707×
was reachable at stage 7380.

**A's multiplier is not a cost variable at all.** Cost is event count ≈
duration × action rate × units × splash. A *solves for* duration — it
targets 37.5 s by construction — so duration is self-limiting whenever A
is working. The unbounded cost driver is elsewhere: `boss_count_for_stage`
is `tiers = stage / boss_count_tier_stages`, capped at
`floor(tiers × boss_count_cap_mult)` — **linear in stage with no absolute
cap** (World 1: 7373/300 = 24 tiers → ~35 bosses, matching the journal).
World 2 halves the tier size to 100, so boss count grows *three times
faster per stage* than World 1.

### Proposal

**(a) LiveTunable, today.** Stop using `hp_multiplier_ceiling` as a
difficulty bound. The owner's 50.0 is already the right move; keep it as a
far-away sanity rail, not a tuning dial. The correctly-shaped pool bound
**already exists** — `enemy_hp_pool_hard_cap` via `capped_hp_mult_for_pool`
— and is expressed in the units cost is actually paid in.

**(b) Code — measure before bounding.** Add per-fight cost telemetry at
`manager.rs:5262`, beside the existing `real_duration_ms` line: `events.len()`,
`units.len()`, and wall-clock around the `spawn_blocking` at 5256. One
struct, no behavior change, written into the fight summary. **You cannot
bound what you do not measure, and there is currently no instrument.**

**(c) Code, after (b) has data.** An event-count budget with an *explicit*
degradation path, not a silent clamp. The honest bound is on boss count
and event volume, not on A.

Recommend against a stage-scaled multiplier ceiling: it re-encodes the
same wrong variable with more arithmetic. Bound the pool and the events.

---

## 2. The two controllers compensate silently

**The asymmetry is the bug, and it is small.** `EffectiveMultipliers`
already exposes `hp_pinned()` / `dmg_pinned()` for the **floor**, and the
admin page renders "pinned at baseline floor" verbatim rather than
absorbing it into the `max()`. There is **no equivalent for the ceiling.**
The system can say "A is pinned low" — the case that has never happened —
and cannot say "A is pinned high", which is the case that just did.

### Proposal — surface it; do not make them coordinate

**(a) Code, small, mirrors an existing precedent exactly.** Add
`hp_saturated()` / `dmg_saturated()` against the ceiling, shaped like
`is_pinned_to_baseline`, and render them the same way. This is the same
four-lines-in-four-places change the floor already made.

**(b) Code.** One derived "pacing health" line on the admin page that
names the state, because the raw numbers do not distinguish it:

| state | condition | what it means |
|---|---|---|
| **healthy** | neither axis at a rail, duration in band | working |
| **A saturated, B compensating** | `hp_saturated()` ∧ win rate ≈ target ∧ B > 1 | **the live case** — win rate lies |
| **pinned at floor** | `hp_pinned()` ∨ `dmg_pinned()` | party under the stage baseline |

A 66% that means "working" and a 66% that means "A is saturated and B is
papering over it" must not render identically. Today they do.

**(c) Do NOT make them coordinate.** `pacing.rs` holds a deliberate
independence doctrine — A and B never read each other's variables — and it
is load-bearing: it is what makes each controller separately testable, and
the module documents it as such. Worse, the coordinated behavior is wrong
on its own terms: if B stood down when A saturated, fights that are
*already too short* would also become *easy*. B raising damage to hold the
win rate is B doing its job correctly under a broken constraint. The
defect is not B's behavior — it is that nothing said so. **Refusing is
wrong, coordinating is wrong, saying so is right.**

---

## 3. Pacing reads outputs, never inputs

**Confirmed:** `boss_stats_for(stage, party_size, avg_level, tunables)`
reads stage, party size and average level. **No gear, no affixes, no
player power.** `avg_level` is the only power-adjacent input, and level is
not gear — which is exactly why craft-driven power growth has nothing
opposing it.

### Recommendation — do not scale difficulty off player power

Any power metric you define becomes the thing players optimise against,
and because it feeds difficulty, **gear upgrades stop feeling like
upgrades**. That is a worse failure than lag: it is the rubber-band that
eats progression, and it is very hard to walk back once players notice.
The measurement also does not support the premise that A is badly lagged —
A is a closed-form solve, not a search.

**Propose instead: anticipation from data already collected.**

`recent_win_dps` holds 20 samples and A uses only their **mean**. The
*slope* of that window is exactly the anticipation signal being asked for
— it is what "player power is growing" looks like in the data — and it
requires **no new coupling, no new metric, and nothing players can game**
that they cannot already game by dealing damage.

Proposal, in order of cost:

1. **(LiveTunable, today)** `pacing_window_fights` = 20 at ~6 boss fights/h
   is **over three hours of memory**. Shortening it is the single cheapest
   reduction in lag and needs no code.
2. **(Code)** Fix **F3** — evaluate `base_pool` at the same stage the DPS
   window was sampled at, or normalise samples by their own fight's pool
   so `mean_dps / base_pool` is dimensionless. This is a real correctness
   bug worth ~20% at low stage, and it is cheaper than any new input.
3. **(Code)** Add a slope term to A's solve, once F3 is fixed. Not before
   — extrapolating a mismatched ratio amplifies the mismatch.

---

## 4. Reset restores compiled defaults

**Concrete: at least four deliberately-moved dials silently reverted when
World 2 opened.**

| dial | World 1 (deliberate) | compiled default | World 2 opened at |
|---|---|---|---|
| `hp_max_step_per_fight` | **0.05** | 0.25 | 0.25 |
| `dmg_max_step_per_fight` | **0.05** | 0.15 | 0.15 |
| `hp_multiplier_ceiling` | 1000 | 6.0 | 6.0 |
| `enemy_hp_pool_hard_cap` | 5e16 | 1e15 | 1e15 |

**The two step dials are the dangerous ones.** The owner had slowed both
controllers — A by 5×, B by 3× — almost certainly against the oscillation
`pacing.rs` documents in its own history ("production oscillated in roughly
ten-win / ten-loss swings"). The reset restored the fast, oscillation-prone
values, and nothing reported it.

### Proposal — extend the existing pre-reset check, no code

`world_reset_procedure.md` already defines a pre-reset sweep whose
deliverable is a table committed to `docs/session_journal.md` **before the
reset runs**, with `BUILD` / `DEFER` / `DROP` against every row and no row
left blank. **Add a fourth sweep to it:** diff the live tunables file
against the compiled defaults and emit one row per differing dial, under
the same three decisions.

It is mechanical, it needs no code, and it fits an approved shape rather
than inventing one.

**Rejected alternative: carry the tunables file across resets.** Some
dials are legitimately world-scoped — stage-gated drop thresholds, a pool
cap sized for stage 7000 — and carrying those forward would be its own
silent wrongness in the opposite direction. Forcing a per-dial ruling is
the point; inheriting silently just moves the failure.

---

## 5. Stage-shaped constants for a season that ends

**There are two opposite miscalibrations, and both were correct for World
1.**

**Too early — boss secondary stats saturate almost immediately:**

| stat | formula | saturates at stage |
|---|---|---|
| `crit_multiplier` | `1.4 + s·0.025` (cap 0.9) | **36** |
| `evasion` | `s·0.015` (cap 0.75) | **50** |
| `increased_damage` | `s·0.01` (cap 0.5) | **50** |
| `crit_chance` | `0.05 + s·0.012` (cap 0.75) | **58** |
| `splash` | `s·0.01` (cap 0.6) | **60** |
| `block_chance` | `s·0.010` (cap 0.75) | **75** |
| `damage_reduction` | `s·0.005` (cap 0.75) | **150** |

**Too late — the pacing shapes never engage:**

| curve | knee / first real effect | value at stage 300 |
|---|---|---|
| `top_layer_half_stage` = 1500 | stage 1500 (50% of cap) | **10% mitigation** |
| baseline anchors | >8% departure at stage 500 | **0.952 / 0.964** |

For World 1 at 7,400 stages this was fine: 98% of the world was
post-saturation, and the late curves had thousands of stages to matter.
**For a season living in the low hundreds it is wrong at both ends** — every
boss secondary is pinned at its cap by the halfway mark, so the back half
of the season is pure HP/ATK inflation with static boss texture, while the
floor and the top layer never do anything at all.

### Proposal

Rescale both families to the season length `S`:

- top layer knee at ≈ `S/3`
- last baseline anchor at `S`, with the same relative shape
- boss secondary ramps stretched to reach cap at ≈ `0.8·S`, so boss
  *texture* keeps changing for most of the season

**Blocked on one owner decision, and it is the same one
`world2_build_plan.md:44` already names as undecided: what stage does a
season top out at?** That document calls it "the number that sets the
design point." Every value in this section is a function of `S`, and none
of them can be honestly proposed without it. **This is the single input
that unblocks the whole family.**

**LiveTunable vs code:** `top_layer_half_stage` and all three anchor lists
are LiveTunables — **today, once `S` is known**. The boss secondary ramp
coefficients and their caps are **hardcoded in `boss_stats_for`
(`manager.rs:7532-7601`)** — code, and by the tunables doctrine they
should be LiveTunables regardless.

---

## 6. Ranked by player experience

| # | proposal | ships as | why this rank |
|---|---|---|---|
| **1** | **Re-slow `hp_max_step_per_fight`** (0.25 → ~0.10) | **LiveTunable, today** | **Urgent and new.** The ceiling raise just unpinned A with a step **5× larger** than the owner's own World-1 setting. A is at 6.0 and wants ~12.9 — it arrives in **3.4 fights (~34 min)**, then overshoots into >45 s fights and reverses. Oscillation is what players feel as *random* difficulty, which is worse than consistent-but-wrong. This is a one-field edit that de-risks a change already made. |
| **2** | **Stage curves rescaled to season length** (§5) | LiveTunable (anchors, top layer) + code (boss ramps) | Shapes the entire season's texture at every stage. Blocked only on `S`. |
| **3** | **Ceiling-saturation visibility** (§2) | code, small | Does not change a fight, but converts a silent multi-week wrongness into a same-day fix. Force-multiplies everything below. |
| **4** | **Fix F3, then shorten A's window** (§3) | LiveTunable (window) + code (F3) | ~20% denominator error at the stage the world lives at; felt as difficulty that does not match what the party is doing. |
| **5** | **Cost telemetry, then an event budget** (§1) | code | Protects the end of the season. Nothing is at risk today at 1.16 MB/fight — but there is currently no instrument at all. |
| **6** | **Pre-reset tunable-diff sweep** (§4) | **procedure, no code** | Zero risk, ships today, and prevents the next season opening mis-tuned the way this one did. |
| **7** | **Do NOT read player power** (§3) | — | Recorded as a recommendation *against*, with the reason: it makes gear upgrades stop feeling like upgrades. |

### Ships today without a deploy (LiveTunables)
`hp_max_step_per_fight`, `dmg_max_step_per_fight`, `pacing_window_fights`,
`top_layer_half_stage`, the three anchor lists, `hp_multiplier_ceiling`,
`enemy_hp_pool_hard_cap`.

### Needs code (queues behind the three branches)
Ceiling-saturation flags + admin rendering; F3's stage-matched pool; A's
slope term; cost telemetry and event budget; boss secondary ramps promoted
to LiveTunables; the missing sample-before-compress regression test.

---

## 7. What I need ruled

1. **What stage does a season top out at (`S`)?** Blocks all of §5.
2. **Re-slow the step dials now?** (#1 above — recommend yes, today.)
3. Is surfacing saturation without coordinating the two axes the behavior
   you want when one axis cannot deliver (§2)?
4. Do you accept the recommendation *against* pacing reading player power
   directly (§3)?

---

# 8. REVISION — the resistance model (owner ruling, 2026-09-03)

**Supersedes the framing of §5 and re-ranks §6.** The stationary fixed
point is **intentional design**, not a defect. The world resists at 2:1;
stage progress is *earned* by improving builds, items and characters
faster than the controllers can compensate. §1-§4 and §7 stand unchanged.

**Do not design a way to make the world climb. Design the resistance.**

## 8.1 The stage walk IS Controller B's setpoint

Win = +1 stage, loss = −2, so net stage per boss fight is exactly

> **net = 3p − 2**, where `p` = boss win rate

which is **zero at p = 2/3 — precisely B's `target_win_loss_ratio` of
2.0.** B is not merely correlated with the stationary point; B *is* the
mechanism that enforces it. This is the whole design in one line.

| win rate | net stage/fight | stages/day (≈144 boss fights) |
|---|---|---|
| **66.7%** (setpoint) | 0.000 | **0** |
| 68.0% | 0.040 | 5.7 |
| **68.3%** (measured) | 0.049 | **7.0** |
| 70.0% | 0.100 | 14.4 |
| 72.0% | 0.160 | 23.0 |
| 75.0% | 0.250 | 35.9 |

## 8.2 Breakthrough rate is set by B's gain, and only B's gain

B's per-fight compensation is `dmg_step × tanh(ln(ratio/2))`. Equilibrium
is where B's compensation equals the party's power growth `g` per boss
fight, giving a closed form for the earned win rate:

> **ratio = 2·exp(atanh(g / dmg_step))**

Inverting the measured 7.0 stages/day gives **g ≈ 0.0112 per boss fight**
today (a fresh world: levels and first items, so this is the fastest `g`
the season will ever see). The model reproduces the observed rate exactly,
which is the check that it is the right model.

| g/fight | `dmg_step` 0.15 (live) | 0.10 | 0.05 (World 1) |
|---|---|---|---|
| **0.0112** (today) | p=0.683, **7.1/day** | p=0.691, 10.6/day | p=0.715, 20.9/day |
| 0.0050 | p=0.674, 3.2/day | p=0.678, 4.8/day | p=0.689, 9.4/day |
| 0.0020 | p=0.670, 1.3/day | p=0.671, 1.9/day | p=0.676, 3.8/day |
| 0.0010 | p=0.668, 0.6/day | p=0.669, 1.0/day | p=0.671, 1.9/day |

**`dmg_max_step_per_fight` is the season-pace dial.** Lower gain = the
world concedes more win rate for the same player effort = faster
breakthrough. It is a LiveTunable: **the season's difficulty ships today,
without a deploy.**

## 8.3 What the two changed parameters actually did

**`hp_multiplier_ceiling` 6 → 50: better fights, same season pace.**
A targets *duration*; B targets *win rate*; the stage walk is neutral at
B's setpoint. So **A cannot change the equilibrium breakthrough rate — B
owns it.** Unpinning A fixes fight *texture* (~17 s bosses → the 30-45 s
band) and B will absorb the difficulty by easing `boss_power_mult` back
down from 2.11. **Checkable prediction: `boss_power_mult` falls over the
next day while the stage rate returns to ~7/day.** If it does not, this
model is wrong.

**The transient is the risk, and it is not oscillation — it is stage
loss.** A doubles boss HP in **3.4 fights (~34 min)** at step 0.25, while
B needs its **20-fight window** to even observe the consequence — a
**5.8:1 timescale mismatch.** In between, the party loses, and every loss
walks stage **−2**, floored at 1. From stage 8 a bad transient can erase
the season's progress to date. A's relaxation valve (3 consecutive losses,
0.20/fight decay) then drags A back *down*, undoing the climb — so the
fast step converts a one-time correction into a repeating cycle.

## 8.4 Revised recommendation on `hp_max_step_per_fight`: 0.05, not 0.10

**This is a change from §6 #1, and the reason changed with it.** `hp_step`
is *not* a season-pace parameter — B's gain is. It is a **transient
coupling** parameter, and the principled constraint is:

> A's slew time must be at least B's observation window, or the two
> controllers fight each other while B is still blind.

Spreading A's ×2.15 move over B's 20-fight window gives **0.039**; over 15
fights, **0.052**.

| `hp_step` | fights to travel | vs B's 20-fight window |
|---|---|---|
| 0.25 (live) | 3.4 | **5.8:1 mismatch** |
| 0.10 (my earlier pick) | 8.0 | 2.5:1 |
| **0.05** (World 1) | 15.7 | **1.3:1 — matched** |

The derivation lands on **0.05 — the value the owner already used in World
1.** My earlier 0.10 was a hedge against oscillation; under the correct
framing the constraint is sharper and the answer is lower.

## 8.5 The gates at 100 / 150 / 300

Days from stage 8, with `g` decaying at half-life `H` (fresh-world growth
does not last):

| scenario | gate 100 | gate 150 | gate 300 |
|---|---|---|---|
| step 0.15, no decay | 13 d | 20 d | 41 d |
| step 0.15, H = 28 d | 16 d | 28 d | **NEVER** |
| step 0.15, H = 14 d | 21 d | 89 d | **NEVER** |
| step 0.15, H = 7 d | **NEVER** | **NEVER** | **NEVER** |
| step 0.05, no decay | 4 d | 7 d | 14 d |
| step 0.05, H = 28 d | 5 d | 7 d | 17 d |
| step 0.05, H = 14 d | 5 d | 8 d | 23 d |
| step 0.05, H = 7 d | 6 d | 11 d | **NEVER** |

**Where the world stalls at the live step of 0.15**, once growth decays:

| power-growth half-life | world stalls at stage |
|---|---|
| 28 days | **~295** |
| 14 days | **~152** |
| 7 days | **~80** |

**The answer to "is divine dust at 300 a mid-season goal or a thing nobody
sees" is: at the live `dmg_step` of 0.15 it is a thing nobody sees**, in
every scenario where player power growth decays at all. Stage 300 at 0.15
requires growth that essentially never slows. At 0.05 it is a 2-3 week
goal and survives realistic decay.

This is not an argument for moving the gates. It is an argument that
**`dmg_max_step_per_fight` and the gate stages are the same decision**,
and they are currently being made independently.

## 8.6 What §5's finding means under this framing

The stage-shaped rescale proposed in §5 is withdrawn — season pace is not
set by stage-shaped curves. **The saturation finding survives, restated:**
boss secondaries cap at stage **36-150** (crit mult 36, evasion 50, crit
chance 58, splash 60, block 75, DR 150). Past ~150 the world's resistance
becomes **purely scalar** — bigger HP and ATK numbers, no new boss
behaviour. Under "design the resistance," that is a resistance-*quality*
question: the world stops finding new ways to fight back exactly in the
range the gates live in. Worth deciding, not urgent, and orthogonal to
pace.

The anchors and `top_layer_half_stage` are simply **inert** in the
reachable range and can be left alone. If the owner wants stage-tied
mitigation to be part of the resistance, the top layer's knee belongs
near the middle of the reachable band (~80-300), not at 1500.

## 8.7 Re-ranked

| # | proposal | ships as | why |
|---|---|---|---|
| **1** | **Set `dmg_max_step_per_fight` deliberately** — it is the season-pace dial (§8.2), and it decides whether the 300 gate is ever seen (§8.5) | **LiveTunable, today** | The single highest-leverage number in the game under this framing, currently sitting at a compiled default nobody chose |
| **2** | **`hp_max_step_per_fight` → 0.05** (§8.4) | **LiveTunable, today** | Protects earned stage during A's in-flight ×2.15 correction; −2 per loss from stage 8 is unforgiving |
| **3** | Ceiling-saturation visibility (§2) | code, small | A silent rail is what let ~17 s bosses run unnoticed |
| **4** | Fix F3, then shorten A's window (§3) | LiveTunable + code | ~20% denominator error at the live stage |
| **5** | Cost telemetry, then event budget (§1) | code | No instrument exists; nothing at risk today |
| **6** | Pre-reset tunable-diff sweep (§4) | procedure | Both step dials — now known to be *the difficulty* — reverted silently at this reset |
| **7** | Do **not** read player power (§3) | — | Unchanged |

## 8.8 What I need ruled (replaces §7)

1. **What breakthrough rate do you want**, in stages/day? That single
   number sets `dmg_max_step_per_fight` via §8.2, and §8.5 says whether
   your gates are reachable under it.
2. **`hp_max_step_per_fight` → 0.05 now?** (§8.4 — recommend yes, today.)
3. Items 3-7 of §8.7 are unchanged from the previous pass and still need a
   go/no-go.

---

# 9. Season pace solved, and the ceiling prediction checked

**Owner rulings received 2026-09-03:** `hp_max_step_per_fight` → 0.05
(accepted, reasoning in §8.4); season pace → **+30% on World 1**.

## 9.1 The prediction from §8.3 is CONFIRMED

Measured live before the ceiling change, and again after:

| | before | after | §8.3 predicted |
|---|---|---|---|
| `hp_pacing_mult` (A) | 6.0 (at ceiling) | **11.71875** | A's request 11.5–12.9 ✅ |
| `boss_power_mult` (B) | 2.1135 | **1.6631** (−21.3%) | falls ✅ |
| stage | 8 | **6** | transient stage loss ✅ |
| boss window | 12W/8L (p=0.60) | 11W/9L (p=0.55) | B still easing |

**A landed inside the predicted band**, and it did so in exactly three
rate-limited steps: `6.0 × 1.25³ = 11.71875` — the arithmetic is exact,
which confirms both the rate limit and §8.3's "3.4 fights" estimate.
B is easing as predicted. The model stands.

**§8.3's transient warning also came true:** stage fell 8 → 6 while A
travelled. That is the cost of `hp_max_step` at 0.25, and it is the reason
Ruling 1 is right.

## 9.2 SEQUENCING — do not set `dmg_max_step_per_fight` yet

**The transient is still running.** The current window is p = 0.55, well
below the 2/3 setpoint, so net stage is **3(0.55) − 2 = −0.35 per fight**
— about **−2.1 stage/hour** — and the stage floor is **1**, from a current
stage of **6**.

B is easing at `0.15 × tanh(ln(1.222/2))` = **6.8% per fight** and needs
roughly 10–15 fights (~2 h) to re-equilibrate.

**Lowering `dmg_step` to 0.0384 now would slow B's recovery by ~3.9×**, to
40–60 fights (7–10 h) of continued bleed from stage 6. **That floors the
world at stage 1.**

**Order of operations:**

1. Set `hp_max_step_per_fight` = **0.05** now. It protects the *next*
   correction and does not affect the recovery in progress (A has already
   arrived).
2. **Wait** for `boss_power_mult` to stabilise and the boss win rate to
   return to ≈ 2/3.
3. *Then* set `dmg_max_step_per_fight` = **0.0384**.

## 9.3 The exact solve

Recovered growth constant, from the measured 7.0 stages/day at
`dmg_step` 0.15: **g = 0.01108030 per boss fight** (≈ ×4.9 player power
per day — a fresh world, so this is the fastest `g` the season will see).

World 1's pace at `dmg_step` 0.05 is **20.722 stages/day**, so +30% is
**26.939 stages/day**. Inverting
`ratio = 2·exp(atanh(g / dmg_step))`:

> p = 0.729155 → ratio = 2.692149 → **`dmg_max_step_per_fight` = 0.038374**

**Value to enter: `0.0384`** → **26.92 stages/day (+29.9% on World 1)**.

| candidate | stages/day | vs World 1 |
|---|---|---|
| 0.0380 | 27.20 | +31.3% |
| 0.0383 | 26.99 | +30.3% |
| **0.0384** | **26.92** | **+29.9%** |
| 0.0390 | 26.51 | +27.9% |

**Range:** the form is `<input type="number" step="any" min="0">` and the
server clamps `[0.0, 100.0]` (`adventure_web.rs:3438`). `0.0384` is well
inside range and `step="any"` accepts the full precision. **No rounding
into a lie.**

**B saturation is not a risk at this value.** B only loses proportional
control if `g ≥ dmg_step`; at 0.0384 that needs **×225 player power per
day** against today's ×4.9. Dismissed with the number.

## 9.4 Gates at `dmg_step` = 0.0384

Days from stage 8:

| growth decay | sand 100 | perfect 150 | divine/sacred 300 |
|---|---|---|---|
| none (sustained) | 3.5 d | 5.3 d | **10.9 d** |
| half-life 28 d | 3.6 d | 5.7 d | **12.6 d** |
| half-life 14 d | 3.7 d | 6.1 d | **15.5 d** |
| half-life 7 d | 4.2 d | 7.4 d | **NEVER** |

**Stage 300 goes from "a thing nobody sees" to a 11-16 day mid-season
destination** in every scenario except the most severe decay. That is the
single biggest gameplay consequence of this ruling.

## 9.5 A 30-day season at this pace

| growth decay | stage at 30 d | eventual stall |
|---|---|---|
| none (sustained) | **816** | unbounded |
| half-life 28 d | **580** | ~1107 |
| half-life 14 d | **432** | ~558 |
| half-life 7 d | **269** | ~283 |

## 9.6 FLAG — the flat regime becomes the season

At 27 stages/day, **stage 150 arrives on day 5.3**, and the world spends
**75–82% of a 30-day season above it**:

| growth decay | time above stage 150 in 30 days |
|---|---|
| none | 24.7 d (**82%**) |
| half-life 28 d | 24.3 d (**81%**) |
| half-life 14 d | 23.9 d (**80%**) |
| half-life 7 d | 22.6 d (**75%**) |

Above stage 150 **every** boss secondary is pinned at its cap — crit
multiplier (36), evasion (50), increased damage (50), crit chance (58),
splash (60), block (75), damage reduction (150). Only `hp` (+15%/stage)
and `atk` (+10%/stage) still move. **Boss behaviour is frozen; only the
numbers grow.**

**Honest sizing.** This is a *quality* problem, not a functional one — the
resistance still works, because A and B scale exactly the hp/atk terms
that keep growing. Nothing breaks. But the texture of every fight from
day 5 to day 30 is identical, and at 27/day that is the season rather
than its tail.

**The ruling does not cause this — it exposes it.** Stage 150 is reached
within a 30-day season at *every* pace considered: day 20.3 even at the
current 0.15. The pace only decides whether the flat regime is a third of
the season or four fifths of it.

| `dmg_step` | stages/day | stage 150 lands |
|---|---|---|
| 0.0384 | 26.92 | day 5.3 |
| 0.05 | 20.72 | day 6.9 |
| 0.08 | 13.03 | day 10.9 |
| 0.10 | 10.45 | day 13.6 |
| 0.15 (live) | 7.00 | day 20.3 |

**What it would take to fix.** The seven ramp coefficients and their caps
are hardcoded in `boss_stats_for` (`manager.rs:7532-7601`); nothing about
boss secondary *shape* is tunable today (`boss_health` and `boss_power`
are scalar multipliers on hp/atk only). Two options:

1. **Stretch the ramps** so they reach cap near stage 600-800 instead of
   36-150 — a coefficient change, still **code + deploy**.
2. **Promote the seven ramps and their caps to LiveTunables** — the
   tunables-doctrine-correct fix, and the standard "add a tunable field in
   four places" shape. Larger, but it makes boss texture a dial forever.

**Scheduling consequence, and it is the real point:** the pace ships today
on a dial, but the work that makes a fast season *interesting* is code
that is not queued. Recommend promoting this above §8.7 items 3-7. If it
cannot be scheduled soon, option (b) in §9.7 is the honest alternative.

## 9.7 Two ways to take this

**(a) Ship 0.0384 now.** The owner gets the pace he asked for and the 300
gate becomes reachable, at the cost of a texturally flat season from day
5. Recommended **if** the boss-secondary work can be scheduled inside the
season.

**(b) Ship an interim 0.08 (13.0/day, stage 150 at day 10.9), then go to
0.0384 once the ramps are stretched.** Half the requested speedup now,
the full pace when the content can carry it.

**Recommend (a) with the boss-secondary work prioritised**, because the
owner asked for a pace and the flatness is fixable without touching pace.
But it is his call, and (b) is the honest alternative rather than a
hedge — stated so he can choose knowing what each buys.

---

# 10. QUEUED WORK — boss secondary ramps (top content priority)

**Status: QUEUED. Not a proposal to act on now. No code is to be written
from this section until it is scheduled.** Owner ruling 2026-09-03: ship
`dmg_max_step_per_fight` = 0.0384, and this becomes the top content
priority.

Recorded so the work can be picked up cold by a session that has not read
this conversation.

## 10.1 What the work is, exactly

Seven hardcoded stat ramps in `boss_stats_for`
(`game/src/adventure/manager.rs:7532-7601`). Each has the shape
`min(stage × slope, cap) × jitter`:

| stat | slope | cap | frozen from stage |
|---|---|---|---|
| `crit_multiplier` | 0.025 | +0.90 (over a 1.4 base) | **36** |
| `evasion` | 0.015 | 0.75 (`BOSS_DEFENSE_CAP`) | **50** |
| `increased_damage` | 0.010 | 0.50 | **50** |
| `crit_chance` | 0.012 | 0.75 (`CRIT_CHANCE_CAP`) | **58** |
| `splash` | 0.010 | 0.60 | **60** |
| `block_chance` | 0.010 | 0.75 (`BOSS_DEFENSE_CAP`) | **75** |
| `damage_reduction` | 0.005 | 0.75 (`BOSS_DEFENSE_CAP`) | **150** |

> **CORRECTION 2026-09-03 (implementing session, accepted by the owner).**
> **The `crit_chance` row above is wrong in one cell.** Its cap is
> **0.70, not 0.75**, and the formula has a **flat 0.05 base the table
> omits**: the code is `(0.05 + s * 0.012).min(CRIT_CHANCE_CAP)`, so the
> *ramp* ceiling is `CRIT_CHANCE_CAP` less that base. The rest of §10 is
> already consistent with 0.70 — the listed freeze stage 58 is
> `0.70/0.012`, the "today frozen" column reads 0.700, and every cell of
> §10.3's and §10.7's value tables checks out — so this is a single-cell
> error. It mattered because a session implementing
> `CRIT_CHANCE_CAP * s/(s+h)` literally would have **dropped the 0.05
> base** and missed this document's own numbers. As shipped, the base
> sits OUTSIDE the curve (`BOSS_CRIT_CHANCE_BASE`) and the ramp climbs to
> `BOSS_CRIT_CHANCE_RAMP_CAP` (0.70), so base + ramp approaches exactly
> `CRIT_CHANCE_CAP` and the clamp stays a rail rather than the shape.
>
> The other six rows were verified against the code and **reproduce
> exactly**, as do the freeze stages.

Above stage 150 **all seven are pinned**. Only `hp` (+15%/stage) and `atk`
(+10%/stage) still move, and both are unbounded.

Nothing about boss secondary *shape* is tunable today. `boss_health` and
`boss_power` are scalar multipliers on hp/atk only.

## 10.2 What it costs the player — the argument for prioritising

**Progress becomes invisible, and that is the whole of it.**

Dynamic pacing's job is to hold the *experience* constant while the
numbers grow: A holds fight duration in the 30-45 s band, B holds the win
rate at 2:1. That is correct and intentional — it is the resistance model
working. But it has a consequence nobody chose:

> A stage-700 fight already lasts the same time and is won just as often
> as a stage-200 fight. The controllers guarantee that. **Boss behaviour
> is the only axis left on which the late season could feel different —
> and it is frozen from stage 150.**

The controllers deliberately remove every other source of variation, so
the one remaining source carries the entire load. Right now it carries
none. **A player at stage 700 has no sensory evidence they are not at
stage 200 except the number on the counter.**

Three consequences that follow:

1. **Counterplay stops developing.** Evasion pinned at 0.75 from stage 50
   means accuracy and pierce investment have a fixed value for the rest of
   the season. The build-optimisation problem is solved once, around day
   2, and stays solved.
2. **No threat ever debuts.** Nothing a boss does at stage 700 differs
   from stage 200. There is never a "this boss finally does X" moment
   after the first two days.
3. **The gates become the only content.** Sand 100, perfect items 150,
   divine dust and sacred 300 are the only things that change after day 5.
   Between and beyond them, nothing changes at all.

At the approved pace this is **75-82% of a 30-day season** (§9.6).

**And the decisive point for scheduling: this is difficulty-neutral to
fix.** Because A and B close the loop on difficulty, changing boss
secondary shape *cannot* make the season easier or harder in aggregate —
the controllers re-equilibrate to 2:1 and 30-45 s regardless. It changes
texture only. That makes it an unusually safe content change: it cannot
break balance, and it is the only lever that adds variety without
touching pace.

## 10.3 The shape they should have

**Replace `min(s × slope, cap)` with `cap × s/(s + h)`** — the saturating
form, approaching the cap asymptotically instead of hitting a corner.

**This is not a new shape. It is `top_layer_for_stage`'s
(`pacing.rs:741`): `cap × s/(s + half)`, already in the codebase, already
tested, and already carrying a LiveTunable half-stage
(`top_layer_half_stage`).** Copy that precedent rather than inventing one.

**Default `h = cap / slope`, which preserves today's behaviour exactly at
the low end.** The derivative of `cap·s/(s+h)` at `s = 0` is `cap/h`, so
`h = cap/slope` reproduces the current slope precisely — and it happens to
equal each stat's current freeze stage, so **the defaults are the numbers
already in the code, reinterpreted.** No new constants to invent, and the
first two days of a season are unchanged.

Values under that default:

| stat | h | s=150 | s=300 | s=500 | s=800 | s=1500 |
|---|---|---|---|---|---|---|
| `damage_reduction` | 150 | 0.375 | 0.500 | 0.577 | 0.632 | 0.682 |
| `block_chance` | 75 | 0.500 | 0.600 | 0.652 | 0.686 | 0.714 |
| `evasion` | 50 | 0.562 | 0.643 | 0.682 | 0.706 | 0.726 |
| `increased_damage` | 50 | 0.375 | 0.429 | 0.455 | 0.471 | 0.484 |
| `crit_chance` | 58 | 0.504 | 0.586 | 0.627 | 0.652 | 0.674 |
| `crit_multiplier` | 36 | 0.726 | 0.804 | 0.840 | 0.861 | 0.879 |
| `splash` | 60 | 0.429 | 0.500 | 0.536 | 0.558 | 0.577 |

against today's, which are a single frozen column from stage 150 on
(0.750 / 0.750 / 0.750 / 0.500 / 0.700 / 0.900 / 0.600).

**Placement rule for tuning:** the curve reaches 50% of cap at `h`, 80% at
`4h`, 90% at `9h`. So to have a stat still visibly developing at the stage
a season actually reaches, set `h ≈ S_top / 4`. For a 30-day season
reaching ~600-800 (§9.5), that is `h` in the 150-200 range — roughly
**3× the behaviour-preserving defaults.** The defaults are the safe
starting point; the stretch is the tuning the owner will actually want.

**One caveat to state honestly:** the asymptotic form is *always* below
the old value above the freeze stage, so bosses are numerically weaker in
the 150+ band than today. That is difficulty-neutral by §10.2 — A and B
raise hp/atk to compensate — but it does shift resistance out of
secondary stats and into raw scaling in the short term, which is the
opposite of the goal until `h` is stretched. **Ship the stretch with the
shape change, not after it.**

> **CORRECTION 2026-09-03 (implementing session, accepted by the owner).**
> **The sentence above is backwards, and it was load-bearing.** Stretching
> `h` does not cure the "resistance moves into raw hp/atk" effect — it
> *maximises* it. `cap·s/(s + h)` is **monotonically decreasing in `h`**,
> so every stretch lowers every value at every stage. At stage 800,
> damage reduction is 0.632 at the behaviour-preserving `h = 150` but
> **0.545** at the stretched `h = 300`, against today's frozen 0.750. The
> stretch is offered here as the cure for exactly the thing it makes
> worse.
>
> **The correct reasoning.** The real case for a stretch is a different
> one, and it survives intact: a larger `h` keeps a stat **visibly
> moving** deeper into a season instead of saturating early, which is
> §10.2's actual goal. That is a genuine argument — but it is a *tuning*
> decision to make from observation once the curve is live, not a guess
> baked into a release. It trades absolute boss strength at every stage
> for continued movement at high stages, and nothing in this document
> measures where that trade should sit.
>
> See the ruling recorded against §10.7, which supersedes the "ship the
> stretch with the shape change" instruction.

## 10.4 Should they be tunable? Yes — seven half-stages

**Promote the seven `h` values to LiveTunables.** Precedent is exact:
`top_layer_half_stage` is one LiveTunable for one curve of this shape, so
seven curves take seven. This is the standard "add a tunable field in four
places" change the CLAUDE.md efficiency rule describes, ×7.

**Keep the caps as compile-time constants.** `BOSS_DEFENSE_CAP` (0.75,
shared by evasion/block/DR) and `CRIT_CHANCE_CAP` (0.75) are structural
safety limits — they are what stops a high enough stage making a boss
literally unhittable — and qualify for the Decision-16 shared-constant
exception, same as `TOP_LAYER_ABSOLUTE_CAP` and `BOSS_DEFENSE_CAP` already
do. The three unshared caps (`increased_damage` 0.50, `crit_multiplier`
+0.90, `splash` 0.60) are tuning values rather than safety rails and
should become tunables in a follow-on, but they are not needed to fix the
flatness and should not expand this item's scope.

**Rejected: a single `boss_secondary_stretch` multiplier applied to all
seven.** One dial is cheaper, but it forbids exactly the tuning that makes
this worth doing — e.g. evasion arriving early as a build check while
damage reduction arrives late as a scaling wall. Per-stat control is the
point.

## 10.5 Scope, and what a session picking this up must check

- **Seven LiveTunable fields**, each in the four standard places, plus the
  admin form.
- **`#[serde(default)]` on every new `TunablesForm` field** — the trap
  named in CLAUDE.md, which has already bitten twice. And the form-POST
  test must derive its field set from the rendered page, per the
  `admin_tunables_splash_http.rs` shape.
- **Golden corpus will move.** Every fixture whose scenario runs at stage
  ≥ 36 changes, because every boss secondary changes. Report mismatches
  with attributed causes; regeneration happens at merge, not on the
  branch.

  > **CORRECTION 2026-09-03 (implementing session, accepted by the
  > owner). This bullet is wrong, and it removed a stated blocker.** The
  > golden corpus does **not** move. `golden_corpus.rs` says so outright
  > in its own header: *"Boss stats are hand-authored fixed values, NOT
  > `boss_stats_for`/`basic_enemy_stats_for` — those two apply their own
  > un-seeded `rand::thread_rng()` jitter."* Fixtures build `BossStats`
  > through local `boss()`/`tough_boss()` helpers, and `boss_stats_for`
  > has exactly two callers, both production. **Confirmed by running the
  > corpus, not asserted:** it passed unchanged on the implementing
  > branch, with zero fixtures regenerated. An un-seeded RNG could never
  > have been captured into a golden fixture in the first place — that is
  > why the exclusion exists.
- **`WIKI_IMPACT.md` line required** — this is player-facing boss
  behaviour, and the wiki renders boss stats.
- **Verify against a live click-through**, not a code trace: boss stat
  rendering at several stages on the admin page.

## 10.6 What this does NOT address

The seven ramps are boss *stat* shape. They do not add new boss
*mechanics* — no new ability, phase, or behaviour. If the owner wants a
stage-700 boss to do something a stage-200 boss cannot, that is separate
content work and is not in this item. This item makes the existing
numbers keep moving; it does not invent new ones.

> **ADDITION 2026-09-03 (implementing session; RULED by the owner —
> accept and record, do not widen scope).**
>
> **The controllers partially undo this change on the three defensive
> stats, and §10 does not model that feedback path.**
> `apply_dynamic_scaling` multiplies the secondaries by
> `sqrt(dmg_mult)` and then re-caps at `BOSS_DEFENSE_CAP` (0.75). Weaker
> bosses → higher win rate → Controller B raises `dmg_mult` → the
> secondaries multiply straight back into the cap:
>
> | stat | s=300 raw | ×√2 | ×√4 |
> |---|---|---|---|
> | `evasion` | 0.562 | 0.750 **pinned** | 0.750 **pinned** |
> | `block_chance` | 0.500 | 0.707 | 0.750 **pinned** |
> | `damage_reduction` | 0.375 | 0.530 | 0.750 **pinned** |
>
> **So for evasion, block and damage reduction the unfreezing only holds
> while B is near baseline, and the flatness returns whenever B runs hot
> — which is exactly when it matters most.** `increased_damage`,
> `crit_multiplier` and `splash` are **unaffected**: their post-scaling
> caps (10.0 and 6.0, and splash's own 1.0) sit far above anything the
> organic ramp produces.
>
> **Ruled out of scope, deliberately, and both rejections are on the
> record:**
> - **Do NOT raise `BOSS_DEFENSE_CAP`.** It is a safety rail that stops
>   an unhittable boss. Raising a safety rail as a side effect of a
>   variety change is how rails stop meaning anything.
> - **Do NOT exempt secondaries from `sqrt(dmg_mult)`.** That is a real
>   change to how the controllers interact with boss composition and it
>   deserves its own pass.
>
> **This is on the board as its own item, to be decided deliberately
> later.** It is also surfaced to the operator: the "Boss Secondary
> Curves" admin section carries this limitation in its own hint text, so
> nobody tunes the three defensive dials without knowing the ceiling can
> take the movement back.

## 10.7 The placement rule needs an `S_top` that does not exist — ship provisional

**Approved 2026-09-03 with the stretch bundled into the same change.** The
recorded reasoning for the approval, because the verdict alone is not the
argument:

1. **It reuses `top_layer_for_stage`'s asymptotic form rather than
   inventing one** — the shape and its tunable-half-stage precedent are
   already in the tree.
2. **Defaults that reproduce today's slope at `s = 0` and equal each
   stat's freeze stage mean shipping it changes nothing until a dial
   moves** — the safest possible landing.
3. **Difficulty-neutrality is the strongest part.** Because A and B close
   the loop, secondary shape cannot make the season easier or harder. This
   is variety with no balance risk attached.

And the caveat from §10.3 is **ratified**: the stretch ships **with** the
shape, never after. Shipping the curve alone moves resistance out of
secondaries and into raw hp/atk — the exact flatness this work exists to
fix, made worse in the name of fixing it.

### The problem with `h ≈ S_top/4`

**There is no `S_top`. Seasons end on time, not at a stage** (owner
ruling). The rule needs a number the design deliberately does not have.

The only available substitute is §9.5's projection of where a 30-day
season lands, and it is wide:

| growth decay | stage at 30 d | implied `S_top/4` |
|---|---|---|
| half-life 7 d | 269 | 67 |
| half-life 14 d | 432 | 108 |
| half-life 28 d | 580 | 145 |
| none (sustained) | 816 | 204 |

**A 3.0× spread**, so `S_top/4` spans **67 to 204**. The spread is
multiplicative, so the correct central estimate is the geometric midpoint:
`sqrt(269 × 816)/4 = ` **117**, not the arithmetic 136.

### Shipping values

Applying a **common stretch factor to the behaviour-preserving defaults**,
rather than a uniform `h`, preserves the per-stat ordering that §10.4
argues is the point of per-stat control (evasion early as a build check,
damage reduction late as a scaling wall). The median default `h` is 58, so
the factor that puts the middle of the set at 117 is **k = 2.02 → ship
k = 2**:

| stat | `h` (behaviour-preserving) | **`h` SHIPPED (×2)** | s=150 | s=300 | s=500 | s=800 |
|---|---|---|---|---|---|---|
| `crit_multiplier` | 36 | **72** | 0.608 | 0.726 | 0.787 | 0.826 |
| `evasion` | 50 | **100** | 0.450 | 0.562 | 0.625 | 0.667 |
| `increased_damage` | 50 | **100** | 0.300 | 0.375 | 0.417 | 0.444 |
| `crit_chance` | 58 | **117** | 0.394 | 0.504 | 0.568 | 0.611 |
| `splash` | 60 | **120** | 0.333 | 0.429 | 0.484 | 0.522 |
| `block_chance` | 75 | **150** | 0.375 | 0.500 | 0.577 | 0.632 |
| `damage_reduction` | 150 | **300** | 0.250 | 0.375 | 0.469 | 0.545 |

Every column moves. Today, every one of these is a frozen constant above
stage 150.

### These defaults are PROVISIONAL — guessed, not derived

**Stated explicitly so no later reader mistakes them for a derivation.**

`k = 2` is the midpoint of a projection with a **3× spread**, taken from a
growth-decay half-life that **has not been measured** — the season is four
days old and `g` has only been sampled at its fresh-world maximum. It is
not derived from a known season length, because no season length exists to
derive it from. **It is the least-wrong guess available on the day, and
nothing more.**

**REVISIT NOTE 2026-09-03:** the revisit below still stands, but it is
now a revisit of **k = 1**, not of k = 2 — see the superseding ruling at
the end of this section.

**Revisit at the two-week mark**, when the season's actual trajectory is
observable: by then `g`'s decay is measurable from the stage history, and
`S_top` for a 30-day season can be projected from data instead of from a
four-point scenario table. If the world is tracking the 7-day-half-life
path, `k = 2` is roughly double what it should be; if it is tracking the
sustained path, it is roughly half.

**These are LiveTunables, and that is the whole argument for shipping a
provisional value rather than waiting for certainty.** Revisiting costs a
dial move and no deploy. Waiting for a known `S_top` costs the entire
first season of the flat regime the work exists to fix.

## 10.8 SUPERSEDING RULING 2026-09-03 — ship k = 1, not k = 2

**This section replaces §10.7's shipping values and §10.3's "ship the
stretch with the shape change" instruction. Both of those remain above,
unedited and dated, because a reader chasing an old citation must land on
what was actually written.**

**Shipped: `h = cap / slope`, k = 1 — the behaviour-preserving set.**

| stat | shipped `h` | constant |
|---|---|---|
| `crit_multiplier` | 36 | `BOSS_CRIT_MULT_HALF_STAGE` |
| `evasion` | 50 | `BOSS_EVASION_HALF_STAGE` |
| `increased_damage` | 50 | `BOSS_INCREASED_DAMAGE_HALF_STAGE` |
| `crit_chance` | 58.33 | `BOSS_CRIT_CHANCE_HALF_STAGE` |
| `splash` | 60 | `BOSS_SPLASH_HALF_STAGE` |
| `block_chance` | 75 | `BOSS_BLOCK_HALF_STAGE` |
| `damage_reduction` | 150 | `BOSS_DR_HALF_STAGE` |

**Why the decision changed.** §10.7's approval was ordered on a reason
that is backwards — see the correction recorded against §10.3. The
stretch was offered as the cure for resistance shifting into raw hp/atk,
and it is in fact the thing that maximises that shift. The owner
approved the stretch repeating that reason as load-bearing, and
disregarded the instruction once the error was shown.

**The deciding argument is §10.7's own approval reason #2**, which is the
property that made this change safe in the first place:

> at **k = 1**, shipping genuinely **changes nothing below the freeze
> stage and unfreezes everything above it**.

That is the shape of every change that has gone well on this project:
ship the mechanism at defaults that alter nothing, then tune on a dial
with real data. **At k = 2 that property is false** — every one of the 42
cells in §10.7's table is lower than today, and boss secondaries below
stage 100 are roughly halved, which is precisely the stage range the live
world occupies right now.

**The case for a stretch is not rejected — it is deferred to
observation.** Keeping stats visibly moving all season rather than
saturating is a good argument and it survives intact. It is a tuning
decision to make once the curve is live, not a guess baked into a
release. **That is what the seven dials are for**, and the admin section
says as much in its own hint text: it states the placement rule, states
that the shipped defaults are PROVISIONAL and are the old ramps
re-expressed rather than a tuned set, and states that the design's own
placement rule wants them roughly 2× larger.

**k = 1 was chosen DELIBERATELY, not by omission.** Recorded here, in the
constants' doc comments, and in the shipping commit message, so that a
later reader finding `h = 150` for damage reduction does not mistake it
for the stretch never having been applied.
