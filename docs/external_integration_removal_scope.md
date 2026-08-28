# External integration removal — scope

**Date:** 2026-08-27
**Branch:** `docs/twitch-removal-scope` (read-only scoping — no behaviour
changed, nothing deleted)
**Builds on:** `docs/bot_decoupling_audit.md` (branch
`docs/bot-decoupling-audit`, commit `f268dd5`). That document's 25
coupling points, its port map, and its `ADVENTURE_API_SECRET` findings
are taken as established and are **not** re-derived here. Where this
document contradicts it, the contradiction is called out explicitly.

**The decision being scoped:** the game becomes fully standalone. Twitch
goes entirely — chat commands, channel-point redemptions, chat activity
XP, and the dashboard's Twitch OAuth login. Patreon goes with it. The
bot (`twitch-bot-rs`, root package) is not ported or replaced; it stops
being part of this product. Player identity will eventually come from an
external application; it does not exist yet, and the new world starts
with fresh characters.

**Terminology.** "Game crate" = `game/**` (`game.exe`). "Bot crate" =
the root package `twitch-bot-rs` (`src/**`, three binaries). The bot
crate leaves the product wholesale; Part 1 enumerates it only where it
holds something that must be *retired*, not merely dropped.

---

## Correction to the companion audit, stated up front

The audit's summary line says "**All 13 chat commands**". Neither 13 nor
any near value matches the code. The real counts, from
`src/commands.rs:1070-1145` and the builtin registry at
`src/commands.rs:165-175`:

| Measure | Count | Evidence |
|---|---|---|
| `/api/commands/*` endpoints | **10** | `game/src/adventure_web/api.rs:64-73` |
| Distinct chat trigger words routed to the adventure | **15** | `join, character, char, me, party, adventure, nextencounter, event, rampage, clearbattlefield, resetbattlefield, giveloot, gearall, giftdust, pinfight` |
| Documented in the `!commands` registry | **9** | `src/commands.rs:165-175` — `clearbattlefield`/`resetbattlefield` are undocumented |

"13" most likely came from misreading the audit's own link numbering
`L1–L13`, which is 10 command routes **plus** 3 redemption routes. This
document uses 10 endpoints / 15 trigger words throughout. Nothing else in
the audit failed verification.

---

# PART 1 — What gets deleted

**58 deletion targets**, grouped by owning crate. "Safe in isolation"
means the deletion compiles and ships with nothing else changed.

## 1A. Game crate — the `/api/*` seam (bot-facing)

| # | Target | Location | Safe in isolation? |
|---|---|---|---|
| D1 | `mod api;` declaration | `game/src/adventure_web.rs:51` | With D2–D4, yes |
| D2 | The whole `api` module (412 lines) | `game/src/adventure_web/api.rs` (entire file) | Yes — nothing in `game/**` calls into it except the mount |
| D3 | Router construction + `/api` nest | `game/src/adventure_web.rs:142`, `:155-157` | Yes |
| D4 | `api_secret` parameter on `start_adventure_web_server` | `game/src/adventure_web.rs:135-139` | **Dependent** — `game/src/main.rs:166` passes it |
| D5 | `ADVENTURE_API_SECRET` env read | `game/src/main.rs:105` | Yes once D4 lands |
| D6 | `API_SECRET_HEADER` const + `require_shared_secret` middleware | `api.rs:43`, `:87-101` | Falls with D2 |
| D7 | `ApiState` struct | `api.rs:45-49` | Falls with D2 |
| D8 | `ReplyResponse` / `reply()` envelope + all request body structs (`UserBody`, `NextEncounterBody`, `EventIntroBody`, `RampageBody`, `GiftDustBody`, `ActivityXpBody`, the redemption bodies) | `api.rs:104-119` and inline throughout | Falls with D2 |

**`ADVENTURE_API_SECRET` note.** Game-side this is already a soft switch:
`api.rs:61-62` returns `None` and un-mounts the entire router when the
env var is unset, so the *behavioural* removal is a `.env` edit, not a
code change. The constraint the audit records still holds — the current
bot binary **hard-fails at startup** without the secret
(`src/config.rs:238-239`) — so the env var cannot be pulled while a live
bot process still runs. That ordering is the whole reason Stage 1 in
Part 4 exists.

## 1B. Game crate — Twitch OAuth and Twitch identity

| # | Target | Location | Safe in isolation? |
|---|---|---|---|
| D9 | `login` handler (Twitch authorize redirect) | `game/src/adventure_web.rs:556-579` | **No** — route `:161`, and `/login` is linked from `top_nav` |
| D10 | `callback` handler | `:604-621` | No — route `:162` |
| D11 | `handle_callback` (token exchange + Helix `GET /users`) | `:622-678` | Falls with D10 |
| D12 | `CallbackParams`, `TokenResponse`, `HelixUser`, `HelixUsersResponse` | `:581-602` | Falls with D10 |
| D13 | `redirect_uri()` helper | `:104` | Falls with D9/D11 |
| D14 | `OAUTH_STATE_TTL` const | `:59` | Falls with D9 |
| D15 | `AppState.oauth_states` field + init | `:81`, `:150` | Falls with D9 |
| D16 | `AppState.client_id` / `client_secret` | `:75-76`, `:145-146` | Falls with D9–D11 |
| D17 | `AppState.http` (`reqwest::Client`) | `:82`, `:151` | **This is the only `reqwest` use in `game/src/**`** |
| D18 | `client_id` / `client_secret` params on `start_adventure_web_server` | `:131-132` | Dependent — `game/src/main.rs:161-162` |
| D19 | `TWITCH_CLIENT_ID` / `TWITCH_CLIENT_SECRET` env reads — **currently hard-fail startup** | `game/src/main.rs:95-97` | Yes once D18 lands. Deleting these is what makes the game start with no Twitch config at all |
| D20 | Module doc describing Twitch login as the identity model | `game/src/adventure_web.rs:1-21` | Rewrite, not delete |

## 1C. Game crate — the Twitch chat embed on `/overlay`

| # | Target | Location | Safe in isolation? |
|---|---|---|---|
| D21 | `TWITCH_CHANNEL` const | `game/src/adventure_web.rs:1887` | Yes |
| D22 | Twitch chat iframe injection block (gated to logged-in sessions, `CHAT_WIDTH_PX` layout) | `:2139-` in `overlay_page` | Yes — pure presentation |

This is a Twitch dependency the companion audit does not list, because it
is not a bot coupling: the dashboard's `/overlay` page injects Twitch's
own chat embed client-side. It dies with Twitch regardless of the bot.

## 1D. Game crate — gameplay that only the seam fed

| # | Target | Location | Safe in isolation? |
|---|---|---|---|
| D23 | `grant_activity_xp` | `game/src/adventure/manager.rs:4397-4425` | Yes once D2 lands (sole caller was `api.rs:375`) |
| D24 | `ACTIVITY_XP_COOLDOWN` (180s), `ACTIVITY_XP_AMOUNT` (4) | `manager.rs:11-12` | Falls with D23 |
| D25 | `last_activity_xp` map + init | `manager.rs:1655`, `:2117` | Falls with D23 |
| D26 | `register_rampage_vote` | `manager.rs:4583-4595` | Yes once D2 lands |
| D27 | `RampageVoteOutcome` enum | `manager.rs:102-107` | Falls with D26 |
| D28 | `RAMPAGE_VOTE_THRESHOLD` (3) | `manager.rs:196` | Falls with D26 |
| D29 | `rampage_votes` HashSet + init | `manager.rs:1763`, `:2133` | Falls with D26 |
| D30 | `subscribe_announcements()` | `manager.rs:2242-2243` | Yes — sole consumer is `api.rs:403-411` |

**Do not delete `announcements_tx` itself** (`manager.rs:1721`, `:2110`).
Its producers are spread across `manager.rs` — `:2290, :2332, :2344,
:2380, :2421, :2426, :2477, :3469, :4420` — covering encounter results,
loot, batch summaries, rampage completion, unique-shard wins, gear crits,
the Wings giveaway and activity level-ups. It is the game's own event bus
and it is exactly what any web narration feed would subscribe to.
Deleting `subscribe_announcements` leaves the channel with no reader —
harmless (`broadcast::Sender::send` on a readerless channel is a no-op
`Err`, already discarded with `let _ =`) but it must be recorded as
*dormant, retained deliberately*, not as dead code for a later cleanup
pass to remove. See Part 3.3 and risk R3.

## 1E. Game crate — published constants (bot→game)

| # | Target | Location | Safe in isolation? |
|---|---|---|---|
| D31 | `PublishedConstants` struct + `PUBLISHED_CONSTANTS_PATH` | `game/src/adventure/published_constants.rs` (54 lines, whole file) | **No — see below** |
| D32 | Startup missing-file warning + `GAME_SUPPRESS_MISSING_PUBLISHED_CONSTANTS_WARNING` | `game/src/main.rs:107-119` | Yes |
| D33 | Wiki placeholder consumption of the five values | `game/src/adventure_web/wiki.rs:217-222` | **BLOCKED — wiki module** |

> **Coordination required (CLAUDE.md §Multi-session, rule 1).**
> `game/src/adventure_web/wiki.rs` belongs to the wiki session. This
> session does not propose editing it, and no removal stage may touch it
> without the owner sequencing that with the wiki session. The graceful
> path is available and needs **no** wiki edit: leave `wiki.rs` alone,
> delete only `api.rs`'s writer (D2) and the startup warning (D32), and
> the five placeholders render `"varies"` forever — which is already the
> documented, shipped fallback. D31/D33 are therefore **optional cleanup,
> deferred**, not part of the removal.

## 1F. Game crate — tests

| # | Target | Location | Safe in isolation? |
|---|---|---|---|
| D34 | `game/tests/api_seam.rs` (329 lines) — the whole file exists to prove the bot wire contract | delete | Yes |
| D35 | `game/tests/published_constants_http.rs` (97 lines) | delete | Yes |
| D36 | The `.env("ADVENTURE_API_SECRET", …)` at `killed_process_smoke.rs:47` and the three `x-adventure-api-secret` calls at `:78, :95, :107` | **rewrite, do not delete** — the liveness/recovery assertion is Twitch-independent and worth keeping; re-point it at a public route | No — needs a rewrite pass |

The other nine `game/tests/*_http.rs` files **survive untouched** and are
the single most valuable asset in this project. They already seed
`adventure-sessions.json` by hand and drive the site with a raw
`adv_session=` cookie (`admin_passives_http.rs:43-50`), never touching
OAuth. See Part 2 — this is the proof that the session layer is already
identity-provider-agnostic.

## 1G. Game crate — Cargo dependencies that become unused

| # | Dependency | Current sole use | Action |
|---|---|---|---|
| D37 | `url = "2"` (`game/Cargo.toml`) | `adventure_web.rs:567` — building the Twitch authorize URL. **Only use in `game/src/**`** | Delete |
| D38 | `reqwest = { version = "0.12", features = ["json"] }` | `adventure_web.rs:82,151` — the OAuth HTTP client. **Only use in `game/src/**`** | **Move to `[dev-dependencies]`** — every `game/tests/*_http.rs` needs it. Deleting it outright breaks the whole integration suite |

`urlencoding`, `base64`, `flate2`, `minijinja*` and `pulldown-cmark` all
have non-Twitch uses in `game/src/**` and stay.

## 1H. Bot crate — the Patreon integration

Patreon is fully self-contained in the bot crate and **touches no game
code whatsoever** (verified: zero `patreon` hits in `game/src/**` other
than a comment in `game/src/state.rs:3`).

| # | Target | Location |
|---|---|---|
| D39 | `src/patreon.rs` (319 lines, whole file) | delete |
| D40 | `src/bin/auth_patreon.rs` (one-time OAuth setup binary) | delete |
| D41 | `[[bin]] auth_patreon` | `Cargo.toml:19-20` |
| D42 | `pub mod patreon;` | `src/lib.rs:29` |
| D43 | `PatreonConfig` struct; `Config.patreon` field | `src/config.rs:11-`, `:28`, `:255` |
| D44 | `PATREON_CLIENT_ID` / `PATREON_CLIENT_SECRET` / `PATREON_POLL_INTERVAL_MS` reads | `src/config.rs:241-245` |
| D45 | Patreon watcher startup + new-patron poll loop | `src/main.rs:595-630`; import at `:20` |
| D46 | `!checkpatreon` command: registry entry, force-poll arm, mod-only list, alias list | `src/commands.rs:137`, `:692-703`, `:464`, `:474`, `:1445` |
| D47 | `Services.patreon` field + its doc | `src/commands.rs:24`, `:64`, `:69` |
| D48 | `.env.example:19-25` (the four Patreon lines) | delete |
| D49 | `README.md:3, 25, 39, 69, 74` | edit |
| D50 | Data files `patreon-tokens.json`, `patreon-seen.json` | retire from the deployment root |

**Every one of D39–D50 is safe in isolation** — the config is already
`Option`, so `None` disables the whole watcher today. Patreon can be
removed in a single self-contained commit with no game-side change at
all. It is the cheapest, lowest-risk work in this project.

## 1I. Deployment-root data files to retire

| # | File | Written by | Note |
|---|---|---|---|
| D51 | `channel-points-reforge-reward.json` | `src/main.rs:796` | Holds the Twitch reward id. The **reward itself must be deleted or disabled in the Twitch dashboard**, not just the file — see risk in Part 5 / Stage 7 |
| D52 | `channel-points-repair-reward.json` | `src/main.rs:802` | Same |
| D53 | `channel-points-force-boss-reward.json` | `src/main.rs:811` | Same |
| D54 | `channel-points-theme-reward.json` | `src/main.rs:768` | Bot-only (entrance themes); leaves with the bot |
| D55 | `channel-points-interrupt-reward.json` | `src/main.rs:784` | Bot-only; leaves with the bot |
| D56 | `bot-published-constants.json` | `api.rs:394` | Becomes permanently stale. Also listed at `backup-game-data.ps1:112` — that line goes with it |
| D57 | `patreon-tokens.json`, `patreon-seen.json` | `src/main.rs:599-600` | See D50 |
| D58 | `.env` keys: `ADVENTURE_API_SECRET`, `ADVENTURE_API_BASE_URL`, `TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET`, `TWITCH_CHANNEL`, all five `PATREON_*` | shared `.env` | `ADVENTURE_API_SECRET` last — see Part 4 Stage 1 |

**Not deleted, but affected:** `backup-game-data.ps1:112` (the
`bot-published-constants.json` entry), `watchdog.ps1` (the bot watchdog —
retires with the bot), `maintenance-flag.ps1:145-146` (the `-Target Bot`
branch), and `REFACTOR_PLAN.md` §13's "game healthy before the bot
starts" ordering rule, which becomes vacuous. These are ops artefacts,
not code, and belong in the final stage.

---

# PART 2 — Identity (the critical section)

## 2.1 What identifies a player and a character today, end to end

**One string: the lowercased Twitch login.** There is no user id, no
account record, no numeric key, no separate player entity. The character
*is* the account.

The full chain, in order:

1. **Twitch is the source.** `handle_callback`
   (`game/src/adventure_web.rs:660-669`) calls
   `GET https://api.twitch.tv/helix/users` with the freshly-exchanged
   user token and reads two fields off the response:
   `HelixUser { login, display_name }` (`:594-598`).
2. **A session is minted.** `adventure_web.rs:671-675` generates a
   32-byte random opaque token and inserts
   `Session { login, display_name, created_at }` (`:63-71`) into an
   in-memory `HashMap<String, Session>` keyed by that token.
3. **The token goes to the browser** as an HttpOnly cookie named
   `adv_session`, `SameSite=Lax`, `Max-Age` = `SESSION_TTL` = 30 days
   (`:55-57`, `:266`).
4. **The map is persisted** to `adventure-sessions.json` on every insert
   and every remove (`save_sessions`, `:92-96`), so a deploy restart
   never forces a re-login. It is loaded back at startup (`:141`).
5. **Every request resolves identity** through `current_session`
   (`:250-262`): read the cookie, look the token up, lazily drop it if
   older than the TTL, return `(login, display_name)`.
6. **The game keys characters by that login, lowercased.**
   `AdventureManager::join` (`manager.rs:2501-2521`) does
   `let key = username.to_lowercase()` and inserts into
   `HashMap<String, Character>`. `character()` (`:2525`), `all_characters()`
   (`:2536`), `equip_item()` (`:2541`) and every other per-player
   operation do the same `to_lowercase()` lookup.
7. **That map is the save file.** `adventure-characters.json` is
   literally `HashMap<lowercased_login, Character>`, loaded fail-loud at
   `manager.rs:1800`.

**The `Character` struct itself contains no identity field at all**
(`game/src/adventure/character.rs:345-`). Its first field is
`display_name`, and its own doc says so explicitly: *"As typed the first
time this person !joined — display only; matching against chat always
goes through the lowercased map key instead."* Identity lives **entirely
in the map key**, never inside the record.

### `adventure-sessions.json`, exactly

- **Holds:** a JSON object mapping opaque session token → `{"login":
  "...", "display_name": "...", "created_at": <unix seconds>}`. Nothing
  else. No Twitch access token, no refresh token, no scopes — the OAuth
  token is used once inside `handle_callback` and dropped on the floor.
- **Written by:** `game.exe` only, via `save_sessions`
  (`adventure_web.rs:92-96`) on login (`:675`) and logout (`:685`).
- **Read by:** `game.exe` at startup (`:141`).
- **Path:** `PathBuf::from("adventure-sessions.json")` passed from
  `game/src/main.rs:165` — **CWD-relative, deliberately NOT
  `data_path`-wrapped** (confirmed by `backup-game-data.ps1:104`'s own
  annotation). It sits in the deployment root next to the binaries.
- **The bot never touches it** — consistent with audit link L21.

### Three hardcoded logins are also identity

| Constant | Value | Location | Governs |
|---|---|---|---|
| `ADMIN_TUNABLES_LOGIN` | `"lokati_gaming"` | `adventure_web.rs:2415` | `/admin/tunables`, `/admin/passives` and both their save routes |
| `FIGHTS_PAGE_LOGIN` | `"lokati_gaming"` | `:2266` | `/fights` operator view |
| `BUNDLE_OPERATOR_LOGIN` | `"lokati_gaming"` | `:2272` | full-roll replay bundles |

All three compare against the session's `login` string. They are
authorization, not just identity, and they break the moment the login
string stops being a Twitch login. Their doc at `:2268-2272` deliberately
keeps them as three separate constants; that decision should survive.

## 2.2 Where Twitch identity enters, and everything downstream

**It enters at exactly one point:** `handle_callback`,
`game/src/adventure_web.rs:660-675` — the Helix `GET /users` response.
That is the only place in `game/src/**` where a Twitch login is
*obtained*. (The bot supplies a login independently over `/api/*`, but
that path is being deleted wholesale, so it is not a second entry point
in the new world.)

Everything downstream of `Session.login`:

| Consumer | Location | What it does with the login |
|---|---|---|
| `current_session` | `:250-262` | the single accessor — every other consumer goes through it |
| `do_join` | `:692-697` | `adventure.join(&login, &display_name)` → creates the character record under that key |
| Every dashboard mutation — equip, unequip, disenchant, reforge, repair, craft, name-item, archetype/model change, wings, auto-repair, auto-disenchant | routes `:165-182` | `manager` lookups keyed by `login.to_lowercase()` |
| Every passive-tree route incl. memories | `:183-195` | same |
| `/characters/:login`, `/characters/:login/passives` | `:201-202` | login is a **URL path segment**, matched case-insensitively against the map key |
| `/fights`, `/fights.json`, `/fights/:seq/members/:member` | `:203-204`, `:214` | participant-tier check compares login against fight member names (`:2274-2280`) |
| `/admin/*` | `:205-213` | equality against `ADMIN_TUNABLES_LOGIN` |
| `/overlay` "Highlight Me" | `:1910`, `:2129` | `own_login` embedded into a JS string literal (escaped via Rust `Debug`) |
| `/overlay` Twitch chat embed | `:2139-` | gated on `session.is_some()` |
| `AdventureManager` internals | `manager.rs` throughout | `characters`, `last_activity_xp`, `rampage_votes`, `downed_until`, memories, cooldowns — **every** per-player map in the manager is keyed by the same lowercased login string |
| `adventure-characters.json` | on disk | the map key *is* the login |

## 2.3 What breaks the moment Twitch OAuth is removed, in order

1. **`game.exe` refuses to start.** `game/src/main.rs:95-97` reads
   `TWITCH_CLIENT_ID` and `TWITCH_CLIENT_SECRET` with `ok_or_else(…)?` —
   a hard error, not an `Option`. This fires before anything else. It is
   also the *easiest* break to fix (delete two lines), and it is why the
   removal must be sequenced rather than attempted as one commit.
2. **`GET /login` 404s or dead-ends.** `top_nav` links to it from every
   page. Nobody can obtain a new session.
3. **`GET /auth/callback` is unreachable.** No new sessions are ever
   minted again.
4. **Existing sessions keep working for up to 30 days** — this is the
   important one. `adventure-sessions.json` is already on disk and
   `current_session` never re-validates against Twitch. Removing OAuth
   does *not* log anyone out; it silently stops anyone new from getting
   in, and stops everyone else 30 days later as tokens age past
   `SESSION_TTL` (`:257-259`). **The failure is invisible for a month.**
   That is Part 5's first risk.
5. **`do_join` becomes unreachable for new players.** Web join
   (`:692-697`) requires a session. With no way to mint one, the roster
   is frozen.
6. **The three operator gates close permanently** once the operator's own
   session expires — including `/admin/tunables`, i.e. the operator loses
   the ability to tune the live game.
7. **The `/overlay` Twitch chat embed and "Highlight Me" both go dark**,
   since both are gated on `session.is_some()`.
8. **Nothing in combat, persistence, the encounter loops, the overlay
   feed, or the wiki breaks at all.** The engine does not know Twitch
   exists. Fights keep resolving and keep saving.

## 2.4 Minimum viable replacement, in this codebase

The honest answer: **the replacement is small, because the session layer
is already abstracted from Twitch and the test suite already proves it.**

`game/tests/admin_passives_http.rs:43-50` writes
`adventure-sessions.json` by hand — `{"admin-token": {"login": "...",
"display_name": "...", "created_at": ...}}` — then drives the entire
authenticated site with `Cookie: adv_session=admin-token`. Nine of the
integration tests do this. **The whole dashboard already runs today with
sessions Twitch never issued.** Twitch is a *session minter*, not an
identity system.

So the MVP is: replace the minter, keep everything else.

| Layer | Change required |
|---|---|
| **Character model** (`character.rs:345-`) | **None.** No identity field exists to change |
| **Save format** (`adventure-characters.json`) | **None.** Still `HashMap<String, Character>`. The keys stop being Twitch logins and become whatever the new minter issues |
| **Session handling** (`adventure_web.rs:63-96, 250-270`) | **None to the mechanism.** `Session { login, display_name, created_at }`, the opaque token, the cookie, the 30-day TTL, `save_sessions`, `current_session` — all survive verbatim. `login` simply becomes an account id rather than a Twitch login |
| **Web routes** | Replace `/login` + `/auth/callback` (D9–D12) with *one* handler that mints a `Session`. `/logout` (`:681-690`) is unchanged. Every other route is untouched |
| **Startup** (`main.rs:95-97`) | Delete the two hard-fail Twitch env reads |
| **Cargo** | `url` out, `reqwest` down to dev-dependencies (D37, D38) |
| **The three operator constants** | `ADMIN_TUNABLES_LOGIN` / `FIGHTS_PAGE_LOGIN` / `BUNDLE_OPERATOR_LOGIN` must be re-pointed at whatever id the operator gets under the new scheme. If they are missed, the operator is locked out of live tuning. Keep them three separate constants |
| **Login-format assumption** | `adventure_web.rs:2053` records that Twitch logins are `[a-z0-9_]`. `:2049-2060` already proves the JS-embedding path is safe even for hostile values, so the assumption is *documented but not relied on for safety*. Any new id scheme should still be validated at mint time and the constraint restated |

**Concretely, the smallest thing that ships:** a `/login` that accepts a
name, mints a session, and redirects — no password, no provider. That is
roughly the twenty lines the tests already simulate. It is a placeholder
and must be understood as one: it is authentication-free, so it must not
be exposed publicly without at minimum an operator-set shared secret in
front of it. **This document does not propose which placeholder to
build** — that is the owner's call, and Part 4 Stage 6 is written to be
blocked on it.

## 2.5 Can character records be made identity-agnostic?

**They already are.** This is the strongest finding in this document.

`Character` holds no identity. `display_name` is explicitly documented as
"display only" (`character.rs:346-349`). Identity lives solely in the
`HashMap` key, in exactly two places: the key in
`adventure-characters.json`, and the `login` field of `Session`.

An external identity provider can therefore be attached later by making
`Session.login` hold that provider's account id and doing **nothing
else** — no change to `Character`, no change to the save format, no
change to combat, passives, crafting, memories, or the fight archive.
The only work is at the seam that mints a `Session`.

**One caveat, and it is real.** The key is used as a *display and routing*
value as well as a lookup key: `/characters/:login` puts it in the URL,
`character_detail` renders it, and fight participant-tier checks compare
it against fight member names. An opaque id (a UUID) would make those
URLs and comparisons ugly or wrong. The clean shape is to keep the map
key **human-readable and stable** — a chosen account name — and let the
external provider map *its* id onto that name at mint time. That keeps
every URL, every fight record, and every existing test working.

## 2.6 Does "fresh characters, no migration" simplify things?

**Yes, substantially — it removes the single hardest part of the job.**

Specifically it removes:

- Any need to map old Twitch logins onto new account ids.
- Any need for a dual-key or aliasing period in `AdventureManager`.
- Any `migrations.rs` entry (`game/src/adventure/migrations.rs`) or
  one-time backfill marker — and `backup-game-data.ps1:118-133` lists
  **eleven** such markers, every one of which would otherwise need
  thinking about.
- Any risk of the fail-loud loader (`manager.rs:1800`,
  `load_json_fail_loud`) refusing to start on a half-migrated roster.
- Any concern about the existing fight archive's member names no longer
  matching live character keys.

What it does **not** simplify: the operator constants (Part 2.4) still
need re-pointing, and `adventure-sessions.json` must be **deleted, not
migrated**, at cutover — a stale session file would grant 30 days of
access under Twitch logins that no longer correspond to any character.

---

# PART 3 — Gameplay that dies with the integrations

## 3.1 `next_encounter` and `rampage` — the live-ops pacing levers

**Code.** `next_encounter`: endpoint `api.rs:171-181` →
`AdventureManager::trigger_encounter_now(forced)`. `rampage`: endpoint
`api.rs:219-229` → `start_rampage()` (mod path) or
`register_rampage_vote()` (player path). Bot triggers at
`src/commands.rs:1075-1108`.

**What is lost.** The ability to make something happen *now*. Both
underlying manager functions are game-side and survive the deletion
untouched; only the trigger disappears. The automatic loops keep running:
`spawn_encounter_loop`, `spawn_basic_encounter_loop` and
`spawn_rampage_loop` are all started unconditionally at
`game/src/main.rs:126-129` and know nothing about the seam. The world
keeps turning on its own timers.

**Web replacement.** Small: two operator-gated POST routes behind the
existing `ADMIN_TUNABLES_LOGIN` check, plus two buttons on the existing
`/admin/tunables` page. That page already renders live pacing state
(`current_pacing_status()`, `adventure_web.rs:2693-2694`;
`render_tunables_page:3556`), so this is a button next to a readout that
already exists. `trigger_boss_intro` (`!event intro`) is the same shape.

**Web equivalent today: NONE.**

## 3.2 The rampage 3-vote player mechanic

**Code.** `register_rampage_vote` (`manager.rs:4583-4595`),
`RAMPAGE_VOTE_THRESHOLD = 3` (`:196`), the `rampage_votes: HashSet`
dedupe (`:1763`), `RampageVoteOutcome` (`:102-107`), and the vote-count
reply strings at `api.rs:225-227`.

**What is lost.** The only *collective player agency* mechanic in the
game. Everything else a player does affects only their own character.
Three distinct viewers typing `!rampage` changed the world for everyone.
That is a genuinely distinct thing, and it is worth saying plainly that
the removal loses it rather than moves it.

**Web replacement.** Harder than it looks, and the difficulty is entirely
identity. The mechanic needs (a) N *distinct* players — which the
`HashSet<String>` of logins already provides, and which survives any
session-based identity — and (b) some notion of who is currently
*present*. Chat presence was the implicit answer to (b), and there is no
web equivalent for it. A `/rampage-vote` POST gated on a session, with a
time window and the existing threshold, is the honest translation; it is
a *different* mechanic (any logged-in player, any time) rather than a
port. **This is a design decision for the owner, not a mechanical port.**

**Web equivalent today: NONE.**

## 3.3 `/api/announcements/stream` — game events to chat

**Code.** Game end: `api.rs:403-411`, an SSE wrapper over
`subscribe_announcements()` (`manager.rs:2242`), itself a
`tokio::broadcast` channel (`manager.rs:1721`, `:2110`). Bot end:
hand-rolled SSE parser `src/adventure_client.rs:162-188`, relay loop
`src/main.rs:1032-1051`, passing each frame verbatim to
`chat_client.say()`.

**What is lost.** All narration. The game-side producers pushing into
`announcements_tx` cover encounter results (`:2290`, `:2332`), loot
(`:2344`), batch summaries (`:2380`), rampage completion (`:2421`),
unique-shard wins (`:2426`), gear crits (`:2477`), the Wings giveaway
(`:3469`) and activity-XP level-ups (`:4420`). None of these has any
other outlet. The game becomes silent.

**This is the most important item in Part 3, and also the cheapest to
replace**, because the producer side is entirely intact. Everything that
formats and emits an announcement lives in `game/**` and is untouched by
the removal. What dies is one consumer.

**Web replacement.** The game already runs a WebSocket (`/ws`,
`adventure_web.rs:216`, reusing
`adventure_overlay_server::handle_socket`) that pushes live state to the
dashboard and the OBS overlay. Adding a second `announcements`
subscriber that pushes each string down that existing socket is a small,
contained change — the same `subscribe_announcements()` the SSE endpoint
used, feeding a scrolling feed panel instead of Twitch chat. **The
`announcements_tx` channel must therefore be preserved through the
removal** (see D30's note in Part 1D, and risk R3).

**Web equivalent today: NONE.** The overlay shows fight *state*; it does
not show the formatted announcement lines.

## 3.4 Chat activity XP

**Code.** `api.rs:374-376` → `grant_activity_xp`
(`manager.rs:4397-4425`). 4 XP (`ACTIVITY_XP_AMOUNT`, `manager.rs:12`)
per chat message, rate-limited to once per 180s per user
(`ACTIVITY_XP_COOLDOWN`, `:11`). Level-ups push straight onto
`announcements_tx` (`:4420`). Highest-volume link in the system — one
POST per chat message (`src/main.rs:1108-1116`, fire-and-forget).

**What is lost.** The entire "reward people for being present" economy.
Characters gain XP from fights alone. Also lost: the *only* signal the
game has that a person exists and is active outside of combat.

**Web replacement.** The mechanic is "presence earns XP", and the web has
a presence signal the game already maintains — an open `/ws` connection
from a logged-in session. A tick that grants `ACTIVITY_XP_AMOUNT` per
`ACTIVITY_XP_COOLDOWN` to each session with a live socket is a faithful
translation and reuses the existing constants and the existing
`add_xp`/level-up path. It is also trivially farmable by leaving a tab
open, which chat was not — flag that as a balance decision for the owner,
not a technical one.

**Web equivalent today: NONE.**

## 3.5 The channel point redemptions

**Code.** Endpoints `api.rs:305-360` (`redeem_reforge`, `redeem_repair`,
`redeem_force_boss`). Bot handlers `src/main.rs:357-368`, `:387-391`,
`:407-420`. Reward creation `src/channel_points.rs:101-111`, ids
persisted to `channel-points-{reforge,repair,force-boss}-reward.json`.
Missed-redemption replay `src/main.rs:445-490`.

| Reward | Web equivalent today |
|---|---|
| Reforge Gear | **YES** — `POST /reforge` (`adventure_web.rs:170`), paid with dust at `WEB_REFORGE_DUST_COST` |
| Repair All Gear | **YES** — `/repair-all`, `/repair-equipped`, `/repair-item` (`:171-173`), plus auto-repair (`:178`) |
| Force Boss Fight | **NO** — `try_force_encounter` (`manager.rs:4651-4663`) with its cycle-wide budget (`ForceBossOutcome::CycleLimitReached`) has no web route at all |

Two of the three already have web equivalents, and the game's own code
says why (`craft.rs:19`, `manager.rs:2885`: *"can't actually spend a
viewer's Twitch channel points — no API lets a website do that"*) — which
is exactly why the dust-priced web versions were built in the first
place.

**Web replacement for Force Boss.** A dust-priced or cooldown-gated
button reusing `try_force_encounter` and its existing budget. Same shape
as `/reforge`.

**Web equivalent today: partial — Force Boss is the only gap.**

## 3.6 The chat commands

Corrected counts per the opening section: **10 endpoints, 15 trigger
words.**

| Trigger(s) | Endpoint | Web equivalent today |
|---|---|---|
| `!join` | `/api/commands/join` | **YES** — `POST /join`, `adventure_web.rs:164`, `do_join:692-697` |
| `!character` `!char` `!me` | `/api/commands/character` | **YES** — `/` index and `/inventory`, richer than the chat reply |
| `!party` `!adventure` | `/api/commands/party` | **YES** — `/characters` (`:200`) |
| `!nextencounter [boss]` | `/api/commands/next_encounter` | **NO** |
| `!event intro <boss>` | `/api/commands/event_intro` | **NO** |
| `!rampage` (mod + 3-vote) | `/api/commands/rampage` | **NO** |
| `!clearbattlefield` `!resetbattlefield` | `/api/commands/clear_battlefield` | **NO** |
| `!giveloot` `!gearall` | `/api/commands/give_loot` | **NO** |
| `!giftdust <target> <n>` | `/api/commands/gift_dust` | **NO** |
| `!pinfight` | `/api/commands/pin_fight` | **NO (partial)** — `list_pinned_fights()` already *renders* pinned fights on the admin page (`adventure_web.rs:3600`), but nothing can *pin*. `pin_most_recent_fight` (`fight_storage.rs:261-331`) is fully game-side and needs only a button |

**All three player-facing commands already have web equivalents.** Every
gap is a mod / live-ops tool.

## 3.7 What Patreon status grants a player in-game

**Nothing. Verified, not assumed.**

`grep -rni "patreon" game/src/` returns exactly one hit: a comment in
`game/src/state.rs:3` naming `patreon-seen.json` as an example of the
JSON-persistence pattern it ports. No game code reads Patreon status, no
`Character` field records it, no tunable references it, no drop rate or
cost varies by it.

The entire integration is: poll the campaign member list on an interval,
diff against `patreon-seen.json`, and for anyone new call one callback
(`src/main.rs:604-614`) that says *"New Patreon supporter: {name} (Tier:
{tier})! Support here: {url}"* in Twitch chat. That is the whole feature.
There is not even an alert-box overlay alert — just the chat line.

**Removing Patreon takes nothing away from any player.** It is a pure
deletion with no gameplay consequence, and it is the safest work in this
project.

## 3.8 Summary — the actual work items

Features with **no web equivalent today**. These are the only real work
in this project:

| # | Feature | Replacement cost |
|---|---|---|
| **W1** | **Announcement feed** (§3.3) | **Small.** Producers all survive; add one `subscribe_announcements()` consumer onto the existing `/ws`. Highest value per line of code in the entire project |
| **W2** | Operator controls — `next_encounter`, `event intro`, `rampage` (mod), `clear_battlefield`, `give_loot`, `gift_dust`, `pin_fight` (§3.1, §3.6) | **Small-medium.** Seven buttons on the existing `/admin/tunables` page behind the existing operator gate. Every underlying manager function already exists and survives |
| **W3** | Force Boss (§3.5) | **Small.** One dust-priced route reusing `try_force_encounter` |
| **W4** | Chat activity XP (§3.4) | **Medium.** Needs a presence signal; `/ws` connections are the natural one. Carries a balance decision |
| **W5** | Rampage 3-vote (§3.2) | **Medium, and a design question.** The only collective-agency mechanic in the game; the web version is a redesign, not a port |

Everything else in Parts 1 and 3 is deletion with no replacement needed.

---

# PART 4 — Order of operations

Seven stages. Each is independently shippable and independently
revertible. Stages 1–5 and 7 remove or add things without touching
identity; only Stage 6 is blocked on the identity replacement existing.

### Stage 1 — Turn the seam off without deleting anything

**Ship:** unset `ADVENTURE_API_SECRET` in `.env`, restart the game, stop
the bot **permanently** (by PID, or by resolving the listening port to
its owning PID — never by image name, per the production-safety rule).
**Why first:** `api.rs:61-62` un-mounts the entire `/api/*` router with
no code change, so this is a fully reversible, zero-code proof of the
whole removal. It also resolves the ordering constraint: the bot
hard-fails without the secret (`src/config.rs:238-239`), so the secret
can only be pulled once the bot is gone for good.
**Test:** every `/api/*` path returns 404 (not 401); the dashboard, wiki,
overlay and `/ws` are unaffected; a fight resolves and persists.
**Blocked on identity?** No.

### Stage 2 — Delete Patreon

**Ship:** D39–D50 and D57. One commit, bot crate only.
**Why here:** completely self-contained, touches no game code, has zero
gameplay consequence (§3.7), and gets an entire integration off the board
before anything risky starts.
**Test:** `cargo build --release --workspace`, then the full suite
`cargo test --release --workspace --quiet`.
**Blocked on identity?** No.

### Stage 3 — Add the announcement feed (W1) **before** deleting the SSE endpoint

**Ship:** a second `subscribe_announcements()` consumer pushing onto the
existing `/ws`, plus a feed panel on the dashboard.
**Why before the deletion:** this is the one place where build-then-delete
beats delete-then-build. Shipping the replacement first means the game is
never silent, and it forces the `announcements_tx` retention decision
(D30) to be made deliberately rather than discovered later.
**Test:** a new `game/tests/*_http.rs` that drives a fight and asserts the
formatted announcement arrives over `/ws`. This also becomes the
regression guard for Stage 4.
**Blocked on identity?** No.

### Stage 4 — Delete the `/api/*` seam

**Ship:** D1–D8, D23–D30, D32, D34–D36.
**Note:** this is where `grant_activity_xp` and `register_rampage_vote`
die (W4, W5). Accept the gap deliberately, or move their replacements
ahead of this stage. Recommend accepting it — both are already
functionally dead as of Stage 1.
**Test:** full workspace suite. `api_seam.rs` and
`published_constants_http.rs` are deleted, so the test count drops —
**report the delta together with the deleted-file names**, or the drop
reads as a regression.
**Blocked on identity?** No.

### Stage 5 — Operator controls and Force Boss (W2, W3)

**Ship:** the seven operator buttons on `/admin/tunables` and the Force
Boss route.
**Why here:** these restore live-ops capability that Stage 1 removed, and
they must land while the operator's existing Twitch session still works —
i.e. **within 30 days of Stage 1** (Part 2.3, item 4). If Stage 6 is
going to slip past that window, this stage is what keeps the operator
able to tune the live game in the meantime.
**Test:** an HTTP test per route asserting the operator gate (rejection
for a non-admin session, success for the admin session) — the shape
`admin_passives_http.rs:91-121` already uses.
**Blocked on identity?** No — it rides on the *existing* session
mechanism.

### Stage 6 — Replace the identity minter ⚠️

**Ship:** delete D9–D22, D37, D38; add the replacement `/login` (Part
2.4); re-point `ADMIN_TUNABLES_LOGIN`, `FIGHTS_PAGE_LOGIN` and
`BUNDLE_OPERATOR_LOGIN`; delete `adventure-sessions.json` and
`adventure-characters.json` at cutover (fresh characters, Part 2.6);
delete D19's hard-fail startup env reads last.
**⚠️ BLOCKED — this stage cannot ship until the owner has ruled on what
the placeholder identity scheme is.** Everything else in this plan is
mechanical; this is the one open decision. Nothing in Stages 1–5 depends
on it.
**Test:** the nine surviving `*_http.rs` files, unchanged, are the
acceptance test — they already mint sessions without Twitch, so if they
pass against the new minter the replacement is faithful. Add one test
that the operator gate still admits the operator and rejects everyone
else under the new scheme, plus the stale-session test from risk R1.

### Stage 7 — Ops and documentation cleanup

**Ship:** retire `watchdog.ps1` and the `-Target Bot` branch of
`maintenance-flag.ps1:145-146`; remove `backup-game-data.ps1:112`; strip
the bot-ordering rule from `REFACTOR_PLAN.md` §13; update `README.md`;
delete the root package and its remaining binaries; **manually delete or
disable the five channel-point rewards in the Twitch dashboard** (D51–D55
— the files only record ids; the rewards live on Twitch's side and no
code deletion touches them).
**Blocked on identity?** No.

---

# PART 5 — Risk

## R1 — The 30-day silent session cliff *(highest risk in the project)*

**What breaks.** Removing OAuth does not log anyone out.
`adventure-sessions.json` is already on disk, `current_session`
(`adventure_web.rs:250-262`) never re-validates against Twitch, and
`SESSION_TTL` is 30 days (`:57`). Every logged-in player — **including
the operator** — keeps full access after Stage 6 and then, one at a time,
on the anniversary of their own individual login, silently loses it with
no way back in. Every smoke test passes. Every click-through works. The
failure arrives weeks later, staggered per user, and the operator's own
lockout takes `/admin/tunables` with it.

The reverse mistake is equally silent: shipping Stage 6 while *keeping*
the old sessions file grants 30 days of access under Twitch logins that
no longer correspond to any character in a fresh roster.

**Test that catches it.** An HTTP test that seeds
`adventure-sessions.json` with a `created_at` older than `SESSION_TTL`,
then asserts (a) the stale cookie is rejected, and (b) the *replacement*
minter can issue a working session for the same account with no Twitch
call anywhere in the process. Run it against a scratch data dir with
**no** `TWITCH_CLIENT_ID` in the environment — if the binary still
refuses to start, the test cannot run, and that is itself the finding.

## R2 — The form-field trap, in the direction no existing test can see

**What breaks.** CLAUDE.md documents this trap by name, and Stage 5 walks
straight into it. `TunablesForm` (`adventure_web.rs:2793-2860`) is a
large struct consumed by `do_save_tunables`. Stage 5 adds operator
buttons to that same page; Stage 6 removes Twitch-derived fields from
pages that render forms. Either direction of drift between the rendered
form and the struct produces a **422 with a green test suite**: a new
required field breaks every test posting the old set (the 2026-08-22
regression), and a removed `<input>` whose field stays required breaks
every *real browser save* while a hand-maintained superset body keeps
passing (the 2026-08-23 dynamic-pacing regression, where saves silently
did nothing).

The second direction is the one to fear here, because Stage 6 is
specifically about *removing* rendered inputs.

**Test that catches it.** `admin_tunables_splash_http.rs` already does
the right thing — it GETs the page, scrapes the `name="..."` attributes
out of the rendered form, and POSTs exactly those. **Copy that shape for
every form touched in Stages 5 and 6**, and put `#[serde(default)]` on
every new field. A hand-maintained field list is not a test.

## R3 — `announcements_tx` deleted as dead code

**What breaks.** After Stage 4 removes `subscribe_announcements()`
(D30), the `announcements_tx` broadcast channel has many producers and —
if Stage 3 was skipped or reordered — zero consumers. It will not warn:
`broadcast::Sender::send` on a readerless channel returns `Err`, and
every call site already discards it with `let _ =`. Clippy will not flag
it. A later "remove unused code" pass, or a reviewer reading
`manager.rs:1721` in isolation, deletes it as obviously dead — and takes
with it every `format_gear_crit`, `format_unique_shard_win`,
`format_batch_summary` and encounter-result call site that feeds it.
That is the entire narration layer, removed in a cleanup commit, and the
symptom is silence rather than a failure.

**Test that catches it.** Stage 3's own test, which drives a real fight
and asserts a formatted announcement arrives over `/ws`. It must land
**before** Stage 4 — which is the whole reason Stage 3 is ordered first.
Additionally: a comment on `announcements_tx` stating it is deliberately
retained and naming its consumer. The codebase already uses exactly this
convention for `bot-published-constants.json`'s deliberately-unchanged
on-disk format.

---

## Appendix — figures at a glance

| Measure | Value |
|---|---|
| Deletion targets enumerated | **58** (D1–D58) |
| Game-crate source targets | 33 (D1–D33) |
| Bot-crate Patreon targets | 12 (D39–D50) |
| Tests deleted | 2 files, 426 lines (`api_seam.rs`, `published_constants_http.rs`) |
| Tests rewritten | 1 (`killed_process_smoke.rs`) |
| Tests surviving untouched | 9 `*_http.rs` files — and they are the acceptance test for Stage 6 |
| Cargo deps removed from `game` | 1 (`url`); 1 demoted to dev-dependency (`reqwest`) |
| Identity fields in `Character` | **0** |
| Places identity actually lives | **2** — the `adventure-characters.json` map key, and `Session.login` |
| Hardcoded operator logins to re-point | 3 |
| Gameplay features with no web equivalent | **5** (W1–W5) |
| Stages | **7**, of which **1** is blocked on identity |
| Things Patreon grants in-game | **0** |
