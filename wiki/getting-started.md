<div class="card wiki-wide">

## Getting Started

<h3 id="joining">Joining</h3>

Type `!join` in chat. That's it - it creates a permanent character tied to
your Twitch login, fully kitted out with a starting item in every slot (you
don't start naked), and drops you straight into the shared world: every
joined character auto-battles together against whatever the game throws at
the party. Running `!join` again while already joined is harmless - it just
tells you your current level. If you've retreated (see below), `!join` is
also how you get back in.

<h3 id="leveling">XP &amp; Leveling</h3>

You earn XP two ways: chatting (a small trickle, rate-limited so spamming
doesn't help - {{ACTIVITY_XP_AMOUNT}} XP per message, at most once every
{{ACTIVITY_XP_COOLDOWN_S}} seconds) and winning real boss fights (a much
bigger chunk, scaled to the current world stage). Basic filler encounters
grant no XP at all.

<p class="muted">The XP needed for your next level grows faster than linearly, so early levels fly by and later ones take real investment: level 2 needs {{XP_TO_LEVEL_2}} XP, level 11 needs {{XP_TO_LEVEL_11}}, level 26 needs {{XP_TO_LEVEL_26}}, and level 51 needs {{XP_TO_LEVEL_51}}. There's no level cap.</p>

If you're behind the rest of the roster, boss-fight rewards (including the
lucky-drop "pity" payouts below) scale up for you automatically until you
catch up - lagging behind isn't a permanent disadvantage.

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
to repair, swapping in fresh gear from your bag, redeeming the free "Repair
All Gear" channel-points reward, or just waiting - after
{{RETREAT_REPAIR_DURATION_MIN}} minutes of retreat, the game auto-repairs
everything for free and puts you straight back on the roster, no `!join`
needed. Typing `!join` while retreated also works as an explicit "I'm back."

An optional **auto-repair** toggle on your dashboard spends dust to fix your
gear immediately after every real boss fight, so you effectively never
retreat as long as you can afford it - off by default.

<h3 id="archetype">Choosing (and Changing) Your Class</h3>

Everyone starts as **Commoner** - a plain, unspecialized fighter with no
bonus and no penalty. Pick a real class from your web dashboard whenever
you're ready; see [Classes &amp; Passives](/wiki/classes) for what each of the
11 does. Changing your mind later costs **{{ARCHETYPE_CHANGE_COST}} dust**
after a couple of free changes are used up, and **fully clears your passive
tree** - it's a real decision, not a free respec-and-keep. Picking Commoner
back is never allowed once you've specialized.

<h3 id="world-stage">World Stage</h3>

There's one shared "world stage" number for the entire game, not per
character - it climbs by 1 on every real boss-fight win, and drops by 1 on a
loss (never below 1). Higher stages mean tougher, often multi-boss fights
(the exact count varies fight-to-fight, trending upward with stage) and
better guaranteed loot - see [Bosses](/wiki/bosses) and
[Crafting](/wiki/crafting)'s Item Tiers section for the Perfect/Sacred
guarantees that kick in at high stages.

By default, a real encounter fires every **{{ENCOUNTER_INTERVAL_MIN}}
minutes**, with a smaller, easier filler encounter roughly every
**{{BASIC_ENCOUNTER_INTERVAL_MIN}} minutes** in between (no durability cost,
no stage movement, no XP - see [Combat](/wiki/combat#basic-vs-boss) for how
much simpler these are mechanically). A **Rampage** - {{RAMPAGE_ENCOUNTER_COUNT}}
back-to-back boss fights, everyone instantly revived between each - can be
triggered by a mod instantly, or by a chat vote ({{RAMPAGE_VOTE_THRESHOLD}}
distinct voters). The streamer can also flip on **Permanent Rampage**, which
replaces the normal timer with continuous back-to-back boss fights until
turned back off - if fights seem to be firing constantly rather than every
{{ENCOUNTER_INTERVAL_MIN}} minutes, that toggle is probably on.

<h3 id="roster">The Roster</h3>

This isn't a party-invite system - every joined character is permanently
part of one shared roster, and every encounter battles with *everyone*
currently eligible (joined, not downed, not retreated) at once. There's no
way to opt just yourself into a specific fight. Rewards (loot, dust, sand,
tokens) are rolled and handed out independently per participant, not pooled
and split.

<h3 id="channel-points">Channel-Point Redemptions</h3>

<div class="wiki-table-wrap">

| Reward | What it does |
|---|---|
| Reforge Gear | Reforges one random equipped item into a fresh, higher tier. Needs at least one item equipped; limited to once per clock hour per redeemer. |
| Repair All Gear | Fully repairs every piece of gear (equipped and bagged), free, and clears retreat status immediately if you were retreated. |
| Force Boss Fight | Triggers the next boss fight right away instead of waiting for the timer. Shared budget of {{FORCE_BOSS_MAX_PER_CYCLE}} uses per encounter cycle, not per person. |

</div>

<p class="muted">Point costs for these are streamer-configurable and can change - check the actual redemption in the channel-points panel for the current price. Don't confuse "Reforge Gear" (the channel-points reward above) with the dashboard's own on-demand Reforge button, which is dust-costed - see <a href="/wiki/crafting#reforge">Crafting</a>.</p>

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
