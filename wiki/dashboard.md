<div class="card wiki-wide">

## Web Dashboard &amp; Overlay

<h3 id="account">Your Account</h3>

The game has **its own account system**. You register a username and a
password at `/account/register` and log in at `/account/login` - that account
is what owns your character. It is not connected to Twitch in any way, and you
do not need a Twitch account to play. Logging in with Twitch used to be the
only way in; that path has been removed entirely.

<p class="muted"><strong>There is no password reset.</strong> No email recovery, no reset link, no in-game flow, no way to ask for one. If you lose your password the account cannot currently be recovered. Store it somewhere you won't lose it.</p>

Some usernames are refused at registration - anything that collides with an
existing character or an operator/system name.

<h3 id="main">Dashboard &amp; Bag</h3>

Your main dashboard (`/`) shows your profile, level/XP, archetype and model
pickers, your gear grid, the Feed (below), and the dashboard's own
Reforge/Repair-All/auto-repair controls. Crafting itself - every action from
[Crafting](/wiki/crafting), Recombine, Divinity, and the Hideout Warrior macro
- along with your full bag, item nicknaming, and auto-disenchant settings live
on a separate **Bag &amp; Crafting** page linked from the dashboard, so the
main page doesn't get cluttered.

If you have an account but no character yet, the dashboard shows a **Join the
Adventure** button. Pressing it creates your character - see
[Getting Started](/wiki/getting-started#joining).

<h3 id="feed">The Feed</h3>

A card on your dashboard carrying the game's own narration, newest first:
encounter results and batched fight summaries, loot and gear-crit lines, boss
arrivals, Unique Shard finds, rampage completion, level-ups.

**This is where the game talks to you now.** None of it goes to Twitch chat
any more. The Feed holds a rolling window of the most recent lines and is not
a permanent log - it lives in memory only and does not survive a server
restart. For a durable record of a specific fight, use Fight History below.

<h3 id="passives-page">Passive Tree (Interactive)</h3>

`/passives` is where you actually spend your points - the read-only
[Passives](/wiki/passives) page on this wiki shows every class's tree at a
glance, but allocating, saving, resetting, and respeccing your own build all
happen here. See [Classes &amp; Passives](/wiki/classes#points) for how the
points themselves work. If you have Split Personality equipped, your second
class's tree and picker appear on this same page, as does the golem slot-type
picker if you're an Elementalist with Golem Master invested.

Your saved build **Memories** are managed here too - three slots per
character, capturing your whole build (archetype, tree, secondary archetype
and its tree, golem slot types). Saving and loading are free and bypass the
usual archetype-change and respec costs, so one paid class change plus a saved
Memory buys unlimited free switching between those builds afterwards. Loading
is allowed **out of combat only**.

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

<p class="muted">Golems do not appear on the overlay - see <a href="/wiki/golems#rules">Golems</a>. They are fully real in combat; they just have no sprite yet.</p>

<p class="muted">Two undocumented-anywhere-else query params work on the overlay URL: <code>?highlight=&lt;login&gt;</code> dims every other character to spotlight one specific hero (handy for a "watch my own fight" link), and <code>?bgOpacity=&lt;0-1&gt;</code> adjusts the background transparency. Neither is discoverable in-app - this page is the only place they're written down.</p>

</div>
