//! Passive skill tree data - one PoE-style tree per archetype, see
//! `Archetype::passive_nodes`. Format: 3 tier-1 Skills (max rank 3, no
//! prerequisite) -> 9 Specializations (3 per skill, max rank 4 - the 4th
//! point "specializes" it and unlocks its 3 modifiers, without adding a
//! further increment of the specialization's own stat) -> 27 Modifiers (3
//! per specialization, max rank 3, gated behind their parent hitting 4/4).
//! Root passives (one flat stat per archetype, scaling with level) already
//! exist separately as `Archetype::bonus()` - this file is everything
//! ABOVE that baseline.
//!
//! Every node's STRUCTURE (parent/rank/unlock gating/name/description) is
//! real and allocatable from day one, so points spent anywhere are
//! banked, never wasted - but not every node's `PassiveEffect` is real
//! yet. Built up across several passes: the original Foundation Pass
//! (2026-08-14, 23 nodes), Slayer's and Cleric's dedicated bespoke
//! follow-ups (`Special`-effect mechanics - wound stacks, Bloodpact,
//! shields, heal bounce, party-wide grants - 15 and 31 nodes), a
//! flat-bonus/overflow-conversion follow-up across the other 9
//! archetypes (2026-08-15) that activated every remaining node shaped
//! like "amplifies an already-real flat/overflow parent" - including
//! chains (an amplifier of an amplifier) and cross-stat overflow grants,
//! both of which `passive_bonus`/`passive_overflow_bonus` already sum
//! correctly with zero new plumbing (100 nodes as of that pass), a
//! same-day Cleric-clone follow-up (Druid's Rejuvenation/Nature's
//! Blessing families sharing Prayer of Mending/Sanctified Touch's own
//! `CombatSimUnit` fields, Mage's Arcane Shield as a single crit-triggered
//! `grant_shield` call, Warlock's Soul Harvest + Eternal Hunger as an
//! on-kill heal-then-shield - 111 nodes), and a same-day third pass
//! building Paladin's Divine Shield as a genuinely new periodic self-cast
//! (the third instance of the Helm/Boots clock pattern) plus its cheap
//! amplifiers (118 nodes), and a same-day fourth pass adding a shared
//! shield-absorb-reflect primitive (`shield_reflect_pct`/
//! `apply_reflect_damage`) that retroactively unlocked Cleric's Sacred
//! Barrier and Slayer's Guardian's Blood alongside Paladin's own
//! Retribution Aura + Holy Vengeance (122 nodes), and a same-day fifth
//! pass covering self-conditional missing-HP% scaling that turned out to
//! be free wherever `resolve_hit`/`roll_attacker_damage`'s existing live
//! hp/max_hp already covered it (Berserker's Gambit - the archetype's
//! first-ever real node - and Druid's Unyielding Roots), plus a second
//! reflect variant (Druid's Bramblegrowth + Thornlash/Poison Thorns,
//! reusing `apply_reflect_damage` off a hit's DR-reduced amount instead
//! of a shield's absorbed amount - 127 nodes), and a same-day sixth pass
//! that redesigned Berserker's ENTIRE Frenzy branch (13 nodes, all
//! newly real) from a kill-proc into a per-attack multi-strike chance
//! (`fire_frenzy`), per a live design conversation - see
//! `BERSERKER_NODES`'s own doc for why this was safe to redefine in
//! place rather than needing a migration. 140 of 429 allocatable nodes
//! carry a real `PassiveEffect` as of that pass, though a same-day audit
//! (prompted by a live report that Warrior's Unbreakable "wasn't
//! working") found all 13 `OverflowConversion` nodes across 6 archetypes
//! (Unbreakable, Elusive/Phantom/Duskveil, Stone Fist/Granite Skin/
//! Earthen Will, Aegis Ward/Sanctified Armor, Lightfoot, Shifting Form/
//! Primal Shift/Claw Strike) were reading `PassiveStat`-correct but
//! practically dead: no archetype's tree-only investment in a capped
//! stat can get anywhere close to its 75%/50% cap (the closest, Druid's
//! Evasion, tops out at 47%). Fixed in `Character::passive_overflow_bonus`
//! (adventure.rs) by drawing from COMBINED gear+archetype+tree overflow
//! instead of tree-only - deliberately overlapping with (not replacing)
//! `defensive_overflow`'s own separate gear-only conversion into
//! Increased Damage, per an explicit live design call that a passive
//! node paying out again off the same raw overflow is intended, not a
//! double-counting bug. The same audit also caught 6 of those 13 nodes
//! (the top-level one in each branch) not matching their own description
//! text - each said "X% efficiency per rank... 2X% at 3/3" but was coded
//! with `per_additional_rank` equal to the FULL `at_rank_1` value instead
//! of half of it, actually reaching 3X% at rank 3 - fixed alongside the
//! overflow-source change.
//! A same-day seventh pass added Warrior's Aegis + Spike Barrier (Bulwark's
//! last two specs - a blocked hit shields the lowest-HP ally / reflects
//! damage back at the attacker, reusing `grant_shield`/`apply_reflect_damage`
//! off the same "total mitigation on this hit" quantity Bramblegrowth
//! already established, gated on a new `HitOutcome::is_blocked` flag
//! instead of any-mitigation) and a new stacking timed-buff primitive
//! (`add_speed_stack`/`speed_stack_multiplier`, read live at the point a
//! unit's next action gets scheduled rather than baked into
//! `attack_interval_ms`) standing up Warrior's Momentum (on a landed hit)
//! and Rogue's Fleetfoot (on a successful evade) - Berserker's Bloodlust
//! and Monk's Flowing Strikes want the exact same primitive with different
//! triggers/output stats and are the natural next follow-up, not part of
//! this pass (144 of 429 nodes now real).
//! A same-day eighth pass built exactly that follow-up: Berserker's
//! Bloodlust (skill + its 3 specs - Unending Rage/Overwhelm/Frenzied
//! Blows) extends the SAME stack counter Momentum/Fleetfoot use with a
//! per-stack increased-damage output (`stack_dmg_per_stack`, read in
//! `roll_attacker_damage`) and Overwhelm's per-stack target-DR-shred
//! (`stack_shred_per_stack`, plugged into `resolve_hit`'s existing
//! reduction-sources list as the negative source its own doc already
//! anticipated); Monk's Flowing Strikes (skill + its 3 specs - Hundred
//! Fists/Pressure Point/Relentless Assault) needed a genuinely separate
//! counter (`add_flowing_stack`) since its trigger is "consecutive hit on
//! the SAME target" rather than "any hit lands" - Pressure Point's crit
//! bonus reads the same way Berserker's Gambit already does. Flowing
//! Strikes' own description text had a self-contradiction caught in
//! passing (stated both "+2% per rank" AND "+5% per stack at 3/3"/"25% at
//! cap", which don't agree - fixed to +1%/rank, the value both endpoint
//! figures already corroborated, same "trust the corroborated numbers"
//! call as the earlier overflow-node audit). Deeper Modifier-tier children
//! of these 8 new spec nodes (18 more, split across the two branches)
//! stay deferred, same "skill/spec first" precedent as every other pass
//! (152 of 429 nodes now real).
//! A ninth pass (2026-08-15) covered two more things: first, the cheap
//! remainder of the `stack_speed_*` primitive - Ranger's Relentless
//! Pursuit and Mage's Flow State are textually IDENTICAL to Momentum's
//! own shape ("each hit... stacking... max 5 stacks"), so they just sum
//! into the same bundle at construction, zero new code. Warlock's Fel
//! Rush looked like a third freebie but isn't: its own text ("a KILL
//! grants... for 4s") is a flat refreshed-on-kill buff, not a per-hit
//! stack, and reusing `stack_speed_*`'s unconditional per-hit trigger
//! would have incorrectly also granted it on ordinary attacks - it got
//! its own small dedicated pair (`fel_rush_speed_bonus`/
//! `_expires_at_ms`) and its own on-kill call site instead. Slayer's
//! Blood Frenzy/Endless Thirst/Reaper's Momentum want the identical
//! treatment (their own bundle, triggered from FlickerStrike's dash
//! specifically, not generic hits) but are real enough in scope to
//! deserve their own follow-up rather than being rushed into this one -
//! still deferred.
//! Second: Ranger's Hunter's Mark + Warlock's Curse of Weakness, a new
//! shared primitive (`apply_first_hit_mark`) - on a unit's first landed
//! hit each fight, it writes a persistent debuff onto that target (no
//! expiry - lives for the rest of the fight), read by `resolve_hit`
//! afterward: Mark's crit-chance/crit-multiplier/low-hp-damage bonuses
//! are personal to the marking Ranger (Pack Tactics is the one
//! exception, extended to allies); Curse's damage-taken bonus is
//! unconditional, read by any attacker, reusing the exact same negative-
//! reduction-source slot Overwhelm's shred already established.
//! Contagious Curse spreads the same value to more random enemies at
//! application time. Doom (detonates when the curse EXPIRES) stays
//! `NotYetImplemented` - nothing in this primitive ever naturally clears
//! a mark/curse mid-fight, so its trigger condition can't fire; same
//! "left implemented-but-inert, no migration needed the day the sim
//! grows real expiry" precedent as Slayer's Rot/Withering Touch (162 of
//! 429 nodes now real).
//! A tenth pass (2026-08-15) also separately rebuilt Paladin's Radiant
//! Smite branch around offensive healing (own heal-on-hit, no defender-
//! side state at all - see `smite_heal_pct`'s doc), and shipped 8 more
//! nodes: Mage's Temporal Rift + Warlock's Unstable Power (identical
//! text - a new `attack_speed_pct`/`speed_overflow_dmg_pct` pair reads
//! the unit's BASELINE gear+tree attack speed, exposed via a new
//! `Character::combat_attack_speed_pct`, and converts whatever's past
//! 100% into damage - deliberately excludes live per-fight stacking
//! buffs, same baseline-vs-live split every other node here already
//! draws); Rogue's Twin Strikes + Mage's Spell Echo (identical text - a
//! crit has a chance to strike/cast again at 50% damage, same family as
//! Frenzy but crit-gated - needed one new `is_followup` parameter on
//! `apply_hit` itself rather than threading `outcome.is_crit` back out
//! through its 11 other call sites); Druid's Pack Instinct + Symbiosis
//! (both ride already-real flat-stat parents, granting a fraction to
//! whoever's CURRENTLY the party's lowest-HP ally - a genuinely live
//! per-hit check, computed in `apply_hit` since `resolve_hit`'s plain
//! `atk`/`def` refs can't see the whole party, and passed down the same
//! way Mark/Curse's bonuses already are); and Paladin's Vow of
//! Protection + Unbreakable Faith (Guardian's Oath's last two specs -
//! Vow of Protection folds straight into the existing Cleric party-
//! broadcast loop, Unbreakable Faith hooks the Intervene-redirect branch
//! directly). Rogue's Opportunist branch was flagged for a design
//! rework instead of being implemented as originally worded - not
//! touched this pass (170 of 429 nodes now real).
//! An eleventh pass (2026-08-15) redesigned Ranger's Volley + Mage's
//! Chain Lightning around a live request, replacing their original
//! "splash overflow converts to extra targets" text entirely: both now
//! deal flat bonus damage per rank for every target the attack is
//! CAPABLE of reaching (1 primary + its own max splash target count,
//! overflow bonus included) rather than however many it actually hits -
//! computed once at the attack site from the unit's own splash stat, no
//! live target-counting or new persistent state needed at all (172 of
//! 429 nodes now real).
//! A twelfth pass (same day) shipped the rest of that round's scoping
//! menu, 12 nodes: Rogue's Exploit Weakness/Nightstalker (both computed
//! directly in `resolve_hit` off `atk`/`def` alone - a live target-HP
//! check and a live is-attacker-a-boss check, no external state needed)
//! and Assassinate (charges banked per fight, same non-linear rank gate
//! as Guardian Spirit - consumed in `apply_hit` before `resolve_hit`
//! rolls anything, forcing at least one crit stack); Monk's Temple
//! Guardian (rides the exact same live lowest-HP-ally primitive Pack
//! Instinct/Symbiosis already proved out - same field, mutually
//! exclusive by archetype); Warlock's Dark Communion (hooks the existing
//! life-leech handling directly - a fraction of the SAME leeched amount
//! also heals the attacker's lowest-HP ally); Berserker's whole Reckless
//! Swing branch (the trade's two independently-scaling percentages -
//! dealt and taken don't share one linear formula - are rank-matched
//! directly via small dedicated functions instead of forcing them
//! through `PassiveStat`, folded straight into `increased_damage`/
//! `damage_reduction` at construction; Vigor is a plain kill-heal); and
//! Warrior's whole Retaliation branch (a hit taken has a chance to
//! immediately counter-attack back - the first "reactive to being hit"
//! trigger in the tree, added at the very end of `apply_hit` gated on
//! the target having survived and reusing Twin Strikes' `is_followup`
//! guard so the counter-attack can't chain into another Retaliation
//! check off itself) (184 of 429 nodes now real).
//! A thirteenth pass (same day) redesigned Monk's Unbroken around a live
//! request (the original "+evasion per 20% missing HP" text felt weak) -
//! it's now an `OverflowConversion`-SHAPED node that isn't actually
//! expressible via that generic system, since its output ("ignore this %
//! of whoever you attack's own evasion") isn't one of the 12 pooled
//! `PassiveStat`s the machinery can add to; it's its own `Special`-effect
//! getter (`Character::combat_unbroken_ignore_evasion_pct`) that still
//! reuses `combined_stat_overflow` for the same "combined gear+tree
//! overflow past the 75% cap" input every other overflow node draws
//! from, read live by `resolve_hit` off the ATTACKER to reduce the
//! DEFENDER's own evasion roll. The same pass also shipped the rest of
//! that round's menu, 8 more nodes: Warlock's Life Tap (a flat one-time
//! HP-for-damage trade at construction, same shape as Reckless Swing -
//! its 3%HP:6%dmg ratio stays constant at every rank, so one magnitude
//! covers both sides instead of needing Reckless Swing's asymmetric
//! rank-matched functions); Warrior's Overwhelming Force (folds a slice
//! of CURRENT combined damage reduction into increased damage at
//! construction - Unbreakable's sibling on the same branch, but off the
//! live stat instead of only its overflow); Monk's whole Inner Focus
//! branch (evade-triggers-heal, firing from the exact same "this hit was
//! evaded" branch in `apply_hit` Fleetfoot's own speed-stack already
//! uses - Meditation/Chi Burst/Serenity all ride the same trigger);
//! Mage's Frost Nova (a temporary evasion debuff applied inside
//! `apply_splash` to whoever splash actually hits); and Ranger's
//! Piercing Shots (rank 3's flat splash-crit-chance bonus is a temporary
//! override on the attacker's own `crit_chance` for just the splash
//! loop, same one-off convention Sanctified Touch's heal-crit bonus
//! already uses - rank 1/2's "splash can crit independently" clause
//! turned out to already be true unconditionally in this sim, since
//! every `apply_hit` call already rolls its own independent crit
//! regardless of caller, so those ranks are effectively banked toward
//! rank 3's real payoff) (193 of 429 nodes now real).
//! A fourteenth pass (2026-08-15) followed up on a live balance concern
//! about Warrior's Titan's Grip - as originally scoped it would have
//! converted a fraction of the character's WHOLE `combat_max_hp`
//! multiplier (gear's uncapped `IncreasedLife` affix included) into
//! increased damage, which has no ceiling and would only get worse as
//! gear tiers climb, unlike Overwhelming Force's identically-shaped
//! conversion off the naturally-capped DamageReduction stat. Shipped
//! instead scoped to just Juggernaut+Colossus's OWN combined tree
//! contribution (the same product Colossus's own line already computes,
//! capped at 48% at 3/3+3/3) - self-limiting regardless of gear power.
//! Tuned up mid-conversation from an initial 5%/10%/15%-per-rank
//! conversion efficiency to 33%/66%/100% (a live balance call, made
//! easier by the input already being capped at 48% either way). Also
//! given its own independent multiplicative layer in
//! `Character::combat_increased_damage` (per a live follow-up request:
//! "all bonuses in the tree should be multiplicative") -
//! `(1+gear)*(1+tree)*(1+titans_grip)*(1+overwhelm)`, NOT summed into
//! `tree_total` alongside every other tree-sourced increased-damage node
//! the way it was first wired - see
//! `Character::titans_grip_increased_damage`'s doc. Overwhelming Force
//! joined it as its own 4th layer in the same follow-up, which also
//! surfaced (and fixed) a real pre-existing bug: Overwhelming Force,
//! Reckless Swing, Death Wish, and Life Tap were ALL only ever being
//! added at `CombatSimUnit` construction time, so they applied correctly
//! in real fights but never showed up in the dashboard's own DPS/
//! Increased Dmg Dealt numbers - a same-day follow-up folded Reckless
//! Swing/Death Wish/Life Tap in as their own multiplicative layers too
//! (same "everything on the tree should be multiplicative" principle,
//! applied consistently), at which point the wrapper method these all
//! lived in became identical to `Character::combat_increased_damage`
//! itself and was deleted - that's now the single shared source of truth
//! both the dashboard and `simulate_battle`'s own construction site read.
//! Same pass also shipped Slayer's whole Vampiric Frenzy temporary-buff
//! branch: Blood Frenzy (a flat refreshed attack-speed buff per dash,
//! reusing Fel Rush's exact shape but gated to the dash itself instead of
//! a kill), Endless Thirst (same dash trigger, temporarily raises - or at
//! rank 3, entirely removes - the leech-per-second cap), and Reaper's
//! Momentum (a kill from FlickerStrike's own direct hit banks bonus
//! targets for the unit's next dash, checked inline in FlickerStrike's
//! own periodic tick rather than the generic on-kill dispatch since it's
//! gated to FlickerStrike specifically) (197 of 429 nodes now real).
//! (NOTE, corrected 2026-08-16: the "Berserker ends up with zero
//! functional nodes" claim that used to sit here was stale by the time it
//! was written - the 6th/8th/12th passes above had already made
//! Berserker's Frenzy branch, Bloodlust skill+specs, and Reckless Swing
//! branch real. An audit that day found the real count was 201/429, not
//! 197, entirely because of this drift between the running total and this
//! closing summary.)
//!
//! A final pass (2026-08-16, "full-coverage release") closed out
//! effectively everything else across all 11 archetypes - every
//! remaining Modifier-tier "extends a real parent's magnitude/duration/
//! target-count" node, plus several genuinely new shared primitives this
//! needed: evade-triggers-free-counter-attack (Rogue's Voidstep/Monk's
//! Counterflow/Druid's Wild Fury), a party-wide temporary-buff broadcast
//! (reused by half a dozen "grants the party X for Ys" nodes), per-stack
//! independent decay (Berserker's Neverending), a real curse expiry +
//! on-expiry detonation event for Warlock's Doom (the sim's `NextEvent`
//! sits alongside Helm/Boots/FlickerStrike/DivineShield/LingeringTick),
//! and a rolling recent-attacker window for Druid's Entangle. Four
//! branches got genuine redesigns per live design calls rather than
//! guessed-at fixes: Rogue's Opportunist (guaranteed-hit + DR-bypass,
//! replacing its original "first-hit crit" text), Slayer's Bloodpact
//! (a real 4s cooldown + per-fight use-stacking, replacing its original
//! flat per-fight-charges model), Cleric's Unbroken Prayer (unlimited
//! chance-gated re-bounce across the whole party), and Slayer's
//! Rot/Withering Touch (wired into `apply_heal`, though it stays inert
//! until some future pass gives any boss/add real self-healing - nothing
//! heals an enemy today).
//!
//! What's still genuinely deferred, and why, falls into a few buckets
//! rather than "not gotten to yet": nodes whose own text was left
//! orphaned by an EARLIER redesign of their parent (Monk's Unbroken
//! trio - lastbastion/risingdefiance/unyieldingspirit - assume the old
//! "+evasion per missing HP" Unbroken that no longer exists; Paladin's
//! Zealotry trio - martyrscall/risingfervor/guardianswrath - assume a
//! below-HP threshold Zealotry's own 2026-08-15 offensive-healing
//! redesign never had); nodes needing a live re-evaluation of a value
//! that's only ever summed once at fight construction (Paladin's
//! Unwavering, Cleric's Unyielding Faith - both "doubles while you're
//! below X% HP" on a party-wide grant that's aggregated once up front);
//! nodes assuming a baseline-only (construction-time) stat can be
//! temporarily doubled mid-fight without new live-vs-baseline plumbing
//! (Mage's Timewarp, Warlock's Demonic Speed); nodes contingent on a
//! curse actually expiring, which is only true for a Warlock who ALSO
//! took Doom (Warlock's Cursed Blood, Virulence); one node whose own
//! "consumes a stack" premise doesn't match how Flowing Strikes stacks
//! actually work (Monk's One Hundred Hands - nothing currently consumes
//! a stack at all); and Rogue's Opportunist branch was a deliberate
//! ground-up redesign rather than a literal implementation of its
//! original text, per a live design call. A number of other nodes
//! ("decays slower", "lasts N seconds longer", "doubled for the first Ns
//! of a fight") are implemented as documented approximations (extending
//! a shared expiry window, a flat construction-time bonus) rather than
//! literal live mechanics, each flagged in its own code comment - a
//! deliberate tradeoff for a release covering this much surface area at
//! once, not an oversight.

use crate::adventure::{Archetype, ArchetypeBonus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassiveTier {
    Skill,
    Specialization,
    Modifier,
}

/// Mirrors `ArchetypeBonus`'s own fields exactly - `Character::passive_bonus`
/// accumulates into a real `ArchetypeBonus` rather than a parallel type, so
/// every `combat_*` getter can add the tree's total in with the same
/// `+= passive_bonus().<field>` shape it already uses for gear+archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassiveStat {
    DamageReduction,
    BlockChance,
    Evasion,
    IntervenePct,
    IncreasedDamage,
    CritChance,
    CritMultiplier,
    Splash,
    HealPowerPct,
    LifeLeechPct,
    AttackSpeed,
    MaxHpPct,
}

impl PassiveStat {
    /// Adds `amount` into this stat's field on `bonus` - the one piece of
    /// `ArchetypeBonus` mutation exposed outside this module, so
    /// `Character::passive_bonus`/`passive_overflow_bonus` (in
    /// `adventure.rs`) never need direct field access.
    pub fn add(self, bonus: &mut ArchetypeBonus, amount: f64) {
        *self.field_mut(bonus) += amount;
    }

    /// Reads this stat's current field value off `bonus` - used to read
    /// back the tree's own pooled raw total for a stat before capping it
    /// (see `Character::passive_overflow_bonus`).
    pub fn get(self, bonus: &ArchetypeBonus) -> f64 {
        match self {
            PassiveStat::DamageReduction => bonus.damage_reduction,
            PassiveStat::BlockChance => bonus.block_chance,
            PassiveStat::Evasion => bonus.evasion,
            PassiveStat::IntervenePct => bonus.intervene_pct,
            PassiveStat::IncreasedDamage => bonus.increased_damage,
            PassiveStat::CritChance => bonus.crit_chance,
            PassiveStat::CritMultiplier => bonus.crit_multiplier,
            PassiveStat::Splash => bonus.splash,
            PassiveStat::HealPowerPct => bonus.heal_power_pct,
            PassiveStat::LifeLeechPct => bonus.life_leech_pct,
            PassiveStat::AttackSpeed => bonus.attack_speed,
            PassiveStat::MaxHpPct => bonus.max_hp_pct,
        }
    }

    fn field_mut(self, bonus: &mut ArchetypeBonus) -> &mut f64 {
        match self {
            PassiveStat::DamageReduction => &mut bonus.damage_reduction,
            PassiveStat::BlockChance => &mut bonus.block_chance,
            PassiveStat::Evasion => &mut bonus.evasion,
            PassiveStat::IntervenePct => &mut bonus.intervene_pct,
            PassiveStat::IncreasedDamage => &mut bonus.increased_damage,
            PassiveStat::CritChance => &mut bonus.crit_chance,
            PassiveStat::CritMultiplier => &mut bonus.crit_multiplier,
            PassiveStat::Splash => &mut bonus.splash,
            PassiveStat::HealPowerPct => &mut bonus.heal_power_pct,
            PassiveStat::LifeLeechPct => &mut bonus.life_leech_pct,
            PassiveStat::AttackSpeed => &mut bonus.attack_speed,
            PassiveStat::MaxHpPct => &mut bonus.max_hp_pct,
        }
    }

    /// Which of the 4 capped-with-overflow stats (see
    /// `Character::combat_damage_reduction`/`combat_block_chance`/
    /// `combat_evasion`/`combat_intervene`) this is, if any - only these
    /// four have a real "overflow past the cap" concept an
    /// `OverflowConversion` node's `input` can draw from.
    /// Which of the 12 pooled stats has a real "overflow past the cap"
    /// concept an `OverflowConversion` node's `input` (or
    /// `Character::combined_stat_overflow`) can draw from, and WHERE that
    /// saturation point sits. Stage 1 (2026-08-24): the caps are
    /// LiveTunables (`evasion_overflow_cap`/`block_overflow_cap`/
    /// `dr_overflow_cap`/`intervene_overflow_cap`) instead of constants -
    /// the value arrives per call from the fight's own tunables snapshot,
    /// never cached. Defaults reproduce the old hardcoded arms exactly.
    ///
    /// CONSISTENCY RULE: `Character::combined_stat_overflow` and the
    /// defensive `combat_*` getters clamp with these same values - if you
    /// add a stat here, thread the same tunable into every place that
    /// clamps it, or conversions and the stat itself will disagree about
    /// where "past the cap" starts.
    pub fn overflow_cap(self, t: &crate::adventure::LiveTunables) -> Option<f64> {
        match self {
            PassiveStat::DamageReduction => Some(t.dr_overflow_cap),
            PassiveStat::BlockChance => Some(t.block_overflow_cap),
            PassiveStat::Evasion => Some(t.evasion_overflow_cap),
            PassiveStat::IntervenePct => Some(t.intervene_overflow_cap),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PassiveEffect {
    /// Adds directly to one of the 12 pooled stats - `at_rank_1` is the
    /// value at rank 1, `per_additional_rank` is added for every rank
    /// past that (so rank 3 = `at_rank_1 + per_additional_rank * 2`),
    /// matching the "X% at rank 1, +Y% per additional rank" shape every
    /// node in the source design uses.
    FlatStat { stat: PassiveStat, at_rank_1: f64, per_additional_rank: f64 },
    /// Converts a fraction of `input`'s OWN overflow (past its cap,
    /// computed from the passive tree's own pooled `input` total only -
    /// see `Character::passive_overflow_bonus`) into `output`, at
    /// `at_rank_1`/`per_additional_rank` efficiency using the same rank
    /// formula as `FlatStat`.
    OverflowConversion { input: PassiveStat, output: PassiveStat, at_rank_1: f64, per_additional_rank: f64 },
    /// A node with bespoke game logic that doesn't reduce to "add to one
    /// of the 12 pooled stats" or "convert overflow" - Slayer's whole kit
    /// (wound stacks, FlickerStrike cooldown, Bloodpact) is built on this.
    /// `at_rank_1`/`per_additional_rank` still follow the exact same
    /// rank-scaling formula as `FlatStat` (`magnitude_at_rank` doesn't
    /// care which variant it's reading), but nothing here consumes the
    /// value generically the way `Character::passive_bonus` does for
    /// `FlatStat` - the bespoke code (see `Character::passive_node_rank`/
    /// `passive_node_magnitude` in adventure.rs, and their call sites in
    /// `simulate_battle`) looks the node up by `key` directly instead.
    Special { at_rank_1: f64, per_additional_rank: f64 },
    /// Like `Special`, but for a node whose real per-rank values are NOT
    /// linear and therefore cannot be expressed as
    /// `at_rank_1 + per_additional_rank * (rank - 1)` at all.
    ///
    /// Added 2026-08-20 (Stage 3 Mage batch) because the linear shape had
    /// become the binding constraint on the whole live-tunable-values
    /// project. **18 of the 31 nodes still awaiting migration are
    /// implemented in `combat.rs` as a `rank >= 2` / `rank >= 3` ladder
    /// with a different constant on each branch** - Absolute Zero's
    /// 0 / 0.50 / 0.65, Arcane Instability's 0.05 / 0.09 / 0.12, Empowered
    /// Bolt's 0 / 0 / 0.20. None of those is linear, so with only
    /// `Special` available their true defaults had nowhere to live and
    /// they could never have been migrated without changing behavior.
    ///
    /// The override store was always per-rank precisely so it could hold
    /// shapes like these (see `adventure::passive_overrides`); this is
    /// the same idea applied to the compiled-in DEFAULT, closing the last
    /// gap between what can be tuned and what can be declared.
    ///
    /// `values` is indexed by effective rank - index 0 is rank 1 - and a
    /// rank past its end reads 0.0, same as an unallocated node. Purely
    /// additive: no existing node uses this variant, so nothing changes
    /// by its introduction.
    SpecialPerRank { values: &'static [f64] },
    /// A real, designed mechanic (proc, stacking buff, conditional,
    /// amplify-a-sibling-node, party-wide grant, etc.) with no
    /// implementation yet - see the module doc. Points invested here are
    /// saved and will activate once a follow-up pass wires in the
    /// mechanic; they are never lost or refunded automatically.
    NotYetImplemented,
}

impl PassiveEffect {
    fn magnitude_at(self, effective_rank: u32) -> f64 {
        if effective_rank == 0 {
            return 0.0;
        }
        match self {
            PassiveEffect::FlatStat { at_rank_1, per_additional_rank, .. }
            | PassiveEffect::OverflowConversion { at_rank_1, per_additional_rank, .. }
            | PassiveEffect::Special { at_rank_1, per_additional_rank } => at_rank_1 + per_additional_rank * (effective_rank - 1) as f64,
            // Index 0 is rank 1. A rank past the table's end reads 0.0
            // rather than panicking or saturating - the same thing an
            // unallocated node reads, and the safe direction if a table
            // is ever shorter than its node's `max_rank`.
            PassiveEffect::SpecialPerRank { values } => values.get(effective_rank as usize - 1).copied().unwrap_or(0.0),
            PassiveEffect::NotYetImplemented => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PassiveNode {
    pub key: &'static str,
    pub tier: PassiveTier,
    pub parent: Option<&'static str>,
    pub max_rank: u32,
    pub unlock_at: Option<u32>,
    pub name: &'static str,
    pub description: &'static str,
    pub effect: PassiveEffect,
}

impl PassiveNode {
    /// The rank actually used for `effect`'s magnitude formula -
    /// Specializations cap their OWN stat growth at rank 3; the 4th point
    /// only unlocks the 3 Modifier children below it (per the source
    /// design's documented behavior - every specialization's own text
    /// tops out describing "at 3/3", never "at 4/4"). Skills and
    /// Modifiers use their raw rank directly (Skills top out at 3
    /// anyway; Modifiers aren't implemented yet regardless).
    pub fn magnitude_at_rank(&self, rank: u32) -> f64 {
        let effective_rank = self.effective_rank(rank);
        // THE live-tunable hook (2026-08-19) - every numeric read of
        // every node in the tree funnels through this one method, so an
        // admin override entering here reaches stat pooling, every
        // `Special` mechanic in combat.rs, and the dashboard's own stat
        // display without any of them knowing it exists. No override
        // stored = the compiled-in value, byte-identically.
        // See `adventure::passive_overrides`.
        crate::adventure::passive_override_for(self.key, effective_rank).unwrap_or_else(|| self.effect.magnitude_at(effective_rank))
    }

    /// `magnitude_at_rank` against an explicit override set instead of
    /// the process-global one - the whole override behavior as a pure
    /// function, so it can be tested without writing to global state
    /// that every other test in this binary shares.
    pub fn magnitude_at_rank_with(&self, rank: u32, overrides: &crate::adventure::PassiveOverrides) -> f64 {
        let effective_rank = self.effective_rank(rank);
        overrides.value_for(self.key, effective_rank).unwrap_or_else(|| self.effect.magnitude_at(effective_rank))
    }

    /// The rank an override is keyed by, and the rank the magnitude
    /// formula reads - one definition, so the live path, the pure path
    /// and the admin page can never disagree about which rank a stored
    /// value belongs to. See `magnitude_at_rank`'s own doc for why a
    /// Specialization floors at 3.
    pub fn effective_rank(&self, rank: u32) -> u32 {
        if matches!(self.tier, PassiveTier::Specialization) {
            rank.min(3)
        } else {
            rank
        }
    }
}

const fn skill(key: &'static str, name: &'static str, description: &'static str, effect: PassiveEffect) -> PassiveNode {
    PassiveNode { key, tier: PassiveTier::Skill, parent: None, max_rank: 3, unlock_at: None, name, description, effect }
}
const fn spec(key: &'static str, parent: &'static str, name: &'static str, description: &'static str, effect: PassiveEffect) -> PassiveNode {
    PassiveNode { key, tier: PassiveTier::Specialization, parent: Some(parent), max_rank: 4, unlock_at: None, name, description, effect }
}
const fn modifier(key: &'static str, parent: &'static str, name: &'static str, description: &'static str) -> PassiveNode {
    PassiveNode {
        key,
        tier: PassiveTier::Modifier,
        parent: Some(parent),
        max_rank: 3,
        unlock_at: Some(4),
        name,
        description,
        effect: PassiveEffect::NotYetImplemented,
    }
}
/// Same shape as `modifier`, for the minority that DO carry a real
/// effect (see the Slayer follow-up pass) rather than `NotYetImplemented`.
const fn modifier_with_effect(key: &'static str, parent: &'static str, name: &'static str, description: &'static str, effect: PassiveEffect) -> PassiveNode {
    PassiveNode { key, tier: PassiveTier::Modifier, parent: Some(parent), max_rank: 3, unlock_at: Some(4), name, description, effect }
}

use PassiveEffect::{FlatStat, OverflowConversion, Special, SpecialPerRank};
use PassiveStat::*;

impl Archetype {
    pub fn passive_nodes(self) -> &'static [PassiveNode] {
        match self {
            Archetype::Commoner => &[],
            Archetype::Warrior => WARRIOR_NODES,
            Archetype::Berserker => BERSERKER_NODES,
            Archetype::Rogue => ROGUE_NODES,
            Archetype::Monk => MONK_NODES,
            Archetype::Paladin => PALADIN_NODES,
            Archetype::Ranger => RANGER_NODES,
            Archetype::Mage => MAGE_NODES,
            Archetype::Warlock => WARLOCK_NODES,
            Archetype::Cleric => CLERIC_NODES,
            Archetype::Druid => DRUID_NODES,
            Archetype::Slayer => SLAYER_NODES,
            Archetype::Elementalist => ELEMENTALIST_NODES,
        }
    }

    /// `passive_nodes()` filtered to just the 3 tier-1 Skills, in a fixed
    /// display order - the tree UI's top row.
    pub fn passive_skills(self) -> impl Iterator<Item = &'static PassiveNode> {
        self.passive_nodes().iter().filter(|n| matches!(n.tier, PassiveTier::Skill))
    }
}

// ---------------------------------------------------------------------
// WARRIOR (root: damage reduction) - real nodes: bulwark, juggernaut,
// unbreakable, fortress, colossus (2026-08-15 flat/overflow follow-up).
// Fortress's "also grants flat damage reduction" is an independent
// FlatStat, no different from a plain flat-bonus modifier despite being
// worded as attached to Unbreakable's overflow conversion. Colossus is
// the one exception this whole pass needed real new (if small) code for:
// its own numbers ("50%... 100% MORE") describe a bonus that's a
// fraction OF JUGGERNAUT'S OWN CURRENT VALUE, not a flat point-addition
// like Fel Haste's analogous "+18%" - `Special`-cased directly in
// `Character::passive_bonus` (reads Juggernaut's magnitude via
// `passive_node_magnitude`), the only node in the whole tree needing
// this cross-node-lookup shape. retaliation/aegis/spikebarrier are proc/
// shield/reflect triggers; vengeance/bloodresolve/laststand modify
// retaliation; momentum is a stacking on-hit buff; overwhelm converts a
// LIVE (not overflow-only) fraction of a stat, which needs new logic
// OverflowConversion can't express. titansgrip (a modifier under
// Colossus) has this same "live fraction" shape and is now real too
// (2026-08-15, a live design conversation) - deliberately scoped to
// JUGGERNAUT+COLOSSUS'S OWN combined tree contribution
// (`passive_node_magnitude("juggernaut") * passive_node_magnitude("colossus")`,
// the same product Colossus's own line above already computes), not the
// character's whole `combat_max_hp` multiplier - that full stat includes
// gear's uncapped `IncreasedLife` affix, which has no ceiling and would
// have made a straight percentage-of-it conversion balloon far past
// Overwhelm's own naturally-bounded-by-a-75%-cap ceiling as gear tiers
// climb. Scoping to just the tree's own (capped at 48% at 3/3 Juggernaut
// + 3/3 Colossus) contribution keeps it self-limiting forever, no matter
// how strong gear gets. Both Titan's Grip and Overwhelm are wired as
// their OWN independent multiplicative layers in
// `Character::combat_increased_damage` (per a live design call: "all
// bonuses in the tree should be multiplicative") - not summed into the
// generic tree-wide `IncreasedDamage` pool alongside Unbreakable's own
// overflow conversion.
// ---------------------------------------------------------------------
static WARRIOR_NODES: &[PassiveNode] = &[
    skill("bulwark", "Bulwark", "Grants a chance to block incoming hits, halving their damage - 8% at rank 1, +6% per additional rank (20% at 3/3).", FlatStat { stat: BlockChance, at_rank_1: 0.08, per_additional_rank: 0.06 }),
    skill("retaliation", "Retaliation", "A hit taken has a chance to trigger an immediate counter-attack - 10% at rank 1, +8% per additional rank (26% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.08 }),
    skill("juggernaut", "Juggernaut", "Increases max HP by 8% per rank (up to 24% at 3/3).", FlatStat { stat: MaxHpPct, at_rank_1: 0.08, per_additional_rank: 0.08 }),
    spec("aegis", "bulwark", "Aegis", "A blocked hit also shields your lowest-HP ally for 20% of the blocked damage, at rank 1 - +10% per rank (40% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.10 }),
    spec("spikebarrier", "bulwark", "Spike Barrier", "A blocked hit reflects 25% of its damage back at the attacker at rank 1 - +15% per rank (55% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.15 }),
    spec("unbreakable", "bulwark", "Unbreakable", "Block chance overflow past the 75% cap converts to increased damage at 50% efficiency per rank - +25% per rank, capped at +10% increased damage per rank (up to +30% at 3/3). Counts your COMBINED gear + tree block chance, not tree investment alone - gear alone can easily push you past 75%.", OverflowConversion { input: BlockChance, output: IncreasedDamage, at_rank_1: 0.5, per_additional_rank: 0.25 }),
    spec("vengeance", "retaliation", "Vengeance", "Retaliation's counter-attacks deal 20% increased damage at rank 1 - +15% per rank (50% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.15 }),
    spec("bloodresolve", "retaliation", "Bloodied Resolve", "Retaliation's counter-attacks heal you for 8% of the damage dealt at rank 1 - +6% per rank (20% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.06 }),
    spec("laststand", "retaliation", "Last Stand", "Below 25% HP, Retaliation's trigger chance is increased by 25% at rank 1 - +15% per rank (55% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.15 }),
    spec("colossus", "juggernaut", "Colossus", "Juggernaut's max HP bonus is increased by 50% at rank 1 - +25% per rank (100% more at 3/3) - e.g. at 3/3 Juggernaut alone (24%) becomes 48% total.", Special { at_rank_1: 0.50, per_additional_rank: 0.25 }),
    spec("momentum", "juggernaut", "Momentum", "Each hit lands a stacking +3% attack speed buff per rank for 4s, max 5 stacks - up to +9% per stack at 3/3 (45% at cap).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Key is "overwhelmingforce", not the plain "overwhelm" its short name
    // would suggest - Berserker's own unrelated "Overwhelm" spec already
    // uses that key (see BERSERKER_NODES below), and node keys must be
    // globally unique across every archetype now that Split Personality
    // lets one character hold allocations from two trees in the same flat
    // lookup (`Character::passive_node_rank`/`passive_node_magnitude`).
    spec("overwhelmingforce", "juggernaut", "Overwhelming Force", "Converts 10% of your current damage reduction into increased damage per rank (up to 30% at 3/3), on top of the existing overflow conversion.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("bastion", "aegis", "Bastion", "Aegis's shield lasts 1 additional second per rank (up to +3s at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("rally", "aegis", "Rally", "Aegis also grants the shielded ally +10% attack speed per rank for its duration (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("ironcircle", "aegis", "Iron Circle", "Aegis shields 1 additional ally per rank (up to all 3 party members at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("thornedhide", "spikebarrier", "Thorned Hide", "Spike Barrier's reflected damage also applies a stacking -5% damage dealt debuff to the attacker per rank (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("retribution", "spikebarrier", "Retribution", "Reflected damage has a 20% chance per rank (up to 60% at 3/3) to crit for double.", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("unyielding", "spikebarrier", "Unyielding", "Spike Barrier can also trigger on unblocked hits, at a 10% chance per rank (up to 30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("fortress", "unbreakable", "Fortress", "Unbreakable's overflow conversion also grants flat damage reduction - 2% per rank (up to 6% at 3/3).", FlatStat { stat: DamageReduction, at_rank_1: 0.02, per_additional_rank: 0.02 }),
    modifier_with_effect("secondskin", "unbreakable", "Second Skin", "Blocking reduces incoming damage by 65% per rank instead of the base 50% (75% at 3/3).", Special { at_rank_1: 0.65, per_additional_rank: 0.05 }),
    modifier_with_effect("stonewall", "unbreakable", "Stonewall", "The first hit each fight is automatically blocked - rank 2 extends this to the first 2 hits, rank 3 to the first 3.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("grudge", "vengeance", "Grudge", "Vengeance gains +5% damage per rank (up to +15% at 3/3) for each prior hit from the SAME attacker this fight, stacking up to 5 times.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Key is "executionersmark", not plain "mark" - Ranger's own unrelated
    // "Hunter's Mark" skill already uses that key (see RANGER_NODES below).
    // Global key uniqueness is required now that Split Personality lets one
    // character hold allocations from two trees in the same flat lookup.
    modifier_with_effect("executionersmark", "vengeance", "Executioner's Mark", "Vengeance's counter-attacks gain +10% crit chance per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Migrated 2026-08-27 (Stage 3): combat.rs hardcoded the 0 / 0.30 /
    // 0.45 HP-fraction ladder off the raw rank, and the old linear
    // Special declared 0.30/0.45/0.60 - one rank out of step with what
    // the game actually used. The real ladder is declared here now and
    // read straight off the magnitude. FRACTION of the attacker's max HP.
    modifier_with_effect("payback", "vengeance", "Payback", "Vengeance's counter always crits when the attacker is below 30% HP, unlocked at rank 2 - rank 3 raises the threshold to 45% HP.", SpecialPerRank { values: &[0.0, 0.30, 0.45] }),
    // Key is "adrenalinesurge", not plain "surge" - Mage's own unrelated
    // "Elemental Surge" skill already uses that key. Global key uniqueness
    // is required now that Split Personality lets one character hold
    // allocations from two trees in the same flat lookup.
    modifier_with_effect("adrenalinesurge", "bloodresolve", "Adrenaline Surge", "A successful Retaliation also grants +8% attack speed per rank for 3s (up to +24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    modifier_with_effect("hardened", "bloodresolve", "Hardened", "Each Retaliation this fight grants a stacking +2% damage reduction per rank (up to 5 stacks, +6%/stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    // "secondwind" itself stays as-is here (Warrior keeps the plain key) -
    // Slayer's own unrelated "Second Wind" modifier is the one renamed to
    // "hemorrhagesecondwind" instead (see SLAYER_NODES below).
    // Migrated 2026-08-27 (Stage 3): same shape as Payback above - the
    // real 0 / 0.50 / 0.65 ladder lived in combat.rs while this linear
    // Special declared 0.50/0.65/0.80. FRACTION of own max HP.
    modifier_with_effect("secondwind", "bloodresolve", "Second Wind", "Retaliation's trigger chance doubles below 50% HP, unlocked at rank 2 - rank 3 raises the threshold to 65% HP.", SpecialPerRank { values: &[0.0, 0.50, 0.65] }),
    modifier_with_effect("defiance", "laststand", "Defiance", "While Last Stand is active, gain +10% damage reduction per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of charges (0/1/2), the
    // ladder combat.rs used to match off the raw rank.
    modifier_with_effect("undyingwill", "laststand", "Undying Will", "Can't be reduced below 1 HP once per fight while under 25% HP, unlocked at rank 2 - rank 3 grants a second use.", SpecialPerRank { values: &[0.0, 1.0, 2.0] }),
    modifier_with_effect("berserkvigor", "laststand", "Berserk Vigor", "While Last Stand is active, also gain +10% increased damage per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect(
        "titansgrip",
        "colossus",
        "Titan's Grip",
        "Converts 33% of Juggernaut and Colossus's own combined max HP bonus into increased damage per rank (up to 100% at 3/3), applied MULTIPLICATIVELY on top of your other damage bonuses rather than adding into them - your gear's own max HP bonuses don't count toward this.",
        Special { at_rank_1: 1.0 / 3.0, per_additional_rank: 1.0 / 3.0 },
    ),
    modifier_with_effect("immovable", "colossus", "Immovable", "Reduces damage taken from enemy critical hits by 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("reserves", "colossus", "Endless Reserves", "Increases healing received from allies by 5% per rank (up to 15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("rampage", "momentum", "Rampage", "Momentum's stacks last 2 additional seconds per rank (up to +6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("avalanche", "momentum", "Avalanche", "Each Momentum stack also adds +2% increased damage per rank (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    modifier_with_effect("unstoppable", "momentum", "Unstoppable", "Momentum gains 1 additional max stack per rank (up to 8 stacks at 3/3, from the base 5).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("grimresolve", "overwhelmingforce", "Grim Resolve", "Overwhelming Force's conversion efficiency is increased by another 5% per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("momentousblow", "overwhelmingforce", "Momentous Blow", "Overwhelming Force also applies to block chance, at half efficiency per rank (up to half at 3/3).", Special { at_rank_1: 0.1667, per_additional_rank: 0.1667 }),
    modifier_with_effect("onslaught", "overwhelmingforce", "Onslaught", "Overwhelming Force's damage bonus also grants +5% attack speed per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
];

// ---------------------------------------------------------------------
// BERSERKER (root: increased damage) - zero real nodes across the
// foundation/flat-overflow passes: every tier-1 skill is either a
// kill-proc (the OLD Frenzy design - see below for its 2026-08-15
// redesign), a stacking on-hit buff (Bloodlust), or a simultaneous
// dual-stat trade (Reckless Swing trades dealt-vs-taken damage at once,
// which PassiveEffect's single-stat shape can't express without breaking
// the intended balance of only implementing the upside). First real node
// (2026-08-15 follow-up): `gambit` - a live missing-HP-scaling crit
// bonus, stepped in 20%-increments, checked directly in
// `roll_attacker_damage` (which already receives the attacker with
// current hp/max_hp - no architecture gap here, unlike the tree's other
// HP-conditional nodes). Its own modifiers (lastlaugh/ragefueled/
// deathdefiant) each need their own extra timer/state on top of the base
// scaling and stay deferred for a future pass.
// Second real branch (2026-08-15, same day): Frenzy's WHOLE
// skill/spec/modifier branch was redesigned from a kill-proc ("A killing
// blow grants an immediate extra attack") to a per-attack multi-strike
// chance, per a live design conversation - the kill-proc shape never had
// a real implementation to begin with (zero functional nodes anywhere in
// this branch until today), so nothing existing gets reinterpreted or
// orphaned. Every KEY below is unchanged from before (bloodfury,
// deathmark, killingspree, etc.) - only names/descriptions/effects
// changed - specifically so this stays a pure content swap with no tree
// topology change and no risk of orphaning a real player's already-spent
// points. See `fire_frenzy` in adventure.rs for the whole mechanic; two
// design notes worth keeping in mind: (1) reflected/executed damage from
// this branch is deliberately NOT leechable (Culling Strike's kill and
// Frenzy's own strikes still count as normal hits and DO leech - only
// truly untyped reflect damage elsewhere in the tree, like Bramblegrowth,
// is deliberately unleechable); (2) nothing here forces or guarantees a
// crit anywhere, on purpose - Overkill shreds DR instead of auto-critting,
// and Culling Strike is a flat-threshold execute instead of a
// guaranteed-crit finisher, per explicit design feedback that the tree
// already has plenty of ways to crit.
// ---------------------------------------------------------------------
static BERSERKER_NODES: &[PassiveNode] = &[
    // Migrated 2026-08-27 (Stage 3): the EXTRA-strike COUNT (1/2/3) was
    // read off the raw rank in combat.rs while this declared 0/0/0. The
    // 10% base trigger rate is a separate aspect and stays a named
    // constant (FRENZY_BASE_STRIKE_CHANCE); Rising Fury tunes it.
    skill("frenzy", "Frenzy", "Each attack has a 10% chance to strike the same target additional times - rank 1 strikes twice total, rank 2 strikes three times total, rank 3 strikes four times total. The 10% rate itself doesn't scale with rank; only the strike count does.", SpecialPerRank { values: &[1.0, 2.0, 3.0] }),
    skill("bloodlust", "Bloodlust", "Each hit you land grants a stacking +4% increased damage buff for 5s, max 5 stacks, at rank 1 - +2% per rank (+8% per stack at 3/3, up to 40% total at cap).", Special { at_rank_1: 0.04, per_additional_rank: 0.02 }),
    skill("reckless", "Reckless Swing", "Deal 15% more damage at rank 1 in exchange for taking 8% more damage - +10%/+5% per additional rank (35% more dealt / 18% more taken at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.10 }),
    spec("bloodfury", "frenzy", "Rising Fury", "Frenzy's trigger chance is increased by 5% per rank (up to +15% at 3/3, on top of the base 10%).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("killingspree", "frenzy", "Berserking", "Frenzy's extra strikes deal 10% increased damage per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("savagemomentum", "frenzy", "Bloodletting", "Each Frenzy strike heals you for 3% of the damage it deals per rank (up to 9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    spec("unendingrage", "bloodlust", "Unending Rage", "Bloodlust's stacks last 2 additional seconds per rank (up to +6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    spec("overwhelm", "bloodlust", "Overwhelm", "Bloodlust stacks also reduce the target's damage reduction by 3% per rank per stack (up to -9% per stack at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    spec("frenziedblows", "bloodlust", "Frenzied Blows", "Bloodlust stacks also grant +2% attack speed per rank per stack (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    spec("deathwish", "reckless", "Death Wish", "Reckless Swing's trade is increased by another 10% dealt / 5% taken per rank (up to +30% dealt / +15% taken at 3/3, stacking with its base).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("vigor", "reckless", "Vigor", "A kill while Reckless Swing is active heals you for 6% of max HP per rank (up to 18% at 3/3).", Special { at_rank_1: 0.06, per_additional_rank: 0.06 }),
    spec("gambit", "reckless", "Berserker's Gambit", "Gain +5% crit chance per rank for every 20% max HP missing (up to +15% per 20% missing at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("deathmark", "bloodfury", "Frenzied Assault", "Rising Fury's bonus is increased by another 5% per rank (up to +15% at 3/3, on top of Rising Fury's own +15%).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Migrated 2026-08-27 (Stage 3): the 0 / 0.50 / 0.65 target-HP
    // FRACTION ladder came off the raw rank in combat.rs.
    modifier_with_effect("bloodscent", "bloodfury", "Blood Scent", "Frenzy's trigger chance doubles against enemies at or below 50% HP, unlocked at rank 2 - rank 3 raises this threshold to 65% HP.", SpecialPerRank { values: &[0.0, 0.50, 0.65] }),
    modifier_with_effect("cullingblow", "bloodfury", "Overkill", "Each Frenzy extra strike reduces the target's damage reduction by 10% per rank for that hit (up to -30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("chainkiller", "killingspree", "Overrun", "Berserking's damage bonus is increased by another 10% per rank (up to +30% at 3/3, on top of Berserking's own +30%).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("massacre", "killingspree", "Culling Strike", "Any Frenzy strike against an enemy at or below 2% HP per rank (up to 6% at 3/3) instead outright kills them.", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    modifier_with_effect("reaperscall", "killingspree", "Chain Frenzy", "A Frenzy trigger has a 10% chance per rank (up to 30% at 3/3) to trigger Frenzy again on the same target - capped at 1 extra chain per point invested here (up to 3 extra chains at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("unbridled", "savagemomentum", "Vitality Surge", "Bloodletting's heal is increased by another 3% of damage dealt per rank (up to +9% at 3/3, on top of Bloodletting's own +9%).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("warpath", "savagemomentum", "Bloodshield", "A Bloodletting heal has a 15% chance per rank (up to 45% at 3/3) to also grant a shield worth 20% of the heal.", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of charges (0/1/2).
    modifier_with_effect("bloodrush", "savagemomentum", "Undying Fury", "Can't be reduced below 1 HP once per fight, unlocked at rank 2 - rank 3 grants a second use.", SpecialPerRank { values: &[0.0, 1.0, 2.0] }),
    modifier_with_effect("furyunleashed", "unendingrage", "Fury Unleashed", "Bloodlust's per-stack damage bonus is increased by 1% per rank (up to +3% per stack at 3/3).", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    modifier_with_effect("neverending", "unendingrage", "Neverending", "Bloodlust's stacks decay one at a time instead of all at once, per rank (fully gradual at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 0.0 }),
    modifier_with_effect("warlord", "unendingrage", "Warlord", "Reaching max Bloodlust stacks grants party members +3% increased damage per rank for 5s (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Migrated 2026-08-27 (Stage 3): the block-chance shred was a
    // hardcoded 1.0 behind an invested check; the declared 1/1/1 table IS
    // that value - a FRACTION of Overwhelm's own shred that carries over.
    // A REAL LADDER since 2026-09-04 (advertised-vs-actual sweep). It was
    // `[1.0, 1.0, 1.0]` - a flat on/off gate whose ranks 2 and 3 bought
    // exactly nothing - while the copy said "by the same amount per rank",
    // which reads as per-rank scaling. Every OTHER flat-ladder node in the
    // tree names its dead rung in its own text; this one did not.
    //
    // WHY 1.65 AND NOT A ROUND 2.0. The multiplier scales Overwhelm's live
    // shred (`stack_shred_bonus`) and is SUBTRACTED from the defender's
    // block chance in `resolve_hit`. Block is clamped only at the roll
    // (`.clamp(0.0, 1.0)`); the `.max(pre_boss_block.min(0.25))` just below
    // the subtraction belongs to the BOSS's own defense-ignore and runs
    // AFTER this, so it cannot hold block up against Shatter. There is
    // therefore no relative floor here and block can be driven to zero.
    //
    // At the Berserker's own end state - Overwhelm 3/3 (0.09/stack),
    // Bloodlust at its 5-stack cap, boss block pinned at BOSS_DEFENSE_CAP
    // 0.75 - the shred is 0.45, so block reaches zero at a multiplier of
    // 0.75/0.45 = 1.667. ANY rank-3 value at or above that is fully
    // absorbed by the clamp in exactly the configuration a maxed Berserker
    // plays in, which is the same defect wearing a new number. 1.65 is the
    // largest value provably not absorbed: it leaves block at 0.008 rather
    // than 0. Below full stacks the saturation point is much higher (2.78x
    // at 3 stacks, 5.0x at Overwhelm 1/3), so a value sized to the narrow
    // case stays meaningful everywhere else.
    //
    // Rank 1 is deliberately unchanged at 1.0: anyone holding one point
    // keeps exactly what they had. The ladder goes up from 1.0, never down
    // to it - a silent nerf to existing allocations is not an acceptable
    // way to fix our own copy (owner ruling).
    //
    // Effect on damage through a blocking target, where a block halves the
    // hit so damage scales as (1 - block/2): 0.625 unshattered, 0.850 at
    // rank 1, 0.929 at rank 2, 0.996 at rank 3 - so the marginal point
    // buys +9.3% then +7.3%, and nothing at all against a target that does
    // not block.
    //
    // `SpecialPerRank`, not `Special`: the deltas are 0.35 then 0.30, so
    // the ladder is not linear and cannot be expressed as
    // `at_rank_1 + per_additional_rank`.
    modifier_with_effect(
        "shatter",
        "overwhelm",
        "Shatter",
        "Overwhelm's damage-reduction shred also applies to the target's block chance - at 100% of the shred at rank 1, 135% at rank 2, 165% at rank 3.",
        SpecialPerRank { values: &[1.0, 1.35, 1.65] },
    ),
    modifier_with_effect("exposed", "overwhelm", "Exposed", "Overwhelm's effect lingers 1 additional second per rank after Bloodlust falls off (up to +3s at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    // Migrated 2026-08-27 (Stage 3): real ladder 0 / 0.50 / 0.65 (a
    // FRACTION of damage reduction) lived in combat.rs; the old linear
    // Special declared 0.50/0.65/0.80, one rank out of step.
    modifier_with_effect("crush", "overwhelm", "Crush", "Overwhelm's shred is doubled against enemies already below 50% damage reduction, unlocked at rank 2 - rank 3 lowers this threshold to 65%.", SpecialPerRank { values: &[0.0, 0.50, 0.65] }),
    modifier_with_effect("hurricane", "frenziedblows", "Hurricane", "Frenzied Blows' attack speed bonus also grants +3% splash per rank per stack (up to +9% per stack at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("tempo", "frenziedblows", "Tempo", "Frenzied Blows grants 1 free stack on entering combat per rank (up to 3 free stacks at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("windfury", "frenziedblows", "Windfury", "Frenzied Blows has a chance to grant 2 stacks instead of 1 on hit - 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("gloryhound", "deathwish", "Glory Hound", "Death Wish's damage bonus is increased by another 5% per rank (up to +15% at 3/3), on top of its base trade.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("recklessabandon", "deathwish", "Reckless Abandon", "Death Wish's extra damage taken is reduced by 5% per rank (up to -15% at 3/3) without losing any of the damage bonus.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of charges - the real ladder
    // is 1/1/2, not the 1/1.5/2 the linear Special produced.
    modifier_with_effect("gloriousdeath", "deathwish", "Glorious Death", "A hit that would kill you while Death Wish is active leaves you at 1 HP instead - once per fight at rank 1, twice at rank 3.", SpecialPerRank { values: &[1.0, 1.0, 2.0] }),
    modifier_with_effect("bloodpump", "vigor", "Blood Pump", "Vigor's heal is increased by 4% max HP per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("secondgale", "vigor", "Second Gale", "A kill while Reckless Swing is active grants immunity to its extra-damage-taken penalty - 2s per rank (up to 6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("vengefulblood", "vigor", "Vengeful Blood", "Vigor's heal also grants a shield worth 50% of the heal per rank (up to 150% at 3/3).", Special { at_rank_1: 0.50, per_additional_rank: 0.50 }),
    modifier_with_effect("lastlaugh", "gambit", "Last Laugh", "Gambit's crit chance bonus gets +15 percentage points while below 25% HP, unlocked at rank 2 - rank 3 also adds +50% crit damage in that state.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("ragefueled", "gambit", "Rage Fueled", "Gambit's unused crit chance (above 80% HP, where missing-HP scaling can't apply) converts to +5% attack speed per rank instead (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Still NotYetImplemented - "persists after healing back above the
    // threshold" needs a live hp%-crossing EVENT (something watching for
    // the moment hp recovers past Gambit's own threshold), and nothing in
    // this event-driven sim currently generates one; every other live-HP%
    // conditional here (Gambit itself included) is instead read fresh at
    // the moment of each hit, with no memory of a past state to persist.
    // Real (2026-08-17) - a genuinely new small primitive (see
    // `apply_heal`'s own hook): freezes Gambit's bonus at its OLD,
    // higher missing-HP bucket for a grace window whenever a heal moves
    // this unit to a lower one, instead of the bonus dropping instantly.
    modifier_with_effect(
        "deathdefiant",
        "gambit",
        "Death Defiant",
        "Gambit's crit bonus persists 3 additional seconds per rank after healing back above its missing-HP threshold (up to 9s at 3/3).",
        // Migrated 2026-08-25 (drift batch): combat.rs used to compute the
        // grace window as `rank * 3000ms`; these declared seconds ARE that
        // real value, and the call site now reads this magnitude (x1000
        // to ms, same shape as warpspeed/bastion).
        Special { at_rank_1: 3.0, per_additional_rank: 3.0 },
    ),
];

// ---------------------------------------------------------------------
// ROGUE (root: crit chance) - real nodes: precision, shadowstep, elusive,
// phantom, duskveil (2026-08-15 flat/overflow follow-up: both are
// independent OverflowConversion nodes drawing from Elusive's same
// Evasion-overflow pool - `passive_overflow_bonus` already sums multiple
// OverflowConversion nodes per input correctly, so "increase Elusive's
// efficiency" and "also convert into a different stat" both just need
// their own node, no new plumbing).
// ---------------------------------------------------------------------
static ROGUE_NODES: &[PassiveNode] = &[
    // Redesigned 2026-08-16 per a live design call, replacing the original
    // "first-hit crit bonus" text entirely: your first N hits each fight
    // (N = this skill's own rank) are guaranteed to land, bypassing both
    // the evasion AND block rolls outright (see `resolve_hit`'s
    // `opportunist_guaranteed` check, adventure.rs) - not a crit-chance
    // buff at all anymore.
    skill("opportunist", "Opportunist", "Your first hit each fight cannot be evaded or blocked - rank 2 extends this to your first 2 hits, rank 3 to your first 3.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    skill("precision", "Deadly Precision", "Increases crit damage dealt by 15% at rank 1 - +10% per rank (35% at 3/3).", FlatStat { stat: CritMultiplier, at_rank_1: 0.15, per_additional_rank: 0.10 }),
    skill("shadowstep", "Shadowstep", "Increases evasion by 8% at rank 1 - +6% per rank (20% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.08, per_additional_rank: 0.06 }),
    // Redesigned alongside `opportunist` - Opportunist's own "final leaf":
    // those same guaranteed hits also ignore a fraction of the target's
    // damage reduction, so a fully-invested Rogue's 3rd guaranteed hit
    // (3/3 Opportunist + 3/3 Ambush) is a hit that cannot be evaded,
    // blocked, OR reduced.
    spec("ambush", "opportunist", "Ambush", "Your guaranteed hits (see Opportunist) also ignore 33% of the target's damage reduction at rank 1 - +33% per rank (up to ~100% at 3/3).", Special { at_rank_1: 0.33, per_additional_rank: 0.33 }),
    spec("cutthroat", "opportunist", "Cutthroat", "Crits against enemies below 25% HP deal 15% more damage per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    spec("vanish", "opportunist", "Vanish", "A crit grants +10% evasion per rank for 3s (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("exploitweakness", "precision", "Exploit Weakness", "Crit multiplier bonus is increased by 10% per rank (up to +30% at 3/3) against enemies below 50% HP.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("twinstrikes", "precision", "Twin Strikes", "A crit has a chance to immediately strike again at 50% damage - 15% at rank 1, +15% per rank (45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of guaranteed-crit charges
    // (0/1/2), matched off the raw rank in combat.rs before this.
    spec("assassinate", "precision", "Assassinate", "Once per fight, your next hit is a guaranteed crit - unlocked at rank 2, rank 3 grants a second use.", SpecialPerRank { values: &[0.0, 1.0, 2.0] }),
    spec("fleetfoot", "shadowstep", "Fleetfoot", "Each successful evade grants a stacking +5% attack speed per rank for 3s, max 3 stacks (up to +15% per stack at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("elusive", "shadowstep", "Elusive", "Evasion overflow past the 75% cap converts to crit chance at 25% efficiency per rank, capped at +10% crit chance per rank (up to +30% at 3/3). Counts your COMBINED gear + tree evasion, not tree investment alone - gear alone can easily push you past 75%.", OverflowConversion { input: Evasion, output: CritChance, at_rank_1: 0.25, per_additional_rank: 0.25 }),
    spec("nightstalker", "shadowstep", "Nightstalker", "Evasion is increased by 10% per rank (up to +30% at 3/3) specifically against boss attacks.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Redesigned 2026-08-16 per a live design call, replacing the
    // original text entirely (which assumed Ambush's pre-redesign crit
    // proc - see the module doc's closing summary for why that no longer
    // applies): a cooldown-gated recharge of Opportunist's own guaranteed-
    // landing effect, independent of the fight-opening budget.
    modifier_with_effect("openingmove", "ambush", "Opening Move", "Adds a cooldown to Opportunist, letting your next hit use Opportunist's and Ambush's bonuses once every 4/3/2 seconds.", Special { at_rank_1: 4.0, per_additional_rank: -1.0 }),
    // Redesigned alongside Opening Move: a guaranteed-landing hit leaves a
    // debuff on the target so the NEXT hit from ANY ally has a chance to
    // also gain Opportunist's/Ambush's bonuses, at 33/66/100% effectiveness.
    modifier_with_effect("coldsteel", "ambush", "Cold Steel", "Leaves a debuff on the target making the next hit from any ally also gain Opportunist's and Ambush's bonuses, at a 33%/66%/100% effectiveness rate.", Special { at_rank_1: 0.33, per_additional_rank: 0.335 }),
    // Redesigned alongside Opening Move: marks any enemy struck by a
    // guaranteed-landing hit to take increased damage from ALL hits for
    // 4s (a genuine multiplicative damage-taken increase, same shared
    // slot Curse of Weakness's own unconditional debuff uses).
    modifier_with_effect("predator", "ambush", "Predator", "Marks any enemy struck by an opportunity attack to take 10%/20%/30% increased damage from all hits for 4 seconds.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Marked for Death - approximated off the target's live post-hit hp%
    // (see the trigger site's own doc) rather than isolating "against
    // THIS specific target" - the guaranteed crits apply to this unit's
    // next hits regardless of target.
    // Migrated 2026-08-27 (Stage 3): a COUNT of marked hits (0/2/3).
    modifier_with_effect("markedfordeath", "cutthroat", "Marked for Death", "Cutthroat also marks the target, causing your next hits to count as crits against it - 2 hits at rank 2, 3 hits at rank 3.", SpecialPerRank { values: &[0.0, 2.0, 3.0] }),
    modifier_with_effect("bloodyknife", "cutthroat", "Bloody Knife", "Cutthroat's damage bonus is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("finalcut", "cutthroat", "Final Cut", "A Cutthroat crit that kills grants +5% attack speed per rank for 3s (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("smokescreen", "vanish", "Smokescreen", "Vanish also grants your lowest-HP ally +5% evasion per rank for its duration (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("fadeaway", "vanish", "Fadeaway", "Vanish's duration is increased by 1s per rank (up to +3s at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("backstab", "vanish", "Backstab", "While Vanish is active, your next hit deals +15% damage per rank (up to +45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Migrated 2026-08-27 (Stage 3): the real threshold ladder in
    // combat.rs was 0.50 / 0.65 / 0.80 (a FRACTION of target max HP),
    // while the linear Special declared 0.65/0.80/0.95.
    modifier_with_effect("vitalstrike", "exploitweakness", "Vital Strike", "Exploit Weakness's threshold is raised to include enemies below 65% HP per rank instead of 50% (up to 80% at 3/3).", SpecialPerRank { values: &[0.50, 0.65, 0.80] }),
    modifier_with_effect("weakpoint", "exploitweakness", "Weak Point", "Exploit Weakness also grants +5% crit chance per rank (up to +15% at 3/3) against affected enemies.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Migrated 2026-08-27 (Stage 3): a MULTIPLIER on Exploit Weakness's
    // whole magnitude - 1x until rank 3, 2x at rank 3 (the "doubles"
    // read that was hardcoded at the call site).
    modifier_with_effect("surgicalstrike", "exploitweakness", "Surgical Strike", "Exploit Weakness's bonus crit multiplier also applies to splash damage, unlocked at rank 2 - rank 3 doubles the splash portion specifically.", SpecialPerRank { values: &[1.0, 1.0, 2.0] }),
    modifier_with_effect("echo", "twinstrikes", "Echo", "Twin Strikes' second-hit damage is increased from 50% per rank (up to 95% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("flurry", "twinstrikes", "Flurry", "Twin Strikes' trigger chance is increased by 10% per rank (up to +30% at 3/3, 75% total at 3/3 combined with its base).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Double Tap - now a real bounded chain (2026-08-16, same treatment as
    // Mage's Finite Loop) instead of the old flat chance bump. Reuses the
    // base Twin Strikes trigger chance for every repeat roll (no separate
    // chance layer) and caps at `doubletap_max_repeats` (3/6/9) - see
    // `combat.rs`'s construction-site doc.
    // Migrated 2026-08-27 (Stage 3): the repeat CAP was `rank * 3` in
    // combat.rs (3/6/9, exactly what the text promises) while this
    // declared a stale 10%/20%/30% chance nothing read. Same shape as
    // Mage's Finite Loop, which already declares 3/6/9 here.
    modifier_with_effect("doubletap", "twinstrikes", "Double Tap", "Twin Strikes' second hit can itself crit and re-trigger Twin Strikes again, at the same chance as the first trigger - can repeat up to 3/6/9 times (rank 1/2/3) before the chain ends.", SpecialPerRank { values: &[3.0, 6.0, 9.0] }),
    modifier_with_effect("coupdegrace", "assassinate", "Coup de Grace", "Assassinate's guaranteed crit also deals +30% crit damage per rank (up to +90% at 3/3).", Special { at_rank_1: 0.30, per_additional_rank: 0.30 }),
    modifier_with_effect("premeditation", "assassinate", "Premeditation", "Assassinate refunds its use if the triggering hit doesn't kill, at a 20% chance per rank (up to 60% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("silentblade", "assassinate", "Silent Blade", "Assassinate's guaranteed crit also grants +20% evasion per rank for 3s afterward (up to +60% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("windrunner", "fleetfoot", "Windrunner", "Fleetfoot's max stacks are increased by 1 per rank (up to 6 stacks at 3/3, from the base 3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("silentsteps", "fleetfoot", "Silent Steps", "Fleetfoot's stacks also grant +3% evasion per rank per stack (up to +9% per stack at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of free stacks (0/1/2).
    modifier_with_effect("quickdraw", "fleetfoot", "Quickdraw", "Fleetfoot grants free stacks on entering combat - 1 at rank 2, 2 at rank 3.", SpecialPerRank { values: &[0.0, 1.0, 2.0] }),
    modifier_with_effect("phantom", "elusive", "Phantom", "A second, independent conversion channel off the same evasion overflow Elusive draws from - crit chance at 10% efficiency per rank, capped at +10% crit chance per rank (up to +30% at 3/3, stacking with Elusive's own cap for up to +60% total).", OverflowConversion { input: Evasion, output: CritChance, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("duskveil", "elusive", "Duskveil", "Elusive also converts overflow into attack speed, at 25% efficiency per rank, capped at +10% attack speed per rank (up to +30% at 3/3).", OverflowConversion { input: Evasion, output: AttackSpeed, at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("voidstep", "elusive", "Voidstep", "An evaded hit has a chance to trigger an immediate free attack against the attacker - 10% per rank (up to 30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("huntersinstinct", "nightstalker", "Hunter's Instinct", "Nightstalker also grants +5% crit chance per rank (up to +15% at 3/3) against bosses.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("apexpredator", "nightstalker", "Apex Predator", "Nightstalker's evasion bonus vs bosses is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("silentkiller", "nightstalker", "Silent Killer", "The first hit landed against a boss each fight deals +25% damage per rank (up to +75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
];

// ---------------------------------------------------------------------
// MONK (root: evasion) - real nodes: ironbody, stonefist (both re-themed
// to evasion this session, freeing damage reduction as Warrior's
// exclusive tree identity), graniteskin, earthenwill (2026-08-15 flat/
// overflow follow-up - same "independent OverflowConversion off Stone
// Fist's Evasion-overflow pool" pattern as Rogue's Phantom/Duskveil).
// ---------------------------------------------------------------------
static MONK_NODES: &[PassiveNode] = &[
    skill("flowingstrikes", "Flowing Strikes", "Each consecutive hit on the same target grants a stacking +3% attack speed for 4s, max 5 stacks, at rank 1 - +1% per rank (+5% per stack at 3/3, 25% at cap).", Special { at_rank_1: 0.03, per_additional_rank: 0.01 }),
    skill("innerfocus", "Inner Focus", "Successfully evading a hit heals you for 3% of max HP at rank 1 - +2% per rank (7% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.02 }),
    skill("ironbody", "Iron Body", "Increases evasion by 6% at rank 1 - +4% per rank (14% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.06, per_additional_rank: 0.04 }),
    // Swapped with Hundred Fists (2026-08-18) - this is the only node that
    // bypasses `add_flowing_stack`'s target-match gate ("even against a new
    // target"), and live logs showed the gate pinning a Monk's stacks at 1
    // for a whole multi-boss fight (consecutive same-target hits: 2.1%).
    // Promoting the counterplay to the spec slot and demoting the cap
    // increase makes the fix reachable before the payoff it enables.
    spec("onehundredhands", "flowingstrikes", "Flow like Water", "A crit from Pressure Point refreshes Flowing Strikes and grants +1 bonus stack per rank (up to 3 at 3/3), even against a new target.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    spec("pressurepoint", "flowingstrikes", "Pressure Point", "Flowing Strikes' stacks also grant +2% crit chance per rank per stack (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    // Migrated 2026-08-27 (Stage 3): the rank-3 "+2s" was a hardcoded
    // 2_000ms at the call site. Declared in SECONDS here (x1000 at the
    // read), same units convention as unbrokenchain/unendingcycle, which
    // add to the very same window.
    spec("relentlessassault", "flowingstrikes", "Relentless Assault", "Landing a hit while at max Flowing Strikes stacks refreshes their duration, unlocked at rank 2 - rank 3 extends the base duration by 2s.", SpecialPerRank { values: &[0.0, 0.0, 2.0] }),
    spec("meditation", "innerfocus", "Meditation", "Inner Focus's heal is increased by 1% max HP per rank for every 10% evasion you have (up to +3% per 10% at 3/3).", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    spec("chiburst", "innerfocus", "Chi Burst", "Inner Focus also heals your lowest-HP ally for 50% of the amount per rank (up to 150% at 3/3).", Special { at_rank_1: 0.50, per_additional_rank: 0.50 }),
    spec("serenity", "innerfocus", "Serenity", "Evading a hit also grants +5% damage reduction per rank for 3s (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("stonefist", "ironbody", "Stone Fist", "Evasion overflow past the 75% cap converts to increased damage at 50% efficiency per rank, capped at +10% increased damage per rank (up to +30% at 3/3). Counts your COMBINED gear + tree evasion, not tree investment alone - gear alone can easily push you past 75%.", OverflowConversion { input: Evasion, output: IncreasedDamage, at_rank_1: 0.5, per_additional_rank: 0.25 }),
    spec("unbroken", "ironbody", "Unbroken", "Evasion overflow past the 75% cap converts into ignoring enemies' own evasion, at 10% efficiency per rank (up to 30% at 3/3). Counts your COMBINED gear + tree evasion, not tree investment alone - gear alone can easily push you past 75%.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("templeguardian", "ironbody", "Temple Guardian", "Iron Body also grants your lowest-HP ally +5% evasion per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("windwalker", "flowingstrikes", "Windwalker", "Flowing Strikes' per-stack attack speed bonus is increased by 1% per rank (up to +3% per stack at 3/3).", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    // Unbroken Chain - "persists through a missed hit" translated to more
    // real time before the streak lapses (see `flowing_duration_ms`'s
    // construction-site doc) - the closest equivalent this event-driven
    // sim's target-switch/window-expiry reset model supports.
    modifier_with_effect("unbrokenchain", "flowingstrikes", "Unbroken Chain", "Flowing Strikes' stacks persist through 1 additional missed hit per rank (up to 3 at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("risingstorm", "flowingstrikes", "Rising Storm", "Reaching max Flowing Strikes stacks grants a burst of +10% increased damage per rank for 3s (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("nervestrike", "pressurepoint", "Nerve Strike", "Pressure Point's crits also deal +10% crit damage per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("vitalpoints", "pressurepoint", "Vital Points", "Pressure Point's stacks also reduce the target's damage reduction by 2% per rank per stack (up to -6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    // Swapped down from its old spec slot under Flowing Strikes
    // (2026-08-18) - see Flow like Water's own note above. Its magnitude
    // is unchanged at the modifier tier: rank 3 still gives +6 max stacks
    // (5 base -> 11), exactly what the description below already says.
    modifier_with_effect(
        "hundredfists",
        "pressurepoint",
        "Hundred Fists",
        "Flowing Strikes' max stacks are increased by 2 per rank (up to 11 stacks at 3/3, from the base 5).",
        Special { at_rank_1: 2.0, per_additional_rank: 2.0 },
    ),
    // The three Chakras follow the spec SLOT, not the node that used to
    // occupy it (2026-08-18 swap) - a modifier's parent must be a
    // Specialization, and Hundred Fists is a modifier now.
    modifier_with_effect(
        "chakraofmany",
        "onehundredhands",
        "Chakra of Many",
        "Summons a spectral clone that attacks alongside you, dealing 10% of your damage per rank (up to 30% at 3/3) - immune to damage itself, and vanishes the instant you fall.",
        Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    modifier_with_effect(
        "chakraoflight",
        "onehundredhands",
        "Chakra of Light",
        "Each hit also triggers the lightning damage debuff (increased damage taken) on the target, worth 10% of your own increased damage per rank (up to 30% at 3/3) - e.g. 3000% increased damage at rank 3 triggers 900%, or 9 stacks of the debuff.",
        Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    modifier_with_effect(
        "chakraoflife",
        "onehundredhands",
        "Chakra of Life",
        "A hit that would kill you instead makes you immune to all damage for 1s per rank (up to 3s at 3/3), during which you keep fighting - the instant it ends, you die.",
        // Migrated 2026-08-27 (Stage 3): combat.rs built the window as
        // `rank * 1000ms`; this already-correct table is those SECONDS,
        // now read off the magnitude (x1000 at the call site).
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect("eternalflow", "relentlessassault", "Eternal Flow", "Relentless Assault's refresh also adds 1 additional stack per rank (up to 3 extra at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("unendingcycle", "relentlessassault", "Unending Cycle", "Relentless Assault's extended duration is increased by another 1s per rank (up to +3s at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("stormfront", "relentlessassault", "Stormfront", "While at max Flowing Strikes stacks, gain +5% splash per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("innerpeace", "meditation", "Inner Peace", "Meditation's heal-per-evasion scaling is increased by another 0.5% max HP per rank (up to +1.5% per 10% evasion at 3/3).", Special { at_rank_1: 0.005, per_additional_rank: 0.005 }),
    modifier_with_effect("risingtide", "meditation", "Rising Tide", "Meditation also grants +3% healing power per rank for 3s after triggering (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("clarity", "meditation", "Clarity", "Meditation also triggers on blocked hits, unlocked at rank 2 - rank 3 also triggers it on hits reduced by Iron Body.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    // Key is "chiburstsanctuary", not plain "sanctuary" - Cleric's own
    // unrelated "Sanctuary" spec already uses that key. Global key
    // uniqueness is required now that Split Personality lets one character
    // hold allocations from two trees in the same flat lookup.
    modifier_with_effect("chiburstsanctuary", "chiburst", "Sanctuary", "Chi Burst's ally heal is increased by another 25% per rank (up to +75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("harmonize", "chiburst", "Harmonize", "Chi Burst also grants the healed ally +5% damage reduction per rank for 3s (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("widecircle", "chiburst", "Wide Circle", "Chi Burst heals 1 additional ally per rank (up to all 3 party members at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("unshakable", "serenity", "Unshakable", "Serenity's damage reduction bonus duration is increased by 1s per rank (up to +3s at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    // Stillwater - already true unconditionally: Serenity's trigger has
    // no chance-gate at all today, so it already fires on EVERY evade,
    // guaranteed, from the first one onward - same "banked toward a later
    // rank's real payoff" precedent Piercing Shots' own rank 1/2 already
    // established (see passive_tree.rs's own module doc).
    modifier_with_effect("stillwater", "serenity", "Stillwater", "Serenity triggers guaranteed on your first evade each fight - rank 2 extends this to your 2nd evade, rank 3 to your 3rd.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("unmovable", "serenity", "Unmovable", "Serenity's damage reduction bonus is increased by another 5% per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("graniteskin", "stonefist", "Granite Skin", "A second, independent conversion channel off the same evasion overflow Stone Fist draws from - increased damage at 15% efficiency per rank, capped at +10% increased damage per rank (up to +30% at 3/3, stacking with Stone Fist's own cap for up to +60% total).", OverflowConversion { input: Evasion, output: IncreasedDamage, at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("earthenwill", "stonefist", "Earthen Will", "Stone Fist also converts a portion of overflow into max HP, at 25% efficiency per rank, capped at +10% max HP per rank (up to +30% at 3/3).", OverflowConversion { input: Evasion, output: MaxHpPct, at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("counterflow", "stonefist", "Counterflow", "An evaded hit has a chance to trigger a free counter-attack - 10% per rank (up to 30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // RESOLVED - this comment is kept, corrected, rather than deleted,
    // because the history explains the names. It used to read "All 3 of
    // these still NotYetImplemented - a genuine text/code mismatch": they
    // once described Unbroken as "+evasion per 20% missing HP", and a
    // live design call (see the module doc's 13th-pass entry) redesigned
    // Unbroken ENTIRELY into an evasion-IGNORE-on-attack mechanic
    // (`combat_unbroken_ignore_evasion_pct`), orphaning all three.
    //
    // **That mismatch is closed as of 2026-09-03 (verified by the
    // advertised-vs-actual sweep).** All three now carry declared
    // per-rank values, live consumers, and copy that matches:
    // `unbroken` at character.rs's `combat_unbroken_ignore_evasion_pct`,
    // `lastbastion` at its `..._dr_pct` twin, and `risingdefiance`
    // through the shared overflow-conversion cap list. Nothing here is
    // `NotYetImplemented` any more; `sacredoverflow` (Paladin) is the
    // last node in the whole tree that still is.
    // Renamed "Crippling Grip" (2026-08-17) - see risingdefiance's own
    // comment above for why the old text is orphaned by Unbroken's
    // redesign. NOT an `OverflowConversion` like Overgrown Reach/Earthen
    // Will - those grant a SELF stat auto-summed generically; this instead
    // debuffs whoever the Monk hits (see `Character::combat_crippling_grip_dr_pct`),
    // same custom-mechanic shape its own parent `unbroken` already uses.
    modifier_with_effect(
        "lastbastion",
        "unbroken",
        "Crippling Grip",
        "Unbroken's evasion-ignore also reduces the target's damage reduction, at half efficiency per rank (up to -15% at 3/3).",
        Special { at_rank_1: 0.05, per_additional_rank: 0.05 },
    ),
    // Renamed "Overgrown Reach" (2026-08-17) - the old text assumed a
    // pre-redesign Unbroken that granted "+evasion per missing HP", which
    // no longer exists (Unbroken was redesigned into an evasion-ignore
    // conversion - see `unbroken`'s own doc). Re-anchored as a second,
    // independent conversion channel off that SAME overflow pool, exactly
    // the "graniteskin"/"earthenwill" pattern Stone Fist's own overflow
    // already establishes one spec over.
    modifier_with_effect(
        "risingdefiance",
        "unbroken",
        "Overgrown Reach",
        "A second, independent conversion channel off the same evasion overflow Unbroken draws from - increased damage at 15% efficiency per rank, capped at +10% increased damage per rank (up to +30% at 3/3).",
        OverflowConversion { input: Evasion, output: IncreasedDamage, at_rank_1: 0.15, per_additional_rank: 0.15 },
    ),
    // Renamed "Last Stand" (2026-08-17) - kept the original text's own
    // numbers verbatim, they scale fine onto Unbroken's real evasion-
    // ignore mechanic. Hard-capped at 75% total in code so this can never
    // approach a guaranteed evasion-ignore even at 3/3 stacked with
    // Overgrown Reach/Earthen Will off the same overflow pool.
    modifier_with_effect(
        "unyieldingspirit",
        "unbroken",
        "Last Stand",
        "Below 25% HP, Unbroken's evasion-ignore is doubled (capped at 75% total) - per rank this activation threshold instead raises to 35%/45%/55% HP (55% at 3/3).",
        // Migrated 2026-08-27 (Stage 3): the threshold was computed as
        // `0.25 + 0.10 * rank` at the call site. These are those exact
        // values as a FRACTION of own max HP.
        SpecialPerRank { values: &[0.35, 0.45, 0.55] },
    ),
    modifier_with_effect("sharedstrength", "templeguardian", "Shared Strength", "Temple Guardian protects 1 additional ally per rank (up to all 3 party members at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("ironwill", "templeguardian", "Iron Will", "Temple Guardian's bonus is increased by another 5% per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Key is "templeguardianspirit", not plain "guardianspirit" - Cleric's
    // own unrelated "Guardian Spirit" spec already uses that key. Global
    // key uniqueness is required now that Split Personality lets one
    // character hold allocations from two trees in the same flat lookup.
    modifier_with_effect("templeguardianspirit", "templeguardian", "Guardian Spirit", "Temple Guardian also heals the protected ally for 2% max HP per rank, once every 5s (up to 6% at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
];

// ---------------------------------------------------------------------
// PALADIN (root: intervene) - real nodes: oath, aegisward,
// sanctifiedarmor (2026-08-15 flat/overflow follow-up - independent
// OverflowConversion off Aegis Ward's Intervene-overflow pool, same
// pattern as Rogue's Phantom). Also real (2026-08-15 second follow-up,
// same day): shield (Divine Shield itself - a periodic self-cast, third
// instance of the Helm/Boots clock pattern - see
// `CombatSimUnit::next_divine_shield_at_ms`), bulwarkoflight/graceperiod/
// radiantbarrier (amplify Divine Shield's amount/cooldown/DR-while-
// shielded - radiantbarrier reuses the SAME generic field Cleric's
// Balanced Faith does), and consecration/widerblessing/sharedlight
// (Divine Shield's party-wide shield + amplifiers). Also real (2026-08-15
// third follow-up, same day): retributionaura + holyvengeance - a new
// shared shield-absorb-reflect primitive (`shield_reflect_pct`/
// `apply_reflect_damage` in adventure.rs) also covering Cleric's Sacred
// Barrier and Slayer's Guardian's Blood (see their own trees' doc
// comments). Retribution Aura specifically requires the shield to FULLY
// absorb the hit (a partial absorption reflects nothing) - the other two
// don't have that restriction. Purify/Last Judgment (a damage-dealt
// debuff and a turn-skip proc) and Sacred Overflow still need their own
// new mechanisms and stay deferred, along with Druid's Bramblegrowth
// family - a DIFFERENT reflect, off Thorned Barrier's damage REDUCTION
// rather than a shield absorbing, so it doesn't share this primitive.
// ---------------------------------------------------------------------
static PALADIN_NODES: &[PassiveNode] = &[
    skill("oath", "Guardian's Oath", "Increases intervene by 4% at rank 1 - +3% per rank (10% at 3/3).", FlatStat { stat: IntervenePct, at_rank_1: 0.04, per_additional_rank: 0.03 }),
    skill("shield", "Divine Shield", "Every 8s, shields your lowest-HP ally for a flat 10% of your max HP. Each rank instead reduces the cooldown by 15% - up to -45% at 3/3 (~4.4s).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    skill("smite", "Radiant Smite", "Every hit also heals up to 2 nearby allies (more with Splash) for 10% of your max HP at rank 1 - +10% per rank (30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("aegisward", "oath", "Aegis Ward", "Intervene overflow past the 50% cap converts to damage reduction at 50% efficiency per rank, capped at +10% damage reduction per rank (up to +30% at 3/3). Counts your COMBINED gear + tree intervene, not tree investment alone - gear alone can easily push you past 50%.", OverflowConversion { input: IntervenePct, output: DamageReduction, at_rank_1: 0.5, per_additional_rank: 0.25 }),
    spec("vowofprotection", "oath", "Vow of Protection", "Guardian's Oath also grants the whole party +3% damage reduction per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    spec("unbreakablefaith", "oath", "Unbreakable Faith", "Damage redirected by intervene heals you for 5% of the redirected amount per rank (up to 15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("bulwarkoflight", "shield", "Bulwark of Light", "Divine Shield's amount is increased by another 10% max HP per rank (up to +30% at 3/3, on top of the base 10%).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("consecration", "shield", "Consecration", "Divine Shield also grants a smaller shield (40% value) to the rest of the party, +10% per rank (70% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("retributionaura", "shield", "Retribution Aura", "When Divine Shield fully absorbs a hit, reflect 20% of the absorbed damage per rank (up to 60% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    spec("zealotry", "smite", "Zealotry", "Radiant Smite heals for another 5% of your max HP per rank (up to +15% at 3/3) and reaches 1 additional target.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("holyfire", "smite", "Holy Fire", "Your total healing done each hit also deals damage to every enemy, equal to 5% of it at rank 1 - +5% per rank (15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("judgment", "smite", "Judgment", "Radiant Smite heals for another 10% of your max HP per rank (up to +30% at 3/3) whenever the enemy you're hitting is below 50% HP.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("sanctifiedarmor", "aegisward", "Sanctified Armor", "A second, independent conversion channel off the same intervene overflow Aegis Ward draws from - damage reduction at 15% efficiency per rank, capped at +10% damage reduction per rank (up to +30% at 3/3, stacking with Aegis Ward's own cap for up to +60% total).", OverflowConversion { input: IntervenePct, output: DamageReduction, at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("bondeddevotion", "aegisward", "Bonded Devotion", "Aegis Ward's conversion also grants the intervened ally +5% damage reduction per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("steadfast", "aegisward", "Steadfast", "Aegis Ward's damage reduction bonus persists for 2s per rank after intervene stops triggering (up to +6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("beaconoflight", "vowofprotection", "Beacon of Light", "Vow of Protection's bonus is increased by another 3% per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Approximated as a flat party DR bonus (see the construction-site
    // doc) rather than a genuine boss-specific channel.
    modifier_with_effect("hallowedground", "vowofprotection", "Hallowed Ground", "Vow of Protection also reduces damage taken from bosses specifically by another 3% per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Still NotYetImplemented - Vow of Protection's party grant is summed
    // once at fight construction (see `party_damage_reduction_pct`'s
    // doc), not re-evaluated live, so "doubles while you're below 50% HP"
    // has no live moment to check against without new architecture.
    // Real (2026-08-17) - `party_damage_reduction_pct` used to be summed
    // once at fight construction with no live moment to re-check a "below
    // 50% HP" condition against; now broadcast live via a dedicated
    // `temp_party_damage_reduction_bonus` field (see combat.rs), refreshed
    // every hit the low-HP Paladin lands. Shared mechanic with Cleric's
    // Unyielding Faith (identical text/effect one archetype over).
    modifier_with_effect(
        "unwavering",
        "vowofprotection",
        "Unwavering",
        "Vow of Protection's bonus doubles for the party while you are below 50% HP, unlocked at rank 2 - rank 3 lowers this threshold to 65%.",
        // Migrated 2026-08-25 (drift batch): the 0 / 0.50 / 0.65 ladder
        // used to be hardcoded in combat.rs off the raw rank; now declared
        // here and read directly (same shape as Mage's absolutezero).
        // Rank 1's row is 0.0 because rank 1 alone genuinely does nothing.
        SpecialPerRank { values: &[0.0, 0.50, 0.65] },
    ),
    modifier_with_effect("martyrsblessing", "unbreakablefaith", "Martyr's Blessing", "Unbreakable Faith's self-heal is increased by another 5% per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("graciousburden", "unbreakablefaith", "Gracious Burden", "Unbreakable Faith also heals the ally whose damage you redirected, for 5% of the redirected amount per rank (up to 15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("eternalvow", "unbreakablefaith", "Eternal Vow", "Unbreakable Faith's heal has a chance to also fully shield you for the same amount - 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("radiantbarrier", "bulwarkoflight", "Radiant Barrier", "Bulwark of Light's shield also grants +5% damage reduction per rank while active (up to +15% at 3/3) - same mechanism as Cleric's Balanced Faith (any active shield, not specifically a Bulwark-of-Light one).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("graceperiod", "bulwarkoflight", "Grace Period", "Bulwark of Light's cooldown is reduced by another 10% per rank (up to -30% at 3/3), stacking with Divine Shield's own tier-1 reduction.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Still NotYetImplemented - shields expire lazily (checked at the next
    // read site, not an active event - same category of gap Doom's curse
    // detonation needed real scheduling infrastructure to solve). Building
    // an equivalent "on shield expiry" event just for this one node is out
    // of scope for this pass.
    modifier("sacredoverflow", "bulwarkoflight", "Sacred Overflow", "Unused Bulwark of Light shield value converts to a heal at 50% efficiency per rank (up to 150% at 3/3) when it expires."),
    modifier_with_effect("widerblessing", "consecration", "Wider Blessing", "Consecration's value is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("communion", "consecration", "Communion", "Consecration also grants the party +5% healing power per rank for its duration (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("sharedlight", "consecration", "Shared Light", "Consecration's party shield lasts 2 additional seconds per rank (up to +6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("holyvengeance", "retributionaura", "Holy Vengeance", "Retribution Aura's reflect is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("purify", "retributionaura", "Purify", "Retribution Aura also reduces the attacker's damage dealt by 5% per rank for 3s (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("lastjudgment", "retributionaura", "Last Judgment", "A fully-reflected hit has a chance per rank to skip the attacker's next action - 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // All 3 still NotYetImplemented - a genuine text/code mismatch: they
    // all assume Zealotry has a "qualifying" HP-threshold condition (a
    // below-X%-HP gate to raise, "per qualifying ally", "under the same
    // conditions"), but Zealotry's OWN spec text (see the 2026-08-15
    // Radiant Smite redesign around offensive healing) has no threshold
    // at all - it's an unconditional heal/target-count bonus. Flagged
    // rather than guessed at, same as Monk's orphaned Unbroken trio.
    // Zealotry trio replaced (2026-08-17) - Radiant Smite's real heal
    // (`smite_zealotry_bonus_pct`) is applied UNCONDITIONALLY, with no
    // HP-threshold "qualifying" gate at all - the old text for all three
    // assumed a gate that was never actually built. Re-anchored to the
    // real per-target heal loop in `apply_radiant_smite_heal`.
    modifier_with_effect(
        "martyrscall",
        "zealotry",
        "Desperate Grace",
        "Zealotry's heal is increased by another 10% per rank to any target below 50% HP.",
        Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    modifier_with_effect(
        "risingfervor",
        "zealotry",
        "United Front",
        "Zealotry's heal is increased by another 2% per rank per ally this cast actually reaches (up to +6%/rank at 3/3).",
        Special { at_rank_1: 0.02, per_additional_rank: 0.02 },
    ),
    modifier_with_effect(
        "guardianswrath",
        "zealotry",
        "Zealous Charge",
        "Healing an ally below 50% HP with Radiant Smite grants +5% attack speed per rank for 3s.",
        Special { at_rank_1: 0.05, per_additional_rank: 0.05 },
    ),
    // Key is "holyfirewildfire", not plain "wildfire" - Mage's own
    // unrelated "Wildfire" spec (under Elemental Surge) already uses that
    // key. Global key uniqueness is required now that Split Personality
    // lets one character hold allocations from two trees in the same flat
    // lookup.
    modifier_with_effect("holyfirewildfire", "holyfire", "Wildfire", "Holy Fire's conversion rate is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("purgingflame", "holyfire", "Purging Flame", "Holy Fire's damage also reduces the struck enemy's healing received by 10% per rank (up to -30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Reworked 2026-08-21 (owner ruling, Paladin Holy Fire fix order,
    // Q1). Holy Fire already strikes EVERY alive enemy each time it
    // fires (see `apply_holy_fire_damage`'s doc), so the old text - "1
    // additional random enemy per rank" - had nothing left to add, same
    // "already-exceeds-its-own-text" tension Piercing Shots' rank 1/2
    // once had. Now reads as a straight damage-contribution increase,
    // folded into `smite_holyfire_dmg_pct` as a third multiplicative
    // factor alongside Wildfire (see that field's construction site).
    modifier_with_effect("risingblaze", "holyfire", "Rising Blaze", "Holy Fire's damage contribution is increased by 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Final Judgment's own text assumes a 20% base threshold - see the
    // construction-site doc for why this is applied as a +10/15/20%
    // DELTA on Judgment's real 50% base instead of taking its absolute
    // numbers literally.
    modifier_with_effect("finaljudgment", "judgment", "Final Judgment", "Judgment's threshold is raised to 30% HP per rank instead of 20% (up to 40% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.05 }),
    modifier_with_effect("executionersblessing", "judgment", "Executioner's Blessing", "A Judgment kill heals you for 8% max HP per rank (up to 24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    // Text updated 2026-08-21 (Paladin Holy Fire fix order, Q2). Used to
    // read as splashing Judgment's own damage, but the code never
    // actually sourced Judgment's real hit value - it re-rolled an
    // unrelated fresh attack roll per target. Now genuinely IS half of
    // Radiant Smite's own Holy Fire damage contribution, computed once
    // and delivered flat (see the Wrath capture-then-apply block next to
    // `apply_holy_fire_damage`'s call site) - the tooltip now describes
    // what the code does rather than what it happened to say before.
    modifier_with_effect("wrathoftheheavens", "judgment", "Wrath of the Heavens", "On a Judgment kill, a chance per rank to also deal half of Radiant Smite's Holy Fire damage to nearby enemies - 20% per rank (up to 60% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
];

// ---------------------------------------------------------------------
// RANGER (root: splash) - real nodes: multishot, fleet, evasivemaneuvers,
// explosivetips, rapidfire, overcharge, windsprint, quickshot, swiftwind,
// lightfoot (2026-08-15 flat/overflow follow-up - all independent
// FlatStat/OverflowConversion amplifiers of an already-real flat parent,
// same pattern as Warlock's Fel Haste chain; Lightfoot specifically
// converts Evasive Maneuvers' OWN Evasion overflow, distinct from
// Elusive-style chains elsewhere since its parent is a FlatStat node
// rather than another OverflowConversion).
// ---------------------------------------------------------------------
static RANGER_NODES: &[PassiveNode] = &[
    skill("multishot", "Multishot", "Increases splash by 8% at rank 1 - +6% per rank (20% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.08, per_additional_rank: 0.06 }),
    skill("mark", "Hunter's Mark", "Marks your target, granting +10% crit chance against it at rank 1 - +8% per rank (26% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.08 }),
    skill("fleet", "Fleet Step", "Increases attack speed by 6% at rank 1 - +4% per rank (14% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.06, per_additional_rank: 0.04 }),
    spec("volley", "multishot", "Volley", "Deals 10% more damage per rank for every target this attack is capable of reaching (not how many it actually hits) - up to 30% per target at 3/3.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Migrated 2026-08-27 (Stage 3): the rank-3 splash crit-chance bonus
    // was a hardcoded 0.10 at the call site. A FRACTION (0.10 = +10%
    // crit chance), like every other crit-chance value in the tree.
    spec("piercingshots", "multishot", "Piercing Shots", "Splash damage can crit independently using your full crit chance/multiplier, unlocked at rank 2 - rank 3 also grants splash +10% crit chance.", SpecialPerRank { values: &[0.0, 0.0, 0.10] }),
    spec("explosivetips", "multishot", "Explosive Tips", "Multishot's splash damage is increased by another 10% per rank (up to +30% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("predatorseye", "mark", "Predator's Eye", "Hunter's Mark also grants +15% crit damage per rank against the marked target (up to +45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    spec("packtactics", "mark", "Pack Tactics", "Hunter's Mark also grants your allies +5% crit chance per rank against the marked target (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("killzone", "mark", "Kill Zone", "Hunter's Mark deals +20% damage per rank (up to +60% at 3/3) to the marked target below 25% HP.", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    spec("rapidfire", "fleet", "Rapid Fire", "Fleet Step's attack speed bonus is increased by another 6% per rank (up to +18% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.06, per_additional_rank: 0.06 }),
    spec("evasivemaneuvers", "fleet", "Evasive Maneuvers", "Fleet Step also grants +6% evasion per rank (up to +18% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.06, per_additional_rank: 0.06 }),
    spec("relentlesspursuit", "fleet", "Relentless Pursuit", "Each hit grants a stacking +2% attack speed per rank for 3s, max 5 stacks (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    // Chain Shot/Deadeye (2026-08-16, tooltip corrected + Deadeye's own
    // behavior changed to match, per a live design call: both boost the
    // MAIN hit directly, splash inherits passively since it already
    // derives from the same base/rolls its own crit off the same
    // crit_chance - see `volley_dmg_per_target_pct`'s doc for Chain Shot,
    // `combat_crit_chance`'s doc for Deadeye).
    modifier_with_effect("chainshot", "volley", "Chain Shot", "Volley's damage-per-target bonus is increased by another 10% per rank (up to +30% at 3/3) - boosts the whole attack, splash included.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("deadeye", "volley", "Deadeye", "+5% crit chance per rank (up to +15% at 3/3) - splash inherits it too, since splash always rolls off your same crit chance.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("stormofarrows", "volley", "Storm of Arrows", "Volley hits 1 additional guaranteed target per rank regardless of overflow (up to 3 extra at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("armorbreaker", "piercingshots", "Armor Breaker", "Piercing Shots' independent splash crit also reduces the target's damage reduction by 3% per rank (up to -9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("windpierce", "piercingshots", "Wind Pierce", "Piercing Shots' splash crit chance bonus is increased by another 5% per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("truestrike", "piercingshots", "True Strike", "Piercing Shots' crit chance against your PRIMARY target (not splash) is increased by 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("widerburst", "explosivetips", "Wider Burst", "Explosive Tips hits 1 additional enemy per rank (up to 3 extra at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("scorchedearth", "explosivetips", "Scorched Earth", "Explosive Tips' splash also reduces enemy damage dealt by 5% per rank for 3s (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("overcharge", "explosivetips", "Overcharge", "Explosive Tips' bonus is increased by another 10% per rank (up to +30% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("apexhunter", "predatorseye", "Apex Hunter", "Predator's Eye's crit damage bonus is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("trueshot", "predatorseye", "Trueshot", "Predator's Eye also grants +10% crit chance per rank against the marked target (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Still NotYetImplemented - Hunter's Mark has no expiry at all
    // (persists permanently once applied, per `apply_first_hit_mark`'s own
    // doc), so "after the mark ends" never has a moment to fire from.
    // Replaced (2026-08-17) - Hunter's Mark never actually expires
    // (persists the whole fight once applied), so "after the mark ends"
    // never happens; building a real expiry would ripple into every other
    // Mark-scaling node, not just this one's 3 points. Re-anchored to
    // Predator's Eye's real crit-damage bonus instead, sharing a fraction
    // of it with allies - same "any OTHER ally" shape Pack Tactics/Alpha's
    // Predator already use one spec over.
    modifier_with_effect(
        "huntersfocus",
        "predatorseye",
        "Hunter's Focus",
        "Predator's Eye's crit damage bonus also applies to allies' hits against the marked target, at 1/3 value per rank (up to full value at 3/3).",
        // Migrated 2026-08-25 (drift batch): the ally share used to be
        // computed as `rank / 3` in combat.rs. These literals are exactly
        // the f64 values `1.0/3.0`, `2.0/3.0` and `1.0` produce (asserted
        // bit-equal in passive_overrides' drift-batch test), so the swap
        // is behavior-neutral at defaults while making the share tunable.
        SpecialPerRank { values: &[0.3333333333333333, 0.6666666666666666, 1.0] },
    ),
    modifier_with_effect("coordinatedstrike", "packtactics", "Coordinated Strike", "Pack Tactics' bonus is increased by another 5% per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("alphaspredator", "packtactics", "Alpha's Predator", "Pack Tactics also grants allies +5% increased damage per rank against the marked target (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("widerpack", "packtactics", "Wider Pack", "Hunter's Mark can affect 1 additional target simultaneously per rank (up to 3 marks at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    // Migrated 2026-08-27 (Stage 3): the 0.35 / 0.40 / 0.45 ladder was
    // hardcoded off the raw rank. A FRACTION of target max HP - this is
    // Kill Zone's whole threshold, not a delta on top of its base.
    modifier_with_effect("finalblow", "killzone", "Final Blow", "Kill Zone's threshold is raised to 35% HP per rank instead of 25% (up to 45% at 3/3).", SpecialPerRank { values: &[0.35, 0.40, 0.45] }),
    modifier_with_effect("cleankill", "killzone", "Clean Kill", "A Kill Zone kill immediately re-applies Hunter's Mark to a new target for free, at a chance per rank - 25% per rank (up to 75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("huntersreward", "killzone", "Hunter's Reward", "A Kill Zone kill heals you for 6% max HP per rank (up to 18% at 3/3).", Special { at_rank_1: 0.06, per_additional_rank: 0.06 }),
    modifier_with_effect("windsprint", "rapidfire", "Wind Sprint", "Rapid Fire's bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("quickshot", "rapidfire", "Quick Shot", "Rapid Fire also grants +3% splash per rank (up to +9% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("fleetingshadow", "rapidfire", "Fleeting Shadow", "Rapid Fire's bonus is doubled for 3s per rank after evading a hit (up to 9s at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("swiftwind", "evasivemaneuvers", "Swift Wind", "Evasive Maneuvers' bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("vanishingshot", "evasivemaneuvers", "Vanishing Shot", "A successful evade grants your next hit +15% crit chance per rank (up to +45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("lightfoot", "evasivemaneuvers", "Lightfoot", "Evasive Maneuvers' overflow past the 75% cap converts to attack speed at 50% efficiency per rank, capped at +10% attack speed per rank (up to +30% at 3/3). Counts your COMBINED gear + tree evasion, not tree investment alone - gear alone can easily push you past 75%.", OverflowConversion { input: Evasion, output: AttackSpeed, at_rank_1: 0.5, per_additional_rank: 0.25 }),
    modifier_with_effect("windborn", "relentlesspursuit", "Windborn", "Relentless Pursuit's max stacks are increased by 1 per rank (up to 8 at 3/3, from the base 5).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("huntersstride", "relentlesspursuit", "Hunter's Stride", "Relentless Pursuit's stacks also grant +2% splash per rank per stack (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    // Never Winded - approximated as extending the shared stack-expiry
    // window (see the construction-site doc) rather than true "only holds
    // longer once already at max" logic.
    modifier_with_effect("neverwinded", "relentlesspursuit", "Never Winded", "Relentless Pursuit's stacks no longer decay once at max, per rank this holds for 2s longer after falling below max (up to 6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
];

// ---------------------------------------------------------------------
// MAGE (root: crit multiplier) - stale as of the 2026-08-16 full-coverage
// pass: temporalrift/unstablepower are real (a 2026-08-15 pass gave
// AttackSpeed a live baseline-overflow conversion via `speed_overflow_dmg_pct`,
// closing the gap this comment used to describe), and every Modifier this
// tree had left (empoweredbolt, volatilemagic, arcaneinstability,
// echoingpower, resonance, infiniteloop, perpetualmotion, riptide,
// unbrokenrhythm, dilation, paradox, eternalmoment, thunderstruck,
// staticfield, stormcaller, conflagration, risingheat, infernalpact,
// blizzard, permafrost, absolutezero) is real too - timewarp joined them
// (2026-08-25 drift batch: its burst window is declared and read like the
// rest). Also real
// (2026-08-15 Cleric-clone follow-up): arcaneshield - a single new
// crit-triggered `grant_shield` call in `apply_hit` (see
// `crit_shield_max_hp_pct`).
// ---------------------------------------------------------------------
static MAGE_NODES: &[PassiveNode] = &[
    skill("arcane", "Arcane Mastery", "Increases crit damage dealt by 3% per rank (up to 9% at 3/3).", FlatStat { stat: CritMultiplier, at_rank_1: 0.03, per_additional_rank: 0.03 }),
    skill("weaving", "Spell Weaving", "Increases attack speed by 5% at rank 1 - +4% per rank (13% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.05, per_additional_rank: 0.04 }),
    skill("surge", "Elemental Surge", "Increases splash by 8% at rank 1 - +6% per rank (20% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.08, per_additional_rank: 0.06 }),
    spec("criticalmass", "arcane", "Critical Mass", "Arcane Mastery also grants +4% crit chance per rank (up to +12% at 3/3).", FlatStat { stat: CritChance, at_rank_1: 0.04, per_additional_rank: 0.04 }),
    spec("overload", "arcane", "Overload", "Arcane Mastery's crit damage bonus is increased by another 3% per rank (up to +9% at 3/3).", FlatStat { stat: CritMultiplier, at_rank_1: 0.03, per_additional_rank: 0.03 }),
    spec("spellecho", "arcane", "Spell Echo", "A crit has a chance to immediately cast again at 50% damage - 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    spec("quickcast", "weaving", "Quickcast", "Spell Weaving's bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("flowstate", "weaving", "Flow State", "Each hit grants a stacking +2% attack speed per rank for 3s, max 5 stacks (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    spec("temporalrift", "weaving", "Temporal Rift", "Attack speed above 100% total converts excess into increased damage at 30% efficiency per rank (up to 90% at 3/3).", Special { at_rank_1: 0.30, per_additional_rank: 0.30 }),
    spec("chainlightning", "surge", "Chain Lightning", "Deals 10% more damage per rank for every target this attack is capable of reaching (not how many it actually hits) - up to 30% per target at 3/3.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("wildfire", "surge", "Wildfire", "Elemental Surge's splash damage is increased by another 10% per rank (up to +30% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("frostnova", "surge", "Frost Nova", "Elemental Surge's splash also reduces the target's evasion by 5% per rank (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("manasurge", "criticalmass", "Mana Surge", "Critical Mass's crit chance bonus is increased by another 3% per rank (up to +9% at 3/3).", FlatStat { stat: CritChance, at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("arcaneshield", "criticalmass", "Arcane Shield", "A crit grants you a shield worth 5% max HP per rank (up to 15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("empoweredbolt", "criticalmass", "Empowered Bolt", "Critical Mass's first hit each fight is a guaranteed crit, unlocked at rank 2 - rank 3 also grants it +20% crit damage.", SpecialPerRank { values: &[0.0, 0.0, 0.20] }),
    modifier_with_effect("cataclysm", "overload", "Cataclysm", "Overload's bonus is increased by another 3% per rank (up to +9% at 3/3).", FlatStat { stat: CritMultiplier, at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("volatilemagic", "overload", "Volatile Magic", "A critical strike splashes 10% of its damage per rank to nearby enemies (up to 30% at 3/3) - this is not a hit and won't trigger any other on-hit effects.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("arcaneinstability", "overload", "Arcane Instability", "Critical damage is increased by 5%/9%/12% (rank 1/2/3) against targets above 65% HP.", SpecialPerRank { values: &[0.05, 0.09, 0.12] }),
    modifier_with_effect("echoingpower", "spellecho", "Echoing Power", "Spell Echo's re-cast damage is increased from 50% per rank (up to 95% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("resonance", "spellecho", "Resonance", "Spell Echo's trigger chance is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Finite Loop (renamed from "Infinite Loop" 2026-08-16 - the old name
    // promised a real recursive chain but was implemented as a flat
    // one-shot chance bump, same shape as Rogue's Double Tap; a live
    // report suspected the old name's literal unbounded-recursion premise
    // as a stack-overflow crash cause - it wasn't (the flat-bump version
    // was never actually recursive), but the mechanic is now a REAL
    // chain, just hard-capped so it can never run away. Reuses the base
    // Spell Echo trigger chance for every repeat roll (no separate chance
    // layer) and caps at `finiteloop_max_repeats` (3/6/9) - see
    // `combat.rs`'s construction-site doc.
    modifier_with_effect("infiniteloop", "spellecho", "Finite Loop", "Spell Echo's re-cast can itself trigger Spell Echo again, at the same chance as the first trigger - can repeat up to 3/6/9 times (rank 1/2/3) before the chain ends.", SpecialPerRank { values: &[3.0, 6.0, 9.0] }),
    modifier_with_effect("haste", "quickcast", "Haste", "Quickcast's bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("acceleration", "quickcast", "Acceleration", "Quickcast also grants +5% splash per rank (up to +15% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Still NotYetImplemented - Quickcast's bonus is a baseline-only
    // (construction-time) attack-speed contributor, not a live-read stat,
    // so "doubled for the first 5s" has no cheap live hook without
    // threading a whole extra live-vs-baseline split through the
    // scheduling path purely for this one node.
    // Real (2026-08-17) - Quickcast is a baseline-only stat with no live
    // "slice" left to double mid-fight; implemented as a separate
    // additive temporary bonus instead (see
    // `early_fight_speed_multiplier`, combat.rs), same player-facing
    // result without needing to re-derive attack_interval_ms()'s formula
    // a second time.
    modifier_with_effect(
        "timewarp",
        "quickcast",
        "Timewarp",
        "Quickcast's bonus is doubled for the first 5s of each fight, per rank this window extends by 2s (up to 11s at 3/3).",
        // Migrated 2026-08-25 (drift batch): combat.rs built the burst
        // window as `5000 + 2000 * rank` ms off the raw rank. These are
        // those real window lengths in seconds (rank1 = 5s base + one
        // extension already included), read x1000 at the call site -
        // same units convention as warpspeed/unbrokenrhythm.
        Special { at_rank_1: 7.0, per_additional_rank: 2.0 },
    ),
    modifier_with_effect("perpetualmotion", "flowstate", "Perpetual Motion", "Flow State's max stacks are increased by 1 per rank (up to 8 at 3/3, from the base 5).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("riptide", "flowstate", "Riptide", "Flow State's stacks also grant +2% crit chance per rank per stack (up to +6% per stack at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    modifier_with_effect("unbrokenrhythm", "flowstate", "Unbroken Rhythm", "Flow State's stacks no longer decay once at max, per rank this holds for 2s longer after falling below max (up to 6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("dilation", "temporalrift", "Dilation", "Temporal Rift's conversion efficiency is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("paradox", "temporalrift", "Paradox", "Temporal Rift also converts excess attack speed into crit chance, at 15% efficiency per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("eternalmoment", "temporalrift", "Eternal Moment", "Temporal Rift's threshold is lowered by 10% per rank, converting excess starting at 90%/80%/70% attack speed (70% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Thunderstruck - see `volley_dmg_per_target_pct`'s construction-site
    // doc: extends the same flat per-target rate Ranger's Chain Shot
    // shares (identical Chain Lightning/Volley pairing this tree's own
    // redesign already established).
    modifier_with_effect("thunderstruck", "chainlightning", "Thunderstruck", "Chain Lightning's extra targets deal 10% more damage per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("staticfield", "chainlightning", "Static Field", "Chain Lightning's splash also reduces the target's attack speed by 4% per rank (up to -12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("stormcaller", "chainlightning", "Stormcaller", "Chain Lightning hits 1 additional guaranteed target per rank regardless of overflow (up to 3 extra at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("conflagration", "wildfire", "Conflagration", "Wildfire's bonus is increased by another 10% per rank (up to +30% at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Rising Heat - approximated as a flat splash bonus (same FlatStat
    // pooled mechanism Wildfire itself already uses) rather than true
    // live per-enemy-hit stacking within a single splash resolution.
    modifier_with_effect("risingheat", "wildfire", "Rising Heat", "Wildfire's splash damage increases by 5% per rank for each enemy it hits, stacking (up to +15% per enemy at 3/3).", FlatStat { stat: Splash, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("infernalpact", "wildfire", "Infernal Pact", "Wildfire also heals you for 3% max HP per rank per enemy hit (up to 9% per enemy at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("blizzard", "frostnova", "Blizzard", "Frost Nova's evasion reduction is increased by another 5% per rank (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("permafrost", "frostnova", "Permafrost", "Frost Nova's effect lasts 2 additional seconds per rank (up to +6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("absolutezero", "frostnova", "Absolute Zero", "Frost Nova's evasion reduction is doubled against enemies below 50% HP, unlocked at rank 2 - rank 3 lowers this threshold to 65%.", SpecialPerRank { values: &[0.0, 0.50, 0.65] }),
];

// ---------------------------------------------------------------------
// WARLOCK (root: attack speed) - real nodes: pact, felhaste, burningrage,
// chaosbolt, siphon (2026-08-15 flat/overflow follow-up). Felhaste/
// burningrage both amplify Dark Pact's flat AttackSpeed (same chain
// pattern used across this whole pass); Chaos Bolt is its own flat crit
// chance grant. Soul Siphon was redefined from a "heal on hit" proc
// (would've needed a new on-hit-heal trigger) to a flat LifeLeechPct
// grant instead - mechanically identical payoff, zero new plumbing, per
// a live simplification request. Also real (2026-08-15 Cleric-clone
// follow-up): soulharvest + eternalhunger - an on-kill heal (checked in
// `apply_hit`'s Defeat branch, right alongside `fire_on_kill`) with a
// guaranteed shield off whatever it actually restores. Reaping/Dark
// Ritual, once genuinely deferred, are now real too, along with the rest
// of this tree's Modifiers as of the 2026-08-16 full-coverage pass -
// voidenergy, entropicforce, chaostheory, warpspeed, deathmarch, ravage,
// witheringcurse, hexmastery, plagueoflocusts, epidemic, harbinger,
// dreadfuldeath, apocalypse, painbond, demonicresilience, soulexchange,
// sharedsuffering, covenant, unbreakablebond. cursedblood/virulence
// (renamed "Soul Stone", both 2026-08-17) were repurposed away from their
// original Doom-expiry-dependent flavor into standalone mechanics - see
// their own doc comments below. demonicspeed joined them (2026-08-25
// drift batch, same window declaration as Mage's Timewarp).
// ---------------------------------------------------------------------
static WARLOCK_NODES: &[PassiveNode] = &[
    skill("pact", "Dark Pact", "Increases attack speed by 6% at rank 1 - +4% per rank (14% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.06, per_additional_rank: 0.04 }),
    skill("curse", "Curse of Weakness", "Curses your target, increasing damage they take by 8% at rank 1 - +6% per rank (20% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.06 }),
    skill("siphon", "Soul Siphon", "Grants 1% life leech per rank (3% at 3/3) - a slice of your damage dealt heals you back, same mechanism as gear's Leech affix.", FlatStat { stat: LifeLeechPct, at_rank_1: 0.01, per_additional_rank: 0.01 }),
    spec("felhaste", "pact", "Fel Haste", "Dark Pact's bonus is increased by another 6% per rank (up to +18% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.06, per_additional_rank: 0.06 }),
    spec("unstablepower", "pact", "Unstable Power", "Attack speed above 100% total converts excess into increased damage at 30% efficiency per rank (up to 90% at 3/3).", Special { at_rank_1: 0.30, per_additional_rank: 0.30 }),
    spec("felrush", "pact", "Fel Rush", "A kill grants +8% attack speed per rank for 4s (up to +24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    spec("amplifycurse", "curse", "Amplify Curse", "Curse of Weakness's damage taken bonus is increased by another 8% per rank (up to +24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    spec("contagiouscurse", "curse", "Contagious Curse", "Curse of Weakness spreads to 1 additional nearby enemy per rank (up to 3 total at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    // Reduced 20/40/60% -> 3/6/9% -> 2/4/6% (2026-08-18, both live requests).
    spec("doom", "curse", "Doom", "Curse of Weakness detonates for a burst of damage when it expires, equal to 2% of damage dealt to the cursed target per rank (up to 6% at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    spec("soulharvest", "siphon", "Soul Harvest", "A kill heals you for 6% max HP per rank (up to 18% at 3/3), on top of Soul Siphon's per-hit heal.", Special { at_rank_1: 0.06, per_additional_rank: 0.06 }),
    spec("lifetap", "siphon", "Life Tap", "Convert 3% of your max HP per rank into +6% increased damage instead, drained once at the start of each fight (up to -9% HP / +18% damage at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    spec("darkcommunion", "siphon", "Dark Communion", "Soul Siphon's heal also applies to your lowest-HP ally, at 50% value per rank (up to 150% at 3/3).", Special { at_rank_1: 0.50, per_additional_rank: 0.50 }),
    modifier_with_effect("chaosbolt", "felhaste", "Chaos Bolt", "Fel Haste's bonus also grants +5% crit chance per rank (up to +15% at 3/3).", FlatStat { stat: CritChance, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("burningrage", "felhaste", "Burning Rage", "Fel Haste's bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Still NotYetImplemented - Fel Haste's bonus is a baseline-only
    // (construction-time) attack-speed contributor, same gap as Mage's
    // Timewarp.
    // Real (2026-08-17) - identical gap/fix to Mage's Timewarp (see its
    // own comment) - Fel Haste is baseline-only too.
    modifier_with_effect(
        "demonicspeed",
        "felhaste",
        "Demonic Speed",
        "Fel Haste's bonus is doubled for the first 5s of each fight, per rank this window extends 2s (up to 11s at 3/3).",
        // Migrated 2026-08-25 (drift batch): identical gap/fix to Mage's
        // Timewarp - combat.rs built the burst window as
        // `5000 + 2000 * rank` ms off the raw rank; these are those real
        // window lengths in seconds, read x1000 at the call site.
        Special { at_rank_1: 7.0, per_additional_rank: 2.0 },
    ),
    modifier_with_effect("voidenergy", "unstablepower", "Void Energy", "Unstable Power's conversion efficiency is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("entropicforce", "unstablepower", "Entropic Force", "Unstable Power also converts excess into splash, at 15% efficiency per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("chaostheory", "unstablepower", "Chaos Theory", "Unstable Power's threshold is lowered by 10% per rank, converting excess starting at 90%/80%/70% attack speed (70% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("warpspeed", "felrush", "Warp Speed", "Fel Rush's duration is increased by 2s per rank (up to +6s at 3/3).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("deathmarch", "felrush", "Death March", "Fel Rush's bonus is increased by another 8% per rank (up to +24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    modifier_with_effect("ravage", "felrush", "Ravage", "Each kill while Fel Rush is active refreshes its full duration, unlocked at rank 2 - rank 3 also stacks its bonus additively.", Special { at_rank_1: 0.5, per_additional_rank: 0.0 }),
    modifier_with_effect("witheringcurse", "amplifycurse", "Withering Curse", "Amplify Curse also reduces the target's healing received by 10% per rank (up to -30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("hexmastery", "amplifycurse", "Hex Mastery", "Amplify Curse's bonus is increased by another 8% per rank (up to +24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    // Real (2026-08-17) - repurposed from its former "persist after Doom's
    // expiry" flavor, which needed a real curse expiry to mean anything
    // and so stayed dormant. Now: immediately curses N random enemies the
    // instant a fight starts (see `own_cursed_blood_target_count`'s doc,
    // combat.rs) - same 1/2/3-by-rank shape as Contagious Curse's own
    // spread-count effect just above.
    modifier_with_effect(
        "cursedblood",
        "amplifycurse",
        "Cursed Blood",
        "Immediately applies Curse of Weakness to 1 enemy per rank at the start of every fight (up to 3 at 3/3), before you've even landed a hit. Bypasses defenses entirely \u{2014} guaranteed to land, no evasion or damage reduction can stop it.",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect("plagueoflocusts", "contagiouscurse", "Plague of Locusts", "Contagious Curse spreads to 1 additional enemy per rank (up to 3 more at 3/3, 6 total possible at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("epidemic", "contagiouscurse", "Epidemic", "Contagious Curse's spread copies deal 15% more damage per rank (up to +45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Real (2026-08-17) - repurposed into "Soul Stone" (its former
    // "spread copies last longer" flavor had the same Doom-dependency
    // problem as Cursed Blood). Key stays `virulence` internally (only the
    // label/effect changed - same "keep the key, redefine the ability"
    // precedent as Mage's Finite Loop) so anyone who already invested
    // points here keeps their rank. See `own_soul_stone_max`'s doc
    // (combat.rs) for the full mechanic.
    modifier_with_effect(
        "virulence",
        "contagiouscurse",
        "Soul Stone",
        "Cursing an enemy creates a Soul Stone (up to 1 per rank, 3 max at 3/3). While holding one, a killing blow instead heals you to full HP and consumes a stone - but stacks a permanent 33% reduction to your own hit damage for each stone used that fight.",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect("harbinger", "doom", "Harbinger", "Doom's detonation damage is increased by another 15% per rank (up to +45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("dreadfuldeath", "doom", "Dreadful Death", "Doom's detonation also reduces the target's damage reduction by 5% per rank for 3s (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("apocalypse", "doom", "Apocalypse", "Doom's detonation splashes to nearby enemies at 30% value per rank (up to 90% at 3/3).", Special { at_rank_1: 0.30, per_additional_rank: 0.30 }),
    modifier_with_effect("reaping", "soulharvest", "Reaping", "Soul Harvest's heal is increased by another 4% per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("darkritual", "soulharvest", "Dark Ritual", "Soul Harvest also grants +5% increased damage per rank for 5s after a kill (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("eternalhunger", "soulharvest", "Eternal Hunger", "Soul Harvest's heal is guaranteed to also grant a small shield worth 25% of the heal per rank (up to 75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("painbond", "lifetap", "Pain Bond", "Life Tap's HP cost is reduced by 1% per rank (up to -3% at 3/3) without lowering the damage bonus.", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    modifier_with_effect("demonicresilience", "lifetap", "Demonic Resilience", "Life Tap also grants +5% damage reduction per rank for the rest of the fight (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("soulexchange", "lifetap", "Soul Exchange", "Life Tap's damage bonus is increased by another 4% per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("sharedsuffering", "darkcommunion", "Shared Suffering", "Dark Communion's value is increased by another 25% per rank (up to +75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("covenant", "darkcommunion", "Covenant", "Dark Communion also applies to your 2nd-lowest-HP ally, unlocked at rank 2 (half value) - rank 3 brings it to full value.", Special { at_rank_1: 0.0, per_additional_rank: 0.5 }),
    modifier_with_effect("unbreakablebond", "darkcommunion", "Unbreakable Bond", "Dark Communion's heal also grants the ally +5% damage reduction per rank for 3s (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
];

// ---------------------------------------------------------------------
// CLERIC (root: healing power) - REAL (2026-08-14 follow-up pass, +1
// 2026-08-15 follow-up): 32 of 39 nodes are genuinely functional. Cleric
// has no dedicated active
// ability (unlike Slayer's FlickerStrike) - its whole identity is the
// passive heal/damage split every unified attack action already rolls
// (`Character::combat_heal_power`), so this pass needed five brand-new
// combat-sim primitives instead of hooking an existing proc: a party-wide
// grant broadcast (`resilience`/`sanctuary`/`radiantaegis` - nothing
// before this let one character's investment affect ANOTHER unit's
// stats, applied once after every fighter exists, Cleric included in
// "party"), a death ward (`guardianspirit`, checked in `apply_hit` before
// hp is allowed to reach 0, can save the Cleric themselves too), a heal
// bounce (`prayer`/`chainoflight`/`mercifultouch` - `apply_heal_bounce`,
// distinct from the older `apply_heal_splash` since it's chance-gated
// and chains through a rank-scaled target count rather than a fixed
// simultaneous set), an overheal-to-shield conversion
// (`overflowinggrace` - excess healing beyond a target's max hp becomes
// a temporary shield instead of being wasted, via a new shared
// `apply_heal` helper that replaced 4 near-duplicate heal-application
// blocks), and heal-crit surfacing (`sanctifiedtouch` - heals already
// crit via the same roll as attacks, but that crit flag used to be
// discarded; now it's read to apply a heal-specific bonus). Also real
// (2026-08-15 follow-up): `sacredbarrier` - a shield-absorb-reflect
// primitive (`shield_reflect_pct`/`apply_reflect_damage`) shared with
// Paladin's Retribution Aura and Slayer's Guardian's Blood.
// `wideningcircle`/`unbrokenprayer`/`compassion`/`wardingprayer`/
// `haloedsteps` were all converted to real `modifier_with_effect` nodes in
// a later pass (this comment used to list them as still deferred - fixed
// 2026-08-17). `eternallight` and `unyieldingfaith` (the last two
// genuinely NotYetImplemented nodes in this tree) are now real too, as of
// the 2026-08-17 full-coverage pass - see their own comments below. Every
// Cleric node is implemented.
// ---------------------------------------------------------------------
static CLERIC_NODES: &[PassiveNode] = &[
    skill("grace", "Divine Grace", "Increases healing power by 20% at rank 1 - +16% per rank (52% at 3/3).", FlatStat { stat: HealPowerPct, at_rank_1: 0.20, per_additional_rank: 0.16 }),
    skill("prayer", "Prayer of Mending", "Your heals have a chance to also chain to another hurt ally for 50% of the primary heal's value (Merciful Touch scales this further; without it, 50% is the flat baseline) - 15% chance at rank 1, +10% per rank (35% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.10 }),
    skill("resilience", "Blessed Resilience", "Grants the WHOLE party (yourself included) +4% max HP at rank 1 - +3% per rank (10% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.03 }),
    spec("radiantlight", "grace", "Radiant Light", "Divine Grace's bonus is increased by another 16% per rank (up to +48% at 3/3) - a second, independent stack of healing power, not a multiplier on Divine Grace's own.", FlatStat { stat: HealPowerPct, at_rank_1: 0.16, per_additional_rank: 0.16 }),
    spec("overflowinggrace", "grace", "Overflowing Grace", "Overheal - the part of a heal that would exceed the target's max HP - no longer wastes, instead becoming a temporary shield (lasting 5s, before Rift of Mercy) worth this fraction of the overheal: 10% at rank 1, +10% per rank (30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("sanctifiedtouch", "grace", "Sanctified Touch", "Unlocked at rank 2: a heal that crits deals 50% more, on top of your normal crit multiplier. Rank 3 also adds a flat +10% crit chance for the heal roll specifically (your attack share's crit chance is untouched). Rank 1 alone does nothing yet.", Special { at_rank_1: 0.0, per_additional_rank: 0.0 }),
    spec("chainoflight", "prayer", "Chain of Light", "Prayer of Mending chains through 1 additional ally per point invested (2 total targets at rank 1, up to 4 at rank 3) - each reached target gets the full bounce value, not a diminishing one.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    spec("mercifultouch", "prayer", "Merciful Touch", "Once invested, overrides Prayer of Mending's flat 50% bounce value with its own scaling: 50% at rank 1, +15% per rank (80% at 3/3).", Special { at_rank_1: 0.50, per_additional_rank: 0.15 }),
    spec("divinefavor", "prayer", "Divine Favor", "Each Prayer of Mending bounce also shields its target (5s, before Warding Light) for this fraction of the bounce heal's value: 20% at rank 1, +20% per rank (60% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    spec("sanctuary", "resilience", "Sanctuary", "Blessed Resilience also grants the whole party +3% damage reduction per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of saves per fight (0/1/2),
    // matched off the raw rank before. The 20% base heal is a flat base
    // released by the rank-2 unlock (structure), not a rank-fed value -
    // Second Chance is the tunable that adds to it.
    spec("guardianspirit", "resilience", "Guardian Spirit", "Unlocked at rank 2: once per fight, prevent ANY party member from dying (yourself included) - heals them for 20% max HP instead of letting the killing blow land. Rank 3 grants a second use per fight. Rank 1 alone does nothing yet.", SpecialPerRank { values: &[0.0, 1.0, 2.0] }),
    spec("radiantaegis", "resilience", "Radiant Aegis", "Blessed Resilience also grants the whole party +4% evasion per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("luminous", "radiantlight", "Luminous", "Radiant Light's bonus is increased by another 16% per rank (up to +48% at 3/3) - same independent healing-power stack as Radiant Light itself.", FlatStat { stat: HealPowerPct, at_rank_1: 0.16, per_additional_rank: 0.16 }),
    modifier_with_effect("graciousspirit", "radiantlight", "Gracious Spirit", "+3% healing power per rank (up to +9% at 3/3), applied only to your PRIMARY heal share each turn - it always targets the lowest-HP hurt ally by construction, so this never reaches splash/bounce targets.", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Rewritten (2026-08-17) - Radiant Light is a PERMANENT stat stack,
    // not a temporary buff, so the old "persists after a heal" premise had
    // nothing to persist. Original draft ("boosts only your first heal
    // each fight") was flagged weak next to sibling Luminous (a full
    // permanent stack of the same magnitude) - rewritten to refresh on
    // EVERY heal instead, giving it a real identity (a keep-casting uptime
    // bonus) rather than a strictly-smaller copy of Luminous.
    modifier_with_effect(
        "eternallight",
        "radiantlight",
        "Eternal Light",
        "Every heal you land grants +16% per rank healing power for 3s - keep casting and it never drops, but a gap longer than 3s lets it lapse.",
        Special { at_rank_1: 0.16, per_additional_rank: 0.16 },
    ),
    modifier_with_effect("graciousoverflow", "overflowinggrace", "Gracious Overflow", "Overflowing Grace's shield value is increased by another 10% of the overheal per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("balancedfaith", "overflowinggrace", "Balanced Faith", "While Overflowing Grace's shield is still active on a unit, they get +10% damage reduction per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("riftofmercy", "overflowinggrace", "Rift of Mercy", "The shield's duration before it decays is increased by 2s per rank (up to +6s at 3/3, from the base 5s).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("holycrit", "sanctifiedtouch", "Holy Crit", "Sanctified Touch's crit-heal bonus climbs from its 50% base by another 10% per rank (60% at rank 1, up to 80% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("divineclarity", "sanctifiedtouch", "Divine Clarity", "Sanctified Touch's rank-3 heal crit chance bonus is increased by another 5% per rank (up to +15% at 3/3, on top of the base +10%).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("radiance", "sanctifiedtouch", "Radiance", "A critical heal also splashes 20% of its value per rank to the rest of the party (up to 60% at 3/3) - separate from, and on top of, your normal gear-based Splash.", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("wideningcircle", "chainoflight", "Widening Circle", "Chain of Light can bounce to 1 additional ally per rank, further (up to 2 more at 3/3, on top of its base extra bounce).", Special { at_rank_1: 1.0, per_additional_rank: 0.5 }),
    modifier_with_effect("swiftmending", "chainoflight", "Swift Mending", "Prayer of Mending's bounce chance is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("unbrokenprayer", "chainoflight", "Unbroken Prayer", "Chain of Light's bounces can themselves bounce again, at a 15% chance per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("gentletouch", "mercifultouch", "Gentle Touch", "Merciful Touch's bounce value is increased by another 5% per rank (up to +15% at 3/3), on top of whichever base is active.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Migrated 2026-08-27 (Stage 3): the rank-3 ally damage reduction was
    // a hardcoded 0.05 at the call site. A FRACTION (0.05 = +5% DR). The
    // rank-2 "guarantees the lowest-HP bounce" read is an unlock gate
    // (structure) and stays on the rank.
    modifier_with_effect("compassion", "mercifultouch", "Compassion", "Merciful Touch prioritizes the lowest-HP ally for its bounce - rank 2 guarantees this, rank 3 also grants that ally +5% damage reduction for 3s.", SpecialPerRank { values: &[0.0, 0.0, 0.05] }),
    modifier_with_effect("healingtouch", "mercifultouch", "Healing Touch", "Each bounced ally gets their own temporary +5% healing power per rank for 3s (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("aegisofmercy", "divinefavor", "Aegis of Mercy", "Divine Favor's shield value is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("wardinglight", "divinefavor", "Warding Light", "Divine Favor's shield lasts 2 additional seconds per rank (up to +6s at 3/3, from the base 5s).", Special { at_rank_1: 2.0, per_additional_rank: 2.0 }),
    modifier_with_effect("sacredbarrier", "divinefavor", "Sacred Barrier", "Divine Favor's shield has a chance per rank to also reflect 20% of absorbed damage - 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("consecratedearth", "sanctuary", "Consecrated Earth", "Sanctuary's party damage reduction bonus is increased by another 3% per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Approximated as a flat party DR bonus (see the construction-site
    // doc) rather than a genuine boss-specific channel.
    modifier_with_effect("wardingprayer", "sanctuary", "Warding Prayer", "Sanctuary also reduces damage taken from bosses specifically by another 3% per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Still NotYetImplemented - Sanctuary's party grant is summed once at
    // fight construction (see `party_damage_reduction_pct`'s doc), not
    // re-evaluated live, same gap as Paladin's Unwavering.
    // Real (2026-08-17) - identical gap/fix to Paladin's Unwavering (see
    // its own comment) - shares the same underlying mechanism.
    modifier_with_effect(
        "unyieldingfaith",
        "sanctuary",
        "Unyielding Faith",
        "Sanctuary's bonus doubles for the party while you are below 50% HP, unlocked at rank 2 - rank 3 lowers this threshold to 65%.",
        // Migrated 2026-08-25 (drift batch): same 0/0.50/0.65 table as
        // Paladin's Unwavering - the two share one mechanic and now one
        // declaration shape too.
        SpecialPerRank { values: &[0.0, 0.50, 0.65] },
    ),
    modifier_with_effect("secondchance", "guardianspirit", "Second Chance", "Guardian Spirit's save heal is increased by another 8% max HP per rank (up to +24% at 3/3, on top of the base 20%).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    modifier_with_effect("divineintervention", "guardianspirit", "Divine Intervention", "Guardian Spirit's save also grants the saved unit +10% damage reduction per rank for 5s (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("finalblessing", "guardianspirit", "Final Blessing", "Guardian Spirit's save also grants the WHOLE party +5% healing power per rank for 5s afterward (up to +15% at 3/3), not just the saved unit.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("windsofgrace", "radiantaegis", "Winds of Grace", "Radiant Aegis's party evasion bonus is increased by another 4% per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("swiftblessing", "radiantaegis", "Swift Blessing", "Radiant Aegis also grants the whole party +3% attack speed per rank (up to +9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Reworked 2026-08-21 (owner design call) - was an OverflowConversion
    // node (Radiant Aegis's evasion overflow past 75% -> party DR). Now a
    // whole-party multiplicative more-damage grant, scaled off the
    // Cleric's own equipped Divine Damage affix COUNT (not its summed
    // value - see `Character::count_affix`'s doc) at a flat per-instance
    // rate per rank, capped per rank: 1/2/3% per Divine Damage affix
    // instance at rank 1/2/3, capped at 3/6/9% (so the cap always binds
    // at exactly 3 instances, every rank, by construction). The
    // per-instance rate lives in `LiveTunables` (`haloedsteps_per_instance_pct_rank1/2/3`,
    // same non-linear-per-rank shape as Righteous Fire's own
    // `rf_self_damage_pct_rank1/2/3`); this node's own magnitude below
    // IS the cap, already live-tunable via the existing per-rank override
    // store (`PassiveNode::magnitude_at_rank`) with no new plumbing.
    modifier_with_effect("haloedsteps", "radiantaegis", "Haloed Steps", "Grants the whole party (yourself included) multiplicative more damage, scaled by how many Divine Damage affix instances you have equipped: 1% per instance at rank 1, 2% at rank 2, 3% at rank 3 - capped at 3%/6%/9% (the cap at every rank).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
];

// ---------------------------------------------------------------------
// DRUID (root: evasion) - real nodes: regrowth, instinct, barrier,
// shiftingform, feralreflexes, quickpaw, wildagility, livingarmor,
// ironbark, primalshift, clawstrike (2026-08-15 flat/overflow follow-up
// - same "amplifies an already-real flat/overflow parent" pattern as
// everywhere else this pass). Wild Surge stays deferred - its own
// "shortens the interval FURTHER" wording is a genuinely separate
// mechanism from a flat HealPowerPct grant, not just phrasing, and
// HealPowerPct has no overflow_cap() either way.
// Also real (2026-08-15 Cleric-clone follow-up): Rejuvenation +
// bloomingfield/evergrowth/seedoflife (word-for-word the same shape as
// Cleric's Prayer of Mending + Chain of Light/Swift Mending/Merciful
// Touch - shares `apply_heal_bounce`'s `prayer_*` fields, see
// `CombatSimUnit`'s doc) and Nature's Blessing + bloomstrike/
// wildinstinct/verdantburst (the same shape as Cleric's Sanctified
// Touch + Holy Crit/Divine Clarity/Radiance - shares the `heal_crit_*`
// fields). Also real (2026-08-15 second follow-up, same day):
// unyieldingroots (doubles Living Armor's own DR contribution while
// below a live self-HP threshold - self-conditional, so `resolve_hit`'s
// existing live `def.hp`/`def.max_hp` already cover it with no
// architecture gap, unlike Paladin/Cleric's party-wide versions of the
// same idea, which stay deferred - see `PALADIN_NODES`'s `unwavering`),
// and bramblegrowth + thornlash/poisonthorns (a DR-reflect - reuses the
// SAME `apply_reflect_damage` helper the shield-absorb-reflect primitive
// added, just triggered off a hit's total reduced amount instead of a
// shield's absorbed amount; see `bramblegrowth`'s own description for
// why "reduced by Thorned Barrier" reads as total combined reduction
// rather than isolating one source). Entangle stays deferred - "a second
// attacker if multiple enemies hit you THIS TURN" has no clean concept
// in an event-driven sim where each hit resolves independently.
// ---------------------------------------------------------------------
static DRUID_NODES: &[PassiveNode] = &[
    // Reworked 2026-08-16 per a live balance report ("Druid healing is
    // absolute trash") - the root cause: Cleric's root passive itself
    // grants heal_power_pct=0.50 (see Archetype::bonus), but Druid's root
    // grants evasion instead, and Cleric's tree then ALSO stacks Grace
    // (52% at 3/3) + Radiant Light (48%) + Luminous (48%) as independent
    // FlatStat pools, while Druid's Regrowth was the tree's ONLY
    // heal_power_pct source at all, capped at 40%. Fix: Regrowth's base
    // FlatStat now matches Grace's rate exactly (same 20%/+16%-per-rank
    // shape, same 52% ceiling), PLUS a brand-new separate multiplicative
    // layer (`Character::combat_heal_power`'s `regrowth_mult` term) that
    // doesn't exist on Cleric's kit at all - Druid's OWN compensation for
    // not having Cleric's extra stacking nodes, not a straight copy.
    skill("regrowth", "Regrowth", "Increases healing power at the same rate as Cleric's Divine Grace - 20% at rank 1, +16% per rank (52% at 3/3) - PLUS its own separate multiplicative healing power increase on top: +10% per rank (up to +30% at 3/3, stacking multiplicatively with everything else, gear included).", FlatStat { stat: HealPowerPct, at_rank_1: 0.20, per_additional_rank: 0.16 }),
    skill("instinct", "Predator's Instinct", "Increases evasion by 6% at rank 1 - +4% per rank (14% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.06, per_additional_rank: 0.04 }),
    // Reworked 2026-08-16 - its own independent MULTIPLICATIVE damage
    // reduction source (see `Character::combat_damage_reduction`'s
    // `thornedbarrier_mult` term), no longer pooled into the generic
    // additive tree stat Living Armor (its sibling spec) still uses.
    skill("barrier", "Thorned Barrier", "A multiplicative damage reduction increase - 5% at rank 1, +5% per rank (15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("rejuvenation", "regrowth", "Rejuvenation", "Regrowth's heals have a chance to also heal a second ally for 50% value - 15% at rank 1, +10% per rank (35% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.10 }),
    spec(
        "wildsurge",
        "regrowth",
        "Wild Surge",
        "Healing Power above 100% already shortens your action interval instead of making individual heals bigger - Wild Surge shortens it FURTHER, by another 8% per rank (up to 24% faster at 3/3, stacking with the base effect).",
        Special { at_rank_1: 0.08, per_additional_rank: 0.08 },
    ),
    // Repurposed 2026-08-16 (same "Druid healing" pass) - a plain crit
    // chance grant, same shape as e.g. Mage's Critical Mass.
    spec("naturesblessing", "regrowth", "Nature's Blessing", "Increases crit chance by 10% at rank 1 - +10% per rank (30% at 3/3).", FlatStat { stat: CritChance, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("feralreflexes", "instinct", "Feral Reflexes", "Predator's Instinct's bonus is increased by another 6% per rank (up to +18% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.06, per_additional_rank: 0.06 }),
    spec("shiftingform", "instinct", "Shifting Form", "Evasion overflow past the 75% cap converts to increased damage at 50% efficiency per rank, capped at +10% increased damage per rank (up to +30% at 3/3). Counts your COMBINED gear + tree evasion, not tree investment alone - gear alone can easily push you past 75%.", OverflowConversion { input: Evasion, output: IncreasedDamage, at_rank_1: 0.5, per_additional_rank: 0.25 }),
    spec("packinstinct", "instinct", "Pack Instinct", "Predator's Instinct also grants your lowest-HP ally +4% evasion per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    spec("livingarmor", "barrier", "Living Armor", "Thorned Barrier's bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: DamageReduction, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("bramblegrowth", "barrier", "Bramblegrowth", "A hit reduced by Thorned Barrier reflects 15% of the reduced damage back at the attacker per rank (up to 45% at 3/3). In practice this reads off the hit's TOTAL reduction (all combined DR/block sources, not just Thorned Barrier's own slice) as long as Thorned Barrier is invested - isolating one source's specific share of an already-combined reduction isn't something the combat sim tracks.", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Reworked 2026-08-16 - renamed Werebear, grants "Thick Hide": periodic
    // self-cleanse of enemy-inflicted debuffs, replacing its old "+DR to
    // lowest-HP ally" role entirely. `at_rank_1: 6000, per_additional_rank:
    // -1000` gives exactly 6000/5000/4000ms via the standard magnitude
    // formula - see `thickhide_cycle_ms`'s doc in combat.rs.
    spec(
        "symbiosis",
        "barrier",
        "Werebear",
        "Grants Thick Hide: every 6 seconds at rank 1, removes any debuffs enemies have inflicted on you - the cycle shortens to every 5s at rank 2, every 4s at rank 3.",
        Special { at_rank_1: 6000.0, per_additional_rank: -1000.0 },
    ),
    // Repurposed 2026-08-16 alongside Regrowth's own rework (same live
    // "Druid healing is absolute trash" report) - a SECOND independent
    // multiplicative healing power layer, separate from Regrowth's own
    // (see `Character::combat_heal_power`'s `bloomingfield_mult` term).
    // No longer touches Rejuvenation's bounce at all.
    modifier_with_effect("bloomingfield", "rejuvenation", "Blooming Field", "A second, independent multiplicative healing power increase, separate from Regrowth's own - +10% per rank (up to +30% at 3/3, stacking multiplicatively with everything else, gear included).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Redesigned 2026-08-21 alongside the Echo rework (was: converts a
    // slice of TOTAL healing power directly into Lingering Effect - same
    // formula, new target stat, same per-rank numbers - see
    // `Character::combat_echo_pct`'s doc for the worked example this
    // description's own number comes from). No longer touches
    // Rejuvenation's bounce value at all.
    modifier_with_effect("evergrowth", "rejuvenation", "Evergrowth", "Converts a slice of your TOTAL healing power directly into Echo chance - 3% at rank 1, +3% per rank (9% at 3/3). E.g. at 1000% healing power, 3/3 Evergrowth alone grants 90% Echo chance.", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Redesigned 2026-08-21 alongside the Echo rework (was: every tick of
    // a Lingering Effect heal-flavor instance granted this shield - see
    // `CombatSimUnit::seedoflife_shield_pct`'s doc). Same per-rank numbers,
    // new trigger: fires once per ECHOED heal instead of once per DoT/HoT
    // tick - a real frequency drop (an echo fires at most once per turn,
    // vs. up to 80 ticks across a Lingering Effect instance's old
    // lifetime), flagged plainly rather than silently compensated with a
    // bigger number; these values ride the live-tunable node-value
    // override path, so they're cheap to retune later if underwhelming in
    // practice. No longer touches Rejuvenation's bounce chance.
    modifier_with_effect("seedoflife", "rejuvenation", "Seed of Life", "Every time your own heal echoes, the echoed heal also grants the target a stacking shield, at the same rate - 10% of the echoed heal's own amount at rank 1, +10% per rank (30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect(
        "overgrowth",
        "wildsurge",
        "Overgrowth",
        "Wild Surge's extra interval reduction is increased by another 5% per rank (up to +15% at 3/3).",
        Special { at_rank_1: 0.05, per_additional_rank: 0.05 },
    ),
    // Repurposed 2026-08-16 (same "Druid healing" pass) - no longer tied
    // to Wild Surge (which stays NotYetImplemented) at all: a flat,
    // unconditional multiplicative splash increase - see
    // `Character::combat_splash`'s `primalforce_mult` term.
    modifier_with_effect("primalforce", "wildsurge", "Primal Force", "A flat, unconditional multiplicative splash increase - +10% per rank (up to +30% at 3/3, stacking multiplicatively with everything else, gear included).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Repurposed 2026-08-16 (same "Druid healing" pass) - no longer tied
    // to Wild Surge (which stays NotYetImplemented) at all. See
    // `apply_heal`'s `wildheart_self_heal_pct` hook in combat.rs.
    modifier_with_effect("wildheart", "wildsurge", "Wild Heart", "A slice of any healing you land on someone ELSE also heals you - 10% at rank 1, +10% per rank (30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Repurposed 2026-08-16 alongside Nature's Blessing itself - a second
    // independent crit chance stack, same shape as Radiant Light stacking
    // on top of Divine Grace.
    modifier_with_effect("bloomstrike", "naturesblessing", "Bloom Strike", "A second, independent crit chance increase, on top of Nature's Blessing's own - +10% per rank (up to +30% at 3/3).", FlatStat { stat: CritChance, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    // Repurposed 2026-08-16 (same "Druid healing" pass) - no longer tied
    // to Nature's Blessing's old crit-heal role. See `apply_heal`'s
    // `wildinstinct_dr_pct` hook in combat.rs.
    modifier_with_effect("wildinstinct", "naturesblessing", "Wild Instinct", "Any heal you land also grants its target a temporary multiplicative damage reduction - 3% at rank 1, +3% per rank (9% at 3/3), for 3s.", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Redesigned 2026-08-21 alongside the Echo rework (was: a death ward
    // gated on your OWN pending Lingering Effect healing on the target -
    // that mechanic no longer exists). New condition is deterministic, not
    // a dice roll, matching every sibling branch in `apply_hit`'s
    // would-kill chain (Guardian Spirit, Undying Fury, Soul Stone, Chakra
    // of Life - none of them roll dice either): triggers when your OWN
    // current Echo chance is at or above `LiveTunables::
    // verdantburst_echo_threshold_pct` (default 100%) - see
    // `CombatSimUnit::verdantburst_echo_threshold_pct`'s doc. Charge count
    // (1/2/3 by rank) is unchanged.
    modifier_with_effect(
        "verdantburst",
        "naturesblessing",
        "Verdant Burst",
        "If a hit would kill an ally, and your own Echo chance already meets a threshold, they survive at 1 HP instead. 1 use per fight at rank 1, +1 per rank (3 uses at 3/3).",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect("quickpaw", "feralreflexes", "Quick Paw", "Feral Reflexes' bonus is increased by another 5% per rank (up to +15% at 3/3).", FlatStat { stat: Evasion, at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("silentprowl", "feralreflexes", "Silent Prowl", "A successful evade grants +10% crit chance per rank on your next hit (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("wildagility", "feralreflexes", "Wild Agility", "Feral Reflexes also grants +3% attack speed per rank (up to +9% at 3/3).", FlatStat { stat: AttackSpeed, at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("primalshift", "shiftingform", "Primal Shift", "A second, independent conversion channel off the same evasion overflow Shifting Form draws from - increased damage at 15% efficiency per rank, capped at +10% increased damage per rank (up to +30% at 3/3, stacking with Shifting Form's own cap for up to +60% total).", OverflowConversion { input: Evasion, output: IncreasedDamage, at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("clawstrike", "shiftingform", "Claw Strike", "Shifting Form also converts a portion of overflow into crit chance, at 20% efficiency per rank, capped at +10% crit chance per rank (up to +30% at 3/3).", OverflowConversion { input: Evasion, output: CritChance, at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("wildfury", "shiftingform", "Wild Fury", "An evaded hit has a chance to trigger an immediate free attack - 10% per rank (up to 30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("pathfinder", "packinstinct", "Pathfinder", "Pack Instinct's bonus is increased by another 4% per rank (up to +12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    modifier_with_effect("unitedpack", "packinstinct", "United Pack", "Pack Instinct protects 1 additional ally per rank (up to all 3 party members at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("wildguardian", "packinstinct", "Wild Guardian", "Pack Instinct also heals the protected ally for 2% max HP per rank, once every 5s (up to 6% at 3/3).", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    // Repurposed 2026-08-16 alongside Thorned Barrier's own rework - no
    // longer touches Living Armor at all, instead adds directly into
    // Thorned Barrier's own multiplicative source (combined total 10/20/30%
    // at 3/3 of each).
    modifier_with_effect("ironbark", "livingarmor", "Ironbark", "Adds directly to Thorned Barrier's own multiplicative damage reduction - +5% per rank (up to +15% at 3/3, 10/20/30% combined total with Thorned Barrier's own).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Reworked 2026-08-16 - a real independent multiplicative DR source
    // gated to boss attackers only, no longer an extension of Living
    // Armor's own pool - see `resolve_hit`'s `naturesward_bonus` term.
    modifier_with_effect("naturesward", "livingarmor", "Nature's Ward", "A multiplicative damage reduction increase specifically against boss attacks - 3% at rank 1, +3% per rank (9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    // Reworked 2026-08-16 - a real taunt, replacing its old "doubles Living
    // Armor's bonus below an HP threshold" role entirely. `at_rank_1: 8000,
    // per_additional_rank: -2000` gives exactly 8000/6000/4000ms via the
    // standard magnitude formula - see `unyieldingroots_cycle_ms`'s doc.
    modifier_with_effect(
        "unyieldingroots",
        "livingarmor",
        "Unyielding Roots",
        "Forces every boss attack to target you specifically for 2 out of every 8 seconds at rank 1 - the cycle shortens to every 6s at rank 2, every 4s at rank 3 (same 2s taunt window each time).",
        Special { at_rank_1: 8000.0, per_additional_rank: -2000.0 },
    ),
    modifier_with_effect("thornlash", "bramblegrowth", "Thornlash", "Bramblegrowth's reflect is increased by another 10% per rank (up to +30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("poisonthorns", "bramblegrowth", "Poison Thorns", "Bramblegrowth's reflect also reduces the attacker's damage dealt by 5% per rank for 3s (up to -15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("entangle", "bramblegrowth", "Entangle", "Bramblegrowth's reflect has a chance per rank to also apply to a second attacker if multiple enemies hit you that turn - 15% per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Repurposed 2026-08-16 alongside Werebear - extends Thick Hide's
    // cleanse to more targets instead of Symbiosis's old protect-count.
    modifier_with_effect(
        "rootednetwork",
        "symbiosis",
        "Rooted Network",
        "Thick Hide also protects 1 additional party member per rank (up to 3 at 3/3), PLUS 1 more for every 100% splash you have.",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    // Repurposed 2026-08-16 (Werebear rework) - a reactive party-wide
    // fear proc, no longer tied to Symbiosis's DR at all. See
    // `apply_hit`'s death-trigger block in combat.rs.
    modifier_with_effect(
        "livingbond",
        "symbiosis",
        "Wild Roar",
        "Whenever a party member dies, fear every enemy for 1 second (unable to act). 1 use per fight at rank 1, +1 per rank (3 uses at 3/3).",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    // Repurposed alongside Wild Roar - same death-trigger, instantly and
    // fully heals other party members instead.
    modifier_with_effect(
        "naturesembrace",
        "symbiosis",
        "Nature's Embrace",
        "Whenever a party member dies, instantly and fully heals the lowest-HP other party members - 1 at rank 1, +1 per rank (3 at 3/3).",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
];

// ---------------------------------------------------------------------
// SLAYER (root: life leech) - REAL (2026-08-14 follow-up pass, corrected
// 2026-08-14 tooltip-accuracy pass, +1 2026-08-15 follow-up, +3 2026-08-15
// second follow-up - Vampiric Frenzy's Blood Frenzy/Endless Thirst/
// Reaper's Momentum): 20 of 39
// nodes are genuinely functional (`Special` effect, bespoke logic in
// adventure.rs's
// `simulate_battle`/`apply_hit`/`resolve_hit` - see
// `Character::passive_node_rank`/`passive_node_magnitude`) - Open Wound's
// core branch (wound/festering/hemorrhage/necrotic + blooddebt/overflow/
// arterialspray), Vampiric Frenzy's cooldown reduction itself (also fixes
// a real pre-existing bug where the discount only ever applied to the
// FIRST FlickerStrike cast, silently reverting to the full 5s cadence
// after - see `flicker_cooldown_ms`), and Bloodpact's core branch
// (sacrifice/grimbargain/bloodsac/martyrdom + bloodforblood/debtcollector/
// sharedpain). Bloodpact auto-fires on this unit's first unified-attack
// action(s) each fight (no live player input during an auto-battle sim -
// see the memory/plan for this decision) and deals a flat 2x/3x/4x damage
// multiplier (by Bloodpact's own rank) rather than a guaranteed crit - a
// guaranteed crit was worthless on a Slayer with low crit_multiplier, so
// this replaced it as a deterministic payoff instead.
// Rot and Withering Touch also carry real `Special` effects (their wound
// heal-received debuff IS computed and stored), but are currently INERT -
// nothing in the sim heals a wounded enemy today, so investing in either
// does nothing observable yet. Left implemented-but-dormant rather than
// reverted, since the day a boss ability heals, they'll start working
// with no further changes.
// Also real (2026-08-15 follow-up): `guardiansblood` - a shield-absorb-
// reflect primitive (`shield_reflect_pct`/`apply_reflect_damage`) shared
// with Cleric's Sacred Barrier and Paladin's Retribution Aura. Also real
// (2026-08-15 second follow-up, same day): Vampiric Frenzy's whole
// temporary-buff branch - Blood Frenzy (a flat refreshed attack-speed
// buff on every dash, same shape as Warlock's Fel Rush but triggered by
// the dash itself instead of a kill - `flicker_frenzy_speed_bonus`/
// `_expires_at_ms`), Endless Thirst (same dash trigger, temporarily
// raises - or at rank 3, entirely removes - the leech-per-second cap;
// `endless_thirst_cap_bonus`/`_uncapped`/`_expires_at_ms`), and Reaper's
// Momentum (a kill from FlickerStrike's own direct hit, checked inline
// in `ArchetypeSkill::on_periodic_tick` rather than the generic
// `fire_on_kill` dispatch since it's gated to FlickerStrike specifically,
// banks bonus targets for the unit's NEXT dash -
// `reapers_momentum_per_kill`/`_banked`).
// Contagion/Plague Bearer/Grave Chill/Second Wind/Clean Slate/Triage/
// Insatiable/Second Heartbeat/Chain Reaper/Death Spiral/Undying/Last
// Rites/War Cry/Warlord's Resolve were all converted to real
// `modifier_with_effect` nodes in a later pass (this comment used to list
// them as still deferred - fixed 2026-08-17). `finaloffering` (the last
// genuinely NotYetImplemented node in this tree, replaced 2026-08-17 -
// see its own comment below) is now real too. Every Slayer node is
// implemented.
// ---------------------------------------------------------------------
static SLAYER_NODES: &[PassiveNode] = &[
    skill("wound", "Open Wound", "Hits apply a stacking wound (max 5 stacks, before Blood Debt). Each stack adds 0.4% leech against that target at rank 1, +0.2% per additional rank - 0.6% per stack at rank 2, 0.8% per stack at rank 3 (up to 4.0% total leech at 5 stacks). Blends into your normal life leech, still capped by the same leech-per-second cap.", Special { at_rank_1: 0.004, per_additional_rank: 0.002 }),
    // Key is "vampiricfrenzy", not plain "frenzy" - Berserker's own
    // unrelated "Frenzy" skill already uses that key. Global key
    // uniqueness is required now that Split Personality lets one character
    // hold allocations from two trees in the same flat lookup.
    skill("vampiricfrenzy", "Vampiric Frenzy", "Reduces FlickerStrike's cooldown by 12% at rank 1 - +8% per additional rank (28% at 3/3, ~3.6s from the base 5s). Folds the real, already-shipped FlickerStrike ability into the tree.", Special { at_rank_1: 0.12, per_additional_rank: 0.08 }),
    skill("sacrifice", "Bloodpact", "Automatically fires every 4s of the fight once available (no live input needed during an auto-battle): sacrifice 10% of your CURRENT HP at rank 1 - cost drops 2% per additional rank (6% HP at rank 3) - to deal double damage on that hit. Triple damage at rank 2, quadruple at rank 3. The sacrifice can never drop you below 1 HP.", Special { at_rank_1: 0.10, per_additional_rank: -0.02 }),
    spec("festering", "wound", "Festering Wound", "Wound stacks last 25% longer at rank 1 (7.5s, up from the base 6s) and spread to your splash targets - +25% per additional rank (75% longer at 3/3, 10.5s).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    spec("hemorrhage", "wound", "Hemorrhage", "When a wound reaches max stacks, it explodes for 10% of all the damage you've dealt to that target since the wound was first applied, at rank 1 - +10% per additional rank (30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("necrotic", "wound", "Necrotic Grip", "While wounded, an enemy deals 10% less damage per rank (up to -30% at 3/3) on their own hits.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    spec("bloodfrenzy", "vampiricfrenzy", "Blood Frenzy", "Each FlickerStrike dash grants +15% attack speed per rank for 4s (up to +45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    spec("endlessthirst", "vampiricfrenzy", "Endless Thirst", "Each FlickerStrike dash raises your leech cap by +5% max HP/sec per rank for 4s - at 3/3 the cap is removed entirely instead of +15%.", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    spec("reapers", "vampiricfrenzy", "Reaper's Momentum", "A kill from FlickerStrike adds 1 bonus target to your next dash per rank (up to 3 extra targets at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    spec("grimbargain", "sacrifice", "Grim Bargain", "A killing Bloodpact hit refunds 30% of the HP sacrificed for that use, at rank 1 - +20% per additional rank (70% refunded at 3/3).", Special { at_rank_1: 0.30, per_additional_rank: 0.20 }),
    spec("bloodsac", "sacrifice", "Blood Sacrifice", "Bloodpact's cooldown is reduced by 0.5s per point invested (3.5s at rank 1, down to 2s at rank 3+), so it fires more often across the fight.", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    spec("martyrdom", "sacrifice", "Martyrdom", "Bloodpact instead shields your lowest-HP ally for 15s, worth 150% of the sacrificed HP at rank 1 - +50% per additional rank (250% at 3/3).", Special { at_rank_1: 1.50, per_additional_rank: 0.50 }),
    modifier_with_effect("rot", "festering", "Rot", "Wound stacks also reduce enemy healing received by 10% per rank (up to -30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("contagion", "festering", "Contagion", "A wound has a 25% chance per rank (up to 75% at 3/3) to jump to a new target when its host dies.", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("blooddebt", "festering", "Blood Debt", "Raises max wound stacks by 1 per rank (up to 8 stacks at 3/3, from the base 5).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("overflow", "hemorrhage", "Overflow", "Hemorrhage's explosion also leeches 20% of its damage for the Slayer, per rank (up to 60% at 3/3). This self-leech bypasses the normal leech-per-second cap.", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("arterialspray", "hemorrhage", "Arterial Spray", "The explosion hits 1 additional nearby enemy per rank (up to 3 extra at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    // Key is "hemorrhagesecondwind", not plain "secondwind" - Warrior's own
    // unrelated "Second Wind" modifier already uses that key. Global key
    // uniqueness is required now that Split Personality lets one character
    // hold allocations from two trees in the same flat lookup.
    modifier_with_effect("hemorrhagesecondwind", "hemorrhage", "Second Wind", "A 15% chance per rank (up to 45% at 3/3) for the explosion to fully reset Bloodpact's cooldown.", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("witheringtouch", "necrotic", "Withering Touch", "Also reduces wounded enemies' healing received, invested and scaling separately from Rot - 10% per rank (up to -30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("plaguebearer", "necrotic", "Plague Bearer", "The damage penalty spreads to 1 additional nearby enemy per rank (up to 3 at 3/3).", Special { at_rank_1: 1.0, per_additional_rank: 1.0 }),
    modifier_with_effect("gravechill", "necrotic", "Grave Chill", "Wounded enemies are also slowed by 8% per rank (up to -24% at 3/3).", Special { at_rank_1: 0.08, per_additional_rank: 0.08 }),
    // Unrelenting - approximated as extending the shared expiry window
    // (see the construction-site doc) rather than a true decay-rate
    // change.
    // Migrated 2026-08-25 (drift batch): combat.rs used to extend Blood
    // Frenzy's expiry by `rank * 1333ms`, replaced by a flat 600_000ms at
    // rank 3 (effectively non-decaying for any real fight). Those real
    // per-rank totals are this table now, read straight off the magnitude.
    modifier_with_effect("unrelenting", "bloodfrenzy", "Unrelenting", "Blood Frenzy's attack speed bonus decays 33% slower per rank - stops decaying entirely at 3/3.", SpecialPerRank { values: &[1333.0, 2666.0, 600_000.0] }),
    modifier_with_effect("warcry", "bloodfrenzy", "War Cry", "Blood Frenzy also grants nearby allies +5% attack speed per rank (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    modifier_with_effect("adrenaline", "bloodfrenzy", "Adrenaline", "FlickerStrike's dash hits deal +20% crit damage per rank (up to +60% at 3/3).", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("overflowvessel", "endlessthirst", "Overflow Vessel", "Leech beyond the cap becomes a 5-second shield worth 25% of the overcap amount per rank (up to 75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("insatiable", "endlessthirst", "Insatiable", "Each FlickerStrike hit has a 10% chance per rank (up to 30% at 3/3) to extend Endless Thirst's leech-cap bonus by 2s.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("secondheartbeat", "endlessthirst", "Second Heartbeat", "A 20% chance per rank (up to 60% at 3/3) for a FlickerStrike hit to trigger an immediate bonus dash at one random additional enemy.", Special { at_rank_1: 0.20, per_additional_rank: 0.20 }),
    modifier_with_effect("chainreaper", "reapers", "Chain Reaper", "Each bonus target granted by Reaper's Momentum also heals you for 3% of max HP per rank (up to 9% at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("deathspiral", "reapers", "Death Spiral", "A kill from one of Reaper's Momentum's bonus dash targets heals a flat 4% of max HP per rank (up to 12% at 3/3).", Special { at_rank_1: 0.04, per_additional_rank: 0.04 }),
    // Migrated 2026-08-27 (Stage 3): a COUNT of charges - the real ladder
    // is 1/1/2, not the 1/1.5/2 the linear Special produced.
    modifier_with_effect("undying", "reapers", "Undying", "Can't drop below 1 HP during a FlickerStrike dash - once per fight at rank 1, twice at rank 3.", SpecialPerRank { values: &[1.0, 1.0, 2.0] }),
    modifier_with_effect("bloodforblood", "grimbargain", "Blood for Blood", "Grim Bargain's refund scales up by 2% of the target's max HP per rank (up to +6% at 3/3), on top of its base refund.", Special { at_rank_1: 0.02, per_additional_rank: 0.02 }),
    modifier_with_effect("debtcollector", "grimbargain", "Debt Collector", "A non-lethal Bloodpact hit still refunds 15% of the sacrificed HP per rank (up to 45% at 3/3).", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("cleanslate", "grimbargain", "Clean Slate", "A successful Grim Bargain refund has a 25% chance per rank (up to 75% at 3/3) to also fully reset Bloodpact's cooldown.", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    modifier_with_effect("triage", "bloodsac", "Triage", "Each prior Bloodpact use this fight discounts the NEXT use's HP cost by 3% per rank (up to -9% per prior use at 3/3).", Special { at_rank_1: 0.03, per_additional_rank: 0.03 }),
    modifier_with_effect("warlordsresolve", "bloodsac", "Warlord's Resolve", "Bloodpact's 3rd use also grants the party +5% damage per rank for 10s (up to +15% at 3/3).", Special { at_rank_1: 0.05, per_additional_rank: 0.05 }),
    // Still NotYetImplemented, unlike its 2 sibling modifiers - Bloodpact's
    // 2026-08-16 redesign from a flat per-fight charge count to a real 4s
    // cooldown (see `next_bloodpact_at_ms`'s doc, adventure.rs) means it
    // now fires an unbounded number of times across a fight, so "the LAST
    // use of the fight" is no longer knowable in advance the way "your
    // final banked charge" was - there's nothing to discount at the moment
    // a use actually fires, only in hindsight once the fight's already
    // over. Left deferred rather than approximated.
    // Replaced (2026-08-17) - the old "the LAST use of the fight costs
    // less" premise is structurally unimplementable: nothing in this sim
    // knows a fight's outcome in advance, so "this is the last use" has
    // no real-time signal to key off. New version anchored to Bloodpact's
    // real repeated-cooldown design (see `bloodpact_uses_this_fight`) -
    // once you've used it enough times this fight, every use after that
    // is discounted, same "each PRIOR use" shape Triage already uses one
    // slot over.
    modifier_with_effect(
        "finaloffering",
        "bloodsac",
        "Final Offering",
        "Once you've used Bloodpact enough times this fight (the 4th use at rank 1, 3rd at rank 2, 2nd at rank 3), every use after that costs 33% less HP - combined with Triage's own discount, capped together at 90% off.",
        // Migrated 2026-08-25 (drift batch): combat.rs computed the
        // unlock ladder as `4 - rank` prior uses; this linear table IS
        // those values (3/2/1). The -33% discount itself was never
        // rank-fed (flat at every rank) and stays a named constant at
        // the call site.
        Special { at_rank_1: 3.0, per_additional_rank: -1.0 },
    ),
    modifier_with_effect("guardiansblood", "martyrdom", "Guardian's Blood", "Martyrdom's shield also reflects 10% of absorbed damage per rank (up to 30% at 3/3) back at the attacker.", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("sharedpain", "martyrdom", "Shared Pain", "Shielding an ally also heals you for 25% of the shield's value per rank (up to 75% at 3/3).", Special { at_rank_1: 0.25, per_additional_rank: 0.25 }),
    // Last Rites - see `guardian_spirit_charges`'s construction-site doc:
    // its per-rank CHANCE collapses to "any investment grants one shared
    // party-wide save charge", since the underlying interception check is
    // a deterministic charge count, not a live roll.
    // Migrated 2026-08-27 (Stage 3): this node's CONSUMED value is a
    // COUNT of saves per fight, not the chance the old declaration
    // carried - the shared interception check Guardian Spirit uses is a
    // deterministic charge count, never a live roll, so combat.rs
    // collapsed "invested at all" to one charge and the declared
    // 33/66/100% was read by nothing. Declared as the real 1/1/1 charge
    // count (same "the magnitude carries the primary numeric aspect"
    // convention as shattering/virulence). The description still
    // advertises a chance - flagged in WIKI_IMPACT.md for the owner.
    // Last Rites was described as a CHANCE from the game's first commit
    // (`9f11541`, 2026-08-17) and has never rolled one: the shared death-save
    // it feeds is a deterministic charge count, so the old 0.33/0.665/1.0
    // ladder was read by nothing. Stage 3 (2026-08-27) made the stored values
    // honest at 1/1/1 but left the prose describing the ladder nobody read,
    // which meant the node advertised 33%->100% while ranks 2 and 3 granted
    // nothing at all. 2026-09-02: values and text corrected together.
    // 1/1/2 matches `undying`, the Slayer's OTHER death-save node, exactly,
    // and matches Guardian Spirit's shape (first charge at the working rank,
    // second at max) adapted for a Modifier that must function at rank 1.
    // Every death-save node in the game tops out at 2 charges - see
    // passive_overrides.rs's declared table - and this one is no exception.
    modifier_with_effect("lastrites", "martyrdom", "Last Rites", "Once per fight, prevent a party member's death (yourself included) - they survive the killing blow on 1 HP. Rank 3 grants a second save per fight.", SpecialPerRank { values: &[1.0, 1.0, 2.0] }),
];

// ---------------------------------------------------------------------
// ELEMENTALIST (root: splash, see `Archetype::bonus`) - staged build (see
// ELEMENTALIST_PROGRESS.md at repo root). Stage 1 built the structure
// only, every node `PassiveEffect::NotYetImplemented`, matching this
// file's own established "structure first, wire in effects across
// passes" precedent (see this file's own top-of-file doc for the
// original 11-archetype build history). Stage 2 wired in the Elemental
// Focus branch for real (the skill itself + Shocking/Chilling/Scorching
// Focus + their 9 modifiers - see `combat.rs`'s own doc on
// `shockingfocus_pct`/`conflagration_dmg_pct`/etc for the mechanic).
// Stage 3 wired in Righteous Fire part 1: the skill's own
// damage/self-burn tick, Scorching Flames' fire-damage-pct
// contribution, and its Relentless Flames/Cauterizing Flames/Ashes to
// Ashes modifiers - see `combat.rs`'s `tick_righteous_fire` doc. Stage
// 4 completed the rest of the Righteous Fire branch (Healing Flames/
// Cleansing Flames families). Stage 5 built Golem Master's foundation
// (summon count/damage penalty, Basic golem, the summoner-death rule).
// Stage 6 wired in Thunder/Flame/Water Golem's own 9 modifiers plus
// Thunder Golem's own base behavior (damage redirect/no heal-shield/
// reform) - see `combat.rs`'s `thunder_golem_redirect`/
// `tick_righteous_fire`-adjacent golem-tick docs. Flame Golem/Water
// Golem's own SPEC nodes intentionally stay `NotYetImplemented`
// forever (per the spec's own text, they have no base effect beyond
// unlocking their 3 modifiers) - every other node in this tree is now
// real. All 6 stages complete.
//
// Two node KEYS were renamed from their spec display name to avoid a
// collision with an existing archetype's key (global key uniqueness is
// required - see Split Personality's own doc on why): "Blizzard"
// (Chilling Focus's crit-chance modifier) is keyed `hoarfrost` instead
// (an existing "blizzard" key belongs to another archetype), and
// "Conflagration" (Scorching Focus's increased-damage modifier) is
// keyed `pyroclasm` instead (an existing "conflagration" key likewise
// collides) - both display NAMES stay exactly as spec'd, only the
// internal key differs, same precedent as Warrior's own
// "overwhelmingforce" (display "Overwhelming Force") avoiding
// Berserker's "overwhelm".
//
// Several passives have SPEC-GIVEN irregular (non-linear) per-rank
// progressions - Healing Flames (3/6/10%, not an even step) and
// Blazing (6/9/18%, not an even step) - flagged in their own
// descriptions below. Since every node here is still
// NotYetImplemented, the `at_rank_1`/`per_additional_rank` numeric
// fields aren't consumed yet (the variant carries none); whichever
// later stage wires each of these two real will need a per-rank
// lookup rather than forcing `FlatStat`'s linear formula through the
// irregular ranks - noted here so that stage doesn't have to
// re-discover it.
static ELEMENTALIST_NODES: &[PassiveNode] = &[
    skill(
        "righteousfire",
        "Righteous Fire",
        "Deals damage equal to 10% of your maximum health to a number of enemies based on splash, at rank 1 - +10% per additional rank (30% at 3/3). While active, you take 10% of your health as damage per second at rank 1 - +10% per additional rank (30% at 3/3).",
        PassiveEffect::Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    skill(
        "elementalfocus",
        "Elemental Focus",
        "Gain 5% additive elemental damage (lightning/cold/fire) × your character level, at rank 1 - +5% per additional rank (15% at 3/3). Applies in full to each element separately.",
        PassiveEffect::Special { at_rank_1: 0.05, per_additional_rank: 0.05 },
    ),
    skill(
        "golemmaster",
        "Golem Master",
        "Grants the ability to summon 1 golem at rank 1 - +1 per additional rank (3 golems at 3/3). Golems are built from your whole build with their base stats at 33% of yours; your own damage is unaffected by how many you have out.",
        // A COUNT (1/2/3) - read via `passive_node_count` at every call
        // site since the 2026-08-25 drift batch (spawn, slot-unlock
        // validation and the admin-page picker all read the same count
        // now; it equals the rank at every default rank).
        //
        // DESCRIPTION CORRECTED 2026-09-03 (advertised-vs-actual sweep).
        // It said "you deal 33% less damage per summoned golem, additive
        // (1% of normal damage at 3 golems)". That penalty was deleted on
        // 2026-08-20 - see the two statements of it in combat.rs
        // ("the golem summon damage penalty was removed entirely" at the
        // golem-spawn pass, and "The penalty no longer exists" on
        // `golem_per_hit_tracks_the_owner...`). Proven by consumption
        // rather than by comment: all four
        // `passive_node_count("golemmaster")` sites (combat.rs's spawn
        // loop, manager.rs's two slot-unlock checks, adventure_web.rs's
        // picker) are SLOT COUNTS, and no damage scaling keyed to golem
        // count exists anywhere in combat.rs or character.rs.
        //
        // It mattered more than a normal stale line because it inverted
        // the allocation decision BEFORE play: as written the node turned
        // a maxed class mechanic into a 99% self-nerf, so a player who
        // believed it took one rank or none. wiki/golems.md already
        // described the corrected behaviour ("Everything else you have
        // inherits at FULL value"), so the game contradicted itself.
        PassiveEffect::Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    spec(
        "healingflames",
        "righteousfire",
        "Healing Flames",
        "Regenerate 3% of your health per second at rank 1, 6% at rank 2, 10% at rank 3 (irregular scaling - see this file's own note above).",
        // Migrated 2026-08-25 (drift batch): the irregular 3/6/10%
        // progression used to live only in combat.rs's
        // `healing_flames_regen_pct(rank)` lookup (deleted); this
        // SpecialPerRank table IS that progression now, and the call
        // site reads it via `passive_node_magnitude` like every other
        // node. Rank 4 (Specialization) floors at rank 3's row.
        PassiveEffect::SpecialPerRank { values: &[0.03, 0.06, 0.10] },
    ),
    spec(
        "cleansingflames",
        "righteousfire",
        "Cleansing Flames",
        "33% chance every 4 seconds to remove all debuffs from yourself and nearby allies at rank 1, 66% at rank 2, 100% at rank 3 - target count based on splash.",
        PassiveEffect::Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    spec(
        "scorchingflames",
        "righteousfire",
        "Scorching Flames",
        "Gain 10% additive fire damage × your character level, at rank 1 - +10% per additional rank (30% at 3/3).",
        PassiveEffect::Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    spec(
        "shockingfocus",
        "elementalfocus",
        "Shocking Focus",
        "You apply lightning damage debuffs 33% more frequently at rank 1, 66% at rank 2, 100% at rank 3.",
        PassiveEffect::Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    spec(
        "chillingfocus",
        "elementalfocus",
        "Chilling Focus",
        "You apply cold damage debuffs 33% more frequently at rank 1, 66% at rank 2, 100% at rank 3.",
        PassiveEffect::Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    spec(
        "scorchingfocus",
        "elementalfocus",
        "Scorching Focus",
        "You apply fire damage debuffs 33% more frequently at rank 1, 66% at rank 2, 100% at rank 3.",
        PassiveEffect::Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    spec(
        "thundergolem",
        "golemmaster",
        "Thunder Golem",
        "Absorbs all externally-sourced damage the party would take until it dies (cannot be shielded or healed by any means) - reforms 4 seconds after dying at rank 1, 3 seconds at rank 2, 2 seconds at rank 3, then rejoins combat.",
        // Reform delay counts DOWN (4/3/2s) but is still perfectly linear
        // (at_rank_1=4.0, per_additional_rank=-1.0) - the Special formula
        // fits directly, in SECONDS (multiplied by 1000 at the real call
        // site to get ms, same convention as `chakraoflife_duration_ms`).
        PassiveEffect::Special { at_rank_1: 4.0, per_additional_rank: -1.0 },
    ),
    spec(
        "flamegolem",
        "golemmaster",
        "Flame Golem",
        "All of your increases to elemental damage are multiplicatively increased by 1.33x at rank 1, 1.66x at rank 2, 2.0x at rank 3 - this multiplied scaling applies to your Flame Golems as well, via inheritance.",
        // Elementalist rework item 4 (2026-08-19) - Flame Golem gained a
        // real base effect (previously NotYetImplemented, "identity is
        // its modifiers" per the original spec). 1.33/1.66/2.0 is the
        // same irregular-Special idiom as Growing/Terrifying/Volcanic
        // Ash: at_rank_1 and rank_3 land exactly on the round prose
        // values, rank_2 lands at 1.665 (described as "1.66x").
        PassiveEffect::Special { at_rank_1: 1.33, per_additional_rank: 0.335 },
    ),
    spec(
        "watergolem",
        "golemmaster",
        "Water Golem",
        "Water Golems regenerate 3% of their own max health per second to ALL party members at rank 1, 6% at rank 2, 9% at rank 3 (non-stacking across multiple Water Golems - highest rank applies once).",
        // Elementalist rework item 6 (2026-08-19) - Water Golem gained a
        // real base effect (previously NotYetImplemented, same as
        // Flame Golem above). Exact linear 3/6/9%, no irregular scaling
        // needed.
        PassiveEffect::Special { at_rank_1: 0.03, per_additional_rank: 0.03 },
    ),
    modifier_with_effect(
        "fanningflames",
        "healingflames",
        "Fanning Flames",
        "Share 33% of your Healing Flames regeneration with nearby allies at rank 1, 66% at rank 2, 100% at rank 3 - target count based on splash.",
        Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    modifier_with_effect(
        "risingphoenix",
        "healingflames",
        "Rising Phoenix",
        "When nearby allies die, up to 1 of them revives and rejoins the battle 1 second after death at rank 1 - +1 per additional rank (3 at 3/3, a per-combat limit). Only applies to allies that had survived at least 3 seconds.",
        // A COUNT (1/2/3) - read via `passive_node_count` at the real
        // call site since the 2026-08-25 drift batch (was
        // `passive_node_rank(...).min(3)`; the count equals the rank at
        // every default rank, and effective_rank already caps a node's
        // own growth at 3 points), same as
        // `onehundredhands_bonus_stacks`'s own precedent.
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect(
        "shieldingflames",
        "healingflames",
        "Shielding Flames",
        "33% of your Healing Flames regeneration is also added as a shield on you at rank 1, 66% at rank 2, 100% at rank 3 - in addition to the healing.",
        Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    modifier_with_effect(
        "enshroudedfire",
        "cleansingflames",
        "Enshrouded Fire",
        "Grants a number of allies (based on splash) 3% multiplicative evasion at rank 1 - +3% per additional rank (9% at 3/3).",
        Special { at_rank_1: 0.03, per_additional_rank: 0.03 },
    ),
    modifier_with_effect(
        "guardianfire",
        "cleansingflames",
        "Guardian Fire",
        "Grants a number of allies (based on splash) 3% multiplicative reduced damage taken at rank 1 - +3% per additional rank (9% at 3/3).",
        Special { at_rank_1: 0.03, per_additional_rank: 0.03 },
    ),
    modifier_with_effect(
        "shieldingfire",
        "cleansingflames",
        "Shielding Fire",
        "Grants a number of allies (based on splash) improved block: blocked attacks reduce damage by 55% at rank 1, 60% at rank 2, 65% at rank 3, instead of the standard 50%.",
        Special { at_rank_1: 0.55, per_additional_rank: 0.05 },
    ),
    modifier_with_effect(
        "relentlessflames",
        "scorchingflames",
        "Relentless Flames",
        "A number of nearby enemies (based on splash) take 1% increased damage per second for every second they remain in your presence at rank 1 - +1% per additional rank (3% at 3/3), stacking.",
        Special { at_rank_1: 0.01, per_additional_rank: 0.01 },
    ),
    modifier_with_effect(
        "cauterizingflames",
        "scorchingflames",
        "Cauterizing Flames",
        "A number of nearby enemies (based on splash) receive 5% multiplicative reduced healing at rank 1 - +5% per additional rank (15% at 3/3).",
        Special { at_rank_1: 0.05, per_additional_rank: 0.05 },
    ),
    modifier_with_effect(
        "ashestoashes",
        "scorchingflames",
        "Ashes to Ashes, Dust to Dust",
        "Any enemy in range, including bosses, instantly bursts into flame and dies when its health drops below 100% of your health at rank 1 - +100% per additional rank (300% at 3/3).",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect("overshock", "shockingfocus", "Overshock", "15% more lightning damage at rank 1 - +15% per additional rank (45% at 3/3), scaling from lightning damage on your gear.", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    modifier_with_effect("electricaloverload", "shockingfocus", "Electrical Overload", "Gain 10% more critical strike damage at rank 1 - +10% per additional rank (30% at 3/3).", FlatStat { stat: CritMultiplier, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("lightningaegis", "shockingfocus", "Lightning Aegis", "Gain 1% of your health as shield every time you apply a lightning debuff at rank 1 - +1% per additional rank (3% at 3/3).", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    modifier_with_effect("polarflux", "chillingfocus", "Polar Flux", "15% more cold damage at rank 1 - +15% per additional rank (45% at 3/3), scaling from cold damage on your gear.", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Key "hoarfrost", not "blizzard" - see this file's own top-of-section
    // note on the rename (an existing "blizzard" key collides).
    modifier_with_effect("hoarfrost", "chillingfocus", "Blizzard", "Gain 10% more critical strike chance at rank 1 - +10% per additional rank (30% at 3/3).", FlatStat { stat: CritChance, at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("chillingaegis", "chillingfocus", "Chilling Aegis", "Gain 1% of your health as shield every time you apply a cold debuff at rank 1 - +1% per additional rank (3% at 3/3).", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    modifier_with_effect("incinerate", "scorchingfocus", "Incinerate", "15% more fire damage at rank 1 - +15% per additional rank (45% at 3/3), scaling from fire damage on your gear.", Special { at_rank_1: 0.15, per_additional_rank: 0.15 }),
    // Key "pyroclasm", not "conflagration" - see this file's own
    // top-of-section note on the rename (an existing "conflagration" key
    // collides).
    modifier_with_effect("pyroclasm", "scorchingfocus", "Conflagration", "Gain 10% multiplicative increased damage at rank 1 - +10% per additional rank (30% at 3/3).", Special { at_rank_1: 0.10, per_additional_rank: 0.10 }),
    modifier_with_effect("scorchingaegis", "scorchingfocus", "Scorching Aegis", "Gain 1% of your health as shield every time you apply a fire debuff at rank 1 - +1% per additional rank (3% at 3/3).", Special { at_rank_1: 0.01, per_additional_rank: 0.01 }),
    modifier_with_effect(
        "gigantify",
        "thundergolem",
        "Gigantify",
        "Thunder Golems get 100% more contribution from your health pool at rank 1 - +100% per additional rank (300% at 3/3) - base 33% of your health becomes 66/99/132%.",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect(
        "growing",
        "thundergolem",
        "Growing",
        "Thunder Golems gain 33% more maximum health each time they reform at rank 1, 66% at rank 2, 100% at rank 3 - stacking within a combat.",
        Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    modifier_with_effect(
        "terrifying",
        "thundergolem",
        "Terrifying",
        "When a Thunder Golem dies, it explodes dealing 33% of its health as damage to enemies at rank 1, 66% at rank 2, 100% at rank 3.",
        Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    modifier_with_effect(
        "volcanicash",
        "flamegolem",
        "Volcanic Ash",
        "Flame Golems inherit 33% of your multiplicative increased fire damage at rank 1, 66% at rank 2, 100% at rank 3.",
        Special { at_rank_1: 0.33, per_additional_rank: 0.335 },
    ),
    modifier_with_effect(
        "blazing",
        "flamegolem",
        "Blazing",
        "Flame Golems gain 6% multiplicative attack speed at rank 1, 9% at rank 2, 18% at rank 3 (irregular scaling - see this file's own note above).",
        // Migrated 2026-08-25 (drift batch): same shape as Healing
        // Flames - the irregular 6/9/18% table moved here from combat.rs's
        // `blazing_attack_speed_pct(rank)` lookup (deleted).
        SpecialPerRank { values: &[0.06, 0.09, 0.18] },
    ),
    modifier_with_effect(
        "surging",
        "flamegolem",
        "Surging",
        "Flame Golems deal 10% multiplicative damage at rank 1 - +10% per additional rank (30% at 3/3).",
        Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    modifier_with_effect(
        "replenishing",
        "watergolem",
        "Replenishing",
        "Water Golems convert all damage they deal into healing for the party at a 100% rate at rank 1 - +100% per additional rank (300% at 3/3).",
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
    modifier_with_effect(
        "singing",
        "watergolem",
        "Singing",
        "All allies gain 10% more effect from shields and heals applied to them at rank 1 - +10% per additional rank (30% at 3/3).",
        Special { at_rank_1: 0.10, per_additional_rank: 0.10 },
    ),
    modifier_with_effect(
        "shattering",
        "watergolem",
        "Shattering",
        "When an enemy dies in the Water Golem's presence, it explodes, sending icicles at (splash + 1) nearby enemies at rank 1 - +1 per additional rank (splash + 3 at 3/3), each dealing damage equal to 1% of the dead enemy's health.",
        // This node's PRIMARY value is target count (2026-08-20 revised
        // convention - see `LiveTunables`'s own doc for the general
        // rule: a node's magnitude carries its primary numeric aspect,
        // additional aspects get named per-rank LiveTunables). A COUNT
        // (1/2/3, added to splash's own target count). Migrated
        // 2026-08-27 (Stage 3): the spawn site read `passive_node_rank`
        // directly until now, which made this declared table inert - it
        // reads `passive_node_count` off THIS table instead, so an admin
        // retuning the node genuinely moves the icicle target count. The
        // icicle's damage basis is a SEPARATE aspect, no longer read
        // from this node at all - see `LiveTunables::shattering_damage_pct_rank1`'s
        // own doc.
        Special { at_rank_1: 1.0, per_additional_rank: 1.0 },
    ),
];

/// Points-per-level formula - 1 point from the start, +1 every 4 levels
/// (2026-08-16, tightened from every 5 per a live request). Originally
/// had a level-10 delay before the first point at all, dropped 2026-08-15
/// so a fresh character already has their first point to spend instead of
/// waiting.
pub fn points_for_level(level: u32) -> u32 {
    1 + level / 4
}

/// The single source of truth for "is this rank legal for this node,
/// given these other allocations in the same tree" - node existence,
/// `max_rank`, and the parent/`unlock_at` prerequisite gate.
///
/// Extracted (2026-08-19, the Memories feature) from
/// `AdventureManager::preview_allocate_passive`, which had been the only
/// place these rules existed and now calls this instead. The extraction
/// is what lets a Memory load replay a stored build through the EXACT
/// rules a live click obeys (see `adventure::memory::replay_snapshot`)
/// rather than a second copy of them that could drift - "a Memory can
/// never produce a tree state the normal UI couldn't have built" is only
/// worth asserting if there is one implementation of "could have built".
///
/// `side` is the allocation map this rank would land in - the caller's
/// pending preview for a live click, or the partially-replayed tree for
/// a Memory load. The prerequisite is checked against THAT map rather
/// than against the character's saved tree, which is what makes a
/// same-request allocate-parent-then-child work (and, for a replay, what
/// makes tier-ordered rebuilding cascade correctly).
///
/// The point BUDGET is deliberately not checked here: it is
/// character-scoped (`Character::total_passive_points`, one pool shared
/// across both trees) rather than a property of a node, and both callers
/// enforce it themselves over the whole resulting spend.
pub fn validate_allocation_step(
    nodes: &'static [PassiveNode],
    side: &std::collections::HashMap<String, u32>,
    node_key: &str,
    new_rank: u32,
) -> Result<(), crate::adventure::PassiveError> {
    use crate::adventure::PassiveError;

    let node = nodes.iter().find(|n| n.key == node_key).ok_or(PassiveError::NodeNotFound)?;
    if new_rank > node.max_rank {
        return Err(PassiveError::MaxRankReached);
    }
    // Rank 0 is "not allocated" - it has no prerequisite to satisfy, and
    // de-allocating a node whose parent is already gone must stay legal.
    if new_rank > 0 {
        if let Some(parent_key) = node.parent {
            let parent_rank = side.get(parent_key).copied().unwrap_or(0);
            let required = node.unlock_at.unwrap_or(1);
            if parent_rank < required {
                return Err(PassiveError::ParentNotInvested);
            }
        }
    }
    Ok(())
}

/// Stage 1 of the Elementalist build (docs/elementalist_spec.md,
/// ELEMENTALIST_PROGRESS.md) - the first structural validation this file
/// has ever had for ANY archetype's tree, not just the new one. Written
/// generically over `ALL_ARCHETYPES` on purpose: it protects every
/// existing archetype's tree from a future mistake too, not just
/// Elementalist's.
#[cfg(test)]
mod tree_shape_tests {
    use super::*;
    use crate::adventure::ALL_ARCHETYPES;
    use std::collections::HashSet;

    /// Every archetype's own node list, plus Commoner's (always empty) -
    /// `ALL_ARCHETYPES` itself deliberately excludes Commoner (see its
    /// own doc: "never a manual target"), but its tree is still part of
    /// `Archetype::passive_nodes`'s exhaustive match and worth including
    /// here for completeness.
    fn every_archetype_nodes() -> Vec<(Archetype, &'static [PassiveNode])> {
        let mut all: Vec<Archetype> = ALL_ARCHETYPES.to_vec();
        all.push(Archetype::Commoner);
        all.into_iter().map(|a| (a, a.passive_nodes())).collect()
    }

    #[test]
    fn every_node_key_is_globally_unique_across_every_archetype() {
        let mut seen: HashSet<&'static str> = HashSet::new();
        for (archetype, nodes) in every_archetype_nodes() {
            for node in nodes {
                assert!(seen.insert(node.key), "duplicate key {:?} - first collision found while checking {archetype:?}", node.key);
            }
        }
    }

    // The three tests below are scoped to `Archetype::Elementalist`
    // specifically, NOT every archetype - an earlier draft checked all
    // 12 and found a pre-existing exception in Monk's tree (the
    // "windwalker" modifier is parented directly to the "flowingstrikes"
    // SKILL, not to one of its 3 Specialization children - see
    // `MONK_NODES`'s own Flowing Strikes redesign history above). That's
    // unrelated, already-shipped, live content this feature has no
    // business touching - fixing or working around it is out of scope
    // for adding a new class. Asserting a general invariant that's
    // already known not to hold everywhere would just be a test that's
    // wrong about the codebase it's testing, so these stay scoped to the
    // one tree this stage actually owns. Key uniqueness above still
    // checks all 12, since that check doesn't assume anything about
    // another archetype's internal shape.

    #[test]
    fn elementalist_every_skill_has_no_parent_and_max_rank_3() {
        for node in Archetype::Elementalist.passive_nodes().iter().filter(|n| matches!(n.tier, PassiveTier::Skill)) {
            assert!(node.parent.is_none(), "skill {:?} must have no parent", node.key);
            assert_eq!(node.max_rank, 3, "skill {:?} must cap at rank 3", node.key);
            assert!(node.unlock_at.is_none(), "skill {:?} must have no unlock_at - it's never gated", node.key);
        }
    }

    #[test]
    fn elementalist_every_specialization_has_a_skill_parent_and_max_rank_4() {
        let nodes = Archetype::Elementalist.passive_nodes();
        for node in nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Specialization)) {
            let parent_key = node.parent.unwrap_or_else(|| panic!("spec {:?} must have a parent", node.key));
            let parent = nodes.iter().find(|n| n.key == parent_key).unwrap_or_else(|| panic!("spec {:?} points at missing parent {parent_key:?}", node.key));
            assert!(matches!(parent.tier, PassiveTier::Skill), "spec {:?}'s parent {parent_key:?} must be a Skill", node.key);
            assert_eq!(node.max_rank, 4, "spec {:?} must cap at rank 4 (3 real + 1 unlock-only)", node.key);
        }
    }

    #[test]
    fn elementalist_every_modifier_has_a_specialization_parent_gated_at_4_and_max_rank_3() {
        let nodes = Archetype::Elementalist.passive_nodes();
        for node in nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Modifier)) {
            let parent_key = node.parent.unwrap_or_else(|| panic!("modifier {:?} must have a parent", node.key));
            let parent = nodes.iter().find(|n| n.key == parent_key).unwrap_or_else(|| panic!("modifier {:?} points at missing parent {parent_key:?}", node.key));
            assert!(matches!(parent.tier, PassiveTier::Specialization), "modifier {:?}'s parent {parent_key:?} must be a Specialization", node.key);
            assert_eq!(node.max_rank, 3, "modifier {:?} must cap at rank 3", node.key);
            assert_eq!(node.unlock_at, Some(4), "modifier {:?} must be gated on its parent hitting 4/4", node.key);
        }
    }

    #[test]
    fn elementalist_tree_has_exactly_3_skills_9_specs_27_modifiers() {
        let nodes = Archetype::Elementalist.passive_nodes();
        let skills = nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Skill)).count();
        let specs = nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Specialization)).count();
        let modifiers = nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Modifier)).count();
        assert_eq!(skills, 3, "3 base passives");
        assert_eq!(specs, 9, "3 specializations per base passive");
        assert_eq!(modifiers, 27, "3 modifiers per specialization");
        assert_eq!(nodes.len(), 39);
    }

    #[test]
    fn elementalist_every_specialization_has_exactly_3_modifier_children() {
        let nodes = Archetype::Elementalist.passive_nodes();
        for spec in nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Specialization)) {
            let child_count = nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Modifier) && n.parent == Some(spec.key)).count();
            assert_eq!(child_count, 3, "specialization {:?} must have exactly 3 modifier children", spec.key);
        }
    }

    #[test]
    fn elementalist_every_skill_has_exactly_3_specialization_children() {
        let nodes = Archetype::Elementalist.passive_nodes();
        for skill in nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Skill)) {
            let child_count = nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Specialization) && n.parent == Some(skill.key)).count();
            assert_eq!(child_count, 3, "skill {:?} must have exactly 3 specialization children", skill.key);
        }
    }

    /// Every node in this list has a real effect by Stage 6 (the final
    /// stage) - everything else stays `NotYetImplemented` FOREVER by
    /// design. "flamegolem"/"watergolem" originally had no base effect
    /// of their own beyond their 3 modifiers each - the Elementalist
    /// rework (2026-08-19, items 4/6) gave both a real base effect
    /// (Flame Golem's owner+golem elemental multiplier, Water Golem's
    /// party regen), so they're included below like every other node
    /// with a real effect.
    const IMPLEMENTED_BY_STAGE_6: &[&str] = &[
        // Stage 2 - Elemental Focus branch.
        "elementalfocus",
        "shockingfocus",
        "chillingfocus",
        "scorchingfocus",
        "overshock",
        "electricaloverload",
        "lightningaegis",
        "polarflux",
        "hoarfrost",
        "chillingaegis",
        "incinerate",
        "pyroclasm",
        "scorchingaegis",
        // Stage 3 - Righteous Fire part 1.
        "righteousfire",
        "scorchingflames",
        "relentlessflames",
        "cauterizingflames",
        "ashestoashes",
        // Stage 4 - Righteous Fire part 2 (the rest of the branch).
        "healingflames",
        "fanningflames",
        "risingphoenix",
        "shieldingflames",
        "cleansingflames",
        "enshroudedfire",
        "guardianfire",
        "shieldingfire",
        // Stage 5/6 - Golem Master branch.
        "golemmaster",
        "thundergolem",
        "gigantify",
        "growing",
        "terrifying",
        "flamegolem",
        "volcanicash",
        "blazing",
        "surging",
        "watergolem",
        "replenishing",
        "singing",
        "shattering",
    ];
    #[test]
    fn elementalist_stage_6_implemented_rest_not_yet_forever() {
        let nodes = Archetype::Elementalist.passive_nodes();
        for node in nodes {
            if IMPLEMENTED_BY_STAGE_6.contains(&node.key) {
                assert!(!matches!(node.effect, PassiveEffect::NotYetImplemented), "{:?} should have a real effect by Stage 6", node.key);
            } else {
                assert!(matches!(node.effect, PassiveEffect::NotYetImplemented), "{:?} should be NotYetImplemented - not yet wired", node.key);
            }
        }
    }

    #[test]
    fn elementalist_elemental_focus_grants_5pct_per_rank_read_via_special() {
        assert_eq!(node_by_key(Archetype::Elementalist, "elementalfocus").magnitude_at_rank(1), 0.05);
        assert!((node_by_key(Archetype::Elementalist, "elementalfocus").magnitude_at_rank(3) - 0.15).abs() < 1e-9);
    }

    #[test]
    fn elementalist_focus_specs_reach_100pct_at_max_rank() {
        for key in ["shockingfocus", "chillingfocus", "scorchingfocus"] {
            let node = node_by_key(Archetype::Elementalist, key);
            assert_eq!(node.magnitude_at_rank(1), 0.33, "{key} rank 1");
            // Specialization tier caps effective rank at 3 for magnitude
            // purposes (the 4th point only unlocks children) - see
            // `magnitude_at_rank`'s own doc.
            assert!((node.magnitude_at_rank(4) - 1.0).abs() < 1e-9, "{key} at 4/4 should read as 3/3 (1.0)");
        }
    }

    #[test]
    fn elementalist_crit_modifiers_feed_the_generic_crit_pool() {
        let electrical_overload = node_by_key(Archetype::Elementalist, "electricaloverload");
        assert!(matches!(electrical_overload.effect, PassiveEffect::FlatStat { stat: PassiveStat::CritMultiplier, .. }));
        let blizzard = node_by_key(Archetype::Elementalist, "hoarfrost");
        assert!(matches!(blizzard.effect, PassiveEffect::FlatStat { stat: PassiveStat::CritChance, .. }));
    }

    #[test]
    fn elementalist_aegis_modifiers_reach_3pct_shield_at_max_rank() {
        for key in ["lightningaegis", "chillingaegis", "scorchingaegis"] {
            let node = node_by_key(Archetype::Elementalist, key);
            assert!((node.magnitude_at_rank(3) - 0.03).abs() < 1e-9, "{key} at 3/3");
        }
    }

    #[test]
    fn elementalist_righteous_fire_reaches_30pct_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "righteousfire");
        assert_eq!(node.magnitude_at_rank(1), 0.10);
        assert!((node.magnitude_at_rank(3) - 0.30).abs() < 1e-9);
    }

    #[test]
    fn elementalist_scorching_flames_reaches_30pct_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "scorchingflames");
        assert_eq!(node.magnitude_at_rank(1), 0.10);
        assert!((node.magnitude_at_rank(3) - 0.30).abs() < 1e-9);
    }

    #[test]
    fn elementalist_relentless_flames_reaches_3pct_per_stack_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "relentlessflames");
        assert!((node.magnitude_at_rank(3) - 0.03).abs() < 1e-9);
    }

    #[test]
    fn elementalist_cauterizing_flames_reaches_15pct_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "cauterizingflames");
        assert!((node.magnitude_at_rank(3) - 0.15).abs() < 1e-9);
    }

    #[test]
    fn elementalist_ashes_to_ashes_reaches_300pct_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "ashestoashes");
        assert_eq!(node.magnitude_at_rank(1), 1.0);
        assert!((node.magnitude_at_rank(3) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn elementalist_fanning_flames_and_shielding_flames_reach_100pct_at_max_rank() {
        for key in ["fanningflames", "shieldingflames", "cleansingflames"] {
            let node = node_by_key(Archetype::Elementalist, key);
            assert_eq!(node.magnitude_at_rank(1), 0.33, "{key} rank 1");
            assert!((node.magnitude_at_rank(3) - 1.0).abs() < 1e-9, "{key} at 3/3");
        }
    }

    #[test]
    fn elementalist_rising_phoenix_reaches_3_revives_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "risingphoenix");
        assert_eq!(node.magnitude_at_rank(1), 1.0);
        assert!((node.magnitude_at_rank(3) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn elementalist_enshrouded_and_guardian_fire_reach_9pct_at_max_rank() {
        for key in ["enshroudedfire", "guardianfire"] {
            let node = node_by_key(Archetype::Elementalist, key);
            assert!((node.magnitude_at_rank(3) - 0.09).abs() < 1e-9, "{key} at 3/3");
        }
    }

    #[test]
    fn elementalist_shielding_fire_reaches_65pct_block_reduction_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "shieldingfire");
        assert_eq!(node.magnitude_at_rank(1), 0.55);
        assert!((node.magnitude_at_rank(3) - 0.65).abs() < 1e-9);
    }

    #[test]
    fn elementalist_golem_master_reaches_3_golems_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "golemmaster");
        assert_eq!(node.magnitude_at_rank(1), 1.0);
        assert!((node.magnitude_at_rank(3) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn elementalist_thunder_golem_reform_delay_counts_down_4_3_2_seconds() {
        let node = node_by_key(Archetype::Elementalist, "thundergolem");
        assert_eq!(node.magnitude_at_rank(1), 4.0);
        assert!((node.magnitude_at_rank(2) - 3.0).abs() < 1e-9);
        assert!((node.magnitude_at_rank(3) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn elementalist_gigantify_reaches_300pct_at_max_rank() {
        let node = node_by_key(Archetype::Elementalist, "gigantify");
        assert_eq!(node.magnitude_at_rank(1), 1.0);
        assert!((node.magnitude_at_rank(3) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn elementalist_growing_and_terrifying_and_volcanicash_reach_100pct_at_max_rank() {
        for key in ["growing", "terrifying", "volcanicash"] {
            let node = node_by_key(Archetype::Elementalist, key);
            assert_eq!(node.magnitude_at_rank(1), 0.33, "{key} rank 1");
            assert!((node.magnitude_at_rank(3) - 1.0).abs() < 1e-9, "{key} at 3/3");
        }
    }

    #[test]
    fn elementalist_surging_and_replenishing_and_singing_reach_30_or_300pct_at_max_rank() {
        let surging = node_by_key(Archetype::Elementalist, "surging");
        assert!((surging.magnitude_at_rank(3) - 0.30).abs() < 1e-9);
        let singing = node_by_key(Archetype::Elementalist, "singing");
        assert!((singing.magnitude_at_rank(3) - 0.30).abs() < 1e-9);
        let replenishing = node_by_key(Archetype::Elementalist, "replenishing");
        assert_eq!(replenishing.magnitude_at_rank(1), 1.0);
        assert!((replenishing.magnitude_at_rank(3) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn elementalist_shattering_reaches_3_extra_targets_at_max_rank() {
        // Target count is this node's PRIMARY value (2026-08-20 revised
        // convention) - read via `passive_node_rank` at the real call
        // site, not this magnitude table directly, but shipped matching
        // it 1:1 for display/tooltip consistency. Damage pct lives on
        // its own named LiveTunables now, not this node at all.
        let node = node_by_key(Archetype::Elementalist, "shattering");
        assert_eq!(node.magnitude_at_rank(1), 1.0);
        assert!((node.magnitude_at_rank(3) - 3.0).abs() < 1e-9);
    }

    fn node_by_key(archetype: Archetype, key: &str) -> &'static PassiveNode {
        archetype.passive_nodes().iter().find(|n| n.key == key).unwrap_or_else(|| panic!("no node with key {key:?}"))
    }
}

/// Shatter's ladder (2026-09-04). What these pin is the reason the values
/// are what they are: rank 1 is untouched, and no rank is spent against
/// the block clamp.
#[cfg(test)]
mod shatter_ladder_tests {
    use super::*;

    /// The reference configuration the ladder was sized against: the
    /// Berserker's own end state. Overwhelm 3/3 is 0.09 of damage
    /// reduction shred per Bloodlust stack, Bloodlust caps at 5 stacks,
    /// and a boss's block chance is pinned at `BOSS_DEFENSE_CAP` 0.75.
    const OVERWHELM_SHRED_PER_STACK_AT_3_3: f64 = 0.09;
    const BLOODLUST_MAX_STACKS: f64 = 5.0;
    const BOSS_BLOCK_AT_CAP: f64 = 0.75;

    fn shatter() -> &'static PassiveNode {
        Archetype::Berserker.passive_nodes().iter().find(|n| n.key == "shatter").expect("the Berserker tree must still carry shatter")
    }

    /// Constraint 1 of the owner's ruling: anyone already holding one
    /// point keeps exactly what they had. The ladder goes UP from 1.0,
    /// never down to it.
    #[test]
    fn rank_1_is_never_nerfed_and_the_ladder_only_rises() {
        let n = shatter();
        assert_eq!(n.magnitude_at_rank(1), 1.0, "rank 1 must stay at 100% of Overwhelm's shred - changing it is a silent nerf to existing allocations");
        assert!(n.magnitude_at_rank(2) > n.magnitude_at_rank(1), "rank 2 must buy something - a flat rung is the defect this ladder replaces");
        assert!(n.magnitude_at_rank(3) > n.magnitude_at_rank(2), "rank 3 must buy something");
    }

    /// Constraint 2, and the whole reason rank 3 is 1.65 rather than a
    /// round 2.0. Block is clamped at the roll and there is no relative
    /// floor protecting the defender from Shatter, so block reaches zero
    /// at `0.75 / 0.45` = 1.667. Any rank at or above that is fully
    /// absorbed by the clamp in exactly the configuration a maxed
    /// Berserker plays in - a ladder scaling into a cap is the same
    /// defect wearing a new number.
    #[test]
    fn no_rank_is_absorbed_by_the_block_clamp() {
        let shred = OVERWHELM_SHRED_PER_STACK_AT_3_3 * BLOODLUST_MAX_STACKS;
        let saturation = BOSS_BLOCK_AT_CAP / shred;
        assert!((saturation - 1.6666).abs() < 0.001, "sanity: saturation is 0.75/0.45, got {saturation}");
        let n = shatter();
        for rank in 1..=n.max_rank {
            let mult = n.magnitude_at_rank(rank);
            let remaining_block = BOSS_BLOCK_AT_CAP - shred * mult;
            assert!(
                remaining_block > 0.0,
                "rank {rank} ({mult}x) drives block to {remaining_block} - at or past the {saturation}x saturation point, so the rank is spent against the clamp rather than on the player"
            );
        }
    }

    /// The marginal point has to be worth taking. A block halves the hit,
    /// so damage through a target scales as `1 - block/2`; this pins that
    /// each rank moves that number by a real amount rather than a
    /// rounding artefact.
    #[test]
    fn each_rank_meaningfully_moves_damage_through_a_blocking_target() {
        let shred = OVERWHELM_SHRED_PER_STACK_AT_3_3 * BLOODLUST_MAX_STACKS;
        let n = shatter();
        let damage_mult = |mult: f64| {
            let block = (BOSS_BLOCK_AT_CAP - shred * mult).max(0.0);
            1.0 - block / 2.0
        };
        let unshattered = damage_mult(0.0);
        let r1 = damage_mult(n.magnitude_at_rank(1));
        let r2 = damage_mult(n.magnitude_at_rank(2));
        let r3 = damage_mult(n.magnitude_at_rank(3));
        assert!((unshattered - 0.625).abs() < 0.001, "sanity: 0.75 block halves damage to 0.625, got {unshattered}");
        // Each step must be worth at least a few percent of the previous,
        // or the point is not worth spending.
        assert!(r1 / unshattered > 1.30, "rank 1 must be a large jump, got {:.3}x", r1 / unshattered);
        assert!(r2 / r1 > 1.05, "rank 2 must buy a real gain over rank 1, got {:.3}x", r2 / r1);
        assert!(r3 / r2 > 1.05, "rank 3 must buy a real gain over rank 2, got {:.3}x", r3 / r2);
    }

    /// The ladder is non-linear (deltas 0.35 then 0.30), so it cannot be
    /// expressed as `Special { at_rank_1, per_additional_rank }` and must
    /// stay a `SpecialPerRank`. This fails if someone "simplifies" it.
    #[test]
    fn the_ladder_is_non_linear_and_must_stay_a_per_rank_table() {
        let n = shatter();
        let d1 = n.magnitude_at_rank(2) - n.magnitude_at_rank(1);
        let d2 = n.magnitude_at_rank(3) - n.magnitude_at_rank(2);
        assert!((d1 - d2).abs() > 1e-9, "the ladder became linear ({d1} then {d2}); if that is deliberate say so, but a linear table hides that rank 3 was sized against the clamp");
    }

    /// The copy must state the multipliers, because "by the same amount
    /// per rank" is exactly the wording that made this a finding.
    #[test]
    fn the_description_states_the_real_multipliers() {
        let d = shatter().description;
        for needle in ["100%", "135%", "165%"] {
            assert!(d.contains(needle), "Shatter's description must state its actual multipliers - missing {needle:?}: {d}");
        }
        assert!(!d.contains("by the same amount per rank"), "the wording that implied per-rank scaling while the ladder was flat must not come back");
    }
}
