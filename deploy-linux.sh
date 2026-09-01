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

SRC="${1:?usage: deploy-linux.sh /path/to/source-root <release-name>}"
NAME="${2:?usage: deploy-linux.sh /path/to/source-root <release-name>}"
BIN=/opt/pathofdust/bin
DATA=/var/lib/pathofdust
BACKUP=/var/backups/pathofdust/deploy-pre-$NAME

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
mkdir -p "$BACKUP"
systemctl start pathofdust-backup.service   # the proven allow-list archive
cp -a "$BIN/game" "$BACKUP/game.pre-$NAME"  # the rollback slot
cp -a "$DATA/adventure-fights-summary" "$BACKUP/"   # pinned pre-deploy corpus
sha256sum "$BACKUP/game.pre-$NAME" > "$BACKUP/SHA256SUMS"
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
