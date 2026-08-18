<div class="card wiki-wide">

## Crafting

Every currency, every action, and the one rule that governs them all: no item can ever carry more than seven modifiers, however you get there.

<h3 id="currencies">Currencies</h3>

<div class="wiki-currency-grid">
  <div class="wiki-currency-card"><h4>Dust</h4><p>Earned from wins and boss kills. Pays for every currency-crafting action, Reforge, and Recombine.</p></div>
  <div class="wiki-currency-card"><h4>Sand</h4><p>Earned from wins and disenchanting. Spent exclusively on Polishing.</p></div>
  <div class="wiki-currency-card"><h4>Craft Tokens</h4><p>One kind per action (Transmute, Scour, Augment, Regal, Exalt, Krangle). Spending a token skips that action's dust cost entirely.</p></div>
  <div class="wiki-currency-card"><h4>Celestial Shard</h4><p>A rare token, separate from every other currency. The only way to grant a Unique Affix.</p></div>
</div>

<h3 id="ceiling">The Modifier Ceiling</h3>

The single rule worth memorizing before anything else here: **four base modifiers, plus at most one each from three independent bonus sources.** Seven, and never more — on any single item, for its entire life, no matter how many times it gets reforged or recombined.

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

The R and C slots each carry a memory: once either crit has landed on an item, it can never land again — even if that item is later merged into something new by Recombine. A merged item inherits "already used" from either parent, so pairing a just-crit'd item with a fresh one can't reset its odds.

A Unique Affix (from a Celestial Shard) and Sacred's implicit affix live entirely outside this pool — they never count toward the seven, and no crafting action can touch them.

<h3 id="currency-crafting">Currency Crafting</h3>

Six actions, each gated by exactly how many modifiers the target item currently has. Every one of them also bumps the item's tier as a side effect — `+3` below tier 25, `+2` below tier 50, `+1` beyond that.

<div class="wiki-table-wrap">

| Action | Effect | Requires | Dust | Veilable |
|---|---|---|---|---|
| Transmute | Adds a random modifier to a bare item. | 0 modifiers | 250 | Yes |
| Augment | Adds a 2nd modifier. | 1 modifier | 500 | Yes |
| Regal | Adds a 3rd modifier. | 2 modifiers | 750 | Yes |
| Exalt | Adds a 4th modifier. | 3 modifiers | 1,250 | Yes |
| Scour | Strips every modifier back to none. | 1+ modifiers | 250 | No |
| Krangle | Adds one final modifier beyond the normal 4, then permanently locks the item — no further crafting of any kind, ever. | Any unlocked item | 2,500 | Yes |

</div>

<p class="muted">Every action above also costs <code>3&times;tier</code> extra dust on top of its base price &mdash; unless you spend that action's own Craft Token instead, which skips the dust entirely. A locked (Krangled) item is permanently excluded from all six.</p>

<h3 id="reforge">Reforge</h3>

Raises an item's tier and rescales everything it already has to match — the closest thing to a straightforward power-up. Comes in two forms.

**Reforge Now** — a free action (via the dashboard button or its matching channel-points redemption), or `1,000` dust on demand. Always targets a random unlocked equipped item, replacing it with a freshly reforged version at the same slot.

**Crafting-Panel Reforge** — costs `30×tier` dust and lets you pick the exact item, equipped or bagged, rather than leaving it to chance. No cooldown, unlike the free button.

**Tier jump:** +2 to +4 below tier 50, +1 to +2 at tier 50–99, +1 at tier 100+.

Every reforge also has a chance to add a bonus modifier on top of the tier increase — the "R" slot from the ceiling above. The chance scales with the item's own quality:

<div class="wiki-table-wrap">

| Item quality | Crit chance |
|---|---|
| 0% quality | 1.0% |
| 50% quality | 1.5% |
| 100% quality | 2.0% |
| <span class="gear-unique">Perfect</span> | 2.2% |

</div>

Once it fires, that's it for this item's lineage — permanently. Higher-quality gear is both better to begin with *and* slightly likelier to pick up that bonus modifier, for as long as it hasn't already.

<h3 id="recombine">Recombine</h3>

Forges two items of the same slot into one, consuming both. The result inherits the better parts of each — not everything.

- **Tier:** the average of the two sources, rounded down, plus one.
- **Modifiers:** any modifier type present on *both* sources is guaranteed to carry over (keeping the stronger of the two values). Each source's own unique modifiers get a coin-flip chance each, capped so the transferred pool never exceeds four.
- **Power roll:** a 50/50 coin flip between the two sources' rolls.
- **Unique Affix & durability:** either source's Unique Affix carries over; the result is indestructible if either source was.
- **Perfect & Sacred never carry over.** The result is always an ordinary item, regardless of what went in.

Costs `{{RECOMBINE_DUST_COST}}` dust, or one of your free recombines if you have one banked. A rare **5% crit** can add a bonus modifier on top — the "C" slot from the ceiling, tracked and gated exactly like Reforge's own crit, independently.

<p class="muted">Veiled Recombine guarantees the higher of the two power rolls and the certain transfer of every shared modifier &mdash; see Veiling below for the cost.</p>

<h3 id="polishing">Polishing</h3>

The only action priced in Sand instead of Dust, and the only one that improves rolls you already have rather than adding new ones.

On an ordinary item, Polishing nudges the primary stat's roll upward by a fixed step, and does the same to one random modifier that still has room to climb. On a <span class="gear-unique">Perfect</span> item — whose primary stat is already maxed — it instead nudges up to two modifiers at once. An affix already sitting at its own cap is skipped automatically.

<p class="muted">Cost scales with how much room is left to improve: <code>ceil(quality% &divide; 10)</code> sand on a normal item (1 to 10), or a flat <code>12</code> sand on a Perfect one. Bypasses tokens and veiling entirely.</p>

<h3 id="celestial-shard">Celestial Shard</h3>

The only way to grant a Unique Affix — a permanent, implicit bonus that lives entirely outside the normal modifier pool.

Consumes a Celestial Shard token (never dust) to grant **Celestial Conversion**, whose effect depends on the wielder's role:

- **Healers** convert 10% of every heal into bonus damage against a random enemy.
- **Every other archetype** instead lands a follow-up hit on whatever they just struck, for 10% of that hit's damage — a real second hit that can trigger Leech, elemental procs, and anything else an on-hit effect would.

<p class="muted">Mutually exclusive with Krangle: an item carrying a Unique Affix can never be Krangled, and vice versa.</p>

<h3 id="item-tiers">Item Tiers</h3>

Beyond the normal 0–100% quality roll, two rarer tiers exist — both earned from boss kills, never crafted.

<div class="wiki-tier-grid">
  <div class="wiki-tier-card"><h4>Normal</h4><p>Rolls somewhere in the standard quality band. What every drop starts as.</p></div>
  <div class="wiki-tier-card"><h4><span class="gear-unique">Perfect Quality</span></h4><p>Primary stat pinned to its maximum roll, plus a flat 20% bonus to that stat <em>and</em> every modifier. Guaranteed on stage-100+ boss kills &mdash; once per kill, and once more the first time each character personally takes part in one. Once Sacred also starts dropping (stage 300+), that per-kill guarantee only fires half as often.</p></div>
  <div class="wiki-tier-card"><h4><span class="gear-sacred">Sacred</span></h4><p>Everything Perfect Quality gets, plus one further modifier &mdash; drawn from the full affix pool regardless of slot, rolled at its own maximum, shown as its own implicit line. Outside the 4-modifier pool entirely; no crafting action can ever touch it. Same drop mechanism as Perfect, gated to stage 300+.</p></div>
</div>

<h3 id="veiling">Veiling</h3>

Most crafts are a blind gamble — you commit, then see what you got. Veiling flips that: pay extra to see the exact result first, and choose whether to keep it.

Available on Transmute, Augment, Regal, Exalt, Krangle, and Recombine. **Not** available on Scour (nothing to choose — it's fully deterministic), Celestial Shard (always the same grant), Polishing (bypasses this system entirely), or Reforge.

<p class="muted">A veiled Recombine costs an extra <code>500</code> dust flat, plus <code>500</code> per modifier in the guaranteed-transfer pool.</p>

<h3 id="disenchanting">Disenchanting</h3>

Turns an unwanted item straight into Dust, valued by how much was invested in it.

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

Toggle **Keep** on any item to protect it from both single and bulk disenchanting — independent of Krangle's lock, and reversible any time. A Krangled item can still be disenchanted; locking only ever blocks further crafting.

**Auto-disenchant** can be set to a threshold — Quality, Perfect, or Sacred — so any new drop at or below that tier is converted to Dust automatically the moment it's picked up, instead of filling the bag.

<h3 id="quick-reference">Quick Reference</h3>

<div class="wiki-table-wrap">

| Action | Currency | Base cost | Notable rule |
|---|---|---|---|
| Transmute | Dust | 250 + 3/tier | Bare items only |
| Augment | Dust | 500 + 3/tier | 1 modifier only |
| Regal | Dust | 750 + 3/tier | 2 modifiers only |
| Exalt | Dust | 1,250 + 3/tier | 3 modifiers only |
| Scour | Dust | 250 + 3/tier | Not veilable |
| Krangle | Dust | 2,500 + 3/tier | Locks the item forever |
| Reforge (free) | — | 0 | Random equipped item |
| Reforge (on demand) | Dust | 1,000 | Random equipped item |
| Reforge (crafting panel) | Dust | 30/tier | Choose the exact item |
| Recombine | Dust | 500 | +500 veil, +500/modifier |
| Polishing | Sand | 1–10, or 12 flat if Perfect | Not veilable |
| Celestial Shard | Shard token | 1 token | Excludes Krangle |

</div>

</div>
