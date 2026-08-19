# Memories — saved passive-tree builds

**Source of truth for this feature.** Committed here rather than left in
chat history specifically so a fresh session with no memory of the
planning conversation can pick it up correctly. Read this file in full
before touching Memories code. If an implementation needs to deviate
from anything here, document why in the commit message and add a
numbered entry to the Decisions log below.

Branch: `feature/memories` off `master`. Companion execution log:
`MEMORIES_PROGRESS.md`.

---

## What it is

Each character has Memory slots (3 by default) holding saved passive
builds. A Memory captures the **entire** build:

- `archetype`
- `passive_allocations` (the primary tree)
- `secondary_archetype` + `secondary_passive_allocations` (Split
  Personality's second tree)
- `golem_slot_types` (Elementalist's Golem Master choices)

Loading a Memory fully becomes that build. Management lives on
`/passives` only — save to a slot, load, rename, delete, and a
per-slot summary. **No chat commands this pass.**

## Design decisions already made — do not re-litigate

1. **3 slots, but implemented as a per-character value**
   (`Character::memory_slots`, defaulting to `STARTING_MEMORY_SLOTS`),
   not a hardcoded constant. A future feature must be able to grant an
   individual character extra slots with no migration and no change to
   any reader. Nothing downstream may assume 3.
2. **Swapping is free**, bypassing both `ARCHETYPE_CHANGE_COST` and
   `PASSIVE_RESPEC_COST` once a build is saved. See the economy note.
3. **Out of combat only.** A load during a fight is rejected with a
   clear message. No queuing, no mid-fight swaps.
4. **Level drift**: loading applies the snapshot's allocations exactly.
   Points earned beyond what the snapshot spent are left unspent for the
   player to place — never auto-spent.
5. **Default naming**: `"Memories of a Warrior"`, or
   `"Memories of a Warrior & Druid"` with a Split Personality secondary.
   Names are customizable.
6. **Class-change interaction**: loading a Memory whose archetype
   differs changes the class as part of the load, free.

## Rules the implementation must keep

- **Persistence is additive.** `memories` and `memory_slots` are
  `#[serde(default)]` fields on `Character`. Old saves load unchanged —
  proved by `a_character_saved_before_memories_existed_still_loads_with_three_empty_slots`
  and by `tests/character_fixture_roundtrip.rs` staying green.
- **Loading goes through the allocation validator, never a raw write.**
  Every stored rank is replayed through
  `passive_tree::validate_allocation_step` — the same function
  `AdventureManager::preview_allocate_passive` calls for a live click.
  There is exactly one implementation of "is this a legal tree".
- **Snapshot validation is graceful.** A stored allocation naming a node
  that no longer exists, or whose rank cap shrank, is dropped; its
  points are refunded as unspent; the rest of the build still applies;
  the player is told what changed. A load never fails outright and never
  corrupts the tree.

## How a load works

1. **In-combat gate.** Checked before anything is read or written, so a
   rejection leaves the character completely untouched.
2. Set `archetype` from the snapshot. Free.
3. **Replay the primary tree** in tier order (Skills → Specializations →
   Modifiers), each rank through `validate_allocation_step`. Tier order
   is what makes staleness cascade with no explicit cascade logic: a
   missing Skill takes its Specializations, which take their Modifiers.
   It also handles Monk's three Skill-parented Modifiers, since a
   parent of any tier is placed before its child either way.
4. **Secondary tree** applied only if Split Personality is still
   equipped *and* the stored secondary differs from the loaded primary —
   the same rule `save_passive_tree` already applies to a preview saved
   after the item came off, and the same filter
   `effective_secondary_archetype` uses. Otherwise skipped wholesale and
   reported.
5. **Budget trim.** If both trees together exceed
   `total_passive_points()`, drop in *reverse* replay order (Modifiers,
   then Specializations, then Skills; secondary before primary) until it
   fits. Reverse order is what guarantees a trim can never orphan an
   allocation that is still applied. Reachable in practice by saving
   with a high-tier Split Personality equipped (worth `1 + tier / 300`
   extra points) and loading without it.
6. `golem_slot_types` restored verbatim, including onto a
   non-Elementalist build where they are inert — same non-lossy
   reasoning that field's own doc gives for never trimming on respec.
7. Clear the pending passive preview (same reason `change_archetype` and
   `set_secondary_archetype` both do: a preview built against the old
   tree keeps counting against the shared budget and could be Saved
   straight over the freshly loaded build), persist, broadcast.

## Name rules

`validate_memory_name` is the only path to a stored name. Trim → reject
empty → reject over `MEMORY_NAME_MAX_LEN` (150) **characters** → reject
control characters and the zero-width/bidi ranges → blocklist.

---

## Economy note (accepted, recorded deliberately)

Free full-build swaps materially reduce the existing class-change dust
sink. **One paid class change plus a saved Memory buys unlimited free
switching between those builds thereafter.** `ARCHETYPE_CHANGE_COST` and
`PASSIVE_RESPEC_COST` (1000 dust each) stop being a recurring cost for
anyone who plans ahead.

This is a deliberate convenience trade-off, accepted by the owner at
design time — recorded here so a future economy review sees it was a
decision, not an oversight. Pinned by
`loading_a_memory_is_free_and_never_touches_dust_or_the_free_change_counters`
so it can't be quietly changed into charging without someone confronting
this note.

---

## Decisions log (newest last)

1. **`Vec<Option<Memory>>`, not `Vec<Memory>`.** Slots have identity —
   filling slot 3 while 1 and 2 are empty has to stay slot 3, which a
   compacting vec can't express. Read through `memories_padded()` /
   `memory_slot()`, which normalize any stored length to `memory_slots`.
2. **`Memory` stores the RAW `secondary_archetype`**, not
   `effective_secondary_archetype()`'s output. Storing the raw choice
   and re-deriving effectiveness at load time is what every other reader
   in the codebase does; a Memory saved while Split Personality was
   equipped and loaded while it isn't must resolve to "no secondary"
   live, not resurrect one. (`snapshot_build` does read the *effective*
   value when capturing, so an inactive secondary tree is never captured
   in the first place.)
3. **Orphaned allocations are dropped and refunded, not reproduced.**
   The live tree lets a player de-allocate a parent while its children
   keep their ranks — validation is node-local and de-allocation does not
   cascade — so a saved snapshot legitimately can contain orphans.
   Strict replay drops them. That is *stricter* than the live UI and
   never looser, which is the invariant that matters ("never produce a
   state the UI couldn't have built"). Cost: save → immediately load can
   lose an orphan. Owner-ratified. The underlying cascade gap is a
   separate backlog item.
4. **A rank above the current cap is dropped whole, not clamped down.**
   A partially applied rank is a build the player never chose; refunding
   lets them re-spend deliberately.
5. **`a`/`an` handled in default names.** The design wrote the pattern
   literally as `"Memories of a <Class>"`, but four archetypes start
   with a vowel. `"Memories of an Elementalist"` is used instead — a
   default name is player-facing text and the literal form reads as a
   typo. Owner-approved.
6. **The blocklist over-rejects on purpose.** This is the first content
   filter in this codebase — nothing existed to reuse (verified across
   both crates). It matches as a **substring** against a **normalized**
   form (lowercased, non-alphanumerics stripped, common digit-for-letter
   swaps folded), so separator and leetspeak evasion trips the same
   entry. That knowingly rejects innocent names containing an entry as a
   substring (the Scunthorpe problem). For a string the bot may echo
   into Twitch chat, a false rejection costs a player one retry and a
   false acceptance is a ToS violation on the streamer's channel — the
   asymmetry is intentional. Owner-approved. The list is a floor meant
   to be extended in place, not a moderation system.
7. **A rejected name is never echoed back.** `memory_error_text` says
   "that name isn't allowed" without quoting the offending word or
   naming the entry that tripped — echoing it would put exactly the
   string the filter exists to suppress back onto the page and into the
   redirect URL. Asserted by the HTTP test.
8. **In-combat is detected from `fight_gate`'s LOCK, not its `Instant`.**
   There is no per-character in-combat flag in this codebase and
   deliberately shouldn't be one: combat resolves instantaneously inside
   `simulate_battle` and the overlay only animates the resulting log
   afterwards. `run_encounter`/`run_basic_encounter` hold `fight_gate` as
   a guard for a fight's whole duration, so a failed `try_lock` is
   exactly "a fight is in flight". The global signal is per-character
   accurate because `eligible_fighters` pulls in every non-downed,
   non-retreated character. The stored deadline is deliberately *not*
   used: it extends past the fight to cover overlay playback plus a 5s
   spacing floor, and blocking a build swap during that quiet tail would
   be stricter than "in an active encounter" means.
9. **Manager tests use absolute scratch paths, not `set_data_dir`.**
   `paths.rs`'s own doc records that racing to be that `OnceLock`'s
   first caller makes a test inherently flaky in a shared test process.
   Absolute paths sidestep it: `data_path` joins onto an empty base, and
   joining an absolute path onto an empty base is that absolute path.
   The HTTP test *does* use `set_data_dir`, which is safe because each
   `tests/*.rs` file is its own process.
10. **Empty Memory slots always render**, unlike the golem and Split
    Personality sections which hide until the player has the prerequisite.
    Empty slots are this feature's entry point; hiding them would hide
    the feature.
11. **Memory names never reach the inline `confirm()` strings.**
    `escape_html` does not escape `'` and minijinja autoescaping is off
    for this template, so the confirm text is static and names go only
    into double-quoted attributes or element text. Pinned by
    `a_name_containing_an_apostrophe_never_reaches_the_inline_confirm_script`.
12. **Saving is refused for Commoner** (`MemoryError::NoBuildToSave`).
    Commoner's `passive_nodes()` is empty, so the snapshot could only
    ever load as "become a Commoner with nothing allocated".
13. **Overwriting an occupied slot is allowed**, behind a `confirm()` —
    it's the natural "update this build to what I'm playing now"
    gesture, and the button says Overwrite.

---

## Out of scope this pass (raised, deliberately not done)

- **Chat commands.** Dashboard only, per the design.
- **The de-allocation cascade fix** (Decision 3's underlying gap).
- **`respec_passive_tree` clearing only the primary tree** while
  charging the full free-token/1000-dust cost. The owner has ruled this
  a bug — intended behavior is that a paid respec clears both trees. It
  ships as its own release; a comment marks the site.
- **The item-nickname content filter retrofit.** `name_item` only trims
  and truncates, yet nicknames reach Twitch chat and the OBS overlay via
  `Item::display_name()`. `validate_memory_name` is written to be
  reusable there. Priority backlog item.
- **`templates/base.html` emits `{{ body }}` a second time inside a
  JavaScript comment** (line 685, in the scroll-restore block's own
  doc comment), duplicating every page's entire body on every page of
  the site. Beyond doubling page size, any newline in the body ends the
  `//` comment and the remainder is parsed as JavaScript, killing the
  whole inline script block. Player-reachable today through item
  nicknames, which are not newline-stripped. Confirmed empirically, not
  by reading. Found while writing this feature's HTTP test; reported,
  not fixed here.

## Verification

- `cargo build --release --workspace --target-dir target-memories`
  (`--workspace` is required; a separate target dir is mandatory because
  `target/release/` holds live, file-locked production binaries).
- `cargo test --workspace --all-targets --target-dir target-memories`.
- `cargo clippy` clean on touched code.
- **No `cargo fmt`** — there is no `rustfmt.toml` and a blanket run
  rewrites unrelated code (`ELEMENTALIST_PROGRESS.md` Decision 6).
- Golden-corpus fixtures are neither regenerated nor deleted.
