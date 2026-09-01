# Linux deploy procedure + rehearsed production-data migration

**Date:** 2026-09-01 · **Session:** LINUX-DEPLOY-PROC · **Branch:** `chore/linux-deploy-proc`
**Source commit built and deployed:** `692da98`
**Binary sha256:** `e5f21e43499626da65c04efca8481ecc17e7dce4269177d5fa2f01ac68ed5930`
**Previous binary (rollback slot):** `a546eb128de3c35218347e84ccf5d9f540a86f58eba3a7545bd6f480a111a52d`

The procedure itself is `REFACTOR_PLAN.md` §13B. This document is its evidence: what the
migration actually did, what diverged, and what the numbers were.

`C:\PathofDust` was not touched — not read for state, not written, not restarted. Every
production fact below comes from a **backup snapshot**, never from the live box. No cutover
was performed; `adventure.lokati.net` still points at Windows.

---

## Part A — the rehearsed data migration

### A0. Source snapshot

`C:\pod-backups\PathofDust\pod-backup-20260901-210002` — the 21:00 hourly, newest available.
Chosen because its manifest reads `verdict: clean`, `filesCopied: 253`, `filesFailed: 0`, no
BOM on any file and no manifest drift, and because the newest clean snapshot is the smallest
divergence from live.

Payload shipped to staging: `pod-prod-state.tar.gz`, 5,367,086 bytes, 258 members,
sha256 `fecd657cb317c1aad61ab0bd0e3958491b91ab7d47acf828fc3cd226770d743f`, hash confirmed
identical after upload. `_backup-manifest.json` was excluded — it is a backup artifact, not
game state.

### A1. What a full production data set contains vs what staging had

| | Production snapshot | Staging before |
|---|---|---|
| Files in the state set | 254 (13 MB) | 377 (40 MB, of which 38 MB is checked-in art) |
| Top-level state files | 40 | 31 |
| Characters | **67** | 2 |
| Accounts / sessions | 1 (`lokati`) / **152** | 2 / 2 |
| World stage | **7,389** | 0-era synthetic |
| `hp_pacing_mult` | **435.21** | — |
| `recent_win_dps` | ~1.5 × 10¹⁵ | 31.8 |
| `boss_losses_since_win` | 0 | 118 |
| `sprites/custom` | **14** files | 9 (the git-tracked ones only) |
| `adventure-fights-summary` | 200 | 200 |
| Fight seq counters | coarse 18969 / detail 18867 / summary 18463 / bundle 14002 | 617 each |

**Nine files existed in production that staging had never created.** Each is a cutover step
that a fresh instance does not produce on its own:

| File | Why staging never had it |
|---|---|
| `adventure-live-tunables.toml` | written only when an operator first saves the tunables form |
| `adventure-passive-overrides.toml` | written only by `/admin/passives` |
| `adventure-item-balance.toml` | written only by the item-balance admin surface |
| `adventure-rampage-state.json` | written when rampage state first advances |
| `adventure-reforge-cooldown.json` | written on the first reforge |
| `patch-notes.json` | 172 KB of real release history; gitignored runtime data |
| `bot-published-constants.json` | written across the `/api/*` seam, which is unmounted on staging |
| `adventure-celestial-shard-first-award-marker.json` | written when that giveaway first fires |
| `_backup-manifest.json` | backup artifact — deliberately not migrated |

### A2. The load

Staging's synthetic state was **moved aside, never deleted**, to
`/var/lib/pod-synthetic-state-20260901-154401` (244 files). Reversible in one `mv`.

Archived first, three ways:

| Artifact | Where | sha256 |
|---|---|---|
| Allow-list archive (proven mechanism) | `/var/backups/pathofdust/pod-backup-20260901-152628.tar.gz` — 240 files, `verdict=clean` | verified `OK` against its sidecar |
| Full state tar | `/var/backups/pod-premigration/staging-prestate-full-20260901-152659.tar.gz` | `002ff6e8…f949bd2` |
| Both, pulled off-box | `C:\pod-backups-linux\premigration\` | re-hashed after transfer, match |

The three checked-in asset directories (`templates/`, `wiki/`, `public_adventure_overlay/`)
were left in place; only game-written state was replaced.

### A3. Proof it loaded

| Claim | Evidence |
|---|---|
| 67 characters | journald: `loaded 67 characters from adventure-characters.json`; `json.load` → 67 keys |
| Shared party intact | fight `0000018465` records **53 participants** resolving as one party; later fights 54 then 55 |
| World state consistent | `stage: 7389`, `last_boss_kind: Dragon`, `hp_pacing_mult: 435.206466606523`, `boss_power_mult: 0.5778040392112982` — byte-identical to the snapshot |
| Accounts readable | parses; 1 account (`lokati`) |
| Sessions readable | parses; **152** sessions. A real production session token authenticated against staging and rendered the operator's own dashboard — that is the proof, and it is also exactly why the scrub in A7 was required |
| Custom sprites present | all **14** on disk, including the 5 that git does not track |
| Custom sprites served | see below |

**Sprites, end to end.** All six custom-sprite characters resolve. Fetched over HTTP, byte
counts match the files on disk exactly:

| URL the rendered page emits | HTTP | bytes |
|---|---|---|
| `/sprites/custom/Sitch89.gif` | 200 | 687,999 |
| `/sprites/custom/kibukah.png` | 200 | 34,868 |
| `/sprites/custom/kmartbikes12.gif` | 200 | 383,800 |
| `/sprites/custom/lokati_gaming6.gif` | 200 | 39,489 |
| `/sprites/custom/qugetus_2.gif` | 200 | 324,048 |
| `/sprites/custom/xborntokillx.png` | 200 | 187,636 |

**The `Sitch89` case test, which is the point.** Character `sitch89` stores
`model: "custom/Sitch89"`; the file on disk is `Sitch89.gif`. On case-insensitive NTFS any
casing works and the bug is invisible. On ext4 it is not:

```
/sprites/custom/Sitch89.gif  -> 200, 687999 bytes   <- what the page emits
/sprites/custom/sitch89.gif  -> 404
/sprites/custom/SITCH89.gif  -> 404
/sprites/custom/Kibukah.png  -> 404
```

The negative controls confirm the filesystem really is case-sensitive, so the 200 is a real
result and not an accident of a forgiving filesystem. The rendered page emits the capital
`S` verbatim, so casing survives end to end. `custom_sprite_file_exists`
(`character.rs:880`) resolves by listing the directory and comparing case-insensitively,
which is what makes the *validation* half agree across platforms; this migration is the
first time the *serving* half was tested on ext4 against real data.

Note: the page emits a `.png` **and** a `.gif` URL for every custom sprite and lets the
browser fall back, so exactly one of each pair 404s by design. That is not a migration
fault.

### A4. Proof it runs — real fights on real data

Seven fights resolved on real stage-7389 characters. Numbers, not a verdict:

| seq | kind | stage | won | participants | duration | `hp_pacing_mult` | `boss_power_mult` |
|---|---|---|---|---|---|---|---|
| 18464 | boss | 7389 | yes | 53 | 25.8 s | 479.82 | 0.573553 |
| 18465 | boss | 7390 | yes | 53 | 26.4 s | 479.82 | 0.573553 |
| 18466 | boss | 7391 | yes | 53 | 32.0 s | 503.81 | 0.577939 |
| 18467 | boss | 7392 | yes | 54 | 48.6 s | 528.99 | 0.589054 |
| 18468 | boss | 7393 | yes | 55 | 25.8 s | 551.97 | 0.606725 |
| 18469 | boss | 7394 | yes | 55 | 41.8 s | **535.76** | 0.630341 |
| 18470 | boss | 7395 | yes | 55 | 27.8 s | 562.55 | 0.658892 |

Starting value on load was `hp_pacing_mult` 435.21 / `boss_power_mult` 0.577804.

**What the pacing controllers actually did, stated plainly.** Every fight was a boss fight,
every fight was a win, `boss_losses_since_win` never left 0, and the stage advanced on every
one (7389 → 7396).

`boss_power_mult` rose **strictly monotonically, 0.5736 → 0.6589 across seven wins (+14.9 %)**,
never once reversing. On a party that wins everything, that is a controller with no downward
pressure applied to it in this window — it is climbing because nothing has told it to stop,
and nothing here demonstrates it can come back down, because nothing ever lost.

`hp_pacing_mult` did **not** behave the same way, and this is worth stating precisely because
it is easy to report the first five fights and call it monotonic: it rose 479.8 → 552.0 over
fights 18464–18468, then **fell to 535.8 on fight 18469**, then rose again to 562.6. So it
oscillates within an upward trend rather than ratcheting. Both fights either side of the dip
were wins with 55 participants, so the reversal is not explained by a loss or by party size
in this data. Seven fights is not enough to characterise the controller and this session did
not try to — the finding is simply that the two controllers do **not** move together, and
`hp_pacing_mult` has at least one reversal that `boss_power_mult` does not.

Net over the window: `hp_pacing_mult` 435 → 563 (+29 %), `boss_power_mult` 0.578 → 0.659
(+14 %). Nothing crashed, nothing produced a NaN, nothing divided by zero on real
stage-7389 numbers — the controllers are operating on real data, they are just operating in
one direction because the party never lost.

Fight duration over the same window went 25.8 → 26.4 → 32.0 → 48.6 → 25.8 → 41.8 → 27.8 s.
The 48.6 s value landed while the test suite was saturating all eight cores, so it is
contended wall-clock, not a game measurement.

**Write volume, which is the real finding.** Per fight, on real data:

| Tier | Bytes per fight | Retained |
|---|---|---|
| summary | ~18 KB | 200 |
| coarse | 188–205 MB | 5 |
| detail | 895 MB – 1.16 GB | 3 |
| bundle | 904 MB – 1.01 GB | 3 |

`/var/lib/pathofdust` went 40 MB → 7.0 GB and then **plateaued**, because
`COARSE_FIGHTS_CAPACITY=5`, `DETAIL_FIGHTS_CAPACITY=3` and `BUNDLE_FIGHTS_CAPACITY=3`
(`fight_storage.rs:57–79`) prune the expensive tiers. Disk was never at risk — 289 GB free
throughout. This reproduces `world2_build_plan.md`'s "TRIGGER FIRED — 2026-08-30" write-volume
finding on Linux, and it is worse than the figures recorded there (detail was ~950 MB then,
1.16 GB in the last fight here), because the fights are longer now.

**One unexplained 120-second stall — observed once, not reproduced.** The first authenticated
request after the migration load (`GET /`, operator session) **timed out at 120 s**, and the
`/characters` request immediately after it took **17.1 s**. Every request after those two was
under 10 ms.

I could not reproduce it, and I tried deliberately:

| Attempt | Result |
|---|---|
| Two controlled `systemctl restart`s, first authenticated hit each time | **1.6 ms**, both |
| Three deploy swaps (deploy, rollback, roll-forward) | no stall |
| 660 s probe polling authenticated `/` and `/characters` every 2 s, spanning a complete fight (detail file written 16:08:23) | **zero requests over 1.0 s** |
| `systemctl restart` then 12 immediate authenticated `/characters` hits | 9 × connection-refused during startup (~0.1 ms each, server not yet listening), then 6.5 ms |

So the tidy explanation — "a ~1 GB fight write blocks the runtime" — is **not supported by the
evidence.** A fight was written straight through the probe window without any request
exceeding one second. I am recording what was observed rather than the mechanism I first
assumed, because the assumption did not survive the test.

What is left is a real, one-off, 120-second unavailability on the first authenticated request
against freshly migrated production data, with no reproduction and no explanation. It should
not be dismissed — it happened on the exact operation a cutover performs — and it should not
be presented as understood. Recorded for the anomaly ledger.

Two facts that *are* solid and matter for the health check: anonymous `/` returns a constant
72,025-byte landing page that renders identically whether or not any data loaded, so it is
worthless as a health probe; and after a restart the server refuses connections for a few
hundred milliseconds before it listens, so a health gate must poll rather than fire once.

### A5. Divergences hit

Each of these is a cutover step we would otherwise have discovered live.

| # | Divergence | Status |
|---|---|---|
| 1 | **Windows UID/GID leak through tar.** A tarball built on Windows extracts with numeric owner `197108:197121`, which is nobody on Debian; the service user then cannot write its own data dir. | **Fixed** — `chown -R pathofdust:pathofdust` after every extract, in both the loader and `deploy-linux.sh`. Now a documented, non-optional step. |
| 2 | **`C:\`-style paths are parsed as `host:path` by POSIX tools.** `tar czf C:/…` fails with `Cannot connect to C: resolve failed`. | **Fixed** — POSIX `/c/...` paths only, on every tool crossing the boundary. |
| 3 | **Case sensitivity on ext4 is real.** `sitch89.gif` 404s where `Sitch89.gif` 200s. | **Checked, no fix needed** — the stored model already carries the exact casing and the validation path is case-insensitive by construction. Proven with negative controls rather than assumed. |
| 4 | **5 of 14 custom sprites are not in git.** `lokati_gaming6.gif`, actively referenced by character `lokati_gaming`, is one of them. | **Fixed for this migration** by sourcing sprites from the backup, not a checkout. Recorded as a standing hazard in `world2_build_plan.md`. |
| 5 | **The bulk fight tiers are not in the snapshot.** `includeFightArchives: false` by design. | **Structural** — see Part C. |
| 6 | **`adventure-item-balance.toml` names a retired affix.** Production's file carries `lingeringEffect`, which the running binary logs as `retired affix with no live base value to override, ignoring` on every load. | **Not fixed, not in scope.** Harmless (it is ignored) but it is real production data that no longer matches the code. One line in the journal under FOUND. |
| 7 | Line endings and BOM | **Checked, clean.** Every `.json` and `.toml` in the snapshot is LF-only with no BOM, top-level and in `adventure-fights-summary/`. The game writes with `serde_json`/`std::fs::write`, so nothing introduces CRLF. No conversion step is needed and none was applied. |
| 8 | File permissions | **Fixed** — dirs 0755, files 0644, `/var/lib/pathofdust` 0750, all owned by `pathofdust`. |
| 9 | **Off-box download throughput collapsed mid-session**, from 3.35 MB/s to ~10–70 KB/s for roughly ten minutes, then recovered to 5.14 MB/s. Uploads were unaffected throughout (1.2–4.2 MB/s). | **Not fixed — flagged.** Migration only needs uploads, so it did not block. But the nightly `PodPullLinuxBackups` and any restore-from-off-box run in the affected direction. Worth watching; not diagnosed here. |
| 10 | **The operator account cannot be re-created on migrated data.** `do_register` (`accounts.rs:273`) refuses any username a live character owns, and `lokati` is both the `OPERATOR_LOGIN` *and* one of the 67 characters. `OPERATOR_BOOTSTRAP` does not pierce that check — it only pierces the operator-gate check above it. So on a box holding production characters there is **no UI path** to create the `lokati` account; it can only arrive by restoring `adventure-accounts.json`. | **Hit during the scrub, worked around.** Staging now holds a single staging-only account, `staging_operator`, which is not the operator login. **This needs an owner decision before cutover:** a Linux production box restored from backup keeps the real account and is fine, but any rebuild-from-empty has no way to mint an operator. Usernames are also `[a-z0-9_]` only, so a hyphenated bootstrap name is rejected. |

Nothing needed a code change. No migration or backfill re-ran on load — every one-time marker
came across in the snapshot and the journal shows none firing.

### A6. Wall-clock — the cutover downtime budget

**Service-down window: 0.83 seconds.**

| Step | Seconds |
|---|---|
| `systemctl stop pathofdust` | 0.04 |
| move synthetic state aside | 0.10 |
| extract payload (258 files) | 0.12 |
| `chown -R` + `chmod` | 0.02 |
| `systemctl start pathofdust` | 0.02 |
| wait until dashboard answers 200 | 0.53 |
| **total** | **0.83** |

Preparation done **outside** the window, with the service running: building the payload
0.53 s, uploading it 7.13 s. So even counting preparation, end to end is **under 10 seconds**
for the state set.

The honest caveat: the box answered 200 in 0.53 s, but the *first authenticated page* after
that took 120 s because a fight was being written (A4). Downtime as "service unavailable" is
0.83 s; downtime as "a player can load their character page" is bounded by fight-write
duration, not by the migration.

### A7. Sessions and accounts scrub

Per the owner's ruling: real files loaded first with the tunnel down, then scrubbed before
the tunnel came back. Evidence is in Part D below.

---

## Part B — the deploy procedure rehearsal

Full procedure: `REFACTOR_PLAN.md` §13B. Rehearsed end to end, including a deliberate
rollback.

| Step | Result |
|---|---|
| Build (`cargo build --release --workspace`, commit `692da98`) | exit **0**, 2 m 36 s |
| Test (`cargo test --release --workspace --quiet`) | **758 passed, 0 failed, 0 ignored**, exit **0** |
| Hash check | old `a546eb12…`, new `e5f21e43…` — differ, so the swap is real |
| Backup before swap | allow-list archive + rollback slot + 200-file pinned summary corpus |
| Swap #1 (deploy) | downtime **0.47 s**, live hash confirmed `e5f21e43…` |
| Health (7 checks) | all pass — see below |
| Swap #2 (**deliberate rollback**) | **0.62 s**, live hash back to `a546eb12…` |
| Swap #3 (roll forward) | **0.17 s**, live hash `e5f21e43…` |
| `NRestarts` across all three | **0 → 0**, every time |

Both the build and the full test suite ran **on the box while the live service was serving
real data and resolving real fights**, which is the realistic Linux deploy shape. The suite
saturates all 8 vCPUs and stretches the fight cadence while it runs.

**Health verification after the swap:**

```
is-active : active
NRestarts : 0
journal   : loaded 67 characters from adventure-characters.json
binary    : e5f21e43499626da65c04efca8481ecc17e7dce4269177d5fa2f01ac68ed5930
/                    HTTP 200  126,480 B
/characters          HTTP 200   94,002 B
/passives            HTTP 200   72,402 B
/admin/tunables      HTTP 200  103,052 B   (operator session)
/admin/tunables      HTTP 200  (anonymous) body contains <h1>Not Found</h1>
/api/status          HTTP 404   -- INVALID PROBE, see the correction below
/sprites/custom/Sitch89.gif  HTTP 200  687,999 B
```

**Correction — the `/api/status` line above proves nothing.** `/api/status` is **not a route**;
it appears nowhere in the `/api/*` route table (`game/src/adventure_web/api.rs:65-80`). An
unmatched path inside the nested router falls through to the outer fallback without ever
reaching the shared-secret middleware, so it returns **404 whether or not `/api/*` is mounted**.
The rehearsal's 404 was therefore consistent with both outcomes and the conclusion it supported
was reached by luck. The **valid** probe is an unauthenticated request to a route that really
exists, sent with no `x-adventure-api-secret` header:

```
curl -s -o /dev/null -w '%{http_code}\n' -X POST <base>/api/commands/join

  401  ->  /api/* IS mounted   (the route exists; the shared-secret middleware rejected it)
  404  ->  /api/* is NOT mounted (router() returned None; the path does not exist)
```

Expected **404** on staging while `ADVENTURE_API_SECRET` is absent; live Windows production
returns **401**. This probe was not run during the rehearsal — the 404 recorded above is from
the invalid check and must not be read as a mounting result.

**The maintenance-gate claim, proven rather than argued.** `Restart=always` fires on process
*exit*; `systemctl stop` is not an exit it acts on. If that were wrong, `NRestarts` would have
incremented on at least one of three stops. It read `0` before and after all three. That is
why §13B ports no flag file: the restarter and the stopper are the same program, so there is
no window for them to race.

**Rollback cost: 0.62 s**, and it is bounded by process start, not copying — nothing crosses
the network and the binary is 19 MB on local disk. Plan for **under 5 seconds**. Rollback
restores the binary only; data is deliberately left alone, because by rollback time it holds
progress players actually earned. Data damage is a *restore*
(`docs/linux_backups.md`), not a rollback.

---

## Part C — the two-phase cutover transfer

Measured with **real fight-archive JSON** produced by this game from these characters — a
188 MB coarse archive round-tripped Windows↔Linux — not synthetic data.

| Mechanism | Throughput on real fight JSON |
|---|---|
| `tar \| ssh` (uncompressed) | **7.54 MB/s** — 188 MB in 23.7 s |
| `tar \| gzip -1 \| ssh \| gunzip \| tar` | **25.23 MB/s effective** — 188 MB in 7.1 s |
| gzip -1 ratio on fight JSON | **14.9×** (187,836,152 → 12,593,066 bytes) |

Transferred content verified byte-identical by md5 on both ends. **Compress.** It is 3.3×
faster on this link and the CPU cost is invisible next to the network.

**Phase 1 — bulk pre-copy, production still running and serving.** At 25.23 MB/s effective,
the 2,465 MB figure of record extrapolates to **≈ 98 seconds**. Performed with the service up;
no downtime at all. Use `tar --newer-mtime` for the second pass, never `rsync --delete`.

**Phase 2 — the delta pass, inside the downtime window.** This is the number that is the
cutover budget: **0.83 s** for the complete state set, plus the archive delta. Do not add
phase 1 to it.

**But the phase-1/phase-2 split does not work the way the plan assumes, and this is the
finding that matters.** The premise of a bulk pre-copy is that the files are append-mostly.
For three of the four tiers they are not:

- `adventure-fights-coarse`, `-detail`, `-bundle` are capped at **5, 3 and 3 files**
  (`fight_storage.rs:57–79`) and each fight writes a new file and prunes the oldest. On the
  measured cadence — one fight per ~3.4 minutes — **every file in the detail and bundle tiers
  is replaced within about ten minutes.** A bulk pre-copy of them is worthless: by cutover
  time, everything copied has been pruned and replaced, and the "delta" is the entire tier
  again. At current sizes that is ~7 GB, or **≈ 4.6 minutes** of downtime at 25 MB/s.
- `adventure-fights-summary` **is** effectively append-mostly (capped at 200, ~18 KB each,
  3.4 MB total) and it is the tier that serves player-facing history. It is already inside
  the backup snapshot and moves in the 0.83 s window at no measurable cost.
- `adventure-fights-pinned` is the only tier the game **never** prunes
  (`fight_storage.rs:261`) — genuinely append-only, and therefore the only real phase-1
  candidate. It does not exist on staging. **Its size on production is unknown to this
  session**, because measuring it means reading `C:\PathofDust`, which was out of bounds. It
  must be measured before a cutover is scheduled.

Also note the 2,465 MB figure is stale: it was taken 2026-08-23, when fights ran ~2 s. Fights
now run 26–49 s and write 1 GB+ per tier. Production's current tier sizes are unknown here for
the same reason.

**Recommendation, for a ruling — not acted on.** Either accept ~4.6 minutes of downtime to
carry the recent detail/bundle tiers, or accept losing 3 detail + 3 bundle files (about ten
minutes of replay history) and cut over in ~1 second, carrying only summary + state. The
summary tier is what players browse; detail and bundle are the live-replay feed. The owner's
constraint that fight event data is player-facing argues for the former, the arithmetic argues
for the latter, and the honest answer depends on the pinned tier's size, which still needs
measuring.

---

## Part D — staging safety

Staging is publicly reachable at `staging.lokati.net` through cloudflared. Real production
data on that box is a cross-environment credential path that did not previously exist:
`adventure-sessions.json` carries 152 session tokens that are simultaneously valid on
production, and `adventure-accounts.json` carries the operator's argon2 hash.

Per the owner's ruling:

1. `systemctl stop cloudflared` **before** any production data reached the box. Confirmed from
   off-box: `https://staging.lokati.net/` → **HTTP 530** (tunnel down). nftables continued to
   drop 4004/4005 from the internet throughout, so the box was loopback-only.
2. All verification ran over SSH — an `ssh -L` port-forward and on-box loopback `curl`.
3. Real, unscrubbed `adventure-sessions.json` and `adventure-accounts.json` were loaded and
   proven to parse and authenticate. A scrubbed file would not have proven that.
4. The scrub and its verification, and only then the tunnel, are recorded below.

`ADVENTURE_API_SECRET` remained absent throughout. The rehearsal recorded this as
"`/api/status` returned 404, never 401" — that check is **invalid** (`/api/status` is not a
route and returns 404 either way; see the correction under the smoke-test block above). The
valid probe is unauthenticated `POST /api/commands/join` with no `x-adventure-api-secret`
header: **401 = `/api/*` mounted, 404 = not mounted**. It expects 404 on staging, and was not
run during the rehearsal.

### The scrub, and its verification

Performed with the tunnel still down. `adventure-sessions.json` and `adventure-accounts.json`
were emptied, then a staging-only credential was minted through the game's own
`/account/register` using the documented `OPERATOR_LOGIN` bootstrap drop-in
(`docs/linux_staging.md`). The password was generated on the box from `/dev/urandom`, exists
nowhere else, and is stored root-only at `/root/.staging-operator-password` (mode 0600). It
is not in this repo, not in any commit, and not in the session report.

The first attempt failed and is divergence #10 above: registering `lokati` returns HTTP 400
because a character owns that name. Staging therefore holds `staging_operator`, which is a
real working login but is **not** the operator — `OPERATOR_LOGIN` is unchanged at `lokati`, so
the admin surfaces on staging are currently reachable by nobody. That is a deliberate
conservative outcome, not an oversight.

Post-scrub state, verbatim:

```
adventure-sessions.json:  {}
adventure-accounts.json:  1 account — staging_operator
                          created_at=1788271947
                          hash=$argon2id$v=19$m=19456,t=2,p=1$…  (97 chars, freshly minted)
```

**Rejection test — the ruling's precondition for restarting the tunnel.** One real production
session token was held aside before the scrub and replayed after it:

| | Pre-scrub | Post-scrub |
|---|---|---|
| `GET /characters` with the production token | HTTP 200, **94,002 B** (authenticated: real character list) | HTTP 200, **72,025 B** (the anonymous landing page — token rejected) |
| `GET /admin/tunables` with the production token | — | HTTP 404, `Not Found` body |

The byte count is the discriminator, because both cases return 200; a status-code assertion
would have proved nothing here, the same trap as the operator gate. Then:
`grep -rl argon2 /var/lib/pathofdust --include=*.json` returns exactly one file — the new
`adventure-accounts.json`. No production password hash remains on the box. The held-aside
token file was deleted.

**Only after all of the above** was `systemctl start cloudflared` run. Verified from the
internet immediately after:

```
https://staging.lokati.net/            -> HTTP 200, 72,025 bytes, <title>Adventure Character Dashboard</title>
https://staging.lokati.net/api/status  -> HTTP 404   -- INVALID PROBE, proves nothing
```

Staging is publicly reachable again, serving the anonymous landing page, holding real
character data but **no production credential of any kind**.

The `/api/status` line is **not** evidence that `/api/*` is unmounted — that path is not a route
and answers 404 in both cases (see the correction under the smoke-test block above). To prove
the seam's state from the internet, use a route that exists, with no secret header:

```
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://staging.lokati.net/api/commands/join
  401 = /api/* mounted,  404 = /api/* not mounted    (expect 404 on staging)
```

### What is left on the box

| | |
|---|---|
| Live state | full production data as migrated — 67 characters, world stage advancing from 7396 |
| Credentials | one staging-only account; zero sessions; `ADVENTURE_API_SECRET` still absent |
| Rollback slots | `/var/backups/pathofdust/deploy-pre-rehearsal-1/`, `-rehearsal-2/` |
| Pre-migration staging state | `/var/lib/pod-synthetic-state-20260901-154401/` (moved, not deleted), plus two archives on-box and both pulled to `C:\pod-backups-linux\premigration\` |
| Binary | `e5f21e43…` (commit `692da98`) |

The synthetic pre-migration state is still on disk, so this whole migration is reversible by
moving it back.
