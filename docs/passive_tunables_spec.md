# Live-tunable passive values — spec

**Source of truth for this feature.** Read this in full before touching
the override store, the hook, or `/admin/passives`. If an implementation
needs to deviate, document why in the commit message and add a numbered
entry to the Decisions log below.

Branch: `feature/live-tunables` off `master` at `45ca8a4` (the commit
containing the Memories merge). Companion execution log:
`LIVE_TUNABLES_PROGRESS.md`.

---

## Goal and scope guard

Change the **numeric values** of any passive node — any class, any rank
— from an admin page, applied live with no rebuild and no restart. The
`/admin/tunables` pattern, generalized to the tree.

**VALUES ONLY.** Node structure — keys, max ranks, parents, unlock
gating, which nodes exist at all — stays code-defined in
`passive_tree.rs` and is deliberately unreachable from the override
store. **No character-data changes of any kind.**

## The audit that sized this

Of 471 nodes: **351 are tunable the moment the hook lands** (47
`FlatStat` + 13 `OverflowConversion` pooled generically, 265 `Special`
reading `passive_node_magnitude`, 26 reading magnitude for the value
with `rank > 0` only as a gate). **60 need migration** — they read
`passive_node_rank` and hardcode their numbers in `combat.rs`. 60
`NotYetImplemented` nodes have no values to tune.

The 60, by shape: 36 declare `1.0 / 1.0` so magnitude equals rank
exactly (trivial swap); 7 declare `0.0 / 0.0` with the real numbers only
in `combat.rs`; 15 have real declared values the code ignores; 2 are
odd (`guardianspirit`, `secondwind`).

Per-class counts (total / trivial / hardcoded / care): Berserker
10/1/3/6 · Rogue 8/5/1/2 · Mage 7/5/0/2 · Monk 7/6/1/0 · Slayer 6/4/0/2
· Ranger 6/5/1/0 · Warrior 5/3/0/1(+1 odd) · Warlock 4/3/0/1 · Cleric
4/2/1/0(+1 odd) · Paladin 2/1/0/1 · Druid 1/1/0/0 · Elementalist 0.

## Design

**Store** — a sparse `node_key → per-rank values` map, TOML-persisted to
`adventure-passive-overrides.toml`, held in a `std::sync::RwLock` so
edits apply with no restart. Absent key, or absent rank within a key,
falls through to the compiled-in value.

**Hook** — `PassiveNode::magnitude_at_rank`. That one method is the
tree's entire numeric read path, so an override entering there reaches
stat pooling, every `Special` mechanic in `combat.rs`, and the dashboard
stat display without any of them knowing it exists.

**Display** — a node's `description` is a hardcoded prose string and
cannot reflect an override. An overridden node therefore gets a
generated `Tuned: X (default Y)` line beside the untouched prose. See
Decision 5.

---

## Decisions log (newest last)

1. **Per-rank arrays, not overridden formula coefficients.**
   `PassiveEffect`'s magnitude formula is strictly linear, but 27 nodes
   are implemented as non-linear per-rank tables (typically "inert at
   rank 1, then two different values"). Coefficient overrides could
   never express those, locking them out of tuning permanently. A
   per-rank array expresses both shapes.
2. **Three values per node, always.** Every node has at most three
   *distinct* magnitudes: Skills and Modifiers cap at rank 3, and a
   Specialization's 4th point is unlock-only (`effective_rank` floors it
   at 3). Overrides are keyed by effective rank, so a 4th entry is never
   needed and rank 4 reads the rank 3 value.
3. **Rank 0 is never overridable.** An unallocated node is worth nothing
   by definition; letting an override change that would grant a player a
   passive they never invested in.
4. **A global, not a manager field.** The read path has no manager to
   reach — `magnitude_at_rank` is an inherent method on a `'static` node
   definition, and `Character::passive_node_magnitude` is a plain sync
   method called from the web layer as well as combat. Threading a store
   parameter through would mean changing every caller of every `combat_*`
   getter. `LazyLock<RwLock<_>>`: the sparse-override data shape of
   `adventure-item-balance.toml`, with the live-update semantics of
   `LiveTunables`.
5. **Display: the computed value line (Option 2).** Templating all 471
   description strings was considered and rejected as a far larger
   content migration in the wiki session's territory; accepting silent
   drift was rejected as putting the wiki's accuracy work permanently at
   odds with the admin page. The generated line leaves the templating
   door open later. **It ships with Stage 1, not after** — Stage 1 is
   what makes overrides settable, so shipping them apart would leave a
   window of exactly the silent divergence this exists to prevent.
6. **The wiki adopts the line by calling the same helper.**
   `passive_override_note` is free-standing and `pub(crate)` precisely
   so `adventure_web::wiki::render_wiki_archetype_graph` can call it.
   That file belongs to the parallel wiki session and is not edited from
   this branch; requested via `WIKI_IMPACT.md`.
7. **Untunable nodes are shown but not offered an input**, with the
   reason stated — an input that silently does nothing is worse than no
   input. Covers both the 60 pending-migration nodes and the
   `NotYetImplemented` ones.
8. **No per-node bounds** (owner ruling): a permissive numeric range
   plus a visible "differs from default" marker, on a single-admin
   surface. Non-finite input **is** rejected — NaN/inf would poison
   every downstream calculation rather than merely being an odd balance
   choice — as is a node key not in the class being edited.
9. **`INTEGER_COUNT_NODES` ships empty and is populated per batch.**
   Seeding it from the 36 nodes declaring `1.0 / 1.0` is wrong for 12 of
   them (`lastlaugh`, `compassion`, `quickdraw`, `markedfordeath`,
   `absolutezero`, `arcaneinstability`, `clarity`, `covenant`,
   `empoweredbolt`, `finalblow`, `ravage`, `surgicalstrike` are boolean
   thresholds, not counts of anything). Entries are added by the batch
   that migrates each node, once its actual code has been read. Nothing
   is lost: every candidate is pending, so none is tunable yet anyway.
10. **The page is scoped to one class at a time.** 471 nodes on one page
    would be unusable, and one giant form would make a single bad input
    lose every other edit. Each node is an independently-savable row.
11. **Migrate bucket C from what the CODE does, never the declared
    values** (owner ruling). For those nodes the declaration is unread
    and follows a different convention — `crush` declares `0.50/0.15`
    (linear .50/.65/.80) while the code does 0/.50/.65, and the prose
    description agrees with the *code* ("unlocked at rank 2"). Each
    migration corrects the declaration to the real per-rank table, so
    declarations become trustworthy going forward.
12. **A node's declared shape does not predict its call site.** Stage 2
    projected 36 mechanical swaps from the `1.0 / 1.0` declarations;
    dumping the actual call sites cut the batch to 20 and turned up two
    nodes with no consumer at all, one feeding a non-linear `match`
    table, and one genuine behavior change. **Read the call site before
    editing, every time.** This is the second occurrence of the same
    assumption failing — see Decision 9.
13. **`UNWIRED_NODES` is a third classification.** A node can declare
    real per-rank values that nothing in the codebase reads. Distinct
    from pending migration (values *do* reach the game, via hardcoded
    constants) and from `NotYetImplemented` (declares no value at all).
    `node_untunable_reason` tells `/admin/passives` which applies, so it
    never promises a migration batch that would have nothing to do.
14. **`chainoflight` migrated, nerf accepted** (owner decision,
    2026-08-20). A Specialization read as `(1 + rank).min(5)`, so 4/4
    yielded 5 targets while magnitude yields 4 (`effective_rank` floors
    a Spec at 3). Its description said "up to 4 at rank 3" and the tree
    documents a Spec 4th point as unlock-only, so the old behavior was a
    latent bug. Migrating makes the node tunable AND makes its own
    description accurate. This is the one deliberate behavior change in
    Stage 2; a 4/4 investment loses a bounce target.
15. **`PassiveEffect::SpecialPerRank` is the convention for any
    non-linear node** (owner-approved, 2026-08-20, Stage 3 Mage batch).
    `Special`'s `at_rank_1 + per_additional_rank * (rank - 1)` is
    strictly linear, but a large share of the tree is implemented in
    `combat.rs` as a `rank >= 2` / `rank >= 3` ladder with a different
    constant per branch — Absolute Zero's `0 / 0.50 / 0.65`, Arcane
    Instability's `0.05 / 0.09 / 0.12`, Empowered Bolt's `0 / 0 / 0.20`.
    None of those can be declared linearly, so before this variant their
    true defaults had nowhere to live and they could not be migrated at
    all without changing behavior. **18 of the 31 nodes pending at that
    point were this shape**, so it pays for itself many times over.

    The override *store* was always per-rank precisely so it could hold
    these shapes; this applies the same idea to the compiled-in default,
    closing the last gap between what can be tuned and what can be
    declared. Purely additive — introducing it changed no existing node.

    **Reach for this rather than inventing a parallel mechanism.** A
    node whose values are not linear declares a `SpecialPerRank` table;
    it does not get a bespoke ladder in `combat.rs`, and it does not get
    its declaration bent into an approximate linear fit. `values` is
    indexed by effective rank (index 0 is rank 1) and reads 0.0 outside
    its range, the same as an unallocated node.

    Note the division of labour this preserves: a *value* that varies by
    rank belongs in the table, but a **structural gate** — "unlocked at
    rank 2", "invested at all" — stays a `passive_node_rank` read in
    code, per the scope guard. Empowered Bolt keeps its `rank >= 2`
    invested flag for exactly this reason while its `0 / 0 / 0.20` crit
    bonus moves into the table.

---

## Staged plan

- **Stage 1 — store, hook, admin page, value line.** Zero behavior
  change; overrides-file-absent is byte-identical to before. Makes 351
  nodes tunable on its own. **Done.**
- **Stage 2 — bucket A**, the 36 nodes where magnitude equals rank by
  construction. Mechanical, provably identical at defaults.
- **Stage 3 — buckets B, C, D**, the 24 real ones, batched per class so
  each batch is small enough to review whole.
  **NOTE:** the golden corpus does NOT protect passive migrations — its
  scenarios never allocate any passives, so every node sits at rank 0 in
  every fixture. Corpus scenarios WITH allocations (a fixture ADDITION,
  not a regeneration) must land before the first value-changing batch.
  Includes **adding an Elementalist corpus scenario** — a fixture
  ADDITION, never a regeneration — to close the one archetype-coverage
  gap (11 of 12 are covered; Elementalist's golem code is unprotected).

**Ask before starting each migration batch** — the owner may reorder for
balancing appetite. Approved risk order: Druid, Paladin, Warlock → Monk,
Ranger, Mage → Rogue, Slayer, Warrior, Cleric → Berserker last.

## Verification

- `cargo build --release --workspace --target-dir target-tunables`
  (`--workspace` required; a separate target dir is mandatory —
  `target/release/` holds live, file-locked production binaries).
- `cargo test --workspace --all-targets --target-dir target-tunables`.
- `cargo clippy` clean on touched code.
- **No `cargo fmt`** — no `rustfmt.toml`; a blanket run rewrites
  unrelated code (`ELEMENTALIST_PROGRESS.md` Decision 6).
- **Golden-corpus fixtures are neither regenerated nor deleted.** Each
  migration batch must leave its class's fixture byte-identical; that is
  the behavior-neutrality proof.

---

## Owner doctrine (2026-08-24, BINDING)

> **EVERY numerical value in EVERY passive is tunable.** Magnitudes,
> caps, thresholds, counts, rates, durations. A hardcoded number in a
> passive is a defect, not a design. New passives must ship tunable.

This supersedes any earlier framing of hardcoded passive values as
deliberate. Decision 16's shared-constant exception remains the only
escape hatch, and it covers genuinely STRUCTURAL constants — not
class/skill numbers.

## Stage 0 record (2026-08-24) — making /admin/passives honest

Branch `feature/passive-tunables-stage0`, cut from origin/master at
95cd06e. Hygiene only: no combat.rs behavior change, no magnitude
changes, live TOML untouched, golden fixtures byte-identical.

1. **PENDING_MIGRATION_NODES grew 28 → 47.** The tunable audit
   (docs/tunable_audit.md §3) found 19 more nodes whose declared value
   never reaches the game because their only consumers read
   `passive_node_rank` (structure): the audit's original three
   (chakraoflife, unyieldingspirit, shattering) plus its Group-B drifts
   (deathdefiant, timewarp, demonicspeed, unwavering, unyieldingfaith,
   huntersfocus, golemmaster, healingflames, blazing, risingphoenix,
   virulence, cursedblood, livingbond, naturesembrace, verdantburst,
   finaloffering). Until each node's migration batch lands, an override
   on it would silently do nothing — so the page now renders it as
   pending instead of offering a dead input.

2. **PARTIALLY_TUNABLE_NODES — new mechanism, 7 mixed nodes:**
   naturesblessing, bloomingfield, reaperscall, ravage, unrelenting,
   endlessthirst, sacrifice. `node_is_tunable` is a whole-node boolean
   and cannot express a half-tunable node; hiding the row would cut off
   the node's WIRED aspect, offering it silently would repeat the
   inert-input lie. The smallest change that can express the half is a
   key→note side table: the node STAYS offered (its primary value
   accepts overrides through this store) while `/admin/passives` shows
   exactly which secondary aspect still reads node RANK.
   **mercifultouch**, flagged in the audit as "verify body", is
   VERIFIED WIRED — combat.rs reads its MAGNITUDE for Prayer of
   Mending's bounce value; its rank read is only an invested-gate, which
   is legitimate structure. It is deliberately on NEITHER list.

3. **CI consumption guard:**
   `every_special_node_whose_value_has_no_magnitude_read_is_tracked_as_not_yet_tunable`
   scans combat.rs / character.rs / manager.rs / adventure_web.rs
   (comment lines stripped, whitespace-insensitive, so wrapped call
   sites still match) for every Special/SpecialPerRank node's
   magnitude/count read. Any such node with NO read anywhere that is not
   tracked fails the suite naming the key. Demonstrated RED against the
   pre-fix lists — it named exactly the 19 keys above — then GREEN once
   they were listed. The older existence checks could not see this
   failure class: a key can exist in the tree and still never be
   consumed.

## Stage 1 record (2026-08-24) — the overflow economy becomes tunable

Branch `feature/passive-tunables-stage1`, stacked on Stage 0
(d246ee6) so the two deploy together. Behavior-neutral at defaults;
golden fixtures must stay byte-identical.

Five new `LiveTunables` fields (all HOT - read from each fight's own
snapshot during `CombatSimUnit` construction; nothing cached in
OnceLock/LazyLock anywhere in the chain):

| field | default | replaces |
|---|---|---|
| `overflow_conversion_cap_per_rank` | 0.10 | combat.rs's compile-time `OVERFLOW_CONVERSION_CAP_PER_RANK` (deleted), applied in `accumulate_overflow_conversion_bonus` |
| `evasion_overflow_cap` | 0.75 | the hardcoded Evasion arm in `combined_stat_overflow` AND `combat_evasion`'s own clamp |
| `block_overflow_cap` | 0.75 | same, for BlockChance / `combat_block_chance` |
| `dr_overflow_cap` | 0.75 | same, for DamageReduction's POSITIVE cap / `combat_damage_reduction` (the −0.75 floor is structural and stays fixed) |
| `intervene_overflow_cap` | 0.50 | same, for IntervenePct / `combat_intervene` INCLUDING its per-character combine `.min()` ceiling |

The three 0.75 caps stay SEPARATE fields (owner asked for the reasoning):
they gate different stats consumed by different builds, so one lane can
be nerfed without collateral on the others; DR additionally sits next to
the `defensive_stat_hard_cap` doctrine boundary and keeping its dial
distinct avoids any implied coupling. Cost is three floats.

Threading shape: `PassiveStat::overflow_cap(self, t: &LiveTunables)` is
now parameterized (it was only ever a gate - the REAL literals lived in
`combined_stat_overflow`'s match and each defensive getter, all of which
now take `t: &LiveTunables`), and every consumer of
`passive_overflow_bonus`/`combined_stat_overflow`/the defensive getters
threads the fight's snapshot through. The web dashboard fetches
`state.adventure.live_tunables()` at its handlers so the sheet shows what
fights actually use; `/admin/tunables` renders the five under a new
"Overflow Economy (cross-class caps)" heading, each form field carrying
`#[serde(default)]` per the CLAUDE.md trap rule, and the existing
page-derived POST drift guard now scrapes and posts them automatically.

Tests: `overflow_economy_tunables_tests` (character.rs) proves (1)
defaults reproduce the old math exactly (saturated Monk trio = +90%
under defaults), (2) `overflow_conversion_cap_per_rank = 0.05` halves it
to +45%, (3) input caps move each pool's start point by exactly their
delta, (4) two different snapshots against one Character give different
results and the untouched default still reads identically afterwards -
per-call freshness, no cache.


