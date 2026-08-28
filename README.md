# twitch-bot-rs

A Rust Twitch chat bot for [Lokati_Gaming](https://twitch.tv/Lokati_Gaming), built around one big centerpiece feature — a full chat-driven idle RPG — plus the usual bot utilities (alerts, song requests, chat overlay, tip integrations).

This was vibe-coded live on stream at **https://twitch.tv/Lokati_Gaming**.

## What's in here

### The Adventure game

A persistent, chat-driven idle RPG (`src/adventure/`, `src/adventure_web.rs`, `src/passive_tree.rs`) viewers play by typing `!join` and then just... watching. Characters auto-battle bosses on a timer, gain XP/levels, and drop gear.

- 11 classes (Warrior, Berserker, Rogue, Slayer, Ranger, Mage, Monk, Cleric, Druid, Paladin, Warlock), each with its own event-driven combat simulation covering crits, blocks, evasion, damage-over-time, elemental procs, and dozens of class-specific mechanics.
- A large Path of Exile-style passive tree per class, allocated and previewed through a full web dashboard.
- A full item/crafting system: tiered gear, rollable affixes, currency-based crafting (transmute/augment/regal/exalt/chance/etc.), quality, uniques, and a "veil" system for choosing crafting outcomes instead of leaving them to chance.
- Boss fights that scale with party size, level, and stage, with a full combat log players can review afterward.
- A web dashboard (`adventure_web.rs`) for character management, crafting, the passive tree, and viewing other players' characters — plus an OBS-embeddable overlay (`public_adventure_overlay/`) showing live fight status.

### The bot itself

- Chat commands: hand-written plus a `commands.json`-backed system for simple text replies, managed live via `!command add/edit/delete`.
- EventSub alerts (follows, subs, gift subs, cheers, raids) posted to chat and pushed to a self-hosted OBS alert-box overlay.
- Built-in YouTube song requests (`!songrequest`/`!sr`, `!queue`, `!nowplaying`, `!skip`, `!voteskip`, ...) with an OBS browser source that plays the actual video.
- A transparent, draggable Twitch chat overlay for OBS with Twitch/BTTV/FFZ emote rendering.
- Optional StreamElements tip alerts and PayPal tip alerts (via a small Cloudflare Worker relay in `cloudflare-paypal-relay/`, since PayPal can't reach a bot with no public address directly).
- Channel points integrations (boss fights, reforging gear, repairs, entrance themes, and more).

## Prerequisites

- Rust (stable), via [rustup](https://rustup.rs/).
- On Windows: the MSVC linker, via Visual Studio Build Tools with the "Desktop development with C++" workload (`winget install --id Microsoft.VisualStudio.2022.BuildTools`, or the full Visual Studio installer's C++ workload).

## Setup

1. Copy `.env.example` to `.env` and fill in `TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET`, and `TWITCH_CHANNEL` (from a Twitch app at https://dev.twitch.tv/console/apps — set its OAuth redirect URL to `http://localhost:3000/callback`).

2. Get a Twitch token: run `cargo run --bin auth`, which opens your browser for the Twitch login flow and saves `tokens.json`.

3. (Optional) Enable other integrations by setting their `.env` keys — see `.env.example` for `STREAMELEMENTS_JWT` (tip alerts), `YOUTUBE_API_KEYS` (song requests), `PAYPAL_RELAY_URL`/`PAYPAL_RELAY_TOKEN` (PayPal tips, see `cloudflare-paypal-relay/` for the Worker side), and `LASTFM_API_KEY` (`!playrandom`). Leaving a key unset disables that feature gracefully; nothing else breaks.

4. Run the bot:

   ```
   cargo run
   ```

   The Adventure game's web dashboard, chat overlay, song request player, and alert box all start automatically as part of the same process, each on its own local port (see `.env.example` for the port variables).

## OBS setup

Each overlay is a plain HTTP server the bot starts on its own port — add a Browser Source in OBS pointed at the listed URL for each one you want:

| Overlay | Default URL | Notes |
|---|---|---|
| Alert box (follows/subs/cheers/raids/tips) | `http://localhost:4001/alert-box.html` | |
| Song request player | `http://localhost:4002/` | Shows the actual playing video (not transparent). |
| Chat overlay | `http://localhost:4003/` | Transparent background; hover the top-left corner to drag it, position is remembered. |
| Adventure fight overlay | `public_adventure_overlay/overlay.html` | Live combat status for the currently-fighting party. |

## Project structure

- `src/config.rs` — loads `.env`.
- `src/twitch/` — auth (token refresh), chat (`twitch-irc`), EventSub (hand-rolled WebSocket client), Helix API calls.
- `src/adventure/` — the RPG's combat sim, characters, items, crafting, and the manager tying it all together; `src/adventure_web.rs` — its web dashboard; `src/passive_tree.rs` — every class's passive tree definitions.
- `src/commands.rs` — command dispatch: hand-written commands plus the `commands.json`-backed system.
- `src/alerts.rs` — SSE-based alert box server.
- `src/streamelements.rs` — optional integrations.
- `src/announcements.rs` — periodic chat announcements.
- `src/song_requests.rs`, `src/song_overlay_server.rs`, `public_song_overlay/` — YouTube song requests and their OBS browser source.
- `src/emotes.rs`, `src/chat_overlay_server.rs`, `public_chat_overlay/` — the chat overlay OBS browser source.
- `cloudflare-paypal-relay/` — the Cloudflare Worker that relays PayPal webhook tips to the bot (PayPal can't reach a bot with no public address directly).
- `src/bin/auth.rs` — one-time OAuth setup binary.

## Note on this repo

This is the source code only — no live game state, player data, or logs are included (see `.gitignore`). The bot creates its own local JSON state files on first run.
