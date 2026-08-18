# Path of Dust — Architecture Refactor Plan

**Status** (2026-08-19): Stages 0 through 5 done, on branch
`refactor/architecture` (pushed to origin as a backup — not merged to
master, not deployed; the deploy procedure still applies to master
exclusively). `bot` is code-complete as a thin HTTP client of the
standalone `game` process, failure-isolation hardened and tested against
a genuinely killed process (see Stage 5's own note) - but this is STILL
not what's running in production: that only happens after the required
LIVE BAKE period and a final merge to master. A bake deployment proposal
(what changes on the machine, rollback, what to watch) has been written
up separately as a stop-and-wait for owner approval - not yet actioned.
This document is the durable record of the plan — written to the repo
root so a fresh session can resume
the project with full context, without needing the original Plan-mode
transcript.

**Scope escalation, mid-audit**: the original ask ("refactor into
domain/persistence/web/ws") was superseded by an addendum: the adventure
game must become a **standalone process**. The bot becomes a thin client
of the game. This is the spine of the whole project; the original
4-bucket internal layering still happens, but as internal structure
*within* the new `game` crate, not as the primary deliverable.

---

## 1. Correction to the original brief

- **Scope is the adventure/game subsystem only, confirmed with the
  owner**: src/adventure/{affix,balance,character,combat,craft,
  fight_storage,item,manager,migrations,tunables}.rs (10 files),
  src/adventure_web.rs + submodules (render.rs, wiki.rs),
  src/adventure_overlay_server.rs (missed by the original file glob —
  imports AdventureManager directly, owns the OBS overlay's own /ws),
  src/passive_tree.rs. ~24,700 lines at Stage 0 time (now larger — see
  §12 for what's landed since). The rest of the bot (~40k total repo, 25
  other files — song requests, Patreon, PayPal, alerts, overlays, Twitch
  plumbing) is confirmed clean of adventure coupling and explicitly OUT
  of scope for internals — but the seam BETWEEN bot and game (main.rs,
  commands.rs, config.rs) is IN scope, per the addendum.
- **`/ws` correction**: served by axum's native `WebSocketUpgrade`, NOT
  tokio-tungstenite. tokio-tungstenite is used only for two OUTBOUND
  client connections (OBS WebSocket, Twitch EventSub) — unrelated to the
  game's own /ws.
- **No unified Router anywhere in the bot today**: 5 fully independent
  Axum servers already run in one process (alerts:4001, song_overlay:4002,
  chat_overlay:4003, adventure_overlay:4004, adventure_web:4005), each
  its own listener/state/router. Good news for the process split — the
  game's two servers are already isolated at the router level.

---

## 2. Owner decisions (from the Stage 0 clarifying questions)

1. **Scope**: game subsystem only for internals; bot/game boundary
   (main.rs, commands.rs, config.rs) is in scope for the seam work. Bot
   internals otherwise untouched.
2. **5th module bucket**: confirmed — add `app/` for AdventureManager's
   async-orchestration shell (spawn loops, broadcast channels,
   Mutex-wrapped state).
3. **Cargo workspace**: confirmed warranted — `game/` and `bot/` as two
   crates/binaries. The workspace split is *only* at the bot/game
   boundary; wiki.rs and the entire internal domain/persistence/web/
   ws/app layering all stay inside ONE `game` crate — `crate::` still
   means "the game crate" throughout, so the flat-reexport-facade
   strategy (§5) is unaffected.
4. **Real save-file fixture**: pseudonymize deterministically (one
   generated placeholder per unique login, applied consistently across
   the whole file), keep full roster size/stat diversity, commit the
   pseudonymization script alongside the fixture so future refreshes
   repeat the process, generate the fixture BEFORE the first commit that
   touches `tests/fixtures/` — nothing from the real file ever enters
   git history.

---

## 3. ADDENDUM — the standalone-game requirement (verbatim intent)

The adventure game must run as its own process: starts, fights,
persists, serves its full web UI (dashboard, wiki, overlay, /ws) with
the Twitch bot not running at all. The bot becomes a client.

**Target**: Cargo workspace —
- `game/` — everything adventure: domain, persistence, web, ws, the
  wiki module (moved intact), its own `main()` producing its own binary.
  Zero Twitch dependencies.
- `bot/` — Twitch chat/IRC, EventSub, songs, themes, alerts, Patreon/
  StreamElements: its own binary, a thin client of the game. Internals
  otherwise untouched.
- Seam: a real API on the game's existing Axum server (internal
  endpoints, e.g. `/api/`): chat-command ingestion, channel-point
  redemption handoff with fulfil/refund outcomes, activity-XP events,
  and an announcements channel for game-initiated messages. Localhost-
  only or shared-secret header — not public.

**Rules**: (1) public chat-command behavior unchanged, latency
negligible; (2) failure isolation both directions — game down → bot
answers game commands with a polite fallback, queues/refunds
redemptions (policy per reward type, §4c); bot down → game runs fully,
announcements drop gracefully (§4b — **owner-confirmed**, see §11);
(3) persistence/saves/fight storage entirely game-side, config splits
per binary; (4) two binaries, two watchdog/task entries, order-
independent startup, deploy stays one command for the owner; (5) Stage
0's audit extends to a full seam map (§4) — stage the split so the game
is playable after every stage, no long-broken big-bang.

**Owner addition (approved with the rest of Stage 0)**: after Stage 4's
process-split cutover and Stage 5's failure isolation, insert a LIVE
BAKE period — run the two-process shape in production for at least a
day of real stream traffic before proceeding to Stage 6+'s internal
decomposition. See §10.

---

## 4. Full seam map (every bot↔game touchpoint, classified)

Gathered by direct reads of main.rs's redemption handlers, the
encounter-broadcast subscribers, and commands.rs's adventure command
bodies. **Headline finding: the seam is thinner and cleaner than
feared** — every touchpoint below already has the shape of "call one
manager method, get one enum/value back, format one reply string" with
no deep interleaving, EXCEPT the channel-points EventSub dispatch/
reconcile functions, which are mechanically shared across reward types
but not logically entangled.

### 4a. Synchronous, request/response (bot asks game something, waits for the answer)

| Bot-side call site | Today | Becomes |
|---|---|---|
| `!join` (commands.rs:1043) | `services.adventure.join(user,user).await` → `JoinOutcome` → format reply | `POST /api/commands/join {user}` → game returns the **already-formatted reply string** |
| `!character`/`!char`/`!me` (1055) | `.character(user).await` → `Option<Character>` → format reply | `GET /api/commands/character?user=` → formatted reply string |
| `!party`/`!adventure` (1072) | `.party_status().await` → `(stage,active,total)` → format | same pattern |
| `!nextencounter [boss]` (1082) | `.trigger_encounter_now(forced).await` → `TriggerEncounterOutcome` → format (or `Reply::None`, real result comes later via announcement) | same — note the FIGHT RESULT itself is NOT synchronous, see §4c |
| `!event intro <boss>` (commands.rs:1467) | `.trigger_boss_intro()` + `BossKind::parse_forced` + `kind.wiki_slug()` → chat text with a wiki link | same pattern — game already owns the wiki-slug knowledge |
| `!rampage` (1110) | `.start_rampage()` or `.register_rampage_vote(user)` → `RampageVoteOutcome` → format | same pattern |
| `!clearbattlefield`/`!resetbattlefield` (1129) | `.clear_battlefield().await` → count → format | same pattern |
| `!giveloot`/`!gearall` (1141) | `.grant_random_gear_to_all().await` → count → format | same pattern |
| `!giftdust` (1153) | `.grant_dust_to_all`/`.grant_dust` → bool/count → format | same pattern |
| `!pinfight` (1445) | `pin_most_recent_fight()` (free fn, not even on AdventureManager) → format | same pattern, becomes an API call regardless |
| 3 channel-points redemption handlers (main.rs:340-446) | each a clean, single-purpose fn taking `adventure: &Arc<AdventureManager>`, calling one manager method, matching FULFILLED/CANCELED + an optional chat line | becomes one API call each; the FULFILLED/CANCELED Twitch status update stays bot-side (needs `helix`) but is driven by the API response |
| Per-message activity XP (main.rs:1422) | fires on EVERY chat message, awaited inline in the main chat loop | **fire-and-forget** — bot doesn't block the message loop on this (§4c) |

**Design principle**: the game's API returns **already-formatted reply
strings**, not raw enums for the bot to re-format. All player-facing
text authoring stays in exactly one place (game-side) — the formatting
code just moves wholesale from commands.rs into the game's API handler
layer, no duplication.

### 4b. Asynchronous, game-initiated (game tells the bot to say something, no request preceded it)

**Real finding, changes the design**: the ~264-line encounter-result
subscriber in main.rs (1049-1312) is NOT pure formatting — it also
**mutates game state**. The "first-fight Celestial Shard" and "launch
giveaway" blocks (1152-1248) read `result.summary.players`, pick a
winner, and call `adventure.grant_craft_token(...)` directly — a real
one-time game mechanic currently living in the BOT process. Under the
new design this entire subscriber moves to the GAME side, run at the
end of `run_encounter_inner`/`run_basic_encounter_inner`. Game produces
a list of fully-formatted chat strings; bot's announcement-consumer loop
relays each to `chat_client.say()` verbatim. This closes a real
architecture gap (a bot-side process was mutating core game state) as a
side effect of the split.

Every broadcast-subscriber block in main.rs collapses into "game pushes
a pre-formatted string to the announcements channel; bot's one consumer
loop relays it":

| Today (main.rs) | Lines | Becomes |
|---|---|---|
| Encounter-result subscriber (outcome/MVP/giveaways/loot/broken/retreated) | 1049-1312 (~264) | moves entirely to game-side, fires at the end of `run_encounter_inner`/`run_basic_encounter_inner`, pushes N formatted strings |
| Gear-crit subscriber | 1321-1341 | game pushes 1 formatted string wherever `gear_crit_tx` fires today |
| Rampage-complete subscriber | 1348-1356 | same, 1 fixed string |
| Unique-shard-win subscriber | 1363-1371 | same |

**Announcements channel mechanism — OWNER-CONFIRMED (§11 resolved)**:
Server-Sent Events (`GET /api/announcements/stream`), Axum's native SSE
support, no new dependency. A bounded channel with "drop oldest on lag"
(reusing the exact `encounter_tx`/`gear_crit_tx`/etc. broadcast-channel
pattern AdventureManager already has, fed to an SSE handler instead of
an in-process subscriber). Bot-down policy: **drop gracefully** — a
missed "so-and-so leveled up" message is low-stakes and self-heals on
the next chat activity, not a persistent queue.

### 4c. Failure-isolation policy — OWNER-CONFIRMED (§11 resolved)

| Direction | Policy |
|---|---|
| Game down, bot receives an adventure chat command | Fixed "the adventure is restarting, try again in a moment" reply |
| Game down, Reforge Gear / Repair All Gear redemption | REFUND silently (matches today's silent-confirmation-only tone) |
| Game down, Force Boss Fight redemption | REFUND + a chat line explaining why (always chat-announced today) |
| Bot down, game running standalone | Game runs fully — announcements drop gracefully per §4b, no persistent queue |
| Bot's `grant_activity_xp` call, game down/slow | Fire-and-forget — bot does NOT block/retry the chat loop |

### 4d. Shared-secret credential handling (settled 2026-08-19)

The `/api/*` seam's shared secret is a real credential from this point
on, not just a mechanism detail - specified explicitly since Stage 4
is what actually starts relying on it in both directions:

- **Where it lives**: one env var, `ADVENTURE_API_SECRET`, read from
  the repo-root `.env` file - the SAME file both binaries already read
  (`bot` via `src/config.rs`'s `env_var()`, `game` via its own
  `main.rs`'s identical `env_var()` helper - see that file's own doc:
  "Reads the SAME `.env` file the bot does"). No config file, no
  second copy to keep in sync while both binaries share one `.env`.
  Stage 6's per-binary config split (each binary gets its own env
  surface) must carry this key forward unchanged on BOTH sides when it
  happens - flagging now so that stage doesn't silently drop it.
- **How both binaries receive it**: automatically, today, by both
  reading the same file - no handshake, no provisioning step. Setting
  the key once and restarting both processes is the entire setup.
- **Never enters git or logs**: `.env` is already gitignored (verified
  - `.gitignore:2`). Neither `Config` (bot) nor the `game` binary's own
  env-read struct derives `Debug` or is ever logged wholesale, so this
  field is already exactly as safe as `TWITCH_CLIENT_SECRET`/
  `PLAYLIST_SYNC_SECRET`, which follow the identical pattern today.
  No request-logging middleware exists on either Axum server (checked
  directly, `grep`-confirmed empty) - the header itself is never
  written to any log. `require_shared_secret`'s own rejection log (see
  next bullet) deliberately logs a bool (whether a header was even
  present), never the presented value, wrong guesses included - a
  wrong guess still isn't safe to persist verbatim, and it adds
  nothing a human needs to fix the mismatch.
- **On mismatch: reject loudly** - `game/src/adventure_web/api.rs`'s
  `require_shared_secret` middleware now (2026-08-19) logs a
  `tracing::warn!` (request path, whether a header was present at all)
  on every rejected request, in addition to the 401 response - a
  silent 401 nobody notices was the actual gap here, not the rejection
  itself, which already existed since Stage 3. Detection is necessarily
  per-request - the two processes have no way to compare secrets except
  by a real request actually failing, there's no separate handshake to
  add. On the bot side, a 401 from this endpoint is indistinguishable
  from "game is down" using today's client alone (`AdventureApiClient`
  just returns an `Err` either way) - Stage 5's failure-isolation work,
  which already has to handle "game unreachable," covers a misconfigured
  secret as a side effect of the same code path. Worth remembering
  during Stage 5, not a gap to close separately.

---

## 5. Internal architecture (scoped inside `game/`)

adventure.rs's existing pattern — `mod X; pub use X::*;` flat glob-
reexport across all 10 submodules, each submodule doing `use super::*`
to see every sibling — stays the model for how the NEW domain/
persistence/web/ws/app submodules nest, so `crate::adventure::X`-style
paths keep resolving with zero edits to consumers, including wiki.rs.
This is the load-bearing assumption that makes moving low-risk files
before high-risk ones safe at all.

**5 buckets** (4 requested + `app/`):
- `domain/` — pure game logic, NO Tokio/Axum/IO. combat.rs (already
  100% pure), the pure parts of character/item/affix/craft.rs,
  passive_tree.rs, and the ~24 frozen wire/persistence struct/enum types
  currently defined in manager.rs (CombatEvent, PlayerFightStats,
  FightSummarySnapshot, EncounterResult, WorldState, BossKind, etc.) —
  **now ~25, `PlayerVitals` added 2026-08-18, see §12** — get their own
  explicit home here as pure data definitions.
- `persistence/` — fight_storage.rs, tunables.rs + balance.rs (TOML),
  migrations.rs, the generic load/save wrapper.
- `web/` — adventure_web.rs's 40+ routes split by feature, minijinja as
  the rendering layer (only 2 of ~42 render functions use it today —
  `base.html` shell + `characters.html`; ~40 raw-`format!()` pages
  remain the biggest migration surface). wiki.rs moves here intact,
  LAST, once shared helpers (top_nav, compute_passive_layout,
  root_node_html, passive_archetype_icon_role, current_session,
  escape_html, AppState) have stopped moving.
- `ws/` — adventure_overlay_server.rs's `handle_socket` + its 2 envelope
  types.
- `app/` — AdventureManager's shell: the struct, its `tokio::spawn`
  loops, its broadcast channels, Mutex-wrapped state. **Enforced one-way
  dependency**: domain/ and persistence/ must never import from app/ or
  web/ — add a cheap grep-based lint (`grep -rn "app::\|web::"
  src/game/domain/ src/game/persistence/` returning nothing) checked
  from the stage that introduces app/ onward.

**God functions / duplication flagged by the audit** (concrete work
items once the process split is stood up):
- combat.rs: `simulate_battle` (~3,000+ lines), `apply_hit` (~1,600+
  lines, now with the pierce split woven through the mitigation
  pipeline — see §12, this makes the eventual decomposition of
  `apply_hit`/`resolve_hit` slightly more delicate, not less, since
  pierce's split point is now load-bearing state threaded through the
  middle of the function, not a clean pre/post wrapper). Plus ~6
  duplication clusters (lazy-expiry buff multiplier shape, "kill this
  unit" boilerplate, mitigation-combine pattern, reflect-computation,
  two near-identical proc-roll functions, two independently-hand-kept
  boss-ability-cadence tables).
- manager.rs: `run_encounter_inner`, `craft_item_ex`,
  `run_basic_encounter_inner` (duplicates run_encounter_inner's loot/
  pity block almost verbatim), `new()` (331 lines, 8+ inlined migration
  blocks that should call the migrations.rs runner instead). Only 12
  tests in the whole file at Stage 0 time, ALL targeting free functions
  — zero coverage of AdventureManager's own async methods (now +11 more
  free-function tests since Stage 0 — `player_vitals_tests` — still zero
  async-method coverage, see §12).
- item/craft/affix.rs: the TOML-override-resolution pattern is
  copy-pasted 4 times near-identically — one shared helper in balance.rs
  replaces all 4. affix.rs and craft.rs currently have ZERO tests.
- adventure_web.rs: a batch-craft loop and a 5-step "Hideout Warrior"
  workflow chain are real game-workflow logic sitting in the web handler
  layer; admin-tunables-save does inline numeric clamping that belongs
  in a constructor/setter; a fight-history viewer-scoping filter bypasses
  AdventureManager and calls a free function directly; two separately-
  hardcoded admin-gate username constants should consolidate into one
  `is_admin()` helper.

---

## 6. Persistence surface (frozen)

Real production files on disk today (paths hardcoded, not
configurable):
- `adventure-characters.json` — `HashMap<String, Character>`, the main
  save. **Real player data — see §2.4/§9 for the fixture plan.**
- `adventure-world.json` — `WorldState`.
- `adventure-reforge-cooldown.json` — `HashMap<String, u64>`.
- `adventure-sessions.json` — `HashMap<String, Session>` (Session is
  defined IN adventure_web.rs, not src/adventure/* — worth folding into
  persistence/'s scope too).
- `adventure-live-tunables.toml` / `adventure-item-balance.toml` — TOML,
  sparse-override shapes.
- `adventure-rampage-state.json` — bare `u32`.
- Fight tiers (fight_storage.rs): `adventure-fights-coarse/` (cap 5,
  `LastFightSnapshot`), `adventure-fights-detail/` (cap 3,
  `DetailFightSnapshot`), `adventure-fights-summary/` (cap 200,
  `FightSummarySnapshot`), `adventure-fights-pinned/` (unpruned copies)
  + 3 `*-seq.json` counters.
- ~10 one-time migration marker files — already fired, their EXISTENCE
  is part of the on-disk contract; never delete.
- `adventure-last-fights.json`(`.bak`) — legacy, migrated away, present
  but dormant.
- `adventure-last-fight.json` (singular) — confirmed ORPHANED, zero code
  references anywhere. Dead data, safe to ignore.

**Migration pattern already established** (migrations.rs): a table of
`(marker_path: &str, transform_fn)` pairs; the runner checks each
marker, skips if present, else runs the transform, persists, writes the
marker. Any future breaking on-disk change follows this exact pattern.

---

## 7. Frozen wiki.rs constant-import list (exact)

Extracted from a full read of `wiki_placeholder_map()`. As long as these
keep resolving at their current `crate::adventure::X` (or
`crate::passive_tree::X`) paths — guaranteed by the flat-reexport-facade
principle in §5 — wiki.rs needs ZERO edits for any of these, regardless
of which internal submodule ends up defining them:

`TIER_CRAFT_DUST_COST, WEB_REFORGE_DUST_COST, VEIL_EXTRA_COST,
PERFECT_QUALITY_MULT, SACRED_STAGE_THRESHOLD, CELESTIAL_CONVERSION_PCT,
CTHULHU_DEBUFF_CADENCE_MS, CUBE_CAPTURE_CADENCE_MS, CUBE_CAPTURE_PCT,
CUBE_SHRED_PCT_PER_STACK, CUBE_SHRED_MAX_STACKS, CUBE_SHRED_DURATION_MS,
CUBE_SPLASH_MAX_TARGETS, DRAGON_SLOW_MULT, FIRE_DEMON_HEAL_MULT,
CTHULHU_DEBUFF_CAP, LICH_SUMMON_CADENCE_MS, LICH_ADDS_PER_SUMMON,
LICH_MAX_ADDS, RECOMBINE_CRIT_CHANCE, PANEL_REFORGE_DUST_PER_TIER,
POLISH_PERFECT_SAND_COST, POLISH_SAND_COST_PER_QUALITY_PCT,
RAMPAGE_VOTE_THRESHOLD, CRIT_BONUS_MULT, OVERCRIT_CURVE_A,
CRIT_CHANCE_CAP, BLOCK_DAMAGE_REDUCTION, LIFE_LEECH_CAP_PER_SEC,
PLAYER_SPLASH_MAX_TARGETS, ENEMY_SPLASH_MAX_TARGETS,
SPLASH_OVERFLOW_BONUS_TARGETS, HEAL_SPLASH_MAX_TARGETS,
ELEMENTAL_PROC_CHANCE_DIVISOR, ELEMENTAL_PROC_DURATION_MS,
ELEMENTAL_DEFENSE_FLOOR, ELEMENTAL_DEFENSE_CEILING,
ELEMENTAL_LIGHTNING_MAX_STACKS, ELEMENTAL_DIVINE_ENEMY_MAX_STACKS,
LINGERING_EFFECT_TICK_INTERVAL_MS, LINGERING_EFFECT_TICKS,
MAX_FIGHT_DURATION_MS, REVIVE_DURATION, ACTIVITY_XP_COOLDOWN,
ACTIVITY_XP_AMOUNT, RETREAT_REPAIR_DURATION, ARCHETYPE_CHANGE_COST,
PASSIVE_RESPEC_COST, MODEL_CHANGE_COST, MODEL_CHANGES_FREE_FOR_ALL,
WINGS_COST, INVENTORY_CAPACITY, ENCOUNTER_INTERVAL,
BASIC_ENCOUNTER_INTERVAL, FORCE_BOSS_MAX_PER_CYCLE,
RAMPAGE_ENCOUNTER_COUNT, RAMPAGE_MIN_INTERVAL, EQUIP_SLOTS,
POWER_ROLL_RANGE, base_power_for_slot(), ALL_AFFIXES, affix_weight(),
BOSS_ITEM_PITY_GAIN, BASIC_ITEM_PITY_GAIN, BOSS_CRAFT_PITY_GAIN,
BASIC_CRAFT_PITY_GAIN, reforge_crit_chance(), CraftAction::base_cost(),
ALL_ARCHETYPES, Character::xp_to_next_level(),
passive_tree::points_for_level()`.

**New since Stage 0 — needs a wiring decision, not yet made (see §12)**:
`PIERCE_CAP_PCT`/`PIERCE_H` are the first wired-placeholder pair backed
by a LiveTunable (admin-editable, re-read every fight) rather than a
compile-time constant. Every entry in the list above resolves via a
plain `crate::adventure::X` path; these two need a live-value read path
instead (e.g. through `AdventureManager::live_tunables()`). Flagging
here so whichever stage handles wiki.rs's move doesn't silently assume
every placeholder is constant-backed.

Non-adventure constants wiki.rs also reads (unaffected — bot-side,
untouched): `commands::BUILTIN_COOLDOWN`, `bug_reports::PER_USER_COOLDOWN`,
`song_requests::SKIP_ACTION_COOLDOWN`/`MIN_VOTE_VOLUME`/`MAX_VOTE_VOLUME`.
Once game/bot are separate crates, these become genuine cross-crate
reads — worth a specific check during the wiki.rs move stage.

---

## 8. Test infrastructure needed (built starting Stage 0.5)

- No `tests/` directory existed at Stage 0 time — everything was
  `#[cfg(test)] mod` unit tests in-file. `reqwest` is already a normal
  dependency — sufficient to drive integration tests against a real
  Axum server bound to `127.0.0.1:0` inside `#[tokio::test]`. No new
  dependencies needed.
- **3 reusable harnesses, built in Stage 0.5, before any code moves**:
  1. A seeded-battle golden corpus — run `simulate_battle` for a fixed
     set of deterministic seeded scenarios (varied archetype × boss ×
     level), snapshot the full `CombatEvent` log + final stats to
     fixture files. Reused by every combat.rs decomposition sub-stage
     and by manager.rs's `run_encounter_inner` decomposition. The
     safety net for the single highest silent-behavior-change risk in
     the whole project — `apply_hit`'s inlined passive-hook blocks
     (now including the pierce split, see §12) almost certainly have
     order-of-operations dependencies unit tests alone won't catch.
  2. A real-save-file round-trip fixture (the pseudonymized character
     file from §2.4/§9) — load under old code path, load under new code
     path, assert equality. Reused at every stage that touches
     Character/Item's shape or the migration runner.
  3. An HTTP golden-response harness (reqwest against a real ephemeral-
     port server) — status/headers/body, including param name/type
     checks. Reused for route-table-preservation verification and for
     the template-migration work.
- **Concurrency-specific risk**: once `app/`'s orchestration layer is
  touched, add a basic soak/smoke test — run the actual spawned loops
  for real wall-clock time against concurrent simulated requests,
  checking for panics/deadlocks/hangs.
- **Every stage's exit criteria**: `cargo build --workspace` /
  `cargo check --workspace --all-targets`, not just "the game binary
  builds" — a mistake in the shared module tree can silently fail
  compilation for `bot` even in stages that don't touch bot-side code.

---

## 9. Baseline/fixture capture plan (Stage 0 execution)

- **GET routes** (safe, read-only): captured directly against the
  already-running production instance.
- **POST routes** (`/craft`, `/join`, `/equip`, etc.): must NOT hit
  production. Captured against a SEPARATE local instance on an
  alternate port, seeded from the pseudonymized fixture (not the real
  file — simpler and strictly safer than the original plan's "copy the
  real files locally," since it means every artifact this stage
  produces is git-safe from the start with no separate real-vs-fixture
  data-handling distinction to maintain).
- **`/ws` sample**: push-only, safe to sample directly against the live
  production instance.
- **Real save-file fixture**: pseudonymized deterministically, script
  committed alongside it, nothing from the real file ever enters git
  history.

See `tests/fixtures/` for the generated artifacts and
`src/bin/pseudonymize_characters.rs` for the script.

**Execution log (2026-08-18, against `54f7c67`)**:
- GET-route baselines captured for `/`, `/inventory`, `/passives`,
  `/fights`, `/fights.json` against the live production instance
  (anonymous, no session cookie) - `/`, `/inventory`, `/passives`, and
  `/fights` all rendered byte-identical output for this logged-out
  visitor state (same "Adventure Character Dashboard" shell/landing
  content); `/fights.json` returned `[]` (no boss fight had landed
  since the process's most recent restart at capture time - boss
  encounters fire on a 10-minute timer with the first post-restart tick
  deliberately skipped, see `spawn_encounter_loop`'s own doc). Worth a
  human look during whichever future stage actually touches these
  routes: is the 4-way identical-shell result intentional (a shared
  logged-out empty state) or a route-gating bug nobody's noticed yet -
  not investigated further here since diagnosing it isn't a Stage 0.5
  harness-building task.
- A `/ws` sample captured (1 `type: "state"` broadcast frame, full
  character roster).
- **Not committed to git**: both contain real player logins/display
  names (the WS state broadcast especially - the full live roster).
  Kept in the session's local scratchpad instead, same "real player
  data never enters git history" principle as the pseudonymized
  character fixture - these are quick to recapture (`curl`/a short
  WebSocket client) against any live instance whenever a future stage
  actually needs to diff against them, so nothing is lost by not
  committing them.
- **Admin-authenticated (`lokati_gaming`) baseline capture**: still not
  done - needs the owner's help (a session cookie, or they capture it
  themselves) per §11's open item.
- **POST-route baselines against an isolated local instance**: not
  captured this pass. `start_adventure_web_server` needs a fully
  constructed `Arc<AdventureManager>`, which reads/writes several
  hardcoded file paths at the process's CWD (`adventure-characters.json`
  and friends - see §6) - there's no path-injection point today to
  safely point a second, disposable instance at copies/fixtures instead
  of the real files. Building that injection point is real production
  surface (touching `AdventureManager::new`'s construction, not just
  test scaffolding) and was judged out of scope for this pass rather
  than rushed - deferred to whichever Stage 8+ sub-stage first touches
  `AdventureManager::new`'s hardcoded paths, which was already an
  acknowledged gap in §6 before this note.
- **Harness #3 (HTTP golden-response harness) itself**: also deferred
  for the same reason - a genuinely reusable ephemeral-port harness
  needs that same path-injection point to run a disposable, isolated
  server instance repeatably in a test. The captures above stand in as
  the Stage 0 baseline reference in the meantime.

---

## 10. Proposed stage sequence

**Ordering principle**: get the standalone-game deliverable proven
EARLY using an incremental "strangler fig" approach — extract to a
library crate in-process first (zero behavior change), prove standalone
startup via a second entry point while the OLD in-process path keeps
running unchanged, build the new API seam ALONGSIDE the old path, cut
over, then remove the old path. The deep internal god-function
decomposition (combat.rs/manager.rs) is sequenced AFTER the split is
proven, not before.

- **Stage 0** (done): audit + seam map + this document.
- **Stage 0.5** (done): build the 3 reusable test harnesses (§8) before
  any code moves.
- **Stage 1** (done, 2026-08-18, two commits): introduced the Cargo
  workspace shape — `game` as a LIBRARY crate (not yet its own binary),
  `twitch-bot-rs` depends on it, calls it exactly as today, in-process.
  **Scope came out narrower than originally written here, for a real
  reason found mid-execution, not a shortcut**: only `adventure.rs`+
  `adventure/*`+`passive_tree.rs`+`state.rs` moved — NOT `adventure_web.rs`/
  `adventure_overlay_server.rs` (those are Stage 2's own move, per that
  stage's existing wording below, so this was already the plan, just
  confirmed against the dependency graph rather than assumed). A full
  audit found this trio's only external coupling is `state.rs` (a tiny
  generic JSON helper, zero bot-specific logic) — so `twitch-bot-rs`'s
  `lib.rs` re-exports all three under their ORIGINAL names
  (`pub use game::adventure;` etc.) instead of declaring them as its
  own modules, and every existing `crate::adventure::X`/
  `crate::passive_tree::X`/`crate::state::X` reference anywhere in the
  bot codebase - including wiki.rs, untouched per CLAUDE.md - keeps
  resolving with zero edits. ~64 items widened `pub(crate)` → `pub`
  (the real, unavoidable cost of an actual crate boundary vs. the
  internal-module reorganization this section originally assumed before
  the standalone-game addendum) - purely visibility, zero behavior
  change. Also fixed a real bug the move itself introduced: `cargo
  test`'s working directory is the PACKAGE root, not the workspace
  root, so `golden_corpus.rs`'s fixtures needed to move to
  `game/tests/fixtures/` (from the repo-root `tests/`) to keep being
  found — caught because a "compiles ⇒ still correct" assumption would
  have missed it; only re-running the harness surfaced it silently
  writing fresh "first capture" baselines instead of comparing against
  the committed ones.
  **A significant finding for Stage 2 — RULED ON by the owner,
  2026-08-18, see below**: wiki.rs reads BOTH `crate::adventure::X` AND
  bot-side constants (`crate::commands::BUILTIN_COOLDOWN`,
  `crate::bug_reports::PER_USER_COOLDOWN`, `crate::song_requests::X` -
  see §7) in the SAME file. Since `game` can't depend back on `bot`
  (that's the whole point of the split), wiki.rs can't move into `game`
  while it still reads bot-side constants directly.

  **Owner's ruling**: wiki.rs goes GAME-side, full stop - the
  standalone deliverable explicitly includes the game serving its full
  web UI (wiki included) with no bot process running at all, so wiki.rs
  staying bot-side would violate that core requirement over three
  trivial cooldown/volume constants. The dependency inverts instead:
  `BUILTIN_COOLDOWN`/`PER_USER_COOLDOWN`/`SKIP_ACTION_COOLDOWN`/
  `MIN_VOTE_VOLUME`/`MAX_VOTE_VOLUME` become "published constants" the
  BOT supplies to the GAME across the API seam (§4) - mechanism left to
  whoever designs Stage 3 (a small POST at bot startup, or a shared
  config file, are both reasonable), with a graceful fallback for
  wiki.rs to render when the bot has never connected (render "varies,"
  or hold the last-known value) - a slightly stale chat cooldown shown
  on a wiki page is an acceptable cost; the wiki depending on the bot
  process being up is not. **Fold this into Stage 3's own seam design
  explicitly** (a small bot→game publish, alongside the chat-command/
  redemption/announcements seam §4 already covers) - noted again at
  Stage 3's own bullet below.

  **Open sequencing question, not resolved here either**: Stage 2 (next)
  is what physically moves `adventure_web.rs` - wiki.rs's literal
  parent module - into `game`, but the published-constants mechanism
  that replaces its bot-side reads is designed as part of Stage 3,
  which comes AFTER Stage 2 in the sequence below. Whoever executes
  Stage 2 needs to actually resolve this ordering - either pull just
  this narrow publish-mechanism piece forward (build it before/within
  Stage 2, ahead of the rest of Stage 3's seam), or resequence so
  wiki.rs's own move waits until that piece exists. Don't silently pick
  one without noting it here; decide when Stage 2 actually reaches the
  wiki.rs move, same "flag it, don't let it slip" spirit as the
  `apply_hit` gate above.

  A WIKI_IMPACT.md line is owed once this actually ships (not yet - no
  code has changed for this decision yet, only the design) - the wiki
  session should know these 5 placeholders' data source changed from a
  direct compile-time-ish read to a published, sometimes-stale value.
  **Configurable persistence paths** (owner-directed, 2026-08-18): done
  as its own commit — `game::adventure::set_data_dir(PathBuf)` +
  `paths::data_path`, threaded through every persisted file
  (fight_storage.rs/manager.rs/tunables.rs/balance.rs/migrations.rs),
  deliberately NOT added to `state.rs` itself (shared with unrelated
  bot-side files - see that module's own doc). Never calling
  `set_data_dir` (true today) resolves byte-identical to the bare
  literal it replaces - confirmed both by inspection and a real,
  written-then-removed functional smoke test (see `paths.rs`'s own doc
  for why the automated version couldn't safely stay in the suite -
  a genuine cross-test race with item generation, not a flaw in the
  mechanism itself). Unblocks Stage 1.5 below.
- **Stage 1.5** (done, 2026-08-18): built harness #3
  (`tests/http_golden_responses.rs`) - a real, disposable
  `AdventureManager`/`adventure_web` Axum server on an OS-assigned
  ephemeral port, seeded from the pseudonymized fixture via Stage 1's
  `set_data_dir`. `start_adventure_web_server` gained a real return
  value (the bound `SocketAddr`, was `()`) so an ephemeral-port caller
  can actually reach it - `main.rs`'s own call site needed zero changes.
  Covers the same GET routes Stage 0 captured against live production
  plus one representative authenticated POST (`/join`, verified with a
  real state-change check through the manager, not just a status code).
  Not yet exhaustive across all ~40 routes/every POST action (`/craft`,
  `/equip`, etc.) - scoped to proving the mechanism and the highest-
  value routes rather than full coverage in one pass; extending it to
  more routes as later stages touch them is straightforward from here,
  the hard part (a working disposable instance) is done.
- **Stage 2** (done, 2026-08-18, four commits): `game` got its OWN
  `main()` (`game/src/main.rs`) that starts the adventure_web +
  adventure_overlay servers standalone, zero Twitch dependency.
  Resolved the wiki.rs crate-placement question first (owner's ruling:
  game-side, full stop - a small bot→game published-constants file for
  the 5 formerly-direct bot-side reads, graceful "varies" fallback -
  see WIKI_IMPACT.md), then moved adventure_web.rs (+ render.rs/wiki.rs)
  and adventure_overlay_server.rs into `game` intact - notably a MUCH
  smaller diff than Stage 1's move (these files' own `crate::adventure::X`
  references needed zero changes once they were actually inside `game`;
  only new external-crate dependencies were needed, no `pub(crate)`→`pub`
  sweep). Caught the same "cargo test's CWD is the package root" class
  of bug Stage 1 hit, this time in `render.rs`'s template loader - fixed
  with a `#[cfg(test)]`-only path override, production behavior
  untouched. **Smoke-test criterion met and LIVE-VERIFIED, not just
  built**: ran the actual compiled standalone binary (no bot process
  running at all) against isolated scratch data on non-conflicting
  ports - `/`, `/wiki`, `/wiki/combat`, `/wiki/commands`, and the
  overlay's own index all returned 200, and the published-constants
  fallback was confirmed rendering "varies" under the exact condition
  it exists for (no bot ever ran against that scratch instance). Not
  merged, not deployed - exists on `refactor/architecture` only;
  `twitch-bot-rs`'s own in-process startup is completely unchanged and
  still the only thing actually running in production.
- **Stage 3** (done, 2026-08-19): build the API seam per §4 (game-side `/api/`
  endpoints returning pre-formatted reply strings; the SSE announcements
  stream; the bot-side HTTP client module) ALONGSIDE the existing direct
  in-process calls. **Includes the small bot→game published-constants
  publish** (owner's ruling, 2026-08-18, see Stage 2's own note above) -
  `BUILTIN_COOLDOWN`/`PER_USER_COOLDOWN`/`SKIP_ACTION_COOLDOWN`/
  `MIN_VOTE_VOLUME`/`MAX_VOTE_VOLUME`, with a graceful stale/"varies"
  fallback on the game side for whenever the bot hasn't connected yet -
  this piece landed already, in Stage 2 (see Stage 2's own note above),
  since the wiki.rs move needed it immediately.
  **§4b groundwork done (2026-08-19)**: `AdventureManager` gained an
  `announcements_tx: broadcast::Sender<String>` (subscribe via
  `subscribe_announcements()`), fed by a new pure-formatting module
  (`game/src/adventure/announcements.rs` - `format_encounter_outcome`,
  `format_loot_line`, `format_gear_crit`, `format_unique_shard_win`,
  `RAMPAGE_COMPLETE_MESSAGE`; 9 unit tests, the first tests this
  previously closure-embedded logic has ever had) plus a new
  `announce_encounter_result` async method on the manager that ports
  the Celestial-Shard-first-award and item-launch-giveaway state
  mutation (previously living in main.rs's subscriber closure - see
  §4b's own "real finding" note) using `data_path()` for its marker
  files, a deliberate, documented behavior change now that this state
  is genuinely game-owned. Wired at all the real send sites: the two
  `encounter_tx.send(result)` call sites (`run_encounter_inner`,
  `run_basic_encounter_inner`) now call `announce_encounter_result`
  first; the pre-existing single `announce_gear_crit` hook (shared by
  reforge's 1% and recombine's 5% crit rolls) now also pushes a
  formatted announcement alongside its existing `gear_crit_tx` send;
  `rampage_complete_tx`'s one call site and `unique_shard_tx`'s four
  scattered call sites (neither had an existing centralizing wrapper,
  unlike gear-crit) now go through two new thin wrapper methods,
  `announce_rampage_complete`/`announce_unique_shard_win`, introduced
  specifically to give them the same single-hook-point shape gear-crit
  already had. Workspace compiles clean; full suite at 143 passing (the
  134 pre-Stage-3 baseline + these 9 new tests), zero regressions. This
  is all still "new, parallel code" per the stage's own scoping rule -
  main.rs's own 4 broadcast subscribers are completely untouched and
  remain the only thing actually announcing in production.
  **`/api/*` HTTP layer done (2026-08-19)**: built
  `game/src/adventure_web/api.rs`, nested onto the SAME Axum server
  `start_adventure_web_server` already runs (`.nest("/api", ...)`) - per
  §3's own already-ratified text ("a real API on the game's existing
  Axum server"), NOT a separate port. Resolves the port/bind-separation
  question flagged as possibly needing to be raised - re-reading §3
  found it was already decided, not an open question, so no owner
  check-in was needed there. Security mechanism: a shared-secret header
  (`x-adventure-api-secret`), not a peer-IP/localhost check - my own
  call between the plan's two pre-authorized options, since this router
  shares a port with the public, reverse-proxied dashboard, where a
  peer-IP check would see every proxied request as loopback-adjacent
  and couldn't tell "the bot" apart from "the public internet" the way
  it could on a genuinely separate, unproxied bind. The secret is a new
  `Option<String>` config field (`ADVENTURE_API_SECRET`, both
  `src/config.rs` and `game/src/main.rs`'s own env read) - `None` (the
  default, unchanged in production) skips mounting `/api/*` at all, so
  today's real deploy gets the EXACT route table it had before this
  landed. All ~12 §4a/§4c rows are implemented (10 command endpoints +
  3 redemption endpoints + fire-and-forget activity-XP, the last of
  which now also pushes its level-up line onto `announcements_tx`
  itself - a genuinely new one-line addition to `grant_activity_xp`,
  harmless in production today since nothing subscribes yet) plus the
  `GET /api/announcements/stream` SSE handler (`tokio_stream`'s already-
  dependency `BroadcastStream`, no new crate). Every handler is a
  byte-for-byte port of its commands.rs/main.rs original - permission
  gating (`is_mod_or_broadcaster`) deliberately stays OUT of this
  module, since only the bot knows Twitch roles; the one endpoint whose
  BEHAVIOR (not just permission) depends on role (`rampage`) takes that
  role as an explicit request field instead. Mirror bot-side client
  built at `src/adventure_client.rs` (`AdventureApiClient`, one method
  per endpoint, plus a hand-rolled minimal SSE line-parser for
  `announcements()` rather than pulling in an SSE-client crate - this
  whole module is test-only for now) - reqwest gained the `"stream"`
  Cargo feature for this, no new dependency. **New end-to-end harness**
  `tests/api_seam.rs`: a disposable game instance, real HTTP calls
  through `AdventureApiClient` covering every endpoint, INCLUDING one
  real round trip through §4b (Force Boss Fight redemption -> a genuine
  boss fight actually runs -> its outcome is read back off the live SSE
  stream) - proves the whole seam works together, not just that each
  half compiles. Full workspace suite: 144 passing (was 143), zero
  regressions. Still "new, parallel code" throughout - nothing in
  src/main.rs or src/commands.rs calls through this seam yet.
  **Still open for this stage**: actually wiring `commands.rs`'s
  dispatch and main.rs's redemption handlers to CALL through
  `AdventureApiClient` when the seam is live is explicitly Stage 4's
  job, not this one's - this stage only had to prove the seam works,
  alongside the untouched original path.
- **Stage 4** (done, 2026-08-19): cut over — `bot` switches to the
  HTTP-client seam, stops starting AdventureManager/adventure_web/
  adventure_overlay in-process at all. The actual "two separate
  processes" moment, code-complete on `refactor/architecture` - still
  not merged, not deployed; production stays on the pre-cutover
  in-process build until the LIVE BAKE below and the final merge.
  `commands.rs`'s `Services.adventure` is now `Arc<AdventureApiClient>`;
  every §4a command handler routes through one new `adventure_reply()`
  helper (`Ok(Some) -> that text`, `Ok(None) -> Reply::None`,
  `Err -> the fixed §4c "restarting" line`) instead of matching a
  manager enum directly. The 3 redemption handlers and
  `reconcile_missed_redemptions` take `&AdventureApiClient` the same
  way; `handle_reforge_redemption`/`handle_repair_redemption` REFUND
  silently on a client `Err` (matching §4c's shared row for the two),
  `handle_force_boss_redemption` REFUNDs + chat-announces (gated on
  `announce`, matching its own already-always-announced tone).
  Activity XP is genuinely fire-and-forget now (`tokio::spawn`, not
  awaited inline in the chat-message loop) - the level-up line itself
  moved server-side (`grant_activity_xp` pushes its own announcement).
  **Second real finding, same shape as Stage 3's Celestial Shard one**:
  a bot-side one-time "Wings of Flight" startup giveaway (main.rs) that
  §0's audit missed - real game-state mutation living bot-side, moved
  into `game/src/main.rs`'s own startup (`grant_random_wings` now
  self-announces, matching `grant_activity_xp`'s pattern).
  **A real regression, caught by actually wiring the seam rather than
  just building it in isolation**: Stage 3's port of the encounter-result
  announcement dropped the `700ms + display_duration_ms` delay
  main.rs's old subscriber applied before ever announcing (so chat
  wouldn't spoil a fight before the overlay's own charge-in/replay
  caught up) - invisible while nothing consumed the SSE stream, surfaced
  the moment `tests/api_seam.rs` exercised a real round trip end-to-end.
  Fixed: the announcement is now pushed from a delayed spawned task
  (cloning `EncounterResult`, since `encounter_tx`'s own broadcast to
  the overlay must stay immediate - only the chat text waits) at both
  real call sites. `combat::MIN_DISPLAY_MS` (6s) means this delay is
  substantial, not a rounding error - confirmed by the test actually
  needing a 10s timeout once the fix landed.
  Config: `adventure_api_secret` is now REQUIRED (`String`, not
  `Option`) on the bot side - the adventure game is always-on, so an
  unset secret would leave every adventure command silently broken;
  `Config::load()` now fails fast at startup instead, matching
  TWITCH_CLIENT_ID's own treatment. New `adventure_api_base_url`
  (default `http://127.0.0.1:4005`, matching `game`'s own
  ADVENTURE_WEB_PORT default) tells the bot where to reach it. See §4d
  for the full credential-handling writeup (where it lives, why it's
  safe, what happens on mismatch - includes a fix landed alongside this
  stage: `require_shared_secret` now logs a `tracing::warn!` on every
  rejected request, not just the 401 itself).
  **Deliberately NOT touched**: `adventure_web_port`/
  `adventure_web_public_url`/`adventure_overlay_server_port` stay
  declared (now dead) in the bot's `Config` - removing them is Stage 6's
  already-scoped "split config.rs into per-binary" job, not this one's;
  touching them now risked pre-empting how that stage wants to shape
  the eventual split.
  **Smoke matrix** (owner-required for this stage specifically):
  - *Game alone*: live-verified again, same as Stage 2 - ran the actual
    compiled `game.exe` standalone (scratch `GAME_DATA_DIR`, no bot
    process anywhere) and drove its real `/api/*` endpoints with `curl`
    (not just the test harness): wrong/missing secret both 401 (and
    both logged - confirmed the loud-rejection fix live), correct
    secret's `!join`/`!character`/`!party` all returned the exact
    expected formatted strings.
  - *Bot+game together*: `tests/api_seam.rs` IS this check at the
    `AdventureApiClient` level (every §4a/§4c endpoint, over real HTTP,
    against a real disposable game instance) - extended this stage with
    the killed-game case below. The curl session above is the same
    proof at the wire level, standing in for the bot's HTTP client.
  - *Bot against a killed game*: `tests/api_seam.rs` now also binds a
    fresh ephemeral listener and immediately drops it (guaranteed
    connection-refused, the same failure shape a real killed `game`
    process leaves), then confirms `AdventureApiClient` fails FAST with
    a real `Err` - no panic, no hang past a 5s bound. Paired with 3 new
    unit tests on `adventure_reply` itself (src/commands.rs) proving
    that `Err` maps to the exact ratified §4c fallback text. Together
    these cover the two halves of "bot against a killed game": the
    network call fails cleanly, and the fallback text is right.
  - **Honest gap, flagged rather than silently skipped**: neither check
    exercises `handle_command`/`handle_builtin` calling all the way
    through to a real `chat_client.say()` - `chat::ChatClient` has no
    test-friendly constructor (its only path to an instance is a real
    Twitch IRC connection via `chat::connect`), and actually starting
    the bot against live Twitch to prove this was ruled out deliberately
    (a real IRC connection + possible real chat/EventSub side effects,
    not something to trigger just to verify a refactor). What's proven
    covers the actual NEW risk this cutover introduced (the network call
    can now fail, and the fallback logic that handles it); the
    dispatch-to-chat-message wiring itself is unchanged from before this
    stage and carries no new risk from it.
  Full workspace suite: 147 passing (was 144), zero regressions.
- **Stage 5** (done, 2026-08-19): failure-isolation behavior (§4c) + per-
  reward-type refund policy, tested against a deliberately-killed game
  process. §4c's policies were already IMPLEMENTED at Stage 4 (the code
  had to handle the `Err` case somehow to compile at all); this stage is
  about hardening and genuinely testing that, plus two real crash-safety
  gaps this cutover exposed:
  - **Redemption decision logic factored into pure functions**
    (`reforge_redemption_action`/`repair_redemption_action`/
    `force_boss_redemption_action` in main.rs) - separates "what status/
    chat-message does this outcome produce" from the actual
    `helix.update_redemption_status`/`chat_client.say` I/O, mirroring
    `adventure_reply`'s Stage 4 split. 8 new unit tests cover every row
    of §4c's redemption table directly (reforge/repair refund silently
    on game-down; force-boss refunds + announces when live, stays quiet
    replaying a backlog) without needing the real `HelixClient`/
    `ChatClient` neither has a test-friendly constructor for (see Stage
    4's "honest gap" note - still applies, this is how it's worked
    around for the parts that CAN be pure functions).
  - **`game/tests/killed_process_smoke.rs`** (new) - a genuinely killed
    process, not Stage 4's synthetic dead-port stand-in: spawns the
    REAL compiled `game` binary as a child process (`env!("CARGO_BIN_EXE_game")`,
    which is why this test lives in `game/tests/` and not the bot
    crate's), confirms it answers normally, hard-kills it
    (`Child::kill()` - `TerminateProcess` on Windows, not a graceful
    shutdown), confirms the wire-level failure is clean, then confirms a
    FRESH process on the same port/data-dir recovers and serves
    normally again - the game-side half of "bot down, game restarts,
    everything just works."
  - **"Bot down, announcements drop gracefully" direction**: verified
    by code inspection rather than a dedicated test - every one of the 7
    `announcements_tx.send(...)` call sites (grep-confirmed) discards
    the `Result` via `let _ = ...`, and `tokio::sync::broadcast`'s own
    documented behavior for zero receivers is an immediate `Err` with
    the value simply dropped, never buffered - the same guarantee 4
    other broadcast channels in this file (`encounter_tx`, `gear_crit_tx`,
    `rampage_complete_tx`, `unique_shard_tx`) already relied on before
    this refactor touched any of it. A dedicated runtime test here would
    mostly be re-proving a Tokio library guarantee already exercised
    elsewhere in this exact codebase, not new risk this cutover added.
  - **Two real crash-safety gaps found and fixed, not just documented**:
    (1) `game/src/main.rs` was still on plain `#[tokio::main]` with
    Tokio's 2MiB default worker stack size - the bot's own `main.rs`
    abandoned that specifically because `simulate_battle`/`apply_hit`
    caused repeat real `STATUS_STACK_OVERFLOW` crashes (see
    watchdog.ps1's own doc). That fix was never mirrored here because it
    never needed to be: Stage 2's live-verification never ran a real
    fight, and Stages 1-4 only ever exercised this binary in a
    foreground terminal. Stage 4 changed that - `game` is now the ONLY
    process that ever calls `run_encounter_inner` for real once a bot is
    pointed at it, so shipping this into a real-traffic bake without the
    same 32MB stack fix would have risked reproducing the exact crash
    class that motivated building a dedicated watchdog in the first
    place. Fixed to match `bot`'s pattern exactly. (2) `game`'s own
    logging was stdout-only (`tracing_subscriber::fmt()`, no file
    appender) - invisible the moment this runs headless under a
    Scheduled Task with no attached console, same problem `bot`'s
    `main.rs` already solved for itself. Added the identical
    `tracing-appender` daily-rolling-file setup (`logs/game.log.<date>`,
    new Cargo.toml dependency, no other bot-side changes needed).
  Full workspace suite: 156 passing (was 147), zero regressions.
- **LIVE BAKE** (owner-required, inserted here): run the two-process
  shape in production for at least a day of real stream traffic — real
  fights, real redemptions, a real bot-restart under it — before
  proceeding to Stage 6+. **Deployment plan for this proposed separately
  (not in this document) as a stop-and-wait for owner approval** -
  covers exactly what changes on the machine, the rollback procedure,
  and what gets watched during the bake day. Two live findings from that
  investigation, unrelated to this refactor but directly relevant to
  trusting the bake: the existing `TwitchBotRS-Watchdog` scheduled
  task's action points at a path that no longer exists
  (`C:\Users\Administrator\Downloads\twitch-bot-rs\watchdog.ps1`) and has
  been silently failing every ~2 minutes since roughly 2026-08-18 13:00 -
  the bot's crash auto-recovery has had no confirmed-working safety net
  for over half a day, independent of anything in this refactor. Flagged
  to the owner directly, not fixed unilaterally (touches live Task
  Scheduler configuration) - recommended as a bake prerequisite either
  way.
- **Stage 6**: split `bot`'s Cargo.toml/config.rs into per-binary
  config; formalize the workspace's final `[[bin]]`/crate layout.
- **Stage 7**: two-binary deploy/watchdog/scheduled-task updates —
  CLAUDE.md's deploy procedure gains a second artifact, staying a single
  command for the owner.
- **Stages 8+**: internal 4(+1)-bucket layering INSIDE the `game` crate
  — lowest-risk files first (fight_storage.rs → tunables.rs+balance.rs →
  migrations.rs → passive_tree.rs → affix.rs+craft.rs → item.rs →
  character.rs, verified against the pseudonymized fixture), then the
  explicit type-ownership stage for the frozen wire/persistence types
  (moving definitions out of manager.rs into domain/), then combat.rs
  (moved as-is first, then decomposed using the Stage 0.5 golden corpus
  — player-unit construction, enemy/boss construction, boss-ability
  match arms + cadence-table unification, `apply_hit`'s inlined blocks
  last as the highest-risk sub-stage — **now including the pierce
  split's own load-bearing position mid-pipeline, see §12**. **Gate,
  owner-directed 2026-08-18: this specific sub-stage must NOT begin
  until either (a) the golden corpus gains real multi-player scenario
  coverage (Intervene, party-heal-lowest-ally targeting, Pack Instinct/
  Symbiosis, curse-splitting across several targets — all explicitly
  out of scope at Stage 0.5, see `golden_corpus.rs`'s own doc) or (b)
  the owner explicitly accepts running `apply_hit`'s decomposition
  without that coverage. Decide this when the stage actually
  approaches — don't let it slip through silently just because Stage
  0.5's solo-only corpus technically exists and looks like "coverage."**),
  then
  `app/` (moved as-is, characterization tests added for the previously-
  untested async methods BEFORE decomposing them, then the god-function
  decomposition + loot/pity duplication unification), then `ws/`, then
  `web/` (route split, then the minijinja migration, `/` dashboard and
  `/inventory` last), then wiki.rs intact as its own stage.
- **Final stage**: rebase onto latest master at the START of every stage
  above (cheap insurance against conflicts with a parallel session),
  full verification pass, owner review, push-inclusive deploy with
  extra smoke checks for both binaries.

---

## 11. Open items — resolutions

- ~~Admin-page baseline capture needs an authenticated session~~ — best-
  effort logged-out baselines captured at Stage 0 execution time; the
  owner will capture the authenticated (`lokati_gaming`) baseline
  themselves and hand over the file (2026-08-18) — still open until
  that file arrives.
- ~~Announcements-channel transport~~ — **RESOLVED**: SSE + drop-
  gracefully, approved as proposed.
- ~~§4c failure policies~~ — **RESOLVED**: approved as proposed (see §4c
  table above).
- Whether `Session` (adventure_web.rs) formally becomes part of
  persistence/'s scope — leaning yes, not finalized.
- The fight-history viewer-scoping filter that bypasses AdventureManager
  today (§5) — decide explicitly whether it becomes a real manager
  method or a relocated direct call, when that stage is reached.

---

## 12. Changes landed on master between Stage 0 approval and Stage 0.5

Per the owner's explicit sequencing ("last master-side changes before
the freeze holds"), master moved **3 times** after Stage 0 was approved
and before Stage 1 begins — noted here for the future branch-rebase
step, and because two of the three touch the frozen-surface inventory
this plan documents elsewhere:

1. **Boss pierce-through** (`6919f0c`, later fixed `54f7c67`) — a new
   combat mechanic, not a refactor concern in itself, but its
   implementation sits INSIDE `apply_hit`/`resolve_hit` — the exact
   functions §5 flags as the highest-risk decomposition target. The
   pierce split is now a real mid-pipeline dependency (split after the
   attacker-side roll, before evasion/block/DR), not a clean wrapper —
   worth extra care (and probably its own characterization test, beyond
   the Stage 0.5 golden corpus) whenever `apply_hit`/`resolve_hit`
   decomposition is actually reached in Stage 8+.
2. **Overlay rendering/settings + `playerVitals`** (`33bedaf`) —
   `EncounterResult` (a frozen wire type, §7/§6 territory) gained
   `player_vitals: Vec<PlayerVitals>` via `#[serde(default)]` —
   additive, old files still deserialize, the freeze's purpose is
   honored. `PlayerVitals` should be counted among the frozen wire/
   persistence types when §5's domain/ type-ownership stage is reached
   (now ~25 types, not ~24). The website settings tray and multi-canvas
   overlay changes are web/ws-layer only, no frozen-surface impact.
3. **Boss pierce split-point bugfix** (`54f7c67`) — the pierce mechanic
   from (1) initially split the wrong quantity (pre-roll base damage
   instead of the fully-rolled hit), fixed same-day before any Stage 0
   baseline was captured. No formula/behavior-surface change beyond
   fixing the bug itself — baselines captured after this commit
   correctly reflect the intended-strength mechanic.

Baselines and fixtures captured for Stage 0 execution are captured
against `54f7c67` (or later) specifically so they record this corrected
world, per the owner's explicit ordering.
