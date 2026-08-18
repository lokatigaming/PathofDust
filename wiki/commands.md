<div class="card wiki-wide">

## Commands

Every command below starts with `!`. Anything marked **Mods** needs a moderator
or the broadcaster; everything else works for any viewer in chat. There's no
subscriber-only tier anywhere in this bot - it's always one of "everyone,"
"mods," or (for one command) "broadcaster only."

Most commands have no cooldown at all. A specific subset shares one **global**
5-second cooldown per command name - "global" means if one viewer runs it,
*everyone* is briefly blocked from that same command, not just that viewer.
That subset is called out per-row below as "Shared {{BUILTIN_COOLDOWN_S}}s."

<h3 id="adventure">Adventure</h3>

The chat RPG the rest of this wiki covers. See [Getting Started](/wiki/getting-started) for what these actually do mechanically.

<div class="wiki-table-wrap">

| Command | Syntax | Who | What it does |
|---|---|---|---|
| `!join` | `!join` | everyone | Creates your character the first time; rejoins you if retreated. |
| `!character` / `!char` / `!me` | `!character` | everyone | Your level, archetype, XP, win/loss record, dust, and a link to your dashboard. |
| `!party` / `!adventure` | `!party` | everyone | Current world stage plus how many joined heroes are active right now. |
| `!rampage` | `!rampage` | everyone (mods trigger instantly; everyone else casts a vote) | Non-mods: votes for a rampage - {{RAMPAGE_VOTE_THRESHOLD}} distinct voters triggers it. Mods: starts it immediately. |
| `!nextencounter [boss]` | `!nextencounter [lich\|firedemon\|cthulhu\|dragon\|bahamut\|purple\|cube]` | **Mods** | Runs the next encounter immediately; optionally forces a specific boss. |
| `!clearbattlefield` / `!resetbattlefield` | `!clearbattlefield` | **Mods** | Force-retreats every joined hero back to "needs to !join again." |
| `!giveloot` / `!gearall` | `!giveloot` | **Mods** | Grants every joined hero one random piece of gear. |
| `!giftdust <all\|username> <amount>` | `!giftdust all 500` | **Mods** | Grants dust to everyone joined, or to one named hero. |

<p class="muted">`!nextencounter`'s boss names also quietly accept a few unlisted synonyms (`demon`, `fire`, `gelatinouscube`) - the ones above are the ones actually meant to be used.</p>

</div>

<h3 id="general">General</h3>

<div class="wiki-table-wrap">

| Command | What it does | Cooldown |
|---|---|---|
| `!hug [@user]` | Sends a hug, to someone specific or to chat generally. | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!uptime` | How long the stream's been live. | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!time` | The streamer's local time. | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!commands` | Link to the full public commands list. | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!theme` / `!themes` | Link to the entrance-theme listing. | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!handsome` | Plays a short video on stream. | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!thatsagoodbuild` | Plays a short video on stream. | Shared {{BUILTIN_COOLDOWN_S}}s |

<p class="muted">Yes, <code>!handsome</code> and <code>!thatsagoodbuild</code> really are open to everyone, no mod gate - that's on purpose.</p>

</div>

<h3 id="bugs">Bug Reports</h3>

<div class="wiki-table-wrap">

| Command | Syntax | Who | What it does | Cooldown |
|---|---|---|---|---|
| `!bugreport <what happened>` | `!bugreport the dragon fight froze` | everyone | Files a free-text bug report for the streamer. | {{BUGREPORT_COOLDOWN_S}}s per user |
| `!bugreports` | `!bugreports` | **Mods** | Lists the 5 most recent reports in chat. | none |

</div>

<h3 id="prices">Price Lookups</h3>

Live Path of Exile market data pulled from poe.ninja - unrelated to this game's own dust/sand economy.

<div class="wiki-table-wrap">

| Command | What it does | Who | Cooldown |
|---|---|---|---|
| `!essenceprofit` / `!ep` | Live Deafening Essence price + estimated farming profit/hr. | everyone | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!ritualprofit` / `!rp` | Live Ritual-currency farming profit estimate. | everyone | Shared {{BUILTIN_COOLDOWN_S}}s |
| `!vesselprice` / `!vp` | Link to Blood-filled Vessel pricing. | **Mods** | none |
| `!price <ritual\|essence\|vessel>` | Shortcut to the three above. | everyone (`vessels` sub-form is mod-only) | Shared {{BUILTIN_COOLDOWN_S}}s |

</div>

<h3 id="music">Song Requests &amp; Music Queue</h3>

<div class="wiki-table-wrap">

| Command | Syntax | Who | What it does |
|---|---|---|---|
| `!songrequest` / `!sr` | `!sr <link or search>` | everyone | Queues a song (or plays it immediately if nothing's playing); also saves it to your personal playlist. |
| `!queue` | `!queue` | everyone | Next 5 queued songs and the last 5 played. |
| `!nowplaying` / `!np` | `!nowplaying` | everyone | What's playing and who requested it. |
| `!song` / `!currentsong` | `!song` | everyone | Same as `!nowplaying`, plus a direct link. |
| `!voteskip` / `!vs` | `!voteskip` | everyone | Votes to skip the current song (3 distinct voters by default triggers it; auto-passes if it's your own song). |
| `!votepause` | `!votepause` | everyone | Votes to pause. |
| `!votestart` | `!votestart` | everyone | Votes to resume a paused song. |
| `!votevolume` / `!vv <{{MIN_VOTE_VOLUME}}-{{MAX_VOTE_VOLUME}}>` | `!vv 60` | everyone | Votes for a volume level within the allowed range. |
| `!skip` / `!modskip` | `!skip` | **Mods** | Immediate skip, no vote. |
| `!modpause` | `!modpause` | **Mods** | Immediate pause, no vote. |
| `!modstart` / `!modresume` | `!modstart` | **Mods** | Immediate resume, no vote. |
| `!modvolume` / `!modvv <0-100>` | `!modvv 80` | **Mods** | Immediate volume set, full range. |
| `!forceplay` | `!forceplay` | **Broadcaster only** | Locks out `!voteskip` so the current song plays through uninterrupted. |
| `!clearqueue` / `!modclear` | `!clearqueue` | **Mods** | Clears the pending queue (not the currently playing song). |
| `!songinsert` / `!si` | `!si <link or search>` | **Mods** | Plays a song immediately, interrupting the current one, then resumes it after. |
| `!playrandom <1-5>` | `!playrandom 3` | everyone | Queues that many songs from recently-played genres. |
| `!playrandom on\|off` | `!playrandom on` | **Mods** | Toggles continuous auto-topup from those genres. |

<p class="muted"><code>!voteskip</code> shares a {{VOTESKIP_COOLDOWN_S}}-second (10 minute) per-user cooldown with the channel-points "Interrupt the Music" reward. Every other command on this list has no cooldown of its own.</p>

</div>

<h3 id="themes">Entrance Themes</h3>

<div class="wiki-table-wrap">

| Command | Syntax | Who | What it does |
|---|---|---|---|
| `!settheme <username> <link or search>` | `!settheme viewer123 never gonna give you up` | **Mods** | Sets a chatter's entrance theme (plays once a day on their first message) and plays it immediately. |
| `!resetgreeted [username]` | `!resetgreeted` | **Mods** | Clears "already greeted today" for one user, or everyone, so themes can retrigger. |

</div>

<h3 id="playlists">Personal Playlists</h3>

<div class="wiki-table-wrap">

| Command | Syntax | What it does |
|---|---|---|
| `!playlist` | `!playlist` | Usage summary + link to the full playlist listing. |
| `!playlist <username>` | `!playlist viewer123` | Queues up to 5 random songs from that person's saved playlist. |
| `!playlist <username> play <#>` | `!playlist viewer123 play 2` | Queues one specific saved song by its position. |
| `!playlist add <link or search>` | `!playlist add some song` | Saves a song to *your own* playlist (rejects anything over 10 minutes). |
| `!playlist remove <position or title>` | `!playlist remove 3` | Removes a song from your own saved playlist. |
| `!playlist clear [username]` | `!playlist clear` | Clears your own playlist; clearing someone else's needs a mod. |

</div>

<h3 id="mod-tools">Streamer/Mod Tools</h3>

<div class="wiki-table-wrap">

| Command | What it does |
|---|---|
| `!command add\|edit\|delete\|remove !name [reply]` | Manages the custom text-command list below. |
| `!announcetest` | Replays every configured periodic announcement immediately. |
| `!checkpatreon` | Forces an immediate Patreon supporter check. |
| `!alerttest [follow\|subscription\|subscriptionGift\|cheer\|raid\|tip]` | Fires a fake alert-box event for testing overlays. |
| `!replaytips` | Re-broadcasts the last 5 recorded tips as alert events. |

</div>

<h3 id="custom">Custom Commands</h3>

Beyond everything above, the streamer can create arbitrary text commands on the
fly with `!command add !name <reply text>` - these aren't fixed in the game's
code, so this wiki can't list them by name. The reply text can include the
caller's name and random-number ranges, and each one gets its own cooldown and
optional mod-only gate. See the live list linked from `!commands` in chat for
whatever currently exists.

</div>
