# Path of Dust — Architecture Refactor Plan

**Status**: Stage 0 (audit + seam map) complete and approved. Stage 0
execution (baseline/fixture capture) and Stage 0.5 (reusable test
harnesses) in progress on branch `refactor/architecture`. This document
is the durable record of the plan — written to the repo root so a fresh
session can resume the project with full context, without needing the
original Plan-mode transcript.

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
- **Stage 0.5** (in progress): build the 3 reusable test harnesses (§8)
  before any code moves.
- **Stage 1**: introduce the Cargo workspace shape — `game` as a
  LIBRARY crate (not yet its own binary) containing the moved adventure
  module; `twitch-bot-rs` depends on it, calls it exactly as today,
  in-process. Mechanical move + path updates only, zero behavior change.
- **Stage 2**: give `game` its OWN `main()` (a second binary) that can
  start the adventure_web + adventure_overlay servers standalone, zero
  Twitch dependency. Smoke-test: game-only startup, full web UI works
  with no bot process running. `twitch-bot-rs` still ALSO starts
  everything in-process (dual-mode transitional state).
- **Stage 3**: build the API seam per §4 (game-side `/api/` endpoints
  returning pre-formatted reply strings; the SSE announcements stream;
  the bot-side HTTP client module) ALONGSIDE the existing direct
  in-process calls.
- **Stage 4**: cut over — `bot` switches to the HTTP-client seam, stops
  starting AdventureManager/adventure_web/adventure_overlay in-process
  at all. The actual "two separate processes" moment. Full smoke test
  both directions.
- **Stage 5**: failure-isolation behavior (§4c) + per-reward-type refund
  policy, tested against a deliberately-killed game process.
- **LIVE BAKE** (owner-required, inserted here): run the two-process
  shape in production for at least a day of real stream traffic — real
  fights, real redemptions, a real bot-restart under it — before
  proceeding to Stage 6+.
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
  split's own load-bearing position mid-pipeline, see §12**), then
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
  effort logged-out baselines captured at Stage 0 execution time; an
  authenticated (`lokati_gaming`) capture needs the owner's help
  (session cookie, or they capture it themselves) — still open, ask when
  convenient.
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
