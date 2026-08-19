<div class="card wiki-wide">

## Golems

Golem Master, one of the [Elementalist](/wiki/classes#elementalist)'s three
branches, lets you summon golems that fight alongside you. This page covers
how they work; see Classes &amp; Passives for everything else about the
Elementalist.

<h3 id="slots">Slots &amp; Types</h3>

Investing in the Golem Master skill grants golem slots - 1 at rank 1, up to 3
at rank 3. Each unlocked slot gets its own type picker on the `/passives`
page: **Basic, Thunder, Flame,** or **Water**. The picker only appears once
you're playing Elementalist with at least one point in Golem Master, and only
shows as many slots as you've actually unlocked. Changing a slot's type is
free and takes effect on your next fight.

<h3 id="stats">Stats &amp; the Damage Trade-off</h3>

A golem fights with 33% of your **fully-buffed effective stats** (max HP,
attack, crit chance/multiplier, evasion, damage reduction, block chance) -
your real, post-buff numbers at the moment it's summoned (level, every
passive-tree bonus including Elemental Focus/Scorching Flames' per-level
scaling, and gear), not a base-stats-only snapshot. It attacks at your own
pace, not a scaled-down one.

<p class="muted">Your damage MULTIPLIERS (increased damage, Conflagration) are the one exception - a golem inherits those at their FULL value, not scaled to 33% like everything else. That's deliberate, not an oversight: scaling a multiplier down compounds against an already-scaled-down base stat instead of canceling out, so passing it through whole is what actually makes a golem's own hit land near the intended 33%-of-your-damage mark. Crit chance/multiplier are still scaled to 33% each, unlike the flat damage multipliers - a crit-heavy build's golems won't crit as meaningfully as the build itself does.</p>

Summoning golems isn't free for your own damage: **each golem summoned cuts
your own damage by 33%, additive** - one golem costs you 33%, two costs 66%,
and three leaves you dealing just 1% of your normal damage. This is designed
to roughly cancel out against each golem's own ~33% share, so your combined
output (yourself plus every golem) should land close to what you'd deal
running solo at any golem count - Golem Master trades some of your own
damage for board presence and build diversity, not a straight loss (crit-
heavy builds should expect to land a little under that mark, per the crit
note above). A Thunder Golem's tanking is separate, additional value on top
of this - expect to rank disproportionately high on Top Tanks compared to
Top DPS if you run one, that's working as intended.

<h3 id="basic">Basic Golem</h3>

No sub-tree, no bonuses - just the standard golem stats and attack described
above. The baseline option.

<h3 id="thunder">Thunder Golem</h3>

Absorbs **all externally-sourced damage** the party would otherwise take,
for as long as it's alive - it cannot be shielded or healed by any means.
When it dies, it reforms after a few seconds (4/3/2s by rank) at full health
and rejoins the fight. It does not protect you from your own Righteous Fire
self-burn - that's still yours to manage.

- **Gigantify** - raises how much of your health pool it draws on (base 33% → up to 132% at max rank).
- **Growing** - permanently grows its max HP a bit more every time it reforms within the same fight, additively off its original spawn HP (not compounding on its already-grown value).
- **Terrifying** - explodes for a fraction of its own HP as damage to nearby enemies when it dies.

<p class="muted"><strong>⚠️ Balance in flux:</strong> Thunder Golem balance is under active tuning - an absorbed-damage redistribution-on-death mechanic is coming (what happens to damage it soaked up right as it dies). No numbers for that yet; don't build around specific figures for Thunder Golem changing soon.</p>

<h3 id="flame">Flame Golem</h3>

- **Volcanic Ash** - inherits a share of your own fire-damage debuff strength.
- **Blazing** - attacks noticeably faster.
- **Surging** - deals more damage outright.

A straightforward damage-focused golem - no unique base behavior beyond the
standard golem attack, its identity is entirely in these three modifiers.

<h3 id="water">Water Golem</h3>

- **Replenishing** - converts all the damage it deals into healing for the party instead.
- **Singing** - the whole party gets more effect from heals and shields they receive.
- **Shattering** - sends damaging icicles at nearby enemies whenever an enemy dies near it.

Same as Flame Golem, its base attack is standard - the modifiers are the
whole point, here turning it into a support unit instead of a damage one.

<h3 id="rules">Rules Worth Knowing</h3>

- **Golems die when you die.** If your Elementalist goes down, every golem you had out dies instantly with you.
- **Golems don't keep a fight going.** They're never counted toward "is anyone still alive" - a fight where every real player is down ends normally even if a Thunder Golem is mid-reform.
- **Your golems' stats count as yours.** Damage dealt and damage absorbed/tanked by any of your golems (including a Thunder Golem's tanking) roll up into your own totals everywhere per-player stats are shown or ranked - Top DPS/Tanks/Heals in chat announcements, and your fight history. A golem never shows up as its own separate entry.
- **Healing credit is specific about *what* counts as healing.** A Thunder Golem reforming (coming back after dying) is not a heal - it grants no healing credit to anyone, yours or otherwise. A Water Golem's Replenishing *is* real healing (it's converting damage into an actual heal), and that credits you, same as any other golem stat. Rising Phoenix (a Righteous Fire passive, not a golem one) works the same way when it revives a nearby ally - the healing credit goes to *you*, the Elementalist whose Phoenix triggered, not to the ally who got revived.
- **Golems are invisible on the OBS overlay, on purpose** - not a bug. They're fully real in combat (the fight log, damage, and outcome all reflect them correctly) but don't currently get their own sprite on stream. Visualizing them is a known future improvement, not yet built.

</div>
