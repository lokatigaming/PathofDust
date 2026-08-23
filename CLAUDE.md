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

## Multi-session house rules (all Claude sessions in this repo)

ROLES. Feature sessions build on branches. The deploy session alone merges to master and deploys. The log parser verifies from fight logs and owns the anomaly ledger (its numbering is canonical). One release at a time, deployed only on the owner's explicit go.

BRANCH DISCIPLINE. Each feature session: own git worktree, branch off current origin/master, own --target-dir (production binaries are file-locked). Never touch the main checkout. Push and STOP — never merge to master, never deploy, never regenerate golden-corpus fixtures (report mismatches with attributed causes; regeneration happens at merge). Fixture ADDITIONS are allowed; re-capturing an unmerged draft needs explicit permission plus a per-diff explanation.

PRODUCTION SAFETY. Never terminate a process by image name. `taskkill /IM` (and any equivalent that matches on executable name — `Stop-Process -Name`, `pkill`) matches production by name and can kill the live game or bot; a `/FI` filter is not a safeguard, because a filter that matches nothing today matches everything the day it stops applying. Stop processes by PID, or by resolving the listening port to its owning PID, and confirm the resolved process path is NOT under `C:\PathofDust` before stopping it. This applies to disposable smoke/test instances too — they run the same `game.exe` image as production.

BUILD & TEST. cargo build --release --workspace (a plain build misses game.exe). Clippy clean on touched code. NO blanket cargo fmt — no rustfmt.toml exists; match file style by hand. Tests must be run with `cargo test --release --workspace --quiet` — the FULL workspace suite, not a single crate's internal tests — and every reported count must name the exact command that produced it. Crate-internal-only runs miss root-level integration tests (`tests/*.rs`), which is exactly how a real regression got past a "581 passed" report on 2026-08-22: a new `TunablesForm` field consumed by an HTTP handler had no `#[serde(default)]`, so `tests/admin_tunables_splash_http.rs` — which posts a fixed, pre-existing field set — started 422ing instead of redirecting, and the crate-internal run never touched that file. General trap worth knowing by name: any new form/struct field an HTTP handler consumes needs `#[serde(default)]`, or it silently breaks every existing integration test that still posts the old field set. Report counts + failures only — never paste full passing output. Known flaky-under-parallel tests: the two legacy redistribution tests and live_reload_tests::editing_a_template_takes_effect_without_a_rebuild — confirm in isolation before flagging.

LARGE FILES. Never read combat.rs or any >5k-line file whole. Grep for symbols, then read targeted line ranges.

PROCESS. Fit report FIRST on every feature: verify the order's premises against the code, enumerate every touch point, propose a staged plan, then STOP for approval. If an order's premise is wrong, say so with evidence — refuting a premise beats building on it. Verified claims outrank code-trace claims: live logs and live click-throughs are the only close for behavior and web-form changes.

TUNABLES DOCTRINE. Every numeric aspect of a mechanic ships as a LiveTunable or node-value override unless genuinely structural (see Decision 16 in docs/passive_tunables_spec.md for the shared-constant exception). Damage reduction caps at defensive_stat_hard_cap (default 0.95) universally — no immunity through DR, ever. Flat/derived damage sources (Shattering icicles, Holy Fire) deliver through the shared dedicated path, never apply_hit.

REPORTS. Compact: tables for numbers, hashes for commits and binaries, a verdict per ordered item. Per-item "BLOCKED + reason" is required — silent omission is banned. No narrative recap of unchanged state. Self-corrections are stated plainly with what changed.

COMMITS & DOCS. git add with explicit paths, never -a/-A. Append a WIKI_IMPACT.md line for any player-facing change (append-only file; keep-both on merge conflicts). Never touch the wiki module or /wiki routes — the wiki session owns them. Every deploy ships a patch-notes entry (C:/PathofDust/patch-notes.json — gitignored runtime data). Patch notes are honest: nerfs say they are nerfs.
