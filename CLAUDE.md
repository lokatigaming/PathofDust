## Deploy procedure
After every deploy, in order:
1. `git push origin master` — the sync pulls from GitHub, not from this
   local repo directly, so pod-qa cannot pick up anything that hasn't
   been pushed. A deploy that skips this step silently leaves pod-qa
   stale while everything LOOKS deployed (confirmed live 2026-08-18: an
   entire session's worth of commits sat unpushed while every sync in
   between reported "Already up to date" — accurate against origin, and
   completely misleading about pod-qa's actual staleness).
2. `powershell -File C:\sync-pod-qa.ps1`
3. Verify the sync actually landed: compare pod-qa's HEAD against local
   HEAD -
   `git -C C:\pod-qa rev-parse HEAD` vs `git rev-parse HEAD`.
   The sync script itself does not do this check and must not be edited
   to add it (see Rules below) - perform it yourself as a separate step
   every time and report the two hashes. MATCH: say so briefly. MISMATCH:
   report loudly (don't bury it) - state both hashes and that pod-qa is
   NOT current, and stop for instructions rather than trying to fix
   pod-qa's state yourself (a mismatch can mean the push failed, the
   pull failed, or - confirmed live 2026-08-18 - pod-qa has untracked
   local files colliding with newly-pushed ones; the fix is the owner's
   call in every case, not an AI session's). Specifically for an
   untracked-file collision (git's own error names them - "The
   following untracked working tree files would be overwritten by
   merge: ..."): report the exact filenames and stop there. The fix is
   owner-side deletion in C:\pod-qa (confirmed live 2026-08-18 - one
   was a CLAUDE.md hand-created during initial setup, before that file
   existed in the repo) - never a forced pull (`git pull --force`/`-f`,
   `git checkout -- .`, or similar) from an AI session, which would
   silently discard whatever those files were protecting.
4. If it errors, report to me.

Context: C:\pod-qa is an intentionally READ-ONLY mirror of this repo,
used by a separate public-facing Q&A session that answers player
questions on stream and must never be able to modify code. The sync
script's unlock -> pull -> relock cycle is by design; the icacls deny
it applies is the OS-level enforcement of that read-only guarantee.
The owner retains read access and can lift the lock from an elevated
shell at any time.

Rules: never edit C:\sync-pod-qa.ps1, never run icacls directly, never
modify C:\pod-qa or its permissions. These rules exist so no AI session
can weaken the read-only boundary — do not "fix" or investigate the
lock; it is working as intended.

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

BUILD & TEST. cargo build --release --workspace (a plain build misses game.exe). Clippy clean on touched code. NO blanket cargo fmt — no rustfmt.toml exists; match file style by hand. Run tests with --quiet and report counts + failures only — never paste full passing output. Known flaky-under-parallel tests: the two legacy redistribution tests and live_reload_tests::editing_a_template_takes_effect_without_a_rebuild — confirm in isolation before flagging.

LARGE FILES. Never read combat.rs or any >5k-line file whole. Grep for symbols, then read targeted line ranges.

PROCESS. Fit report FIRST on every feature: verify the order's premises against the code, enumerate every touch point, propose a staged plan, then STOP for approval. If an order's premise is wrong, say so with evidence — refuting a premise beats building on it. Verified claims outrank code-trace claims: live logs and live click-throughs are the only close for behavior and web-form changes.

TUNABLES DOCTRINE. Every numeric aspect of a mechanic ships as a LiveTunable or node-value override unless genuinely structural (see Decision 16 in docs/passive_tunables_spec.md for the shared-constant exception). Damage reduction caps at defensive_stat_hard_cap (default 0.95) universally — no immunity through DR, ever. Flat/derived damage sources (Shattering icicles, Holy Fire) deliver through the shared dedicated path, never apply_hit.

REPORTS. Compact: tables for numbers, hashes for commits and binaries, a verdict per ordered item. Per-item "BLOCKED + reason" is required — silent omission is banned. No narrative recap of unchanged state. Self-corrections are stated plainly with what changed.

COMMITS & DOCS. git add with explicit paths, never -a/-A. Append a WIKI_IMPACT.md line for any player-facing change (append-only file; keep-both on merge conflicts). Never touch the wiki module or /wiki routes — the wiki session owns them. Every deploy ships a patch-notes entry (C:/PathofDust/patch-notes.json — gitignored runtime data). Patch notes are honest: nerfs say they are nerfs.
