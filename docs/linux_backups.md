# Linux staging backups — script, timer, and the off-box pull

**Date:** 2026-08-31 · **Session:** LINUX-BACKUPS · **Branch:** `chore/linux-backups`

Ports `backup-game-data.ps1` to the Debian box as `backup-game-data.sh` plus a systemd service
and timer, and stands up the Linux half of an off-box copy. Continues
[`docs/linux_staging.md`](linux_staging.md) (the instance) and
[`docs/linux_ingress.md`](linux_ingress.md) (the tunnel). Before this, nothing on the Linux box
backed anything up.

`<SERVER-IP>` is the Debian box's address, kept in `C:\dust-work\.server-ip` and not in this
repo.

## Where things live

| Path | What |
|---|---|
| `/opt/pathofdust/bin/backup-game-data.sh` | the backup script (repo root: `backup-game-data.sh`) |
| `/opt/pathofdust/bin/backup-pull-shell` | forced command for the off-box pull key (repo root: `backup-pull-shell`) |
| `/etc/systemd/system/pathofdust-backup.service` | oneshot that runs the script |
| `/etc/systemd/system/pathofdust-backup.timer` | daily, `03:15` + up to 5 min jitter, `Persistent=true` |
| **`/var/backups/pathofdust/`** | **the archives** — `pod-backup-<stamp>.tar.gz` + `.sha256` |
| journald, `pathofdust-backup` | the log of record — `journalctl -u pathofdust-backup` |

One archive measured **2.7 MB / 122 files** on 2026-08-31.

### Installing the script from the repo

```sh
install -o root -g root -m 0755 <src>/backup-game-data.sh /opt/pathofdust/bin/
install -o root -g root -m 0755 <src>/backup-pull-shell   /opt/pathofdust/bin/
install -o root -g root -m 0644 <src>/pathofdust-backup.service /etc/systemd/system/
install -o root -g root -m 0644 <src>/pathofdust-backup.timer   /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now pathofdust-backup.timer
```

`.gitattributes` pins `*.sh`, `*.service`, `*.timer` and `backup-pull-shell` to `eol=lf`. It has
to: this repo has `core.autocrlf=true`, and without those rules a fresh Windows checkout hands
you a script whose shebang ends `\r` and a Linux box that answers `bad interpreter:
/bin/bash^M`. If you ever copy one of these out of a Windows working copy by some other route,
pipe it through `tr -d '\r'` first.

## What is backed up

An **allow-list derived from the code**, not a glob, carried over from the PowerShell original
in the same order so the two can be diffed. Four groups: 13 core state files, 23 one-time
migration/giveaway markers, the 4 fight-tier sequence counters, and two directories —
`adventure-fights-summary` (the tier that serves player-facing history, capped at 200 by
`SUMMARY_FIGHTS_CAPACITY`) and `public_adventure_overlay/sprites/custom` (player uploads, which
exist nowhere else).

**Marker drift is checked every run.** The marker list was derived from the code, so a release
that adds one would silently fall out of the set. The script globs `adventure-*-marker.json`,
backs up anything it finds regardless, and logs `MANIFEST DRIFT` naming the files so the list
gets updated. Same durable lesson CLAUDE.md records for form POSTs: derive the set from reality,
do not hand-maintain it and hope.

> **Standing rule, from `world2_build_plan.md`:** any new persisted state file must be added to
> the backup allow-list **in the same branch that creates it.** `adventure-accounts.json` was
> missed on first delivery and caught in review; account loss is unrecoverable.

## What is not backed up, and why

136 files under `/var/lib/pathofdust` are deliberately outside the archive:

| Not covered | Count | Why |
|---|---|---|
| `public_adventure_overlay/` except `sprites/custom` | 108 | git-tracked assets; `deploy.sh` reinstalls them |
| `wiki/`, `templates/` | 16 | git-tracked; the repo is their backup |
| `adventure-fights-coarse` / `-detail` / `-bundle` | 11 | bulk archives, excluded by size — 2,465 MB live on Windows |
| `logs/` | 1 | journald is the log of record; `logrotate.timer` already handles it |

`adventure-fights-pinned` is the one fight tier the game **never** prunes. It does not exist on
this box yet; if it ever holds files the script logs a `NOTE` naming the count, so it cannot
quietly look covered.

## Live snapshots: what can and cannot tear

The game is **never stopped** to back up, and the script contains no process code at all — it
does not start, stop, query or enumerate one (CLAUDE.md production safety).

State writes go through `state::write_atomic`: temp file in the same directory → `sync_all` →
`rename` → parent-directory fsync on unix. `rename(2)` is atomic, so a reader sees either the
whole old file or the whole new one, never a mixture.

**Two files were not on that path until 2026-08-31.** `tunables.rs:639` and
`passive_overrides.rs:193` used plain `std::fs::write`, which truncates and then writes: an
admin save on `/admin/tunables` opened `adventure-live-tunables.toml` with `O_TRUNC` and a
backup reading it in that window would have archived zero bytes, or a prefix ending mid-array,
**with a perfectly correct checksum of the truncated file.** Both now route through
`state::save_text`. This document's sibling commit carries that fix; **the bug is still live on
Windows production** until that ships through the normal deploy path.

**Two hazards remain, and the script handles them by construction:**

1. **`.tmp` siblings.** `write_atomic` names its temporaries `<stem>.<pid>.<n>.tmp` in the
   target's own directory, so a directory sweep can capture a half-written temp. Every directory
   walk excludes `*.tmp`, and the script asserts none reached the staging tree before archiving.
2. **Sequence counter vs fight directory.** `next_seq` (`fight_storage.rs:87`) persists the
   counter *before* writing the fight file it names, so on disk the counter is always ≥ the
   highest file present. **The script copies fight directories BEFORE the seq counters** — copy
   the counter first and a fight written in between lands in the archive while the archived
   counter still points below it, so restoring that pair makes the next fight overwrite a fight
   you just restored. Copying it last keeps the captured counter ≥ the captured directory, which
   is the safe direction.

**What is accepted:** there is no snapshot isolation *across* files. This is ext4 on a plain
partition — no LVM or btrfs snapshot to take. `adventure-characters.json` and
`adventure-world.json` are read microseconds apart and can land either side of a save. Every
file is individually valid; the *set* can be skewed by about one fight. The PowerShell original
accepts the same thing.

## Integrity checking

Every file is copied into a staging tree, verified **there**, hashed there, and only then
archived — so the manifest describes exactly what is in the tarball, with no read-it-twice race.
A verify failure retries the copy (3 attempts, 750 ms apart), because a failure almost always
means the copy landed mid-write.

- **zero length is a hard failure** — nothing in the manifest is written by anything that can
  legitimately produce zero bytes, so an empty result is a truncated write caught in flight
- NUL bytes rejected; strict UTF-8 decode (a half-written multibyte sequence at a truncation
  boundary fails rather than becoming U+FFFD); UTF-8 BOM tolerated and stripped before parsing
- `.json` → real `json.loads`; **`.toml` → real `tomllib.loads`** (python3.13 is on the box; the
  PowerShell version had no TOML parser and had to settle for bracket balance)
- `adventure-accounts.json` gets a shape check: every entry must carry an `$argon2` password
  hash. `{}` passes deliberately — that is the legitimate state before anyone registers. A lost
  hash has no external identity provider to re-authenticate against, so the account is gone.

**Three ways this is stricter than the Windows script.** The PowerShell version copies its two
`$SmallDirs` **blind** — no parse, no retry, successes never recorded in the manifest — and a
*missing* directory is skipped with no log and no failure, which yields a `clean` verdict over a
snapshot containing **zero sprites**. Here every JSON member is parsed, every member is recorded
with its SHA-256, and a missing required directory is a hard failure. (Checked 2026-08-31: the
latest Windows snapshot holds all 14 sprites against 14 live, so the defect is latent there, not
fired.)

After writing, the archive must prove it reads back: `gzip -t`, `sha256sum -c` against the
sidecar, and the member count compared to the staged count. Any mismatch aborts.

## Retention

**One snapshot per day, newest 90 kept**, pruned only at the end of a clean run.

The Windows script keeps 30 days. That number was **disk-driven** — chosen when including the
bulk fight tiers threatened ~130 GB against 165 GB free. That constraint does not exist here:
the Linux set excludes those tiers and is 2.7 MB, so 90 days costs ~270 MB against 297 GB free,
**0.09% of the disk.** With disk contributing nothing to the decision the only remaining input
is detection latency, and that argues in one direction: the backup tier you reach for days later
is the one you are reaching for *because nobody noticed at the time*. 90 days covers a World 2
season.

Two rules carried over unchanged, both of which matter most on the worst day:

- **A degraded run never prunes.** A run that could not produce a good snapshot is exactly when
  an incident may be under way, and the worst possible moment to delete older copies. The
  archive is still written and labelled `degraded`, and the script exits non-zero.
- **A plan leaving zero verified archives aborts** rather than proceeding. An archive counts as
  verified only if its sidecar checksum matches **and** its embedded manifest says `clean` — a
  degraded archive is kept, but can never be the reason pruning decides there is good history to
  fall back on.

A manual run consumes one of the 90 slots.

## Failure is loud

The script is `set -Eeuo pipefail` with an `ERR` trap, exits non-zero on any failure, and logs
to journald through the unit. A failed run leaves the unit in `failed` state, visible in
`systemctl list-units --failed`. A silent partial backup is worse than no backup.

Exercised on 2026-08-31 against a source directory missing a required directory: logged
`MISSING REQUIRED DIRECTORY`, marked the run `degraded`, logged `PRUNE SKIPPED`, and exited **1**
with the older archives untouched.

---

# Restore procedure

**Read this part first when something has gone wrong.** Nothing here writes to
`/var/lib/pathofdust` until the last step, which is the only destructive one.

### 1. Pick an archive

```sh
ls -1 /var/backups/pathofdust/pod-backup-*.tar.gz
```

Names are `pod-backup-YYYYmmdd-HHMMSS.tar.gz` in local time. If the damage has a known time,
take the **newest archive from before it** — not the newest overall.

### 2. Prove the archive before trusting it

```sh
A=/var/backups/pathofdust/pod-backup-20260831-154537.tar.gz
cd "$(dirname "$A")" && sha256sum -c "$(basename "$A").sha256"
gzip -t "$A" && echo "archive intact"
tar -xzOf "$A" ./_backup-manifest.json | python3 -m json.tool | head -20
```

The manifest's `verdict` must be `clean`. If it says `degraded`, `filesFailed` and the per-entry
`reason` fields say exactly what is missing — decide deliberately, do not restore blind.

### 3. Extract to scratch — never straight over the live directory

```sh
rm -rf /tmp/pod-restore && mkdir -p /tmp/pod-restore
tar -xzf "$A" -C /tmp/pod-restore
find /tmp/pod-restore -type f | wc -l
```

### 4. Verify the extraction against the manifest

Every member, by SHA-256:

```sh
python3 - /tmp/pod-restore <<'PY'
import hashlib, json, os, sys
root = sys.argv[1]
m = json.load(open(os.path.join(root, "_backup-manifest.json")))
bad = 0
for e in m["entries"]:
    p = os.path.join(root, e["name"])
    if not os.path.exists(p):
        print("MISSING", e["name"]); bad += 1; continue
    if hashlib.sha256(open(p, "rb").read()).hexdigest() != e["sha256"]:
        print("CORRUPT", e["name"]); bad += 1
print("verdict:", m["verdict"], "| entries:", len(m["entries"]), "| problems:", bad)
PY
```

### 5. Restore

The game **must** be stopped for this step — it holds state in memory and would write it back
over anything you restore.

```sh
systemctl stop pathofdust

# Keep what is there now. You may need it, and it costs nothing.
mv /var/lib/pathofdust /var/lib/pathofdust.before-restore-$(date +%Y%m%d-%H%M%S)
mkdir -p /var/lib/pathofdust

# Reinstall the git-tracked assets the archive deliberately does NOT carry
# (templates/, wiki/, public_adventure_overlay/) from a source checkout:
/opt/pathofdust/bin/deploy.sh /root/dust

# Then lay the archived state on top. cp -r only ever creates or overwrites.
cp -r /tmp/pod-restore/. /var/lib/pathofdust/
rm -f /var/lib/pathofdust/_backup-manifest.json
chown -R pathofdust:pathofdust /var/lib/pathofdust

systemctl start pathofdust
systemctl is-active pathofdust
journalctl -u pathofdust -n 30 --no-pager
```

**Restoring a subset** (say, only characters) is the same but narrower — stop the game, copy the
one file, start it. Do not copy `adventure-characters.json` back without its markers: a missing
marker re-runs that migration or giveaway against already-migrated data.

### Restore evidence, 2026-08-31

Proven on the box, not reasoned about, from `pod-backup-20260831-154537.tar.gz`:

| Check | Result |
|---|---|
| sidecar `sha256sum -c` | OK |
| extracted into `/tmp/pod-restore-proof/` | 123 files (122 members + manifest) |
| every member vs its manifest SHA-256 | **122 / 122 match**, 0 missing, 0 extra |
| every member vs the live source | **122 / 122 identical**, 0 changed, 0 gone |
| live files not in the archive | 136, every one accounted for in the table above |

The second row is the archive's own integrity; the third is drift since the snapshot, which was
zero here because no fight completed in the window. On a busy box that row would legitimately be
non-zero for `adventure-characters.json`, `adventure-world.json` and the summary tier — the
question to ask is whether anything differs that *should not* churn.

---

# Off-box copy: the Windows-side pull

**Not installed. This is a ready-to-run procedure to be scheduled separately.**

Windows **pulls**; Linux never pushes. The private key lives on the Windows box and the Linux
box holds nothing that can reach Windows, so a compromise of the now-internet-facing staging
origin gives no path into `C:\PathofDust`.

### The Linux side (already built)

- User `podbackup` — system account, no password (`!` in shadow), owns nothing.
- `/var/backups/pathofdust` is `0750 root:podbackup`, so podbackup can read and cannot write.
- Its `~/.ssh/authorized_keys` entry is
  `restrict,command="/opt/pathofdust/bin/backup-pull-shell" <key>`. `restrict` disables port
  forwarding, agent forwarding, PTY and X11; the forced command means `SSH_ORIGINAL_COMMAND` is
  the only input and whatever the client asked to run is ignored.
- The vocabulary is two words. Anything else is refused:

| Sent | Result |
|---|---|
| `list` | archive and `.sha256` filenames, one per line |
| `cat pod-backup-….tar.gz` | that file's bytes |
| `cat ../../etc/shadow`, `cat /etc/shadow`, `cat .verify.py` | exit 2, `refused: bare filenames only` |
| `cat nope.tar.gz` | exit 2, `refused: not a backup artifact` |
| `bash`, or any other command, or a bare shell | exit 1, `refused. vocabulary: list \| cat <name>` |

> **`podbackup`'s login shell must be a real shell** (`/bin/bash`), not `/usr/sbin/nologin`.
> sshd runs a forced command *through* the user's shell, so `nologin` refuses the pull itself
> with "This account is currently not available". The restriction comes from `restrict` plus the
> forced command, not from the shell.

Verified end-to-end from the Windows box on 2026-08-31: `list` returned the archives, `cat`
streamed one back, its checksum verified on the Windows side, Windows' built-in `tar` read all
129 entries, and an attempt to run `id; cat /etc/shadow` over the same key was refused.

### Currently authorised key

The pull currently reuses the existing `C:\Users\Administrator\.ssh\id_ed25519`. That key already
has root on the box, so the restriction buys separation of *purpose*, not of privilege. To give
the pull its own key — generate on Windows, install only the public half:

```powershell
ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\pod_pull -C "windows-pull-podbackup" -N '""'
Get-Content $env:USERPROFILE\.ssh\pod_pull.pub | ssh root@<SERVER-IP> `
  "printf 'restrict,command=\"/opt/pathofdust/bin/backup-pull-shell\" %s\n' \"$(cat)\" > /var/lib/podbackup/.ssh/authorized_keys; chown podbackup:podbackup /var/lib/podbackup/.ssh/authorized_keys; chmod 0600 /var/lib/podbackup/.ssh/authorized_keys"
```

### The puller

Save as `C:\pod-backup-pull\pull-linux-backups.ps1` — **outside `C:\PathofDust`**, so a
deployment never touches it and it never lands inside the directory production backs up.

```powershell
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $ServerIp,
    [string] $Dest = 'C:\pod-backups-linux',
    [string] $KeyPath = "$env:USERPROFILE\.ssh\id_ed25519",
    [int]    $Keep = 90,
    [int]    $StaleHours = 36
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $Dest -Force | Out-Null
$log = Join-Path $Dest 'pull-linux-backups.log'
function Say($m) {
    $line = "$(Get-Date -Format 'yyyy-MM-ddTHH:mm:sszzz') - $m"
    Add-Content -Path $log -Value $line -Encoding utf8
    Write-Host $line
}

$ssh = @('-i', $KeyPath, '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', "podbackup@$ServerIp")
Say "pull start server=$ServerIp dest=$Dest keep=$Keep"

$remote = & ssh @ssh 'list'
if ($LASTEXITCODE -ne 0) { Say 'FAILED: could not list remote archives'; exit 1 }
$archives = @($remote | Where-Object { $_ -like 'pod-backup-*.tar.gz' })
if ($archives.Count -eq 0) { Say 'FAILED: remote listed no archives'; exit 1 }

$fetched = 0
foreach ($a in $archives) {
    $local = Join-Path $Dest $a
    if (Test-Path -LiteralPath $local) { continue }

    # .sha256 first: without it the archive cannot be proven and must not be kept.
    $sumText = & ssh @ssh "cat $a.sha256"
    if ($LASTEXITCODE -ne 0) { Say "FAILED: no checksum for $a"; exit 1 }
    $expected = ($sumText -split '\s+')[0]

    # -Encoding Byte is PS5.1; on PowerShell 7 use -AsByteStream instead.
    & ssh @ssh "cat $a" | Set-Content -LiteralPath $local -Encoding Byte
    if ($LASTEXITCODE -ne 0) { Remove-Item -LiteralPath $local -Force -EA SilentlyContinue; Say "FAILED: transfer of $a"; exit 1 }

    $actual = (Get-FileHash -LiteralPath $local -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected.ToLower()) {
        Remove-Item -LiteralPath $local -Force
        Say "FAILED: checksum mismatch on $a (expected $expected, got $actual) - partial deleted"
        exit 1
    }
    # Readability, not just checksum: tar.exe ships with Windows 10+.
    & tar.exe -tzf $local > $null 2>&1
    if ($LASTEXITCODE -ne 0) { Remove-Item -LiteralPath $local -Force; Say "FAILED: $a is not readable as a tar.gz"; exit 1 }

    Set-Content -LiteralPath "$local.sha256" -Value $sumText -Encoding utf8
    $fetched++
    Say "pulled $a ($((Get-Item $local).Length) bytes, checksum + readability OK)"
}

# Retention. Count-based, oldest first. No rsync, no --delete, anywhere in this design.
$local = @(Get-ChildItem -Path $Dest -Filter 'pod-backup-*.tar.gz' | Sort-Object Name)
$pruned = 0
if ($local.Count -gt $Keep) {
    foreach ($old in $local[0..($local.Count - $Keep - 1)]) {
        Remove-Item -LiteralPath $old.FullName -Force
        Remove-Item -LiteralPath "$($old.FullName).sha256" -Force -EA SilentlyContinue
        $pruned++
    }
}

# Staleness alarm: a pull that succeeds while the Linux timer has quietly died
# looks healthy forever. Age the NEWEST archive, not this run.
$newest = Get-ChildItem -Path $Dest -Filter 'pod-backup-*.tar.gz' | Sort-Object Name | Select-Object -Last 1
$age = (Get-Date) - $newest.LastWriteTime
Say "pull end - fetched $fetched, pruned $pruned, newest=$($newest.Name) age=$([math]::Round($age.TotalHours,1))h"
if ($age.TotalHours -gt $StaleHours) {
    Say "FAILED: newest archive is $([math]::Round($age.TotalHours,1))h old (> $StaleHours) - the Linux timer may be dead"
    exit 1
}
```

### The Scheduled Task

Runs at 04:00, an hour after the Linux timer's 03:15 + jitter. Replace `<SERVER-IP>`:

```powershell
$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
  -Argument '-NoProfile -ExecutionPolicy Bypass -File "C:\pod-backup-pull\pull-linux-backups.ps1" -ServerIp <SERVER-IP>'
$trigger  = New-ScheduledTaskTrigger -Daily -At 4:00am
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -DontStopIfGoingOnBatteries `
  -ExecutionTimeLimit (New-TimeSpan -Hours 1)
Register-ScheduledTask -TaskName 'PodPullLinuxBackups' -Action $action -Trigger $trigger `
  -Settings $settings -RunLevel Limited `
  -User "$env:USERDOMAIN\$env:USERNAME" -Password (Read-Host -AsSecureString 'Password')
```

The task must run as the account whose `.ssh` holds the key — `-RunLevel Limited` is deliberate,
the pull needs no elevation. Verify with one manual run before trusting the schedule:

```powershell
Start-ScheduledTask -TaskName 'PodPullLinuxBackups'
Get-ScheduledTaskInfo -TaskName 'PodPullLinuxBackups' | Select LastRunTime, LastTaskResult
Get-Content C:\pod-backups-linux\pull-linux-backups.log -Tail 20
```

`LastTaskResult` of `0` is success; anything else means the log's last line says why.

## Notes

- **`rsync --delete` appears nowhere** in any part of this design, by instruction. The Linux
  prune is an explicit count-based `rm` of archives it has just decided are surplus; the Windows
  prune is the same. Neither mirrors a directory.
- **The backup unit never touches the game process.** `ReadOnlyPaths=/var/lib/pathofdust` makes
  "the script only reads the source" structural rather than a promise in a comment.
- A throwaway account `podtest_admin` exists on staging from the 2026-08-31 atomic-write
  round-trip proof. It is a normal player account with no operator gate pointing at it
  (`OPERATOR_LOGIN` is back to `lokati`); delete it whenever convenient.
