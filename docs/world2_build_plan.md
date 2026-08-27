# Path of Dust — World 2 Build Plan

**Status:** Authoritative plan. Supersedes earlier ad-hoc scoping.
**Date:** 2026-08-27

**Evidence base — three completed audits, all read-only, all pushed:**

| Audit | Branch | Commit |
|---|---|---|
| Bot decoupling | `docs/bot-decoupling-audit` | `f268dd5` |
| External integration removal scope | `docs/twitch-removal-scope` | `bdf8c39` |
| Platform portability | `docs/platform-portability-audit` | `3bc5e9f` |

Read those before executing any stage below. This document is the plan; they are the ground truth.

---

## 1. What World 2 is

World 1 retires. World 2 is not a second world — it is **the** world, rebuilt as a standalone game:

- Running on Linux, not Windows
- With no Twitch integration of any kind
- With no Patreon integration
- With its own login, not Twitch OAuth
- Fresh characters, no migration
- On current balance; content divergence is a separate project

The bot does not migrate. It stays on the Windows machine with OBS, keeps its Twitch and overlay duties, and stops talking to the game entirely.

---

## 2. Settled decisions

These are rulings, not open questions. Do not relitigate them inside a work session.

| Decision | Ruling |
|---|---|
| Characters | Fresh. No migration from World 1. |
| Twitch | Removed entirely — chat commands, redemptions, chat XP, OAuth. |
| Patreon | Removed entirely. Grants nothing in-game today; pure deletion. |
| The bot | Stays on Windows. Keeps OBS and Twitch. No link to the game. |
| Host | netcup VPS Lite 3 G12s — 8 vCPU, 16 GB, 320 GB, €11.67/mo. |
| Billing | **Monthly, not annual.** Convert only after a month of proven operation. |
| OS | Ubuntu LTS, distribution only. No control panel image. |
| Ingress | Cloudflare Tunnel, as today. TLS terminates at Cloudflare's edge. |
| Inbound ports | **None.** `cloudflared` dials outbound. |
| What migrates | The game only. |
| Registration | **Open.** Not invite-gated. |

### Why netcup and not OVH

OVH was the earlier recommendation, on the strength of included anti-DDoS and US datacenters. The portability audit proved ingress runs through a Cloudflare Tunnel, which terminates TLS at the edge and absorbs attacks before they reach the origin — and players connect to Cloudflare's network rather than to the origin directly. Both OVH advantages therefore stop applying. What remains is hardware per euro, and netcup wins it: 8 vCPU / 16 GB / 320 GB against 6 / 12 / 100 for slightly more money.

### Why no reverse proxy is needed

TLS terminates at Cloudflare. No TLS listener exists in the code and none is needed. Caddy, nginx and Let's Encrypt are all unnecessary.

---

## 3. The sequencing principle

**Remove the integrations before moving to Linux.** This is not a preference; it avoids inventing work.

The portability audit flagged that once the game moves to a VPS, the bot→game `/api/*` seam becomes a cross-internet link carrying a shared secret in plaintext, requiring `ADVENTURE_API_BASE_URL` to migrate to HTTPS. That work is entirely avoidable: the seam is being deleted. Remove first, and the problem never exists.

Second constraint: **identity comes before any removal.** Removing Twitch OAuth without a replacement produces the session cliff described in section 6. Nothing ships before login exists.

---

## 4. Stages

Each stage is independently shippable and independently testable. Each gets its own worktree, its own session, and its own deploy.

### Stage 1 — Local identity ✅ **SHIPPED 2026-08-27** (`3ef0651`, binary `71F52483…`)

Give the game its own session minter. Username and password, argon2id, minimal.

The removal-scope audit established this is far cheaper than assumed: nine existing `*_http.rs` tests already drive the authenticated site with a hand-seeded `adv_session` cookie, proving the session mechanism never depended on Twitch. Twitch is a session minter, nothing more. The replacement is one `/login` handler with **no change** to the character model, save format, or session mechanism.

**Adds a minter. Removes nothing.** Twitch OAuth continues working untouched.

**Hard requirement:** registration must refuse any username colliding with an existing character key or session login, case-insensitively. Characters are keyed by lowercased Twitch login; without this guard, anyone can register a matching name and take over a live character.

**Seam:** keep minting behind one thin function so an external identity provider — the Kibukah app — can mint sessions later without touching login internals. One function. No trait hierarchy.

### Stage 2 — Replace what chat provides ✅ **SHIPPED 2026-08-28** (`ac5573a` + `f5c38f8`, binary `B8D9B459…`)

Shipped: the announcement feed (tee into an in-memory ring, cap 50, pushed over the existing `/ws`, server-rendered card on the dashboard, 1s–30s reconnect backoff) and `POST /admin/ops/next-encounter` with a boss select, guarded by an `operator_action_gate` try-lock plus a `fight_in_progress` refusal. Twitch chat verified still receiving announcements — the tee did not become a reroute.

**Two items shrank on contact with the code:**

- **Rampage needed no work.** `LiveTunables::permanent_rampage` already existed and already rendered as a checkbox. The bot's `!rampage` drives a *different* variable (`rampage_remaining`), which becomes unreachable once its producers are deleted in Stage 3.
- **Force Boss was cut, not built.** As an operator control it is a strictly worse `next_encounter` — identical effect plus a 2-per-cycle cap. Its only meaningful form is a player-facing dust-priced purchase, which is new economy design, not a chat replacement. Deferred to the content work by owner ruling.

Original rulings, for the record:

| Capability | Ruling |
|---|---|
| Announcement feed | **Build.** Cheapest, highest value; all producers survive. Without it the game goes mute. |
| `next_encounter` operator control | **Build.** A live-ops lever with no web equivalent. |
| Force Boss | **Build.** Trivial once `next_encounter` exists. |
| Rampage | **Ruled: a toggle on `/admin/tunables` only, for now.** No player vote, no bespoke operator UI, no dedicated control surface. Owner ruling, 2026-08-27. |
| Rampage 3-vote player mechanic | **Cut for now.** Superseded by the tunables toggle above. Revisit only if the standalone game later wants player voting. |
| Chat activity XP | **Delete.** A Twitch engagement mechanic, meaningless standalone. |

All three player-facing chat commands already have web equivalents. Every remaining gap is a moderator tool.

### Stage 3 — Remove Twitch and Patreon

58 deletion targets across game crate, bot crate, deployment-root data files and `.env` keys. Two Cargo dependencies fall out: `url` deleted, `reqwest` demoted to dev-dependency.

**At cutover, invalidate every existing session.** Deliberately. See section 6.

**Split into 3a and 3b.** The operator-lockout gate below is too important to sit inside a large removal branch, so it ships first, on its own:

- **Stage 3a — operator identity.** Make the three operator logins configurable rather than hardcoded constants, defaulting to today's value so nothing changes on deploy. Owner registers a local account. Point the config at it. Verify `/admin/tunables` loads under that account *while Twitch still works*. Additive, independently shippable, reversible.
- **Stage 3b — the removal.** 58 deletion targets. Cannot start until 3a is verified live.

Making the logins configuration rather than constants also means the operator account can change later without a rebuild and a swap window.

#### HARD GATE — operator lockout

`ADMIN_TUNABLES_LOGIN`, `FIGHTS_PAGE_LOGIN` and `BUNDLE_OPERATOR_LOGIN` still hold `lokati_gaming`. Stage 1's collision guard **reserves that name**, so it cannot be registered through local login.

If Twitch OAuth is removed before those constants are re-pointed, the operator loses `/admin/tunables`, the fights page and bundle operations on the live game, with no route back in.

**Stage 3 may not ship until:** the three constants point at an operator account that exists and has been logged into successfully. Verify by logging in as that account and loading `/admin/tunables` *before* the OAuth removal is swapped, not after.

### Stage 4 — Durability and correctness fixes

Two code fixes required before Linux, neither optional:

1. **Directory fsync after rename** (`state.rs:136-140`) — the atomic save path renames without fsyncing the parent directory. Harmless on NTFS; on ext4 a crash after rename can lose the file outright. That file is character data. ~8 lines under `cfg(unix)`.
2. **`is_valid_custom_sprite`** (`character.rs:840-850`) — lowercases for the ownership check but not for `.exists()`. A hand-POSTed `custom/<MixedCase>` validates on NTFS and 404s on ext4. Requires a grep of live `adventure-characters.json` before cutover to find affected records.

Also drop the 5×20 ms rename retry loop — a Windows-only workaround with no purpose on Linux.

### Stage 5 — Split code from data

Today the deployment root holds source, game data, backups and build output in one directory, by construction, in both processes.

Now that only the game migrates, the split costs **~5 lines across two files** (`game/src/main.rs`, `game/src/adventure_web.rs`). `GAME_DATA_DIR` already relocates ~40 game files and 5 fight directories for free; six files bypass it.

Target layout: binary in `/opt/pathofdust`, mutable data in `/var/lib/pathofdust`, logs in `/var/log/pathofdust`, backups off-box.

### Stage 6 — First Linux build *(go/no-go gate)*

**No Linux build of this workspace has ever been attempted.** Only `x86_64-pc-windows-msvc` is installed and there is no CI. Dependency resolution succeeding is not the same as compiling.

Predicted requirements: `build-essential`, `pkg-config`, `libssl-dev`. `openssl-sys` enters through `reqwest`'s `default-tls`; `openssl-src` is absent, so system libssl is required. No ring, rustls or zstd in the game's graph.

This stage is a gate. Nothing downstream proceeds until a Linux binary exists and runs.

### Stage 7 — Provision and ops

- Purchase the box, monthly billing
- Ubuntu LTS, distribution only
- One systemd unit for the game
- `cloudflared` installed and pointed at the new origin
- `backup-game-data.ps1` rewritten as a shell script plus a systemd timer, retention logic carried over unchanged, writing off-box
- **Deleted, not ported:** `game-watchdog.ps1`, `watchdog.ps1`, `maintenance-flag.ps1`. systemd provides restart-on-failure, backoff, ordering and journald natively. Roughly 1,300 of ~1,940 PowerShell lines disappear. The maintenance-flag lease system exists solely to work around non-elevated sessions being unable to disable Windows scheduled tasks; on Linux the deploy runs `systemctl stop`.
- REFACTOR_PLAN §13 rewritten — shorter, since the maintenance-flag dance is gone

### Stage 8 — Cutover

Fresh world, fresh characters, sessions invalidated, DNS/tunnel repointed, World 1 retired.

---

## 5. What is not in this plan

Deferred deliberately. None of it blocks the world existing.

- Rampage player vote — cut for now; the tunables toggle replaces it
- ~~Auto-chess / TFT-style direction~~ — **CLOSED.** Asked and answered; no further investigation. Not a technology port; it would be a different game reusing the combat math.
- Affix tier curve, four new gear slots, crit multiplier halving, passive rebalance
- Passive tunables Stage 4 — the 9 remaining nodes (4 structure-only, 2 needing a second value slot, 3 excluded for the same reason)
- Ledger `#46` — environmental damage credited to nobody
- Golden fixture `hitId`/`eventId` churn, which makes every regeneration produce noise

---

## 6. Risks

**The 30-day silent session cliff — highest severity.**
`current_session` never re-validates. Removing OAuth logs nobody out; everyone keeps working for up to 30 days, then loses access one at a time, staggered, weeks later, with no common trigger. It takes the operator's `/admin/tunables` with it. Every smoke test passes.
*Mitigation:* invalidate all sessions at cutover. Fail loud and all at once. One bad afternoon beats a month of mystery.

**No Linux build has ever been attempted.**
*Mitigation:* Stage 6 is an explicit gate.

**Missing directory fsync on Linux.**
Silent data loss on crash. *Mitigation:* Stage 4, required.

**Sprite case sensitivity.**
Silent 404s post-cutover only. *Mitigation:* Stage 4 plus a live data grep.

**Override page has no unit labels and no range validation.**
Typing `45` meaning 45% persists an always-true threshold, silently. *Mitigation:* `fix/passive-override-units`, in flight.

**Account data has no backup coverage by default.**
`backup-game-data.ps1` uses an explicit allow-list, not a glob. A new state file is invisible to it until added by hand. `adventure-accounts.json` was missed on first delivery and caught in review. Account loss is unrecoverable — there is no external identity provider to re-authenticate against.
*Mitigation:* added to the CRITICAL list. **Standing rule: any new persisted state file must be added to the backup allow-list in the same branch that creates it.**

**No login rate limiting — accepted.**
Registration is open and there is no throttling on login attempts beyond what the site already has. Cloudflare fronts the origin and provides some bot protection. Accepted deliberately for now; revisit if abuse appears.

**`/admin/passives` Revert deletes rather than restores.**
The Revert control drops the override entirely instead of returning it to a prior value, so using it on a deliberately tuned node destroys the tuning with no undo. Same silent-destruction class as the save-path defects fixed in `fix/passive-override-units`. Found during the 2026-08-27 deploy verification. Recorded, not scheduled.

**Account file could be wiped to a valid empty `{}`.**
Backup shape validation accepts `{}` as legitimate (a game with no accounts yet), so a logic bug wiping the file would verify clean and prune normally — collapsing the safety margin from 30 days to 24 hours. The 30-day earliest-of-day retention still saves you. The guard would be a count-regression check. `adventure-characters.json` has identical exposure. Recorded, not scheduled.

**Stale overrides activating on migration.**
Shipped once already — three nodes ran wrong for ~20 minutes on 2026-08-27. Any node moving from structure-only to override-aware activates whatever sits in `adventure-passive-overrides.toml` for that key. *Mitigation:* the pre-migration check now recorded in `docs/passive_tunables_spec.md`.

---

## 7. Open rulings

- **`lastrites`** advertises a 33/66/100% chance that is never rolled — the check is a charge count. Either the description is wrong or the mechanic was never built.

### Parked for discussion, not blocking

- **Environmental-tagged damage and attribution** — *corrected 2026-08-27: this was previously cited here as "ledger #46". No such entry exists; that citation was carried forward from a stale summary and was wrong.* What the ledger does record is that Holy Fire carries `sourceKind: "environmental"`, which by construction excludes it from Doom accumulation — logged as an evidentiary gap shared with `#29` (Shattering). Whether environmental-tagged damage is also invisible to player attribution and therefore skews the damage leaderboard is **an open question, not an established finding**. Verify against the code before acting on it or citing it.
- **JSON → SQLite** — considered 2026-08-27, **deferred deliberately.** Real wins: partial writes (today one character save rewrites all 4.1 MB), WAL transactions replacing the hand-rolled atomic save, single-file backup, and queryable leaderboards. If done, the cheap shape is **SQLite with JSON blob columns** — one row per character, serde structs unchanged — not a relational schema. Not during World 2: no stage requires it, and changing persistence during a platform migration makes any data loss impossible to attribute. "Fresh characters makes it free" is misleading — that removes the migration, not the call-site refactor. **Trigger to revisit:** measure how often the full-file rewrite fires; if it is per-fight, that is ~5.8 GB/day of writes today and scales linearly with player count. Also revisit when a leaderboard query is wanted that cannot afford to load everything (see `#46`).
- **Golden fixture `hitId`/`eventId` churn** — regeneration rewrites most fixtures with no semantic change, so every deploy produces diff noise that will eventually stop being read. Owner to be briefed; no action scheduled.
