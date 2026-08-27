# Scheduled backup for the adventure game's persisted state - the gap
# found during second-deployment scoping (2026-08-23): NOTHING backs this
# data up on a schedule. The only two backup mechanisms that exist today
# are incidental:
#
#   1. Migration-time `.pre-*-backup` copies, written by whichever
#      one-time migration happened to run.
#   2. Deploy-time `backup-pre-<name>/` directories, written by hand as
#      REFACTOR_PLAN.md section 13 step 4.
#
# Both are tied to a DEPLOY. A world that is not being deployed to -
# exactly what a frozen legacy world is - receives no backups at all.
# The August 2026 UTF-8 BOM incident wiped all 60 characters, and the
# only reason they came back is that a deploy had just happened to make
# a copy. That is luck, not a backup strategy.
#
# SAFE TO RUN AGAINST A LIVE GAME. This script:
#   * COPIES, never moves, never renames, never writes into $SourceDir.
#   * opens every source with FileShare ReadWrite|Delete, so it can never
#     block a write the game is trying to make (a plain Copy-Item asks
#     only for FILE_SHARE_READ; the game's own `std::fs::write` is more
#     permissive than that, but taking the widest share mode explicitly
#     means this script can never be the reason a save fails).
#   * verifies every copy parses before it prunes anything, and refuses
#     to prune at all if this run's snapshot is degraded.
#   * NEVER touches a process. It does not start, stop, query or
#     enumerate one - by image name or otherwise (CLAUDE.md PRODUCTION
#     SAFETY). There is deliberately no process code in this file.
#
# THE HAZARD THIS VERIFIES AGAINST. The game persists with
# `std::fs::write` (game/src/state.rs), which truncates and then writes.
# A copy taken inside that window gets a truncated or empty file that is
# still perfectly valid on disk and completely useless as a backup. That
# is why every copy is parsed, why a zero-length result is a hard
# failure, and why a failed verify retries the copy rather than
# accepting it.
#
# Parameterized by -SourceDir so a second deployment runs the same script
# with different arguments and its own -BackupRoot. Nothing here is
# specific to C:\PathofDust.
#
# Register it with a scheduled task - see docs/ops_backup_and_watchdog.md
# for the exact definition. This script does not create one.

[CmdletBinding()]
param(
    # The game's WORKING DIRECTORY - the directory the game.exe process
    # was launched with, which is what every persisted path resolves
    # against (see game/src/adventure/paths.rs: `data_path` falls back to
    # an EMPTY base, so unset GAME_DATA_DIR means "CWD-relative", and the
    # GameProcess task sets WorkingDirectory to the deployment root).
    [string] $SourceDir = $PSScriptRoot,

    # Where snapshots land. Defaults to a sibling of the deployment,
    # named after it, so two deployments never share a backup root and
    # the backup root is never inside the directory being backed up.
    [string] $BackupRoot,

    # Retention. See docs/ops_backup_and_watchdog.md for the reasoning
    # behind these two numbers; they are parameters rather than constants
    # because a frozen legacy world may well want a longer daily tail
    # than an actively-developed one.
    [int] $HourlyRetentionHours = 24,
    [int] $DailyRetentionDays = 30,

    # Opt-in. The bulk fight archives measured 2,465 MB live on
    # 2026-08-23 (coarse 333 MB / detail 1,060 MB / bundle 1,072 MB).
    # At the default retention that is ~130 GB of snapshots against
    # 165 GB free, so they are excluded unless explicitly asked for.
    # The summary tier and custom sprites are NOT in here - they are
    # small and irreplaceable, and are always included.
    [switch] $IncludeFightArchives,

    # Report what would be copied and what would be pruned, touch
    # nothing. Also verifies the LIVE source files in place, which makes
    # this a useful "is my current data parseable right now" check on its
    # own.
    [switch] $DryRun,

    [string] $LogPath,

    # A verify failure almost always means the copy landed inside the
    # game's truncate-then-write window. Retrying a few hundred ms later
    # lands on a complete file.
    [int] $CopyRetries = 3,
    [int] $RetryDelayMilliseconds = 750
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------
# The file manifest, DERIVED FROM THE CODE (2026-08-23), not guessed.
# Each entry carries the source that proves it is persisted state.
# `Test-ManifestDrift` below re-checks the marker list against what is
# actually on disk on every run, so this list announces when it has gone
# stale instead of silently under-backing-up.
# ---------------------------------------------------------------------

# Irreplaceable live state. Losing any of these loses player progress,
# an operator's tuning, or a login session.
$CoreFiles = @(
    'adventure-characters.json'        # main.rs:122 -> manager.rs:1785 (data_path)
    'adventure-world.json'             # main.rs:123 -> manager.rs:1786 (data_path)
    'adventure-reforge-cooldown.json'  # main.rs:124 -> manager.rs:1787 (data_path)
    'adventure-rampage-state.json'     # manager.rs:206  RAMPAGE_STATE_PATH
    'adventure-sessions.json'          # main.rs:165 -> adventure_web.rs:92/140 (CWD, NOT data_path)
    'adventure-accounts.json'          # adventure_web/accounts.rs:37 accounts_path (sibling of sessions; CWD, NOT data_path)
    'adventure-live-tunables.toml'     # tunables.rs:526 TUNABLES_PATH
    'adventure-passive-overrides.toml' # passive_overrides.rs:45 PASSIVE_OVERRIDES_PATH
    'adventure-item-balance.toml'      # balance.rs:54 ITEM_BALANCE_PATH
    'adventure-sprite-count.json'      # manager.rs:2053 SPRITE_COUNT_MARKER_PATH
    'patch-notes.json'                 # adventure_web.rs:1768 (CWD, NOT data_path)
    'bot-published-constants.json'     # published_constants.rs:37 (CWD, NOT data_path)
    'adventure-last-fights.json'       # manager.rs:1496 - pre-split legacy blob, absent post-migration
)

# One-time markers. Individually 4 bytes, collectively load-bearing: a
# missing marker re-runs its giveaway or its item/character migration
# against LIVE data on the next start. Restoring characters.json without
# these would re-apply every migration to already-migrated items.
$MarkerFiles = @(
    # fight_storage.rs:339
    'adventure-fights-storage-migration-marker.json'
    # manager.rs, inline consts
    'adventure-crit-reforge-equipped-backfill-marker.json'   # :1838
    'adventure-craft-token-backfill-marker.json'             # :1882
    'adventure-craft-token-backfill-v2-marker.json'          # :1907
    'adventure-pity-launch-marker.json'                      # :1932
    'adventure-wings-launch-grant-marker.json'               # :1952
    'adventure-passive-key-rename-marker.json'               # :1983
    'adventure-kibukah-compensation-marker.json'             # :2018
    'adventure-celestial-shard-first-award-marker.json'      # :2273
    'adventure-unique-shard-first-award-marker.json'         # :2291 ITEM_LAUNCH_GIVEAWAYS
    # main.rs:145 - the one marker that is NOT data_path-wrapped
    'adventure-wings-giveaway-marker.json'
    # migrations.rs:209 ITEM_MIGRATIONS
    'adventure-helm-rebalance-v2-marker.json'
    'adventure-power-roll-backfill-marker.json'
    'adventure-krangle-accuracy-marker.json'
    'adventure-item-accuracy-marker.json'
    'adventure-crit-value-nerf-marker.json'
    'adventure-gloves-speed-rebalance-marker.json'
    'adventure-crit-lineage-backfill-marker.json'
    'adventure-crit-flag-to-affix-tracking-marker.json'
    # migrations.rs:398 CHARACTER_MIGRATIONS
    'adventure-flowlikewater-swap-marker.json'
    'adventure-celestial-shard-into-unique-shard-marker.json'
    'adventure-duplicate-unique-effects-cleanup-marker.json'
    'adventure-lingering-effect-to-echo-marker.json'
)

# Fight-tier sequence counters (fight_storage.rs:42-46). Tiny, but a
# restored archive directory whose counter has moved on writes over
# existing files.
$SeqFiles = @(
    'adventure-fights-coarse-seq.json'
    'adventure-fights-detail-seq.json'
    'adventure-fights-summary-seq.json'
    'adventure-fights-bundle-seq.json'
)

# Small directories that are always included. Both are irreplaceable and
# both measured under 5 MB on 2026-08-23.
$SmallDirs = @(
    # fight_storage.rs:41 - the tier that actually serves player-facing
    # fight history, capped at 200 files (2.6 MB live). Section 13 step 4
    # already pins this per-deploy for exactly this reason.
    'adventure-fights-summary'
    # character.rs:792 CUSTOM_SPRITE_DIR - player-uploaded sprites
    # (4.3 MB live). Nothing else on disk holds these.
    'public_adventure_overlay\sprites\custom'
)

# Bulk archives, -IncludeFightArchives only. `adventure-fights-pinned`
# is in here because a pinned file is a full-size coarse/detail copy,
# but note it is the one directory the game NEVER prunes
# (fight_storage.rs:261) - if it is non-empty and not being backed up,
# say so out loud rather than letting it look covered.
$ArchiveDirs = @(
    'adventure-fights-coarse'
    'adventure-fights-detail'
    'adventure-fights-bundle'
    'adventure-fights-pinned'
)

$SnapshotPrefix = 'pod-backup-'
$SnapshotStampFormat = 'yyyyMMdd-HHmmss'
$ManifestName = '_backup-manifest.json'

# ---------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------

function Get-Stamp {
    # Real offset, not a bare "Z" on a local clock. game-watchdog.ps1's
    # legacy line format does the latter and it has already cost one
    # session a false "log gap" reading (docs/anomaly_ledger.md, #44
    # self-correction). New log surfaces do not repeat that.
    return (Get-Date -Format 'yyyy-MM-ddTHH:mm:sszzz')
}

function Write-Log {
    param([string] $Message, [switch] $Console)
    $line = "$(Get-Stamp) - $Message"
    try { Add-Content -Path $script:LogPath -Value $line -Encoding utf8 } catch { }
    if ($Console -or $DryRun -or $VerbosePreference -ne 'SilentlyContinue') { Write-Host $line }
}

function Copy-Shared {
    # A copy that cannot lock the source. FileShare.ReadWrite|Delete says
    # "I am reading this, but anyone may write it or delete it out from
    # under me while I do" - the game's writes and prunes proceed exactly
    # as if this script were not running.
    param([string] $Source, [string] $Destination)
    $in = $null
    $out = $null
    try {
        $share = [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete
        $in = New-Object IO.FileStream($Source, [IO.FileMode]::Open, [IO.FileAccess]::Read, $share)
        $out = New-Object IO.FileStream($Destination, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $in.CopyTo($out)
        return @{ Ok = $true; Error = $null }
    } catch {
        return @{ Ok = $false; Error = $_.Exception.Message }
    } finally {
        if ($null -ne $out) { $out.Dispose() }
        if ($null -ne $in) { $in.Dispose() }
    }
}

function Test-DataFile {
    # Does this file parse as what it claims to be? Returns Ok plus a
    # human reason and a Bom flag.
    #
    # Zero-length is a HARD failure, not an empty file: every path in the
    # manifest is written by `serde_json::to_string*` or `toml::to_string`
    # and none of them can legitimately produce zero bytes. A zero-length
    # result is the truncate half of `std::fs::write` caught mid-flight.
    #
    # A BOM is reported explicitly because a UTF-8 BOM on
    # adventure-characters.json is the exact shape of the August 2026
    # incident. The game now strips it with a warning
    # (game/src/state.rs:61), so it is no longer fatal - but a backup
    # that has silently started carrying one is worth knowing about.
    param([string] $Path)

    $result = @{ Ok = $false; Reason = ''; Bom = $false }

    try { $bytes = [IO.File]::ReadAllBytes($Path) }
    catch { $result.Reason = "unreadable: $($_.Exception.Message)"; return $result }

    if ($bytes.Length -eq 0) {
        $result.Reason = 'zero-length (truncated write caught mid-flight?)'
        return $result
    }
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
        $result.Bom = $true
    }
    # [Array]::IndexOf, not -contains: -contains enumerates a 3.3 MB byte
    # array through the PowerShell pipeline and costs seconds on
    # adventure-characters.json alone.
    if ([Array]::IndexOf($bytes, [byte]0) -ge 0) {
        $result.Reason = 'contains NUL bytes (not text)'
        return $result
    }

    # Strict UTF-8 decode - throws rather than substituting U+FFFD, so a
    # half-written multibyte sequence at a truncation boundary is caught.
    try {
        $enc = New-Object Text.UTF8Encoding($false, $true)
        $offset = 0
        if ($result.Bom) { $offset = 3 }
        $text = $enc.GetString($bytes, $offset, $bytes.Length - $offset)
    } catch {
        $result.Reason = "not valid UTF-8: $($_.Exception.Message)"
        return $result
    }

    if ([string]::IsNullOrWhiteSpace($text)) {
        $result.Reason = 'whitespace only'
        return $result
    }

    if ([IO.Path]::GetExtension($Path) -eq '.toml') {
        # INTEGRITY CHECK, NOT A PARSE. Windows PowerShell 5.1 has no TOML
        # parser and this script deliberately takes no dependencies, so
        # say what this is rather than dressing it up: the byte-level
        # checks above (non-empty, no NULs, strict UTF-8) plus bracket
        # balance. See docs/ops_backup_and_watchdog.md for the limits.
        #
        # Bracket balance is the one structural invariant worth checking
        # here because it targets the exact hazard - a copy taken inside
        # `std::fs::write`'s truncate-then-write window. The game writes
        # these files with multi-line arrays (`baseline_stage_anchors = [`
        # ... `]` in adventure-live-tunables.toml, every node in
        # adventure-passive-overrides.toml), so a truncated copy almost
        # always ends inside an unclosed array.
        #
        # Two earlier, stricter versions of this check were wrong on the
        # LIVE data and the dry run caught both - which is what the dry
        # run is for. First: requiring at least one uncommented
        # assignment failed adventure-item-balance.toml, which ships
        # entirely commented out (that is its legitimate default state).
        # Second: requiring every line to be a comment, a [table] header
        # or a key = value assignment failed the two files that use
        # multi-line arrays. Do not tighten this again without running
        # -DryRun against all three live TOMLs.
        $depth = 0
        $lineNo = 0
        foreach ($line in ($text -split "`n")) {
            $lineNo++
            $code = $line
            $hash = $code.IndexOf('#')
            if ($hash -ge 0) { $code = $code.Substring(0, $hash) }
            foreach ($ch in $code.ToCharArray()) {
                if ($ch -eq '[') { $depth++ }
                elseif ($ch -eq ']') { $depth-- }
            }
            if ($depth -lt 0) {
                $result.Reason = "TOML integrity check failed at line ${lineNo}: unbalanced ']'"
                return $result
            }
        }
        if ($depth -ne 0) {
            $result.Reason = "TOML integrity check failed: $depth unclosed '[' at end of file (truncated write?)"
            return $result
        }
        $result.Ok = $true
        $result.Reason = 'toml integrity ok'
        return $result
    }

    try {
        $parsed = $text | ConvertFrom-Json -ErrorAction Stop
    } catch {
        $result.Reason = "JSON parse failed: $($_.Exception.Message)"
        return $result
    }

    if ([IO.Path]::GetFileName($Path) -eq 'adventure-accounts.json') {
        # The ONE name-specific arm, and it earns the exception: local
        # accounts are the only file in the manifest that cannot be
        # reconstructed from anything else. Characters can be replayed
        # from an older snapshot, sessions can be re-minted by logging in
        # again - a lost password hash has no external identity provider
        # to re-authenticate against, so the account is simply gone.
        #
        # INTEGRITY CHECK, NOT A SECURITY CHECK, same honesty as the TOML
        # arm above: it asserts the shape accounts.rs writes (an object of
        # login -> { username, password_hash, created_at }, hashes in PHC
        # format) so a structurally-valid but credential-empty backup is
        # refused rather than verified. It does not and cannot judge
        # whether a hash is the right one.
        #
        # An empty object passes deliberately: `{}` is the legitimate
        # state of the store after the game has written it but before
        # anyone has registered. A file that does not exist at all - the
        # state before the FIRST registration - never reaches here;
        # `Add-OneFile` and the dry run both skip an absent source, the
        # same as every other legitimately-absent manifest entry.
        if ($null -eq $parsed -or $parsed -isnot [psobject]) {
            $result.Reason = 'accounts shape check failed: not a JSON object'
            return $result
        }
        foreach ($prop in $parsed.PSObject.Properties) {
            $hash = $prop.Value.password_hash
            if ([string]::IsNullOrWhiteSpace($hash)) {
                $result.Reason = "accounts shape check failed: '$($prop.Name)' has no password_hash"
                return $result
            }
            if (-not $hash.StartsWith('$argon2')) {
                $result.Reason = "accounts shape check failed: '$($prop.Name)' has a non-argon2 password_hash"
                return $result
            }
        }
        $result.Ok = $true
        $result.Reason = "json ok (accounts shape ok, $(@($parsed.PSObject.Properties).Count) account(s))"
        return $result
    }

    $result.Ok = $true
    $result.Reason = 'json ok'
    return $result
}

function Test-ManifestDrift {
    # The marker list above was derived from the code on 2026-08-23. A
    # future release adding a marker would silently fall out of the
    # backup set, so compare the list against the glob every run and say
    # so. This is the same durable lesson CLAUDE.md records for form
    # POSTs: derive the field set from reality, do not hand-maintain it
    # and hope. Anything the glob finds IS backed up regardless - the
    # drift report exists to get the list updated, not to skip a file.
    param([string] $Dir)
    $onDisk = @(Get-ChildItem -LiteralPath $Dir -Filter 'adventure-*-marker.json' -File -ErrorAction SilentlyContinue |
        ForEach-Object { $_.Name })
    $unknown = @($onDisk | Where-Object { $MarkerFiles -notcontains $_ })
    return $unknown
}

function Get-Snapshots {
    param([string] $Root)
    if (-not (Test-Path -LiteralPath $Root)) { return @() }
    $out = New-Object Collections.ArrayList
    foreach ($d in (Get-ChildItem -LiteralPath $Root -Directory -ErrorAction SilentlyContinue)) {
        if (-not $d.Name.StartsWith($SnapshotPrefix)) { continue }
        $stamp = $d.Name.Substring($SnapshotPrefix.Length)
        $when = [DateTime]::MinValue
        $parsed = [DateTime]::TryParseExact(
            $stamp, $SnapshotStampFormat, [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::None, [ref] $when)
        if (-not $parsed) { continue }

        # A snapshot counts as verified only if its own manifest says so.
        # An unreadable or missing manifest reads as NOT verified, which
        # is the safe direction: it can still be kept, but it can never
        # be the reason pruning decides a day is covered.
        $verified = $false
        $mf = Join-Path $d.FullName $ManifestName
        if (Test-Path -LiteralPath $mf) {
            try {
                $m = (Get-Content -LiteralPath $mf -Raw) | ConvertFrom-Json
                if ($m.verdict -eq 'clean') { $verified = $true }
            } catch { }
        }
        $null = $out.Add([pscustomobject]@{
            Path = $d.FullName; Name = $d.Name; When = $when; Verified = $verified
        })
    }
    return @($out | Sort-Object When)
}

function Get-PruneDecisions {
    # Retention: keep EVERYTHING inside the hourly window; outside it,
    # keep the EARLIEST snapshot of each calendar day for
    # $DailyRetentionDays; delete the rest.
    #
    # Earliest-of-day, not latest, and the reason is the whole point of
    # this script. A backup tier that only gets consulted days later is
    # being consulted because damage went unnoticed. The earliest
    # snapshot of a day is the one with the most of that day still ahead
    # of it - i.e. the most pre-damage state available for that day.
    # Latest-of-day would hand back 23:xx, which for a corruption that
    # happened at 14:00 is a backup of the corruption.
    param([object[]] $Snapshots, [datetime] $Now)

    $decisions = New-Object Collections.ArrayList
    $hourlyCutoff = $Now.AddHours(-$HourlyRetentionHours)
    $dailyCutoff = $Now.Date.AddDays(-$DailyRetentionDays)

    $older = @($Snapshots | Where-Object { $_.When -lt $hourlyCutoff })
    $keepDaily = @{}
    foreach ($group in ($older | Group-Object -Property { $_.When.Date })) {
        $sorted = @($group.Group | Sort-Object When)
        # Prefer the earliest VERIFIED snapshot of the day; fall back to
        # the earliest of any kind rather than dropping the day whole.
        $pick = $sorted | Where-Object { $_.Verified } | Select-Object -First 1
        if ($null -eq $pick) { $pick = $sorted[0] }
        $keepDaily[$pick.Path] = $true
    }

    foreach ($s in $Snapshots) {
        if ($s.When -ge $hourlyCutoff) {
            $null = $decisions.Add([pscustomobject]@{ Snapshot = $s; Keep = $true; Tier = 'hourly'; Why = "within ${HourlyRetentionHours}h window" })
            continue
        }
        if ($s.When -lt $dailyCutoff) {
            $null = $decisions.Add([pscustomobject]@{ Snapshot = $s; Keep = $false; Tier = 'expired'; Why = "older than ${DailyRetentionDays}d" })
            continue
        }
        if ($keepDaily.ContainsKey($s.Path)) {
            $null = $decisions.Add([pscustomobject]@{ Snapshot = $s; Keep = $true; Tier = 'daily'; Why = 'earliest of its calendar day' })
            continue
        }
        $null = $decisions.Add([pscustomobject]@{ Snapshot = $s; Keep = $false; Tier = 'superseded'; Why = 'not the earliest of its calendar day' })
    }
    return @($decisions)
}

# ---------------------------------------------------------------------
# Resolve arguments
# ---------------------------------------------------------------------

if (-not (Test-Path -LiteralPath $SourceDir)) {
    throw "SourceDir does not exist: $SourceDir"
}
$SourceDir = (Resolve-Path -LiteralPath $SourceDir).ProviderPath.TrimEnd('\')

if ([string]::IsNullOrWhiteSpace($BackupRoot)) {
    $parent = Split-Path -Path $SourceDir -Parent
    $leaf = Split-Path -Path $SourceDir -Leaf
    $BackupRoot = Join-Path $parent (Join-Path 'pod-backups' $leaf)
}

if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $LogPath = Join-Path $SourceDir 'backup-game-data.log'
}
$script:LogPath = $LogPath

# The backup root must not live inside the directory being backed up -
# otherwise each snapshot eventually contains the previous ones.
$normalizedRoot = [IO.Path]::GetFullPath($BackupRoot).TrimEnd('\')
if ($normalizedRoot.Equals($SourceDir, 'OrdinalIgnoreCase') -or
    $normalizedRoot.StartsWith($SourceDir + '\', 'OrdinalIgnoreCase')) {
    throw "BackupRoot must not be inside SourceDir (BackupRoot=$normalizedRoot, SourceDir=$SourceDir)"
}
$BackupRoot = $normalizedRoot

$mode = 'live'
if ($DryRun) { $mode = 'DRY RUN' }
Write-Log "backup start ($mode) source=$SourceDir root=$BackupRoot retention=${HourlyRetentionHours}h/${DailyRetentionDays}d archives=$([bool]$IncludeFightArchives)"

# ---------------------------------------------------------------------
# Build the work list
# ---------------------------------------------------------------------

$drift = Test-ManifestDrift -Dir $SourceDir
if ($drift.Count -gt 0) {
    Write-Log "MANIFEST DRIFT - $($drift.Count) marker file(s) on disk are not in this script's code-derived list (they ARE being backed up; update `$MarkerFiles): $($drift -join ', ')" -Console
}

$wantedFiles = New-Object Collections.ArrayList
foreach ($n in ($CoreFiles + $MarkerFiles + $SeqFiles + $drift)) { $null = $wantedFiles.Add($n) }

$dirs = @($SmallDirs)
if ($IncludeFightArchives) {
    $dirs += $ArchiveDirs
} else {
    # `adventure-fights-pinned` is mod-curated and the game never prunes
    # it. If it holds anything and we are skipping it, that must not look
    # like coverage.
    $pinned = Join-Path $SourceDir 'adventure-fights-pinned'
    if (Test-Path -LiteralPath $pinned) {
        $n = @(Get-ChildItem -LiteralPath $pinned -File -Recurse -ErrorAction SilentlyContinue).Count
        if ($n -gt 0) {
            Write-Log "NOTE - adventure-fights-pinned holds $n mod-pinned file(s) and is NOT covered (re-run with -IncludeFightArchives to include it)." -Console
        }
    }
}

# ---------------------------------------------------------------------
# Dry run: verify sources in place, report the plan, touch nothing
# ---------------------------------------------------------------------

if ($DryRun) {
    Write-Host ''
    Write-Host 'WOULD COPY (files):'
    $total = 0L
    foreach ($name in $wantedFiles) {
        $src = Join-Path $SourceDir $name
        if (-not (Test-Path -LiteralPath $src)) {
            Write-Host ("  {0,-56} {1}" -f $name, 'absent (skipped)')
            continue
        }
        $len = (Get-Item -LiteralPath $src).Length
        $total += $len
        $v = Test-DataFile -Path $src
        $flag = 'OK'
        if (-not $v.Ok) { $flag = "SOURCE FAILS VERIFY: $($v.Reason)" }
        elseif ($v.Bom) { $flag = 'ok (BOM present)' }
        Write-Host ("  {0,-56} {1,12:N0} B  {2}" -f $name, $len, $flag)
    }
    Write-Host ''
    Write-Host 'WOULD COPY (directories):'
    foreach ($d in $dirs) {
        $src = Join-Path $SourceDir $d
        if (-not (Test-Path -LiteralPath $src)) {
            Write-Host ("  {0,-56} {1}" -f $d, 'absent (skipped)')
            continue
        }
        $items = @(Get-ChildItem -LiteralPath $src -Recurse -File -ErrorAction SilentlyContinue)
        $sz = ($items | Measure-Object -Property Length -Sum).Sum
        if ($null -eq $sz) { $sz = 0 }
        $total += $sz
        Write-Host ("  {0,-56} {1,12:N0} B  ({2} files)" -f $d, $sz, $items.Count)
    }
    Write-Host ''
    Write-Host ("SNAPSHOT SIZE: {0:N0} bytes ({1:N2} MB)" -f $total, ($total / 1MB))
    Write-Host ("WOULD CREATE : {0}" -f (Join-Path $BackupRoot ($SnapshotPrefix + (Get-Date -Format $SnapshotStampFormat))))
    Write-Host ''
    Write-Host 'WOULD PRUNE:'
    $existing = Get-Snapshots -Root $BackupRoot
    if ($existing.Count -eq 0) {
        Write-Host '  (no existing snapshots)'
    } else {
        foreach ($d in (Get-PruneDecisions -Snapshots $existing -Now (Get-Date))) {
            $verb = 'KEEP  '
            if (-not $d.Keep) { $verb = 'DELETE' }
            $vf = 'unverified'
            if ($d.Snapshot.Verified) { $vf = 'verified' }
            Write-Host ("  {0} {1,-28} [{2,-10}] {3,-10} {4}" -f $verb, $d.Snapshot.Name, $d.Tier, $vf, $d.Why)
        }
    }
    Write-Log "backup end (DRY RUN) - nothing written"
    return
}

# ---------------------------------------------------------------------
# Live: snapshot
# ---------------------------------------------------------------------

$snapshotName = $SnapshotPrefix + (Get-Date -Format $SnapshotStampFormat)
$snapshotDir = Join-Path $BackupRoot $snapshotName
$null = New-Item -ItemType Directory -Path $snapshotDir -Force

$entries = New-Object Collections.ArrayList
$copied = 0
$failed = 0
$bytes = 0L

function Add-OneFile {
    param([string] $RelativeName)

    $src = Join-Path $SourceDir $RelativeName
    if (-not (Test-Path -LiteralPath $src -PathType Leaf)) { return }

    $dst = Join-Path $snapshotDir $RelativeName
    $dstParent = Split-Path -Path $dst -Parent
    if (-not (Test-Path -LiteralPath $dstParent)) { $null = New-Item -ItemType Directory -Path $dstParent -Force }

    $attempt = 0
    $lastReason = ''
    while ($attempt -lt $CopyRetries) {
        $attempt++
        $c = Copy-Shared -Source $src -Destination $dst
        if (-not $c.Ok) {
            $lastReason = "copy failed: $($c.Error)"
        } else {
            $v = Test-DataFile -Path $dst
            if ($v.Ok) {
                $len = (Get-Item -LiteralPath $dst).Length
                $script:copied++
                $script:bytes += $len
                $null = $script:entries.Add([pscustomobject]@{
                    name = $RelativeName; ok = $true; bytes = $len
                    attempts = $attempt; bom = $v.Bom; reason = $v.Reason
                })
                if ($v.Bom) { Write-Log "NOTE - $RelativeName carries a UTF-8 BOM (copied; the game strips it with a warning)" }
                return
            }
            $lastReason = $v.Reason
        }
        if ($attempt -lt $CopyRetries) { Start-Sleep -Milliseconds $RetryDelayMilliseconds }
    }

    $script:failed++
    $null = $script:entries.Add([pscustomobject]@{
        name = $RelativeName; ok = $false; bytes = 0
        attempts = $attempt; bom = $false; reason = $lastReason
    })
    Write-Log "VERIFY FAILED after $attempt attempt(s): $RelativeName - $lastReason" -Console
}

foreach ($name in $wantedFiles) { Add-OneFile -RelativeName $name }

foreach ($d in $dirs) {
    $srcDir = Join-Path $SourceDir $d
    if (-not (Test-Path -LiteralPath $srcDir -PathType Container)) { continue }
    foreach ($item in (Get-ChildItem -LiteralPath $srcDir -Recurse -File -ErrorAction SilentlyContinue)) {
        $rel = $item.FullName.Substring($SourceDir.Length + 1)
        $dst = Join-Path $snapshotDir $rel
        $dstParent = Split-Path -Path $dst -Parent
        if (-not (Test-Path -LiteralPath $dstParent)) { $null = New-Item -ItemType Directory -Path $dstParent -Force }
        $c = Copy-Shared -Source $item.FullName -Destination $dst
        if ($c.Ok) {
            $copied++
            $bytes += $item.Length
        } else {
            # A sprite or an archived fight file is not JSON-verifiable in
            # any useful sense (sprites are binary; fight files are up to
            # 620 MB and parsing them would dominate the run). Copy
            # success is the check here, and the manifest says so rather
            # than implying a parse happened.
            $failed++
            $null = $entries.Add([pscustomobject]@{
                name = $rel; ok = $false; bytes = 0; attempts = 1; bom = $false
                reason = "copy failed: $($c.Error)"
            })
            Write-Log "COPY FAILED: $rel - $($c.Error)" -Console
        }
    }
}

$verdict = 'clean'
if ($failed -gt 0) { $verdict = 'degraded' }

$manifest = [pscustomobject]@{
    createdAt = (Get-Stamp)
    sourceDir = $SourceDir
    verdict = $verdict
    filesCopied = $copied
    filesFailed = $failed
    bytes = $bytes
    includeFightArchives = [bool]$IncludeFightArchives
    manifestDrift = @($drift)
    entries = @($entries)
}
# BOM-less on purpose. This repo has already lost a save file to a BOM;
# nothing this script writes will be the next one.
$json = $manifest | ConvertTo-Json -Depth 6
[IO.File]::WriteAllText((Join-Path $snapshotDir $ManifestName), $json, (New-Object Text.UTF8Encoding($false)))

Write-Log "snapshot $snapshotName - $copied file(s), $([math]::Round($bytes / 1MB, 2)) MB, verdict=$verdict"

# ---------------------------------------------------------------------
# Prune
# ---------------------------------------------------------------------

if ($verdict -ne 'clean') {
    # Never destroy history on a run that could not produce a good
    # snapshot. A degraded run is exactly when an incident may be under
    # way, and that is the worst possible moment to be deleting older
    # copies.
    Write-Log "PRUNE SKIPPED - this run's snapshot is degraded ($failed failure(s)); older snapshots left untouched" -Console
    Write-Log 'backup end (degraded)'
    exit 1
}

$all = Get-Snapshots -Root $BackupRoot
$decisions = Get-PruneDecisions -Snapshots $all -Now (Get-Date)
$survivors = @($decisions | Where-Object { $_.Keep })
$verifiedSurvivors = @($survivors | Where-Object { $_.Snapshot.Verified })

if ($verifiedSurvivors.Count -eq 0) {
    # Cannot happen on a clean run (this run's own snapshot is verified
    # and inside the hourly window), so reaching here means something is
    # wrong with the retention arithmetic. Refuse rather than proceed.
    Write-Log 'PRUNE ABORTED - the retention plan would leave zero verified snapshots; nothing deleted' -Console
    Write-Log 'backup end (prune aborted)'
    exit 1
}

$deleted = 0
foreach ($d in ($decisions | Where-Object { -not $_.Keep })) {
    try {
        Remove-Item -LiteralPath $d.Snapshot.Path -Recurse -Force -Confirm:$false
        $deleted++
        Write-Log "pruned $($d.Snapshot.Name) ($($d.Tier): $($d.Why))"
    } catch {
        Write-Log "prune failed for $($d.Snapshot.Name): $($_.Exception.Message)" -Console
    }
}

Write-Log "backup end - kept $($survivors.Count) snapshot(s) ($($verifiedSurvivors.Count) verified), pruned $deleted"
