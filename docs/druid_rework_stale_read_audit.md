# #39 — 2026-08-16 Druid rework: stale-read audit

The 2026-08-16 Druid rework repurposed seven nodes. It updated the node
definitions but not every `combat.rs` read of them, so several reads kept
computing the mechanic each node USED to have, using the magnitude of the
mechanic it now has. Where the *unit* changed — milliseconds or a count
now being read as a `0..1` rate — the result was live-distorting.

This is the audit of every read of every node that rework touched, with a
verdict per read. Same discipline as the migration batches, applied to
the blast radius of my own rework.

## The table

Node keys are unchanged by the rework; only their meanings moved.

| Node key | Old mechanic | New mechanic (magnitude unit) | `combat.rs` read | Verdict |
|---|---|---|---|---|
| `symbiosis` | +DR to lowest-HP ally | **Werebear** — Thick Hide cleanse cycle (**milliseconds**, 6000/5000/4000) | `own_symbiosis_dr_pct` (:10960) | **stale-removed** |
| | | | `thickhide_cycle_ms` (:10976) | still-correct |
| | | | `thickhide_target_count` gate (:10982) | still-correct |
| `livingbond` | fed Symbiosis's DR | **Wild Roar** — party-death fear procs (**count**, 1/2/3) | `own_symbiosis_dr_pct` (:10960) | **stale-removed** |
| | | | `wildroar_charges` (:10974) | still-correct |
| `rootednetwork` | extra allies Symbiosis protected | **Rooted Network** — extra Thick Hide *cleanse* targets (**count**) | `sharedstrength_extra_targets` (:10963) | **stale-fixed** |
| | | | `thickhide_target_count` (:10983) | still-correct |
| `naturesembrace` | (Temple-Guardian-style heal share) | **Nature's Embrace** — on-death full heals (**count**, 1/2/3) | `templeguardian_heal_pct` (:10966) | **stale-fixed** |
| | | | `naturesembrace_heal_targets` (:10975) | still-correct |
| `verdantburst` | heal-crit splash share | **Verdant Burst** — death ward (**charge count**, 1/2/3) | `heal_crit_splash_pct` (:11426) | **stale-fixed** |
| | | | `verdantburst_charges` (:11387) | still-correct |
| `unyieldingroots` | doubled Living Armor below an HP threshold | **Unyielding Roots** — taunt cycle (**milliseconds**, 8000/6000/4000) | `unyieldingroots_cycle_ms` (:11282) | still-correct |
| `wildsurge` | — (reworked in place) | Wild Surge — interval reduction (rate) | none in `combat.rs`; read in `character.rs` | still-correct |
| `overgrowth` | — (reworked in place) | Overgrowth — amplifies Wild Surge (rate) | none in `combat.rs`; read in `character.rs` | still-correct |

**Also checked and CLEARED:** `own_pack_instinct_evasion_pct` (:10959),
the sibling in the same expression pair at :8187. It reads `packinstinct`,
whose description still reads "Predator's Instinct also grants your
lowest-HP ally +4% evasion per rank" — that mechanic survived the rework
intact, and the magnitude is still a rate. **Still-correct, no change.**

## Severity

**One live-distorting, two more that were also live-distorting, one
semantic.**

`own_symbiosis_dr_pct` summed Werebear's cycle with Wild Roar's charges:
at rank 3 that is `4000.0 + 3.0 = 4003.0`, pushed into the
reduction-source pipeline as a fraction. Clamped to 1.0 per source, it
pinned whichever party member was currently lowest-HP — players and
golems alike — at the 95% mitigation cap, whenever a character with
those nodes was in the party.

The sweep found two more of the same class that had not been reported:

- `templeguardian_heal_pct` — real contributors give **0.02–0.06**;
  `naturesembrace` was adding **1.0–3.0**. Roughly a **50×** overstatement.
- `heal_crit_splash_pct` — real contributor gives **0.20–0.60**;
  `verdantburst` was adding **1.0–3.0**. Roughly a **6×** overstatement.

And one semantic-only: `rootednetwork` was still extending the
lowest-HP-ally *protect* count, which the rework's own note says it no
longer does ("extends Thick Hide's cleanse to more targets **instead of**
Symbiosis's old protect-count"). Same magnitude scale, wrong mechanic.

## What changed

1. `own_symbiosis_dr_pct` deleted outright — field, all initialisers, the
   `:8187` fold, the `symbiosis_dr_bonus` parameter and argument, and the
   reduction-source push. **Nothing replaces it**; the tree has no
   "+DR to lowest-HP ally" node any more.
2. The `"Symbiosis"` roll-log literal removed with it. That label named a
   deleted mechanic and is the reason the source could not be identified
   from a fight log at all.
3. `rootednetwork`, `naturesembrace` and `verdantburst` terms removed
   from the three sums above, each with a comment stating what the rework
   moved them to.

The `:8187` gate now yields evasion only; Pack Instinct's evasion and
Temple Guardian's heal still work exactly as before.

## Tests

- `the_reworked_druid_nodes_still_have_the_units_their_readers_expect` —
  asserts on the *scale* of each reworked node's magnitude (millisecond
  cycles stay ≥1000, counts stay whole and ≥1), so a future rework that
  changes one of these units again trips a test rather than shipping.
- `the_stale_druid_rework_reads_stay_removed` — asserts the dead field,
  its plumbing, the `"Symbiosis"` label and each stale magnitude read
  stay absent.
- Golden corpus **expected green and is** — no committed fixture
  allocates any of these nodes, so none moved. That is also precisely why
  the corpus never caught this.

## Patch-notes proposal (player-facing)

> **Fixed:** a removed Druid mechanic was still granting maximum damage
> reduction to the lowest-health party member. Symbiosis was reworked
> into Werebear on 16 August, but the old effect kept running off the new
> node's timer value — pinning whoever was lowest-health in the party at
> the damage-reduction cap. Parties containing a Druid with these nodes
> have been taking far less damage than intended since then. This is a
> nerf, and you will feel it.
>
> **Also fixed** in the same pass: Nature's Embrace and Verdant Burst
> were inflating Temple Guardian's heal and heal-crit splash by roughly
> 50× and 6× respectively, and Rooted Network was still extending the
> wrong protection count.

Honest about it being a nerf, honest that it ran for four days, and it
names the date so anyone comparing old fight logs understands why their
numbers moved.
