<div class="card wiki-wide">

## Web Dashboard &amp; Overlay

Everything on this page is reached by logging in with your Twitch account at
the top of the site - your Twitch login *is* your character, there's no
separate account system.

<h3 id="main">Dashboard &amp; Bag</h3>

Your main dashboard (`/`) shows your profile, level/XP, archetype and model
pickers, your gear grid, and the dashboard's own Reforge/Repair-All/
auto-repair controls. Crafting itself - every action from
[Crafting](/wiki/crafting), Recombine, and the Hideout Warrior macro - along
with your full bag, item nicknaming, and auto-disenchant settings live on a
separate **Bag &amp; Crafting** page linked from the dashboard, so the main
page doesn't get cluttered.

<h3 id="passives-page">Passive Tree (Interactive)</h3>

`/passives` is where you actually spend your points - the read-only
[Passives](/wiki/passives) page on this wiki shows every class's tree at a
glance, but allocating, saving, resetting, and respeccing your own build all
happen here. See [Classes &amp; Passives](/wiki/classes#points) for how the
points themselves work. If you have Split Personality equipped, your second
class's tree and picker appear on this same page.

<h3 id="characters">Character List</h3>

`/characters` lists every character that's ever joined, and `/characters/:login`
shows a read-only mirror of anyone's dashboard - gear, stats, and (if they've
specialized) a link to their passive tree. Handy for checking out another
player's build.

<h3 id="fights">Fight History</h3>

`/fights` is a real, player-facing feature - a per-fight breakdown of recent
encounters you took part in, including boss stats, a battle-report summary,
and skill-cast/buff-activity detail. The history window it shows is
currently short and under active review, so don't be surprised if a fight
you were just in hasn't appeared yet - that's being worked on, not intended
long-term behavior.

<h3 id="patch-notes">Patch Notes</h3>

`/patch-notes` is a public changelog, no login needed - the closest thing to
"what changed recently" for the whole game.

<h3 id="overlay">The OBS Overlay</h3>

`/overlay` is the same animated view streaming on screen during a fight,
also viewable in a plain browser tab - no login needed. Green/yellow/red HP
bars track health at a glance, white damage numbers pop at the target
(bigger gold/orange ones for crits), gray text marks a miss, and a heal
shows as a visual spark between healer and target rather than a floating
number.

<p class="muted">Two undocumented-anywhere-else query params work on the overlay URL: <code>?highlight=&lt;login&gt;</code> dims every other character to spotlight one specific hero (handy for a "watch my own fight" link), and <code>?bgOpacity=&lt;0-1&gt;</code> adjusts the background transparency. Neither is discoverable in-app - this page is the only place they're written down.</p>

</div>
