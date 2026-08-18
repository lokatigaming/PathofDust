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

<h3 id="affixes">Affixes</h3>

There are **{{ALL_AFFIXES_COUNT}}** affix types in the game. Most are
slot-agnostic - block chance, evasion, crit chance/damage, splash, intervene,
leech, increased damage/life, and Lingering Effect can roll on any slot. The
five elemental damage types (Fire/Cold/Lightning/Divine/Chaos - see
[Combat](/wiki/combat#elemental)) are the one exception: they only roll on a
**Weapon or Helm**.

Not every affix is equally common. Life Leech in particular is deliberately
rare - roughly **{{LEECH_RARITY_DIVISOR}}x** less likely to roll than any
other affix, specifically so it feels like a real find rather than a normal
stat.

<h3 id="drops">Where Drops Come From</h3>

Real boss-fight wins are the generous source: real dust/sand/loot rolls, a
chance at craft tokens, and (at high enough world stage) guaranteed Perfect
or Sacred items - see [Crafting](/wiki/crafting#item-tiers). Basic filler
encounters pay out too, just at a fraction of the rate, with no chance at
craft tokens and no Perfect/Sacred guarantees at all.

If you go a while without an item or craft-token drop, a **pity** meter
quietly builds in the background and guarantees your next one once it fills
- a real boss fight advances it **{{BOSS_ITEM_PITY_GAIN_PCT}}%** per fight
toward a guaranteed item (roughly a 4-fight dry-streak cap) and
**{{BOSS_CRAFT_PITY_GAIN_PCT}}%** toward a guaranteed token; a basic
encounter advances the same meters more slowly
({{BASIC_ITEM_PITY_GAIN_PCT}}% / {{BASIC_CRAFT_PITY_GAIN_PCT}}%). You'll
never go on a genuinely unbounded dry streak.

<h3 id="uniques">Unique Affixes</h3>

Two exist today, each granted by its own rare crafting token and living
entirely outside the normal 4-modifier pool:

- **Celestial Conversion** (from a Celestial Shard) - covered in full on [Crafting](/wiki/crafting#celestial-shard).
- **Split Personality** (from a Unique Shard) - unlocks a second archetype's passive tree while equipped. Covered in full on [Classes &amp; Passives](/wiki/classes#split-personality).

You can only have one of a given unique affix *equipped* at a time - owning a
second copy just means it's sitting in your bag until you swap it in.

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
