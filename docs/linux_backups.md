# Linux staging backups — script, timer, and the off-box pull

**Date:** 2026-08-31, off-box half completed 2026-09-01 · **Session:** LINUX-BACKUPS ·
**Branch:** `chore/linux-backups`

Ports `backup-game-data.ps1` to the Debian box as `backup-game-data.sh` plus a systemd service
and timer, and stands up **both halves** of an off-box copy — the Linux pull endpoint and the
registered Windows scheduled task that fetches from it. Continues
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

And on the Windows box, for the off-box copy — **nothing inside `C:\PathofDust`**:

| Path | What |
|---|---|
| `C:\pod-backup-pull\pull-linux-backups.ps1` | the puller (source below) |
| `C:\pod-backups-linux\` | the pulled archives + sidecars + `pull-linux-backups.log` |
| Scheduled task `PodPullLinuxBackups` | daily 04:00, S4U |

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

**Registered and running as of 2026-09-01.** Task `PodPullLinuxBackups`, daily at 04:00, pulling
into `C:\pod-backups-linux\`. Nothing was created inside `C:\PathofDust`.

Windows **pulls**; Linux never pushes. The private key lives on the Windows box and the Linux
box holds nothing that can reach Windows, so a compromise of the now-internet-facing staging
origin gives no path into `C:\PathofDust`.

## The Linux side

- User `podbackup` — system account, no password (`!` in shadow), owns nothing.
- `/var/backups/pathofdust` is `0750 root:podbackup`, so podbackup can read and cannot write.
- Its `~/.ssh/authorized_keys` holds **exactly one** entry:
  `restrict,command="/opt/pathofdust/bin/backup-pull-shell" <pod_pull public key>`. `restrict`
  disables port forwarding, agent forwarding, PTY and X11; the forced command means
  `SSH_ORIGINAL_COMMAND` is the only input and whatever the client asked to run is ignored.

> **`podbackup`'s login shell must be a real shell** (`/bin/bash`), not `/usr/sbin/nologin`.
> sshd runs a forced command *through* the user's shell, so `nologin` refuses the pull itself
> with "This account is currently not available". The restriction comes from `restrict` plus the
> forced command, not from the shell.

### Forced-command evidence — 2026-09-01, from Windows over the `pod_pull` key

Every one of these was attempted, not reasoned about. Nothing succeeded, and afterwards
`/tmp/pwned` and `/var/backups/pathofdust/x` were both confirmed absent, all three archives
still present, and the directory still `0750 root:podbackup`.

| Attempt | Result |
|---|---|
| bare `ssh podbackup@…` (interactive shell) | `refused. vocabulary: list \| cat <name>` |
| `ssh -tt` (force a PTY) | exit 255, `PTY allocation request failed on channel 0` |
| `cat /etc/shadow` | exit 2, `refused: bare filenames only` |
| `cat /etc/passwd` | exit 2, `refused: bare filenames only` |
| `cat ../../var/lib/pathofdust/adventure-accounts.json` | exit 2, `refused: bare filenames only` |
| `cat ../.verify.py` | exit 2, `refused: bare filenames only` |
| `rm pod-backup-….tar.gz` | exit 1, `refused. vocabulary` |
| `echo pwned > /var/backups/pathofdust/x` | exit 1, `refused. vocabulary` |
| `touch /tmp/pwned` | exit 1, `refused. vocabulary` |
| `list; id` and `list && id` | exit 1, `refused. vocabulary` |
| `cat $(echo hi)` | exit 2, `refused: not a backup artifact` |

The metacharacter rows matter: `SSH_ORIGINAL_COMMAND` is matched with a `case` on the **whole
string**, never re-evaluated by a shell, so `list; id` is simply not the word `list`.

### The dedicated key

`pod_pull` (`SHA256:F5lF68iYhAQxdv1uXvhlCpgCuS2IDDxWl+rPOGRhFTM`, comment
`windows-pull-podbackup`) is used for this and nothing else. The earlier arrangement reused the
deploy key `id_ed25519`, which already has root on the box — that bought separation of purpose
but not of privilege, and its entry has been removed from podbackup's `authorized_keys`.

Verified 2026-09-01, in both directions:

| Check | Result |
|---|---|
| `pod_pull` → `podbackup@` | works (`list` returns the archives) |
| `id_ed25519` → `podbackup@` | **Permission denied (publickey)** — entry removed |
| `pod_pull` → `root@` | **Permission denied (publickey)** — never authorised |
| `id_ed25519` → `root@` | works — **root SSH from Windows is unaffected** |

`/root/.ssh/authorized_keys` was not touched: still one entry, the deploy key.

To rotate, generate on Windows and install only the public half:

```powershell
ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\pod_pull -C "windows-pull-podbackup" -N '""'
```

```sh
# on the server, as root - REPLACES the single entry rather than appending
printf 'restrict,command="/opt/pathofdust/bin/backup-pull-shell" %s\n' "<paste pod_pull.pub>" \
  > /var/lib/podbackup/.ssh/authorized_keys
chown podbackup:podbackup /var/lib/podbackup/.ssh/authorized_keys
chmod 0600 /var/lib/podbackup/.ssh/authorized_keys
```

Prove the new key works with `ssh -i ~/.ssh/pod_pull -o IdentitiesOnly=yes podbackup@<SERVER-IP> list`
**before** removing the old entry. `IdentitiesOnly=yes` matters — without it ssh will happily
fall back to another key in the agent and you will not learn anything.

## The puller

`C:\pod-backup-pull\pull-linux-backups.ps1` — **outside `C:\PathofDust`**, so a deployment never
touches it and it never lands inside the directory production backs up.

### Getting the bytes across: two ways that do not work

Both were measured, not predicted, and both look correct on paper:

1. **`ssh … | Set-Content -Encoding Byte`** — fails outright. PowerShell decodes a native
   command's stdout into strings, and `Set-Content` then refuses them with *"Cannot proceed with
   byte encoding. When using byte encoding the content must be of type byte"*, once per line,
   leaving a **0-byte file**.
2. **`cmd.exe /c "ssh … > file"`** — produces byte-exact output when run interactively, and
   **deadlocks when the script runs as a background or scheduled job**, observed stalling at
   exactly 2 MiB (a pipe-buffer boundary) because `cmd` inherits the job's captured handles.
   This is the dangerous one: it passes a hand test and hangs in production.

`Start-Process -RedirectStandardOutput` is what works. ssh gets its own file handle for stdout
and a separate one for stderr, so nothing is inherited and nothing has to be drained.

```powershell
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $ServerIp,
    [string] $Dest = 'C:\pod-backups-linux',
    [string] $KeyPath = "$env:USERPROFILE\.ssh\pod_pull",
    [int]    $Keep = 90,
    [int]    $StaleHours = 36,
    # Skip the fetch and run only the retention + staleness checks. Exists so
    # the staleness alarm can be exercised against a deliberately old archive
    # without inventing a fake server.
    [switch] $SkipFetch
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $Dest -Force | Out-Null
$log = Join-Path $Dest 'pull-linux-backups.log'

function Say($m) {
    $line = "$(Get-Date -Format 'yyyy-MM-ddTHH:mm:sszzz') - $m"
    try { Add-Content -Path $log -Value $line -Encoding utf8 } catch { }
    Write-Host $line
}

# Age comes from the archive NAME (pod-backup-yyyyMMdd-HHmmss), which is the
# moment the snapshot was taken. Deliberately not LastWriteTime, which records
# when this box happened to fetch it and would keep looking healthy if the
# Linux timer died and we simply stopped receiving anything new.
function Get-StampFromName([string] $name) {
    $m = [regex]::Match($name, '^pod-backup-(\d{8}-\d{6})\.tar\.gz$')
    if (-not $m.Success) { return $null }
    $when = [DateTime]::MinValue
    $ok = [DateTime]::TryParseExact($m.Groups[1].Value, 'yyyyMMdd-HHmmss',
        [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::None, [ref] $when)
    if ($ok) { return $when } else { return $null }
}

$sshArgs = @('-i', $KeyPath, '-o', 'BatchMode=yes', '-o', 'IdentitiesOnly=yes',
             '-o', 'ConnectTimeout=15', "podbackup@$ServerIp")

Say "pull start server=$ServerIp dest=$Dest keep=$Keep staleHours=$StaleHours skipFetch=$SkipFetch"

$fetched = 0
if (-not $SkipFetch) {
    $remote = & ssh @sshArgs 'list'
    if ($LASTEXITCODE -ne 0) { Say 'FAILED: could not list remote archives'; exit 1 }
    $archives = @($remote | Where-Object { $_ -like 'pod-backup-*.tar.gz' })
    if ($archives.Count -eq 0) { Say 'FAILED: remote listed no archives'; exit 1 }
    Say "remote holds $($archives.Count) archive(s)"

    foreach ($a in $archives) {
        $local = Join-Path $Dest $a
        if (Test-Path -LiteralPath $local) { continue }

        # Checksum first: without it the archive cannot be proven, and an
        # archive that cannot be proven must not be kept.
        $sumText = (& ssh @sshArgs "cat $a.sha256") -join "`n"
        if ($LASTEXITCODE -ne 0 -or -not $sumText) { Say "FAILED: no checksum for $a"; exit 1 }
        $expected = ($sumText.Trim() -split '\s+')[0]

        $errFile = "$local.stderr"
        $argList = @(
            '-i', "`"$KeyPath`"",
            '-o', 'BatchMode=yes', '-o', 'IdentitiesOnly=yes', '-o', 'ConnectTimeout=15',
            "podbackup@$ServerIp", "`"cat $a`""
        )
        $proc = Start-Process -FilePath 'ssh' -ArgumentList $argList -NoNewWindow -Wait -PassThru `
                    -RedirectStandardOutput $local -RedirectStandardError $errFile
        $sshExit = $proc.ExitCode
        $errText = (Get-Content -LiteralPath $errFile -Raw -ErrorAction SilentlyContinue)
        Remove-Item -LiteralPath $errFile -Force -ErrorAction SilentlyContinue
        if ($sshExit -ne 0) {
            Remove-Item -LiteralPath $local -Force -ErrorAction SilentlyContinue
            Say "FAILED: transfer of $a (ssh exit $sshExit) $($errText -replace '\s+', ' ') - partial deleted"
            exit 1
        }

        $actual = (Get-FileHash -LiteralPath $local -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $expected.ToLower()) {
            Remove-Item -LiteralPath $local -Force
            Say "FAILED: checksum mismatch on $a (expected $expected, got $actual) - partial deleted"
            exit 1
        }

        # Readability, not just checksum. tar.exe ships with Windows 10+.
        & tar.exe -tzf $local > $null 2>&1
        if ($LASTEXITCODE -ne 0) {
            Remove-Item -LiteralPath $local -Force
            Say "FAILED: $a is not readable as a tar.gz - deleted"
            exit 1
        }

        Set-Content -LiteralPath "$local.sha256" -Value $sumText -Encoding utf8
        $fetched++
        Say "pulled $a ($((Get-Item -LiteralPath $local).Length) bytes; checksum + readability OK)"
    }
}

# Retention: count-based, oldest first. Names sort chronologically because the
# stamp is zero-padded. No rsync and no --delete anywhere in this design.
$local = @(Get-ChildItem -Path $Dest -Filter 'pod-backup-*.tar.gz' | Sort-Object Name)
if ($local.Count -eq 0) { Say 'FAILED: no archives present after the pull'; exit 1 }

$pruned = 0
if ($local.Count -gt $Keep) {
    foreach ($old in $local[0..($local.Count - $Keep - 1)]) {
        Remove-Item -LiteralPath $old.FullName -Force
        Remove-Item -LiteralPath "$($old.FullName).sha256" -Force -ErrorAction SilentlyContinue
        $pruned++
        Say "pruned $($old.Name) (older than the newest $Keep)"
    }
    $local = @(Get-ChildItem -Path $Dest -Filter 'pod-backup-*.tar.gz' | Sort-Object Name)
}

# Staleness. A pull that succeeds while the Linux timer has quietly died looks
# healthy forever, so age the newest SNAPSHOT, not this run.
$newest = $local[-1]
$stamp = Get-StampFromName $newest.Name
if ($null -eq $stamp) { Say "FAILED: cannot read a timestamp out of $($newest.Name)"; exit 1 }
$ageHours = [math]::Round(((Get-Date) - $stamp).TotalHours, 1)

Say "pull end - fetched $fetched, pruned $pruned, held $($local.Count), newest=$($newest.Name) age=${ageHours}h"

if ($ageHours -gt $StaleHours) {
    Say "FAILED: newest snapshot is ${ageHours}h old (limit $StaleHours) - the Linux backup timer may be dead"
    exit 1
}
exit 0
```

## The Scheduled Task, as registered

Registered 2026-09-01. Replace `<SERVER-IP>` to reproduce:

```powershell
$me = "$env:USERDOMAIN\$env:USERNAME"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
  -Argument "-NoProfile -ExecutionPolicy Bypass -File `"C:\pod-backup-pull\pull-linux-backups.ps1`" -ServerIp <SERVER-IP>"
$trigger  = New-ScheduledTaskTrigger -Daily -At 4:00am
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -DontStopIfGoingOnBatteries `
  -ExecutionTimeLimit (New-TimeSpan -Hours 1)
# S4U = "run whether the user is logged on or not" WITHOUT storing a password.
# Not SYSTEM: the SSH key lives in this user's profile and SYSTEM cannot read it.
$principal = New-ScheduledTaskPrincipal -UserId $me -LogonType S4U -RunLevel Limited
Register-ScheduledTask -TaskName 'PodPullLinuxBackups' -Action $action -Trigger $trigger `
  -Settings $settings -Principal $principal -Force
```

| Property | Value |
|---|---|
| Task name | `PodPullLinuxBackups` (path `\`) |
| Principal | `Administrator` |
| Logon type | **S4U** — runs whether or not the user is logged on, no stored password |
| Run level | `Limited` (the pull needs no elevation) |
| Trigger | daily `04:00` — an hour after the Linux timer's 03:15 + jitter |
| Execution time limit | `PT1H`, `StartWhenAvailable=True` |
| Command line | `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\pod-backup-pull\pull-linux-backups.ps1" -ServerIp <SERVER-IP>` |

Checking on it:

```powershell
Start-ScheduledTask -TaskName 'PodPullLinuxBackups'
Get-ScheduledTaskInfo -TaskName 'PodPullLinuxBackups' | Select LastRunTime, LastTaskResult
Get-Content C:\pod-backups-linux\pull-linux-backups.log -Tail 20
```

`LastTaskResult` of `0` is success; anything else means the log's last line says why.

## Pull evidence — 2026-09-01

**End to end, through the registered task** (`LastRunTime 09/01/2026 10:24:13`,
`LastTaskResult 0`). Every archive independently re-verified afterwards, `Get-FileHash` against
the fetched sidecar:

| Archive | Bytes | SHA-256 matches sidecar | `tar -tzf` members |
|---|---|---|---|
| `pod-backup-20260831-154537.tar.gz` | 2 750 038 | ✅ `0f96cdca…` | 128 |
| `pod-backup-20260831-154724.tar.gz` | 2 750 263 | ✅ `a8f5de49…` | 129 |
| `pod-backup-20260901-032011.tar.gz` | 2 765 371 | ✅ `3c228b8e…` | 248 |

`C:\pod-backups-linux\` holds those three archives, their three `.sha256` sidecars, and
`pull-linux-backups.log`. Nothing else. The third archive is the **timer's own unattended
overnight run** — the daily schedule is confirmed working, not just the manual path.

A second run fetched 0 and exited 0: the pull is idempotent and re-downloads nothing.

### Failure paths, exercised

**Checksum mismatch / partial deletion.** A real archive was copied on the server under an old
name, its sidecar written for the good bytes, then 9 bytes overwritten at offset 1 000 000. The
puller fetched it and:

```
FAILED: checksum mismatch on pod-backup-20260101-000000.tar.gz
  (expected 3c228b8e8e9afa4e83a49c598f11d33406e125639f2567c1f7e0f633a676a227,
        got 24880fc4eb6a5d04ea8c9c1e7a3202ff558a8f13f09dfba6351cd78bd90b1d9d) - partial deleted
EXITCODE=1
```

`Test-Path` on the partial afterwards: **False**. The planted archive was removed from the
server and the real destination re-verified intact.

**Staleness alarm.** A real archive renamed to a 12-day-old snapshot stamp, run with the default
`-StaleHours 36`:

```
pull end - fetched 0, pruned 0, held 1, newest=pod-backup-20260820-031500.tar.gz age=295.2h
FAILED: newest snapshot is 295.2h old (limit 36) - the Linux backup timer may be dead
EXITCODE=1
```

Control, same code path against the current destination: `age=7.1h`, **exit 0**, no alarm. Both
branches proven, not just the failing one.

## Notes

- **`rsync --delete` appears nowhere** in any part of this design, by instruction. The Linux
  prune is an explicit count-based `rm` of archives it has just decided are surplus; the Windows
  prune is the same. Neither mirrors a directory.
- **The backup unit never touches the game process.** `ReadOnlyPaths=/var/lib/pathofdust` makes
  "the script only reads the source" structural rather than a promise in a comment.
- **Nothing was created in `C:\PathofDust`**, and nothing in it was read or modified. The puller
  lives in `C:\pod-backup-pull\` and its output in `C:\pod-backups-linux\`.
- The throwaway account `podtest_admin` created for the 2026-08-31 atomic-write round-trip proof
  has been **removed** (2026-09-01), along with its 3 sessions and the two admin TOMLs that proof
  wrote — both of which were absent before it, so staging is back on built-in defaults.
  `OPERATOR_LOGIN` resolves to `lokati`, there are no `pathofdust.service.d` drop-ins, and
  `https://staging.lokati.net` returns 200.
