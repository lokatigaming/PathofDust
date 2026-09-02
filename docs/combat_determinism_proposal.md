# Deterministic unit ordering in `simulate_battle` — PROPOSAL

> **RECOVERY NOTE — added 2026-09-02, not part of the original document.**
>
> Written 2026-08-20 by Lokati on the branch
> `docs/combat-determinism-proposal` (commit `99efbfc`), which was never
> merged. Recovered to master 2026-09-02, **verbatim** — nothing below
> this note was altered.
>
> **STATUS: THIS PROPOSAL WAS ACCEPTED AND SHIPPED.** The document is a
> record, not an open question. It was implemented the same day it was
> written, by `c029a55` (2026-08-20, *"Make party ordering in
> simulate_battle deterministic and fair"*), which is on master. Only the
> proposal document was stranded; the work itself landed.
>
> Verified against master `a2d75fa`, both iteration sites this document
> identifies are now ordered:
>
> - **Main unit list** — `combat.rs:12121` sorts `units` by id and then
>   shuffles from a `StdRng` seeded off a new `fight_seed`, deliberately
>   separate from the combat `rng` so the eleven committed solo fixtures
>   do not move. The sort-then-shuffle order is load-bearing and is
>   explained in the code comment above it.
> - **Golem pass** — `combat.rs:12145` collects `summoner_ids`, sorts
>   them, and iterates in that order instead of `characters.iter()`.
>
> The header below still reads *"Status: proposal only. No
> implementation. Awaiting owner approval."* That line was true when
> written on 2026-08-20 and is **stale as of 2026-09-02**. It is left
> unedited, per the append-only rule; this note is the correction.
>
> Line numbers cited in the body (`combat.rs:10432`, `combat.rs:11562`)
> are as of master `5b80a49` and do not resolve against current master.

**Status: proposal only. No implementation. Awaiting owner approval.**

Written 2026-08-20 against master `5b80a49`, by the passive-tree session,
at the owner's request after the passive-corpus work surfaced the
coverage gap this would close.

---

## 1. The mechanism, and the ordering key

### What is non-deterministic today

`simulate_battle` takes `characters: &HashMap<String, Character>` and
iterates it **twice**, in HashMap order:

| Site | Line | What it builds |
|---|---|---|
| Main unit list | `combat.rs:10432` | `let mut units: Vec<CombatSimUnit> = characters.iter().map(...)` |
| Golem pass | `combat.rs:11562` | `for (id, c) in characters.iter()` → `golems_to_add`, later `units.extend(golems_to_add)` |

Nothing sorts `units` afterwards. Rust's `HashMap` seeds its hasher from
a per-process random source, so **the same fight, with the same seeded
`rng`, produces a different `units` order in every process**. Index 0 is
a different player each run.

That matters because index order feeds:

- "random alive player" target selection (a seeded roll picks an
  *index*, so which player it lands on depends on the ordering),
- first-mover and same-tick tie-breaks,
- the order golems are appended in, and therefore which golem a
  redistribution or targeting pass reaches first.

`golden_corpus.rs`'s own module doc already identifies this precisely,
and its answer was to make **every scenario solo** — one character means
one possible order. That is why the corpus cannot cover party mechanics
today.

### The ordering key — and why this one

Any fixed order is as arbitrary as a random one; the question is which
arbitrary order has the best properties. Candidates:

| Key | Deterministic | Stable across saves/restarts | Notes |
|---|---|---|---|
| **Character id** (lowercased login) | Yes | Yes — ids never change | Simple, no new state. **Recommended.** |
| Join order | Yes | Only if persisted | Needs a new `Character` field and a backfill for every existing save. More machinery for no extra benefit. |
| Level, then id | Yes | No — changes as players level | Ordering would silently shift under players; worse for reproducing a reported fight. |
| Seeded shuffle from the fight `rng` | Yes | N/A | Fair *and* deterministic, but consumes RNG draws — see the warning below. |

**Recommendation: sort by character id.** It needs no new persisted
state, no migration, and it is stable across saves, restarts and
sessions — which is the property that actually matters for reproducing a
reported fight weeks later. Two lines:

```rust
// after the existing units construction at combat.rs:10432
units.sort_by(|a, b| a.id.cmp(&b.id));
// and the golem pass at :11562 iterates a sorted view of `characters`
```

### The fairness caveat, stated plainly

Sorting by id gives alphabetically-early logins a **permanently fixed
position** in the order. If any mechanic advantages index 0 — and
first-mover tie-breaks plausibly do — that becomes a small, systematic,
name-correlated bias that persists across every fight forever. Today's
randomness spreads that bias evenly by accident.

Two honest options:

1. **Accept it.** Quantify first: if no mechanic actually rewards a low
   index, the concern is theoretical. This is measurable from the
   existing fight logs before committing to anything.
2. **Layer a deterministic per-fight permutation on top.** Sort by id
   for a stable base, then rotate or shuffle using a value derived from
   the fight's own inputs (stage, boss kind, fight sequence number) — 
   *not* from the shared `rng`.

**Do not shuffle using the fight's own `rng`.** It would be deterministic
and fair, but every draw it consumes shifts every subsequent roll in the
fight, which moves **all eleven** existing solo fixtures. That converts a
zero-fixture-churn change into a full regeneration, and regenerating the
baseline is exactly what these fixtures exist to prevent.

My recommendation is to ship the id-sort alone, and treat the
permutation as a separate follow-up **only if** measurement shows index
position confers a real advantage.

---

## 2. Blast radius

### What observably changes in live fights

Only for fights with **two or more participants**:

- The sequence in which units are constructed and therefore indexed.
- Which player a "random alive player" roll resolves to for a given RNG
  draw.
- Same-tick resolution order between players.
- The order golems are appended, and so which golem a targeting or
  redistribution pass reaches first.

### Does any player-facing outcome distribution shift?

**No — the distribution is unchanged; only the per-fight assignment is.**
Targeting still draws from the same seeded RNG against the same number
of candidates, so each player's probability of being targeted is what it
was. What changes is *which specific player* a given draw maps onto.

The one genuine distributional caveat is the fairness point in §1: today
the mapping is re-randomized every process, so any index-position
advantage is spread evenly across players over time. After the change it
is fixed per player. That is a redistribution of an existing (possibly
zero) effect, not a new one.

### What needs regenerating

**Expected: nothing. No fixture moves.**

The proof is structural rather than empirical, and it is exact:

- Every one of the 12 committed corpus fixtures is a **solo** scenario —
  `run_scenario` inserts exactly one `Character` into the map.
- With one element, `characters.iter()` has exactly one possible order,
  and `units.sort_by(id)` on a one-element `Vec` is a no-op.
- The golem pass likewise iterates a single entry.
- No RNG draw is added or removed, so every subsequent roll in every
  fixture is untouched.

So every fixture must reproduce byte-for-byte. **This is cheap to verify
empirically and must be**: implement, run `cargo test golden_corpus`,
and confirm zero divergences and zero rewritten files. If any fixture
moves, the change did something unintended and should not proceed.

Tests affected:

- `every_golem_death_gets_handle_golem_death_even_on_the_fights_final_tick`
  — currently flaky at **~29% on unmodified master** (measured: 4
  failures in 14 runs). It uses a 3-character party, and this change is
  the fix. It should become deterministic, not merely less flaky.
- No other test constructs a multi-character `simulate_battle` call that
  I found.

---

## 3. What it unlocks

1. **Party corpus scenarios**, which are impossible today. That directly
   closes the documented gap at `Scenario::passives`: the ally-targeted
   passive set — `covenant`, `compassion`, `chainoflight`,
   `sharedstrength`, `unitedpack` and others — currently has **no
   snapshot coverage at all**, because a solo character has no ally for
   those mechanics to act on. Their allocation pins a stat contribution;
   their actual behavior is unprotected.
2. **Golem and redistribution coverage in a party**, which is where the
   mechanic actually behaves differently — redistribution splits across
   *surviving party members*, a branch a solo fixture can never reach.
3. **The permanent death of a whole flaky-test class.** The golem test
   above is not uniquely unlucky; it is the first test to combine a
   multi-character party with an exact assertion. Any future test doing
   the same inherits the same ~29% failure rate. This removes the cause
   rather than widening a tolerance — note that the last such test was
   "fixed" by widening tolerance (`5132ffa`), which treats the symptom.
4. **Reproducible bug reports.** A player-reported multi-character fight
   could be replayed from its seed and actually reproduce, which is not
   true today.

---

## 4. Where it should be implemented, and by whom

This is **production combat code** — `combat.rs`, the single
highest-risk file in the project, and the one every release note warns
about. It is not passive-tree territory and I would not merge it
unilaterally.

**What I would hand to the release queue** (the whole change — it is
small):

- The two-line sort at `combat.rs:10432` and the sorted iteration at
  `:11562`.
- A doc comment at both sites explaining *why* the order is fixed, so a
  future refactor does not "simplify" it back into a raw `HashMap`
  iteration. This is the part most likely to be lost otherwise.
- Deleting the solo-only constraint note in `golden_corpus.rs`'s module
  doc, replaced with the new invariant.
- Verification: full suite, plus `cargo test golden_corpus` proving zero
  fixture divergence, plus the previously-flaky golem test run 20+ times
  to demonstrate it is now deterministic rather than merely luckier.

**What I would do myself, after it lands:** the party corpus scenarios
that depend on it — a Cleric party fixture covering `chainoflight` and
`compassion`, a Warlock party fixture covering `covenant`, and a
multi-player Elementalist fixture covering redistribution across
survivors. Those are corpus additions in my own area and follow the same
addition-only discipline as the existing ones.

**Sequencing:** this should land **before** the remaining Stage 3
migration batches touch any ally-targeted node. `covenant` is in the
Warlock batch and `compassion`/`chainoflight` are in the Cleric batch;
migrating them without party coverage means migrating exactly the
mechanics nothing can verify.

---

## Open question for the owner

Is the alphabetical-position fairness concern (§1) worth measuring
before implementing, or is it acceptable as-is? That is the only
decision in this proposal that changes what gets built; everything else
follows from it.
