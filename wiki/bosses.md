<div class="card">

## Bosses

Every world boss fight pits the party against one or more of five named bosses. Each one plays completely differently - here's what to watch for.

<h3 id="dragon">🐲 The Dragon</h3>

Flies back and forth across the battlefield instead of standing on the ground, breathing fire with **every attack** it lands - unlike a normal attack, which only hits one hero, this sweeps the **whole party at once**, so nobody is safe. It also radiates a passive aura for the entire fight that slows everyone's attack speed by {{DRAGON_SLOW_PCT}}%. Two looks can show up in a Dragon fight - the classic purple dragon, or the rarer, fully-animated Bahamut - purely cosmetic, same fight either way.

<h3 id="cthulhu">🐙 Cthulhu</h3>

Every {{CTHULHU_DEBUFF_CADENCE_S}} seconds, a purple bubble locks onto roughly half the party at random and stacks a debuff that cuts both damage AND healing from anyone it lands on, capped at a brutal {{CTHULHU_DEBUFF_CAP_PCT}}% reduction - no single hero can carry a Cthulhu fight alone, and hanging back doesn't save you either: sooner or later the bubble finds everyone.

<h3 id="lich">💀 The Lich</h3>

Raises {{LICH_ADDS_PER_SUMMON}} skeletons to fight alongside it every {{LICH_SUMMON_CADENCE_S}} seconds, up to {{LICH_MAX_ADDS}} in a single battle. Each one is individually weak - a fraction of the Lich's own health and attack - but they pile up fast if the fight drags on, so speed matters against this one.

<h3 id="fire-demon">🔥 Fire Demon</h3>

No flashy abilities - just relentless heat. Its aura cuts the entire party's healing power by {{FIRE_DEMON_HEAL_MULT_PCT}}% for the whole fight, so healers get a lot less mileage out of every cast.

<h3 id="gelatinous-cube">🧊 The Gelatinous Cube</h3>

Crawls across the field absorbing party members into its body - every {{CUBE_CAPTURE_CADENCE_S}} seconds it rotates a fresh {{CUBE_CAPTURE_PCT}}% of whoever's still standing into itself, and while trapped they can't act, though allies can still damage or heal them normally. It also lashes out with a splash attack hitting {{CUBE_SPLASH_TOTAL_TARGETS}} heroes at once, and every hit it lands stacks a {{CUBE_SHRED_PCT_PER_STACK}}% defense shred on that hero (up to {{CUBE_SHRED_MAX_STACKS}} stacks, {{CUBE_SHRED_MAX_PCT}}% total, each stack lasting {{CUBE_SHRED_DURATION_S}}s) - the longer a fight against the Cube drags on, the softer the party gets.

### Multiple bosses at once

As the world stage climbs, more bosses join the fight at once - it's not a fixed number for a given stage, either: every fight rolls its own boss count with some real variance, trending upward the higher the stage, so two fights at the same stage can genuinely differ in size. It's always a different mix - the game never repeats the last fight's boss back-to-back, and won't repeat a kind WITHIN one fight either, unless there are more boss slots than distinct kinds left to fill them (only relevant at the very highest stages, since there are only five named kinds total).

<p class="muted">These five only show up in real world-boss fights. The smaller, more frequent filler encounters between them use weaker, unnamed enemies instead.</p>

<h3 id="basic-enemies">Filler Encounter Enemies</h3>

Basic encounters draw from a pool of **50 distinct enemy sprites**, one picked
per enemy in the group. The server picks them, not your browser, which means
the fight looks the same for everyone watching and looks the same again on a
replay - it used to be re-rolled at render time, so it differed on every
screen and every rewatch.

<p class="muted">Presentation only - no stats, costs, chances or mechanics changed, and these enemies are still mechanically bare (see <a href="/wiki/combat#basic-vs-boss">Combat</a>). The enemy <em>name</em> you see announced ("a pack of Wild Wolves") is rolled separately from the artwork, so the two are not expected to match.</p>

</div>
