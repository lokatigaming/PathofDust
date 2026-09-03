<div class="card wiki-wide">

## Items &amp; Loot

Currencies and crafting actions have their own page - see
[Crafting](/wiki/crafting). This page covers where gear actually comes from
and what makes one item better than another.

<h3 id="slots">Equip Slots</h3>

**{{EQUIP_SLOTS_COUNT}}** slots, one item each: Weapon, Helm, Body, Gloves,
Boots. Every slot contributes to your character differently - Weapon feeds
your outgoing damage, Body feeds max HP, Gloves feed attack speed, and Helm
and Boots each grant their own periodic proc (a damage tick from Helm, a
self-heal tick from Boots) that fires on its own independent clock.

<h3 id="generation">How an Item Is Generated</h3>

Every drop rolls three things independently: **tier** (scales with the world
stage you found it at - deeper stages drop higher-tier gear), a **power
roll** (a fixed multiplier between {{POWER_ROLL_MIN_PCT}}% and
{{POWER_ROLL_MAX_PCT}}% of that tier's potential, locked in for the item's
whole life - Reforge keeps it, it never re-rolls), and a random set of
**affixes** (secondary modifiers, see Crafting's Modifier Ceiling for the
7-total cap). An item also gets a small chance to spawn **indestructible**
(never wears out) or, far more commonly, a limited number of real-boss-fight
uses before it wears out completely - see Durability below.

<p class="muted">Each slot has its own base power coefficient, scaled by tier and the power roll above: Weapon {{WEAPON_BASE_POWER}}, Helm {{HELM_BASE_POWER}}, Body {{BODY_BASE_POWER}}, Gloves {{GLOVES_BASE_POWER}}, Boots {{BOOTS_BASE_POWER}} - per tier. A worn (partially-decayed) item's effective power is scaled down by how used-up it is, same as its affix values.</p>

<h3 id="tier-curve">How Tier Translates Into Power</h3>

An affix's magnitude does **not** scale linearly with tier. It follows a
curve that grows much more slowly than the tier number does: below tier 100
an affix's tier term grows with the square root of the tier, and past tier
100 it flattens further still, so climbing another thousand tiers is worth
roughly one doubling rather than a thousandfold increase.

Two things follow from that, and they are the point of the design:

- **Tier 100 now delivers roughly what tier 10 used to.** A tier-1 item is unchanged; the further up you go, the wider the gap between the tier number and the power it buys.
- **Every affix keeps its exact relative weight against every other affix, at every tier.** No affix became better or worse compared to another - only the tier term itself travels more slowly. A build that was well-geared relative to its peers still is.

This was applied **retroactively** to gear players already held, rescaled in
place. Each affix kept its own roll quality, and no item's displayed quality%
changed - the numbers on your existing gear simply became smaller. A tier-7
item that read +14.92% cold / +18.90% divine / +22% max HP reads about +5.64 /
+7.14 / +8.32 after the rescale.

<p class="muted">Shipped alongside it: the <strong>Crit Multiplier affix now rolls at half</strong> its former per-tier value, also applied retroactively. Crit <em>chance</em> was untouched. See <a href="/wiki/combat#crit">Combat</a>.</p>

<h3 id="affixes">Affixes</h3>

There are **{{ALL_AFFIXES_COUNT}}** affix types in the game, and every one of
them is slot-agnostic - damage reduction, block chance, evasion, increased
damage, crit chance, crit multiplier, splash, Echo, intervene, leech,
increased life, flat life, and the five elemental damage types
(Fire/Cold/Lightning/Divine/Chaos - see [Combat](/wiki/combat#elemental)) can
all roll on any of the 5 slots. The elemental types used to be restricted to
Weapon or Helm only; that restriction is gone, so a build can now stack
elemental damage affixes across every slot instead of just two.

<p class="muted">This widen has a real, felt side effect on Body/Gloves/Boots: each of those three slots went from 12 to 17 eligible affixes, which dilutes every other affix's odds there by roughly 31% - including Increased Life, the main source of effective HP those slots provide. Same-tier gear on those three slots now yields noticeably less effective HP on average than it did before the widen. This is intended, not a bug: elemental affixes now compete with HP for the same slots, a deliberate gearing tradeoff rather than a pure upgrade.</p>

<p class="muted"><strong>"Lingering Effect" is retired.</strong> It was replaced by <strong>Echo</strong> - a chance for a hit to fire again rather than a damage-over-time tick. See <a href="/wiki/combat#echo">Combat</a> for how Echo's ladder works. Existing items convert automatically at half their stored value, but the timing isn't guaranteed, so a piece of your gear may still display the old name for a while. It does nothing either way.</p>

Not every affix is equally common. Life Leech in particular is deliberately
rare - roughly **{{LEECH_RARITY_DIVISOR}}x** less likely to roll than any
other affix, specifically so it feels like a real find rather than a normal
stat.

<h3 id="drops">Where Drops Come From</h3>

Real boss-fight wins are the generous source: dust, sand and loot rolls, plus
guaranteed Perfect or Sacred items once the world is deep enough - see
[Crafting](/wiki/crafting#stage-gates), since several drop types don't exist
below a stage threshold at all. Basic filler encounters pay out too, just at a
fraction of the rate, with no Perfect/Sacred guarantees.

**Craft tokens are not a drop.** They were once, and are not now - see
[Crafting](/wiki/crafting#currencies).

If you go a while without an item drop, a **pity** meter quietly builds in the
background and guarantees your next one once it fills - a real boss fight
advances it **{{BOSS_ITEM_PITY_GAIN_PCT}}%** per fight (roughly a 4-fight
dry-streak cap), a basic encounter **{{BASIC_ITEM_PITY_GAIN_PCT}}%**. You'll
never go on a genuinely unbounded dry streak for items. There is no equivalent
meter for anything else any more.

<h3 id="uniques">Unique Affixes</h3>

Two exist today, both granted by the **same** currency - a Unique Shard, which
opens a picker letting you choose between them. They live entirely outside the
normal 4-modifier pool:

- **Celestial Conversion** - covered in full on [Crafting](/wiki/crafting#celestial-shard).
- **Split Personality** - unlocks a second archetype's passive tree while equipped. Covered in full on [Classes &amp; Passives](/wiki/classes#split-personality).

<p class="muted">Celestial Conversion used to come from its own separate "Celestial Shard". That currency is retired and merged into Unique Shard - there is only one shard now.</p>

You can only have one of a given unique affix *equipped* at a time - owning a
second copy just means it's sitting in your bag until you swap it in. Applying
a shard to an item you are already wearing is checked against that rule and
will be refused rather than creating a duplicate; a refused attempt doesn't
spend the shard.

<h3 id="durability">Durability &amp; Repair</h3>

Most items wear down: a small chance to be indestructible (never wears out),
otherwise a limited number of real-boss-fight uses before they hit 0%
effective power. A worn-out item isn't destroyed or auto-unequipped - it just
stops contributing anything until repaired. **Basic encounters never wear
down your gear**, only real boss fights do.

<p class="muted">Repairing a single item costs <code>ceil(tier &times; missing_fraction)</code> dust. Repairing everything at once (equipped and bagged) costs a 10% premium over the sum of what each individual repair would have cost.</p>

If every piece of your equipped gear wears out at once, you retreat - see
[Getting Started](/wiki/getting-started#death-and-retreat) for what that
means and how to get back in.

<h3 id="inventory">Inventory</h3>

Your bag holds up to {{INVENTORY_CAPACITY}} items (see [Getting
Started](/wiki/getting-started#bag)) before new drops start getting lost.
Auto-disenchant (see [Crafting](/wiki/crafting)) can intercept low-value
drops before they ever take up a slot.

</div>
