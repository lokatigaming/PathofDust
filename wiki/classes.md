<div class="card wiki-wide">

## Classes &amp; Passives

This page covers how the class system and passive points actually work. For
the full node-by-node tree of any specific class - every Skill,
Specialization, and Modifier with its real rank-by-rank numbers - see the
code-generated [Passives](/wiki/passives) viewer, which draws the exact same
tree the game itself uses.

<h3 id="archetypes">The {{ARCHETYPE_COUNT}} Classes</h3>

Everyone starts as **Commoner** - a plain fighter with no archetype bonus and
no penalty, a blank slate rather than a real build. Pick one of the
{{ARCHETYPE_COUNT}} real classes below from your dashboard whenever you're
ready (see [Getting Started](/wiki/getting-started#archetype) for the cost
of changing your mind later).

<div class="wiki-table-wrap">

| Class | Role | Identity |
|---|---|---|
| Warrior | Melee tank | Block-focused, innately counter-attacks anyone who hits it |
| Berserker | Melee DPS | Glass-cannon multi-strike damage |
| Rogue | Melee DPS | Crit-focused, evasion-into-crit conversion, guaranteed opening hits |
| Monk | Melee (evasion-based) | Dodge-tank/hybrid DPS, converts evasion overflow into damage |
| Paladin | Melee (hybrid) | Redirects damage away from allies (Intervene) *and* heals - a real hybrid, not purely a tank |
| Ranger | Ranged DPS | Splash/AoE-focused |
| Mage | Ranged DPS | Crit-chained recasts, burst damage |
| Warlock | Ranged DPS | Attack-speed and life-drain sustain |
| Cleric | Healer | Chain-heals and shields |
| Druid | Healer (evasion-flavored) | Same healer baseline as Cleric, leans on evasion instead of raw heal-power gear |
| Slayer | Melee DPS | HP-cost resource system (Bloodpact) and stacking bleed |
| Elementalist | Ranged DPS | Splash-based root; 3 elemental branches (Elemental Focus, Righteous Fire, Golem Master) - see below |

</div>

<p class="muted">Paladin's healer-grade heal-power bonus is baked in outside the normal role calculation specifically so it doesn't also change Paladin's base damage formula or attack pace - it's genuinely hybrid, even though the game labels its combat role "Melee."</p>

<h3 id="points">Passive Points</h3>

You earn passive points purely from leveling, starting immediately - no
delay before your first point. The formula is `1 + level ÷ 4`, rounded down:
level 1 gives you **{{POINTS_AT_LEVEL_1}}** point, level 20 gives
**{{POINTS_AT_LEVEL_20}}**, level 50 gives **{{POINTS_AT_LEVEL_50}}**.

Every tree follows the same shape: 1 root passive (always active, scales
with your level automatically, no points needed), 3 Skills you can rank up
freely, 9 Specializations (3 per Skill, gated behind investing in their
parent Skill), and 27 Modifiers (3 per Specialization, gated behind pushing
that Specialization to its max rank).

**The 4th point in a Specialization is special.** It doesn't add another
point of that Specialization's own stat - instead, it "specializes" the
node and unlocks its 3 child Modifiers below. This is a real game mechanic,
not just UI framing: the extra stat growth genuinely stops at rank 3, and
the 4th point's entire job is unlocking the row underneath.

<p class="muted">Cleric's Chain of Light (Prayer of Mending's bounce-target Specialization) is a concrete example worth knowing by name: 2 total bounce targets at rank 1, up to 4 at rank 3 - a 4/4 investment unlocks its Modifiers, not a 5th target. A brief live bug once let a 4/4 investment reach 5 targets by reading the raw rank instead of stopping the stat growth at 3 like every other Specialization; that's fixed, and the tree's own numbers (2 → 4) are the accurate, current behavior.</p>

A node marked **(inactive)** on the tree accepts point investment like any
other - those points are banked, not wasted, and the node will start working
the moment that mechanic ships. This is intentional and known; if you see
one, it's not a bug.

<h3 id="respec">Changing Your Mind</h3>

**Save/Reset**: while you're allocating on the interactive `/passives` page,
nothing is committed until you hit Save - Reset throws away your in-progress
changes and reverts to whatever you last saved, for free, anytime.

**Full respec**: wipes your entire (primary) tree and refunds every point,
free the first time, **{{PASSIVE_RESPEC_COST}} dust** after that. This is a
complete do-over, not a per-node undo.

<h3 id="split-personality">Split Personality (Second Archetype)</h3>

A real, shipped feature - not every player will have it, since it's gated
behind a specific Unique item.

**Unlocking it**: consume a **Unique Shard** crafting token on an item and
pick **Split Personality** from the effect picker (Unique Shards can also
grant Celestial Conversion instead - see [Crafting](/wiki/crafting) for the
full picker). While that item is equipped, a second-archetype picker appears
on `/passives`.

**How it works**: pick any class other than Commoner or your current
primary, and you get a fully separate second passive tree to invest in -
same rules, same 4/4 gate, tracked completely independently even where node
names collide with your primary tree. Both trees draw from the **same**
shared point pool, though - Split Personality doesn't double your points, it
adds a bonus on top: a flat +1, plus +1 more for every 300 tiers on the item
granting it.

**It's always free** to equip and pick a secondary class - no dust, ever,
unlike changing your primary archetype. Unequipping the item is an instant,
complete "refund" of whatever you'd invested in the secondary tree; re-equip
later and you resume right where you left off.

<p class="muted">A full respec (above) only clears your primary tree - today, unequipping Split Personality's item is the only way to reset the secondary tree specifically.</p>

<p class="muted">Your secondary tree's passives now work everywhere they should, not just partially. Two separate bugs existed after launch and both are fixed: first, generic stat nodes (crit chance/multiplier, evasion, block, damage reduction, attack speed, splash, max HP%, heal power, Intervene) weren't applying in combat from a secondary tree; then, more broadly, dozens of passive and aura checks across the game were only ever asking "is this character archetype X," which silently ignored your secondary class entirely - so most of a secondary class's own unique passives and auras weren't firing at all, only its plain stat nodes were (after the first fix). Both are resolved now: your secondary class functions as a real second class, not a partial one.</p>

<h4>Dual-Archetype Interactions</h4>

A handful of skills across different classes share the same underlying
mechanism under the hood - for example, Rogue's Twin Strikes and Mage's
Spell Echo both write into the same shared trigger-chance/damage fields,
normally kept apart only because a character can usually only ever be one
archetype at a time. Split Personality breaks that assumption: investing in
both classes' matching nodes at once currently **stacks** them together
rather than staying mutually exclusive. This is current behavior, not a
documented or guaranteed feature - treat any cross-archetype combo like this
as something that could change without notice, not a build you should count
on staying exactly this strong.

<h3 id="elementalist">The Elementalist</h3>

The 12th class - a Ranged elemental caster built around three distinct
branches, each following the same 1 Skill → 3 Specializations → 9 Modifiers
shape (13 nodes each, 39 total) every other class's tree already uses,
including the standard 4/4 gate on Specializations.

**Root passive**: splash, scaling with level - the same mechanic and
magnitude Ranger's own root uses, just on a caster instead of an archer.

**Elemental Focus** - raises how reliably your Fire/Cold/Lightning on-hit
debuffs actually land (see [Combat](/wiki/combat#elemental) for what those
debuffs do), scaling with your character level, independently per element.
Its 3 specs (Shocking/Chilling/Scorching Focus) each add "applies debuffs
more frequently," and their modifiers add gear-scaled elemental damage, bonus
crit, and on-proc shields for their own element.

<p class="muted">The per-level numbers here get large fast (rank 3 is 15% × your level, separately for each of fire/cold/lightning) - but this stat feeds a proc-chance roll that's hard-capped at 100%, not a raw damage multiplier. In practice this means a mid-level Elementalist's elemental debuffs go from "sometimes" to "reliably every hit" well before max level, not that damage itself scales unboundedly - your base attack damage, crit, and increased-damage stats are untouched by this branch.</p>

<p class="muted">Fire/Cold/Lightning/Divine/Chaos damage affixes now roll on every gear slot, not just Weapon/Helm (see <a href="/wiki/items#affixes">Items</a>) - Overshock/Polar Flux/Incinerate all scale off your own elemental gear stats, so this branch benefits directly from stacking those affixes across more of your kit than before.</p>

**Righteous Fire** - your regular attacks (including splash) deal bonus fire
damage on top of their normal damage - not a separate periodic tick, it's
folded directly into every hit you land. Separately, and still very real,
you take ongoing self-damage every second while it's active: reduced by your
own damage reduction, with whatever's left after that absorbed by your
active shield before it touches your HP - evasion and block never apply to
it (you can't dodge your own fire). Damage reduction can never fully cancel
this out, by design - it caps at mitigating 95% of the tick (see
[Combat](/wiki/combat#block-and-dr) for the universal DR cap this follows),
so at least 5% of the raw self-damage always lands even on an extremely
tanky build, and your shield absorbs that post-cap remainder, not the full
pre-mitigation amount. This is meant to be offset by the branch's own
healing/shield/DR investment (Healing Flames' self-regen, Shielding Flames,
Lightning/Chilling/Scorching Aegis's on-proc shields) rather than avoided
outright - a tanky or shield-generating Righteous Fire build can sustain it
far more comfortably than an uninvested one, by design. **Rising Phoenix**
revives a couple of recently-alive nearby allies shortly after they die - the
healing credit for that revival goes to you, not the revived ally. Cleansing
Flames periodically clears debuffs from you and nearby allies and refreshes a
few defensive buffs. Scorching Flames adds more fire damage (same
per-level/proc-chance shape as Elemental Focus above) and unlocks **Ashes to
Ashes**.

<p class="muted"><strong>⚠️ Balance in flux:</strong> Ashes to Ashes is queued for a full rework - the currently-live "execute any enemy, bosses included, below a health percentage of your own max HP" behavior is expected to be replaced, not just retuned. Don't treat its current behavior as final.</p>

**Golem Master** - summons 1-3 golems that fight alongside you, at **no
cost to your own damage** - you and every golem you field all hit at full
strength simultaneously. Big enough a system to get its own page - see
[Golems](/wiki/golems).

</div>
