## Deploy procedure
After every deploy: powershell -File C:\sync-pod-qa.ps1
If it errors, report to me.

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
