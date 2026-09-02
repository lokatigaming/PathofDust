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
