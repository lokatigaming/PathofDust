# Wiki Impact Log

Player-facing changes that may require wiki updates. The wiki session consumes
this. Append one line per change; newest at the bottom.

Format: `<file>:<const or fn> — what changed — affects <bosses|crafting|passives|commands>`

---

src/passive_tree.rs:MONK_NODES — Hundred Fists and One Hundred Hands swapped tiers: "One Hundred Hands" is renamed **"Flow like Water"** and is now the Specialization under Flowing Strikes (max rank 4), while Hundred Fists is now a Modifier under Pressure Point (max rank 3). Chakra of Many/Light/Life re-parented from Hundred Fists to Flow like Water. Both nodes' effects and description text are unchanged — only names, tiers, and parents moved — affects passives
src/adventure/combat.rs:onehundredhands_bonus_stacks — clamped to rank 3, so Flow like Water's 4th point only unlocks its Modifiers and does not grant a 4th bonus stack (matches every other Specialization and the node's own "up to 3 at 3/3" text) — affects passives
src/passive_tree.rs:"doom" node — detonation cut from 3/6/9% to 2/4/6% of damage dealt to the cursed target; the node's own description prose was updated to match, so the rendered passive text follows automatically — affects passives
src/adventure/combat.rs:ELEMENTAL_PROC_CHANCE_DIVISOR — 50.0 → 10.0, a 5x increase to how often Fire/Cold/Chaos/Lightning/Divine ailments apply on a landed hit; per-stack strength, stack caps and duration all unchanged — affects passives
src/adventure/combat.rs:apply_late_stage_penalty (new fn) — the late-stage damage penalty vs a real boss now applies to every damage source, not just normal attacks; previously bypassed by Lingering Effect DoT ticks, Warlock's Doom detonation (both on-expiry and on-death variants plus their Apocalypse splash), Mage's Volatile Magic splash, Slayer's Hemorrhage/Wound explosion, and reflected damage. Slayer's Culling Strike execute is deliberately exempt (guaranteed kill, no damage amount to scale) — affects passives
