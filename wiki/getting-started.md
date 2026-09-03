<div class="card wiki-wide">

## Getting Started

<p class="muted"><strong>Worlds reset.</strong> The shared world is periodically wiped and started over from scratch - characters, levels, gear, currencies and the world stage all go back to the beginning, for everyone at once. Nothing carries across a reset. No schedule for this has been set, so treat everything you build as belonging to the current world rather than as permanent progress.</p>

<h3 id="joining">Joining</h3>

Two steps, both on the website - none of this happens in chat any more.

1. **Make an account.** Register a username and password at `/account/register`. This is the game's own account system; it has nothing to do with Twitch, and you do not need a Twitch account to play.
2. **Press "Join the Adventure"** on your dashboard. That creates your character, fully kitted out with a starting item in every slot (you don't start naked) and one starting craft token of each kind, and drops you into the shared world.

Every joined character auto-battles together against whatever the game throws
at the party. If you've retreated (see below), the same Join button is how you
get back in.

<p class="muted"><strong>There is no password reset.</strong> None exists - not by email, not by support request, not by any in-game flow. If you lose your password there is currently no way to recover the account. Write it down somewhere safe.</p>

<h3 id="leveling">XP &amp; Leveling</h3>

**Winning real boss fights is the only source of XP in the game.** Basic filler
encounters pay none, and neither do losses. Chatting earns nothing - a chat
activity trickle used to exist and has been removed entirely.

A win's XP is priced off **your own level**, not the world stage: it's a flat
amount plus a share of whatever your next level costs, so it stays meaningful
as you climb instead of being dictated by how deep the world has gone. Both of
those terms, and an overall multiplier, are operator-set dials - the current
values live on the admin page rather than being published here.

<p class="muted"><strong>There is a minimum gap between two XP-paying wins.</strong> Win a second boss fight inside that window and you still get the loot, the stage movement and everything else - but no XP for it. This is deliberate: it stops a burst of back-to-back fights (see Rampage below) from paying many times the normal rate and setting the leveling curve instead of the schedule. The normal boss cadence is comfortably longer than the window, so it never binds on a normally-scheduled fight. If you win twice in quick succession and only level once, this is why - it isn't a bug.</p>

<p class="muted">The XP needed for your next level grows faster than linearly, so early levels fly by and later ones take real investment: level 2 needs {{XP_TO_LEVEL_2}} XP, level 11 needs {{XP_TO_LEVEL_11}}, level 26 needs {{XP_TO_LEVEL_26}}, and level 51 needs {{XP_TO_LEVEL_51}}. There's no level cap.</p>

If you're behind the rest of the roster, boss-fight rewards scale up for you
automatically until you catch up - lagging behind isn't a permanent
disadvantage.

Leveling up raises your base HP/attack/defense automatically - there's no
separate stat-point allocation for base stats (that's what the [passive
tree](/wiki/passives) is for).

<h3 id="death-and-retreat">Getting Knocked Out vs. Retreating</h3>

There's no permadeath. Two different things can take you out of the fight
temporarily:

**Downed** - if you actually go to 0 HP in a fight, you sit out for a short
window ({{REVIVE_DURATION_S}} seconds) and then you're automatically back,
no action needed. This applies to basic encounters too, not just real boss
fights.

**Retreated** - your gear wears down a little with every *real boss fight*
you take part in (basic encounters never cost durability). Once every piece
of equipped gear you own is fully worn out, you're pulled off the
battlefield entirely until you deal with it. You get back in by: paying dust
to repair, swapping in fresh gear from your bag, or just waiting - after
{{RETREAT_REPAIR_DURATION_MIN}} minutes of retreat, the game auto-repairs
everything for free and puts you straight back on the roster. Pressing Join
while retreated also works as an explicit "I'm back."

An optional **auto-repair** toggle on your dashboard spends dust to fix your
gear immediately after every real boss fight, so you effectively never
retreat as long as you can afford it - off by default.

<h3 id="archetype">Choosing (and Changing) Your Class</h3>

Everyone starts as **Commoner** - a plain, unspecialized fighter with no
bonus and no penalty. Pick a real class from your web dashboard whenever
you're ready; see [Classes &amp; Passives](/wiki/classes) for what each of the
12 does. Changing your mind later costs **{{ARCHETYPE_CHANGE_COST}} dust**
after a couple of free changes are used up, and **fully clears your passive
tree** - it's a real decision, not a free respec-and-keep. Picking Commoner
back is never allowed once you've specialized.

<h3 id="world-stage">World Stage</h3>

There's one shared "world stage" number for the entire game, not per
character - it climbs by 1 on every real boss-fight win, and drops by 1 on a
loss (never below 1). Higher stages mean tougher, often multi-boss fights
(the exact count varies fight-to-fight, trending upward with stage) and
better loot: several drop types don't unlock at all until the world reaches
a given stage - see [Crafting](/wiki/crafting#stage-gates).

Stage also quietly makes real bosses harder in a way defensive gear can't
answer - see [Combat](/wiki/combat#boss-pierce).

By default, a real encounter fires every **{{ENCOUNTER_INTERVAL_MIN}}
minutes**, with a smaller, easier filler encounter roughly every
**{{BASIC_ENCOUNTER_INTERVAL_MIN}} minutes** in between (no durability cost,
no stage movement, no XP - see [Combat](/wiki/combat#basic-vs-boss) for how
much simpler these are mechanically).

**Rampage** replaces that timer with continuous back-to-back boss fights,
everyone instantly revived between each, with a minimum of
{{RAMPAGE_MIN_INTERVAL_S}} seconds between fights. It is an operator toggle -
there is no way for a player to trigger one or vote for one. If fights seem
to be firing constantly rather than every {{ENCOUNTER_INTERVAL_MIN}} minutes,
that toggle is on. The XP window above still applies during a rampage, so
most rampage wins pay loot without paying XP.

<h3 id="announcements">The Adventure Feed</h3>

The game narrates itself - encounter results, loot, batched fight summaries,
rampage completion, Unique Shard finds, gear crits, level-ups. **All of that
appears in the Feed card on your dashboard**, newest first. It is not posted
to Twitch chat; the game no longer speaks to chat at all.

Routine fight-result messages (win/loss, Top DPS/Tanks/Heals) don't post one
at a time per fight - they batch into a single summary covering multiple
fights at once. The Top DPS/Tanks/Heals lines in that summary are totals
across the whole batch, not any one fight. A new boss's arrival and per-fight
loot/gear-crit lines still post to the Feed immediately.

<h3 id="roster">The Roster</h3>

This isn't a party-invite system - every joined character is permanently
part of one shared roster, and every encounter battles with *everyone*
currently eligible (joined, not downed, not retreated) at once. There's no
way to opt just yourself into a specific fight. Rewards (loot, dust, sand)
are rolled and handed out independently per participant, not pooled and
split.

<h3 id="cosmetics">Cosmetics</h3>

**Model/sprite**: purely visual, no combat effect. Costs
**{{MODEL_CHANGE_COST}} dust** after your first free change - {{MODEL_CHANGES_FREE_FOR_ALL}}
right now while a big new sprite set gets tested, so check your dashboard for
the current price if this note goes stale.

**Wings of Flight**: a rare cosmetic, no combat effect either. Buy it outright
for **{{WINGS_COST}} dust**, or hope for the extremely rare bonus drop
alongside a normal item reward.

<h3 id="bag">Your Bag</h3>

Your inventory holds up to **{{INVENTORY_CAPACITY}}** items before new drops
start getting lost - keep it trimmed with Disenchanting or Auto-Disenchant
(see [Crafting](/wiki/crafting)).

</div>
