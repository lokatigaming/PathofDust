# Cutover runbook — Windows → Linux

**Status:** not yet executed. **Owner action required at Step 0 and at the point of no return.**
**Written:** 2026-09-01 · **Session:** CUTOVER-RUNBOOK · **Branch:** `chore/cutover-runbook`

This is the procedure for moving live production of Path of Dust from `C:\PathofDust` on
Windows to the Debian box, keeping the hostname `adventure.lokati.net`. It is written to be
executed by someone who has not read the sessions that produced it, at an awkward hour, without
having to make design decisions on the way.

Read §1 through §4 before you start anything. Everything up to and including §7 is reversible
with no player-visible effect. §8 is the point of no return.

**One variable changes: the platform.** The bot stays on Windows, stays running, and stays
connected. Removing Twitch is a separate, later, deliberate piece of work. Do not do both.

Throughout:
- `<SERVER-IP>` is the Debian box's address, kept in `C:\dust-work\.server-ip`, never in this repo.
- `<TUNNEL-UUID>` is the Linux tunnel's id. Never in this repo.
- Windows commands run in **PowerShell on the Windows box**. Linux commands run **as `root` over
  SSH on the Debian box**. Each block says which.
- No command in this document prints a secret. Keep it that way.

---

## 1. Facts of record — measured, not estimated

| Fact | Value | Where it came from |
|---|---|---|
| State-transfer downtime | **0.83 s** | rehearsed end to end, `docs/linux_deploy.md` §A6 |
| Binary swap / rollback on Linux | 0.47 s / **0.62 s** | same, §B |
| Transfer throughput, compressed | **25.23 MB/s** effective (`gzip -1`, 14.9× on fight JSON) | same, §C |
| State set actually carried | **~10 MB** — 5.7 MB top-level `.json`/`.toml` + 4 MB `adventure-fights-summary` (200 files) | measured on live production, 2026-09-01 |
| Characters | 67 | rehearsal load |
| Sessions | 152, 30-day TTL | rehearsal load |
| Churning fight tiers **not** carried | coarse 1,188 MB (5 files) · detail 3,735 MB (3) · bundle 3,775 MB (3) = **8,698 MB → 5 m 45 s** if carried | measured on live production, 2026-09-01 |
| `adventure-fights-pinned` | **does not exist on production.** 0 files, 0 bytes, 0 s to transfer | measured on live production, 2026-09-01 |

### Why the churning tiers are skipped — settled, do not re-open

`adventure-fights-coarse`, `-detail` and `-bundle` are capped at 5, 3 and 3 files
([`fight_storage.rs:57-79`](../game/src/adventure/fight_storage.rs#L57-L79)). Every fight writes a
new file and prunes the oldest, so on the current cadence **every file in those tiers is replaced
within about ten minutes.** Carrying them costs 5 m 45 s of downtime to move history that
self-replaces before you have finished verifying the cutover. **Ruling: they are not
transferred.** The accepted loss is roughly ten minutes of live-replay detail. It is not player
progress and it is not recoverable history.

`adventure-fights-summary` — the 200-file tier players actually browse — **is** carried, inside
the 0.83 s window, at no measurable cost.

`adventure-fights-pinned` is the one tier the game never prunes
([`fight_storage.rs:261`](../game/src/adventure/fight_storage.rs#L261)), created lazily the first
time a mod runs `!pinfight`. It has never been created on production. It stays a pre-flight
measurement (§5.6) because a mod could pin a fight between now and the cutover, and if they do,
that file is irreplaceable evidence.

---

## 2. The three things people get wrong about this cutover

**2.1 The ingress flip is a DNS record change, and there is no propagation delay.**
`adventure.lokati.net` is a **proxied** Cloudflare record: public resolvers return Cloudflare
anycast addresses, and `nslookup -type=CNAME adventure.lokati.net` returns no CNAME at all. The
hostname → tunnel mapping lives *behind* the proxy, inside Cloudflare's edge. Flipping it changes
nothing that any resolver on the internet has cached, because no resolver ever saw it. It takes
effect in seconds and it reverses in seconds.

**2.2 Both tunnels can never serve the hostname at once.** One proxied record points at exactly
one tunnel UUID. The Windows tunnel keeps its ingress rule and keeps running; it simply stops
receiving requests for that hostname the moment the record moves. That is deliberate — **an
untouched, still-running Windows tunnel is what makes rollback a single record edit.** Do not
stop it, do not edit its config, do not delete its rule.

**2.3 The bot's link to the game is not local, and it is not optional.**
`ADVENTURE_API_BASE_URL` is **absent** from `C:\PathofDust\.env`, so the bot uses its default
`http://127.0.0.1:4005`. The moment the game leaves Windows, that address is dead and **all 16
routes break at once** — including `GET /api/announcements/stream`, which is the entire narration
of the game in Twitch chat. This is not graceful degradation. See §3.

---

## 3. The bot: every coupling that survives the platform move

Full inventory: [`docs/bot_decoupling_audit.md`](bot_decoupling_audit.md). What matters here is
what changes when the game's address changes. Nothing about the bot's own code or its Twitch
connection is affected — only the address it dials.

| Link | What it is | After the move, before the repoint | Verdict |
|---|---|---|---|
| L1–L10 | `POST/GET /api/commands/*` — `!join`, `!character`, `!party`, `!nextencounter`, `!event intro`, `!rampage`, `!clear_battlefield`, `!give_loot`, `!gift_dust`, `!pinfight` | connection refused → bot replies *"The adventure is restarting — try again in a moment!"* to every one | **Breaks.** Cosmetically graceful, functionally total. |
| L11–L13 | `POST /api/redemptions/*` — Reforge, Repair, Force Boss channel-point rewards | bot CANCELs the redemption, refunding the points silently | **Breaks**, but refunds. No player loses points. |
| L14 | `POST /api/activity_xp` — passive XP for chatting, one call per chat message | fire-and-forget, `tokio::spawn`ed; the bot never stalls | **Breaks silently.** The passive-XP economy stops. Highest-volume link; no other source of "this person is active in chat". |
| L15 | `POST /api/published-constants` — 5 integers the wiki renders | 3 attempts, 1 s backoff, then gives up; never fails startup | **Degrades.** The wiki renders `"varies"` for five cooldown/vote-volume values. Cosmetic, already-handled fallback. |
| L16 | `GET /api/announcements/stream` — SSE, game → bot | bot reconnects every 5 s forever, gets nothing | **Breaks, and it is the worst one.** The game keeps fighting, persisting and updating the dashboard and overlay, and says **nothing in chat, ever.** |
| L17 | `ADVENTURE_API_SECRET`, shared byte-for-byte | bot **refuses to start** without it; game silently un-mounts `/api/*` without it | The single switch that arms or disarms L1–L16. |
| L19 | `bot-published-constants.json` | **already inert as a bot link.** Since 2026-08-22 the *game* writes and reads this file; no bot code touches the path | **Already inert.** Nothing to do. |
| L20 | Shared deployment root `C:\PathofDust` | ends at cutover — two roots, two log directories | **Structural, intended.** Not a break. |
| L21 | Game data files | the bot reads and writes **none** of them | **No link.** Cleanest boundary in the system. |
| L22 | "Game first, always" startup order | soft in code, hard in procedure | Preserved: §9 brings Linux up and verified *before* the bot is repointed. |
| L23 | Watchdogs | independent scripts, independent flags, neither reads the other | Game watchdog is retired at Step 0; **bot watchdog stays live and untouched.** |
| L24 | Vestigial bot config (`adventure_web_port`, `adventure_overlay_server_port`, `adventure_web_public_url`) | zero reads anywhere in the bot | **Already inert.** Leave it. |

**How long can the game run with the bot link down?** Indefinitely, without data loss. The game's
own encounter loop, persistence, dashboard, wiki and OBS overlay are all self-contained. What
stops is *everything players see in Twitch chat* and the passive XP they earn by chatting. Treat
it as minutes, not hours: a silent chat during a live stream is the most visible possible failure
of this cutover, and it is the first thing to verify (§9.4, §11).

**The repoint is not new exposure.** `/api/*` is reachable from the internet on Windows **today**
— `POST https://adventure.lokati.net/api/commands/join` with no secret header returns **401**
right now. Pointing the bot at the public hostname instead of loopback moves the secret from a
loopback hop to a TLS hop to the same already-public endpoint. It does not open anything that is
not already open.

---

## 4. What a player experiences

**Sessions survive.** The session cookie is host-only (`Path=/; HttpOnly; SameSite=Lax`, no
`Domain`), scoped to `adventure.lokati.net`, and the hostname does not change. `SESSION_TTL` is 30
days and `adventure-sessions.json` is carried in the state set. A player who was logged in before
the cutover is logged in after it, with no re-login prompt.

**A page load during the window returns a Cloudflare 502**, for about a second. The tunnel is up;
the origin behind it is not yet listening. cloudflared retries the origin on its own and recovers
without intervention.

**An in-flight fight is lost.** The game persists characters and world state at the end of each
fight — `adventure-characters.json` and `adventure-world.json` move together every cycle, and
there is no periodic autosave. A fight that is mid-resolution when the process stops leaves no
trace: no XP, no loot, no stage advance from that fight. The next fight starts from the last
persisted state.

**Can a player lose progress? Yes — bounded, and small.** The loss is everything the game
resolved between the final state copy (§8.3) and the Windows stop (§8.2). Because §8 stops the
game *before* copying, that window is **zero completed fights**, and the only loss is the single
fight that was in flight at the stop — at most one encounter's XP and loot per participant.
If the steps are executed out of order — copy first, stop second — the loss becomes every fight
in between. **Stop before you copy. That ordering is the whole reason the loss is bounded.**

**Chat goes quiet** from the Windows stop (§8.2) until the bot is repointed and verified (§9.4).
Keep that gap short; it is the most visible symptom to a live audience. Nothing is lost while it
is quiet — the fights still happen and still persist, they are just not narrated.

**Nothing a player owns can be lost** by the cutover itself. Characters, inventory, dust,
passives, crafting state and account are all in the carried state set, and the source is a
verified copy, not a move. The only deletion in this procedure is of *pruning-doomed replay
files* (§1) and *nothing else*.

---

## 5. Pre-flight checklist — all reversible, all safe with production live

Run every one. Each has an expected output; a mismatch stops the cutover.

**5.1 — Confirm production is healthy and is the thing you are moving.** *(Windows)*
```powershell
curl.exe -s -o NUL -w "site %{http_code}`n" https://adventure.lokati.net/
Get-NetTCPConnection -State Listen -LocalPort 4005 | Measure-Object | % Count
(Get-ScheduledTask -TaskName GameProcess).State
```
Expect: `site 200` · `1` or more · `Running`.

**5.2 — Confirm the hostname is proxied, so the flip has no propagation delay.** *(Windows)*
```powershell
nslookup -type=CNAME adventure.lokati.net
```
Expect: **no CNAME record** in the answer — an SOA block for `lokati.net` only. If a CNAME to
`<something>.cfargotunnel.com` *is* returned publicly, the record is not proxied, §2.1 does not
hold, and you must stop and re-plan the flip around a real TTL.

**5.3 — Confirm the API seam is mounted on Windows and answers.** *(Windows)*
```powershell
curl.exe -s -o NUL -w "%{http_code}`n" -X POST https://adventure.lokati.net/api/commands/join
```
Expect: **401**. This is the *valid* mount check — an unauthenticated request to a **real route**.
The middleware rejects before the handler runs, so it has no side effect.
**Do not use `/api/status` for this — it is not a route** and returns 404 whether the seam is
mounted or not (router table, [`api.rs:61-84`](../game/src/adventure_web/api.rs#L61-L84)).
**401 = mounted. 404 = not mounted.** See §13.

**5.4 — Confirm the Linux box is healthy and serving staging.** *(Linux)*
```sh
systemctl is-active pathofdust cloudflared
systemctl show -p NRestarts --value pathofdust
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:4005/
df -h /var/lib | tail -1
```
Expect: `active` twice · a stable `NRestarts` · `200` · **at least 20 GB free** (the churning
tiers rebuild to ~7 GB within an hour of going live).

**5.5 — Confirm the Linux binary is the one you intend to run.** *(Linux)*
```sh
sha256sum /opt/pathofdust/bin/game
systemctl show -p ExecStart --value pathofdust
```
Record the hash in the cutover log. It must match the binary built and tested per
`REFACTOR_PLAN.md` §13B for the commit you are shipping.

**5.6 — Measure the pinned tier. It is expected to be absent.** *(Windows)*
```powershell
if (Test-Path C:\PathofDust\adventure-fights-pinned) {
  Get-ChildItem C:\PathofDust\adventure-fights-pinned | Measure-Object Length -Sum |
    % { "{0} files, {1:N1} MB" -f $_.Count, ($_.Sum/1MB) }
} else { "absent - nothing to carry" }
```
Expect: `absent - nothing to carry`. **If it is present, it must be carried** — it is unpruned
bug-report evidence a mod deliberately kept, and nothing regenerates it. Add it to the state set
in §8.3 and add its size ÷ 25.23 MB/s to the downtime budget.

**5.7 — Confirm the backup you would restore from is clean.** *(Windows)*
```powershell
Get-ChildItem C:\pod-backups\PathofDust -Directory |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1 |
  % { Get-Content (Join-Path $_.FullName '_backup-manifest.json') | ConvertFrom-Json } |
  Select-Object verdict, filesCopied, filesFailed
```
Expect: `verdict = clean`, `filesFailed = 0`. This snapshot is the floor under everything below.

**5.8 — Have the four Linux env values in hand, unprinted.** You need
`TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET` and `ADVENTURE_API_SECRET` from
`C:\PathofDust\.env`, plus the literal string `https://adventure.lokati.net`. Have a way to move
them to the Linux box that does not put them in a shell history, a log, or this repo. §7.1 gives
one.

---

## 6. Step 0 — retire the Windows restarters *(OWNER, ELEVATED, REVERSIBLE)*

**This needs an elevated PowerShell prompt and is an owner action.** A deploy session's
non-elevated token gets `Access denied` from `Disable-ScheduledTask`; the maintenance-flag
mechanism used by ordinary deploys is a **30-minute lease** and cannot hold a window this long.

`GameProcess-Watchdog` fires **every 2 minutes** and will restart the game you are about to stop.
`GameProcess` itself has **Logon and Boot triggers**, so a reboot would bring the old world back
online on its own. `Disable-ScheduledTask` disables the task and every trigger it declares —
which is exactly what is wanted here.

*(Windows, elevated)*
```powershell
Disable-ScheduledTask -TaskName GameProcess-Watchdog
Disable-ScheduledTask -TaskName GameProcess
Disable-ScheduledTask -TaskName GameDataBackup
Get-ScheduledTask -TaskName GameProcess, GameProcess-Watchdog, GameDataBackup |
  Select-Object TaskName, State
```
Expect: `State = Disabled` for all three.

**Disabling a task does not stop a running instance.** The game keeps serving. Confirm it:
```powershell
curl.exe -s -o NUL -w "still live: %{http_code}`n" https://adventure.lokati.net/
```
Expect: `still live: 200`. **Production is still up and this step is fully reversible** —
`Enable-ScheduledTask` on all three puts it back.

**Why `GameDataBackup` is disabled too:** after the stop, it would keep producing hourly snapshots
of a frozen instance. Those snapshots pass every manifest check and mean nothing — they are a
photograph of a corpse, filed next to the real backups, indistinguishable by name or verdict. An
operator reaching for "the newest clean backup" in an incident three weeks from now must not find
one of these.

**`TwitchBotRS`, `TwitchBotRS-Watchdog` and `PodPullLinuxBackups` are NOT touched.** The bot keeps
running and its watchdog keeps protecting it. The two watchdog flags are separate files precisely
so suppressing the game's protection never suppresses the bot's.

---

## 7. Pre-stage the Linux side *(REVERSIBLE, INVISIBLE TO PLAYERS)*

Everything here is done with production still live on Windows. None of it changes what any player
sees. All of it is undoable.

### 7.1 The four environment values *(Linux)*

The staging unit runs with placeholder Twitch credentials, a loopback public URL, and
**`ADVENTURE_API_SECRET` deliberately absent** — `docs/linux_staging.md` says in the unit file
"Do not add it." **That instruction was correct for a staging box and is being deliberately
reversed here, for one reason: production must serve the bot.** With the secret absent,
`adventure_web/api.rs`'s `router()` returns `None`, `/api/*` is never mounted, and the bot's 16
links stay dead no matter what address it dials. A production box that does not mount `/api/*` is
a box with a permanently silent Twitch chat.

Put all four in a root-only environment file rather than `Environment=` lines, so the values are
not visible in `systemctl show`:

```sh
install -d -m 0700 /etc/pathofdust
umask 077
# Type or paste the three secrets in an editor. Do NOT echo them, and do NOT
# read them out of a shell that keeps history.
cat > /etc/pathofdust/production.env <<'EOF'
TWITCH_CLIENT_ID=
TWITCH_CLIENT_SECRET=
ADVENTURE_API_SECRET=
ADVENTURE_WEB_PUBLIC_URL=https://adventure.lokati.net
EOF
chmod 0600 /etc/pathofdust/production.env
chown root:root /etc/pathofdust/production.env
# fill in the three blanks, then:
grep -c '=$' /etc/pathofdust/production.env      # expect: 0  (no empty values left)
```

Wire it in with a drop-in. A drop-in's `EnvironmentFile` is read after the unit's own
`Environment=` lines, so it overrides the placeholders without editing the unit:

```sh
mkdir -p /etc/systemd/system/pathofdust.service.d
cat > /etc/systemd/system/pathofdust.service.d/10-production.conf <<'EOF'
[Service]
EnvironmentFile=/etc/pathofdust/production.env
EOF
systemctl daemon-reload
systemctl restart pathofdust
systemctl is-active pathofdust
```

**Verify each of the four, separately.** *(Linux, over loopback — the flip has not happened yet)*

| Value | Command | Expected |
|---|---|---|
| `ADVENTURE_API_SECRET` | `curl -s -o /dev/null -w '%{http_code}\n' -X POST http://localhost:4005/api/commands/join` | **401** — mounted. **404** means the secret is not reaching the process. |
| `ADVENTURE_WEB_PUBLIC_URL` | `curl -s -D- -o /dev/null http://localhost:4005/login \| grep -i '^location:'` | a **303** to `id.twitch.tv/oauth2/authorize?…` whose `redirect_uri` is `https%3A%2F%2Fadventure.lokati.net%2Fauth%2Fcallback` |
| `TWITCH_CLIENT_ID` | the same `location:` line | its `client_id=` matches the value in `C:\PathofDust\.env`. Compare by eye; do not paste it anywhere. |
| `TWITCH_CLIENT_SECRET` | `journalctl -u pathofdust --since -2min \| grep -iE 'twitch\|panic\|abort'` | nothing. A wrong secret is only detectable at the OAuth callback — §9.5 is where it actually gets proven. |

**Reversal:** `rm /etc/systemd/system/pathofdust.service.d/10-production.conf` and
`rm /etc/pathofdust/production.env`, `systemctl daemon-reload`, `systemctl restart pathofdust`.
Back to the staging configuration exactly.

### 7.2 The Linux tunnel ingress rule *(Linux)*

Add the production hostname to the Linux tunnel's config. **This is invisible while DNS still
points at Windows** — a tunnel with a rule for a hostname it never receives requests for does
nothing at all. Edit `/etc/cloudflared/config.yml`, the copy the unit actually reads; editing
`/root/.cloudflared/config.yml` alone changes nothing (see
[`docs/linux_ingress.md`](linux_ingress.md)):

```yaml
tunnel: <TUNNEL-UUID>
credentials-file: /root/.cloudflared/<TUNNEL-UUID>.json

ingress:
  - hostname: adventure.lokati.net
    service: http://localhost:4005
  - hostname: staging.lokati.net
    service: http://localhost:4005
  - service: http_status:404
```

Keep the `/root` copy in sync by hand. Then:
```sh
cloudflared tunnel ingress validate
cloudflared tunnel ingress rule https://adventure.lokati.net/
cloudflared tunnel ingress rule https://staging.lokati.net/
systemctl restart cloudflared
journalctl -u cloudflared --since -2min | grep -c "Registered tunnel connection"
```
Expect: `validate` OK · both `rule` calls resolve to `http://localhost:4005`, **not** the 404
catch-all · at least 2 registered connections (4 is normal).

`ingress rule` is the check that matters. `validate` only proves the YAML parses; `rule` proves
which rule a URL actually matches, which is the failure that produces a mystery 404.

**Verify from off-box that nothing changed for players** *(Windows)*:
```powershell
curl.exe -s -o NUL -w "prod still Windows: %{http_code}`n" https://adventure.lokati.net/
```
Expect: `200`, still served by Windows. If this starts returning Linux content, the record has
moved and you are past the point of no return without meaning to be — go to §10.

**Reversal:** delete the two added lines, `systemctl restart cloudflared`. Nothing observed it.

### 7.3 The operator-login gate — must pass BEFORE the flip

`adventure-accounts.json` carries over, and that is the **only** way the operator account can
exist on a box holding production characters: `do_register` refuses any username a live character
owns, and `lokati` is both the `OPERATOR_LOGIN` and one of the 67 characters. There is **no UI
path** to re-create it. If the accounts file does not land, you must find that out before the
flip, not after.

This check can only run after the state load, so it is executed at §8.5 — but it is a **gate on
the flip**, not a post-flip check. It is written here so nobody plans around it.

Over an SSH port-forward, log in at `/account/login` as the operator and load `/admin/tunables`.

Expect: `/admin/tunables` returns **200 with a real page** (~100 KB) while authenticated, and
**200 with a small `<h1>Not Found</h1>` body** while anonymous. **Compare byte counts, not status
codes** — the operator gate returns 200 either way and a status-code assertion proves nothing.

**If the operator cannot log in, do not flip.** Go to §10.1 while nothing has moved.

---

## 8. THE POINT OF NO RETURN

> **The point of no return is §8.6 — the moment the proxied DNS record for
> `adventure.lokati.net` is repointed at the Linux tunnel.**
>
> Everything before it is reversible with **no player-visible effect**: Step 0 is
> `Enable-ScheduledTask`, the env file and drop-in delete cleanly, the ingress rule is inert
> until DNS moves, and the state copy is a copy.
>
> Everything after it is reversible **in seconds**, but no longer invisibly: from §8.6 onward,
> every fight the Linux box resolves is progress a rollback discards. §10 gives the exact cost at
> each stage.
>
> The irreversible thing is not the record — the record flips back in seconds. **The irreversible
> thing is time: the fights that happen on the new platform.**

### The window opens here. Work briskly and in this order.

**8.1 — Announce, if the stream is live.** A short "world is restarting" in chat. The bot is still
connected at this point and this is the last moment it can say anything.

**8.2 — Stop the Windows game. Stop it BEFORE you copy.** *(Windows)*
```powershell
Stop-ScheduledTask -TaskName GameProcess
do { Start-Sleep -Milliseconds 500 }
  until (-not (Get-NetTCPConnection -State Listen -LocalPort 4005 -ErrorAction SilentlyContinue))
"port 4005 released"
```
Never `Stop-Process`, and never by image name — the bot runs the same image family and
`taskkill /IM` or `Stop-Process -Name` matches production by name (CLAUDE.md, PRODUCTION SAFETY).
The port poll **is** the confirmation that the process exited: a task-started process reports an
empty `Path`, so process identity cannot confirm it, and a released port can.

Copying before stopping is the one mistake in this procedure that costs players real progress:
every fight resolved between the copy and the stop is silently discarded. See §4.

**8.3 — Build the state payload.** *(Windows)*
```powershell
cd C:\PathofDust
$files = @(
  'adventure-accounts.json','adventure-sessions.json','adventure-characters.json',
  'adventure-world.json','adventure-reforge-cooldown.json','adventure-rampage-state.json',
  'adventure-live-tunables.toml','adventure-passive-overrides.toml','adventure-item-balance.toml',
  'patch-notes.json','announcements.json','bot-published-constants.json',
  'adventure-last-fight.json','adventure-sprite-count.json'
) + (Get-ChildItem 'adventure-*marker.json' | % Name) +
    (Get-ChildItem 'adventure-fights-*-seq.json' | % Name)
$files | Where-Object { Test-Path $_ } | Measure-Object | % { "$($_.Count) state files" }
```
Then tar that file list plus `adventure-fights-summary/` and the custom sprites directory. **Use
POSIX `/c/PathofDust/...` paths**, never `C:\`-style ones — POSIX tools parse `C:/…` as
`host:path` and fail with `Cannot connect to C:`. **If §5.6 found a pinned directory, add it
here.**

Two things that are easy to lose:
- **The one-time markers.** Every `adventure-*marker.json` must come across. A missing marker
  re-runs a completed migration or backfill against already-migrated data.
- **The sprites.** **5 of the 14 custom sprites are not in git**, including `lokati_gaming6.gif`,
  which a live character references. They must come from this copy or from the backup — never
  from a checkout.

**8.4 — Ship it and load it.** *(Windows → Linux)*
Compress. Measured at **25.23 MB/s effective** with `gzip -1` versus 7.54 MB/s uncompressed, a
14.9× ratio on this data. At ~10 MB the transfer is under a second either way; compress anyway,
because the habit is what makes a re-run with a pinned tier cheap. Hash the payload on both ends
and confirm the hashes match before extracting.

On the Linux box:
```sh
systemctl stop pathofdust
mv /var/lib/pathofdust /var/lib/pod-precutover-$(date +%Y%m%d-%H%M%S)   # move, never delete
install -d -o pathofdust -g pathofdust -m 0750 /var/lib/pathofdust
tar xzf /root/pod-cutover-state.tar.gz -C /var/lib/pathofdust
chown -R pathofdust:pathofdust /var/lib/pathofdust
find /var/lib/pathofdust -type d -exec chmod 0755 {} +
find /var/lib/pathofdust -type f -exec chmod 0644 {} +
chmod 0750 /var/lib/pathofdust
systemctl start pathofdust
```
**`chown` is not optional.** A tarball built on Windows extracts with numeric owner
`197108:197121`, which is nobody on Debian, and the service user then cannot write its own data
directory. This was hit in rehearsal and it is silent until the first write fails.

`templates/`, `wiki/` and `public_adventure_overlay/` are **code, not state** — they come from the
deployment, not the payload. If the move disturbed them, restore them from the deployment.

**8.5 — Verify the load before you flip.** *(Linux)*
```sh
journalctl -u pathofdust --since -2min | grep "loaded .* characters"
curl -s -o /dev/null -w "root   %{http_code}\n" http://localhost:4005/
curl -s -o /dev/null -w "chars  %{http_code} %{size_download}B\n" http://localhost:4005/characters
curl -s -o /dev/null -w "sprite %{http_code} %{size_download}B\n" \
  http://localhost:4005/sprites/custom/Sitch89.gif
curl -s -o /dev/null -w "case   %{http_code} (must be 404)\n" \
  http://localhost:4005/sprites/custom/sitch89.gif
python3 -c "import json;print('stage', json.load(open('/var/lib/pathofdust/adventure-world.json'))['stage'])"
```
Expect: the journal's character count **equal to production's**, not smaller · `root 200` ·
`chars 200` with a large body · `sprite 200 687999B` · `case 404` · the world stage matching
production's last value.

**The case check is not padding.** ext4 is case-sensitive where NTFS is not. If the lowercase
variant also returns 200, you are not on the filesystem you think you are and the sprite result
means nothing.

**Anonymous `/` is worthless as a health probe** — it returns a constant 72,025-byte landing page
that renders identically whether or not any data loaded. Use `/characters` and the journal line.

**Now run the §7.3 operator-login gate.** Do not proceed past a failure.

**8.6 — FLIP THE RECORD. This is the point of no return.**

In the Cloudflare dashboard for the `lokati.net` zone, edit the **proxied** `adventure` record so
it points at the Linux tunnel instead of the Windows one, leaving the proxy (orange cloud)
**on**. Equivalently, from the Linux box:
```sh
cloudflared tunnel route dns --overwrite-dns <LINUX-TUNNEL-NAME> adventure.lokati.net
```
**`--overwrite-dns` is banned everywhere else in this project** — every other document says never
to pass it, because it silently clobbers whatever owns a record. Here, clobbering that exact
record *is* the operation, and this is the only place in any procedure where the flag is correct.
Read the record you are about to overwrite before you run it.

Do **not** touch the Windows tunnel. It keeps its ingress rule and keeps running. That is what
makes §10.2 a seconds-long rollback.

**8.7 — Verify the flip.** *(Windows)*
```powershell
1..10 | % { curl.exe -s -o NUL -w "%{http_code} " https://adventure.lokati.net/characters; Start-Sleep 2 }
```
Expect: possibly a 502 or two, then steady **200**. Confirm the body is Linux's by comparing its
size to §8.5's `chars` byte count — not by status code, which both boxes return as 200.

If it is still serving Windows content after 60 seconds, the record did not move. Check that you
edited the proxied `adventure` record, on the right zone, and that the Linux tunnel is `active`.

---

## 9. After the flip — finish the job

**9.1 — Confirm the API seam through the public hostname.** *(Windows)*
```powershell
curl.exe -s -o NUL -w "%{http_code}`n" -X POST https://adventure.lokati.net/api/commands/join
```
Expect: **401**. A 404 means `/api/*` is not mounted on Linux — §7.1 did not take, and the bot
cannot be repointed until it does.

**9.2 — Repoint the bot.** *(Windows)* Add one line to `C:\PathofDust\.env`:
```
ADVENTURE_API_BASE_URL=https://adventure.lokati.net
```
The key is currently **absent**, so the bot is using its default `http://127.0.0.1:4005`. Adding
it is the entire repoint; no bot code changes.

**9.3 — Restart the bot.** *(Windows)*
```powershell
C:\PathofDust\maintenance-flag.ps1 -Target Bot -Set -Reason "cutover bot repoint"
C:\PathofDust\maintenance-flag.ps1 -Target Bot -Status
Stop-ScheduledTask -TaskName TwitchBotRS
do { Start-Sleep -Milliseconds 500 }
  until (-not (Get-NetTCPConnection -State Listen -LocalPort 4001 -ErrorAction SilentlyContinue))
Start-ScheduledTask -TaskName TwitchBotRS
C:\PathofDust\maintenance-flag.ps1 -Target Bot -Clear
```
Run the deployment's own copy of `maintenance-flag.ps1` by absolute path, and confirm `-Status`
prints `scope : this IS the flag 'TwitchBotRS-Watchdog' reads` before stopping anything — a copy
run from a worktree writes a flag the live watchdog never reads while still reporting
"SUPPRESSED".

The bot **refuses to start** without `ADVENTURE_API_SECRET`. If it does not come up, that value
is the first thing to check.

**9.4 — Prove the chat link end to end. Do not skip this.** *(Twitch chat)*
- Type `!party` → a real roster comes back, not *"The adventure is restarting."*
- Wait for one encounter to resolve → **the result is announced in chat, unprompted.**

`!party` proves bot → game. Only a spontaneous announcement proves game → bot over L16, the SSE
stream that carries everything players see. **You need both**, and the second takes one encounter
cycle (~3.5 min).

**9.5 — Prove Twitch login.** *(browser, logged out / private window)*
Visit `https://adventure.lokati.net/login`, complete the Twitch consent, land back logged in.
This is the only real test of `TWITCH_CLIENT_SECRET` — a wrong secret passes every check in §7.1
and fails at the callback.

**9.6 — Prove an existing session survived.** In a browser that was logged in **before** the
cutover, load the dashboard. Expect: still logged in, no re-login prompt.

**9.7 — Confirm the backup timer on Linux.** *(Linux)*
```sh
systemctl list-timers --all | grep pathofdust-backup
systemctl is-enabled pathofdust-backup.timer
```
Expect: `enabled`, with a next-run time. Linux is now the box being backed up.

**9.8 — Patch notes.** Every deploy ships an entry in `patch-notes.json` — which now lives at
`/var/lib/pathofdust/patch-notes.json`. A platform migration with a downtime window is a
player-facing event. Say plainly that roughly ten minutes of detailed fight replay was not carried
across.

---

## 10. ROLLBACK

Three stages, three very different costs. **Read the cost before you act, not after.**

### 10.1 Rollback before §8.6 — free

Nothing player-visible has happened. Undo in any order.

*(Windows, elevated)*
```powershell
Enable-ScheduledTask -TaskName GameProcess-Watchdog
Enable-ScheduledTask -TaskName GameProcess
Enable-ScheduledTask -TaskName GameDataBackup
Start-ScheduledTask  -TaskName GameProcess
do { Start-Sleep -Milliseconds 500 }
  until (Get-NetTCPConnection -State Listen -LocalPort 4005 -ErrorAction SilentlyContinue)
curl.exe -s -o NUL -w "%{http_code}`n" https://adventure.lokati.net/passives
```
Expect: `200`. Health-check a real page, not just the port. `(Get-ScheduledTaskInfo -TaskName
GameProcess).LastTaskResult` reads `267009` (`SCHED_S_TASK_RUNNING`) for a healthy long-running
game — **`0` is not the healthy value here**; a non-running failure code means the start died.

*(Linux)* `rm` the drop-in and `/etc/pathofdust/production.env`, `systemctl daemon-reload`,
`systemctl restart pathofdust`; remove the added ingress lines and `systemctl restart cloudflared`.

**Time: under a minute. Lost: nothing, except the in-flight fight from §8.2 if you got that far.**

### 10.2 Rollback after §8.6, before any fight resolves on Linux — seconds

Flip the record back. The Windows tunnel is still running with its rule intact, which is the
entire reason this is fast:
```sh
cloudflared tunnel route dns --overwrite-dns <WINDOWS-TUNNEL-NAME> adventure.lokati.net
```
or — preferably, under pressure — edit the proxied `adventure` record in the Cloudflare dashboard
back to the Windows tunnel. Then bring Windows back up per §10.1, and revert §9.2 by removing the
`ADVENTURE_API_BASE_URL` line from `.env` and restarting the bot per §9.3.

**Time: seconds for the record, under a minute for the game. Lost: the in-flight fight, plus any
fight Linux resolved while it was live.** Windows' state is exactly as it was at §8.2 — it has
been stopped, not running, so it has not forked.

### 10.3 Rollback after Linux has been live for a while — costly, and the cost is players' progress

Same commands as §10.2. The *mechanism* has not got slower. What has changed is what it discards:
**every fight, level, drop, crafted item and stage advance that happened on Linux is gone.**
Windows resumes from the moment of §8.2 as if the intervening time never happened. Players who
earned something in that window lose it, visibly, and there is no way to merge the two histories.

At the current cadence — roughly one encounter every 3.5 minutes — an hour on Linux is about 17
encounters of progress across the whole party.

**Rolling back at this stage is a decision about players' work, not about infrastructure.** Do it
for data corruption, or for a defect that is actively destroying progress. Do not do it for
anything that can wait for a fix rolled forward on Linux.

**The alternative is a roll-forward, and on Linux that is fast:** the rehearsed binary swap is
**0.62 s**, `Restart=always` does not fight an explicit `systemctl stop` (proven — `NRestarts`
stayed 0 across three rehearsed swaps), and the rollback script restores the previous binary
without touching data. **If the problem is the build rather than the platform, swap the binary and
keep the progress.**

### What a rollback never does

A rollback restores the **binary and the ingress**, never the data. Data damage is a **restore**
([`docs/linux_backups.md`](linux_backups.md)) — a different, slower, more deliberate operation.
Never reach for a rollback to fix corrupted state.

---

## 11. How do I know it worked

### At 5 minutes

| Check | Good | Roll back if |
|---|---|---|
| `https://adventure.lokati.net/characters` | 200, full roster, same size as §8.5 | 5xx, or a roster short of the §8.5 count — **missing characters is a data fault; roll back immediately** |
| Chat | `!party` answers **and** one encounter has announced itself unprompted | still *"The adventure is restarting"* after §9.3 — that is a repoint fault. **Fix it forward; do not roll back for it.** |
| `systemctl show -p NRestarts --value pathofdust` | `0` | climbing — the process is crash-looping. Read `journalctl -u pathofdust` once; roll back if the cause is not obvious. |
| A pre-cutover browser session | still logged in | logged out — `adventure-sessions.json` did not land. Roll back rather than force 152 people to re-authenticate. |

### At 1 hour

| Check | Good | Roll back if |
|---|---|---|
| Fights resolved | ~17 encounters, stage advancing, no gaps in the fight list | not resolving at all — **or resolving in ~2 s**, which means the pacing controllers loaded wrong and every fight is being trivialised. Both are roll-back faults. |
| `df -h /var/lib` | churning tiers rebuilt to ~7 GB and **plateaued** | still climbing past ~10 GB — pruning is not working. Investigate before the disk fills. |
| `journalctl -u pathofdust -p warning --since -1h` | the known-harmless `lingeringEffect` "retired affix with no live base value" line, and little else | repeated write or permission failures → §8.4's `chown` did not take |
| Twitch login | a new player can log in and join | broken → `TWITCH_CLIENT_SECRET`. Costs new sign-ups only; **fix forward.** |
| Response times | page loads well under a second | **Known unknown:** rehearsal saw one unexplained 120-second stall on the first authenticated request after a migration load, never reproduced across a 660 s probe spanning a complete fight write. If it recurs, record it — it is a known-unknown, not a new fault. **Do not roll back for a single slow first request.** |

### At 24 hours

| Check | Good | Roll back if |
|---|---|---|
| `systemctl show -p NRestarts --value pathofdust` | still `0` | any restart you did not cause |
| Backups | one `pathofdust-backup` run completed and restore-verified | none ran → §9.7 |
| Off-box pull | `PodPullLinuxBackups` ran overnight and Windows holds a copy | nothing arrived — **the pull is now the only off-box copy of anything.** Treat as an incident; it is not a rollback trigger. |
| Disk | flat | still growing 24 h in |
| Player reports | nothing about missing items, lost levels or vanished characters | **any credible report of lost progress.** This is the one class of problem worth the cost of §10.3. |

**By 24 hours with all of the above clean, the cutover is done.** A rollback after that point is
no longer a rollback; it is a restore-and-migrate in the other direction, and it must be planned
as its own piece of work.

---

## 12. Standing rules after cutover

### 12.1 Windows stays warm-and-STOPPED. It never runs again on its own.

**RULING.** After cutover, `C:\PathofDust` stays installed and intact, and the game there stays
**stopped**, with `GameProcess` and `GameProcess-Watchdog` **disabled**.

**Do not "helpfully" restart it.** A running Windows instance is not a harmless warm spare. Its
own `spawn_encounter_loop` keeps resolving fights every ~3.5 minutes, writing characters, world
state and fight archives, and advancing the stage — against a world nobody is playing. Within an
hour it has forked into a parallel history. That destroys the one property that makes rollback
cheap: that Windows' state is a **clean snapshot of the moment of cutover**. A forked Windows is
not a rollback target, it is a second world you now have to choose between, and choosing means
somebody loses progress either way.

Stopped, it is a perfect frozen restore point. Running, it is a liability that grows every three
and a half minutes.

If you find `GameProcess` running after cutover: stop it, re-disable it, and check the state-file
mtimes to see how far it forked.

### 12.2 What Windows is for now

Two things, and nothing else:

1. **The off-box backup target.** `PodPullLinuxBackups` **stays enabled and stays running.** It is
   the only copy of the Linux box's backups that is not on the Linux box. Nightly. If it stops
   arriving, that is an incident.
2. **The standby.** The frozen cutover-moment state, available for §10.

It is no longer the game server, no longer the thing being backed up (`GameDataBackup` is
disabled — §6), and no longer where anyone deploys.

The **bot** still runs on Windows and is a third, separate thing: a live production process, with
a live watchdog, untouched by all of the above.

### 12.3 How long to keep Windows warm

**Decision point: 30 days.** That is `SESSION_TTL`. After 30 days every session that predates the
cutover has expired on its own, so the last piece of state that was *carried* rather than
*created* on Linux is gone — and a rollback could no longer restore a coherent player experience
even in principle.

**Keep it beyond 30 days if** any of these is still true: a data fault has been reported and not
diagnosed; the Linux box has had an unplanned restart you cannot explain; the off-box pull has not
completed a full 7-day cycle; or you have not yet restored a Linux backup and verified it end to
end.

**What would actually make you use it:** discovering that migrated data is wrong in a way Linux
has since been building on top of — a character's inventory, dust balance or passive allocation
that did not survive and has been written over since. That is the only failure a rollback fixes
better than a forward fix, because it is the only one where the *older* state is the *correct*
one.

**When you retire it, retire it deliberately:** confirm a Linux backup restores end to end,
confirm the off-box pull is healthy, then archive `C:\PathofDust` — do not delete it while it is
the only copy of anything. `C:\pod-qa` and `C:\sync-pod-qa.ps1` are untouched by all of this;
they are the owner's to decide about separately.

---

## 13. Known-defective check — do not copy it forward

**`/api/status` is not a route.** It never was. It returns **404 whether or not `/api/*` is
mounted**, because an unmatched path inside the nested router falls through to the outer fallback
without ever reaching the shared-secret middleware. The mounted routes are `/api/commands/*` (10),
`/api/redemptions/*` (3), `/api/activity_xp`, `/api/published-constants` and
`/api/announcements/stream` — router table at
[`api.rs:61-84`](../game/src/adventure_web/api.rs#L61-L84).

Any check of the form *"`/api/status` returned 404, so the API is not mounted"* proves nothing. It
was recorded that way in [`docs/linux_ingress.md`](linux_ingress.md) — **corrected on this
branch** — and in three places in `docs/linux_deploy.md` on `chore/linux-deploy-proc`, which is
**not on this branch and must be corrected when it merges** (lines 295, 385 and 430 as of
`d913da9`).

**The valid probe, used throughout this runbook:**

```
POST https://adventure.lokati.net/api/commands/join     (no x-adventure-api-secret header)

  401  ->  /api/* IS mounted      (the shared-secret middleware rejected the request)
  404  ->  /api/* is NOT mounted  (router() returned None; the path does not exist)
```

Safe to run against live production: the middleware rejects before the handler runs, so the
request has no side effect. Verified against live Windows production on 2026-09-01 — **401**.
