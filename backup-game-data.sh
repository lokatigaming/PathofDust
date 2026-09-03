#!/bin/bash
# Linux port of backup-game-data.ps1 - scheduled backup of the adventure
# game's persisted state, for the Debian staging box and (after cutover)
# for production.
#
# See docs/linux_backups.md for what is covered, what is not, the
# retention reasoning, and the restore procedure.
#
# SAFE TO RUN AGAINST A LIVE GAME. Like its PowerShell ancestor this
# script:
#   * COPIES, never moves, never renames, never writes into $SOURCE.
#   * NEVER touches a process. It does not start, stop, query or
#     enumerate one, by name or otherwise (CLAUDE.md PRODUCTION SAFETY).
#     There is deliberately no process code in this file.
#   * verifies every copy parses before it prunes anything, and refuses
#     to prune at all if this run is degraded.
#
# WHAT IS DIFFERENT FROM THE WINDOWS SCRIPT, AND WHY
#
# 1. Directory members are verified and recorded like every other file.
#    The PowerShell version copies $SmallDirs blind - no parse, no retry,
#    successes never recorded in the manifest, and a MISSING directory is
#    skipped silently, which yields a "clean" snapshot containing zero
#    sprites. Here a missing expected directory is a hard failure and
#    every JSON member is parsed.
#
# 2. TOML is really parsed, with tomllib. PowerShell 5.1 has no TOML
#    parser so the original had to settle for a bracket-balance check.
#
# 3. Files are staged into a temp tree, verified there, hashed there, and
#    only then archived. The manifest therefore describes exactly what is
#    in the tarball, with no read-it-twice race between hashing and tar.
#
# 4. Output is one .tar.gz plus a .sha256 sidecar, not a directory tree,
#    because the off-box copy is a pull of whole files.

set -Eeuo pipefail

SOURCE="${1:-/var/lib/pathofdust}"
DEST="${2:-/var/backups/pathofdust}"
KEEP="${KEEP:-90}"
COPY_RETRIES="${COPY_RETRIES:-3}"
RETRY_DELAY="${RETRY_DELAY:-0.75}"

log()  { printf '%s - %s\n' "$(date --rfc-3339=seconds)" "$*"; }
fail() { printf '%s - FAILURE: %s\n' "$(date --rfc-3339=seconds)" "$*" >&2; exit 1; }
trap 'rc=$?; [ $rc -ne 0 ] && printf "%s - ABORTED (exit %d) at line %d\n" "$(date --rfc-3339=seconds)" "$rc" "$LINENO" >&2; exit $rc' ERR

# ---------------------------------------------------------------------
# The manifest, derived from the code, mirroring backup-game-data.ps1.
# Each entry's proof-of-persistence lives in that script's comments; this
# list is kept deliberately in the same order so the two can be diffed.
# ---------------------------------------------------------------------

CORE_FILES=(
  adventure-characters.json
  adventure-world.json
  adventure-reforge-cooldown.json
  adventure-rampage-state.json
  adventure-sessions.json
  adventure-accounts.json
  adventure-live-tunables.toml
  adventure-passive-overrides.toml
  adventure-item-balance.toml
  adventure-sprite-count.json
  patch-notes.json
  adventure-last-fights.json
  # Player-submitted bug reports, added with /bugs 2026-09-03. Like
  # MARKER_FILES below, this array is a hand-maintained literal list, and
  # the drift check further down only covers MARKERS - a core data file
  # that is missing from here is backed up by nothing and warned about by
  # nothing. Caught at deploy rather than in the commit that added the
  # file; without this line every bug a player files would have been
  # outside the backup set from the first one.
  adventure-bugreports.json
)

MARKER_FILES=(
  adventure-fights-storage-migration-marker.json
  adventure-crit-reforge-equipped-backfill-marker.json
  adventure-craft-token-backfill-marker.json
  adventure-craft-token-backfill-v2-marker.json
  adventure-pity-launch-marker.json
  adventure-wings-launch-grant-marker.json
  adventure-passive-key-rename-marker.json
  adventure-kibukah-compensation-marker.json
  adventure-celestial-shard-first-award-marker.json
  adventure-unique-shard-first-award-marker.json
  adventure-wings-giveaway-marker.json
  adventure-helm-rebalance-v2-marker.json
  adventure-power-roll-backfill-marker.json
  adventure-krangle-accuracy-marker.json
  adventure-item-accuracy-marker.json
  adventure-crit-value-nerf-marker.json
  adventure-gloves-speed-rebalance-marker.json
  adventure-crit-lineage-backfill-marker.json
  adventure-crit-flag-to-affix-tracking-marker.json
  adventure-flowlikewater-swap-marker.json
  adventure-celestial-shard-into-unique-shard-marker.json
  adventure-duplicate-unique-effects-cleanup-marker.json
  adventure-lingering-effect-to-echo-marker.json
  # 2026-09-03, affix tier curve. This one matters more than most: its
  # migration (`migrate_affix_tier_curve`) is deliberately NOT idempotent
  # - it multiplies every stored affix value by f(tier)/tier - so a
  # restore that brought back the characters file WITHOUT this marker
  # would let the rescale run a second time and apply the cut twice.
  # NOTE FOR WHOEVER ADDS THE NEXT ONE: this array is a hand-maintained
  # literal list, NOT a glob over adventure-*-marker.json. The drift
  # check near the bottom of this script will TELL you when a marker on
  # disk is missing from here, but it will not back the file up for you.
  adventure-affix-tier-curve-marker.json
)

# Fight-tier sequence counters. THESE ARE COPIED LAST, AFTER the fight
# directories, and the ordering is load-bearing.
#
# `next_seq` (fight_storage.rs:87) persists the counter BEFORE writing
# the fight file it names, so on disk the counter is always >= the
# highest-numbered file present. Copy the counter first and a fight
# written in between lands in the archive while the archived counter
# still points below it - restore that pair and the next fight overwrites
# a fight you just restored. Copy the counter last and the captured
# counter is always >= the captured directory, which is the safe
# direction: the next write goes to a fresh number.
SEQ_FILES=(
  adventure-fights-coarse-seq.json
  adventure-fights-detail-seq.json
  adventure-fights-summary-seq.json
  adventure-fights-bundle-seq.json
)

# Always included, and - unlike the PowerShell original - REQUIRED. A
# missing one aborts the run instead of producing a clean-looking
# snapshot with nothing in it.
SMALL_DIRS=(
  adventure-fights-summary
  public_adventure_overlay/sprites/custom
)

# Excluded by size, same criterion as the Windows script's
# -IncludeFightArchives. `adventure-fights-pinned` is the one the game
# never prunes, so a non-empty one is called out rather than left looking
# covered.
ARCHIVE_DIRS=(
  adventure-fights-coarse
  adventure-fights-detail
  adventure-fights-bundle
  adventure-fights-pinned
)

[ -d "$SOURCE" ] || fail "source directory does not exist: $SOURCE"
mkdir -p "$DEST"

case "$(readlink -f "$DEST")/" in
  "$(readlink -f "$SOURCE")"/*) fail "destination must not live inside the source ($DEST is inside $SOURCE)" ;;
esac

STAMP="$(date +%Y%m%d-%H%M%S)"
NAME="pod-backup-$STAMP"
STAGE="$DEST/.staging-$NAME.$$"
ARCHIVE="$DEST/$NAME.tar.gz"
trap 'rm -rf "$STAGE" "$DEST/.verify.py"' EXIT

log "backup start source=$SOURCE dest=$DEST keep=$KEEP"
mkdir -p "$STAGE"

FAILURES=0
COPIED=0

# Copy one file and verify the COPY (not the source): a verify failure
# means the copy landed mid-write, so retrying is what fixes it.
stage_one() {
  local rel="$1" required="${2:-no}"
  local src="$SOURCE/$rel" dst="$STAGE/$rel"
  if [ ! -f "$src" ]; then
    if [ "$required" = "yes" ]; then
      log "MISSING REQUIRED FILE: $rel"
      FAILURES=$((FAILURES + 1))
    fi
    return 0
  fi
  mkdir -p "$(dirname "$dst")"
  local attempt=0 reason=''
  while [ "$attempt" -lt "$COPY_RETRIES" ]; do
    attempt=$((attempt + 1))
    if cp -p -- "$src" "$dst" 2>/dev/null; then
      if reason="$(python3 "$DEST/.verify.py" "$dst" 2>&1)"; then
        COPIED=$((COPIED + 1))
        return 0
      fi
    else
      reason="copy failed"
    fi
    [ "$attempt" -lt "$COPY_RETRIES" ] && sleep "$RETRY_DELAY"
  done
  log "VERIFY FAILED after $attempt attempt(s): $rel - $reason"
  FAILURES=$((FAILURES + 1))
  return 0
}

# The verifier. Zero-length is a hard failure: every path in the manifest
# is written by serde_json or toml::to_string_pretty and none of them can
# legitimately produce zero bytes, so an empty result is a truncated
# write caught mid-flight. Binary members (sprites) are checked for
# non-emptiness only - there is nothing to parse.
cat > "$DEST/.verify.py" <<'PY'
import json
import sys
import tomllib

path = sys.argv[1]
try:
    raw = open(path, "rb").read()
except OSError as err:
    sys.exit("unreadable: %s" % err)

if not raw:
    sys.exit("zero-length (truncated write caught mid-flight?)")

if path.endswith((".json", ".toml")):
    if b"\x00" in raw:
        sys.exit("contains NUL bytes (not text)")
    body = raw[3:] if raw.startswith(b"\xef\xbb\xbf") else raw
    try:
        text = body.decode("utf-8")
    except UnicodeDecodeError as err:
        sys.exit("not valid UTF-8: %s" % err)
    if not text.strip():
        sys.exit("whitespace only")
    try:
        if path.endswith(".toml"):
            tomllib.loads(text)
        else:
            parsed = json.loads(text)
    except Exception as err:
        sys.exit("parse failed: %s" % err)
    # The one name-specific arm, same reasoning as the PowerShell
    # original: a lost password hash has no external identity provider to
    # re-authenticate against, so the account is simply gone. `{}` passes
    # deliberately - that is the legitimate state before anyone has
    # registered.
    if path.endswith("adventure-accounts.json"):
        if not isinstance(parsed, dict):
            sys.exit("accounts shape check failed: not a JSON object")
        for login, acct in parsed.items():
            h = (acct or {}).get("password_hash") if isinstance(acct, dict) else None
            if not h:
                sys.exit("accounts shape check failed: %r has no password_hash" % login)
            if not h.startswith("$argon2"):
                sys.exit("accounts shape check failed: %r has a non-argon2 password_hash" % login)
print("ok")
PY

# Marker drift: the list above was derived from the code, so compare it
# against the glob every run. Anything the glob finds IS backed up
# regardless - the report exists to get the list updated, not to skip a
# file. Same durable lesson CLAUDE.md records for form POSTs: derive the
# set from reality, do not hand-maintain it and hope.
DRIFT=()
while IFS= read -r found; do
  [ -z "$found" ] && continue
  known=no
  for m in "${MARKER_FILES[@]}"; do [ "$m" = "$found" ] && known=yes && break; done
  [ "$known" = no ] && DRIFT+=("$found")
done < <(cd "$SOURCE" && find . -maxdepth 1 -name 'adventure-*-marker.json' -printf '%f\n' 2>/dev/null | sort)

if [ "${#DRIFT[@]}" -gt 0 ]; then
  log "MANIFEST DRIFT - ${#DRIFT[@]} marker file(s) on disk are not in this script's code-derived list (they ARE being backed up; update MARKER_FILES): ${DRIFT[*]}"
fi

# ---------------------------------------------------------------------
# Stage. Order matters: fight directories BEFORE the seq counters.
# ---------------------------------------------------------------------

for f in "${CORE_FILES[@]}"; do stage_one "$f"; done
for f in "${MARKER_FILES[@]}"; do stage_one "$f"; done
for f in "${DRIFT[@]}"; do stage_one "$f"; done

for d in "${SMALL_DIRS[@]}"; do
  if [ ! -d "$SOURCE/$d" ]; then
    log "MISSING REQUIRED DIRECTORY: $d - refusing to produce a snapshot that would look complete without it"
    FAILURES=$((FAILURES + 1))
    continue
  fi
  n=0
  while IFS= read -r rel; do
    stage_one "$d/$rel"
    n=$((n + 1))
  done < <(cd "$SOURCE/$d" && find . -type f ! -name '*.tmp' -printf '%P\n' | sort)
  log "staged $d - $n file(s)"
done

# Seq counters last. See SEQ_FILES' comment.
for f in "${SEQ_FILES[@]}"; do stage_one "$f"; done

for d in "${ARCHIVE_DIRS[@]}"; do
  [ -d "$SOURCE/$d" ] || continue
  n="$(find "$SOURCE/$d" -type f ! -name '*.tmp' | wc -l)"
  if [ "$d" = "adventure-fights-pinned" ] && [ "$n" -gt 0 ]; then
    log "NOTE - adventure-fights-pinned holds $n mod-pinned file(s) and is NOT covered by this backup"
  fi
done

# `write_atomic` (state.rs) names its temp files <stem>.<pid>.<n>.tmp in
# the same directory as the target. They are excluded above; this asserts
# none slipped into the staging tree by another route.
if find "$STAGE" -name '*.tmp' | grep -q .; then
  fail "a .tmp file reached the staging tree - the exclude is not holding"
fi

VERDICT=clean
[ "$FAILURES" -gt 0 ] && VERDICT=degraded

# ---------------------------------------------------------------------
# Manifest, written INTO the archive, describing exactly what is in it.
# ---------------------------------------------------------------------

python3 - "$STAGE" "$SOURCE" "$VERDICT" "$COPIED" "$FAILURES" "${DRIFT[*]:-}" <<'PY'
import hashlib
import json
import os
import subprocess
import sys

stage, source, verdict, copied, failures, drift = sys.argv[1:7]
entries = []
for root, _dirs, files in os.walk(stage):
    for f in sorted(files):
        p = os.path.join(root, f)
        rel = os.path.relpath(p, stage).replace(os.sep, "/")
        if rel == "_backup-manifest.json":
            continue
        h = hashlib.sha256(open(p, "rb").read()).hexdigest()
        entries.append({"name": rel, "bytes": os.path.getsize(p), "sha256": h})

manifest = {
    "createdAt": subprocess.run(["date", "--rfc-3339=seconds"], capture_output=True, text=True).stdout.strip(),
    "sourceDir": source,
    "verdict": verdict,
    "filesCopied": int(copied),
    "filesFailed": int(failures),
    "bytes": sum(e["bytes"] for e in entries),
    "manifestDrift": drift.split() if drift.strip() else [],
    "entries": entries,
}
# BOM-less on purpose. This repo has already lost a save file to a BOM.
with open(os.path.join(stage, "_backup-manifest.json"), "w", encoding="utf-8", newline="\n") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")
print("manifest: %d entries, %d bytes" % (len(entries), manifest["bytes"]))
PY

# ---------------------------------------------------------------------
# Archive, checksum, and prove it reads back.
# ---------------------------------------------------------------------

tar -czf "$ARCHIVE" -C "$STAGE" . || fail "tar failed for $ARCHIVE"
( cd "$DEST" && sha256sum "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256" )

gzip -t "$ARCHIVE" || fail "gzip integrity check failed for $ARCHIVE"
( cd "$DEST" && sha256sum -c --quiet "$(basename "$ARCHIVE").sha256" ) || fail "checksum does not match the archive just written"

STAGED_COUNT="$(find "$STAGE" -type f | wc -l)"
ARCHIVE_COUNT="$(tar -tzf "$ARCHIVE" | grep -vc '/$' || true)"
[ "$STAGED_COUNT" -eq "$ARCHIVE_COUNT" ] \
  || fail "archive holds $ARCHIVE_COUNT file(s) but $STAGED_COUNT were staged"

log "snapshot $NAME - $COPIED file(s), $(du -h "$ARCHIVE" | cut -f1), $ARCHIVE_COUNT archive members, verdict=$VERDICT"

if [ "$VERDICT" != clean ]; then
  # Never destroy history on a run that could not produce a good
  # snapshot. A degraded run is exactly when an incident may be under
  # way, and that is the worst moment to be deleting older copies.
  log "PRUNE SKIPPED - this run is degraded ($FAILURES failure(s)); older archives left untouched"
  fail "backup degraded: $FAILURES file(s) failed to copy or verify"
fi

# ---------------------------------------------------------------------
# Prune. Keep the newest $KEEP archives; abort if the plan would leave
# nothing verified.
# ---------------------------------------------------------------------

mapfile -t ALL < <(find "$DEST" -maxdepth 1 -name 'pod-backup-*.tar.gz' -printf '%f\n' | sort)
if [ "${#ALL[@]}" -le "$KEEP" ]; then
  KEPT=("${ALL[@]}")
else
  KEPT=("${ALL[@]: -KEEP}")
fi
# An archive counts as verified only if its sidecar matches AND its own
# embedded manifest says `clean` - the same rule the PowerShell version
# applies to a snapshot directory's manifest. A degraded archive is still
# KEPT (a partial snapshot beats none), but it must never be the reason
# pruning decides there is still good history to fall back on.
VERIFIED=0
for a in "${KEPT[@]}"; do
  [ -f "$DEST/$a.sha256" ] || continue
  ( cd "$DEST" && sha256sum -c --quiet "$a.sha256" ) 2>/dev/null || continue
  v="$(tar -xzOf "$DEST/$a" ./_backup-manifest.json 2>/dev/null \
       | python3 -c 'import json,sys; print(json.load(sys.stdin).get("verdict",""))' 2>/dev/null || true)"
  [ "$v" = clean ] && VERIFIED=$((VERIFIED + 1))
done

if [ "$VERIFIED" -eq 0 ]; then
  log "PRUNE ABORTED - the retention plan would leave zero verified archives; nothing deleted"
  fail "retention arithmetic left no verified archive"
fi

DELETED=0
for a in "${ALL[@]}"; do
  keep=no
  for k in "${KEPT[@]}"; do [ "$k" = "$a" ] && keep=yes && break; done
  if [ "$keep" = no ]; then
    rm -f -- "$DEST/$a" "$DEST/$a.sha256"
    DELETED=$((DELETED + 1))
    log "pruned $a (older than the newest $KEEP)"
  fi
done

log "backup end - kept ${#KEPT[@]} archive(s) ($VERIFIED verified), pruned $DELETED"
