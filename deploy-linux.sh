#!/bin/bash
# Path of Dust - Linux deploy. REFACTOR_PLAN.md section 13B.
#
# Replaces the Windows swap recipe (13A). The two mechanisms it drops:
#   * no maintenance flag - `systemctl stop` cannot race the restarter,
#     because Restart=always only fires on process EXIT, and a stop is
#     not an exit it acts on. See 13B for the full argument.
#   * no rename-the-locked-binary dance - Linux has no file lock; but we
#     still keep the previous binary, as the rollback slot.
#
# Backups: uses the proven backup-game-data.sh, then ALSO pins the
# fight-summary tier, mirroring 13A step 4's pinned pre-deploy snapshot.
# No rsync --delete and no rm -rf against $DATA anywhere in this file.
set -euo pipefail

SRC="${1:?usage: deploy-linux.sh /path/to/source-root <release-name> [commit-sha]}"
NAME="${2:?usage: deploy-linux.sh /path/to/source-root <release-name> [commit-sha]}"
# Optional. The tree is a `git archive` extraction with no .git, so the
# script cannot derive this - §13B passes the commit it archived. Absent,
# the slot records "unknown" rather than lying.
COMMIT="${3:-}"
BIN=/opt/pathofdust/bin
DATA=/var/lib/pathofdust
BACKUP_ROOT=/var/backups/pathofdust

# THE SLOT NAME CARRIES A TIMESTAMP, AND THE TIMESTAMP COMES FIRST.
#
# It used to be `deploy-pre-$NAME` - the release name alone - so
# redeploying a name landed on the previous slot and overwrote it.
# That happened on 2026-09-03 and was harmless only because the two
# binaries were byte-identical by coincidence.
#
# WHY IT IS WORSE THAN LOSING A FILE: the collision overwrote
# `SHA256SUMS` alongside the binary, so `rollback-linux.sh` would have
# rolled forward to the WRONG binary and PASSED its integrity check while
# reporting success. A lost file announces itself. A lost file with a
# matching checksum does not. That is the whole reason this changed.
#
# Timestamp FIRST so that lexical sort is chronological sort and nothing
# has to parse anything - `ls` alone orders these correctly. The format
# matches `pod-backup-YYYYMMDD-HHMMSS` deliberately: one convention on
# this box, not two. A commit SHA was considered and rejected for the
# name - it does not sort, it still collides when the same commit is
# redeployed, and it means nothing to a human at 3am. It is recorded
# INSIDE the slot instead, where it answers a different question.
STAMP=$(date +%Y%m%d-%H%M%S)
SLOT_NAME="deploy-pre-$STAMP-$NAME"
BACKUP="$BACKUP_ROOT/$SLOT_NAME"

NEW="$SRC/target/release/game"
[ -x "$NEW" ] || { echo "FATAL: no built binary at $NEW"; exit 1; }

OLD_HASH=$(sha256sum "$BIN/game" | cut -d' ' -f1)
NEW_HASH=$(sha256sum "$NEW"      | cut -d' ' -f1)
echo "old binary sha256: $OLD_HASH"
echo "new binary sha256: $NEW_HASH"
if [ "$OLD_HASH" = "$NEW_HASH" ]; then
  echo "FATAL: new binary is identical to the live one - nothing to deploy"; exit 1
fi

# ---- backup before swap -------------------------------------------------
# `mkdir` WITHOUT -p, deliberately: -p succeeds silently on an existing
# directory, which is exactly how the old scheme overwrote a live
# recovery path. This is a guard, not an assumption - refusing is the
# whole point, and the timestamp makes a genuine collision impossible
# anyway (two deploys cannot share a second on one box).
if [ -e "$BACKUP" ]; then
  echo "FATAL: rollback slot $BACKUP already exists - refusing to overwrite a recovery path"; exit 1
fi
mkdir "$BACKUP"
systemctl start pathofdust-backup.service   # the proven allow-list archive
cp -a "$BIN/game" "$BACKUP/game.pre-$NAME"  # the rollback slot
cp -a "$DATA/adventure-fights-summary" "$BACKUP/"   # pinned pre-deploy corpus
# Comment lines carry what the filename deliberately does not. Readers
# must skip `#` lines - `rollback-linux.sh` does, and legacy slots with no
# comments parse identically under the same rule.
{
  echo "# release: $NAME"
  echo "# slot:    $SLOT_NAME"
  echo "# commit:  ${COMMIT:-unknown}"
  echo "# saved:   $(date -Is)"
  sha256sum "$BACKUP/game.pre-$NAME"
} > "$BACKUP/SHA256SUMS"
echo "rollback slot: $BACKUP/game.pre-$NAME"

# ---- the gate + swap ----------------------------------------------------
T0=$(date +%s.%N)
systemctl stop pathofdust                   # THIS is the maintenance gate

install -o root -g root -m 0755 "$NEW" "$BIN/game.new"
mv -f "$BIN/game.new" "$BIN/game"

for d in templates wiki public_adventure_overlay; do
  mkdir -p "$DATA/$d"
  cp -r --preserve=mode,timestamps "$SRC/$d/." "$DATA/$d/"
done
chown -R pathofdust:pathofdust "$DATA"

systemctl start pathofdust
T1=$(date +%s.%N)
echo "downtime: $(echo "$T0 $T1" | awk '{printf "%.2f", $2-$1}')s"

LIVE_HASH=$(sha256sum "$BIN/game" | cut -d' ' -f1)
[ "$LIVE_HASH" = "$NEW_HASH" ] || { echo "FATAL: live binary hash is not the new one"; exit 1; }
echo "live binary sha256 confirmed: $LIVE_HASH"

# ---- LATEST, repointed only after the deploy is known good --------------
#
# Answers "which slot do I roll back to" with `ls -l`, no command to
# remember and no document to consult. It exists because `ls -lt` LIES
# about slot order: `cp -a` above preserves the source timestamp, so every
# slot's binary carries its PREDECESSOR's install time. Sorting by mtime
# is plausible, consistently off by one deploy, and wrong.
#
# Relative target, so the whole backup root can be moved or copied without
# breaking it. Written via a temp name and `mv -T`, which is a rename(2)
# and therefore atomic - a reader either sees the old link or the new one,
# never a missing one.
#
# LATEST tracks the newest BINARY rollback slot. §13B.8's template-only
# releases deliberately do NOT repoint it: rolling templates back is a
# different operation with a different slot shape, and pointing the
# binary-rollback path at a slot holding no binary would be a trap.
ln -sfn "$SLOT_NAME" "$BACKUP_ROOT/.deploy-pre-LATEST.new"
mv -Tf "$BACKUP_ROOT/.deploy-pre-LATEST.new" "$BACKUP_ROOT/deploy-pre-LATEST"
echo "deploy-pre-LATEST -> $SLOT_NAME"
