<div class="card wiki-wide">

## Golems

Golem Master, one of the [Elementalist](/wiki/classes#elementalist)'s three
branches, lets you summon golems that fight alongside you. This page covers
how they work; see Classes &amp; Passives for everything else about the
Elementalist.

<p class="muted"><strong>Where the numbers are.</strong> Every passive node's value can be retuned live, without a patch, so this page describes what each node <em>does</em> and what it scales with rather than printing per-rank percentages that would quietly go stale. <strong>The <a href="/wiki/passives">Passives</a> tree renders each node's real current value</strong>, and flags it explicitly when a node has been tuned away from its shipped default. Read the numbers there.</p>

<h3 id="slots">Slots &amp; Types</h3>

Investing in the Golem Master skill grants golem slots - 1 at rank 1, up to 3
at rank 3. Each unlocked slot gets its own type picker on the `/passives`
page: **Basic, Thunder, Flame,** or **Water**. The picker only appears once
you're playing Elementalist with at least one point in Golem Master, and only
shows as many slots as you've actually unlocked. Changing a slot's type is
free and takes effect on your next fight.

<p class="muted">Your per-slot type assignments are saved and restored correctly when you save or load a <a href="/wiki/passives">Memory</a> - loading an older Memory that predates typed golem slots no longer resets your live slot types back to default.</p>

<h3 id="stats">Stats &amp; Inheritance</h3>

A golem's **base stats** - max HP, attack, evasion, damage reduction, block
chance - are 33% of your **fully-buffed effective stats**: your real,
post-buff numbers at the moment it's summoned (level, every passive-tree
bonus including Elemental Focus/Scorching Flames' per-level scaling, and
gear), not a base-stats-only snapshot.

<p class="muted"><strong>Everything else you have inherits at FULL value, not 33%.</strong> A golem is built as a full copy of you - your whole build, including a second class's tree if you're running <a href="/wiki/classes#split-personality">Split Personality</a> - with only base stats scaled down and a short list of things stripped off because they don't make sense on a golem (your temp buffs/shields at the exact moment you summoned it, a golem's own reform/tick machinery, and a handful of mechanics that only work by scanning the whole roster for their real owner specifically, like Guardian Spirit and Rising Phoenix). Assume full inheritance of anything you've built unless you have a specific reason to think otherwise, rather than checking it field by field.</p>

<p class="muted">Heal power is the one stat with real per-type behavior instead of a flat rule: it's zeroed out on Thunder, Flame, and Basic golems (they don't heal, so there's nothing for it to affect), and additive on a Water Golem (your own heal power plus its own Replenishing bonus) - which is why gearing heal power meaningfully strengthens a Water Golem's output specifically, not the other three types.</p>

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

Each golem type is also assigned one of the game's own combat roles - the
same Melee/Ranged/Heal categories your own archetype has (see
[Combat](/wiki/combat#timing)) - which sets its base attack pace before any
speed buffs apply: **Thunder and Basic are Melee, Flame is Ranged, Water is
Heal** (naturally the fastest and slowest base paces respectively, same as
for a player).

<div class="wiki-table-wrap">

| Type | Combat Role | Practical Role | Why |
|---|---|---|---|
| Basic | Melee | Baseline | No sub-tree, no type bonus - still a full second copy of your output at 33% base stats. |
| Thunder | Melee | Tank | Absorbs all external party damage while it's up; can't be healed or shielded. |
| Flame | Ranged | Damage | Multiplies its own inherited elemental damage further; attacks faster; hits harder. |
| Water | Heal | Healer/Support | Passively heals the whole party every second; boosts everyone's healing and shields received. |

</div>

<h3 id="basic">Basic Golem</h3>

No sub-tree, no type-specific bonus - just the standard golem stats,
inheritance, and attack described above. The baseline option, and still a
meaningful power add on its own thanks to full inheritance.

<h3 id="thunder">Thunder Golem</h3>

Absorbs **all externally-sourced damage** the party would otherwise take,
for as long as it's alive - it cannot be shielded or healed by any means.
When it dies, it reforms at full health after a delay and rejoins the fight -
ranking the node up shortens that delay. It does not protect you from your
own Righteous Fire self-burn - that's still yours to manage.

- **Gigantify** - raises how much of your health pool it draws on, well past the 33% baseline at higher ranks.
- **Growing** - permanently grows its max HP a bit more every time it reforms within the same fight, additively off its original spawn HP (not compounding on its already-grown value).
- **Terrifying** - explodes for a fraction of its own HP as damage to nearby enemies when it dies.

When a Thunder Golem dies, a share of everything it had absorbed doesn't just
disappear - it splits evenly across your whole party as real, unavoidable
damage spread over the next couple of seconds (the remainder is forgiven).
Losing a Thunder Golem has real weight: the more it was tanking, the more your
party feels its death. This can down a party member exactly like any other
lethal damage.

<p class="muted">What share gets redistributed, and how long it spreads over, are operator dials rather than fixed numbers - the mechanic is "some of what it absorbed comes back at the party when it dies, the rest is forgiven," and the split can be retuned. Several bugs that used to make the real delivery fall short of the configured share have been fixed: a recipient who died before their tick landed no longer loses it (it redirects to someone still alive), and a golem dying twice in quick succession no longer discards the first death's undelivered amount.</p>

<h3 id="flame">Flame Golem</h3>

**Base effect:** every point of fire/cold/lightning damage bonus you'd
otherwise pass down to it (already at full value, per Stats &amp;
Inheritance above) gets multiplied further, by more at each rank. This
applies to your whole elemental kit, not just fire.

- **Volcanic Ash** - a further bonus specific to fire damage, stacking on top
  of the base multiplier above. Read it as "this much MORE fire damage than
  what's already inherited," not as a fraction of it: at max rank it roughly
  doubles an already fully-inherited fire bonus rather than reducing it.
- **Blazing** - attacks faster.
- **Surging** - deals more damage outright.

Your dedicated damage-dealer: the base multiplier alone meaningfully
amplifies whatever elemental kit you've already built, before its three
modifiers stack further on top.

<h3 id="water">Water Golem</h3>

**Base effect:** regenerates a percentage of the Water Golem's own max HP per
second, applied to every party member (not the golem itself) - more per rank.
A real, ongoing heal that ticks once a second and benefits from your other
heal-boosting effects. Running more than one Water Golem doesn't stack this
- only the strongest one's regen actually ticks.

- **Replenishing** - converts all the damage it deals into healing for the
  party instead, at a rank-scaled rate that reaches well above 1:1.
- **Singing** - the whole party gets more effect from heals and shields they
  receive.
- **Shattering** - when an enemy dies in the Water Golem's presence, sends
  icicles at (splash + rank) nearby enemies. Each icicle deals a percentage
  of the *dead* enemy's own health - that percentage is an operator dial set
  per rank, so it is not printed here. Icicle damage is **Environmental**:
  mitigated only by the target's
  own damage reduction (never evasion or block, and never past the universal
  DR cap - see [Combat](/wiki/combat#block-and-dr), so a target can never
  become fully immune to it), can never crit, and draws nothing from the
  Water Golem's - or its owner's - own damage stats, buffs, or on-hit
  effects. It hits the same regardless of how strong your build
  is; only the dead enemy's health and the target's own DR matter.

Your support unit: the passive party-wide regen alone makes it worth
running even before its modifiers turn it into real sustain (Replenishing)
or a party-wide heal amplifier (Singing).

<h3 id="rules">Rules Worth Knowing</h3>

- **Golems die when you die.** If your Elementalist goes down, every golem you had out dies instantly with you.
- **Golems don't keep a fight going.** They're never counted toward "is anyone still alive" - a fight where every real player is down ends normally even if a Thunder Golem is mid-reform.
- **Your golems' stats count as yours.** Damage dealt and damage absorbed/tanked by any of your golems (including a Thunder Golem's tanking) roll up into your own totals everywhere per-player stats are shown or ranked - Top DPS/Tanks/Heals in the <a href="/wiki/dashboard#feed">Feed</a>'s fight summaries, and your fight history. A golem never shows up as its own separate entry.
- **Healing credit is specific about *what* counts as healing.** A Thunder Golem reforming (coming back after dying) is not a heal - it grants no healing credit to anyone, yours or otherwise. A Water Golem's base party-wide regen and its Replenishing modifier are both real healing, and both credit *you*, same as any other golem stat. Rising Phoenix (a Righteous Fire passive, not a golem one) works the same way when it revives a nearby ally - the healing credit goes to *you*, the Elementalist whose Phoenix triggered, not to the ally who got revived.
- **Golems are invisible on the OBS overlay, on purpose** - not a bug. They're fully real in combat (the fight log, damage, and outcome all reflect them correctly) but don't currently get their own sprite on stream. Visualizing them is a known future improvement, not yet built.

</div>
