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
