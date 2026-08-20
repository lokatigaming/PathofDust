<div class="card wiki-wide">

## Combat

This is the shared math every fight runs on - world bosses, filler encounters,
every class, every skill. If you've ever wondered "why did that hit for so
much" or "why did I get evaded four times in a row," it's all here.

Fights aren't turn-based. Every combatant has their own attack clock ticking
independently, and whoever's clock is soonest acts next - see
[Attack Speed &amp; Timing](#timing) below.

<h3 id="hit-resolution">Hit Resolution</h3>

An attack doesn't always land. Every hit rolls **evasion** first - if it
dodges, nothing else happens: no damage, no crit, no on-hit effects. If it
connects, the attack then rolls for a critical hit, gets halved by a block if
one triggers, and finally gets cut down by damage reduction. There's no
separate "accuracy" stat anywhere in the game - evasion *is* the miss chance.

A handful of skills across the classes grant genuinely unavoidable hits (can't
be evaded, blocked, or reduced) for a limited window - those are called out on
the skill's own tooltip in the [Passives](/wiki/passives) tree, not covered
here since they're class-specific.

<h3 id="crit">Critical Hits</h3>

Every hit has a chance to crit for extra damage. Crit chance and crit damage
both start from a baseline everyone has for free, then grow from gear,
archetype, and passive-tree investment.

**Players' crit chance has no ceiling.** Push it past 100% and you don't just
get a bigger chance to crit once - every full 100% is a *guaranteed* extra
crit stack (250% crit chance = 2 guaranteed crits plus a 50% roll at a 3rd).
This is a deliberate, real build-around: stacking crit chance past 100% is one
of the strongest damage levers in the game, on purpose.

<p class="muted">Each crit stack past the first pays out on a curve instead of linearly, specifically so unlimited crit-chance stacking can't spiral into unbounded damage - only the first stack pays its full rate; every stack after that is worth progressively less. A landed crit's damage bonus is multiplied by <code>{{CRIT_BONUS_MULT_PCT}}%</code> of your crit multiplier's bonus over 1.0x, on that curve.</p>

Real world bosses play by a different rule: their own crit chance is capped at
**{{CRIT_CHANCE_CAP_PCT}}%**, full stop. Yours isn't. That asymmetry is
intentional - it's one of the reasons a heavily-invested crit build can out-pace
what a boss can ever do back to you.

<h3 id="block-and-dr">Block &amp; Damage Reduction</h3>

Two separate defensive layers, both worth building:

- **Block** is a coin flip - if it triggers, the hit is cut by **{{BLOCK_DAMAGE_REDUCTION_PCT}}%** flat, regardless of how much block chance you've stacked. Investing further just makes the coin flip land in your favor more often.
- **Damage reduction (DR)** is a straight percentage cut applied to every hit, whether it was blocked or not - and unlike every other defensive stat, DR can go *negative* (cursed gear that increases damage taken instead of reducing it).

Multiple sources of the same stat (gear + archetype + tree, or several
different skills) combine multiplicatively, not by simple addition - three
independent 50% damage-reduction sources add up to 87.5% total, not 150%.
This keeps stacking defense powerful without letting it reach outright
immunity. Both Block and DR (combined with each other) are hard-capped well
short of 100% - a landed hit against you always deals *some* damage, no matter
how defensive your build gets.

<p class="muted"><strong>Damage reduction specifically caps at mitigating 95% of a hit - currently, since it's an admin tunable - and this is universal doctrine, not just a normal-attack rule.</strong> No character, golem, or enemy can ever become immune to damage through damage reduction, full stop: this applies to every damage source DR touches, including self-inflicted damage like Righteous Fire's own self-burn (see Classes &amp; Passives) and Water Golem's Shattering icicles (see Golems). Evasion, Block, and Paladin's Intervene are separate mechanics with their own unrelated caps and are NOT part of this doctrine - only damage reduction itself is universally capped this way.</p>

<p class="muted">Raw damage reduction stacking well past 100% (before the cap applies) is intentional, not wasted investment - a real boss fight ignores a growing slice of your Block/DR/Evasion the longer it drags on (below), so excess raw DR is your buffer against that shred, not overcap waste.</p>

<p class="muted">Real world bosses additionally ignore a growing slice of your Block/DR/Evasion the longer a fight against them drags on, though never enough to push you below a floor relative to your own natural value - a deliberate anti-stalling pressure on very long fights.</p>

<h3 id="evasion">Evasion</h3>

Evasion is the actual miss-chance stat (see Hit Resolution above) - there's no
separate accuracy roll. Like Block and DR, multiple evasion sources combine
multiplicatively rather than adding, and the combined total is hard-capped
well short of 100% - total invisibility isn't achievable, on purpose.

<h3 id="splash">Splash / Multi-Target Attacks</h3>

Some attacks and heals hit more than one target - a fraction of the primary
hit's same base roll splashes onto extra targets nearby, each rolling its own
independent crit/evasion/block/DR, not a copy of the primary result.

- A player's own splash investment hits up to **{{PLAYER_SPLASH_MAX_TARGETS}}** extra enemies by default.
- A typical enemy's splash/cleave hits just **{{ENEMY_SPLASH_MAX_TARGETS}}** extra target - "a threat, not a reward."
- Push your own splash investment past 100% and instead of the overflow being wasted, you get **{{SPLASH_OVERFLOW_BONUS_TARGETS}}** more targets.
- Healing splash works the same way, capped at **{{HEAL_SPLASH_MAX_TARGETS}}** extra allies.

Named world bosses often override these defaults entirely - the Dragon's
breath sweeps the *whole party* every attack, and the Gelatinous Cube's splash
hits several heroes uniformly at random. See [Bosses](/wiki/bosses) for each
one's specifics.

When an attack has to pick WHO it splashes onto, it's uniformly random among
eligible targets - except an enemy attack splashing onto players prefers
whoever's above the party's median level first, falling back to everyone once
nobody above-median is left standing. ("The strongest heroes die first," per
the game's own design intent.)

<h3 id="elemental">Elemental Procs</h3>

Five elements exist - Fire, Cold, Chaos, Lightning, Divine - each rolled as
its own % on gear. Every landed hit (or landed heal, for the buff-flavored
procs) has a chance to trigger each type you're carrying, independently, at
`your % ÷ {{ELEMENTAL_PROC_CHANCE_DIVISOR}}`. Each proc that lands stacks for
**{{ELEMENTAL_PROC_DURATION_S}} seconds** - stacks from different procs are
tracked independently rather than refreshing one shared counter, so holding a
big stack takes sustained, repeated procs, not one lucky roll.

| Element | On a landed hit (debuff) | On a landed heal (buff) |
|---|---|---|
| Fire | Reduces the target's damage reduction | Increases the healed ally's damage reduction |
| Cold | Reduces the target's evasion | Increases the healed ally's evasion |
| Chaos | Reduces the target's block chance | Increases the healed ally's block chance |
| Lightning | Increases damage the target takes from everything, up to {{ELEMENTAL_LIGHTNING_MAX_STACKS}} stacks | - |
| Divine | Reduces the target's healing received, up to {{ELEMENTAL_DIVINE_ENEMY_MAX_STACKS}} stacks | Increases the *healer's own* future healing power instead of the ally's - and uniquely, this one isn't capped |

<p class="muted">Fire/Cold/Chaos's combined effect on a stat is bounded between {{ELEMENTAL_DEFENSE_FLOOR_PCT}}% and {{ELEMENTAL_DEFENSE_CEILING_PCT}}% - enough to matter, never enough to zero a stat out or send it past a sane ceiling.</p>

<h3 id="dots">Lingering Effect (Damage/Healing Over Time)</h3>

A hit or heal invested in Lingering Effect spawns a DoT (or, on a heal, the
same mechanic working as a heal-over-time) that ticks every
**{{LINGERING_EFFECT_TICK_INTERVAL_MS}}ms** for **{{LINGERING_EFFECT_TICKS}}**
ticks - **{{LINGERING_EFFECT_DURATION_S}} seconds** total. Its per-tick amount
is fixed the moment it's created, based on the triggering hit and your
investment at that instant - it does not scale up or down later even if your
stats change mid-fight. Landing another Lingering Effect (from the same
source or a different one) always adds a brand-new stack rather than
refreshing an existing one, so multiple instances tick independently side by
side.

The damage flavor is deliberately simple: it ignores block and evasion
entirely (unavoidable by design) and only cares about the target's flat
damage reduction, re-read fresh on every tick.

<h3 id="leech">Leech</h3>

A fraction of damage you deal can come back as healing to you. It's capped at
**{{LIFE_LEECH_CAP_PER_SEC_PCT}}%** of your own max HP per second - past that,
extra leech is simply wasted unless a specific passive converts the overflow
into something else (a temporary shield, for one class).

<h3 id="intervene">Intervene (Protect)</h3>

Some classes can redirect part of an incoming hit away from its original
target onto themselves instead - a shared party-wide mechanic, not a single
skill. Everyone's Intervene investment adds into one pooled total; that pool
determines how much of an incoming hit gets redirected away from its target
at all (hard-capped at half, no matter how much the party stacks), and is then
split among every contributing protector in proportion to their own share of
that pool. The original target always keeps whatever wasn't redirected -
including, if they also invest in Intervene, their own contribution coming
back to them. Every protector's share of a redirected hit rolls its own
independent crit/evasion/block/DR, same as a splash target would.

Taunt is a separate, simpler mechanic - a unit with an active taunt effect
makes every enemy attack that turn target them specifically, no split, no
pool, just a full redirect of targeting.

<h3 id="healing">Healing</h3>

Healing is built from the same base roll a normal attack would have used,
split between damage and healing according to how much heal-power you've
invested - a healer with 100% or more of their potential heal-power invested
puts the *entire* roll into healing rather than any into damage. Heals use the
exact same crit mechanic as attacks (see Critical Hits above) - a heal can
crit for extra healing, at the same formula.

Healing past 100% heal-power investment doesn't make each heal bigger - it
instead makes your heals land *more often* by shortening your attack clock,
so a heavily-invested healer heals the same amount, faster, rather than
bigger amounts less often.

A heal is capped at the target's missing HP - **overheal** (whatever didn't
fit) is wasted unless a specific passive converts it into a shield instead.
Healers default to targeting whoever's lowest on HP; if nobody's actually
hurt, a heal still goes to a random ally so on-heal effects (procs, shields,
buffs) keep firing even at full party health.

<h3 id="late-stage">The Late-Stage Penalty</h3>

The higher the world stage climbs, the less relative damage every player
deals **to a real world boss** specifically - a soft, unbypassable brake on
how trivial fights can get as the party's gear and passives snowball over
time. It does not apply to basic filler encounters, and nothing in the game
can bypass or reduce it - every true-damage source (reflected damage, DoT
ticks, on-death explosions, splash) goes through the same cut.

<h3 id="timing">Attack Speed &amp; Timing</h3>

Every combatant - players and enemies alike - has their own attack interval in
milliseconds, and the simulation just fires whoever's clock comes due next,
repeatedly, until the fight ends. There's no shared "turn." Melee, Ranged, and
Healer archetypes each have their own different base pace before gear/tree
speed investment (Ranged is naturally fastest, Healers naturally slowest),
and a real boss's own attack pace scales up with how many players are in the
fight, so a bigger party faces a proportionally faster boss.

A fight that somehow drags past **{{MAX_FIGHT_DURATION_S}} seconds** ends in a
loss automatically, regardless of remaining HP on either side - fights are
built to resolve well before that in practice.

<h3 id="basic-vs-boss">Basic Encounters vs. World Bosses</h3>

Not every fight is a real boss fight. Filler ("basic") encounters happen far
more often between real boss fights, and they're mechanically much simpler:

<p class="muted">Basic-encounter enemies have HP and a base attack and nothing else - zero crit chance, zero evasion, zero block, zero damage reduction, zero splash. Every mechanic on this page past Hit Resolution is functionally a no-op against one. They also never wear down your gear (only real boss fights do), and they don't advance the world stage or grant the late-stage-penalty-relevant XP a real kill does.</p>

Real world-boss fights are the actual progression loop - full stats on the
enemy side, gear durability at stake, and the world stage itself moves on a
win or loss. See [Getting Started](/wiki/getting-started) for how encounters
are scheduled, and [Bosses](/wiki/bosses) for what each named boss actually
does on top of all of this.

</div>
