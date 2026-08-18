# Elementalist Class — Implementation Progress

**Status** (2026-08-19): Stage 0 (investigation + plan) complete and
approved. Working autonomously through Stages 1-6 overnight per
explicit owner authorization — no check-ins, most-conservative-choice-
and-document rule for any decision point, branch/commit/push only, no
deploy, no master merge. This file is written so a brand-new session
with zero memory of the planning conversation can resume exactly where
things stand — read this file first, then `docs/elementalist_spec.md`
(once Stage 1 creates it) for the full class spec.

Branch: `feature/elementalist` off `master` (created at the `master`
commit that includes the boss-size overlay fix, 2026-08-19 — Phase 1
architecture refactor is complete and deployed on `master` as of this
branch point).

Baseline before any Elementalist work: **156 tests passing, 0 failed**
(`cargo test --workspace --all-targets`), confirmed 2026-08-19
immediately before Stage 1 began. This is the number every later
stage's "all existing tests still pass" check is protecting.

---

## Stages

- [x] Stage 1 — `docs/elementalist_spec.md` + tree skeleton (39 nodes,
      structurally real, `NotYetImplemented` effects) + `Elementalist`
      archetype/access (done 2026-08-19)
- [ ] Stage 2 — Elemental Focus branch (proc-frequency, crit mods,
      gear-scaled elemental damage)
- [ ] Stage 3 — Righteous Fire part 1 (damage + self-burn, Scorching
      Flames, Relentless/Cauterizing Flames, Ashes to Ashes)
- [ ] Stage 4 — Righteous Fire part 2 (regen clock, Fanning Flames,
      Rising Phoenix, Shielding Flames, Cleansing Flames)
- [ ] Stage 5 — Golem foundation (summoner-death rule, dead-but-
      scheduled lifecycle state, on-death hook wrapper, Golem Master,
      `/passives` type-picker UI, Basic golem)
- [ ] Stage 6 — Golem types (Thunder/Flame/Water) + the mandatory
      Thunder Golem all-`BossKind` damage-isolation test

(Stage numbering/order per the approved plan at
`C:\Users\Administrator\.claude\plans\jaunty-pondering-waterfall.md` —
that file is the design rationale/architecture-fit record; this file
is the live execution log.)

---

## Decisions (running log — every decision made without a check-in goes here, newest last)

*(Stage 0, already approved by the owner before autonomous work began —
listed here for completeness, not decisions made unattended):*

1. **Golem overlay visibility**: invisible in v1 - golems are
   mechanically full participants (act/absorb/heal/explode, all
   correct in the combat log and fight outcome) but draw no OBS
   sprite. Visualization is an explicit future follow-up. Reason:
   keeps this feature's frontend footprint at exactly what was scoped
   (`/passives` only) - the overlay's player-formation loop has no
   mechanism for non-roster units today and building one is real,
   separate frontend work.
2. **Rising Phoenix vs. frozen `PlayerVitals` contract**: `died_at_ms`
   stays single-shot, recording only a unit's FINAL death in a fight
   (a revived-then-still-alive unit never sets it). The HP-sample
   curve and combat log show the real dip-and-recovery regardless.
   Reason: `died_at_ms` is documented as frozen for an external
   companion-app consumer; this satisfies that contract with no
   coordination needed while still delivering the real gameplay effect
   everywhere else.
3. **Thunder Golem damage absorption scope**: external damage only -
   does NOT cancel the Elementalist's own Righteous Fire self-burn.
   Reason: the spec's own design note says sustaining RF should
   require Shielding Flames/allies/gear, not "own a Thunder Golem";
   canceling self-burn would trivially defeat that intended cost.
4. **Golem/`any_player_alive` interaction (owner-mandated rule, not a
   free choice)**: golems die when the Elementalist dies, and golems
   never count toward `any_player_alive`'s fight-termination check.
   Reason (owner's own): without this, an all-real-players-dead fight
   with a reforming Thunder Golem would never terminate.

5. **Verification builds use a separate `--target-dir target-elementalist`,
   never plain `cargo build`/`cargo test`.** `target/release/{game,
   twitch-bot-rs}.exe` are the LIVE production processes (confirmed
   running throughout this work) - a plain release build tries to
   overwrite their locked files and fails with "Access is denied."
   `/target-elementalist` added to `.gitignore`. Every stage's
   verification in this log uses this target dir; production was
   reconfirmed undisturbed (same PIDs/StartTimes) after the first build
   hit this and after switching.
6. **Dropped blanket `cargo fmt` from the per-stage checklist.** No
   `rustfmt.toml` exists in this repo, and running plain `cargo fmt
   --check` against the actual crate shows the EXISTING codebase
   (checked: `affix.rs`, untouched by this work) doesn't match
   rustfmt's default output - wide struct literals stay single-line
   where default rustfmt would wrap them. This predates Stage 1
   entirely; applying `cargo fmt` for real would rewrite large amounts
   of pre-existing, unrelated code as a side effect of this feature,
   which is worse than not running it. New code in this branch is kept
   visually consistent with the file's own established conventions by
   hand (matching e.g. `WARRIOR_NODES`'s one-node-per-line-or-wrapped-
   when-long style) instead.

*(Everything above this line was decided without a check-in, during
autonomous execution. Nothing further yet - this section grows as
Stages 2-6 proceed.)*

---

## Unresolved / open items

- Found (not fixed - out of scope, see Decision 6): Monk's tree has a
  pre-existing structural inconsistency - the "windwalker" modifier is
  parented directly to the "flowingstrikes" SKILL rather than to one of
  its 3 Specialization children. Predates this work entirely. Worth
  the owner's awareness, not this branch's to fix.

## Stage 1 summary (for the morning, or a fresh session)

- `docs/elementalist_spec.md` committed - full spec, all Stage 0
  resolutions, now the source of truth for Stages 2-6.
- `Archetype::Elementalist` added (`character.rs`): `combat_function()`
  → Ranged, `bonus()` → splash only (matches Ranger's magnitude, no
  baseline heal_power_pct - see spec doc), added to `ALL_ARCHETYPES`
  (now 12) and the passives-page icon match (🔥) in `adventure_web.rs`.
- Full 39-node tree in `passive_tree.rs` (`ELEMENTALIST_NODES`): 3
  skills → 9 specs → 27 modifiers, every node `NotYetImplemented`.
  Two node keys renamed from their spec display name to avoid a
  collision with an existing archetype's key: "Blizzard" → key
  `hoarfrost`, "Conflagration" → key `pyroclasm` (both display names
  unchanged, only the internal key differs - see the file's own
  section comment).
- 15 new tests (7 in `character.rs`'s `elementalist_tests`, 8 in
  `passive_tree.rs`'s `tree_shape_tests` - the latter is the first
  structural tree validation this codebase has ever had, generic
  enough to protect every future archetype's tree too, not just this
  one). Full suite: **171 passing, 0 failed** (was 156).
- Commit is next.

---

## Hard-stop conditions (reminder to self - halt and wait if any occur)

1. A change would break loading of existing (pre-Elementalist) save
   files.
2. Something forces violating one of the 4 decisions above.
3. The same stage fails verification (`cargo build --release
   --workspace`, clippy, fmt, full test suite, save-fixture test) 3
   times in a row.

If any of these fire, write exactly why here, in this section, BEFORE
doing anything else - then stop.
