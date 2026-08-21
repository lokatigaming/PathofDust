---
name: deploy-release
description: Execute a production release for Path of Dust per REFACTOR_PLAN.md section 13. Use for every merge-and-deploy.
---
Sequence, no steps skipped, per-item verdict in the report: (1) merge (conflicts: trivial keep-both only; anything substantive in combat.rs or the write path is STOP-and-report); (2) full suite green, --quiet, counts only; corpus regeneration only here, with diffs attributed; (3) disable watchdogs; stop bot, then game; (4) SHA-256 both old binaries, back up to a per-release backup dir with the pinned fight-summary snapshot; (5) swap; game up + health-check before bot starts; re-enable watchdogs; (6) patch-notes entry; (7) push master, run pod-qa sync, verify pod-qa HEAD = local HEAD, report both; (8) report: per-item hash or BLOCKED+reason. Rollback = restore the backup dir binaries. Backup dirs older than 7 days are prunable.
