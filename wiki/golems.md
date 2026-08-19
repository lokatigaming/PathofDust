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

A golem fights with 33% of your own core combat stats (max HP, attack, crit
chance/multiplier, evasion, damage reduction, block chance) - it attacks at
your own pace, not a scaled-down one. None of your other passive-tree bonuses
(elemental procs, splash, Righteous Fire, etc.) carry over to a golem - it's
a basic unified hit, nothing more, unless its type grants something extra
(see below).

<p class="muted"><strong>⚠️ Balance in flux:</strong> golem stats currently scale off your character's base combat stats. This is actively being corrected to scale off your fully-buffed effective stats instead (gear and passives included) - exact numbers are being tuned this week, so treat "33%" as directionally correct but not final.</p>

Summoning golems isn't free for your own damage: **each golem summoned cuts
your own damage by 33%, additive** - one golem costs you 33%, two costs 66%,
and three leaves you dealing just 1% of your normal damage. Golems are a
real trade of your own offense for board presence, not a pure bonus.

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
- **Your golems' stats count as yours.** Damage dealt, damage absorbed/tanked, and healing done by any of your golems (including a Thunder Golem's tanking and a Water Golem's heals) roll up into your own totals everywhere per-player stats are shown or ranked - Top DPS/Tanks/Heals in chat announcements, and your fight history. A golem never shows up as its own separate entry.
- **Golems are invisible on the OBS overlay, on purpose** - not a bug. They're fully real in combat (the fight log, damage, and outcome all reflect them correctly) but don't currently get their own sprite on stream. Visualizing them is a known future improvement, not yet built.

</div>
