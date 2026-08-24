# Tunable Audit — classes, passives, and skill magnitudes

**Session:** tunables audit (read-only except this file)
**Date:** 2026-08-24
**Repo state audited:** `master @ f8af51c` ("ledger: fixed-step damage control caused the observed oscillation")

Scope legend for **state** column:

| state | meaning |
|---|---|
| (a) | rendered on `/admin/passives`, live-tunable — usable |
| (b) | rendered on `/admin/tunables` — live-tunable but on the wrong page |
| (c) | overridable via `adventure-passive-overrides.toml` but NOT rendered anywhere |
| (d) | hardcoded, no override path |

Type legend: **MAGNITUDE** = how much per unit; **CAP** = bound on output/stacks; **THRESHOLD** = trigger point; **RATE** = per-time / per-proc frequency.

> NOTE: this file is being written incrementally section by section. Empty sections are placeholders until filled.

---

## METHOD & PLUMBING (how a value becomes tunable)

Verified call graph:

- **Override hook:** every numeric read of every tree node funnels through `PassiveNode::magnitude_at_rank` (game/src/passive_tree.rs:505), which consults `passive_override_for(key, effective_rank)` — a process-global `LazyLock<RwLock<PassiveOverrides>>` (adventure/passive_overrides.rs:113) loaded from `adventure-passive-overrides.toml`. Absent entry ⇒ compiled-in value, byte-identical.
- **Bespoke reads:** `Character::passive_node_magnitude` (adventure/character.rs:2681) and `Character::passive_node_count` (:2717, magnitude rounded) are **override-aware**. `Character::passive_node_rank` (:2660) is **structure-only** — an override on a node consumed only via `_rank` never reaches the game. That is the inert-override shape (§3).
- **Generic pooling:** FlatStat nodes sum via `accumulate_flat_stat_bonus` (character.rs:2571-2579) using `magnitude_at_rank` — override-aware; all FlatStat nodes are live-tunable end to end.
- **Overflow conversions:** `accumulate_overflow_conversion_bonus` (character.rs:2630-2644): `raw = overflow × magnitude_at_rank` (**override-aware**), then clamped to `OVERFLOW_CONVERSION_CAP_PER_RANK * rank` — combat.rs:342, `= 0.10`, **hardcoded** (§1/§4).
- **Render/edit surfaces:** `/admin/passives` (adventure_web.rs:2364) renders per archetype: compiled-in default vs current per-rank values, edit form (`PassiveOverrideForm`, :2487) unless `node_is_tunable(key)` is false — pending/unwired lists live in adventure/passive_overrides.rs:282/:288 and render as not-editable. `/admin/tunables` (render :3394, `TunablesForm` :2559) edits `LiveTunables`.
- **Reload semantics today:** `LiveTunables` sits behind an `RwLock` on `AdventureManager`, re-read **every fight** — HOT (tunables.rs:3-16). Passive overrides are a `LazyLock<RwLock<..>>` swapped on save — HOT. `ItemBalanceFile` (adventure-item-balance.toml) is cached behind `OnceLock` per consumer — RESTART-only. §6 applies this per proposed field.

Scale of the surface: **468 real-effect tree nodes** over 12 archetypes; **76 rendered `/admin/tunables` controls**; **~133 compile-time combat constants**.

---

## MASTER TABLE

Columns: **what** = what the number does; **type** = MAGNITUDE/CAP/THRESHOLD/RATE; **d** = shipped default r1/r2/r3 (linear nodes shown `at_rank_1/per_extra`: r2=r1+per, r3=r1+2·per); **live** = current `adventure-passive-overrides.toml` entry (— = none); **site** = declaration line and main consumer. State legend at top of file.

`(c-pending)` = in `PENDING_MIGRATION_NODES` (passive_overrides.rs) — page shows it as not-yet-tunable, offers NO input; declared magnitude is decorative, real number is a rank-keyed ladder/hardcode. `(c-drift)` = in NO repo list — the page OFFERS an editable input and accepts saves, but the only consumer reads `passive_node_rank`, so the override silently does nothing. See §3.


### WARRIOR (passive_tree.rs 628–727)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| bulwark | +block chance | MAG | .08/.14/.20 | — | (a) | :629 pool |
| retaliation | reflect blocked hit's dmg | MAG | .10/.18/.26 | [.33,.66,1.0] | (a) | :630 → cb:11420 |
| juggernaut | +max HP% | MAG | .08/.16/.24 | =d | (a) | :631 → chr:2563 |
| aegis | periodic party shield pct | MAG | .20/.30/.40 | — | (a) | :632; duration const (d) |
| spikebarrier | reflect on block | MAG | .25/.40/.55 | — | (a) | :633 |
| unbreakable | block-overflow→dmg | RATE | .5/.75/1.0 | =d | (a)* | :634 → chr:2639; cap hard 0.10/rank |
| vengeance | dmg back per hit taken | MAG | .20/.35/.50 | [.33,.66,1.0] | (a) | :635 → cb:11421 |
| bloodresolve | heal % of dmg dealt | MAG | .08/.14/.20 | — | (a) | :636 |
| laststand | DR below HP thresh | MAG | .25/.40/.55 | — | (a) | :637 |
| colossus | ×Juggernaut's own bonus | MAG | .50/.75/1.0 | — | (a) | :638 → chr:2563 |
| momentum | stacking AS/hit | RATE | .03/.06/.09 | — | (a) | :639 → cb:11827ff |
| overwhelmingforce | DR→dmg conversion | RATE | .10/.20/.30 | — | (a) | :646 → chr:2476 |
| bastion | Aegis +1s/rank | THR | 1/2/3 s | — | (a) | :647 |
| rally | Aegis grants +AS | MAG | .10/.20/.30 | — | (a) | :648 |
| ironcircle | Aegis extra allies | CAP | 1/2/3 | — | (a) | :649 |
| thornedhide | SpikeBar −dmg debuff | MAG | .05/.10/.15 | — | (a) | :650 |
| retribution | reflect crit chance | MAG | .20/.40/.60 | — | (a) | :651 |
| unyielding | SpikeBar on unblocked | MAG | .10/.20/.30 | — | (a) | :652 |
| fortress | Unbreakable→flat DR | MAG | .02/.04/.06 | — | (a) | :653 pool |
| secondskin | block DR 65/70/75% | MAG | .65/.70/.75 | — | (a) | :654 → cb:11801 |
| stonewall | auto-block first N hits | CAP | 1/2/3 | — | (a) | :655 → cb:11802 |
| grudge | Vengeance+/same-attacker hit | MAG | .05/.10/.15 | — | (a) | :656 → cb:11424 |
| executionersmark | marked target +dmg taken | MAG | .10/.20/.30 | — | (a) | :661 |
| payback | +dmg while marked | MAG | .30/.45/.60 | — | (c-pending) | :662 |
| adrenalsurge | AS buff | MAG | .08/.16/.24 | — | (a) | :667 |
| hardened | flat DR | MAG | .02/.04/.06 | — | (a) | :668 |
| secondwind | heal below threshold | MAG | .50/.65/.80 | — | (c-pending) | :672 |
| defiance | DR per missing-HP step | MAG | .10/.20/.30 | — | (a) | :673 |
| undyingwill | cheat-death charges | CAP | 1/2/3 | — | (c-pending) | :674 → cb:11655 |
| berserkvigor | AS while low HP | MAG | .10/.20/.30 | — | (a) | :675 |
| titansgrip | Jugg×Colossus→dmg layer | RATE | 1/2/3 ×base | — | (a) | :676 → chr:2597 |
| immovable | CC resist | MAG | .15/.30/.45 | — | (a) | :683 |
| reserves | shield from maxHP | MAG | .05/.10/.15 | — | (a) | :684 |
| rampage | +dmg per kill stack | MAG | 2/4/6 % | — | (a) | :685 |
| avalanche | crit dmg vs low HP | MAG | .02/.04/.06 | — | (a) | :686 |
| unstoppable | Momentum stacks +1/rank | CAP | 1/2/3 | — | (a) | :687 → cb:11830 |
| grimresolve | extra DR→dmg | RATE | .05/.10/.15 | — | (a) | :688 → chr:3058 |
| momentousblow | block chance→dmg | RATE | .1667/.3334/.5001 | — | (a) | :689 → chr:3063 |
| onslaught | DR→dmg mult add-on | RATE | .05/.10/.15 | — | (a) | :690 → chr:2476 |

### BERSERKER (passive_tree.rs 729–795)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| frenzy | multi-strike proc (base FRENZY_PROC_CHANCE .25 const) | RATE | 0/0/0 decorative | — | (c-pending) | :730 → cb:11629ff |
| bloodlust | stack-speed value | MAG | .04/.06/.08 | — | (a) | :731 → cb:11835ff |
| reckless | Reckless Swing dealt/taken ladders | MAG | .15/.25/.35 | — | (c-pending) | :732 → fn cb:3770 |
| bloodfury | frenzy strike-chance add-on | MAG | .05/.10/.15 | [.15,.3,.45] | (a) | :733 → cb:11630 |
| killingspree | frenzy extra dmg | MAG | .10/.20/.30 | — | (a) | :734 → cb:11641 |
| savagemomentum | frenzy heal/kill | MAG | .03/.06/.09 | — | (a) | :735 → cb:11643 |
| unendingrage | rage window s | THR | 2/4/6 | — | (a) | :736 |
| overwhelm | DR→dmg (Ber side) | RATE | .03/.06/.09 | — | (a) | :737 |
| frenziedblows | frenzy hit dmg add-on | MAG | .02/.04/.06 | — | (a) | :738 |
| deathwish | self taken/dealt ladders | MAG | .10/.20/.30 | — | (c-pending) | :739 → fn cb:3788 |
| vigor | regen | MAG | .06/.12/.18 | — | (a) | :740 |
| gambit | crit per missing 20% HP | MAG | .05/.10/.15 | — | (a) | :741 → cb:11605 |
| deathmark(Ber) | frenzy strike-chance add-on | MAG | .05/.10/.15 | [.15,.3,.45]* | (a) | :742 → cb:11630 (*TOML key shared w/ Ranger's deathmark — OQ4) |
| bloodscent | execute-threshold ladder | THR | ladder cb:11635 | — | (c-pending) | :743 |
| cullingblow | frenzy DR shred | MAG | .10/.20/.30 | — | (a) | :744 → cb:11640 |
| chainkiller | frenzy extra dmg add-on | MAG | .10/.20/.30 | [.2,.4,.6] | (a) | :745 → cb:11641 |
| massacre | culling threshold | THR | .02/.04/.06 | — | (a) | :746 → cb:11642 |
| reaperscall | chain chance (+max-extra count) | MAG/CAP | .10/.20/.30 | — | (a) | :747 → cb:11671-72 |
| unbridled | frenzy heal add-on | MAG | .03/.06/.09 | — | (a) | :748 → cb:11643 |
| warpath | frenzy shield chance | MAG | .15/.30/.45 | — | (a) | :749 → cb:11644 |
| bloodrush | undying-charges ladder | CAP | ladder cb:11655 | — | (c-pending) | :750 |
| furyunleashed | ramping dmg/swing | MAG | .01/.02/.03 | — | (a) | :751 |
| neverending | tempo extension gate | THR | gate cb:11501 | — | (c-pending) | :752 |
| warlord | warlord buff value | MAG | .03/.06/.09 | — | (a) | :753 |
| shatter(Ber) | shred-on-hit gate | THR | gate cb:11815 | — | (c-pending) | :754 |
| exposed | shred amp % | MAG | 1/2/3 | — | (a) | :755 |
| crush | shred threshold ladder | THR | ladder cb:11817-19 | — | (c-pending) | :756 |
| hurricane | whirlwind tick dmg | MAG | .03/.06/.09 | — | (a) | :757 |
| tempo | stack expiry gate | THR | gate cb:11886 | — | (a) | :758 |
| windfury | extra swing chance | MAG | .15/.30/.45 | — | (a) | :759 |
| gloryhound | kill-streak dmg add-on | MAG | .05/.10/.15 | — | (a) | :760 → chr:3066 |
| recklessabandon | nets out Reckless taken-dmg | MAG | .05/.10/.15 | — | (a) | :761 → chr:2828 |
| gloriousdeath | undying-charges ladder | CAP | ladder cb:11660 | — | (c-pending) | :762 |
| bloodpump | heal on frenzy hit | MAG | .04/.08/.12 | — | (a) | :763 |
| secondgale | extra FlickerStrike waves | CAP | 2/4/6 | — | (a) | :764 |
| vengefulblood | shield on life loss | MAG | .50/1.0/1.5 | — | (a) | :765 |
| lastlaugh | death-nuke ladder | MAG | ladder by rank | — | (c-pending) | :766 |
| ragefueled | AS per missing-HP step | MAG | .05/.10/.15 | — | (a) | :767 |
| deathdefiant | Gambit grace = rank×3000ms | THR | 3/6/9 s | — | **(c-drift)** | :778 → cb:11606 |

### ROGUE (passive_tree.rs 796–875)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| precision | +crit multiplier | MAG | .15/.25/.35 | — | (a) | :804 pool |
| shadowstep | +evasion | MAG | .08/.14/.20 | — | (a) | :805 pool |
| opportunist | guaranteed hits vs marked | CAP | 1/2/3 | — | (a) | :803 → cb:11503 |
| ambush | opener dmg bonus | MAG | .33/.66/.99 | — | (a) | :811 |
| cutthroat | crit-kill dmg amp | MAG | .15/.30/.45 | [1,2,3] | (a) | :812 → cb:11516 |
| vanish | disappear (dur const) | MAG | .10/.20/.30 | — | (a) | :813 |
| exploitweakness | dmg vs marked | MAG | .10/.20/.30 | — | (a) | :814 → cb:11351 |
| twinstrikes | twin hit (base TWIN_STRIKE .50 const) | MAG | .15/.30/.45 | — | (a) | :815 |
| assassinate | execute ladder | THR | ladder by rank | — | (c-pending) | :816 |
| fleetfoot | stack-speed value | MAG | .05/.10/.15 | [.2,.4,.6] | (a) | :817 → cb:11811 (+gates :11831/:11850) |
| elusive | evasion-overflow→crit chance | RATE | .25/.50/.75 | =d* | (a)* | :818 pool; cap hard |
| nightstalker | dmg from stealth | MAG | .10/.20/.30 | — | (a) | :819 |
| openingmove | first-hit FlickerStrike cooldown s | THR | 4/3/2 s | [1,.75,.5] | (a) | :825 → cb:11506 |
| coldsteel | chill/freeze amp | MAG | .33/.665/.995+ | — | (a) | :829 |
| predator | mark duration add-on | MAG | .10/.20/.30 | — | (a) | :834 |
| markedfordeath | mark ladder | MAG | ladder by rank | — | (c-pending) | :839 |
| bloodyknife | crit-dmg-vs-marked amp | MAG | .10/.20/.30 | [.33,.66,1.0] | (a) | :840 → cb:11516 |
| finalcut | crit-kill AS buff | MAG | .05/.10/.15 | [1,2,3] | (a) | :841 → cb:11377 |
| smokescreen | evade-dmg debuff | MAG | .05/.10/.15 | — | (a) | :842 |
| fadeaway | post-vanish safety | THR/CAP | 1/2/3 | — | (a) | :843 |
| backstab | positional dmg | MAG | .15/.30/.45 | — | (a) | :844 |
| vitalstrike | big-hit ladder | MAG | .65/.80/.95 | — | (c-pending) | :845 |
| weakpoint | armor shred | MAG | .05/.10/.15 | — | (a) | :846 |
| surgicalstrike | ExploitWeakness ×2 at r3 | THR | gate ×2 cb:11351 | — | (c-pending) | :847 |
| echo | repeat-hit chance | MAG | .15/.30/.45 | [.5,1,1.5] | (a) | :848 → cb:11253 |
| flurry | AS burst | MAG | .10/.20/.30 | — | (a) | :849 |
| doubletap | extra repeats (= rank×3) | CAP | 3/6/9 via rank | — | (c-pending) | :855 → cb:11259 |
| coupdegrace | execute dmg | MAG | .30/.60/.90 | — | (a) | :856 |
| premeditation | opening dmg | MAG | .20/.40/.60 | — | (a) | :857 |
| silentblade | quiet dmg | MAG | .20/.40/.60 | — | (a) | :858 |
| windrunner | FleetFoot max stacks +1/rank | CAP | 1/2/3 | — | (a) | :859 → cb:11834 |
| silentsteps | stealth value | MAG | .03/.06/.09 | — | (a) | :860 |
| quickdraw | tempo ladders by rank | THR | ladders cb:11878/:11888 | — | (c-pending) | :861 |
| phantom | 2nd evasion-overflow→crit channel | RATE | .10/.20/.30 | =d* | (a)* | :862 pool; cap hard |
| duskveil | overflow→attack speed | RATE | .25/.50/.75 | =d* | (a)* | :863 pool; cap hard |
| voidstep | dodge-triggered teleport value | MAG | .10/.20/.30 | [.33,.66,1.0] | (a) | :864 → cb:11488 |
| huntersinstinct | vs low-HP dmg | MAG | .05/.10/.15 | — | (a) | :865 |
| apexpredator | boss dmg | MAG | .10/.20/.30 | — | (a) | :866 |
| silentkiller | stealth crit | MAG | .25/.50/.75 | — | (a) | :867 |

### MONK (passive_tree.rs 877–1044)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| flowingstrikes | stack-speed value (+count gates cb:11899/:11909) | MAG/CAP | .03/.04/.05 | — | (a) | :878 |
| innerfocus | crit chance | MAG | .03/.05/.07 | — | (a) | :879 |
| ironbody | +evasion | MAG | .06/.10/.14 | — | (a) | :880 pool |
| onehundredhands | bonus stacks (min 3) | CAP | 1/2/3 | — | (a) | :887 → cb:11929 |
| pressurepoint | dmg amp | MAG | .02/.04/.06 | — | (a) | :888 |
| relentlessassault | window extension (+2000ms at r3) | THR | gate cb:11911 | — | (c-pending) | :889 |
| meditation | regen | MAG | .01/.02/.03 | — | (a) | :890 |
| chiburst | chi dmg | MAG | .50/1.0/1.5 | — | (a) | :891 |
| serenity | DR proc (dur const SERENITY 3000ms) | MAG | .05/.10/.15 | — | (a) | :892 |
| **stonefist** | **evasion-overflow→increased dmg** | RATE | .5/.75/1.0 | [.1,.2,.3]* | (a)* | :893 pool; *cap hard 0.10/rank — see §1 |
| unbroken | evasion-ignore conversion efficiency | MAG | .10/.20/.30 | [.05,.1,.15] | (a) | :894 → chr:2930 |
| templeguardian | periodic lowest-HP heal | MAG | .05/.10/.15 | — | (a) | :895 |
| windwalker | move/AS value | MAG | .01/.02/.03 | — | (a) | :896 |
| hundredfists | extra hits window | THR/CAP | 2/4/6 | — | (a) | :909 |
| risingstorm | storm dmg | MAG | .10/.20/.30 | — | (a) | :902 |
| nervestrike | interrupt value | MAG | .10/.20/.30 | — | (a) | :903 |
| vitalpoints | crit-dmg amp | MAG | .02/.04/.06 | — | (a) | :904 |
| unbrokenchain | chain window | THR | 1/2/3 | — | (a) | :901 |
| eternalflow | bonus stacks | CAP | 1/2/3 | — | (a) | :940 → cb:11920 |
| unendingcycle | cycle extension | THR | 1/2/3 | — | (a) | :941 |
| stormfront | storm dmg add-on | MAG | .05/.10/.15 | — | (a) | :942 |
| innerpeace | heal amp | MAG | .005/.01/.015 | — | (a) | :943 |
| risingtide | ramping heal | MAG | .03/.06/.09 | — | (a) | :944 |
| clarity | trigger-on-block gate (r≥2) | THR | gate cb:11487 | — | (c-pending) | :945 |
| chiburstsanctuary | sanctuary chi | MAG | .25/.50/.75 | — | (a) | :950 |
| harmonize | DR dur (const 3000ms) | MAG | .05/.10/.15 | — | (a) | :951 |
| widecircle | chi extra targets | CAP | 1/2/3 | — | (a) | :952 → cb:11484 |
| unshakable | uninterruptible | THR | 1/2/3 | — | (a) | :953 |
| stillwater | Serenity guaranteed on first evade(s) | THR | 1/2/3 | — | **(d-unwired)** | :959 — NO consumer anywhere |
| unmovable | resist value | MAG | .05/.10/.15 | — | (a) | :960 |
| **graniteskin** | **2nd evasion-overflow→dmg channel** | RATE | .15/.30/.45 | [.05,.1,.15] | (a)* | :961 pool; *cap hard — §1 |
| earthenwill | overflow→max HP | RATE | .25/.50/.75 | =d* | (a)* | :962 pool; cap hard |
| counterflow | evade→counterattack chance | MAG | .10/.20/.30 | — | (a) | :963 |
| lastbastion | low-HP DR efficiency | MAG | .05/.10/.15 | [.01,.02,.03] | (a) | :979 → chr:2944 |
| chakraofmany | Chakra-of-Life ally share pct | MAG | .10/.20/.30 | [.05,.1,.15] | (a) | :919 → cb:11934 |
| chakraoflight | Chakra heal-on-trigger pct | MAG | .10/.20/.30 | [.033,.066,.1] | (a) | :926 → cb:11935 |
| **chakraoflife** | cheat-death immunity = rank×1000ms | THR | 1/2/3 s | [.33,.66,1.0] **INERT** | **(c-drift)** | :933 → cb:11997 (rank only) |
| risingdefiance ("Overgrown Reach") | **3rd evasion-overflow→dmg channel** | RATE | .15/.30/.45 | [.05,.1,.15] | (a)* | :993 pool; cap hard — §1 |
| unyieldingspirit ("Last Stand") | Unbroken evasion-ignore HP threshold ladder 25/35/45/55% | THR | 1/2/3 decorative | [.33,.66,1.0] **INERT** | **(c-drift)** | :1005 → cb:11451-59 (rank only) |
| sharedstrength | TempleGuardian extra allies | CAP | 1/2/3 | — | (a) | :1012 → cb:11284 |
| ironwill | TempleGuardian amp | MAG | .05/.10/.15 | — | (a) | :1013 |

### PALADIN (passive_tree.rs 1045–1174)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| oath | +intervene% | MAG | .04/.07/.10 | — | (a) | :1046 pool |
| smite | Smite dmg (+extra target via zealotry) | MAG | .10/.20/.30 | — | (a) | :1048 → cb:11156-57 |
| aegisward | intervene-overflow→DR | RATE | .5/.75/1.0 | =d* | (a)* | :1049 pool; cap hard |
| vowofprotection | party DR | MAG | .03/.06/.09 | — | (a) | :1050 → cb:10963/:12813 |
| unbreakablefaith | DR amp | MAG | .05/.10/.15 | — | (a) | :1051 |
| bulwarkoflight | Divine Shield amount add-on | MAG | .10/.20/.30 | — | (a) | :1052 → cb:11004 |
| consecration | consecrate shield pct | MAG | .10/.20/.30 | — | (a) | :1053 → cb:11142-43 |
| retributionaura | shield-reflect pct | MAG | .20/.40/.60 | — | (a) | :1054 → cb:11580 |
| zealotry | Smite extra target (gate) + value | MAG/CAP | .05/.10/.15 | — | (a) | :1055 → cb:11156-57 |
| holyfire | Holy Fire burn pct (dedicated flat path) | MAG | .05/.10/.15 | — | (a) | :1056 → cb:20132-50 |
| judgment | execute threshold add-on (base JUDGMENT .50 const) | MAG | .10/.20/.30 | — | (a) | :1057 → cb:11186ff |
| sanctifiedarmor | 2nd intervene-overflow→DR channel | RATE | .15/.30/.45 | =d* | (a)* | :1058 pool; cap hard |
| bondeddevotion | bond value | MAG | .05/.10/.15 | — | (a) | :1059 |
| steadfast | duration ×2/x3/x4 | THR | 2/4/6 | — | (a) | :1060 |
| beaconoflight | Vow extension | MAG | .03/.06/.09 | — | (a) | :1061 → cb:12813 |
| hallowedground | boss DR (flat approx) | MAG | .03/.06/.09 | — | (a) | :1064 → cb:12813 |
| unwavering | low-HP party-DR threshold ladder 0/.50/.65 | THR | 1/2/3 decorative | — | **(c-drift)** | :1075 → cb:10974-80 (rank only) |
| martyrscall | martyr value | MAG | .10/.20/.30 | — | (a) | :1111 (mag) |
| risingfervor | fervor stack | MAG | .02/.04/.06 | — | (a) | :1118 (mag) |
| guardianswrath | guardian dmg | MAG | .05/.10/.15 | — | (a) | :1125 (mag) |
| martyrsblessing | blessing heal | MAG | .05/.10/.15 | — | (a) | :1082 |
| graciousburden | burden value | MAG | .05/.10/.15 | — | (a) | :1083 |
| eternalvow | vow shield (dur const 8000ms) | MAG | .15/.30/.45 | — | (a) | :1084 |
| radiantbarrier | OverflowGrace shield DR add-on | MAG | .05/.10/.15 | — | (a) | :1085 → cb:11719 |
| graceperiod | grace window | MAG | .10/.20/.30 | — | (a) | :1086 |
| widerblessing | blessing extra targets | CAP | .10/.20/.30 declared | — | (a) | :1093 |
| communion | communion heal | MAG | .05/.10/.15 | — | (a) | :1094 |
| sharedlight | light share ×2/x3/x4 | THR | 2/4/6 | — | (a) | :1095 |
| holyvengeance | reflect pct add-on | MAG | .10/.20/.30 | [.33,.66,1.0] | (a) | :1096 → cb:11580 |
| purify | cleanse (dur const 3000ms) | MAG | .05/.10/.15 | — | (a) | :1097 |
| lastjudgment | final-judgment amp | MAG | .15/.30/.45 | — | (a) | :1098 |
| holyfirewildfire | Holy Fire splash amp | MAG | .10/.20/.30 | — | (a) | :1137 → cb:20136-50 |
| purgingflame | HF debuff (const 3000ms) | MAG | .10/.20/.30 | — | (a) | :1138 |
| risingblaze | Holy Fire ramp add-on | MAG | .10/.20/.30 | — | (a) | :1147 → cb:20136-50 |
| finaljudgment | judgment threshold unlock/add-on | MAG | .10/.15/.20 | — | (a) | :1152 → cb:11186 |
| executionersblessing | exec buff | MAG | .08/.16/.24 | — | (a) | :1153 |
| wrathoftheheavens | wrath dmg | MAG | .20/.40/.60 | — | (a) | :1162 |

### RANGER (passive_tree.rs 1175–1274)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| multishot | +splash | MAG | .08/.14/.20 | — | (a) | :1176 pool |
| mark | Hunter's Mark dmg-taken | MAG | .10/.18/.26 | — | (a) | :1177 → cb:12027 |
| fleet | +attack speed | MAG | .06/.10/.14 | — | (a) | :1178 pool |
| volley | multi-hit chance add-on | MAG | .10/.20/.30 | [.5,1,1.5] | (a) | :1179 → cb:11340 |
| piercingshots | crit-chance vs armor (+10% at r3 ladder) | MAG/CAP | gate cb:11469 | — | (c-pending) | :1180 |
| explosivetips | Multishot splash amp | MAG | .10/.20/.30 | [.33,.66,1.0] | (a) | :1181 pool |
| predatorseye | mark ally crit-mult add-on | MAG | .15/.30/.45 | — | (a) | :1182 → cb:12011 |
| packtactics | party dmg vs marked | MAG | .05/.10/.15 | — | (a) | :1183 |
| killzone | execute window value | MAG | .20/.40/.60 | — | (a) | :1184 |
| rapidfire | +attack speed amp | MAG | .06/.12/.18 | — | (a) | :1185 |
| evasivemaneuvers | +evasion (overflow source) | MAG | .06/.12/.18 | — | (a) | :1186 pool |
| relentlesspursuit | stack-speed value (+gates) | MAG | .02/.04/.06 | — | (a) | :1187 → cb:11837ff/:11811 |
| chainshot | chain-hit chance | MAG | .10/.20/.30 | [.5,1,1.5] | (a) | :1194 → cb:11340 |
| deadeye | total-crit add-on | MAG | .05/.10/.15 | — | (a) | :1195 → chr:3144 |
| stormofarrows | storm extra targets | CAP | 1/2/3 | — | (a) | :1196 → cb:11474 |
| armorbreaker | shred debuff (dur const 3000ms) | MAG | .03/.06/.09 | — | (a) | :1197 |
| windpierce | pierce value | MAG | .05/.10/.15 | — | (a) | :1198 |
| truestrike | accuracy/dmg | MAG | .10/.20/.30 | — | (a) | :1199 |
| widerburst | ExplosiveTips extra targets | CAP | 1/2/3 | — | (a) | :1200 → cb:11475 |
| scorchedearth | splash debuff (const 3000ms) | MAG | .05/.10/.15 | — | (a) | :1201 |
| overcharge | ExplosiveTips splash amp | MAG | .10/.20/.30 | [.33,.66,1.0] | (a) | :1202 pool |
| apexhunter | mark ally crit-mult add-on | MAG | .10/.20/.30 | — | (a) | :1203 → cb:12011 |
| trueshot | guaranteed crits value | MAG | .10/.20/.30 | — | (a) | :1204 |
| **huntersfocus** | mark-ally share = rank/3 of Predator's Eye | RATE | 1/2/3 decorative (⅓/⅔/full) | — | **(c-drift)** | :1215 → cb:12012 (rank only) |
| coordinatedstrike | partner strike | MAG | .05/.10/.15 | — | (a) | :1222 |
| alphaspredator | marked-target ally dmg | MAG | .05/.10/.15 | — | (a) | :1223 → cb:12007 |
| widerpack | mark spread extra targets | CAP | 1/2/3 | — | (a) | :1224 → cb:12013 |
| finalblow | killzone threshold ladders | THR | ladders cb:12014-18 | — | (c-pending) | :1225 |
| cleankill | re-mark chance | MAG | .25/.50/.75 | — | (a) | :1226 → cb:12023 |
| huntersreward | kill heal | MAG | .06/.12/.18 | — | (a) | :1227 → cb:12024 |
| windsprint | +AS | MAG | .05/.10/.15 | — | (a) | :1228 pool |
| quickshot | +splash | MAG | .03/.06/.09 | — | (a) | :1229 pool |
| fleetingshadow | haste debuff (const 3000ms) | MAG | .15/.30/.45 | — | (a) | :1230 |
| swiftwind | +evasion | MAG | .05/.10/.15 | — | (a) | :1231 pool |
| vanishingshot | vanish-shot (const 3000ms) | MAG | .15/.30/.45 | — | (a) | :1232 |
| lightfoot | evasion-overflow→AS | RATE | .5/.75/1.0 | =d* | (a)* | :1233 pool; cap hard |
| windborn | RelentlessPursuit stacks +1/rank | CAP | 1/2/3 | — | (a) | :1234 → cb:11840 |
| huntersstride | stride value | MAG | .02/.04/.06 | — | (a) | :1235 |
| neverwinded | duration ×2/x3/x4 | THR | 2/4/6 | — | (a) | :1239 |

### MAGE (passive_tree.rs 1257–1356)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| arcane | +crit multiplier | MAG | .03/.06/.09 | — | (a) | :1258 pool |
| weaving | +attack speed | MAG | .05/.09/.13 | — | (a) | :1259 pool |
| surge | +splash | MAG | .08/.14/.20 | — | (a) | :1260 pool |
| criticalmass | +crit chance | MAG | .04/.08/.12 | — | (a) | :1261 pool |
| overload | +crit multiplier | MAG | .03/.06/.09 | — | (a) | :1262 pool |
| spellecho | crit recast at 50% chance | MAG | .15/.30/.45 | — | (a) | :1263 |
| quickcast | +AS amp; also early-burst value cb:10950 | MAG | .05/.10/.15 | — | (a) | :1264 pool |
| flowstate | stack-speed value (+stacks cb:11843) | MAG/CAP | .02/.04/.06 | [.05,.1,.15] | (a) | :1265 → cb:11841/:11811 |
| temporalrift | speed-overflow→dmg efficiency | RATE | .30/.60/.90 | — | (a) | :1266 |
| chainlightning | dmg per reachable target | MAG | .10/.20/.30 | — | (a) | :1267 |
| wildfire | splash amp | MAG | .10/.20/.30 | — | (a) | :1268 pool |
| frostnova | splash evas-debuff (const 3000ms) | MAG | .05/.10/.15 | — | (a) | :1269 |
| manasurge | +crit chance | MAG | .03/.06/.09 | — | (a) | :1270 pool |
| arcaneshield | crit-shield %maxHP | MAG | .05/.10/.15 | — | (a) | :1271 |
| empoweredbolt | force-crit r2+, +20% cdmg r3 | THR/MAG | table 0,0,0.20 | — | (a) | :1272 → cb:11378-79 |
| cataclysm | +crit multiplier | MAG | .03/.06/.09 | — | (a) | :1273 pool |
| volatilemagic | crit splash pct; targets const MAX=2 CAP hard | MAG/CAP | .10/.20/.30 | [.3,.6,.9] | (a) | :1274 → cb:11380 |
| arcaneinstability | vs>65%HP crit-dmg table | MAG | tbl .05/.09/.12 | [.1,.2,.3] | (a) | :1275 → cb:11382; thresh const .65 hard cb:11381 |
| echoingpower | echo chance add-on | MAG | .15/.30/.45 | [.2,.4,.6] | (a) | :1276 → cb:11253 |
| resonance | echo dmg amp add-on | MAG | .10/.20/.30 | [.15,.25,.4] | (a) | :1277 → cb:11247 |
| infiniteloop | max repeats table | CAP | tbl 3/6/9 | — | (a) | :1288 → count cb:11258 |
| haste | +AS | MAG | .05/.10/.15 | — | (a) | :1289 pool |
| acceleration | +splash | MAG | .05/.10/.15 | — | (a) | :1290 pool |
| **timewarp** | burst window = 5s+2s×rank | THR | 1/2/3 decorative | — | **(c-drift)** | :1302 → cb:10944/10957 rank only |
| perpetualmotion | FlowState stacks +1/rank | CAP | 1/2/3 | — | (a) | :1309 → cb:11843 |
| riptide | AS-on-evade value | MAG | .02/.04/.06 | [.5,.1,.15]* | (a) | :1310 → cb:11814 (*TOML suspect: r1 .50 > r2 .10, OQ5) |
| unbrokenrhythm | duration ×2/x3/x4 | THR | 2/4/6 | — | (a) | :1311 |
| dilation | slow-field value | MAG | .10/.20/.30 | — | (a) | :1312 |
| paradox | echo-of-echo chance | MAG | .15/.30/.45 | — | (a) | :1313 |
| eternalmoment | time-walk delta (max chaostheory) | MAG | .10/.20/.30 | — | (a) | :1314 → cb:112 |
| thunderstruck | lightning amp | MAG | .10/.20/.30 | — | (a) | :1319 |
| staticfield | field debuff (const 3000ms) | MAG | .04/.08/.12 | — | (a) | :1320 |
| stormcaller | guaranteed extra targets | CAP | count 1/2/3 | [3,6,9]* | (a) | :1321 → cb:11468 (*live ≠ count semantics, OQ5) |
| conflagration | +splash | MAG | .10/.20/.30 | — | (a) | :1322 pool |
| risingheat | +splash | MAG | .05/.10/.15 | — | (a) | :1326 pool |
| infernalpact | pact value | MAG | .03/.06/.09 | — | (a) | :1327 |
| blizzard | blizzard pct | MAG | .05/.10/.15 | — | (a) | :1328 |
| permafrost | duration ×2/x3/x4 | THR | 2/4/6 | — | (a) | :1329 |
| absolutezero | freeze-threshold table | THR | tbl 0/.50/.65 | — | (a) | :1330 → cb:11463 |

### WARLOCK (passive_tree.rs 1357–1469)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| pact | +attack speed | MAG | .06/.10/.14 | — | (a) | :1358 pool |
| curse | Curse dmg-taken | MAG | .08/.14/.20 | — | (a) | :1359 → cb:12027 |
| siphon | +life leech | MAG | .01/.02/.03 | — | (a) | :1360 pool |
| felhaste | +AS amp; early-burst value cb:10951 | MAG | .06/.12/.18 | — | (a) | :1361 pool |
| unstablepower | power swing | MAG | .30/.60/.90 | — | (a) | :1362 |
| felrush | rush speed (+ramp mult cb:12069) | MAG | .08/.16/.24 | — | (a) | :1363 → cb:12055 |
| amplifycurse | Curse amp | MAG | .08/.16/.24 | — | (a) | :1364 → cb:12027 |
| contagiouscurse | curse spread count | CAP | 1/2/3 | — | (a) | :1365 → cb:12028 |
| doom | detonate pct add-on | MAG | .02/.04/.06 | =d | (a) | :1367 → cb:12029 |
| soulharvest | on-kill heal/shield | MAG | .06/.12/.18 | — | (a) | :1368 |
| lifetap | HP→dmg ×(2+soulexchange) | RATE | .03/.06/.09 | — | (a) | :1369 → chr:3069 |
| darkcommunion | communion heal/shield | MAG | .50/1.0/1.5 | — | (a) | :1370 |
| chaosbolt | +crit chance | MAG | .05/.10/.15 | — | (a) | :1371 pool |
| burningrage | +AS | MAG | .05/.10/.15 | — | (a) | :1372 pool |
| **demonicspeed** | burst window = 5s+2s×rank | THR | 1/2/3 decorative | — | **(c-drift)** | :1378 → cb:10945/10957 rank only |
| voidenergy | void energy value | MAG | .10/.20/.30 | — | (a) | :1385 |
| entropicforce | entropy pull | MAG | .15/.30/.45 | — | (a) | :1386 |
| chaostheory | time-walk delta (max eternalmoment) | MAG | .10/.20/.30 | — | (a) | :1387 → cb:112 |
| warpspeed | Fel Rush duration +1s/rank | THR | 2/4/6 s | — | (a) | :1388 → cb:12056 |
| deathmarch | rush speed add-on | MAG | .08/.16/.24 | — | (a) | :1389 → cb:12055 |
| ravage | ramping stack pct (+ladder cb:12068) | MAG/CAP | .5 flat | — | (a) | :1390 → cb:12068-69 |
| witheringcurse | curse heal-reduction | MAG | .10/.20/.30 | — | (a) | :1391 → cb:12030 |
| hexmastery | Curse amp add-on | MAG | .08/.16/.24 | — | (a) | :1392 → cb:12027 |
| **cursedblood** | fight-start auto-curse COUNT | CAP | 1/2/3 via rank | — | **(c-drift)** | :1399 → cb:12033 rank only |
| plagueoflocusts | locust extra targets | CAP | 1/2/3 | — | (a) | :1406 → cb:12028 |
| epidemic | spread bonus pct | MAG | .15/.30/.45 | — | (a) | :1407 → cb:12031 |
| **virulence** | Soul Stone max COUNT (−33% hit dmg/use, const SOUL_STONE .33) | CAP | 1/2/3 via rank | — | **(c-drift)** | :1415 → cb:12032 rank only |
| harbinger | detonate pct add-on | MAG | .15/.30/.45 | [.02,.04,.06] | (a) | :1422 → cb:12029 |
| dreadfuldeath | death shred debuff | MAG | .05/.10/.15 | — | (a) | :1423 → cb:12034 |
| apocalypse | death splash pct | MAG | .30/.60/.90 | — | (a) | :1424 → cb:12035 |
| reaping | reap value | MAG | .04/.08/.12 | — | (a) | :1425 |
| darkritual | ritual buff (const 5000ms) | MAG | .05/.10/.15 | — | (a) | :1426 |
| eternalhunger | hunger shield (const 5000ms) | MAG | .25/.50/.75 | — | (a) | :1427 |
| painbond | bond share | MAG | .01/.02/.03 | — | (a) | :1428 |
| demonicresilience | resilience DR | MAG | .05/.10/.15 | — | (a) | :1429 |
| soulexchange | Lifetap multiplier add-on | MAG | .30/.60/.90 | — | (a) | :1430 → chr:3069 |

### CLERIC (passive_tree.rs 1470–1581)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| grace | +heal power | MAG | .20/.36/.52 | — | (a) | :1471 pool |
| prayer | mending chain chance | MAG | .15/.25/.35 | — | (a) | :1472 → cb:11755ff |
| resilience | party +max HP% | MAG | .04/.07/.10 | — | (a) | :1473 → cb:12774 |
| radiantlight | +heal power | MAG | .16/.32/.48 | — | (a) | :1474 pool |
| overflowinggrace | grace overflow→shield pct | MAG | .10/.20/.30 | — | (a) | :1475 → cb:11712 |
| sanctifiedtouch | heal-crit bonus ladders r2/r3 | MAG | 0/0/0 decorative | — | (c-pending) | :1476 → cb:11727-36 |
| chainoflight | chain extra targets | CAP | 1/2/3 | — | (a) | :1477 → cb:11769 |
| mercifultouch | overrides bounce value 50→80% | MAG | .50/.65/.80 | — | **(c-drift?)** | :1478 → cb:11776ff reads rank — VERIFY |
| divinefavor | favor shield pct | MAG | .20/.40/.60 | — | (a) | :1479 (dur const 5000ms) |
| sanctuary | party DR | MAG | .03/.06/.09 | [.33,.66,1.0]* | (a) | :1480 → cb:12780 (*TOML value far above DR norms — OQ5) |
| guardianspirit(Cle) | save buff ladders r2/r3 | THR | ladders cb:11679-94 | — | (c-pending) | :1481 |
| radiantaegis | party evasion | MAG | .04/.08/.12 | — | (a) | :1482 → cb:12781 |
| luminous | +heal power | MAG | .16/.32/.48 | — | (a) | :1483 pool |
| graciousspirit | spirit share | MAG | .03/.06/.09 | — | (a) | :1484 |
| eternallight | eternal-light bonus pct | MAG | .16/.32/.48 | — | (a) | :1492 → cb:11705 |
| graciousoverflow | grace shield add-on | MAG | .10/.20/.30 | — | (a) | :1499 → cb:11712 |
| balancedfaith | grace shield DR add-on | MAG | .10/.20/.30 | — | (a) | :1500 → cb:11719 |
| riftofmercy | grace shield duration +2s/rank (base const 5000ms) | THR | 2/4/6 s | [3,6,9]* | (a) | :1501 → cb:11713 (*TOML in ms-like scale vs s semantics — OQ5) |
| holycrit | heal-crit dmg add-on (base .50 const) | MAG | .10/.20/.30 | — | (a) | :1502 → cb:11728 |
| divineclarity | heal-crit chance add-on (base .10 const) | MAG | .05/.10/.15 | — | (a) | :1503 → cb:11735 |
| radiance | heal-crit splash pct | MAG | .20/.40/.60 | — | (a) | :1504 → cb:11745 |
| wideningcircle | chain targets add-on (count+mag) | CAP | 1/1.5/2 round | — | (a) | :1505 → cb:11769 |
| swiftmending | mend speed | MAG | .10/.20/.30 | — | (a) | :1506 |
| unbrokenprayer | prayer persistence | MAG | .15/.30/.45 | — | (a) | :1507 |
| gentletouch | touch buff (const 3000ms) | MAG | .05/.10/.15 | — | (a) | :1508 |
| compassion | DR-on-heal proc (const dur) | MAG | 1/2/3 decorative | — | (c-pending) | :1509 |
| healingtouch | touch amp | MAG | .05/.10/.15 | — | (a) | :1510 |
| aegisofmercy | mercy aegis | MAG | .10/.20/.30 | — | (a) | :1511 |
| wardinglight | duration ×2/x3/x4 | THR | 2/4/6 | — | (a) | :1512 |
| sacredbarrier | shield-reflect pct/chance | MAG | .15/.30/.45 | — | (a) | :1513 → cb:11577-87 |
| consecratedearth | party DR add-on | MAG | .03/.06/.09 | — | (a) | :1514 → cb:12780 |
| wardingprayer | boss DR approx | MAG | .03/.06/.09 | — | (a) | :1517 → cb:12780 |
| unyieldingfaith | low-HP party-DR threshold ladder 0/.50/.65 | THR | 1/2/3 decorative | — | **(c-drift)** | :1523 → cb:10968/10974-80 rank only |
| secondchance | revive-chance add-on (base .20 const) | MAG | .08/.16/.24 | — | (a) | :1530 → cb:11695 |
| divineintervention | save DR pct | MAG | .10/.20/.30 | — | (a) | :1531 → cb:11699 |
| finalblessing | save heal-power pct | MAG | .05/.10/.15 | — | (a) | :1532 → cb:11700 |
| windsofgrace | party evasion add-on | MAG | .04/.08/.12 | — | (a) | :1533 → cb:12781 |
| swiftblessing | party AS | MAG | .03/.06/.09 | — | (a) | :1534 → cb:12782 |
| haloedsteps | party more-dmg CAP per rank (rate lives in LiveTunables) | CAP | .03/.06/.09 | — | (a)+(b) mixed | :1548 → cb:12796-12800 |

### DRUID (passive_tree.rs 1582–1807)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| regrowth | +heal power (also mult chr:3307) | MAG | .20/.36/.52 | — | (a) | :1595 |
| instinct | +evasion | MAG | .06/.10/.14 | — | (a) | :1596 pool |
| barrier | Thorned Barrier DR | MAG | .05/.10/.15 | [.33,.66,1.0]* | (a) | :1601 → chr:2839 (*TOML ≫ norms, OQ5) |
| rejuvenation | second-heal chance; gate cb:11770 | MAG | .15/.25/.35 | — | (a) | :1602 |
| wildsurge | heal-power→action-speed shortening | RATE | .08/.16/.24 | — | (a) | :1603 → chr:2455 |
| naturesblessing | +crit chance pool BUT r2/r3 heal-crit ladder gates by rank cb:11729/:11733 | MIXED | .10/.20/.30 | — | (a)+(c-drift half) | :1612 |
| feralreflexes | +evasion amp | MAG | .06/.12/.18 | — | (a) | :1613 pool |
| shiftingform | evasion-overflow→dmg | RATE | .5/.75/1.0 | =d* | (a)* | :1614 pool; cap hard |
| packinstinct | lowest-HP ally evasion | MAG | .04/.08/.12 | — | (a) | :1615 |
| livingarmor | DR amp | MAG | .05/.10/.15 | — | (a) | :1616 pool |
| bramblegrowth | reflect reduced dmg | MAG | .15/.30/.45 | — | (a) | :1617 → cb:11611 |
| symbiosis ("Werebear") | Thick Hide cleanse cycle ms via magnitude | THR | 6000/5000/4000 ms | — | (a) | :1623 → cb:11299 |
| bloomingfield | ×heal power mult chr:3313 (+bounce count = rank, cb:11771 drift-half) | MAG/CAP | .10/.20/.30 | — | (a)+(drift half) | :1635 |
| evergrowth | echo-of-heal fraction ×heal power | MAG | .03/.06/.09 | — | (a) | :1642 → chr:3415 |
| seedoflife | seed shield (const 5000ms) | MAG | .10/.20/.30 | — | (a) | :1653 |
| overgrowth | wildsurge add-on | MAG | .05/.10/.15 | — | (a) | :1654 → chr:2455 |
| primalforce | action-force mult | MAG | .10/.20/.30 | — | (a) | :1665 → chr:3194 |
| wildheart | heart value | MAG | .10/.20/.30 | — | (a) | :1669 |
| bloomstrike | +crit chance pool; heal-crit mult base .50+mag cb:11730 | MAG | .10/.20/.30 | — | (a) | :1673 |
| wildinstinct | DR proc (const 3000ms); base .10+mag cb:11737 | MAG | .03/.06/.09 | — | (a) | :1677 |
| **verdantburst** | ally-save charges COUNT via rank; echo threshold is LiveTunable | CAP | 1/2/3 via rank | — | **(c-drift)** + (b) | :1688 → cb:11701 |
| quickpaw | +evasion | MAG | .05/.10/.15 | — | (a) | :1695 pool |
| silentprowl | prowl value | MAG | .10/.20/.30 | — | (a) | :1696 |
| wildagility | +AS | MAG | .03/.06/.09 | — | (a) | :1697 pool |
| primalshift | 2nd evasion-overflow→dmg channel | RATE | .15/.30/.45 | =d* | (a)* | :1698 pool; cap hard |
| clawstrike | overflow→crit chance | RATE | .20/.40/.60 | =d* | (a)* | :1699 pool; cap hard |
| wildfury | fury value | MAG | .10/.20/.30 | — | (a) | :1700 |
| pathfinder | pathfind value | MAG | .04/.08/.12 | — | (a) | :1701 |
| unitedpack | SharedStrength extra allies | CAP | 1/2/3 | — | (a) | :1702 → cb:11284 |
| wildguardian | guardian value | MAG | .02/.04/.06 | — | (a) | :1703 |
| ironbark | Thorned Barrier amp | MAG | .05/.10/.15 | — | (a) | :1708 → chr:2839 |
| naturesward | vs-boss DR pct | MAG | .03/.06/.09 | — | (a) | :1712 → cb:11603 |
| unyieldingroots | taunt cycle ms via magnitude (8000/6000/4000) | THR | 8000/6000/4000 ms | — | (a) | :1717 → cb:11596-97 |
| thornlash | bramble reflect add-on | MAG | .10/.20/.30 | — | (a) | :1724 → cb:11611 |
| poisonthorns | attackers −dmg debuff | MAG | .05/.10/.15 | — | (a) | :1725 → cb:11612 |
| entangle | entangle chance (window const 3000ms) | MAG | .15/.30/.45 | — | (a) | :1726 → cb:11613 |
| rootednetwork | Thick Hide cleanse count extension | CAP | 1/2/3 | — | (a) | :1729 (count read) |

### SLAYER (passive_tree.rs 1808–1934)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| wound | bleed stacks (dur const 6000ms; cap 5+blooddebt) | MAG/CAP | .004/.006/.008 | — | (a)* | :1809 → cb:11533ff (*cap partly hard) |
| vampiricfrenzy | FlickerStrike cooldown ×(1−v), clamp .9 | RATE | .12/.20/.28 | [.3,.6,.9]* | (a) | :1814 → cb:10990 (*TOML r3 .90 hits clamp — OQ5) |
| sacrifice ("Bloodpact") | HP cost pct (dmg mult = 1+rank is DRIFT) | MAG | .10/.08/.06 | [.33,.66,1.0]* | (a)+(drift half) | :1815 → cb:11557 cost; **mult cb:11558 rank-only** |
| festering | wound spread gate/value | MAG | .25/.50/.75 | — | (a) | :1816 → cb:11540 |
| hemorrhage | wound explosion pct | MAG | .10/.20/.30 | [.2,.4,.6] | (a) | :1817 → cb:11537 |
| necrotic | necrotic value | MAG | .10/.20/.30 | — | (a) | :1818 |
| bloodfrenzy | FlickerFrenzy speed (const dur 4000ms) | MAG | .15/.30/.45 | — | (a) | :1819 → cb:12078 |
| endlessthirst | thirst cap bonus; r3 removes cap via rank | MAG/CAP | .05/.10/.15 | — | (a)+gate mix | :1820 → cb:12091-92 |
| reapers | reaper's momentum per kill (count) | CAP | 1/2/3 round | — | (a) | :1821 → cb:12094 |
| grimbargain | kill refund pct | MAG | .30/.50/.70 | — | (a) | :1822 → cb:11560 |
| bloodsac | Bloodpact cd −0.5s/count (floor 2000ms const) | THR/CAP | 1/2/3 count | [1,2,3]=d | (a) | :1823 → cb:10998 |
| martyrdom | martyrdom shield pct | MAG | 1.5/2.0/2.5 | — | (a) | :1824 → cb:11559 |
| rot | rot dmg | MAG | .10/.20/.30 | — | (a) | :1825 |
| contagion | spread chance | MAG | .25/.50/.75 | — | (a) | :1826 → cb:11541 |
| blooddebt | wound max stacks +N | CAP | 1/2/3 count | — | (a) | :1827 → cb:11533 |
| overflow | explosion self-leech pct | MAG | .20/.40/.60 | — | (a) | :1828 → cb:11538 |
| arterialspray | explosion extra targets | CAP | 1/2/3 | — | (a) | :1829 → cb:11539 |
| hemorrhagesecondwind | second-wind reset chance | MAG | .15/.30/.45 | — | (a) | :1834 → cb:11571 |
| witheringtouch | wound −dmg debuff | MAG | .10/.20/.30 | — | (a) | :1835 → cb:~11536 |
| plaguebearer | plague extra targets | CAP | 1/2/3 | — | (a) | :1836 → cb:11543 |
| gravechill | chill speed debuff | MAG | .08/.16/.24 | — | (a) | :1837 → cb:11542 |
| unrelenting | frenzy duration +1333ms×mag (+r3 ladder cb:12079) | THR | 1/2/3 s | — | (a)+ladder mix | :1841 → cb:12082 |
| warcry | cry value | MAG | .05/.10/.15 | — | (a) | :1842 |
| adrenaline | crit-mult add-on | MAG | .20/.40/.60 | — | (a) | :1843 → cb:12084 |
| overflowvessel | shield pct on overflow | MAG | .25/.50/.75 | — | (a) | :1844 → cb:12089 |
| insatiable | frenzy extend chance | MAG | .10/.20/.30 | — | (a) | :1845 → cb:12087 |
| secondheartbeat | cheat-death chance | MAG | .20/.40/.60 | — | (a) | :1846 → cb:12088 |
| chainreaper | chain heal pct | MAG | .03/.06/.09 | — | (a) | :1847 → cb:12085 |
| deathspiral | low-HP heal pct | MAG | .04/.08/.12 | — | (a) | :1848 → cb:12086 |
| undying | undying-charges ladder | CAP | ladder cb:11666 | — | (c-pending) | :1849 |
| bloodforblood | reflect pct | MAG | .02/.04/.06 | — | (a) | :1850 → cb:11562 |
| debtcollector | non-lethal refund pct | MAG | .15/.30/.45 | — | (a) | :1851 → cb:11561 |
| cleanslate | reset chance | MAG | .25/.50/.75 | — | (a) | :1852 → cb:11570 |
| triage | cost-discount pct/use | MAG | .03/.06/.09 | — | (a) | :1853 → cb:11563 |
| warlordsresolve | 3rd-use party dmg | MAG | .05/.10/.15 | — | (a) | :1854 → cb:11569 |
| **finaloffering** | post-Nth-use cost −33% flat; N ladder by rank | THR/MAG | 1/2/3 decorative; .33 hard | [1,2,3]**INERT** | **(c-drift)** | :1871 → cb:11565-68 (rank only; pct hardcoded) |
| guardiansblood | reflect pct/chance | MAG | .10/.20/.30 | — | (a) | :1878 → cb:11581-82 |
| sharedpain | party share of wounds | MAG | .25/.50/.75 | — | (a) | :1879 → cb:14231 |
| lastrites | lastrites ladder | MAG | ladder cb:11684 | — | (c-pending) | :1884 |

### ELEMENTALIST (passive_tree.rs 1935–2525)

| key | what | type | d | live | state | site |
|---|---|---|---|---|---|---|
| righteousfire | RF burn value on enemies | MAG | .10/.20/.30 | — | (a)+(b) split | :1936 → cb:11943 (burn) / **self-dmg = LiveTunables rf_* (b)** |
| elementalfocus | focus dmg | MAG | .05/.10/.15 | — | (a) | :1942 |
| **golemmaster** | golem slot COUNT via rank | CAP | 1/2/3 via rank | — | **(c-drift)** | :1948 → cb:12162, manager.rs:3003, web:4760 |
| **healingflames** | regen ladder 3/6/10% by rank (irregular fn) | MAG | decorative .03/.065/.10 | — | **(c-drift)** | :1957 → cb:11955, fn cb:6536 |
| cleansingflames | cleanse tick chance (tick const 4000ms hard) | MAG/RATE | .33/.665/.995+ | — | (a)* | :1970 → cb:11966 (*interval hard) |
| scorchingflames | burn amp | MAG | .10/.20/.30 | — | (a) | :1977 |
| shockingfocus | shock focus value | MAG | .10/.20/.30 | — | (a) | :1984 |
| chillingfocus | chill focus value | MAG | .10/.20/.30 | — | (a) | :1991 |
| scorchingfocus | scorch focus value | MAG | .10/.20/.30 | — | (a) | :1998 |
| thundergolem | reform delay secs via magnitude | THR | 4/3/2 s | [3,6,9]* | (a) | :2005 → cb:6062 (*TOML ≫ s semantics, OQ5) |
| fanningflames | fan value | MAG | .10/.20/.30 | — | (a) | :2040 → cb:11956 |
| **risingphoenix** | revive COUNT via rank (delay/survival consts 1000/3000ms, hp .25) | CAP | 1/2/3 via rank | — | **(c-drift)** | :2047 → cb:11958 (+consts cb:267-278) |
| shieldingflames | shield flame pct | MAG | .10/.20/.30 | — | (a) | :2057 → cb:11957 |
| enshroudedfire | shroud evasion | MAG | .03/.06/.09 | — | (a) | :2064 → cb:11968 |
| guardianfire | guardian DR pct | MAG | .03/.06/.09 | — | (a) | :2071 → cb:11969 |
| shieldingfire | shield fire pct | MAG | .55/.60/.65 | — | (a) | :2078 |
| relentlessflames | per-stack ramping pct | MAG | .01/.02/.03 | — | (a) | :2085 → cb:11951 |
| cauterizingflames | cauterize debuff (const 1500ms) | MAG | .05/.10/.15 | — | (a) | :2092 → cb:11953 |
| ashestoashes | ashes pct | MAG | .05/.10/.15 | — | (a) | :2099 → cb:11954 |
| overshock | shock amp | MAG | .15/.30/.45 | — | (a) | :2106 |
| electricaloverload | +crit multiplier | MAG | .10/.20/.30 | — | (a) | :2107 pool |
| lightningaegis | aegis shield pct | MAG | .01/.02/.03 | — | (a) | :2108 → cb:~11939 |
| polarflux | flux value | MAG | .15/.30/.45 | — | (a) | :2109 |
| hoarfrost | +crit chance | MAG | .10/.20/.30 | — | (a) | :2112 pool |
| chillingaegis | cold aegis shield pct | MAG | .01/.02/.03 | — | (a) | :2113 → cb:11940 |
| incinerate | incinerate dmg | MAG | .15/.30/.45 | — | (a) | :2114 |
| pyroclasm | conflagration dmg pct (field reads this key) | MAG | .10/.20/.30 | — | (a) | :2118 → cb:11942 |
| scorchingaegis | fire aegis shield pct | MAG | .01/.02/.03 | — | (a) | :2119 → cb:11941 |
| gigantify | Thunder Golem HP-scale add-on | MAG | 1/2/3 % | — | (a) | :2120 → cb:6041 (base GOLEM_STAT_SCALE .33 const hard) |
| growing | golem growth pct | MAG | .33/.665/.995+ declared | [.5,1,1.5] | (a) | :2127 → cb:6063 |
| terrifying | terrify pct | MAG | .33/.665/.995+ | — | (a) | :2134 → cb:6065 |
| volcanicash | ash pct | MAG | .33/.665/.995+ | — | (a) | :2141 → cb:6079 |
| **blazing** | Flame Golem AS ladder 6/9/18% (irregular fn) | MAG | decorative .06/.12/.18 | — | **(c-drift)** | :2148 → cb:6090, fn cb:5943 |
| surging | surge value | MAG | .10/.20/.30 | — | (a) | :2158 |
| replenishing | replenish value | MAG | 1/2/3 | — | (a) | :2165 |
| singing | singing value | MAG | .10/.20/.30 | — | (a) | :2172 |
| **shattering** | icicle target count = splash+rank (by design); damage pct is (b) | CAP | count 1/2/3 | [2,4,6] **INERT** | **(c-drift)**+(b) | :2179 → cb:6139 (rank only); pct → LiveTunables |

### Class/skill dials currently ON `/admin/tunables` — state (b)

| field(s) | belongs to | what it does | type | shipped d | live now | read path |
|---|---|---|---|---|---|---|
| `rf_self_damage_pct_rank1..3` (tunables.rs:300ff, d=.10/.20/.30) | Paladin Righteous Fire | self-burn %maxHP/s per rank | RATE | .10/.20/.30 | .20/.40/.60 | tunables match on rank, combat.rs:11945-50 |
| `haloedsteps_per_instance_pct_rank1..3` (d=.01/.02/.03) | Cleric Haloed Steps | party more-dmg per Divine affix instance | RATE | .01/.02/.03 | same | haloedsteps_more_damage_pct, cb:12800 |
| `shattering_enabled` + `shattering_damage_pct_rank1..3` (d=1%/1%/1%) | Elementalist Shattering | icicle damage basis (% of dead enemy HP) + kill-switch | MAG+toggle | .01×3 | enabled; .003/.006/.009 | dedicated flat path (never apply_hit) |
| `verdantburst_echo_threshold_pct` (d=1.0) | Druid Verdant Burst | Echo-chance threshold for ally save | THRESHOLD | 1.0 | 1.0 | combat sim, Verdant Burst check |
| `thunder_redistribution_pct` / `_window_secs` | Elementalist Thunder Golem | party-redistribute share/window | RATE/THR | — live .5 / 2.0 | same | thunder redistribution ticks |
| `defensive_stat_hard_cap` (d/live .95) | GLOBAL (doctrine) | universal DR/stat ceiling | CAP | .95 | .95 | shared DR path |

---

## 1. CAPS

Every constant bounding a passive's output, with tunability status:

| cap | bounds | type | value | tunable? | site |
|---|---|---|---|---|---|
| **OVERFLOW_CONVERSION_CAP_PER_RANK** | every OverflowConversion node's OWN contribution | CAP | 0.10/rank (30% at 3/3) | **NO — const** | combat.rs:342, applied chr:2640 |
| input overflow caps (DR/Block/Evasion) | where conversion input saturates | CAP | 0.75 | NO — hardcoded match arm | passive_tree.rs:400-406 |
| intervene overflow cap | Aegis Ward family input | CAP | 0.50 | NO | passive_tree.rs:403 |
| defensive_stat_hard_cap | universal DR ceiling (doctrine) | CAP | 0.95 | YES (b) | tunables.rs + form |
| evasion-ignore total (Unbroken/Last Stand) | "capped at 75% total" | CAP | 0.75 | NO | combat.rs:11451ff |
| block DR (secondskin ladder tops at) | block mitigation | MAG | .65/.70/.75 declared | node-magnitude (a); BASE BLOCK_DAMAGE_REDUCTION const when uninvested | cb:11801/:11888 region |
| ELEMENTAL_DEFENSE_FLOOR/CEILING | elemental proc debuff/buff clamps | CAP | 0.25 / 0.75 | NO | cb:328/:333 |
| stack-max bases: MOMENTUM 5, FLEETFOOT 3, BLOODLUST 5, RELENTLESS_PURSUIT 5, FLOWSTATE 5, FLOWING 5 | stack-speed buffs | CAP | consts | NO (count-ext nodes ARE tunable) | cb:290-313 + :11827-11860 |
| onehundredhands bonus stacks `.min(3)` | Monk stacks | CAP | 3 | NO (inline) | cb:11929 |
| chain targets `.min(5)` (chainoflight/wideningcircle) | Cleric chains | CAP | 5 | NO (inline) | cb:11769 |
| golem slots `.min(3)` | Elementalist | CAP | 3 | NO (inline, rank-fed) | cb:12162 |
| bloomingfield bounce `.min(3)` | Druid | CAP | 3 | NO (inline) | cb:11771 |
| VOLATILE_MAGIC_MAX_TARGETS | Mage crit-splash | CAP | 2 | NO | cb:192 |
| ELEMENTAL_LIGHTNING_MAX_STACKS / DIVINE_ENEMY 200/100 | boss debuff stacks | CAP | 200/100 | NO | cb:346/:353 |
| CTHULHU_DEBUFF_CAP | boss debuff | CAP | 0.9 | NO | cb:401 |
| vampiricfrenzy `.clamp(0.0, 0.9)` | FlickerStrike cd reduction | CAP | 0.9 | NO (inline) | cb:10990 |
| Bloodpact cooldown floor 2000ms / Flicker floor 200ms | Slayer cadence floors | THRESHOLD | consts | NO | cb:10998/:10990 |
| RIGHTEOUS_FIRE_TICK_INTERVAL_MS / CLEANSING_FLAMES_TICK_INTERVAL_MS | burn/cleanse cadence | RATE | 1000/4000 ms | NO | cb:239/:257 |
| pierce_cap/pierce_h, boss HP/DMG floors+ceilings | boss-side | CAP | various | YES (b) | tunables.rs |

**Owner position check:** agreed — a cap is a balance decision. Today exactly ONE passive-output cap is dialable (`defensive_stat_hard_cap`) plus the boss-side ceilings; the entire overflow-conversion economy (the strongest scaling in the game, since input scales with GEAR) is pinned by two compile constants (`OVERFLOW_CONVERSION_CAP_PER_RANK`, `overflow_cap()`). The Monk trio (stonefist+graniteskin+risingdefiance) draws three independent IncreasedDamage channels off the SAME evasion overflow pool; with gear-driven overflow each channel saturates at its hardcoded 10%/rank ⇒ up to +90% increased damage (≈×1.9 on the increased-damage layer — the observed "free ×2.00") with no dial anywhere. Making the per-rank cap a LiveTunable alone would let the owner nerf ALL 13 conversion nodes with one number; per-node caps would need a fourth value slot per node (structure change — see §4/OQ2).

## 2. MISPLACED CONTROLS (on `/admin/tunables`, belong on `/admin/passives`)

| control | owner | move = pure render change? |
|---|---|---|
| `rf_self_damage_pct_rank1..3` | Righteous Fire (Paladin) | **Yes.** Storage can stay `LiveTunables`; only the form HTML + save handler grouping change. Value is read via a rank-match in combat.rs:11945-50 regardless of which page edits it. CAUTION: keep `TunablesForm` fields required-or-`#[serde(default)]` and derive the POST test from the rendered page (CLAUDE.md trap). |
| `haloedsteps_per_instance_pct_rank1..3` | Haloed Steps (Cleric) | **Yes** — same shape, same caveat. Read path cb:12800 is page-agnostic. |
| `shattering_enabled` + `shattering_damage_pct_rank1..3` | Shattering (Elementalist) | **Yes**, plus: delete/repair the node's INERT TOML entry (§3) so the node stops showing a fake editable magnitude. The kill-switch checkbox arguably belongs on either page; recommend moving with its pct trio. |
| `verdantburst_echo_threshold_pct` | Verdant Burst (Druid) | **Yes** — single-value second aspect of a node, exact `rf_*` precedent. |
| `thunder_redistribution_pct` / `_window_secs` | Thunder Golem (Elementalist) | **Recommend moving** to a class section; slightly less clear-cut since it's a party-wide combat rule, but it is a class mechanic dial, not a world dial. |
| `defensive_stat_hard_cap` | doctrine-global | **Keep on /admin/tunables** — genuinely global (doctrine: no immunity through DR). |
| everything else on the page (loot/sand/drops, boss health/power, pacing anchors, splash ladder, reactive_proc_cap_ms, buffsnapshot_dedupe_window_ms, fight batching) | world/combat infra | correctly placed |

Net: **6 controls (14 fields) move; all moves are render-layer only under the recommended storage-unchanged approach.** The alternative — migrating second-aspect values INTO the node's override array — is a structural change (a node would need TWO magnitude tables; today `PassiveEffect` carries exactly one, and the convention doc at tunables.rs:18-31 deliberately forbids overloading it).

---

## 3. INERT OVERRIDES

**Group A — named in the brief (confirmed):**

| key | class | consumer (rank-only) | TOML entry |
|---|---|---|---|
| chakraoflife | Monk | cb:11997 `duration = rank × 1000ms` | [.33,.66,1.0] inert |
| unyieldingspirit ("Last Stand") | Monk | cb:11451-59 threshold ladder 25/35/45/55% by rank | [.33,.66,1.0] inert |
| shattering | Elementalist | cb:6139 target count = rank; damage pct moved to LiveTunables | [2,4,6] inert |

**Group B — SAME SHAPE, not tracked by any repo list (new finds; `/admin/passives` currently OFFERS these as editable):**

| key | class | what the rank secretly controls | site |
|---|---|---|---|
| deathdefiant | Berserker | Gambit grace = rank×3000ms | cb:11606 |
| timewarp | Mage | burst window = 5000+2000×rank ms | cb:10944/:10957 |
| demonicspeed | Warlock | same shape as timewarp | cb:10945/:10957 |
| unwavering | Paladin | party-DR threshold ladder 0/.50/.65 | cb:10974-80 |
| unyieldingfaith | Cleric | same ladder | cb:10968/10974-80 |
| huntersfocus | Ranger | ally share = rank/3 of Predator's Eye | cb:12012 |
| golemmaster | Elementalist | golem slots = rank (also manager.rs:3003, adventure_web.rs:4760) | cb:12162 |
| healingflames | Elementalist | regen = irregular 3/6/10% fn(rank) | cb:11955, fn :6536 |
| blazing | Elementalist | Flame Golem AS = irregular 6/9/18% fn(rank) | cb:6090, fn :5943 |
| risingphoenix | Elementalist | max revives = rank.min(3) | cb:11958 |
| virulence | Warlock | Soul Stone count = rank (−33%/use is const SOUL_STONE_DMG_PENALTY_PER_USE, cb:152) | cb:12032 |
| cursedblood | Warlock | auto-curse target count = rank | cb:12033 |
| livingbond | Druid | Wild Roar charges = rank (#39 test pins it as a COUNT unit) | cb:11297 |
| naturesembrace | Druid | heal-target count = rank | cb:11298 |
| verdantburst (charges) | Druid | save charges = rank (threshold half is state b) | cb:11701 |
| finaloffering | Slayer | unlock-use ladder by rank AND −33% flat hardcoded | cb:11565-68 |
| sacrifice — damage half | Slayer | Bloodpact dmg mult = 1+rank (cost half IS wired via magnitude) | cb:11558 |

**Group C — MIXED nodes: one aspect wired, a second aspect silently reads rank** (an override works for the primary value, does nothing for the secondary):

| key | wired aspect | rank-fed aspect |
|---|---|---|
| naturesblessing (Druid) | FlatStat crit-chance pool | r2/r3 heal-crit bonus ladders cb:11729/:11733 |
| bloomingfield (Druid) | heal-power mult chr:3313 | bounce-target count `(1+rank).min(3)` cb:11771 |
| mercifultouch (Cleric) | — | bounce-value branch keyed on rank cb:11776ff (**flagged — verify body**) |
| reaperscall (Berserker) | chain chance mag cb:11671 | chain max-extra = rank cb:11672 |
| ravage (Warlock) | ramp mult cb:12069 | r≥3 stack ladder cb:12068 |
| unrelenting (Slayer) | duration via mag cb:12082 | r≥3 bonus ladder cb:12079 |
| endlessthirst (Slayer) | cap bonus mag cb:12091 | "uncapped at r3" gate cb:12092 |

**Group D — already tracked (`PENDING_MIGRATION_NODES`, 28 keys; page shows "pending", offers no input; overrides equally inert):**
assassinate, bloodrush, bloodscent, clarity, compassion, crush, deathwish, doubletap, finalblow, frenzy, gloriousdeath, guardianspirit(Cleric), lastlaugh, lastrites, markedfordeath, neverending, payback, piercingshots, quickdraw, reckless, relentlessassault, sanctifiedtouch, secondwind, shatter(Berserker), surgicalstrike, undying(Slayer), undyingwill, vitalstrike.

**Group E — unwired entirely:** stillwater (Monk, passive_tree.rs:959) — zero consumers anywhere.

Count: the brief's 3 + **17 Group-B drifts + 7 Group-C mixed halves** beyond them. The repo's own lists have drifted from combat.rs reality — recommend a CI test asserting "any node whose ONLY reads are `_rank` must appear in PENDING or UNWIRED" so this silent-no-op class can't regrow (the existing `every_unwired_key_still_exists_in_the_tree` test checks existence, not consumption).



## 4. UNREACHABLE

The (d) list — numbers with NO override path today — and what each would take:

| item | where | what it would take |
|---|---|---|
| **OVERFLOW_CONVERSION_CAP_PER_RANK** = 0.10 | cb:342, applied chr:2640 | New `LiveTunables` field `overflow_conversion_cap_per_rank` + thread the manager's tunables into `passive_overflow_bonus`'s caller path (the sim already receives tunables for haloedsteps/shattering, so plumbing exists). HOT-able; behavior-neutral at 0.10. **This is the Monk ×2 dial.** |
| Input overflow caps (0.75 DR/Block/Evasion, 0.50 Intervene) | passive_tree.rs:400-406 | Either three new LiveTunable fields (`evasion_overflow_cap`, …) read via a tunables parameter into `combined_stat_overflow`, or accept as structural. Recommend tunable — they directly gate how much raw overflow the conversions eat. |
| stillwater mechanic missing entirely | passive_tree.rs:959 | Structural: write the consumer (guaranteed Serenity on first N evades), reading rank or count like siblings. Not a key/field job. |
| Rank-ladder values behind PENDING nodes (28) | various combat.rs sites | Per node: declare true values in tree (linear → Special coefficients; non-linear → `SpecialPerRank`, pattern proven by absolutezero/arcaneinstability/empoweredbolt/infiniteloop/finaljudgment migrations + their tests), then switch the call site from `_rank` ladder to `_magnitude`. Behavior-neutral at defaults; golden corpus must stay green. |
| Group-B drift nodes (17) + Group-C halves (7) | §3 tables | Same migration shape as PENDING, plus add them to the repo's tracking lists so the page stops offering dead inputs in the meantime (a one-line list edit each — do this FIRST as pure hygiene). |
| Stack-max bases (MOMENTUM/FLEETFOOT/BLOODLUST/RELENTLESS/FLOWSTATE/FLOWING consts) + their `.min(8)` ceilings | cb:290-313, :11827-60 | New LiveTunable fields per family base (6 fields) OR one shared `stack_speed_max_stacks_base` if owner wants one knob; inline `.min()` ceilings become max(base+ext, ceiling) reads. HOT-able (read at unit construction). |
| Tick/cadence constants: RIGHTEOUS_FIRE_TICK_INTERVAL_MS 1000, CLEANSING_FLAMES_TICK_INTERVAL_MS 4000, TEMPLE_GUARDIAN 5000, thickhide/symbiosis cycle (already magnitude-fed ✓), FEL_RUSH_DURATION 4000 | cb:239/:257/:176 etc. | One LiveTunable field each where wanted; all read at fight-construction time ⇒ HOT. Low priority except RF cadence (player-visible burn pacing). |
| Duration constants family (~40× "…_DURATION_MS") | cb:66-377 | Doctrine Decision 16 territory (shared/structural); recommend leaving unless a specific buff needs tuning. If promoted, batch as one `durations` table rather than 40 fields. |
| GOLEM_STAT_SCALE 0.33 (+per-golem damage penalty text "33% less per golem" hardcoded in spawn math) | cb:6041 region | Two LiveTunables (`golem_stat_scale`, `golem_player_damage_penalty_per_summon`) if the owner wants golem economy dials; currently compile-time. |
| SOUL_STONE_DMG_PENALTY_PER_USE 0.33 | cb:152 | Single LiveTunable; virulence count itself becomes tunable via the §3 migration. |
| Boss-side caps already live (pierce, HP/DMG floors/ceilings) | tunables.rs | Already reachable — listed for completeness. |

Rule of thumb applied: **anything read during `CombatSimUnit` construction (per-fight) can be HOT**; nothing here requires the OnceLock/restart treatment — that's specific to the item-balance file's load-once consumers.

---

## 5. SIZE IT
Independently shippable stages, smallest first. Each is one branch/one deploy candidate; none touches the wiki module.

| stage | contents | estimate |
|---|---|---|
| **0 — hygiene** | Add Group-B/C keys to the repo's tracking lists so `/admin/passives` stops offering dead inputs; decide & clean the suspicious live TOML entries (OQ5); fix `riptide` [.5,.1,.15]. No combat.rs changes; golden corpus untouched. | **half a day** |
| **1 — Monk ×2 dial + caps** | `overflow_conversion_cap_per_rank` (+ optionally input caps) as LiveTunables on /admin/tunables under a "passive economy" group. Plumbing precedent exists (haloedsteps). Includes page-derived POST test. | **1 day** |
| **2 — drift migration (Groups B+C)** | 17+7 sites switched from rank-ladders to magnitude/count reads with true values declared in-tree; behavior-neutral at defaults; extend stale-read tests like #39. Makes every /admin/passives input honest. | **2–3 days** |
| **3 — PENDING 28 migration** | Same pattern per docs/passive_tunables_spec.md; mechanical but wide; splittable per archetype. | **3–4 days** |
| **4 — page reorg (b→a)** | Move rf/haloedsteps/shattering/verdantburst/thunder dials into /admin/passives class sections; keep LiveTunables storage (render+handler only); keep hard-cap global. Best after 2–3. | **1 day** |
| **5 — optional extras** | stillwater mechanic; stack bases + RF cadence tunables; golem economy fields; duration-table decision. | **2–3 days** |

**Total ≈ 9–12 working days (~a fortnight at care level); an afternoon delivers Stage 0+1** — enough for a real dial on the Monk ×2 problem and an end to silent no-ops. Stages 2–4 unblock end-to-end class tuning and are order-independent except 4-after-2/3.




## 6. RELOAD SEMANTICS

For each proposed tunable, HOT vs RESTART:

| proposed field/group | mechanism | verdict | why |
|---|---|---|---|
| All §3 migrations (node magnitudes/counts) | existing override store (`LazyLock<RwLock>`, swapped on save) | **HOT** | `magnitude_at_rank` consults it on every read; the save path already swaps live. |
| `overflow_conversion_cap_per_rank` | new LiveTunable field | **HOT** | must be plumbed into the per-fight unit-construction path where tunables is already in scope; do NOT cache in `OnceLock`/`LazyLock`. |
| input overflow caps (if promoted) | new LiveTunable fields | **HOT** | same construction-time read; `PassiveStat::overflow_cap()` would take the value as a parameter instead of returning consts. |
| stack-max bases / `.min()` ceilings | new LiveTunable fields | **HOT** | consumed at CombatSimUnit build per fight. |
| rf/haloedsteps/shattering/verdantburst/thunder second-aspects (post-move) | keep `LiveTunables` storage | **HOT** unchanged | RwLock re-read every fight (tunables.rs:3-16). |
| tick cadences (RF etc.) if promoted | LiveTunable | **HOT**, next-fight caveat | interval bakes into `next_*_tick_at_ms` scheduling at unit build; mid-fight saves apply from the next fight — consistent with owner expectations elsewhere. |
| anything via `ItemBalanceFile`/OnceLock | item-balance pattern | **RESTART — do not use** | load-once-per-process consumers; routing class dials through it would recreate the exact HOT/RESTART confusion this section exists to prevent. |
| accompanying form fields | serde discipline | n/a | every new optional form field needs `#[serde(default)]`; POST tests must scrape field names from the rendered page (CLAUDE.md 2026-08-23 trap, both directions). |



## OPEN QUESTIONS

1. **Storage for moved (b) dials:** keep `LiveTunables` and relocate presentation only (recommended), or migrate second aspects into per-node override arrays (structural — a node has exactly one magnitude table by doctrine)?
2. **Monk trio remedy:** one global `overflow_conversion_cap_per_rank` dial (hits all 13 conversion nodes at once), or per-node output caps (needs a 4th value slot per node — structure change)? The free ×2.00 also scales with gear-driven evasion-overflow volume — should the input caps move too?
3. **Rank-as-count nodes documented as deliberate** (golemmaster, risingphoenix, shattering count, virulence, cursedblood, livingbond, naturesembrace, verdantburst): keep untunable-by-design, or migrate to `INTEGER_COUNT_NODES`-style counts (precedent: stormcaller, bloodsac) so counts can exceed rank defaults?
4. **Globally unique keys vs shared mechanics:** TOML keys are one flat namespace — Berserker's deathmark and Ranger's deathmark are different nodes but the audit found only one entry per key; overrides on such keys hit exactly one node (keys are globally unique in the tree). Confirm no cross-class intent was assumed when those entries were written.
5. **Live TOML entries that look semantically off vs their units** — confirm intent before cleanup touches them: riptide [.5,.1,.15] (non-monotonic), stormcaller [3,6,9] (count semantics → targets become 3/6/9), sanctuary & barrier [.33,.66,1.0] (party DR at/above the .95 hard cap ⇒ near-immunity from those sources), vampiricfrenzy r3 .90 (exactly on the clamp), riftofmercy [3,6,9] and thundergolem [3,6,9] vs seconds-based semantics, chakraofmany/chakraoflight scaled below declared defaults.
6. **Mixed-half nodes (Group C):** migrate the secondary halves too, or leave those ladders structural?
7. **stillwater:** build the missing mechanic or retire the node?
8. **Cap promotion scope:** overflow dial first; stack bases / cadences / golem economy now or wait for a felt need?

---

*Abbreviations: MAG = MAGNITUDE, THR = THRESHOLD, CAP, RATE. cb = game/src/adventure/combat.rs, chr = game/src/adventure/character.rs, "pool" = generic FlatStat pooling (character.rs:2571) or overflow pooling (:2630). All line numbers against `master @ f8af51c`. This file is the only artifact produced by this session; no code, config, or test files were modified.*

