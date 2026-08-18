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
   call in every case, not an AI session's).
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
