#!/bin/bash
# Path of Dust - Linux rollback. REFACTOR_PLAN.md section 13B.
# Puts back the binary this deploy replaced. Data is NOT rolled back -
# see 13B for why that is deliberate.
#
# USAGE - all four forms, because the one that matters is used in a hurry:
#
#   rollback-linux.sh                 roll back to the newest deploy (LATEST)
#   rollback-linux.sh <release-name>  newest slot for that release
#   rollback-linux.sh <slot-dir-name> that exact slot
#   rollback-linux.sh --list          every slot, newest first
#
# The no-argument form is the incident form: "undo what just happened".
# It follows the `deploy-pre-LATEST` symlink that deploy-linux.sh
# repoints after each successful deploy.
#
# WHY NOT JUST SORT BY mtime: `deploy-linux.sh` saves the slot with
# `cp -a`, which preserves the source timestamp, so a slot's binary
# carries its PREDECESSOR's install time - every entry is off by one
# deploy. Ordering here comes from the timestamp in the slot NAME, never
# from mtime. Legacy `deploy-pre-<name>` slots have no timestamp in the
# name and fall back to directory mtime, which is approximate; they are
# listed as `legacy` so nobody mistakes the ordering for exact.
set -euo pipefail

ROOT=/var/backups/pathofdust
BIN=/opt/pathofdust/bin

# Sort key + metadata for one slot directory. Emits:
#   <sortkey>\t<slotname>\t<when>\t<shape>
slot_row() {
  local dir="$1" name when key shape
  name=$(basename "$dir")
  if [[ "$name" =~ ^deploy-pre-([0-9]{8})-([0-9]{6})-(.+)$ ]]; then
    key="${BASH_REMATCH[1]}${BASH_REMATCH[2]}"
    when="${BASH_REMATCH[1]:0:4}-${BASH_REMATCH[1]:4:2}-${BASH_REMATCH[1]:6:2} ${BASH_REMATCH[2]:0:2}:${BASH_REMATCH[2]:2:2}:${BASH_REMATCH[2]:4:2}"
    shape="stamped"
  else
    # Legacy shape. Directory mtime, formatted the same way so the two
    # sort against each other, but flagged as approximate.
    key=$(date -d "@$(stat -c %Y "$dir")" +%Y%m%d%H%M%S)
    when=$(stat -c %y "$dir" | cut -d. -f1)
    shape="legacy"
  fi
  printf '%s\t%s\t%s\t%s\n' "$key" "$name" "$when" "$shape"
}

# Every slot, newest first.
all_slots() {
  local d
  for d in "$ROOT"/deploy-pre-*/; do
    [ -d "$d" ] || continue
    [ "$(basename "$d")" = "deploy-pre-LATEST" ] && continue   # the symlink
    slot_row "${d%/}"
  done | sort -r
}

# The single `game.pre-*` inside a slot, or empty if there is none
# (§13B.8 template-only slots hold templates and no binary).
slot_binary() {
  local dir="$1" b
  b=$(find "$dir" -maxdepth 1 -type f -name 'game.pre-*' | head -1)
  printf '%s' "$b"
}

# The hash out of SHA256SUMS, skipping `#` comment lines. Legacy slots
# have no comments and parse identically under the same rule.
slot_hash() {
  awk '!/^#/ && NF {print $1; exit}' "$1/SHA256SUMS" 2>/dev/null || true
}

do_list() {
  printf '%-46s  %-19s  %-7s  %-12s  %s\n' SLOT WHEN SHAPE HASH BINARY
  local key name when shape dir bin hash
  while IFS=$'\t' read -r key name when shape; do
    dir="$ROOT/$name"
    bin=$(slot_binary "$dir")
    hash=$(slot_hash "$dir")
    printf '%-46s  %-19s  %-7s  %-12s  %s\n' \
      "$name" "$when" "$shape" "${hash:0:12}" "${bin:+$(basename "$bin")}${bin:-<none - template-only>}"
  done < <(all_slots)
  if [ -L "$ROOT/deploy-pre-LATEST" ]; then
    echo
    echo "LATEST -> $(readlink "$ROOT/deploy-pre-LATEST")"
  else
    echo
    echo "LATEST -> (not set yet - no deploy has run under the timestamped scheme)"
  fi
}

# ---- resolve which slot ------------------------------------------------
ARG="${1:-}"

if [ "$ARG" = "--list" ] || [ "$ARG" = "-l" ]; then
  do_list; exit 0
fi

if [ -z "$ARG" ]; then
  if [ ! -L "$ROOT/deploy-pre-LATEST" ]; then
    echo "FATAL: no deploy-pre-LATEST symlink, so there is no 'last deploy' to undo."
    echo "Pick one explicitly - here is what exists:"; echo
    do_list
    exit 1
  fi
  SLOT_NAME=$(readlink "$ROOT/deploy-pre-LATEST")
  WHY="LATEST"
elif [ -d "$ROOT/$ARG" ]; then
  SLOT_NAME="$ARG"
  WHY="exact slot"
else
  # Treat as a release name: newest slot named for it, either shape.
  MATCHES=$(all_slots | awk -F'\t' -v r="$ARG" '$2 == "deploy-pre-" r || $2 ~ "^deploy-pre-[0-9]{8}-[0-9]{6}-" r "$" {print $2}')
  [ -n "$MATCHES" ] || { echo "FATAL: no slot for release '$ARG'. Try --list."; exit 1; }
  SLOT_NAME=$(printf '%s\n' "$MATCHES" | head -1)
  WHY="newest slot for release '$ARG'"
  if [ "$(printf '%s\n' "$MATCHES" | wc -l)" -gt 1 ]; then
    echo "NOTE: $(printf '%s\n' "$MATCHES" | wc -l) slots match '$ARG'; choosing the newest:"
    printf '  %s\n' $MATCHES
  fi
fi

BACKUP="$ROOT/$SLOT_NAME"
[ -d "$BACKUP" ] || { echo "FATAL: $BACKUP is not a directory"; exit 1; }

SLOT=$(slot_binary "$BACKUP")
if [ -z "$SLOT" ]; then
  echo "FATAL: $SLOT_NAME holds no game.pre-* binary."
  echo "It is a template-only slot (§13B.8); rolling those back is a different procedure."
  exit 1
fi

WANT=$(slot_hash "$BACKUP")
[ -n "$WANT" ] || { echo "FATAL: no usable hash in $BACKUP/SHA256SUMS"; exit 1; }
HAVE=$(sha256sum "$SLOT" | cut -d' ' -f1)
[ "$WANT" = "$HAVE" ] || { echo "FATAL: rollback slot hash mismatch (want $WANT, have $HAVE)"; exit 1; }

# SAY WHAT IS ABOUT TO HAPPEN, BEFORE IT HAPPENS. This is the part that
# makes the tool safe to use in a hurry: the operator sees which slot was
# chosen and why, and can Ctrl-C if it is not the one they meant.
echo "rolling back using : $SLOT_NAME  ($WHY)"
echo "  binary           : $(basename "$SLOT")"
echo "  sha256           : $HAVE  (verified against SHA256SUMS)"
grep -E '^# (release|commit|saved):' "$BACKUP/SHA256SUMS" 2>/dev/null | sed 's/^/  /' || true
echo "  currently live   : $(sha256sum "$BIN/game" | cut -d' ' -f1)"
echo

T0=$(date +%s.%N)
systemctl stop pathofdust
install -o root -g root -m 0755 "$SLOT" "$BIN/game.new"
mv -f "$BIN/game.new" "$BIN/game"
systemctl start pathofdust
for i in $(seq 1 120); do
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1:4005/ || true)
  [ "$code" = "200" ] && break
  sleep 0.5
done
T1=$(date +%s.%N)
echo "rolled back to $HAVE"
echo "dashboard HTTP $code"
echo "rollback wall-clock: $(echo "$T0 $T1" | awk '{printf "%.2f", $2-$1}')s"
