<div class="card wiki-wide">

## Crafting

Every currency, every action, and the one rule that governs them all: no item can ever carry more than seven modifiers, however you get there.

<p class="muted"><strong>About prices on this page.</strong> Crafting costs are set by operator dials that can change at any time without a patch, so this page deliberately does not print most dust prices - it would go stale silently and tell you something confidently wrong. <strong>The real, current price of every action is shown on the crafting panel itself, next to the button.</strong> Where a number here is fixed in the game's code rather than on a dial, it is printed and said to be fixed.</p>

<h3 id="currencies">Currencies</h3>

<div class="wiki-currency-grid">
  <div class="wiki-currency-card"><h4>Dust</h4><p>Earned from wins and disenchanting. Pays for every currency-crafting action, Reforge, Recombine and repairs.</p></div>
  <div class="wiki-currency-card"><h4>Sand</h4><p>Spent exclusively on Polishing. Dropped by boss wins once the world is deep enough, and by disenchanting at any stage.</p></div>
  <div class="wiki-currency-card"><h4>Divine Dust</h4><p>Makes an item Sacred, or rerolls a Sacred item's implicit affix. See <a href="#divine-dust">Divine Dust</a> below.</p></div>
  <div class="wiki-currency-card"><h4>Craft Tokens</h4><p>One kind per action. Spending a token skips that action's dust cost entirely. <strong>Starting stock only</strong> - see below.</p></div>
  <div class="wiki-currency-card"><h4>Unique Shard</h4><p>A rare drop. Lets you pick which Unique Affix to grant an item - Celestial Conversion or Split Personality - or spend it on <a href="#divinity">Divinity</a>.</p></div>
</div>

<p class="muted"><strong>Craft Tokens no longer drop.</strong> You are given one of each (Transmute, Scour, Augment, Regal, Exalt, Krangle, Annulment, Chancing) when you first join, and that is the entire supply - there is no fight drop, and the "pity" payout that used to guarantee one after a dry streak has been removed too. Spend them deliberately. Unique Shards are unaffected and still drop normally.</p>

<h3 id="ceiling">The Modifier Ceiling</h3>

The single rule worth memorizing before anything else here: **four base modifiers, plus at most one each from three independent bonus sources.** Seven, and never more — on any single item, for its entire life, no matter how many times it gets reforged or recombined. Annulment and Chancing (below) only ever touch modifiers that already exist - removing or rerolling them never raises this ceiling.

<div class="wiki-ceiling">
  <div class="wiki-ceiling-row">
    <span class="wiki-slot wiki-slot--base">1</span><span class="wiki-slot wiki-slot--base">2</span><span class="wiki-slot wiki-slot--base">3</span><span class="wiki-slot wiki-slot--base">4</span>
    <span class="wiki-slot-plus">+</span>
    <span class="wiki-slot wiki-slot--bonus" title="Reforge crit">R</span><span class="wiki-slot wiki-slot--bonus" title="Recombine crit">C</span><span class="wiki-slot wiki-slot--bonus" title="Krangle">K</span>
  </div>
  <ul>
    <li><strong>1&ndash;4</strong> &mdash; Transmute / Augment / Regal / Exalt, ordinary currency crafting.</li>
    <li><strong>R</strong> &mdash; Reforge's rare crit, can land once, ever, on this item's whole lineage.</li>
    <li><strong>C</strong> &mdash; Recombine's own separate crit, same one-time rule, tracked independently of Reforge's.</li>
    <li><strong>K</strong> &mdash; Krangle, guarantees a bonus modifier, but permanently locks the item afterward.</li>
  </ul>
  <p class="wiki-ceiling-cap">Hard ceiling: <strong>7</strong> modifiers total.</p>
</div>

The R and C slots each carry a memory: while the bonus modifier either crit granted is still on the item, that same crit can't land again — even if the item is later merged into something new by Recombine, which inherits "already used" from either parent. Removing that specific modifier by any means (Annulment, for instance) re-opens the odds for that slot, same as if it had never crit at all.

A Unique Affix (from a Unique Shard) and Sacred's implicit affix live entirely outside this pool — they never count toward the seven, and no crafting action can touch them.

<h3 id="cost-formula">What a Craft Costs</h3>

Every dust-priced craft action is charged the same way, in two parts added together:

1. **A flat fee for the action.** Krangle costs more than Transmute, and so on down the list. Each action's own fee is multiplied by a single operator dial that scales all of them at once - that dial is how crafting gets made cheaper or dearer across the board, and it has been moved.
2. **A per-tier surcharge that accelerates.** It is *not* a flat rate per tier. The surcharge is the item's tier raised to an exponent above 1, so the cost per tier climbs as the tier climbs - the gap between crafting a tier-10 item and a tier-100 item is much wider than ten times. That exponent is also an operator dial and has been moved.

Both parts are rounded up independently, then summed. **The crafting panel shows you the real total before you commit** - read it there rather than computing it from this page.

<p class="muted">Two consequences worth knowing. First, every craft also bumps the item's tier, which means every craft makes the <em>next</em> craft on that item more expensive - the cost of working one item up compounds. Second, spending that action's Craft Token skips the dust entirely, both parts, which makes a token most valuable on a high-tier item, not a fresh one.</p>

<h3 id="currency-crafting">Currency Crafting</h3>

Eight actions in total. Six are gated by exactly how many modifiers the target item currently has; Annulment and Chancing just need at least one modifier to work with.

<div class="wiki-table-wrap">

| Action | Effect | Requires | Veilable |
|---|---|---|---|
| Transmute | Adds a random modifier to a bare item. | 0 modifiers | Yes |
| Augment | Adds a 2nd modifier. | 1 modifier | Yes |
| Regal | Adds a 3rd modifier. | 2 modifiers | Yes |
| Exalt | Adds a 4th modifier. | 3 modifiers | Yes |
| Scour | Strips every modifier back to none. | 1+ modifiers | No |
| Krangle | Adds one final modifier beyond the normal 4, then permanently locks the item — no further crafting of any kind, ever. | Any unlocked item | Yes |
| Annulment Orb | Removes one modifier. Unveiled: a random one goes. Veiled: rolls up to 2 candidates and you pick which leaves. | 1+ modifiers | Yes |
| Chancing | Rerolls every existing modifier to a brand-new *type* (not just a new value), each at a fresh roll range. Veiled: walks them one at a time with 3 candidates each. | 1+ modifiers | Yes |

</div>

<p class="muted">Relative cost order, cheapest to dearest, which does not change when the dials move: Transmute and Scour, then Augment, Regal, Chancing, Annulment, Exalt, and Krangle dearest of all. A locked (Krangled) item is permanently excluded from all eight.</p>

**Hideout Warrior** is a one-click macro on the dashboard's crafting card
that runs Transmute → Augment → Regal → Exalt → (optionally Krangle, via a
checkbox) against one item in sequence - skipping any step whose
precondition doesn't currently match, and always paying full dust per step
rather than spending a banked token. It stops early if you run out of dust
partway through.

<h3 id="divinity">Divinity</h3>

Costs **1 Unique Shard** and runs the whole Hideout Warrior chain - including
Krangle - over **every eligible item in your bag at once**, waiving all dust.
Equipped gear is never touched. Items that are already Krangled, or that have
**Keep** ticked, are skipped and reported back to you as skipped. Every item
it Krangles is auto-named "From Divinity".

The button only appears if you hold a Unique Shard, is disabled when nothing
in your bag is eligible, and asks for confirmation first. It is never batched -
one shard per use, no x10. If it refuses (no shard, empty bag, nothing
eligible) it costs you nothing.

<h3 id="reforge">Reforge</h3>

Raises an item's tier and rescales everything it already has to match — the closest thing to a straightforward power-up. Comes in two forms.

**Reforge Now** — the dashboard button. Costs `{{WEB_REFORGE_DUST_COST}}` dust
(a fixed price, not affected by the crafting dials) and is limited to **once
per clock hour**. Targets a random unlocked equipped item, replacing it with a
freshly reforged version in the same slot.

**Crafting-Panel Reforge** — costs `{{PANEL_REFORGE_DUST_PER_TIER}}×tier` dust
(also fixed, and note this is charged per tier at a flat rate rather than on
the accelerating curve the eight actions above use) and lets you pick the exact
item, equipped or bagged. No hourly cooldown.

**Tier jump:** +2 to +4 below tier 50, +1 to +2 at tier 50–99, +1 at tier 100+.

Every reforge also has a chance to add a bonus modifier on top of the tier increase — the "R" slot from the ceiling above. The chance scales with the item's own quality:

<div class="wiki-table-wrap">

| Item quality | Crit chance |
|---|---|
| 0% quality | {{REFORGE_CRIT_AT_0}}% |
| 50% quality | {{REFORGE_CRIT_AT_50}}% |
| 100% quality | {{REFORGE_CRIT_AT_100}}% |
| <span class="gear-unique">Perfect</span> | {{REFORGE_CRIT_AT_PERFECT}}% |

</div>

Once it fires, that's it for this item's lineage — permanently. Higher-quality gear is both better to begin with *and* slightly likelier to pick up that bonus modifier, for as long as it hasn't already.

<h3 id="recombine">Recombine</h3>

Forges two items of the same slot into one, consuming both. The result inherits the better parts of each — not everything.

- **Tier:** the average of the two sources, rounded down, plus one.
- **Modifiers:** any modifier type present on *both* sources is guaranteed to carry over (keeping the stronger of the two values). Each source's own unique modifiers get a coin-flip chance each, capped so the transferred pool never exceeds four.
- **Power roll:** a 50/50 coin flip between the two sources' rolls.
- **Unique Affix & durability:** either source's Unique Affix carries over; the result is indestructible if either source was.
- **Perfect & Sacred never carry over.** The result is always an ordinary item, regardless of what went in.

Free to recombine unveiled - no dust cost at all, blind result. **Veiling** it (or spending one of your banked free recombines, which always veils too) rolls 3 full candidate outcomes and lets you pick, and guarantees the higher power roll plus every shared modifier's transfer - it costs a flat veil fee plus a further per-modifier charge counted across *both* source items combined, shown on the panel. A rare **{{RECOMBINE_CRIT_CHANCE_PCT}}% crit** can add a bonus modifier on top of any recombine, veiled or not — the "C" slot from the ceiling, tracked and gated exactly like Reforge's own crit, independently.

<h3 id="polishing">Polishing</h3>

The only action priced in Sand instead of Dust, and the only one that improves rolls you already have rather than adding new ones.

On an ordinary item, Polishing nudges the primary stat's roll upward by a fixed step, and does the same to one random modifier that still has room to climb. On a <span class="gear-unique">Perfect</span> item — whose primary stat is already maxed — it instead nudges up to two modifiers at once. An affix already sitting at its own cap is skipped automatically.

<p class="muted">Sand costs are fixed in code, not on a dial: <code>ceil(quality% &divide; {{POLISH_SAND_COST_PER_QUALITY_PCT}})</code> sand on a normal item (1 to {{POLISH_MAX_SAND_COST}}), or a flat <code>{{POLISH_PERFECT_SAND_COST}}</code> sand on a Perfect one. Bypasses tokens and veiling entirely.</p>

<h3 id="divine-dust">Divine Dust</h3>

A separate currency that exists to make items **Sacred**.

**Getting it.** Three sources: a chance per character per boss or basic win
(only once the world is deep enough - see Stage Gates below), a chance each
time you manually disenchant a **Sacred** item (non-Sacred disenchants never
yield any, and this source is *not* stage-gated), and a craft recipe that
converts dust plus sand into Divine Dust. All the rates and the recipe's
amounts are operator dials; the recipe's real cost and output are shown on the
crafting panel. None of these announce themselves - Divine Dust arrives
silently.

**Spending it.** Applying Divine Dust to an item costs **2 × the item's tier**
in Divine Dust, on the crafting panel beside the other actions.

- On an item that is **not yet Sacred**: it becomes Sacred and gains one random affix drawn from the full pool regardless of slot, as its own implicit line.
- On an item that is **already Sacred**: it instead rerolls that implicit affix to a different one, excluding the current one.

<p class="muted"><strong>Sacralizing also makes the item Perfect, and this surprises people.</strong> If the item was not already Perfect, applying Divine Dust maxes its power roll and bumps every existing modifier by the Perfect quality multiplier <em>in the same click</em> - it is not merely "gains a sacred affix" on top of whatever quality it had. This is deliberate: everything else in the game assumes Sacred implies Perfect. It means a mediocre-quality item is a much better Divine Dust target than it looks.</p>

Krangle's lock applies: a locked item rejects Divine Dust like every other crafting action.

<h3 id="stage-gates">Stage Gates</h3>

Four drop types don't exist at all until the shared world stage is deep enough.
All four thresholds are operator dials; the **stage they gate on is the current
world stage**, so a losing streak that walks the stage back below a threshold
temporarily switches that drop off again.

<div class="wiki-table-wrap">

| Drop | Gated on |
|---|---|
| Polishing sand (from fight wins) | A mid-early stage threshold |
| <span class="gear-unique">Perfect</span> items | A higher threshold than sand |
| Divine Dust (from fight wins) | A late threshold |
| <span class="gear-sacred">Sacred</span> items | The same late threshold as Divine Dust |

</div>

<p class="muted"><strong>Two ways around the gates.</strong> Sand and Divine Dust from <em>disenchanting</em> are deliberately not gated - breaking gear down yields both at any stage, just at low volume compared to what a boss win pays once the gate opens. That's a real route, not a loophole.</p>

<p class="muted"><strong>The Divine Dust craft recipe is locked separately, and unlike the drops it latches.</strong> It unlocks when the group reaches the Divine Dust stage threshold and then <em>stays</em> unlocked forever, even if the world stage later falls back below it. While locked, the recipe shows on the crafting page as a locked row naming the stage it needs, with no usable form.</p>

<h3 id="celestial-shard">Unique Shard</h3>

The only way to grant a Unique Affix — a permanent, implicit bonus that lives entirely outside the normal modifier pool.

Consuming a Unique Shard token (never dust) opens a picker letting you choose which Unique Affix to grant, at no extra cost:

- **Celestial Conversion** — effect depends on the wielder's role. **Healers** convert {{CELESTIAL_CONVERSION_PCT}}% of every heal into bonus damage against a random enemy. **Every other archetype** instead lands a follow-up hit on whatever they just struck, for {{CELESTIAL_CONVERSION_PCT}}% of that hit's damage — a real second hit that can trigger Leech, elemental procs, and anything else an on-hit effect would.
- **Split Personality** — unlocks a second class's passive tree on that item. See [Classes &amp; Passives](/wiki/classes#split-personality) for the full mechanic.

<p class="muted">There is only one shard currency. "Celestial Shard" was a separate token once; it was merged into Unique Shard, and any Celestial Shards you held converted 1:1 automatically. Nothing drops Celestial Shards any more, and any older text you find calling Celestial Conversion "the Celestial Shard affix" is out of date - both effects come from the same Unique Shard picker now.</p>

<p class="muted">You can only have one copy of a given unique effect <em>equipped</em> at a time. Applying a shard to an item you are already wearing will offer you only the non-conflicting choice, or refuse outright if every choice would duplicate something you already have equipped - and a refused attempt does not spend the shard. Applying one to an item sitting in your bag is never restricted.</p>

<p class="muted">Mutually exclusive with Krangle: an item carrying a Unique Affix can never be Krangled, and vice versa.</p>

<h3 id="item-tiers">Item Tiers</h3>

Beyond the normal 0–100% quality roll, two rarer tiers exist. Both drop from boss kills; Sacred can also be crafted onto an item with Divine Dust.

<div class="wiki-tier-grid">
  <div class="wiki-tier-card"><h4>Normal</h4><p>Rolls somewhere in the standard quality band. What every drop starts as.</p></div>
  <div class="wiki-tier-card"><h4><span class="gear-unique">Perfect Quality</span></h4><p>Primary stat pinned to its maximum roll, plus a flat {{PERFECT_QUALITY_BONUS_PCT}}% bonus to that stat <em>and</em> every modifier. Guaranteed on a boss kill once the world passes the Perfect threshold &mdash; once per kill, and once more the first time each character personally takes part in one. Once Sacred also starts dropping, that per-kill guarantee only fires half as often.</p></div>
  <div class="wiki-tier-card"><h4><span class="gear-sacred">Sacred</span></h4><p>Everything Perfect Quality gets, plus one further modifier &mdash; drawn from the full affix pool regardless of slot, rolled at its own maximum, shown as its own implicit line. Outside the 4-modifier pool entirely; no crafting action can ever touch it. Same drop mechanism as Perfect, at a deeper stage &mdash; or apply Divine Dust yourself.</p></div>
</div>

<h3 id="veiling">Veiling</h3>

Most crafts are a blind gamble — you commit, then see what you got. Veiling flips that: pay extra to see the exact result first, and choose whether to keep it.

Available on Transmute, Augment, Regal, Exalt, Krangle, Annulment, Chancing, and Recombine. **Not** available on Scour (nothing to choose — it's fully deterministic), Unique Shard (its own free effect picker isn't part of this system), Divine Dust, Polishing (bypasses this system entirely), or Reforge.

<p class="muted">Veiling any currency-craft action other than Recombine adds a flat dust surcharge on top of its base + per-tier cost. That surcharge is scaled by the same operator dial as the base fees, so it moves with them &mdash; the panel shows the real figure. Recombine's own veil cost works differently &mdash; see Recombine above.</p>

<h3 id="disenchanting">Disenchanting</h3>

Turns an unwanted item straight into Dust, valued by how much was invested in it. It's also an ungated source of Sand, and of Divine Dust from Sacred items.

<div class="wiki-table-wrap">

| Normal modifiers | Ordinary | Perfect | Sacred |
|---|---|---|---|
| 0 | 1× | 5× | 25× |
| 1 | 5× | 25× | 50× |
| 2 | 10× | 50× | 75× |
| 3 | 15× | 75× | 100× |
| 4 | 20× | 100× | 125× |

</div>

<p class="muted">Sacred's bonus implicit counts as one more modifier toward this same table &mdash; a Sacred item with <em>N</em> normal modifiers is always worth exactly what a Perfect item with <em>N+1</em> would be.</p>

Toggle **Keep** on any item to protect it. **Keep now blocks every kind of
modification, not just disenchanting** - crafts, Polish, Reforge, Divine Dust,
applying a unique, Recombine as either input, and Divinity all refuse a
Keep-ticked item. Repair and Krangle's level growth still apply. It's
independent of Krangle's own lock and reversible any time.

A Krangled item can still be disenchanted; Krangle's lock only ever blocks further crafting.

**Auto-disenchant** can be set to a threshold — Quality, Perfect, or Sacred — so any new drop at or below that tier is converted to Dust automatically the moment it's picked up, instead of filling the bag.

<h3 id="quick-reference">Quick Reference</h3>

<div class="wiki-table-wrap">

| Action | Currency | Cost | Notable rule |
|---|---|---|---|
| Transmute | Dust | On the panel — dial-scaled | Bare items only |
| Augment | Dust | On the panel — dial-scaled | 1 modifier only |
| Regal | Dust | On the panel — dial-scaled | 2 modifiers only |
| Exalt | Dust | On the panel — dial-scaled | 3 modifiers only |
| Scour | Dust | On the panel — dial-scaled | Not veilable |
| Krangle | Dust | On the panel — dial-scaled, dearest action | Locks the item forever |
| Annulment Orb | Dust | On the panel — dial-scaled | Needs 1+ modifiers |
| Chancing | Dust | On the panel — dial-scaled | Needs 1+ modifiers |
| Reforge (dashboard) | Dust | {{WEB_REFORGE_DUST_COST}}, fixed | Random equipped item, once per clock hour |
| Reforge (crafting panel) | Dust | {{PANEL_REFORGE_DUST_PER_TIER}}/tier, fixed | Choose the exact item |
| Recombine | Dust | Free unveiled; veil fee + per-modifier on the panel | Perfect/Sacred never carry over |
| Polishing | Sand | 1–{{POLISH_MAX_SAND_COST}}, or {{POLISH_PERFECT_SAND_COST}} flat if Perfect — fixed | Not veilable |
| Divine Dust | Divine Dust | 2 × tier, fixed | Sacralizes (and makes Perfect), or rerolls |
| Unique Shard | Shard token | 1 token | Excludes Krangle |
| Divinity | Shard token | 1 token | Whole bag, no dust |

</div>

</div>
