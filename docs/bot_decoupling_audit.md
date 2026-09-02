# Bot ↔ Game coupling audit

**Date:** 2026-08-27
**Branch:** `docs/bot-decoupling-audit` (read-only audit — no behaviour changed)
**Scope:** every existing link between `twitch-bot-rs.exe` (root package
`twitch-bot-rs`, `src/**`) and `game.exe` (workspace member `game`,
`game/src/**`), as the two run in production out of `C:\PathofDust`.

This document reports what exists. It proposes no design and recommends
no change.

---

> **SUPERSEDED IN PART — 2026-09-02 (`chore/bot-decoupling`).** The bot no
> longer has an adventure integration of any kind. `AdventureApiClient`,
> `src/published_constants.rs`, the ten adventure chat commands, the three
> adventure channel-point redemptions, chat activity XP and the SSE
> announcements relay are all deleted from `src/**`, and
> `ADVENTURE_API_SECRET`, `ADVENTURE_API_BASE_URL`,
> `CHANNEL_POINTS_REFORGE_REWARD_COST`, `CHANNEL_POINTS_REPAIR_REWARD_COST`
> and `CHANNEL_POINTS_FORCE_BOSS_REWARD_COST` no longer exist in
> `src/config.rs`. The game side went first: `game/src/adventure_web/api.rs`
> and the whole `/api/*` router are gone. Every statement below that
> describes the seam, those env keys or those commands as live describes
> history, not the current tree. The bot itself is unaffected and still
> runs: Twitch chat, song requests, alerts, entrance themes, the two
> surviving channel-point rewards, PoE utilities and OBS control.

## 0. Starting facts

The **build-time** coupling is already gone. `src/lib.rs:1-16` records the
2026-08-22 decoupling: the bot crate no longer has a path dependency on
the `game` crate, the five `pub use game::...` re-exports were deleted,
and the twelve cross-process integration tests moved to `game/tests`.
`Cargo.toml:1-2` (`members = ["game"]`) and `game/Cargo.toml` confirm it —
the bot's dependency list contains no `game` entry.

What remains is **runtime** coupling, and it is almost entirely one
mechanism: HTTP on port 4005 behind one shared secret.

### Port map (measured, not assumed)

| Port | Bound by | Purpose | Other process depends on it? |
|---|---|---|---|
| 4001 | **bot** — `src/config.rs:258`, `src/alerts.rs` | OBS "Alert Box" browser source (SSE). Unconditional, binds early. | **No.** Nothing in `game/**` references it. |
| 4002 | **bot** — `src/config.rs:267`, `src/song_overlay_server.rs` | Song-request overlay + OBS control dock (HTTP + WS). **Conditional** — only starts when `YOUTUBE_API_KEYS` is non-empty (`src/main.rs:555`). | **No.** Only `public/alert-box.html:221-223` (the bot's own page) references it. |
| 4003 | **bot** — `src/config.rs:275`, `src/chat_overlay_server.rs` | Transparent Twitch chat overlay (HTTP + WS), emote-tokenised server-side. Binds **last**, after `emotes::fetch_all`. | **No.** |
| 4004 | **game** — `game/src/main.rs:100`, `game/src/adventure_overlay_server.rs` | Adventure OBS overlay. Not publicly exposed. | Bot declares the env key but never uses it (see L24). |
| 4005 | **game** — `game/src/main.rs:98`, `game/src/adventure_web.rs` | Public dashboard + wiki + `/ws` **and** the `/api/*` bot seam. Fronted by cloudflared. | **Yes — this is the only port the bot uses.** |

**Answer to "which of 4001/4002/4003 does the game depend on": none of
them.** All three are bot-owned, OBS-facing, and invisible to `game.exe`.
The only shared port surface in either direction is 4005.

---

## 1. Bot → game: the `/api/*` HTTP seam

**Direction:** bot → game (except L16, which reverses).
**Mechanism:** HTTP/1.1 to `ADVENTURE_API_BASE_URL` (default
`http://127.0.0.1:4005`, `src/config.rs:279`), every request carrying the
header `x-adventure-api-secret`.

- Bot end: `src/adventure_client.rs` (`AdventureApiClient`), constructed
  once at `src/main.rs:746`.
- Game end: `game/src/adventure_web/api.rs`, router at `:61-84`, nested
  onto the *same* Axum server as the public dashboard. Auth middleware:
  `game/src/adventure_web/api.rs:87-101`.
- **The whole router is optional.** `router()` returns `None` when
  `ADVENTURE_API_SECRET` is unset (`api.rs:61-62`, `game/src/main.rs:105`),
  in which case the game serves exactly the route table it had before the
  seam existed.
- Uniform response envelope: `{"reply": <string|null>}` — an
  already-formatted chat line. Design principle stated at `api.rs:12-19`:
  **the game owns all player-facing text**; the bot never re-formats.
- Failure policy is bot-side and uniform: `src/commands.rs:440-451` — any
  transport or non-2xx error yields
  `"The adventure is restarting — try again in a moment!"`.

> **Stale comments, flagged.** `src/adventure_client.rs:2-9` and
> `game/src/adventure_web/api.rs:21-23` both claim nothing calls through
> the seam yet and that it is test-only. That is no longer true — the
> Stage 4 cutover happened (`src/main.rs:735-746`). Every route below is
> live in production dispatch. Reported, not fixed.

### 1a. Chat-command routes (10)

| # | Route | Bot call site | Game handler | Data → game | Data ← game | Player-facing feature | If cut with no replacement |
|---|---|---|---|---|---|---|---|
| L1 | `POST /api/commands/join` | `src/commands.rs:1070` → `adventure_client.rs:64` | `api.rs:121` | `{user}` | reply string | `!join` | Nobody can enrol from chat. The web dashboard's own `do_join` (`game/src/adventure_web.rs:692-697`) still works, so joining survives — but only for people who log into the site. |
| L2 | `GET /api/commands/character` | `src/commands.rs:1072` → `:68` | `api.rs:141` | `?user=` | reply string | `!character` / `!char` / `!me` | No character summary in chat. Equivalent data still on the dashboard. |
| L3 | `GET /api/commands/party` | `src/commands.rs:1074` → `:72` | `api.rs:159` | — | reply string | `!party` / `!adventure` | No party roster in chat. |
| L4 | `POST /api/commands/next_encounter` | `src/commands.rs:1088` → `:76` | `api.rs:171` | `{forced: Option<String>}` (boss name) | reply string, often `null` | `!nextencounter [boss]` (mod-only) | Mods lose the ability to force or advance an encounter on demand. The automatic `spawn_encounter_loop` (`game/src/main.rs:127`) keeps running, so fights still happen on their own timer. |
| L5 | `POST /api/commands/event_intro` | `src/commands.rs:1425` → `:80` | `api.rs:188` | `{args: [String]}` | reply string incl. the boss-intro line + wiki link (`api.rs:206`) | `!event intro <boss>` (mod-only) | No "NEW BOSS ALERT" intro can be posted to chat. |
| L6 | `POST /api/commands/rampage` | `src/commands.rs:1106` → `:84` | `api.rs:219` | `{user, is_mod_or_broadcaster}` | reply string | `!rampage` — mod trigger *and* the 3-vote player trigger | Rampages can no longer be triggered or voted for. `spawn_rampage_loop` still runs its own schedule. **Note the role field:** permission gating deliberately stays bot-side (`api.rs:15-19`) because only the bot knows Twitch roles. |
| L7 | `POST /api/commands/clear_battlefield` | `src/commands.rs:1113` → `:88` | `api.rs:230` | `{}` | reply string | `!clearbattlefield` / `!resetbattlefield` (mod-only) | Mod recovery tool gone; a stuck battlefield needs a dashboard/admin route or a restart. |
| L8 | `POST /api/commands/give_loot` | `src/commands.rs:1120` → `:92` | `api.rs:239` | `{}` | reply string | `!giveloot` / `!gearall` (mod-only) | Mods cannot mass-gear the party. |
| L9 | `POST /api/commands/gift_dust` | `src/commands.rs:1139` → `:96` | `api.rs:258` | `{target, amount}` | reply string | `!giftdust <all\|username> <amount>` (mod-only) | Mods cannot grant dust. The direct economy grant is lost. |
| L10 | `POST /api/commands/pin_fight` | `src/commands.rs:1405` → `:100` | `api.rs:276` → `pin_most_recent_fight` (`game/src/adventure/fight_storage.rs:261-331`) | `{}` | reply string naming the pinned fight id | `!pinfight` (mod-only) | Bug-report evidence can no longer be pinned from chat before the 3–5 file rolling window prunes it. The pinned directory and the function are entirely game-side; only the trigger is lost. |

### 1b. Channel-point redemption routes (3)

Distinct envelope: `{"fulfilled": bool, "chat_message": Option<String>}`
(`src/adventure_client.rs:36-42`, mirrored game-side). The
FULFILLED/CANCELED status update back to Twitch stays bot-side — it needs
Helix, which `game.exe` has no token for.

| # | Route | Bot call site | Game handler | Data → game | Player-facing feature | If cut |
|---|---|---|---|---|---|---|
| L11 | `POST /api/redemptions/reforge` | `src/main.rs:357-368` (`handle_reforge_redemption`), decision fn `:340` | `api.rs:305` | `{user_name}` | "Reforge Gear" channel-point reward; reward created by `src/channel_points.rs:101` / `src/main.rs:796`. Always chat-announced, by explicit request. | The reward becomes inert. On error the bot already CANCELs (refunds) silently — with the link cut permanently, every redemption refunds and the reward should be retired. |
| L12 | `POST /api/redemptions/repair` | `src/main.rs:387-391` | `api.rs:332` | `{user_name}` | "Repair Gear" reward (`channel_points.rs:105`, `main.rs:802`). Silent in chat. | Same: inert reward, permanent silent refund. |
| L13 | `POST /api/redemptions/force_boss` | `src/main.rs:407-420` | `api.rs:349` | `{user_name, announce}` | "Force Boss" reward (`channel_points.rs:109`, `main.rs:811`); shared cycle-wide budget game-side. | Same. Also removes the only player-purchasable way to force an encounter. |

Bot-side machinery that exists **only** to serve L11–L13:
`reconcile_missed_redemptions` (`src/main.rs:445-490`), which replays
redemptions missed while the bot was down, and the EventSub redemption
dispatch at `src/main.rs:920-928`.

### 1c. Background routes (2)

| # | Route | Bot call site | Game handler | Data | Feature | If cut |
|---|---|---|---|---|---|---|
| L14 | `POST /api/activity_xp` | `src/main.rs:1108-1116` — **fire-and-forget**, `tokio::spawn`ed so a slow game can never stall the sequential chat loop | `api.rs:374` | `{username}`, one call per chat message | Passive XP for chatting. The level-up announcement comes back over L16, not from here (`api.rs:370-372`). | **The passive-XP economy stops entirely.** This is the highest-volume link, and the game has no other source of "this person is active in chat". Characters would gain XP from fights alone. |
| L15 | `POST /api/published-constants` | `src/published_constants.rs:51-76`, called once at startup (`src/main.rs:754`); 3 attempts, 1s backoff, never fails startup | `api.rs:393-401` → `state::save_json(PUBLISHED_CONSTANTS_PATH)` | 5 integers: `builtin_cooldown_secs`, `bug_report_cooldown_secs`, `song_skip_cooldown_secs`, `min_vote_volume`, `max_vote_volume` | The wiki's chat-cooldown / vote-volume placeholders (`game/src/adventure_web/wiki.rs:217-222`) | The wiki renders `"varies"` for those five values, forever. Documented, already-graceful fallback (`game/src/main.rs:112-119` logs the same warning at startup). **Cosmetic.** |

---

## 2. Game → bot: the announcements stream

| # | Link | Direction | Mechanism |
|---|---|---|---|
| **L16** | `GET /api/announcements/stream` | **game → bot** (data), bot-initiated connection | Server-Sent Events on port 4005, same shared secret |

- Game end: `game/src/adventure_web/api.rs:403-411` — an SSE wrapper over
  `AdventureManager::subscribe_announcements()`, a `tokio::broadcast`
  channel. A lagged reader skips missed messages rather than dropping the
  stream.
- Bot end: hand-rolled SSE parser at `src/adventure_client.rs:162-188` (no
  SSE crate); relay loop at `src/main.rs:1032-1051`, which passes each
  frame **verbatim** to `chat_client.say()` with no re-formatting, and
  reconnects every 5s on any error.
- **What crosses:** every already-formatted player-facing announcement the
  game produces — encounter results, gear crits, rampage completions,
  unique-shard wins, activity-XP level-ups, the Wings-of-Flight giveaway
  line (`game/src/main.rs:136-155`), boss alerts. `src/main.rs:1015-1031`
  records that four separate in-process broadcast subscribers collapsed
  into this one loop at the Stage 4 cutover.
- **Player-facing dependency:** the entire narration of the game in Twitch
  chat.
- **If cut with no replacement:** the game keeps fighting, keeps
  persisting, keeps updating the overlay and the dashboard — and says
  **nothing** in chat, ever. Fights resolve invisibly to anyone not
  watching the OBS overlay or the website. This is the single most
  load-bearing link in the inventory.

---

## 3. Shared resources

| # | Link | Direction | Mechanism | Both ends | Consequence of cutting |
|---|---|---|---|---|---|
| **L17** | `ADVENTURE_API_SECRET` | shared secret | Env var read independently by both processes; must match byte-for-byte | Bot: `src/config.rs:238-239` (**hard-required** — startup fails without it), `:280`. Game: `game/src/main.rs:105`, `:166` (**optional** — `None` un-mounts `/api/*`). Header name declared twice: `src/adventure_client.rs:21`, `game/src/adventure_web/api.rs:43`. | Cutting the secret cuts L1–L16 in one stroke, game-side, with no code change: unset it and the router is never mounted. Bot-side it is the opposite — the bot **refuses to start** without it. |
| **L18** | Shared `.env` at the deployment root | shared config | Both call `dotenvy::dotenv()` from the same working directory. `game/src/main.rs:9-11` states it explicitly: "Reads the SAME `.env` file the bot does". | Shared keys: `TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET` (`game/src/main.rs:95-97`; `game/src/adventure_web.rs:16` — "Reuses the bot's own TWITCH_CLIENT_ID/SECRET"), `ADVENTURE_API_SECRET`, and the port keys. | No functional break — the game only needs the client id/secret pair to keep existing. But the Twitch **application** is shared: both processes authenticate as the same registered app. Separating them means registering a second Twitch app or accepting the shared one. Note that `.env.example` documents 4001/4002/4003 (`:37,74,95`) but **not** `ADVENTURE_API_SECRET`, `ADVENTURE_API_BASE_URL`, `ADVENTURE_WEB_PORT` or `ADVENTURE_OVERLAY_SERVER_PORT` — a gap; reported, not fixed. |
| **L19** | `bot-published-constants.json` | shared file (historically bot→game) | CWD-relative path, deliberately **not** under `data_path` | Const: `game/src/adventure/published_constants.rs:37`. Written: `game/src/adventure_web/api.rs:394` (the game writes it now, on the bot's behalf). Read: `game/src/adventure_web/wiki.rs:222`, `game/src/main.rs:114`. Backed up: `backup-game-data.ps1:112`. | This **was** the last bot→game file write; it became L15 in the 2026-08-22 decoupling (`src/published_constants.rs:1-19`). Today no bot code touches the path — the file is game-written and game-read. Cutting L15 leaves it stale or absent, which is handled. The on-disk format is deliberately unchanged so a pre-decoupling bot still interoperates. |
| **L20** | Shared deployment root `C:\PathofDust` | shared resource | Same working directory, same `target\release\` (both binaries), same `logs\` directory (`bot.log` at `src/main.rs:520-521`, `game.log` at `game/src/main.rs:69-71` — distinct files, same directory), same git checkout, same `Cargo.lock` and workspace. | Both `main.rs` files; `backup-game-data.ps1:45-53`. | Not a functional link, but it is why a single `git push origin master` and a single build tree serve both. `GAME_DATA_DIR` (`game/src/main.rs:86-92`) can redirect the game's persistence elsewhere, but is documented as test-only. |
| **L21** | Game data files | **game-exclusive** | — | `adventure-characters.json`, `adventure-world.json`, `adventure-reforge-cooldown.json`, `adventure-sessions.json`, `adventure-live-tunables.toml`, `adventure-passive-overrides.toml`, `adventure-item-balance.toml`, the fight archives, every one-time marker — full manifest at `backup-game-data.ps1:100-130`. | Verified: **the bot reads and writes none of them.** Its own persisted files are disjoint — `tokens.json`, `commands.json`, `song-queue.json`, `entrance-themes.json`, `bugreports.json`, `patreon-*`, `channel-points-*-reward.json`, `personal-playlists.json`, `playrandom-state.json` (`src/main.rs:539-974`). | **No link to cut.** This is the cleanest boundary in the system. |

---

## 4. Startup order, liveness, and ops

| # | Link | Direction | Mechanism | Location | Consequence of cutting |
|---|---|---|---|---|---|
| **L22** | **Startup-order dependency: game first, always** | bot depends on game | Deploy procedure, enforced by humans | `REFACTOR_PLAN.md` §13 step 4: *"start `TwitchBotRS` only after `GameProcess` is confirmed healthy … Game always comes up and is verified healthy before the bot starts — **never the other order, whether or not the bot is moving this release**."* | The dependency is **soft in code and hard in procedure**. Code-wise the bot tolerates a down game at every touch point: L1–L13 fall back to the down-reply, L14 is fire-and-forget, L15 retries 3× then gives up, L16 reconnects on a 5s loop forever. Nothing in the bot blocks or crashes on a missing game. The ordering exists to avoid a cold-start window of down-replies and a missed constants publish, not to prevent a failure. |
| **L23** | Watchdogs | independent; coupled only by procedure | Two scheduled tasks, two scripts, two flags | `watchdog.ps1` (task `TwitchBotRS`, **port 4001**, `:129`/`:133`, 45s startup grace) and `game-watchdog.ps1` (task `GameProcess`, **port 4005**, `:115`/`:117`, 90s grace). Flags: `maintenance-flag.ps1:145-146` — `bot-watchdog-maintenance.flag` vs `game-watchdog-maintenance.flag`, selected by `-Target Bot\|Game`. | **No runtime coupling.** Neither watchdog reads the other's port, task, flag or log; neither terminates anything. The couplings that exist are documentary: `game-watchdog.ps1:111-113` justifies port 4005 partly as *"the one the bot's `ADVENTURE_API_BASE_URL` points at"*; `watchdog.ps1:31-33` warns that 4004/4005 are still declared in the bot's config but bound by the game; and both files carry a **"DELIBERATELY DUPLICATED … KEEP IN SYNC"** note (`watchdog.ps1:118-122`) about their four copied helper functions. The separate-flags design (`maintenance-flag.ps1:39-40`, `REFACTOR_PLAN.md` §13) exists precisely so a game-only deploy never un-protects the bot. |
| **L24** | Vestigial bot config | bot → (nothing) | Dead env reads | `src/config.rs:83,88,100` and `:276,277,278` declare `adventure_overlay_server_port`, `adventure_web_port`, `adventure_web_public_url`. Grep confirms **zero** other uses anywhere in `src/**`. | Nothing. Dead weight from the in-process era. Deleting the three fields is a no-op. |
| **L25** | Cross-process test harness | test-only | Tests spawn a real `game` binary and speak the bot's exact wire protocol | `game/tests/api_seam.rs:38`, `game/tests/killed_process_smoke.rs:47,78,95,107`, `game/tests/published_constants_http.rs:63,86` — all set or send `x-adventure-api-secret`. | These live in `game/tests`, so they do not couple the *builds* (`game/Cargo.toml` dev-dependencies only). They do couple the *contract*: they are the only automated proof that `src/adventure_client.rs` and `api.rs` still agree. Cutting the seam orphans them. |

**Total: 25 coupling points** — 16 HTTP routes (15 bot→game, 1 game→bot),
5 shared resources, 4 ops / procedural / vestigial.

---

## If every link is cut

### 1. What player-facing capability the game loses

Everything that happens in Twitch chat. Concretely:

- **All chat narration of the game.** Encounter results, gear crits,
  rampage completions, unique-shard wins, level-ups, boss-intro alerts and
  giveaway announcements are produced by the game and delivered by the bot
  (L16). Cut it and the game becomes silent in chat. Fights still run,
  still persist, still render on the 4004 overlay and the 4005 dashboard —
  but a viewer reading chat sees nothing.
- **All 13 chat commands and all 3 channel-point redemptions** (L1–L13):
  `!join`, `!character` / `!char` / `!me`, `!party` / `!adventure`,
  `!nextencounter`, `!event intro`, `!rampage`, `!clearbattlefield` /
  `!resetbattlefield`, `!giveloot` / `!gearall`, `!giftdust`, `!pinfight`,
  plus the Reforge Gear, Repair Gear and Force Boss rewards.
- **Passive chat XP** (L14). One POST per chat message is the game's only
  signal that a person is active in Twitch chat. Without it, characters
  gain XP from fights alone and the whole "reward people for being in
  chat" mechanic disappears.
- **Mod control from chat.** `!nextencounter`, `!rampage`,
  `!clearbattlefield`, `!giveloot`, `!giftdust` and `!pinfight` are the
  live-ops toolkit. The automated loops (`spawn_encounter_loop`,
  `spawn_basic_encounter_loop`, `spawn_rampage_loop`) keep the world
  turning on their own timers, but nobody can intervene from chat.
- **The wiki's five published constants** (L15) render `"varies"`.
  Cosmetic, and already the documented fallback.

Not lost: the dashboard, the wiki, the OBS adventure overlay, the fight
engine, all persistence, all admin and tunables pages, and web-based
joining (`game/src/adventure_web.rs:692`). Twitch **login** to the
dashboard also survives untouched — see below.

### 2. What the game would have to acquire to replace it

**Yes — the game would need its own Twitch connection**, and it is roughly
half-built already.

**What the game already has:** a `reqwest` client and a **complete Twitch
OAuth authorization-code flow** — `game/src/adventure_web.rs:640-676`
exchanges a code at `id.twitch.tv/oauth2/token` and identifies the user via
`api.twitch.tv/helix/users`, using `TWITCH_CLIENT_ID` /
`TWITCH_CLIENT_SECRET` from the shared `.env` (L18). It persists browser
sessions to `adventure-sessions.json`. So the game can already talk to
Twitch's REST API and already knows who a Twitch user is.

**What the game does not have, and would have to acquire:**

- **A chat connection.** The bot uses the `twitch-irc` crate with
  `RefreshingLoginCredentials` (`src/twitch/chat.rs:1-15`); `twitch-irc` is
  **not** in `game/Cargo.toml` at all. This is the single largest missing
  piece — it is what makes an announcement appear in chat, and what
  receives `!commands` in the first place.
- **A bot-account token with chat scopes, plus refresh machinery.**
  `src/twitch/auth.rs` (191 lines) owns `tokens.json`, the camelCase token
  shape, and the refresh cycle shared between `twitch-irc` and direct Helix
  calls. The game has no token store — its OAuth flow issues *viewer*
  sessions, not a *bot identity*. The practical consequence: the chat
  identity is an account, not just an app. Either the game inherits
  `tokens.json` (in which case the bot must stop using it, or both refresh
  the same token concurrently), or a second Twitch account is provisioned.
- **An EventSub WebSocket client.** `src/twitch/eventsub.rs` (476 lines) —
  session handling, `subscribe_all`, and the stale-subscription cleanup at
  `:101-140` that exists because Twitch does not delete WebSocket-transport
  subscriptions promptly. This is what delivers channel-point redemptions.
- **A Helix client.** `src/twitch/helix.rs` (215 lines) — including
  `update_redemption_status` (the FULFILLED/CANCELED call that
  `src/adventure_client.rs:36-42` explicitly names as the reason
  redemptions stay bot-side) and reward creation
  (`src/channel_points.rs:93-113`).
- **Role knowledge.** `api.rs:15-19` states the gating rationale plainly:
  only the bot knows Twitch roles, which is why `rampage` takes
  `is_mod_or_broadcaster` as an explicit field (L6) instead of deciding for
  itself. A game with its own IRC connection would read badges directly and
  that parameter would become internal.
- **Command parsing and cooldowns.** The `src/commands.rs` dispatch,
  `BUILTIN_COOLDOWN`, and the reply plumbing. The *bodies* of all ten
  command handlers are already game-side (`api.rs` is a byte-for-byte port
  of them — `api.rs:12-15`); only the parse/route/cooldown layer is missing.

In short: `src/twitch/**` is ~1,020 lines across four files, plus the
`twitch-irc` dependency, plus `channel_points.rs` and the dispatch half of
`commands.rs`. The formatted text, the game logic and the OAuth half all
already live in `game/**`.

### 3. What the bot would have left to do

A substantial, coherent, Twitch-only bot:

- **Song requests** — YouTube search and queue, the 4002 overlay and OBS
  control dock, vote-skip / vote-pause / vote-resume / vote-volume
  (`src/song_requests.rs`, `src/song_overlay_server.rs`,
  `src/personal_playlists.rs`, `src/playrandom.rs`).
- **Alerts** — the 4001 OBS alert box, fed by EventSub follows, subs and
  raids plus StreamElements, PayPal and Patreon tips (`src/alerts.rs`,
  `src/streamelements.rs`, `src/paypal.rs`, `src/patreon.rs`).
- **Chat overlay** — the 4003 transparent overlay with server-side
  Twitch/BTTV/FFZ emote tokenisation (`src/chat_overlay_server.rs`,
  `src/emotes.rs`).
- **Entrance themes** and the theme/interrupt channel-point rewards
  (`src/entrance_themes.rs`, `src/channel_points.rs:93-97`).
- **Static and custom commands, timed announcements, bug reports**
  (the static path in `src/commands.rs`, `src/announcements.rs`,
  `src/bug_reports.rs`).
- **Path of Exile utilities** — `!essence`, `!ritualscarab`, the build feed
  (`src/poe_ninja.rs`, `src/essence_pricing.rs`, `src/vessel_pricing.rs`,
  `src/build_feed.rs`).
- **OBS control** (`src/obs_websocket.rs`).

Only `adventure_client.rs`, `published_constants.rs`, three of the five
`ensure_*_reward` functions, the eleven adventure arms of `handle_builtin`,
three redemption handlers and the SSE relay loop would become dead code.

### 4. Which links are trivial to cut, which are load-bearing

**Trivial** — no replacement needed, or the fallback is already the
documented behaviour:

- **L24** vestigial config — pure dead code; deleting it is a no-op.
- **L15 / L19** published constants — the wiki already renders `"varies"`
  when the file is absent, and the game already logs that at startup.
- **L20 / L21** shared root and data files — the boundary is already clean;
  the bot touches no game data.
- **L23** watchdogs — already fully independent (separate ports, tasks,
  flags, logs). Only the KEEP-IN-SYNC comments and two documentation
  cross-references would need editing.
- **L22** startup order — soft in code. Every bot touch point already
  tolerates a down game. This is a procedural rule in `REFACTOR_PLAN.md`,
  not a mechanism.
- **L7, L8, L9, L10** (`!clearbattlefield`, `!giveloot`, `!giftdust`,
  `!pinfight`) — mod tools, no player-visible loss, and the underlying
  operations remain reachable game-side.
- **L18** shared `.env` — a naming and deployment concern, not a functional
  one.

**Load-bearing** — cutting them removes something with no equivalent
anywhere else:

1. **L16 — `GET /api/announcements/stream`.** The most load-bearing link in
   the system. It is the *only* path from game events to Twitch chat. Cut
   it and the game goes permanently silent in chat.
2. **L14 — `POST /api/activity_xp`.** The only source of chat-activity XP,
   and the only signal the game has that someone is present in chat. Highest
   call volume of any link (one per message).
3. **L1 — `POST /api/commands/join`.** The primary on-ramp. Web join
   survives, so this is load-bearing but not fatal.
4. **L11, L12, L13 — the three redemptions.** Each is backed by a real
   Twitch channel-point reward that players spend points on. Cutting the
   links without retiring the rewards leaves three purchasable items that
   silently refund forever.
5. **L4, L6 — `next_encounter` and `rampage`.** The only live-ops levers
   over pacing, and `rampage` additionally carries a *player-facing* 3-vote
   mechanic, not just a mod trigger.
6. **L17 — `ADVENTURE_API_SECRET`.** Load-bearing in an unusual way: it is
   the master switch. Unsetting it un-mounts the entire `/api/*` router
   game-side with no code change (`api.rs:61-62`) — but the **bot refuses to
   start without it** (`src/config.rs:238-239`), so it cannot simply be
   deleted from `.env` while the current bot binary runs.

**L2, L3, L5** (`!character`, `!party`, `!event intro`) sit in between: real
player-facing losses, but every one of them has a dashboard or wiki
equivalent already serving the same information.
