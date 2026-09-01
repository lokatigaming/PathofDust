#!/bin/bash
# Path of Dust - Linux rollback. REFACTOR_PLAN.md section 13B.
# Puts back the binary this deploy replaced. Data is NOT rolled back -
# see 13B for why that is deliberate.
set -euo pipefail
NAME="${1:?usage: rollback-linux.sh <release-name>}"
BIN=/opt/pathofdust/bin
BACKUP=/var/backups/pathofdust/deploy-pre-$NAME
SLOT="$BACKUP/game.pre-$NAME"
[ -x "$SLOT" ] || { echo "FATAL: no rollback slot at $SLOT"; exit 1; }

WANT=$(cut -d' ' -f1 "$BACKUP/SHA256SUMS")
HAVE=$(sha256sum "$SLOT" | cut -d' ' -f1)
[ "$WANT" = "$HAVE" ] || { echo "FATAL: rollback slot hash mismatch"; exit 1; }

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
