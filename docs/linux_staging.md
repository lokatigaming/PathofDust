# Linux staging instance — Debian 13

**Date:** 2026-08-31 · **Session:** LINUX-STAGING · **Branch:** `chore/linux-staging`
**Commit deployed:** `153e804` · **Binary sha256:** `bd700b6e2fab63a3ed75b4e2a92390c47cf66df7ef064c67f3109b12bf9a1120`

Follows `docs/linux_build_gate.md` (e90ec8a), which established that the workspace builds and
passes 755/755 on this box with zero code changes. That gate never ran the game; this
document covers the *running* instance — layout, service unit, and deploy procedure.

**No production data reached this box.** No characters, sessions, accounts, fight history,
`.env`, token or secret was copied. The instance started empty, which for World 2 is the
correct starting state rather than a throwaway. Every file under `/var/lib/pathofdust` was
either created by the instance itself or is a git-tracked asset. Windows production was not
touched and kept running throughout.

The source already on the box was confirmed byte-identical to `153e804` before deploying: a
fresh `git archive HEAD` was extracted alongside it and `diff -rq` reported no source
difference (only the fight-storage directories the gate's own test run left behind). The
existing `cargo build --release --workspace` was re-run and finished in 0.28 s as a no-op,
confirming the installed binary is current for that commit.

## Twitch-free by construction

`ADVENTURE_API_SECRET` is gone from the unit, and as of 2026-09-02 the seam it switched is
gone from the source: `adventure_web/api.rs` is deleted, along with the `/api` nest and the
`api_secret` parameter. `/api/*` now 404s unconditionally on every box, with any environment
— there is no longer a code path that can return 401.

This section used to describe a *runtime* switch over a router that still shipped in the
binary. It now describes a deletion. **The key does nothing; do not add it.**

## Layout

| Path | Owner / mode | Contents |
|---|---|---|
| `/opt/pathofdust/bin/game` | `root:root` `0755` | the game binary, replaced on deploy |
| `/opt/pathofdust/bin/deploy.sh` | `root:root` `0755` | the deploy procedure below |
| `/var/lib/pathofdust/` | `pathofdust:pathofdust` `0750` | **all mutable state** — the systemd `WorkingDirectory` |
| logs | — | journald (`journalctl -u pathofdust`) |

The service user is `pathofdust` (uid 988), a system account with `/usr/sbin/nologin` and no
password. It owns `/var/lib/pathofdust` and nothing else; `/opt/pathofdust` is root-owned and
the service only reads from it.

The game resolves its persisted files CWD-relative — `adventure::data_path` falls back to an
empty base when `GAME_DATA_DIR` is unset, see `game/src/adventure/paths.rs` — so
`WorkingDirectory=/var/lib/pathofdust` lands every data file there with no code change and
without setting `GAME_DATA_DIR` at all.

### The checked-in assets

Three directories are read relative to CWD and must be present in the data directory. Derived
from the code, not guessed:

| Directory | Read by |
|---|---|
| `templates/` | `adventure_web/render.rs` — `TEMPLATE_DIR` |
| `wiki/` | `adventure_web/wiki.rs` — `WIKI_MD_DIR` |
| `public_adventure_overlay/` | `adventure_overlay_server.rs` root, plus the `/sprites` and `/skill-effects` `ServeDir` mounts in `adventure_web.rs` |

`adventure-live-tunables.toml` and every `adventure-*.json` are **runtime** files, not checked
in. They are created on first run and a deploy must never ship them.

## The deploy procedure — and the sprite trap

`/opt/pathofdust/bin/deploy.sh <source-root>` installs the binary and refreshes the assets.

**Ledger #60.** `public_adventure_overlay/sprites/custom/` holds *player-uploaded* sprites
that exist nowhere else — on the live Windows box there are 14 files there and git tracks 9.
Because `WorkingDirectory` is the data directory, that tree lives under `/var/lib/pathofdust`,
so a careless refresh eats player uploads.

**How this procedure guarantees it cannot:** the refresh is `cp -r "$SRC/$d/." "$DATA/$d/"`.
`cp -r` only ever creates or overwrites — it has no mechanism to remove a destination entry
that is absent from the source. There is deliberately **no `rsync --delete` and no `rm -rf`**
anywhere in the script, so an untracked upload is not merely spared, it is unreachable.

Verified empirically rather than argued: an untracked
`sprites/custom/zz-staging-upload-proof.png` was created, a tracked template was tampered
with, and `deploy.sh` was re-run. The upload survived; the tampered template was refreshed.
Both halves matter — a refresh that spares uploads by not refreshing anything at all would
pass the first check and fail the second.

Replacing the binary while the service runs is safe here (`mv` over a running binary's inode);
the Windows file-lock dance has no Linux equivalent.

```sh
#!/bin/bash
set -euo pipefail
SRC="${1:?usage: deploy.sh /path/to/source-root}"
BIN=/opt/pathofdust/bin
DATA=/var/lib/pathofdust

install -o root -g root -m 0755 "$SRC/target/release/game" "$BIN/game.new"
mv -f "$BIN/game.new" "$BIN/game"

for d in templates wiki public_adventure_overlay; do
  mkdir -p "$DATA/$d"
  cp -r --preserve=mode,timestamps "$SRC/$d/." "$DATA/$d/"
done

chown -R pathofdust:pathofdust "$DATA"
```

Then `systemctl restart pathofdust`.

## The systemd unit

`/etc/systemd/system/pathofdust.service`, verbatim:

```ini
[Unit]
Description=Path of Dust - game server (Linux staging)
Documentation=file:///opt/pathofdust/bin/deploy.sh
StartLimitIntervalSec=300
StartLimitBurst=5
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pathofdust
Group=pathofdust

ExecStart=/opt/pathofdust/bin/game
WorkingDirectory=/var/lib/pathofdust
StateDirectory=pathofdust
StateDirectoryMode=0750

# Replaces game-watchdog.ps1. `always` rather than `on-failure` because the
# watchdog it replaces restarted the game whenever it died, clean exit or not;
# an explicit `systemctl stop` still never triggers a restart. The burst limit
# stops a crash-on-startup from spinning forever - it gives up after 5 tries in
# 5 minutes and stays down, visible in `systemctl status`.
Restart=always
RestartSec=5s

# journald is the log of record. The binary ALSO writes logs/game.log via its
# tracing_appender layer (main.rs) - that is in the code, not configurable
# here, and lands under WorkingDirectory like every other data file.
StandardOutput=journal
StandardError=journal
SyslogIdentifier=pathofdust

# TWITCH_CLIENT_ID, TWITCH_CLIENT_SECRET, ADVENTURE_API_SECRET and
# ADVENTURE_WEB_PUBLIC_URL were all REMOVED from this unit on 2026-09-02
# (TWITCH-REMOVAL-GAME). This comment has now been corrected twice in the same
# direction, which is the point: it once said the two Twitch keys were REQUIRED
# and that main.rs aborts without them; then that they were OPTIONAL
# placeholders. Both are now moot - no code reads any of the four. The Twitch
# OAuth login, the /api/* seam and the redirect_uri that was
# ADVENTURE_WEB_PUBLIC_URL's only consumer are deleted from the source, not
# merely unmounted by absent configuration. Do not re-add any of them; a
# variable no code consumes reads as meaningful to whoever finds it next.
Environment=OPERATOR_LOGIN=lokati
Environment=ADVENTURE_WEB_PORT=4005
Environment=ADVENTURE_OVERLAY_SERVER_PORT=4004

NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictNamespaces=true
LockPersonality=true
MemoryDenyWriteExecute=false
ReadWritePaths=/var/lib/pathofdust

[Install]
WantedBy=multi-user.target
```

`StartLimitIntervalSec`/`StartLimitBurst` belong in `[Unit]`, **not** `[Service]` — placed in
`[Service]` systemd logs `Unknown key ... ignoring` and the burst limit is silently inert.
This unit had exactly that bug on its first start; it is fixed above and confirmed with
`systemctl show -p StartLimitIntervalUSec -p StartLimitBurst`.

### Two environment notes

**`TWITCH_CLIENT_ID` / `TWITCH_CLIENT_SECRET` no longer exist.** This paragraph
previously said they were required to start at all and that `main.rs` aborts without
them. That was true when written, then became false when they were made optional
(2026-08-31), and is now moot: they were deleted from the source entirely on
2026-09-02 along with the Twitch OAuth login. Nothing reads them; nothing should set
them. `ADVENTURE_WEB_PUBLIC_URL` went the same way — its only consumer was the OAuth
`redirect_uri`. Local `/account/register` is the only identity path, on staging and in
production alike.

**Operator bootstrap ordering.** `accounts.rs:147` refuses to register any username equal to
the current `OPERATOR_LOGIN`, so with `OPERATOR_LOGIN=lokati` live you cannot register
`lokati` — a chicken-and-egg on a fresh instance. Bootstrap by pointing `OPERATOR_LOGIN`
elsewhere with a temporary drop-in, registering the operator account, then removing it:

```sh
mkdir -p /etc/systemd/system/pathofdust.service.d
printf '[Service]\nEnvironment=OPERATOR_LOGIN=operator-bootstrap-unused\n' \
  > /etc/systemd/system/pathofdust.service.d/99-bootstrap.conf
systemctl daemon-reload && systemctl restart pathofdust
# ...register the operator account through /account/register...
rm -rf /etc/systemd/system/pathofdust.service.d
systemctl daemon-reload && systemctl restart pathofdust
```

## No public ingress

`adventure_web.rs:254` and `adventure_overlay_server.rs:52` both `bind(("0.0.0.0", port))`,
hardcoded. This box has a public IP and shipped with an empty firewall ruleset, so simply
starting the service would have published the staging dashboard to the internet. A
bind-address change is a code change and out of scope, so ingress is blocked at the host
firewall instead:

```sh
nft add table inet pod
nft add chain inet pod input '{ type filter hook input priority 0; policy accept; }'
nft add rule inet pod input iif lo accept
nft add rule inet pod input tcp dport '{ 4004, 4005 }' drop
nft list ruleset > /etc/nftables.conf
systemctl enable --now nftables.service
```

The chain policy stays `accept` and only the two game ports are dropped, so this rule cannot
cost anyone SSH. Loopback is accepted first, so `curl` on the box and an SSH port-forward
(`ssh -L 4005:127.0.0.1:4005 root@<box>`) both work. Verified from off-box: both ports time
out. Rules are persisted to `/etc/nftables.conf` and survive reboot, confirmed.
`adventure.lokati.net` still points at Windows — no cloudflared, no DNS, no ingress.

## Verification

| # | Check | Result |
|---|---|---|
| 6 | Dashboard renders | `GET /` → 200, 72,025 bytes, `<title>Adventure Character Dashboard</title>`. Overlay `GET :4004/overlay.html` → 200 |
| 7 | `/api/*` un-mounted | `/api/commands/character`, `/api/commands/join`, `/api/activity_xp`, `/api/published-constants` → **404**, not 401. Still 404 with an `Authorization` header — the absence of a route, not the rejection of a caller |
| 8 | Local identity | `POST /account/register` (`tester1`) → 302 + `adv_session` cookie; `POST /account/login` → 302; dashboard renders as `tester1`. `adventure-accounts.json` and `adventure-sessions.json` written under `/var/lib/pathofdust` |
| 9 | Encounter loop | Fight #1 resolved on the natural `BASIC_ENCOUNTER_INTERVAL` (180 s) schedule with no forcing, and persisted across all four storage tiers |
| 10 | Operator gate | `lokati` gets the tunables form; `tester1` and anonymous get the `Not Found` card |
| — | `kill -9` | PID 10889 → 11188; `Scheduled restart job, restart counter is at 1`; dashboard 200 within 9 s |
| — | Reboot | Box rebooted; back in ~30 s, service `enabled` and `active` unprompted, dashboard 200, nftables rules persisted, characters and fight history intact |

**A correction worth recording on check 10.** The operator gate does **not** return HTTP 404.
`adventure_web.rs:2965-2976` renders a 200 page whose *body* is
`<div class="card"><h1>Not Found</h1></div>`. A status-code assertion therefore passes for
everyone and proves nothing — this gate can only be tested on page content. (`/api/*` in
check 7 genuinely is a 404; the two are different mechanisms and it is easy to assume they
match.)

## Where a fight lands, and the season-1 baseline

Fight #1 — two-character party, stage 0. Fight storage is tiered, and every tier landed under
`/var/lib/pathofdust`, which is the proof the layout holds:

| File | Size |
|---|---|
| `adventure-fights-summary/fight-0000000001.json` | 540 B |
| `adventure-fights-coarse/fight-0000000001.json` | 6,579 B |
| `adventure-fights-detail/fight-0000000001.json` | 24,242 B |
| `adventure-fights-bundle/fight-0000000001.json` | 25,579 B |
| **per fight, all tiers** | **≈ 57 KB** |

Alongside them: the four `adventure-fights-*-seq.json` counters,
`adventure-characters.json`, `adventure-accounts.json`, `adventure-sessions.json`, the
one-time migration/backfill markers the first run wrote, and `logs/game.log.2026-08-31`.

After six fights the tiers hold different counts, which is tiered retention working rather
than a fault — the cheap tiers keep everything and the expensive ones do not:

| Tier | Total | Files retained (of 6 fights) |
|---|---|---|
| summary | 3,242 B | 6 |
| coarse | 30,470 B | 5 |
| detail | 69,247 B | 3 |
| bundle | 73,086 B | 3 |

**Season-1 baseline.** ~57 KB for a freshly written fight across all four tiers, with a
two-character party at stage 1, is trivially small — and retention makes steady-state growth
slower still. Total game state went 216 K → 380 K across those six fights, on a box with
297 G free. Both numbers grow with party size and stage; this is the floor to compare future
measurements against.

Footprint of `/var/lib/pathofdust`:

| | |
|---|---|
| Total | 39 M |
| `public_adventure_overlay/` | 38 M (checked-in sprite and effect art — the dominant term, and static) |
| `wiki/` | 128 K |
| `templates/` | 80 K |
| **Game state only** (excluding the three asset dirs) | **216 K** |

## What this settles, and what it does not

Settled: the game runs on Linux as an unprivileged systemd service, persists correctly into a
single data directory, serves its dashboard and overlay, registers and authenticates local
accounts, resolves and stores fights on its own schedule, gates its operator surfaces, and
comes back from both `kill -9` and a reboot without help.

Not settled, and out of scope here: no backup script yet (the Windows `backup-game-data.ps1`
has no Linux counterpart on this box), no `REFACTOR_PLAN.md` §13 rewrite, no public ingress,
and no migration of any production data. The instance holds only accounts created during this
verification.
