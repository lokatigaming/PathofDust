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

### The game is CYCLICAL — worlds reset periodically *(owner ruling, 2026-08-30)*

World 2 is not "the permanent world." It is **season 1 of a game that resets on a cycle.** World 1 was season 0, and it ran unbounded for over a year.

This is load-bearing context for every design decision, because **most of what went wrong in World 1 is a symptom of unbounded accumulation, not of any individual mechanic:**

- Both pacing controllers saturating at their ceilings
- Player power outgrowing enemy scaling until a 100× boss-damage multiplier could not kill anyone
- Fight replays reaching ~950 MB, because event count is a product of boss count × action rate × splash targets × duration, and all four inflated together over 7,400 stages
- 3×10¹⁴ party DPS, 15,000% divine damage, 100,000% crit multipliers

A periodic reset bounds every one of those by construction. They are not permanent properties of the design; they are what a year without a reset looks like.

**The number that sets the design point: what stage does a season reach before it resets?** That single figure determines the growth-curve shapes, boss-count scaling, event volume and whether storage is ever a concern. A season topping out around 1,000–2,000 makes the entire World 1 failure class moot. **Not yet decided.**

**Consequence for scope:** do not build defences against problems the reset cadence already eliminates. The fight-storage migration was scoped and then **cancelled on this basis** — see the SQLite entry in section 5.

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
| OS | **Debian 13 (trixie)**, distribution only. No control panel image. *(Ubuntu was planned; the provisioned image is Debian 13 and was kept — leaner base, identical package names for our needs, same systemd, cloudflared ships .deb.)* |
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

#### Stage 3a — operator identity ✅ **SHIPPED AND VERIFIED 2026-08-28** (`6b5fa51`, binary `5CE40C85…`)

Three operator logins became `LazyLock<String>` over a single `OPERATOR_LOGIN` env key, defaulting to `lokati_gaming` so deploying changed nothing. Owner registered `lokati`, pointed the key at it, restarted, and confirmed all four admin surfaces live. `lokati_gaming` is now **permanently reserved** from registration regardless of what `OPERATOR_LOGIN` holds — which matters because World 2's fresh characters remove the live-character guard protecting it today.

One key rather than three, deliberately: three keys would be three chances to typo one and half-lock-out the operator, which is the exact failure the stage exists to prevent.

Revert if ever needed: delete the `OPERATOR_LOGIN` line from `.env` and restart.

#### Stage 3b — the removal

The removal-scope audit (`docs/external_integration_removal_scope.md`, Part 4) defines seven stages. Reconciled against what has actually shipped:

| Audit stage | Status |
|---|---|
| 1 — turn the seam off, no deletion | **Needs correction — see below** |
| 2 — delete Patreon | ✅ **Shipped 2026-08-28** (`7ef31ac`, game `11CC1783…`, bot `CBC8A67E…`). Two files deleted, net −105/+16. No dependencies freed. `patreon-tokens.json` and `patreon-seen.json` moved to `backup-pre-remove-patreon/`, not deleted |
| 3 — announcement feed before deleting SSE | ✅ Shipped 2026-08-28 |
| 4 — delete the `/api/*` seam | After 1 |
| 5 — operator controls | ✅ Shipped, scoped to `next_encounter`; Force Boss deferred by ruling |
| 6 — replace the identity minter | Largely done — local login shipped, operator constants repointed. **Remaining:** delete Twitch OAuth (D9–D22, D37, D38) and, at cutover, `adventure-sessions.json` + `adventure-characters.json` |
| 7 — ops and documentation cleanup | Last |

##### The audit is a map, not an inventory

On the Patreon slice alone — the smallest and cleanest item on the list — the removal-scope audit missed **five** targets, one of them build-breaking (`src/main.rs:1056`), and three of its figures were wrong. Later stages, particularly the `/api/*` seam and the Twitch OAuth removal, must **independently verify every target list against the code** rather than deleting from it. Recorded as ledger `#54`.

##### Correction to the audit's Stage 1

The audit says to stop the bot **permanently**, because it hard-fails without `ADVENTURE_API_SECRET` (`src/config.rs:238-239`) and that secret must be pulled to un-mount `/api/*`.

**That conflicts with the settled decision that the bot survives** on the Windows box doing Twitch and OBS work. The hard-fail must be removed first. Corrected order:

1. **Bot standalone** — remove the `ADVENTURE_API_SECRET` hard-fail and retire the game-calling paths. The bot keeps song requests, alerts, chat overlay, entrance themes, PoE utilities and OBS control.
2. **Seam off** — unset `ADVENTURE_API_SECRET`, restart the game. `api.rs:61-62` un-mounts the whole `/api/*` router with no code change. Fully reversible.

##### This is the player-facing cutover moment

Turning the seam off is when players lose the 10 chat commands, the 3 redemptions and chat activity XP. Narration already has a web home and web join already works, so nothing goes dark — but it is a deliberate, announced change, not a quiet flip. Patreon deletion has no player-facing effect and ships before it.

#### HARD GATE — operator lockout ✅ **CLEARED 2026-08-28**

`ADMIN_TUNABLES_LOGIN`, `FIGHTS_PAGE_LOGIN` and `BUNDLE_OPERATOR_LOGIN` still hold `lokati_gaming`. Stage 1's collision guard **reserves that name**, so it cannot be registered through local login.

If Twitch OAuth is removed before those constants are re-pointed, the operator loses `/admin/tunables`, the fights page and bundle operations on the live game, with no route back in.

**Stage 3 may not ship until:** the three constants point at an operator account that exists and has been logged into successfully. Verify by logging in as that account and loading `/admin/tunables` *before* the OAuth removal is swapped, not after.

### Stage 4 — Durability and correctness fixes ✅ **SHIPPED 2026-08-29** (`31165ad`, binary `9EB658FF…`)

Two code fixes required before Linux, neither optional:

1. **Directory fsync after rename** (`state.rs:136-140`) — the atomic save path renames without fsyncing the parent directory. Harmless on NTFS; on ext4 a crash after rename can lose the file outright. That file is character data. ~8 lines under `cfg(unix)`.
2. **`is_valid_custom_sprite`** (`character.rs:840-850`) — lowercases for the ownership check but not for `.exists()`. A hand-POSTed `custom/<MixedCase>` validates on NTFS and 404s on ext4. Requires a grep of live `adventure-characters.json` before cutover to find affected records.

Also drop the 5×20 ms rename retry loop — a Windows-only workaround with no purpose on Linux.

### Stage 5 — Split code from data ✅ **SHIPPED 2026-08-29** (same release as Stage 4)

Six data files plus `adventure-accounts.json` now route through `GAME_DATA_DIR`. Verified live: with the variable unset, every write landed **in place** in the deployment root across three post-swap fights, and no new directory appeared anywhere. The unset-is-identical test was proven non-vacuous by mutation — reverting the routing failed the set scenario while the unset scenario stayed green.

`CUSTOM_SPRITE_DIR` deliberately left CWD-relative; see the provisioning trap in ledger `#60`.


Today the deployment root holds source, game data, backups and build output in one directory, by construction, in both processes.

Now that only the game migrates, the split costs **~5 lines across two files** (`game/src/main.rs`, `game/src/adventure_web.rs`). `GAME_DATA_DIR` already relocates ~40 game files and 5 fight directories for free; six files bypass it.

Target layout: binary in `/opt/pathofdust`, mutable data in `/var/lib/pathofdust`, logs in `/var/log/pathofdust`, backups off-box.

### Stage 6 — First Linux build ✅ **GATE PASSED 2026-08-31** (`e90ec8a`, `docs/linux_build_gate.md`)

**Builds and passes on Debian 13 with zero code changes.** The audit missed no portability defect.

- Build succeeded first attempt: 2m23s wall, 700% CPU, peak RSS 1.29 GiB. Binaries: `game` 19 MB, `twitch-bot-rs` 23 MB.
- **755 passed, 0 failed — identical to the Windows baseline.** Verified as the whole suite, not a subset masking a swap of Windows-only for Linux-only tests.
- **Golden corpus: Windows-generated fixtures pass on Linux**, across two rustc versions and two LLVM backends. Float results and ordering are reproducible cross-platform, so a future fixture mismatch is a behavioural change, not platform drift.
- The `cfg(unix)` parent-directory fsync and the case-insensitive sprite resolver both ran on their target platform for the first time and behave.

**The entire build dependency surface:**

```
apt-get install -y build-essential pkg-config libssl-dev
```

`gcc 14.2.0`, `ld 2.44`, `OpenSSL 3.5.7`. Nothing else needed, no second pass. Rust via rustup, stable.

**Two findings to carry forward:**

- **No toolchain pin.** Linux is on rustc 1.98.0, Windows on 1.97.1, and no `rust-toolchain.toml` exists. Benign today — identical test results — but two machines building the production binary on different compilers is a reproducibility gap. A one-line file closes it.
- **Correction to the portability audit:** its "no ring/rustls/zstd in the graph" claim holds for the `game` crate but *not* the workspace — root `twitch-bot-rs` pulls ring, two rustls, zstd-sys and flate2. They vendor their C so the package list is unchanged, but the audit's reasoning was wrong at workspace scope.
- Box notes: no swap configured; `git` is not installed (only `curl`).

*(Original gate description retained below for context.)*

#### Original gate definition

**No Linux build of this workspace has ever been attempted.** Only `x86_64-pc-windows-msvc` is installed and there is no CI. Dependency resolution succeeding is not the same as compiling.

Predicted requirements: `build-essential`, `pkg-config`, `libssl-dev`. `openssl-sys` enters through `reqwest`'s `default-tls`; `openssl-src` is absent, so system libssl is required. No ring, rustls or zstd in the game's graph.

This stage is a gate. Nothing downstream proceeds until a Linux binary exists and runs.

### Stage 7 — Provision and ops 🟡 **STAGING INSTANCE LIVE 2026-08-31** (`6cc5456`, `70f601b`, `docs/linux_staging.md`)

**Path of Dust runs on Linux.** Empty data, no public ingress, Windows production untouched.

| Path | Owner | Contents |
|---|---|---|
| `/opt/pathofdust/bin/` | `root:root 0755` | binary + `deploy.sh` |
| `/var/lib/pathofdust/` | `pathofdust:pathofdust 0750` | all mutable state — the systemd `WorkingDirectory` |
| logs | — | journald |

Service runs as a `nologin` system user under `ProtectSystem=strict`, `NoNewPrivileges`, `PrivateTmp`, `ReadWritePaths=/var/lib/pathofdust`. `Restart=always`, `RestartSec=5s` — this **replaces `game-watchdog.ps1`**, which is not being ported. Verified: `kill -9` → back in 9s; reboot → auto-start with state intact.

**`ADVENTURE_API_SECRET` is deliberately absent**, so `api.rs:61-62` un-mounts `/api/*` entirely. Verified 404 on four routes, still 404 with an `Authorization` header. The staging instance is Twitch-free by construction, with no code deleted — which is why provisioning and removal stopped being sequential.

**Ledger `#60` closed by demonstration.** Asset refresh is `cp -r` only — never `rsync --delete`, never `rm -rf`. An untracked `zz-staging-upload-proof.png` survived a redeploy in the same run that reverted a tampered tracked template.

#### The season-1 storage baseline — this settles the SQLite question

**57 KB per fight** (summary 540 B, coarse 6.6 K, detail 24 K, bundle 25.6 K) against World 1's **~1.9 GB**. A factor of roughly **30,000**. Total footprint after six fights: 39 MB, of which 38 MB is checked-in sprite art; game state is 380 KB.

Confirms the cyclical-reset ruling in section 1: the gigabyte replays were an artifact of unbounded accumulation, not of the storage design.

#### Findings from standing it up — all resolved ✅ *(shipped 2026-08-31, `a2fc69d`, binary `695E8C68…`, ledger `#71`–`#74`)*

1. ~~Operator gate returns HTTP 200 with a "Not Found" body~~ → **real 404**, body byte-identical. Ledger `#51` closed in the same release: the three admin POSTs no longer answer a non-operator with a fake `?saved=1` redirect.
2. ~~Operator bootstrap chicken-and-egg~~ → **`OPERATOR_BOOTSTRAP`**, carrying the login *value* rather than a boolean, so a stale variable permits nothing after `OPERATOR_LOGIN` moves. Unset by default. `lokati_gaming` stays permanently reserved regardless.
3. ~~Twitch credentials mandatory at boot~~ → **optional**. Absent means the Twitch login route and link are simply not registered. Nothing deleted; removal is still Stage 3b's job.
4. `bind("0.0.0.0")` with no bind-address config — **unfixed**, mitigated at nftables on the staging box (4004/4005 loopback-only). Revisit when ingress is configured.
5. ~~`StartLimit*` in `[Service]` silently ignored~~ → moved to `[Unit]`.

**Also shipped, unordered but necessary:** nine dynamic-pacing fields had `serde(default)` resolving to `0.0`, below their own floors — previously masked by silent clamping, and would have made every omitting POST 400 once validation went live. **This is the second `serde(default)` mismatch found** (the pool-cap field was the first); check it whenever a form field is added.

**And the silent clamp is gone page-wide:** one validation pass over all 67 fields reports *every* offending field at once with its accepted range, HTTP 400, nothing written. Malformed anchor lists now reject instead of silently emptying the pacing baseline floor. Verified live by a no-op save returning the file byte-identical, same SHA-256.

**Still open:** `/admin/passives` returns 200 on a rejected save while `/admin/tunables` now returns 400 — needs its own alignment order. The bad-key arm of `do_save_passive_override` still fake-redirects, and the public "no such character" cards at `adventure_web.rs:2254`/`:2277` are the same fake 404.

#### Remaining in Stage 7

- cloudflared and public ingress *(deliberately not done — `adventure.lokati.net` still points at Windows)*
- `backup-game-data.ps1` → shell script plus systemd timer, retention logic carried over, writing off-box
- REFACTOR_PLAN §13 rewritten for Linux

#### Original stage definition

- Purchase the box, monthly billing
- Debian 13 (trixie), distribution only — **purchased and provisioned 2026-08-31.** VPS Lite 3 G12s: 8 vCPU, 15 GiB RAM, 314 GB disk. Root SSH by ed25519 key from the Windows box; no password auth used.
- One systemd unit for the game
- `cloudflared` installed and pointed at the new origin — ✅ **Done 2026-08-31** (`docs/linux_ingress.md`). Locally-managed tunnel `pod-staging`, `staging.lokati.net` → `http://localhost:4005`, live and serving. Note the config the unit actually reads is `/etc/cloudflared/config.yml`, the copy `service install` makes — **not** `/root/.cloudflared/config.yml`, which is inert after install
- `backup-game-data.ps1` rewritten as a shell script plus a systemd timer, retention logic carried over unchanged, writing off-box — ⚠️ **Local half done 2026-08-31** (`docs/linux_backups.md`). `backup-game-data.sh` + `pathofdust-backup.{service,timer}` run daily and are restore-verified; retention is **90 daily**, not the Windows 30 (that number was disk-driven and the constraint does not exist here — the Linux set is 2.7 MB). Off-box is **not yet running**: the Linux pull endpoint (`podbackup` + forced-command shell) is built and proven, the Windows Scheduled Task that fetches from it is written up ready-to-run but not installed
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

  **TRIGGER FIRED — 2026-08-30.** Measured on the live box after the pool-cap raise lengthened fights from ~2s to ~30s: `adventure-fights-detail` and `adventure-fights-bundle` are writing **~950 MB and ~965 MB per fight**, one fight every ~2.6 minutes — roughly **730 MB/minute sustained**, each through the atomic temp-write → fsync → rename path. The game became visibly sluggish. Summary files are unaffected at ~15 KB.

  **But the storage format is the second problem, not the first.** Event capture has no cap: a single fight produces on the order of five million events, and a 1 GB replay bundle is unusable *as a replay* — nothing will parse it. Cap or sample event capture first; that shrinks the problem by an order of magnitude before any storage technology changes. SQLite then removes the remaining stall (incremental row append instead of one 1 GB fsync), cuts on-disk size perhaps 3–5×, and makes fight data queryable per player or per time window.

  **Decision: design World 2's persistence around SQLite from the start; do not convert World 1 mid-flight.** The refactor touches every read and write site, on a world being retired, competing with the Linux move.

  **SUPERSEDED — 2026-08-30, cyclical reset ruling.** A scoping session was prepared and then **cancelled**. Event volume is a product of boss count × action rate × splash targets × duration, and a periodic world reset bounds all four. The 950 MB replay is what a year without a reset produces, not a property of the storage design. JSON is adequate at season-scale volumes.

  **Binding constraint if this is ever reopened:** players read fight events LIVE through an external desktop app and use them to decide how to build their characters. Fight event data is a player-facing feature. **Capping, truncating, sampling or thinning events is not an available option** — only representing the same information more cheaply.

  **Worth knowing regardless, never answered:** two artifacts of near-identical size are written per fight — `adventure-fights-detail` (~950 MB) and `adventure-fights-bundle` (~965 MB), seconds apart, 3 retained each. The bundle format is newer (`replay-bundle.v1.json`). If one is legacy and nothing reads it, that is half the write volume for nothing, and season 1 should not inherit it. Cheap to check whenever someone is in that code.
- **Golden fixture `hitId`/`eventId` churn** — regeneration rewrites most fixtures with no semantic change, so every deploy produces diff noise that will eventually stop being read. Owner to be briefed; no action scheduled.
- **`/admin/passives` pins every saved row, including rows saved at their compiled defaults** — recorded 2026-09-01, **not decided.** `do_save_passive_override` (`adventure_web.rs:2922`) runs `overrides.nodes.insert(form.node_key, vec![form.r1, form.r2, form.r3])` unconditionally on save. There is no comparison against the compiled-in node values, so saving a row you did not change — opening a node, glancing at it, hitting save — writes an explicit override into `adventure-passive-overrides.toml` that is byte-identical to the default. From that moment the node is **pinned**: `passive_override_for` returns the stored value and any future change to the compiled default silently does not reach it.

  Live risk for World 2 rebalancing specifically: a rebalance pass that edits compiled defaults will appear to do nothing on exactly those nodes an operator happened to save at some point, and nothing in the UI distinguishes a pinned-at-default node from an unpinned one.

  **Narrowing finding:** a clear-override control *already exists* — `do_revert_passive_override` (`adventure_web.rs:2937`, route `/admin/passives/revert`, backed by `PassiveOverrides::revert`). So this is not "there is no way out", it is "the save path creates pins nobody asked for, and the operator has to know to go press revert on each one." That materially shrinks option 2 below.

  Options on the table, **none chosen**:
  1. Leave as-is — saving is explicit, and revert already exists.
  2. Surface the existing revert control better (e.g. mark pinned rows in the form), rather than adding a new mechanism.
  3. Skip the write when the submitted values equal the compiled defaults, so a no-op save leaves no pin.

  Owner to rule. Do not implement any of the three without that ruling.
- **Custom sprites live in two sources and only one of them is git** — recorded 2026-09-01 during the Linux data-migration rehearsal. `public_adventure_overlay/sprites/custom/` held **14** files on Windows production; git tracks **9**. The other 5 are player uploads that exist only on the live box and in `backup-game-data.ps1`/`backup-game-data.sh` snapshots (both scripts include that directory for exactly this reason). One of the untracked five, `lokati_gaming6.gif`, is the *actively referenced* model for character `lokati_gaming`.

  Consequence: a deploy that reinstalls the checked-in assets restores 9 sprites and a restore-from-backup restores 14, so "the assets are in git" is true of the directory and false of its contents. Any cutover, rebuild, or fresh instance must take this directory from a **backup**, not from a checkout, or characters silently fall back to their hash-default sprite with no error anywhere. Proven on staging: see `docs/linux_deploy.md`.

  Not acted on here — player uploads were deliberately **not** added to git in that session.
- **The Linux tunnel is still named `pod-staging` while serving production** — recorded 2026-09-02 at cutover, **cosmetic, deliberately not fixed then.** `dbe011f9-a8df-4cbf-811c-e7a3d773b9c1` was created as the staging tunnel and now carries `adventure.lokati.net`. Nothing functional depends on the name — DNS points at the UUID, and the ingress rule is matched by hostname — so the cutover session left it alone rather than improvise a rename during a live production move.

  Why it is worth fixing eventually: the name is what a human reads in `cloudflared tunnel list`, in the Cloudflare dashboard, and in any incident. A tunnel labelled `pod-staging` in the row that is actually serving players is a misdirection at exactly the moment nobody can afford one — and the rollback target beside it is honestly named `adventure-dashboard`, which makes the wrong one look like the real one.

  Renaming a tunnel does not move its DNS routes or its credentials, so this is low-risk, but it is not zero-risk and it should not be done during an incident. Schedule it deliberately, when production is quiet and a rollback is not in flight.
- **Never put unbounded synchronous work back on the async runtime** — recorded 2026-09-02, fixed the same day, and the lesson is structural rather than numeric. `simulate_battle` was called directly from the async encounter loop. Because it is synchronous and never yields, it froze the entire Tokio runtime for the duration of every fight — `accept()` included, so even static files hung and the site was unreachable, not merely slow. Fixed by `tokio::task::spawn_blocking` at all four call sites (`manager.rs` `simulate_battle` ×2, `save_last_fight` ×2).

  **The 145 s figure is an end-of-World-1 artifact, not a design target.** It was measured at stage 7380 with a 46-player party on a slow emulated vCPU, and stage-7380 fights with a party that size will not recur once World 1 ends. Do not carry that number forward as a budget, and do not design World 2 around "fights are expensive".

  **What does carry forward is the shape of the defect.** A synchronous, unbounded-duration computation on the runtime freezes the HTTP server *in proportion to fight cost, at any scale* — it was firing on Windows too, just briefly enough that nobody saw it. It only became an outage when the platform got slower. The `spawn_blocking` fix removes the class of defect permanently, so World 2 inherits no constraint from this beyond the one-line rule above: whatever the new combat does, it must not run synchronously on an async worker. Cheap fights hide this bug; they do not prevent it.
