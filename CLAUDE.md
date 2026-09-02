## Deploy procedure
Deploy procedure is documented in REFACTOR_PLAN.md §13 (authoritative,
supersedes any earlier version) — `git push origin master` remains a
required step there.

Note (2026-08-22): the pod-qa mirror's consumer session has been
retired by the owner. The pod-qa sync/verify steps that used to live
here are removed — no future session should run `sync-pod-qa.ps1` or
check pod-qa's HEAD as part of a deploy. `C:\pod-qa` and
`C:\sync-pod-qa.ps1` still exist on disk; do not delete or modify
either, and do not run icacls against `C:\pod-qa` — that decision
belongs to the owner separately, not to an automatic cleanup.

## Multi-session coordination

A parallel session may be overhauling the wiki module at any time.

1. Do NOT edit the wiki module (the extracted wiki source file(s)) or
   the /wiki route registrations — that's the wiki session's workspace.
   If your task seems to require touching them, stop and tell me.
2. Everything else — game logic, adventure_web.rs, combat, commands —
   remains yours to edit freely.
3. Shared helpers the wiki renders with (top_nav, compute_passive_layout,
   node_html) and wiki_slug() in adventure/manager.rs: if you need to
   change one's signature or behavior, tell me BEFORE doing it so I can
   sequence it with the wiki session. Additive changes are fine.
4. If you change anything player-facing — costs, chances, formulas,
   timers, boss behavior, crafting rules, command names — append one
   line to WIKI_IMPACT.md at repo root (create if missing):
   "<file>:<const or fn> — what changed — affects <bosses|crafting|passives|commands>"
   Do this even for tiny number tweaks; the wiki session consumes it.
5. Constants may gain pub(crate) visibility from the wiki session's work
   (it's wiring the wiki to read real values). Don't revert those, and
   don't rename or move player-facing constants without a WIKI_IMPACT.md
   line — renames break the wiki's imports.
6. Commit with explicit paths (git add <file>), never -a/-A; don't
   rebase/reset/stash shared state — another session may have work in
   flight.
7. wiki/*.md files are live content — the owner edits them directly;
   sessions request changes via WIKI_IMPACT.md, never edit them unasked.

## EFFICIENCY — BINDING

Token cost is a hard constraint on this project, not a preference. A task
that takes four times longer than it should is a failed task even if the
code is correct.

1. FIND THE PRECEDENT FIRST. This codebase is mature. Almost every task
   is "do what that other thing already does." Before reading anything
   broadly, search for an existing feature of the same shape and copy its
   pattern. Name the precedent you copied in your report. Adding a
   tunable field means finding the last tunable field that was added and
   mirroring it in the same four places — not reading the tunables
   system.

2. NEVER READ A LARGE FILE WHOLE. combat.rs is ~19,000 lines; manager.rs,
   character.rs, item.rs and adventure_web.rs are also large. Use
   targeted search and read narrow line ranges around the hits. A
   whole-file read poisons the rest of the session — context only grows,
   so every later tool call is charged against it.

3. ONE TASK PER SESSION. Stop at the task boundary and report, even if
   you have budget left. Multi-stage work runs as separate sessions with
   clean context. If an order contains several stages, do the first and
   say so.

4. TEST NARROWLY WHILE WORKING, BROADLY ONCE. Run the single relevant
   test while iterating. Run the full workspace suite once, before
   reporting. Report the summary line, not the dump.

5. ANSWER THE QUESTION ASKED. Do not audit adjacent code, enumerate
   related defects, or verify things nobody asked about. If you notice
   something, put ONE line in the journal under FOUND and move on. An
   exhaustive audit is a task someone orders explicitly, never a bonus.

6. STOP WHEN YOU ARE STUCK. If a mechanical problem has taken more than
   a few attempts, or you are reasoning in circles, stop and report.
   Trying harder on a stuck problem is the most expensive thing a session
   can do.

7. REPORT UNDER 400 WORDS. Detail belongs in the commit message, the
   journal, or a doc that can be read on demand. A long report is charged
   twice — once to write, once to read.

## THE OWNER'S ORDER IS THE SPEC — BINDING

This is Lokati's game. His request is a specification, not a starting
point for design.

1. BUILD WHAT WAS ASKED FOR. Not an improved version, not a first step
   toward it, not a more general version. If the order says "make the
   per-rank cap tunable on the passive page", that is the deliverable —
   a global dial on a different page is a failed task even if it is
   better engineering.

2. DISAGREE BEFORE, NEVER AFTER. If you believe a different approach is
   better, say so in ONE sentence and STOP. Wait for the ruling. Never
   implement your alternative and explain it in the report.

3. ADD NOTHING THAT WAS NOT ASKED FOR. No extra audits, no adjacent
   fixes, no "while I was in there". If you notice something, one line in
   docs/session_journal.md under FOUND, then move on.

4. SMALLEST CHANGE THAT SATISFIES THE ORDER. Find the existing feature
   that already works this way and copy it. Most requests are four lines
   in four places, not a design pass.

5. CONTENT BEFORE INFRASTRUCTURE. When a choice exists between shipping
   what the owner asked for and improving process, tooling, tests, or
   documentation, ship the feature. Infrastructure work happens only when
   it is blocking, or when explicitly ordered.

6. CHECK YOURSELF BEFORE YOU START. Three questions:
     - Is this exactly what was asked for?
     - Is this the smallest version of it?
     - Is this one thing?
   If any answer is no, say so and stop before writing code.

7. THIS PROJECT IS BEHIND SCHEDULE. Time and cost are the binding
   constraints, not elegance. A correct, minimal change delivered today
   beats a thorough one delivered tomorrow.

## Multi-session house rules (all Claude sessions in this repo)

ROLES. Feature sessions build on branches. The deploy session alone merges to master and deploys. The log parser verifies from fight logs and owns the anomaly ledger (its numbering is canonical). One release at a time, deployed only on the owner's explicit go.

BRANCH DISCIPLINE. Each feature session: own git worktree, branch off current origin/master, own --target-dir (production binaries are file-locked). Never touch the main checkout. Push and STOP — never merge to master, never deploy, never regenerate golden-corpus fixtures (report mismatches with attributed causes; regeneration happens at merge). Fixture ADDITIONS are allowed; re-capturing an unmerged draft needs explicit permission plus a per-diff explanation.

PRODUCTION SAFETY. Never terminate a process by image name. `taskkill /IM` (and any equivalent that matches on executable name — `Stop-Process -Name`, `pkill`) matches production by name and can kill the live game or bot; a `/FI` filter is not a safeguard, because a filter that matches nothing today matches everything the day it stops applying. Stop processes by PID, or by resolving the listening port to its owning PID, and confirm the resolved process path is NOT under `C:\PathofDust` before stopping it. This applies to disposable smoke/test instances too — they run the same `game.exe` image as production.

BUILD & TEST. cargo build --release --workspace (a plain build misses game.exe). Clippy clean on touched code. NO blanket cargo fmt — no rustfmt.toml exists; match file style by hand. Tests must be run with `cargo test --release --workspace --quiet` — the FULL workspace suite, not a single crate's internal tests — and every reported count must name the exact command that produced it. Crate-internal-only runs miss root-level integration tests (`tests/*.rs`), which is exactly how a real regression got past a "581 passed" report on 2026-08-22: a new `TunablesForm` field consumed by an HTTP handler had no `#[serde(default)]`, so `tests/admin_tunables_splash_http.rs` — which posts a fixed, pre-existing field set — started 422ing instead of redirecting, and the crate-internal run never touched that file. General trap worth knowing by name: any new form/struct field an HTTP handler consumes needs `#[serde(default)]`, or it silently breaks every existing integration test that still posts the old field set. **The trap runs in BOTH directions, and the second direction is the dangerous one because no existing test can see it** — 2026-08-23, dynamic pacing: the retired `dynamic_scaling_mult` had its `<input>` REMOVED from the rendered form and its handler read dropped, but the field stayed REQUIRED on `TunablesForm`. Every real browser save — which posts only what the page renders — 422'd and silently changed nothing, while the whole suite stayed green, because `admin_tunables_splash_http.rs` posts a hand-maintained SUPERSET body that still included the retired key. A superset body can never catch a field the page stopped rendering; it 422s only on fields it forgot to add, never on fields the page no longer sends. Durable rule: **a form POST test must derive its field set from the rendered page** (GET the page, scrape the `name="..."` attributes out of that form, POST exactly those), never from a hand-maintained list — then drift in either direction fails the suite instead of shipping. `admin_tunables_splash_http.rs` now does this; copy that shape for any new form. Report counts + failures only — never paste full passing output. Known flaky-under-parallel tests: the two legacy redistribution tests and live_reload_tests::editing_a_template_takes_effect_without_a_rebuild — confirm in isolation before flagging.

LARGE FILES. Never read combat.rs or any >5k-line file whole. Grep for symbols, then read targeted line ranges.

PROCESS. Fit report FIRST on every feature: verify the order's premises against the code, enumerate every touch point, propose a staged plan, then STOP for approval. If an order's premise is wrong, say so with evidence — refuting a premise beats building on it. Verified claims outrank code-trace claims: live logs and live click-throughs are the only close for behavior and web-form changes.

TUNABLES DOCTRINE. Every numeric aspect of a mechanic ships as a LiveTunable or node-value override unless genuinely structural (see Decision 16 in docs/passive_tunables_spec.md for the shared-constant exception). Damage reduction caps at defensive_stat_hard_cap (default 0.95) universally — no immunity through DR, ever. Flat/derived damage sources (Shattering icicles, Holy Fire) deliver through the shared dedicated path, never apply_hit.

REPORTS. Compact: tables for numbers, hashes for commits and binaries, a verdict per ordered item. Per-item "BLOCKED + reason" is required — silent omission is banned. No narrative recap of unchanged state. Self-corrections are stated plainly with what changed.

COMMITS & DOCS. git add with explicit paths, never -a/-A. Append a WIKI_IMPACT.md line for any player-facing change (append-only file; keep-both on merge conflicts). Never touch the wiki module or /wiki routes — the wiki session owns them. Every deploy ships a patch-notes entry (C:/PathofDust/patch-notes.json — gitignored runtime data). Patch notes are honest: nerfs say they are nerfs.

APPEND-ONLY, DEFINED (ruling 2026-09-02). Applies to `docs/anomaly_ledger.md`, `docs/session_journal.md` and `WIKI_IMPACT.md`. **Append-only means never lose a FACT, not never edit a LINE.** Permitted in place: status transitions (Open → Closed, NOT VERIFIED → VERIFIED), typo and encoding repairs, and moving text provided the move is annotated at the origin naming the destination. Banned: removing a substantive finding, and rewording an existing finding so it asserts something different. **A correction is a NEW dated entry that references the entry it corrects — never an overwrite of it.** The original stays readable, wrong, and dated, because a future reader chasing an old citation must land on what was actually written. If a claim is stale, annotate it with a dated note; do not rewrite it. Grandfathered: seven ledger commits (`ea2e19e`, `0ad5b52`, `45388d7`, `a031e37`, `7065c9d`, `307816d`, `c30e581`) and three journal commits (`2fbdde2`, `a031e37`, `38677a2`) predate this rule. All ten were audited line by line on 2026-09-02 and **lost no facts** — they are status transitions, cp1252 mojibake repair, and one properly-annotated move. No action needed on them.

BRANCH CLOSURE — NO SILENT ORPHANS (ruling 2026-09-02). **No branch may be marked CLOSED, done, or abandoned unless it has either been merged, or carries an explicit written statement of what happens to its contents.** That statement says where the content went, or that it is deliberately discarded and why. It goes in the final commit message and, for a branch carrying a document, in the document itself. This rule exists because `docs/affix-curve-spec` was closed with the message *"branch CLOSED"* while holding a 3,008-line owner-ratified spec that existed nowhere else. Master carried only a four-word stub pointing at it, a later session searched master, found nothing, and recorded in `docs/world2_build_plan.md` that the work did not exist — a wrong ruling that stood for five days. **The durable lesson, stated as a rule: absence from master is not absence.** Before recording that something does not exist, search every ref — `git log --all`, `git ls-remote origin` — not just the branch you are standing on. Corollary: a docs-only branch is not finished when it is pushed; it is finished when it is merged or explicitly disposed of.
