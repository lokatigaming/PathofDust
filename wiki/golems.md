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

<h3 id="stats">Stats &amp; Inheritance</h3>

A golem's **base stats** - max HP, attack, evasion, damage reduction, block
chance - are 33% of your **fully-buffed effective stats**: your real,
post-buff numbers at the moment it's summoned (level, every passive-tree
bonus including Elemental Focus/Scorching Flames' per-level scaling, and
gear), not a base-stats-only snapshot.

<p class="muted"><strong>Everything else you have inherits at FULL value, not 33%.</strong> Crit chance/multiplier, increased damage, Conflagration, your fire/cold/lightning/chaos/divine damage bonuses, splash, Lingering Effect, Righteous Fire's damage bonus, and every other multiplier or tree-passive effect you carry - all pass through to a golem exactly as strong as they are on you. Base stats are the <em>only</em> thing scaled down to 33%; a golem is otherwise a full copy of your build's output, not a diluted one.</p>

<strong>Golems fight at your own pace, live.</strong> A golem's base attack
speed is a one-time snapshot taken at summon time, but any speed buff you
pick up mid-fight (Momentum, Flowing Strikes, Bloodlust, a party speed buff,
etc.) is read off you fresh every turn - your golems speed up and slow down
right alongside you, not frozen at whatever your speed happened to be the
moment you summoned them.

<strong>Summoning golems costs you nothing.</strong> There is no damage
penalty for fielding golems, at any count - you deal your full normal
damage whether you're running zero golems or three. Each golem you add is
close to a second (or third, or fourth) full copy of your own output
standing next to you, not a trade-off against it - Golem Master is a
straight power multiplier, and the biggest one in the game at 3 golems.
A Thunder Golem's tanking is separate, additional value on top of all this -
expect to rank disproportionately high on Top Tanks compared to Top DPS if
you run one, that's working as intended.

<h3 id="roles">Golem Roles at a Glance</h3>

<div class="wiki-table-wrap">

| Type | Role | Why |
|---|---|---|
| Basic | Baseline | No sub-tree, no type bonus - still a full second copy of your output at 33% base stats. |
| Thunder | Tank | Absorbs all external party damage while it's up; can't be healed or shielded. |
| Flame | Damage | Multiplies its own inherited elemental damage further; attacks faster; hits harder. |
| Water | Healer/Support | Passively heals the whole party every second; boosts everyone's healing and shields received. |

</div>

<h3 id="basic">Basic Golem</h3>

No sub-tree, no type-specific bonus - just the standard golem stats,
inheritance, and attack described above. The baseline option, and still a
meaningful power add on its own thanks to full inheritance.

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

**Base effect:** every point of fire/cold/lightning damage bonus you'd
otherwise pass down to it (already at full value, per Stats &amp;
Inheritance above) gets multiplied further - 1.33x at rank 1, 1.66x at rank
2, 2.0x at rank 3. This applies to your whole elemental kit, not just fire.

- **Volcanic Ash** - a further bonus specific to fire damage, on top of the
  base multiplier above: 33/66/100% MORE fire damage than what's already
  inherited (e.g. if your fire damage bonus already reaches 1000% through
  full inheritance, rank 3 Volcanic Ash adds another 1000% on top of that,
  landing at 2000% total - not a fraction of it).
- **Blazing** - attacks 6/9/18% faster.
- **Surging** - deals 10/20/30% more damage outright.

Your dedicated damage-dealer: the base multiplier alone meaningfully
amplifies whatever elemental kit you've already built, before its three
modifiers stack further on top.

<h3 id="water">Water Golem</h3>

**Base effect:** regenerates 3/6/9% of the Water Golem's own max HP per
second, applied to every party member (not the golem itself). A real,
ongoing heal that ticks once a second and benefits from your other
heal-boosting effects. Running more than one Water Golem doesn't stack this
- only the strongest one's regen actually ticks.

- **Replenishing** - converts all the damage it deals into healing for the
  party instead, at 100/200/300%.
- **Singing** - the whole party gets 10/20/30% more effect from heals and
  shields they receive.
- **Shattering** - when an enemy dies near it, sends damaging icicles at
  nearby enemies.

Your support unit: the passive party-wide regen alone makes it worth
running even before its modifiers turn it into real sustain (Replenishing)
or a party-wide heal amplifier (Singing).

<h3 id="rules">Rules Worth Knowing</h3>

- **Golems die when you die.** If your Elementalist goes down, every golem you had out dies instantly with you.
- **Golems don't keep a fight going.** They're never counted toward "is anyone still alive" - a fight where every real player is down ends normally even if a Thunder Golem is mid-reform.
- **Your golems' stats count as yours.** Damage dealt and damage absorbed/tanked by any of your golems (including a Thunder Golem's tanking) roll up into your own totals everywhere per-player stats are shown or ranked - Top DPS/Tanks/Heals in chat announcements, and your fight history. A golem never shows up as its own separate entry.
- **Healing credit is specific about *what* counts as healing.** A Thunder Golem reforming (coming back after dying) is not a heal - it grants no healing credit to anyone, yours or otherwise. A Water Golem's base party-wide regen and its Replenishing modifier are both real healing, and both credit *you*, same as any other golem stat. Rising Phoenix (a Righteous Fire passive, not a golem one) works the same way when it revives a nearby ally - the healing credit goes to *you*, the Elementalist whose Phoenix triggered, not to the ally who got revived.
- **Golems are invisible on the OBS overlay, on purpose** - not a bug. They're fully real in combat (the fight log, damage, and outcome all reflect them correctly) but don't currently get their own sprite on stream. Visualizing them is a known future improvement, not yet built.

</div>
