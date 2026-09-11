# Scheduled backup for the Twitch bot's persisted state - the gap found
# during the bot-extraction survey (2026-09-08): NOTHING backs this data
# up, on a schedule or otherwise.
#
# `backup-game-data.ps1` is an explicit allow-list and every entry in it
# is a game file. `backup-game-data.sh` archives /var/lib/pathofdust,
# which is the LINUX game data root and holds no bot file at all. The
# game moved to Linux; the bot did not. So the nightly backup that has
# been running all week covers none of the files below.
#
# WHAT THAT COSTS. `tokens.json` is recoverable - re-run `cargo run --bin
# auth`. The rest are not. `commands.json` is hand-curated, the entrance
# themes and personal playlists are per-user data built up over months,
# and the tip histories are a financial record. Each exists in exactly
# one place on one disk.
#
# THIS SCRIPT IS `backup-game-data.ps1`'s SHAPE, NOT A NEW DESIGN. Same
# parameters, same share-mode copy, same verify-then-prune ordering, same
# manifest and verdict, same retention reasoning. Read that file for the
# arguments behind those choices; only the differences are re-argued
# here.
#
# SAFE TO RUN AGAINST A LIVE BOT. This script:
#   * COPIES, never moves, never renames, never writes into $SourceDir.
#   * opens every source with FileShare ReadWrite|Delete, so it can never
#     block a write the bot is trying to make.
#   * verifies every copy parses before it prunes anything, and refuses
#     to prune at all if this run's snapshot is degraded.
#   * NEVER touches a process. It does not start, stop, query or
#     enumerate one - by image name or otherwise (CLAUDE.md PRODUCTION
#     SAFETY). There is deliberately no process code in this file.
#
# THE HAZARD IS DIFFERENT FROM THE GAME'S, AND SMALLER. The game persists
# with `std::fs::write`, which truncates and then writes, so a copy taken
# inside that window gets a truncated file that is valid on disk and
# useless as a backup - that window is the reason its script retries.
# The bot does NOT have that window: `src/state.rs`'s `save_json` goes
# through `write_atomic` (temp file beside the target, fsync, rename
# over), so a reader can only ever see the complete old file or the
# complete new one.
#
# Verification is kept anyway, for the two failures atomicity does not
# cover: the COPY itself failing part-way, and a source that was already
# corrupt before this run started. It is cheap here - the whole manifest
# is a few hundred KB of JSON - so there is no reason to trade it away.
#
# WHY AN ALLOW-LIST AND NOT A DIRECTORY SWEEP. Three reasons, and the
# third is specific to this bot. A sweep backs up whatever happens to be
# lying around, so it silently grows; it cannot distinguish state from
# derived output; and `write_atomic` leaves transient
# `<name>.<pid>.<n>.tmp` files beside their targets, which a sweep would
# capture mid-write and store as though they were state. A named list
# sees none of them.
#
# Parameterized by -SourceDir, which defaults to this script's own
# directory. That is what makes it survive the bot's move into `bot/`:
# the script travels with the files it backs up, so relocating them
# changes NO path in this file. If the two are ever separated, -SourceDir
# is the one argument to set.
#
# Register it with a scheduled task - see docs/ops_backup_and_watchdog.md
# for the game's equivalent definition. This script does not create one.

[CmdletBinding()]
param(
    # The bot's WORKING DIRECTORY - the directory twitch-bot-rs.exe was
    # launched with, which is what every persisted path resolves against.
    # The bot has NO path indirection at all (no `data_path` equivalent,
    # no GAME_DATA_DIR): every one of the files below is a bare
    # CWD-relative literal in `src/**`, so the working directory IS the
    # data directory, always.
    [string] $SourceDir = $PSScriptRoot,

    # Where snapshots land. Defaults to a sibling of the source, named
    # after it, so the backup root is never inside the directory being
    # backed up and two bot instances never share one.
    [string] $BackupRoot,

    # Retention. Same two-tier scheme and the same reasoning as
    # backup-game-data.ps1: keep everything recent, then the earliest
    # snapshot of each day.
    [int] $HourlyRetentionHours = 24,
    [int] $DailyRetentionDays = 30,

    # Opt-in, and OFF by default on purpose - the same shape as the game
    # script's -IncludeFightArchives, for a different reason.
    #
    # `.env` holds the bot's 29 live secrets (TWITCH_CLIENT_SECRET,
    # STREAMELEMENTS_JWT, PAYPAL_RELAY_TOKEN, YOUTUBE_API_KEYS, ...).
    # Copying it by default would put those secrets in up to 54 retained
    # snapshot directories, multiplying the number of places on disk that
    # a leak could come from, to protect values that are all re-issuable
    # from their own consoles. Losing `.env` is an afternoon; leaking it
    # is a credential rotation across five services.
    #
    # It is a switch rather than a flat refusal because that trade is the
    # owner's to make, not this script's, and because `.env.example`
    # documents the KEYS but not the VALUES.
    [switch] $IncludeEnv,

    # Report what would be copied and what would be pruned, touch
    # nothing. Also verifies the LIVE source files in place, which makes
    # this a useful "is my current bot state parseable right now" check
    # on its own.
    [switch] $DryRun,

    [string] $LogPath,

    # Retained from the game script's shape. The bot's atomic writes mean
    # a retry should never actually be needed for a torn source; it is
    # here for a copy that fails on a transient IO error.
    [int] $CopyRetries = 3,
    [int] $RetryDelayMilliseconds = 750
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------
# The file manifest, DERIVED FROM THE CODE (2026-09-08), not guessed.
# Each entry carries the source that proves it is persisted state.
#
# Derived by grepping every file literal in `src/**` rather than every
# `PathBuf::from(...)` - the narrower search MISSES three files, because
# `playrandom.rs` holds its path in a `const STATE_PATH: &str` and the
# two public-site outputs are built with `dir.join(...)`. That is the
# whole reason this list is derived twice and cross-checked:
#
#   $ grep -rhoE '"[a-z0-9][a-z0-9._-]*\.(json|toml|txt)"' src/ --include=*.rs
# ---------------------------------------------------------------------

# Irreplaceable live state. Losing any of these loses hand-curated
# operator content, per-user data built up over months, or a financial
# record.
$CoreFiles = @(
    'tokens.json'                            # bin/auth.rs:149, main.rs - Twitch OAuth. The ONE recoverable entry (re-run bin/auth), included because recovering it is a manual re-authorization mid-stream.
    'commands.json'                          # commands.rs:180 StaticCommands::load - hand-curated static chat commands, managed live via !command add/edit/delete.
    'entrance-themes.json'                   # entrance_themes.rs:117 themes_path - per-user entrance themes set by mods via !settheme.
    'personal-playlists.json'                # main.rs - per-user song playlists.
    'song-queue.json'                        # song_requests.rs:363 persist_queue - the LIVE queue plus now-playing. Losing it mid-stream drops every queued request.
    'bugreports.json'                        # main.rs:415 BugReportManager::new - viewer-submitted reports awaiting review.
    'tips-history.json'                      # main.rs:458 streamelements watcher - StreamElements tip record.
    'paypal-tips-history.json'               # paypal.rs:56 - PayPal tip record. Both histories are financial records with no second copy.
    'channel-points-interrupt-reward.json'   # main.rs:588 - the Twitch reward ID. Losing it does not delete the reward; it makes the bot CREATE A SECOND ONE, leaving two identical rewards in the channel and orphaning redemptions against the first.
    'channel-points-theme-reward.json'       # main.rs:572 - same shape, same failure.
    'playrandom-state.json'                  # playrandom.rs:274 STATE_PATH - the !playrandom on/off flag. Tiny, and included because its absence has ALREADY been misread once as "continuous mode randomly stopping on its own" (that comment's own words) when it was every bot restart.
    'playrandom-log.json'                    # playrandom.rs PLAY_LOG_PATH - every random song that actually reached the stream (timestamp, id, title). A record of what was played, with no second copy anywhere. CAPPED at PLAY_LOG_MAX_ENTRIES = 10,000 entries, oldest dropped, because this file rides in every hourly snapshot and was the only allow-listed file with no ceiling; at ~100 bytes an entry that bounds it near a megabyte.
    'playrandom-history.json'                # playrandom.rs HISTORY_PATH - the last 100 random video ids, which is what the no-repeat window IS. Losing it does not break playback; it makes !playrandom start repeating songs it just played, which reads as the feature having silently stopped working.
    'playrandom-blocklist.json'              # playrandom.rs BLOCKLIST_PATH - random videos the overlay could not play, with the reason. Each entry cost one dead slot on stream to learn; losing the file means paying for every one of them again.
)

# DELIBERATELY EXCLUDED, each with the reason rather than by omission.
# An exclusion without a stated reason is indistinguishable from an
# oversight six months later.
#
#   search-cache.json    song_requests.rs:369 - a YouTube search-result
#                        cache, kept to save API quota. Rebuilt by
#                        re-querying. Losing it costs quota, not data.
#
#   daily-greeted.json   entrance_themes.rs:18 `GreetedToday { date:
#                        Option<NaiveDate>, users: HashSet<String> }`.
#                        It carries its own date and self-invalidates
#                        when the day rolls, so its maximum lifetime is
#                        one calendar day. The worst consequence of
#                        losing it is a duplicate greeting for the
#                        remainder of one stream.
#
#   commands-data.json   commands.rs:210 and entrance_themes.rs:205 -
#   themes-data.json     DERIVED public-site outputs, regenerated from
#                        commands.json / entrance-themes.json on every
#                        load and after every edit. They are also written
#                        into $PUBLIC_SITE_DIR, which is lokati.net's
#                        site folder and NOT the bot's directory, so they
#                        are outside this script's scope twice over.
#
#   *.tmp                state.rs write_atomic's transient temp files.
#                        Named here only to record that the allow-list is
#                        what excludes them; nothing else does.
#
#   .env                 -IncludeEnv, see that switch's own reasoning.

$SnapshotPrefix = 'bot-backup-'
$SnapshotStampFormat = 'yyyyMMdd-HHmmss'
$ManifestName = '_backup-manifest.json'

# ---------------------------------------------------------------------
# Helpers - deliberate mirrors of backup-game-data.ps1's, kept in step.
# ---------------------------------------------------------------------

function Get-Stamp {
    # Real offset, not a bare "Z" on a local clock - game-watchdog.ps1's
    # legacy format did the latter and it cost one session a false "log
    # gap" reading (docs/anomaly_ledger.md #44). New log surfaces do not
    # repeat that.
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
    # under me while I do" - the bot's writes and renames proceed exactly
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
    # Does this file parse as JSON? Returns Ok plus a human reason and a
    # Bom flag.
    #
    # Zero-length is a HARD failure, not an empty file: every path in the
    # manifest is written by `serde_json::to_string_pretty` and none of
    # them can legitimately produce zero bytes.
    #
    # A BOM is reported explicitly rather than treated as a failure. This
    # repo has already lost a save file to one (the August 2026
    # adventure-characters.json incident), and `serde_json` will not
    # parse through a BOM - so a BOM here means the file is ALREADY
    # broken for the bot, and saying so is the point.
    #
    # Every bot file in $CoreFiles is JSON. There is no TOML branch,
    # unlike the game's version of this function, because the bot
    # persists no TOML.
    #
    # `.env` is the one exception and it is checked as KEY=value instead.
    # Found by the first real run of this script (2026-09-08): with a
    # single JSON branch, -IncludeEnv made EVERY run degraded, and a
    # degraded run skips pruning - so the switch would have silently
    # disabled retention forever while still appearing to back up. The
    # kind is decided here, from the name, rather than at the two call
    # sites, so the dry run and the live run can never disagree about it.
    param([string] $Path)

    $leaf = Split-Path -Path $Path -Leaf

    $len = 0L
    try { $len = (Get-Item -LiteralPath $Path).Length } catch {
        return @{ Ok = $false; Reason = "not readable: $($_.Exception.Message)"; Bom = $false }
    }
    if ($len -eq 0) {
        return @{ Ok = $false; Reason = 'zero-length'; Bom = $false }
    }

    $bytes = $null
    try { $bytes = [IO.File]::ReadAllBytes($Path) } catch {
        return @{ Ok = $false; Reason = "read failed: $($_.Exception.Message)"; Bom = $false }
    }

    $bom = ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)
    $offset = 0
    if ($bom) { $offset = 3 }
    $text = [Text.Encoding]::UTF8.GetString($bytes, $offset, $bytes.Length - $offset)

    if ($leaf -eq '.env') {
        # dotenvy's own shape, checked no more strictly than dotenvy
        # itself would: at least one non-comment, non-blank line carrying
        # a `KEY=`. That catches a truncated or wrong-file copy without
        # this script pretending to be an .env parser and rejecting
        # something the bot would have loaded happily.
        $keyed = @($text -split "`r?`n" | Where-Object {
            $t = $_.Trim()
            $t.Length -gt 0 -and -not $t.StartsWith('#') -and $t -match '^[A-Za-z_][A-Za-z0-9_]*\s*='
        })
        if ($keyed.Count -gt 0) {
            return @{ Ok = $true; Reason = "env ok ($($keyed.Count) key(s))"; Bom = $bom }
        }
        return @{ Ok = $false; Reason = 'env has no KEY=value lines'; Bom = $bom }
    }

    try {
        $null = $text | ConvertFrom-Json
        return @{ Ok = $true; Reason = 'json ok'; Bom = $bom }
    } catch {
        return @{ Ok = $false; Reason = "json parse failed: $($_.Exception.Message)"; Bom = $bom }
    }
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
    # Earliest-of-day, not latest, for the same reason as the game's: a
    # backup tier consulted days later is being consulted because damage
    # went unnoticed, and the earliest snapshot of a day is the one with
    # the most of that day still ahead of it. Latest-of-day hands back
    # 23:xx, which for a 14:00 corruption is a backup of the corruption.
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

if (-not (Test-Path -LiteralPath $SourceDir -PathType Container)) {
    throw "SourceDir does not exist or is not a directory: $SourceDir"
}
$SourceDir = (Resolve-Path -LiteralPath $SourceDir).Path.TrimEnd('\')

if ([string]::IsNullOrWhiteSpace($BackupRoot)) {
    $parent = Split-Path -Path $SourceDir -Parent
    $leaf = Split-Path -Path $SourceDir -Leaf
    # `<leaf>-backups`, the same convention backup-game-data.ps1 uses. For
    # the shipped layout that resolves to `<repo>\bot-backups` - a sibling
    # of `bot\`, so never inside the directory being backed up, but it IS
    # inside the checkout, because the Windows deployment root and the
    # checkout are the same directory. `/bot-backups` is in .gitignore for
    # exactly that reason. The game's equivalent lands outside the
    # checkout only because the game's own SourceDir IS the checkout root.
    $BackupRoot = Join-Path $parent "$leaf-backups"
}
if (-not (Test-Path -LiteralPath $BackupRoot)) {
    $null = New-Item -ItemType Directory -Path $BackupRoot -Force
}
$BackupRoot = (Resolve-Path -LiteralPath $BackupRoot).Path.TrimEnd('\')

# The backup root must never sit inside the directory being backed up -
# that is how a backup ends up backing up its own snapshots.
if ($BackupRoot.ToLowerInvariant().StartsWith($SourceDir.ToLowerInvariant() + '\')) {
    throw "BackupRoot ($BackupRoot) is inside SourceDir ($SourceDir) - refusing to back a directory up into itself"
}

if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $LogPath = Join-Path $BackupRoot 'backup-bot-data.log'
}
$script:LogPath = $LogPath

$wantedFiles = @($CoreFiles)
if ($IncludeEnv) { $wantedFiles += '.env' }

$mode = 'live'
if ($DryRun) { $mode = 'dry-run' }
Write-Log "backup start ($mode) source=$SourceDir root=$BackupRoot retention=${HourlyRetentionHours}h/${DailyRetentionDays}d env=$([bool]$IncludeEnv)"

# ---------------------------------------------------------------------
# Absent-file reporting.
#
# Not every manifest entry exists on every install: a bot that has never
# had a channel-point reward created, or never had a song requested, has
# no such file, and that is correct rather than missing. But "absent" and
# "should have been there" are indistinguishable from silence, so both
# are reported and neither is a failure.
# ---------------------------------------------------------------------

$absent = @()
foreach ($name in $wantedFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $SourceDir $name) -PathType Leaf)) { $absent += $name }
}
if ($absent.Count -gt 0) {
    # -Console deliberately: a scheduled run's console goes nowhere, so
    # this costs nothing there, and an operator running it by hand is
    # precisely the person who needs to see which files were not found.
    Write-Log "absent (not an error - reported so 'missing' is never silent): $($absent -join ', ')" -Console
}

# ---------------------------------------------------------------------
# Dry run - verify the live sources in place, decide nothing else
# ---------------------------------------------------------------------

if ($DryRun) {
    $wouldCopy = 0
    foreach ($name in $wantedFiles) {
        $src = Join-Path $SourceDir $name
        if (-not (Test-Path -LiteralPath $src -PathType Leaf)) { continue }
        $v = Test-DataFile -Path $src
        $len = (Get-Item -LiteralPath $src).Length
        $flag = 'ok'
        if (-not $v.Ok) { $flag = "PROBLEM - $($v.Reason)" }
        if ($v.Bom) { $flag += ' (UTF-8 BOM)' }
        Write-Log ("would copy {0,-38} {1,9} bytes  {2}" -f $name, $len, $flag) -Console
        $wouldCopy++
    }

    $snapshots = Get-Snapshots -Root $BackupRoot
    foreach ($d in (Get-PruneDecisions -Snapshots $snapshots -Now (Get-Date))) {
        $verb = 'KEEP  '
        if (-not $d.Keep) { $verb = 'PRUNE ' }
        Write-Log "$verb $($d.Snapshot.Name) [$($d.Tier)] - $($d.Why)" -Console
    }
    Write-Log "dry run complete - $wouldCopy file(s) would be copied, $($snapshots.Count) existing snapshot(s)" -Console
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
                if ($v.Bom) { Write-Log "NOTE - $RelativeName carries a UTF-8 BOM; serde_json will NOT parse through it, so the LIVE file is already broken for the bot" -Console }
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

$verdict = 'clean'
if ($failed -gt 0) { $verdict = 'degraded' }

$manifest = [pscustomobject]@{
    createdAt = (Get-Stamp)
    sourceDir = $SourceDir
    verdict = $verdict
    filesCopied = $copied
    filesFailed = $failed
    filesAbsent = @($absent)
    bytes = $bytes
    includeEnv = [bool]$IncludeEnv
    entries = @($entries)
}
# BOM-less on purpose. This repo has already lost a save file to a BOM;
# nothing this script writes will be the next one.
$json = $manifest | ConvertTo-Json -Depth 6
[IO.File]::WriteAllText((Join-Path $snapshotDir $ManifestName), $json, (New-Object Text.UTF8Encoding($false)))

Write-Log "snapshot $snapshotName - $copied file(s), $([math]::Round($bytes / 1KB, 2)) KB, verdict=$verdict" -Console

# ---------------------------------------------------------------------
# Prune
# ---------------------------------------------------------------------

if ($verdict -ne 'clean') {
    # Never destroy history on a run that could not produce a good
    # snapshot. A degraded run is exactly when an incident may be under
    # way, and exactly when older snapshots are worth the most.
    Write-Log "verdict=$verdict - pruning SKIPPED, no snapshot deleted" -Console
    Write-Log "backup end - degraded"
    exit 1
}

$snapshots = Get-Snapshots -Root $BackupRoot
$decisions = Get-PruneDecisions -Snapshots $snapshots -Now (Get-Date)
$deleted = 0
foreach ($d in $decisions) {
    if ($d.Keep) { continue }
    try {
        Remove-Item -LiteralPath $d.Snapshot.Path -Recurse -Force -Confirm:$false
        $deleted++
        Write-Log "pruned $($d.Snapshot.Name) [$($d.Tier)] - $($d.Why)"
    } catch {
        Write-Log "prune FAILED for $($d.Snapshot.Name): $($_.Exception.Message)" -Console
    }
}

$survivors = @(Get-Snapshots -Root $BackupRoot)
$verifiedSurvivors = @($survivors | Where-Object { $_.Verified })
Write-Log "backup end - kept $($survivors.Count) snapshot(s) ($($verifiedSurvivors.Count) verified), pruned $deleted" -Console
