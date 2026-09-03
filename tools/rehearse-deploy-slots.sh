#!/bin/bash
# Rehearsal harness for deploy-linux.sh and rollback-linux.sh.
#
# WHAT THIS IS
# ------------
# It runs the REAL deploy and rollback scripts against a throwaway
# directory tree, with fake "binaries" that are just text files, and
# checks the behaviour that matters for rollback safety: that a slot is
# never silently overwritten, that a recovery path survives a redeploy of
# the same release name, that legacy slots still resolve, and that the
# listing comes out newest-first.
#
# It exists because on 2026-09-03 it found two defects that reading the
# scripts did not - a sort that was non-deterministic on a tied
# timestamp, and a `${x:+a}${x:-b}` idiom that printed BOTH branches, so
# the rollback listing showed `game.pre-alpha/full/path/to/game.pre-alpha`
# in exactly the column a person would be squinting at during a rollback
# at 3am.
#
# HOW IT AVOIDS TOUCHING ANYTHING REAL
# ------------------------------------
# It copies both scripts, `sed`s their three production paths to a
# scratch root, and prepends no-op stubs:
#
#     systemctl() { :; }      # no service is stopped or started
#     chown()     { :; }      # no ownership is changed
#     curl()      { echo 200; }   # the health poll always "succeeds"
#
# Nothing under /var/backups/pathofdust, /opt/pathofdust or
# /var/lib/pathofdust is read or written. The scratch root is deleted and
# recreated on every run.
#
# THE THREE PATHS IT REWRITES - RE-POINT THESE IF PRODUCTION MOVES
# ---------------------------------------------------------------
#     /var/backups/pathofdust   ->  $R/backups
#     /opt/pathofdust/bin       ->  $R/bin
#     /var/lib/pathofdust       ->  $R/data
# These are matched as literal strings. If a path in either script
# changes, or a new absolute path is introduced, the sed below stops
# covering it and the harness will start operating on the REAL location.
# That is the one way this file can become dangerous rather than merely
# stale, so check it whenever the scripts gain a path.
#
# IT WILL DRIFT, AND IT IS ONLY HONEST IF YOU RUN IT
# --------------------------------------------------
# The assertions encode what the scripts were expected to do in September
# 2026. They are not run by CI and nothing forces them to stay true.
# Assume it is stale until you have run it and seen it pass. A harness
# that is honest about being stale is worth more than no harness; one
# that is quietly wrong is worth less than none.
#
# Note also, from the same day: an assertion that encodes an expected
# ANSWER goes stale the moment the fixture grows. T5 below originally
# asserted "slot X is newest", which broke as soon as a later test
# created another slot. It now asserts the PROPERTY - rows descend in
# time - which is what was actually meant.
#
# USAGE
# -----
#     bash tools/rehearse-deploy-slots.sh [scratch-root]
#
# Run it on Linux (it relies on GNU `stat -c`, `date -d` and `mv -T`).
# Exits non-zero if any check fails.
set -uo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
SRC_DEPLOY="$REPO/deploy-linux.sh"
SRC_ROLLBACK="$REPO/rollback-linux.sh"
for f in "$SRC_DEPLOY" "$SRC_ROLLBACK"; do
  [ -r "$f" ] || { echo "FATAL: cannot read $f - run this from a checkout"; exit 1; }
done

R="${1:-/tmp/pod-rehearse-slots}"
case "$R" in
  /var/*|/opt/*|/etc/*) echo "FATAL: refusing to use $R as a scratch root"; exit 1 ;;
esac

rm -rf "$R"; mkdir -p "$R"/{backups,bin,data,src/target/release}
mkdir -p "$R"/src/{templates,wiki,public_adventure_overlay}
mkdir -p "$R"/data/adventure-fights-summary
echo "fight1" > "$R"/data/adventure-fights-summary/fight-0000000001.json

mkstub() {  # $1 = real script, $2 = stubbed copy
  {
    head -1 "$1"
    cat <<'STUB'
systemctl() { :; }
chown() { :; }
curl() { echo 200; }
STUB
    tail -n +2 "$1"
  } > "$2"
  sed -i -e "s#/var/backups/pathofdust#$R/backups#g" \
         -e "s#/opt/pathofdust/bin#$R/bin#g" \
         -e "s#/var/lib/pathofdust#$R/data#g" "$2"
  chmod +x "$2"
  # Fail loudly rather than silently operating on production.
  if grep -qE '/var/backups/pathofdust|/opt/pathofdust/bin|/var/lib/pathofdust' "$2"; then
    echo "FATAL: $2 still references a production path after rewriting - refusing to run"
    grep -nE '/var/backups/pathofdust|/opt/pathofdust/bin|/var/lib/pathofdust' "$2"
    exit 1
  fi
}
mkstub "$SRC_DEPLOY"   "$R/deploy.sh"
mkstub "$SRC_ROLLBACK" "$R/rollback.sh"

newbin() { printf 'BINARY-%s\n' "$1" > "$R/src/target/release/game"; chmod +x "$R/src/target/release/game"; }

pass=0; fail=0
ok()  { echo "  PASS  $1"; pass=$((pass+1)); }
bad() { echo "  FAIL  $1"; fail=$((fail+1)); }

echo "=============================================================="
echo "T1  first deploy of release 'alpha'"
echo "=============================================================="
printf 'BINARY-original\n' > "$R/bin/game"; chmod +x "$R/bin/game"
newbin a1
"$R/deploy.sh" "$R/src" alpha deadbeefcafe1234 2>&1 | sed 's/^/    /'
S1=$(ls -d "$R"/backups/deploy-pre-*-alpha 2>/dev/null | head -1)
[ -n "$S1" ] && ok "slot created: $(basename "$S1")" || bad "no slot created"
basename "$S1" | grep -qE '^deploy-pre-[0-9]{8}-[0-9]{6}-alpha$' && ok "name shape is deploy-pre-<YYYYMMDD-HHMMSS>-<release>" || bad "name shape wrong"
[ -L "$R/backups/deploy-pre-LATEST" ] && ok "LATEST symlink exists -> $(readlink "$R/backups/deploy-pre-LATEST")" || bad "no LATEST symlink"
[ "$(readlink "$R/backups/deploy-pre-LATEST")" = "$(basename "$S1")" ] && ok "LATEST points at the new slot" || bad "LATEST wrong target"
grep -q '^# commit:  deadbeefcafe1234' "$S1/SHA256SUMS" && ok "commit SHA recorded inside SHA256SUMS" || bad "commit not recorded"
S1HASH=$(sha256sum "$S1"/game.pre-alpha | cut -d' ' -f1)

echo
echo "=============================================================="
echo "T2  SECOND deploy of the SAME release name 'alpha'  <-- the fix"
echo "=============================================================="
sleep 1
newbin a2
"$R/deploy.sh" "$R/src" alpha feedface00009999 2>&1 | sed 's/^/    /'
COUNT=$(ls -d "$R"/backups/deploy-pre-*-alpha | wc -l)
[ "$COUNT" -eq 2 ] && ok "two distinct slots for the same release name (was: one, overwritten)" || bad "expected 2 slots, got $COUNT"
S1HASH_NOW=$(sha256sum "$S1"/game.pre-alpha | cut -d' ' -f1)
[ "$S1HASH" = "$S1HASH_NOW" ] && ok "FIRST slot's binary is UNCHANGED - the recovery path survived" || bad "first slot's binary was overwritten"
S2=$(ls -d "$R"/backups/deploy-pre-*-alpha | tail -1)
[ "$(readlink "$R/backups/deploy-pre-LATEST")" = "$(basename "$S2")" ] && ok "LATEST moved to the newer slot" || bad "LATEST did not move"
grep -q '^# commit:  feedface00009999' "$S2/SHA256SUMS" && ok "second slot records its own commit" || bad "second slot commit wrong"

echo
echo "=============================================================="
echo "T3  forced exact collision (slot name already exists)  <-- the guard"
echo "=============================================================="
STAMP=$(date +%Y%m%d-%H%M%S)
mkdir -p "$R/backups/deploy-pre-$STAMP-guardtest"
echo "PRECIOUS-DO-NOT-LOSE" > "$R/backups/deploy-pre-$STAMP-guardtest/game.pre-guardtest"
BEFORE=$(cat "$R/backups/deploy-pre-$STAMP-guardtest/game.pre-guardtest")
newbin g1
OUT=$("$R/deploy.sh" "$R/src" guardtest 2>&1); RC=$?
echo "$OUT" | sed 's/^/    /'
[ $RC -ne 0 ] && ok "refused with non-zero exit ($RC)" || bad "did NOT refuse (exit $RC)"
echo "$OUT" | grep -q "refusing to overwrite a recovery path" && ok "message names the reason" || bad "no refusal message"
AFTER=$(cat "$R/backups/deploy-pre-$STAMP-guardtest/game.pre-guardtest")
[ "$BEFORE" = "$AFTER" ] && ok "pre-existing slot content untouched" || bad "pre-existing slot was modified"

echo
echo "=============================================================="
echo "T4  LEGACY slot resolution (pre-2026-09-03 deploy-pre-<name>)"
echo "=============================================================="
LEG="$R/backups/deploy-pre-legacyrel"
mkdir -p "$LEG"
printf 'BINARY-legacy\n' > "$LEG/game.pre-legacyrel"; chmod +x "$LEG/game.pre-legacyrel"
sha256sum "$LEG/game.pre-legacyrel" > "$LEG/SHA256SUMS"   # legacy: NO comment lines
OUT=$("$R/rollback.sh" legacyrel 2>&1); RC=$?
echo "$OUT" | sed 's/^/    /'
[ $RC -eq 0 ] && ok "legacy slot resolved and rolled back (exit 0)" || bad "legacy rollback failed (exit $RC)"
echo "$OUT" | grep -q "deploy-pre-legacyrel" && ok "chose the legacy slot by release name" || bad "did not name the legacy slot"
grep -q 'BINARY-legacy' "$R/bin/game" && ok "legacy binary is now live in scratch bin" || bad "legacy binary not installed"

echo
echo "=============================================================="
echo "T5  --list ordering and LATEST line"
echo "=============================================================="
"$R/rollback.sh" --list 2>&1 | sed 's/^/    /'
LIST=$("$R/rollback.sh" --list 2>&1)
echo "$LIST" | grep -q "LATEST ->" && ok "--list reports LATEST" || bad "--list missing LATEST"
echo "$LIST" | grep -q "legacy" && ok "--list flags legacy slots as approximate ordering" || bad "legacy not flagged"
# The PROPERTY, not an expected winner - see the header note about
# assertions that encode an answer.
WHENS=$(echo "$LIST" | grep -E '^deploy-pre-' | awk '{print $2" "$3}')
if [ "$WHENS" = "$(printf '%s\n' "$WHENS" | sort -r)" ]; then
  ok "rows are in descending time order (newest first)"
else
  bad "ordering not descending:"; printf '        %s\n' "$WHENS"
fi

echo
echo "=============================================================="
echo "T6  no-argument rollback follows LATEST"
echo "=============================================================="
OUT=$("$R/rollback.sh" 2>&1); RC=$?
echo "$OUT" | sed 's/^/    /'
[ $RC -eq 0 ] && ok "no-arg rollback succeeded" || bad "no-arg rollback failed"
echo "$OUT" | grep -q "(LATEST)" && ok "says it chose LATEST and why" || bad "did not explain the choice"
grep -q 'BINARY-a1' "$R/bin/game" && ok "restored the binary the LATEST slot saved (a1, a2's predecessor)" || bad "wrong binary restored: $(head -1 "$R/bin/game")"

echo
echo "=============================================================="
echo "T7  template-only slot (no binary) is refused clearly"
echo "=============================================================="
mkdir -p "$R/backups/deploy-pre-20260101-000000-tmplonly/templates"
OUT=$("$R/rollback.sh" tmplonly 2>&1); RC=$?
echo "$OUT" | sed 's/^/    /'
[ $RC -ne 0 ] && ok "refused (exit $RC)" || bad "did not refuse a binary-less slot"
echo "$OUT" | grep -q "template-only slot" && ok "explains it is a 13B.8 template slot" || bad "unclear message"

echo
echo "=============================================================="
echo "T8  a release name matching MULTIPLE slots picks the newest, and says so"
echo "=============================================================="
OUT=$("$R/rollback.sh" alpha 2>&1); RC=$?
echo "$OUT" | sed 's/^/    /'
echo "$OUT" | grep -q "2 slots match" && ok "warns that multiple slots matched" || bad "no multi-match warning"
echo "$OUT" | grep -q "$(basename "$S2")" && ok "chose the newest of them" || bad "did not choose newest"

echo
echo "=============================================================="
echo "RESULT: $pass passed, $fail failed   (scratch root: $R)"
echo "=============================================================="
[ $fail -eq 0 ]
