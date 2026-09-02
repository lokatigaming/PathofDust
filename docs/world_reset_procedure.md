# World reset procedure

Path of Dust runs in seasons: a world ends, everything in it is wiped, and
a new world starts from fresh characters (owner ruling 2026-08-30, see
[`world2_build_plan.md`](world2_build_plan.md) §1). This document holds
the steps of a reset that have **failed in practice** and therefore need
to be verified rather than merely instructed.

It is deliberately not a full runbook. Steps live here once they have gone
wrong once; everything else is still in `world2_build_plan.md`.

---

## Step: `OPERATOR_BOOTSTRAP` must be unset, and the unset VERIFIED

**Added 2026-09-02 after this step silently did not happen during the
World 2 reset.**

### What it is

`OPERATOR_BOOTSTRAP` is the opt-in that lets the operator register their
own account on a fresh world. `accounts.rs`'s `username_rejection`
permanently reserves `OPERATOR_LOGIN` (and `lokati_gaming`
unconditionally), which on a brand-new deployment means the operator
cannot create the very account the three operator gates point at.
`OPERATOR_BOOTSTRAP` permits exactly one name — its own value, which must
equal the current `OPERATOR_LOGIN` — and nothing else.

`accounts.rs` states the procedure as: **"Set it, register, remove it,
restart."** It also states, in its own words, why the window matters:

> Deliberately NOT "allow it while the account store is empty": on a
> public launch that leaves a window in which any player could claim the
> operator name first, which is the exact grief vector the reservation
> exists to prevent. **This window is never open unattended.**

### What actually happened

At the World 2 reset (2026-09-02) the variable was set at 12:56 in
`/etc/systemd/system/pathofdust.service.d/20-bootstrap.conf`, the operator
registered, and **the removal never happened.** It was still set on live
production hours later, found incidentally during the Twitch-removal
deploy. The instruction existed and was not followed, which is why this is
now a step with a check rather than a trailing sentence.

It was not exploitable when found — the `lokati` account already existed,
so `do_register`'s collision check refused a second claim — but that is a
second line of defence doing the job of the first. One account-store loss
and the operator name is claimable by anyone.

### The step

Remove the drop-in entirely rather than blanking the value; an empty
`Environment=` line is another thing that reads as meaningful later.

```sh
rm -f /etc/systemd/system/pathofdust.service.d/20-bootstrap.conf
systemctl daemon-reload
systemctl restart pathofdust
```

### The check — both halves are required

**1. Configuration.** The variable must not appear in the resolved unit
environment:

```sh
systemctl show pathofdust -p Environment | grep -c OPERATOR_BOOTSTRAP   # expect: 0
```

**2. By effect.** Configuration checks pass on a box where the drop-in was
never read. Prove the reservation is actually back by attempting the
registration the bootstrap existed to permit — it must be refused as
taken/reserved:

```sh
curl -s -X POST http://127.0.0.1:4005/account/register \
  --data-urlencode "username=$OPERATOR_LOGIN" \
  --data-urlencode 'password=irrelevant-this-must-fail' \
  | grep -o 'already taken'          # expect: already taken
```

A 303/redirect, or a body without that phrase, means the reservation is
**not** in force and the world is standing open. Stop and fix it before
announcing the reset.

**3. And confirm the operator did not lock themselves out** — the failure
this bootstrap exists to prevent is the opposite one:

```sh
# with the operator's own adv_session cookie
curl -s -o /dev/null -w '%{http_code}\n' -b "adv_session=<operator token>" \
  http://127.0.0.1:4005/admin/tunables    # expect: 200
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:4005/admin/tunables
                                          # expect: 404 anonymous
```

### Why a check and not an instruction

The reset procedure already told someone to unset it. That was not
enough. A step whose completion is not verified is a step that eventually
does not happen, and this one fails **silently and in the safe-looking
direction** — the game runs perfectly with the hole open, so nothing draws
attention to it. The registration attempt above is the only thing that
distinguishes "removed" from "still set."

---

## Step: enumerate everything RATIFIED BUT UNBUILT, and rule on each one, before the world opens

**Added 2026-09-02 after its absence shipped World 2 with the wrong item
scaling.**

### What it is

Work on this project is ratified in documents, then built later. The gap
between those two moments is where a decision can be lost, and nothing in
the codebase notices: unbuilt work leaves no failing test, no compiler
error and no log line. A world opens on whatever happens to be in the
binary that day, not on whatever has been agreed.

So before a world opens, every ratified-but-unbuilt item must be found
and **ruled on** — built now, deliberately deferred, or dropped. The
point is not to build everything. It is that nothing reaches a live world
by nobody having looked.

### What actually happened

`docs/affix_curve_spec.md` is a 3,008-line owner-ratified specification
of the affix tier curve, four new gear slots (Ring1, Ring2, Amulet,
Pants), and the crit-multiplier halving from `0.05` to `0.025` per tier.
It was written 2026-08-23 on the branch `docs/affix-curve-spec`, whose
final commit message was **"branch CLOSED"**. It was never merged.

For ten days master carried a four-word stub — *"Affix tier curve, four
new gear slots, crit multiplier halving, passive rebalance"* — pointing
at a document that was not there. A session searched master for the work,
found nothing, and recorded in `world2_build_plan.md` that it did not
exist. That wrong ruling stood for five days.

**World 2 opened on 2026-09-02 with none of it built.** The affix curve
is not in `affix_base_value`, `EQUIP_SLOTS` is still `[EquipSlot; 5]`,
and `affix.rs` still carries `default_per_tier: 0.05` — so item power
scales on the pre-ratification curve the spec exists to replace, and the
world is over-tuned by exactly the amount the halving and the curve were
ratified to remove. It surfaced only because the owner remembered a stray
line about base items and went looking. Nothing in the reset caught it,
because nothing in the reset was looking.

The spec was recovered to master on 2026-09-02 and the work is still
unbuilt. That is now a **recorded decision** rather than an oversight —
which is the entire difference this step is asking for.

### The step

Run all three sweeps. They overlap deliberately; each one catches
something the others miss.

**1. Every branch that is not an ancestor of `origin/master`.** This is
the sweep that would have caught the affix spec. A closed, abandoned or
merely forgotten branch is still a place ratified work can be sitting.

```sh
git fetch --all --prune
git for-each-ref --format='%(refname:short)' refs/remotes/origin \
  | grep -v 'origin/HEAD\|origin/master' \
  | while read b; do
      git merge-base --is-ancestor "$b" origin/master || echo "$b"
    done
```

For each branch listed, `git log --stat origin/master..<branch>` and read
what it holds. **A branch's own commit message saying it is closed,
abandoned, superseded or done is not evidence that its contents reached
master** — that is precisely the failure above. Only `git merge-base`
answers that question.

**2. Every document on master that ratifies something.** A spec can be
merged and still unbuilt; being on master proves nothing about the
binary.

```sh
grep -rniE 'owner-ratified|ratified|owner ruling|settled [0-9]{4}-|RULING' \
  docs/ *.md | cut -d: -f1 | sort -u
```

For each hit, check the claim **against code, not against another
document**. `world2_build_plan.md`'s recovered item-rebalance table is the
model: every row says where the thing is specified and what the tree
actually contains, with a file and line number, verified on the day.

**3. The plan document's own deferred list** — `world2_build_plan.md`
§5, "What is not in this plan", plus §7's open rulings. Anything parked
there is a candidate by definition, and parking something is not the same
as deciding it may ship absent from a fresh world.

### The check — the output is a written list, and it exists before the reset runs

The deliverable is a table, appended to `docs/session_journal.md` in the
reset's own entry, **committed before the reset is executed**, with one
row per item found and **an explicit decision against every row**:

| Item | Where ratified | Status in code (file:line, verified today) | DECISION | Ruled by | Date |
|---|---|---|---|---|---|

`DECISION` is one of exactly three words, and no row may be blank:

- **BUILD** — ships before the world opens. The reset waits.
- **DEFER** — the world opens without it, knowingly. The row must say
  what the world will be like without it, in the terms players
  experience. "Item power stays on the old curve, so the world opens
  over-tuned" is a decision; "not blocking" is not.
- **DROP** — no longer wanted. Say so in the source document too, or the
  next sweep finds it again and re-litigates it.

Two rules on who decides. **DEFER and DROP are owner rulings**, not a
session's call — a session may recommend and must not decide. And **an
empty table is itself a finding that must be defended**: three sweeps
returning nothing means either the project genuinely has no outstanding
ratified work, or a sweep was run wrong. Say which, and show the command
output.

### Why a check and not an instruction

Nobody had to be told the affix curve mattered; the owner had ratified
3,008 lines of it. What was missing was a moment at which someone was
obliged to go and look, and a place the answer had to be written down.

Unbuilt work fails in the safe-looking direction, exactly like
`OPERATOR_BOOTSTRAP` above: the game runs, the tests pass, the deploy is
green, and the only symptom is that the numbers are wrong in a way you
have to already suspect in order to notice. A reset is the one moment
when that becomes permanent for a whole season, because the world it
opens is the world players spend the season in.

The three commands above take a few minutes. World 2 is being played on
the wrong item scaling because nobody spent them.
