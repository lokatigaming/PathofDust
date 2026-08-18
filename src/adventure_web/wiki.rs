// The `/wiki` page - extracted verbatim from adventure_web.rs (2026-08-18,
// a pure code-motion refactor: zero behavior change, no renames, no
// cleanup). Every helper it leans on (top_nav, render_page,
// current_session, escape_html, compute_passive_layout, root_node_html,
// passive_archetype_icon_role, ...) is shared with other pages and
// deliberately stayed in the parent - reached here through `use super::*`,
// the same convention src/adventure/*.rs already uses to see adventure.rs.

use super::*;

/// Public - no login needed, same "pure reference content, same for
/// everyone" reasoning as patch-notes above (the two deliberate
/// exceptions to this dashboard's usual login gate). First section:
/// what each world boss actually does, so chat knows what it's walking
/// into before a fight - see `render_wiki_bosses`. Content is hand-
/// written prose baked into the binary (unlike patch-notes' JSON file),
/// since unlike a changelog this doesn't change fight-to-fight - editing
/// it is a real code change/redeploy, which is fine for how rarely boss
/// mechanics themselves change. Still resolves the session (if any) now,
/// purely so `top_nav`'s stat summary can show for a logged-in visitor -
/// the page itself stays fully viewable logged out.
pub(super) async fn wiki_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = match current_session(&headers, &state).await {
        Some((login, _)) => state.adventure.character(&login).await,
        None => None,
    };
    Html(render_page(&render_wiki(character.as_ref())))
}

fn render_wiki(character: Option<&Character>) -> String {
    format!(
        "{}{}{}{}{}",
        top_nav(character),
        render_wiki_bosses(),
        render_wiki_crafting(),
        render_wiki_healing(),
        render_wiki_passives()
    )
}

/// Public, read-only reference for every class's full passive tree - the
/// interactive `/passives` page (`render_passive_tree_page`) needs a
/// logged-in character (it's driven by THEIR current ranks/unlock state),
/// so it can't double as a "just let me look at the whole tree" page for
/// a logged-out visitor or someone comparing classes before picking one.
/// This instead renders the SAME node-graph component (`compute_passive_layout`/
/// `root_node_html`, the exact boxes-and-SVG-connectors tree `/passives`
/// itself draws) per class, one collapsible section each - not a
/// hand-duplicated text summary, so the two views can never drift apart.
fn render_wiki_passives() -> String {
    let sections: String = ALL_ARCHETYPES.iter().map(|&archetype| render_wiki_archetype_graph(archetype)).collect();
    format!(
        "<div class=\"card\">\
          <h2>Class Passive Trees</h2>\
          <p>Every class has its own tree: 1 root passive (always active, scales with level), 3 Skills you can rank up freely, \
          9 Specializations (3 per Skill, gated behind their Skill), and 27 Modifiers (3 per Specialization, gated behind pushing \
          that Specialization to 4/4 - the 4th point \"specializes\" it without adding another point of its own stat, but unlocks \
          its 3 Modifiers below). Shown fully unlocked below so every node is visible at once - a node flagged \
          <span class=\"muted\">(inactive)</span> doesn't do anything yet, though points spent there are banked, not wasted, and \
          will start working the day that mechanic ships.</p>\
        </div>{sections}"
    )
}

/// One class's full tree, drawn with the exact same graph renderer the
/// interactive `/passives` page uses - see `render_wiki_passives`'s doc.
/// Since this has no real character behind it, `allocations` is a
/// synthetic "every node maxed" map (every skill/spec/modifier's own
/// `max_rank`) rather than anyone's real ranks - the ONLY purpose here is
/// to make `compute_passive_layout` treat every Specialization as
/// unlocked (so every Modifier row actually gets drawn instead of hidden
/// behind its normal in-game gate), and it doubles as a natural "here's
/// what a fully-invested tree looks like" reference view. Read-only, same
/// "no forms, plain rank text" shape as `render_passive_tree_readonly`'s
/// own node_html.
fn render_wiki_archetype_graph(archetype: Archetype) -> String {
    let (icon, role) = passive_archetype_icon_role(archetype);
    let role_class = archetype.css_class();
    let nodes = archetype.passive_nodes();
    let allocations: HashMap<String, u32> = nodes.iter().map(|n| (n.key.to_string(), n.max_rank)).collect();

    let layout = compute_passive_layout(archetype, &allocations);
    let PassiveLayout { positioned, root_x, svg_lines, stage_w, stage_h } = layout;

    let node_html = |p: &PosNode| -> String {
        let n = &p.node;
        let rank = n.max_rank;
        let not_yet = matches!(n.effect, crate::passive_tree::PassiveEffect::NotYetImplemented);
        let tip = if not_yet {
            format!("{} Not yet active - allocating still banks the point for when this mechanic ships.", escape_html(n.description))
        } else {
            escape_html(n.description)
        };
        let (kind_class, kind_label) = match n.tier {
            PassiveTier::Skill => ("node-skill", "Tier 1"),
            PassiveTier::Specialization => ("node-spec", "Specialization"),
            PassiveTier::Modifier => ("node-mod", ""),
        };
        let state_class = if n.max_rank == 4 { " node--specialized" } else { " node--maxed" };
        let dots: String = (0..n.max_rank)
            .map(|i| {
                let gold = n.max_rank == 4 && i == 3;
                format!("<span class=\"dot filled{}\"></span>", if gold { " dot-spec" } else { "" })
            })
            .collect();
        let kind_label_html = if kind_label.is_empty() { String::new() } else { format!("<div class=\"node-kind\">{kind_label}</div>") };
        format!(
            "<div class=\"node {kind_class}{state_class}\" style=\"left:{x}px;top:{y}px;width:{w}px;\" data-tip=\"{tip}\">\
              {kind_label_html}\
              <div class=\"node-name\">{name}{flag}</div>\
              <div class=\"dots\">{dots}</div>\
              <div class=\"node-buttons\"><span class=\"node-rank\">{rank}/{max_rank}</span></div>\
            </div>",
            x = p.x,
            y = p.y,
            w = p.w,
            name = escape_html(n.name),
            flag = if not_yet { " <span class=\"muted\">(inactive)</span>" } else { "" },
            max_rank = n.max_rank,
        )
    };

    let root_desc = escape_html(&archetype.description(1));
    let root_html = root_node_html(archetype, root_x, &root_desc);
    let nodes_html: String = positioned.iter().map(node_html).collect();

    format!(
        "<details class=\"card bag-row wiki-archetype\">\
          <summary>{icon} {archetype:?} <span class=\"role-badge role-{role_class}\">{role}</span></summary>\
          <p class=\"wiki-root-desc\"><strong>Root passive (always active):</strong> {root_desc} <span class=\"muted\">- shown at level 1; grows with level.</span></p>\
          <div class=\"ptree-page\">\
            <div class=\"tree-wrap\"><div style=\"width:{stage_w}px;height:{stage_h}px;position:relative;\">\
              <svg class=\"connectors\" width=\"{stage_w}\" height=\"{stage_h}\">{svg_lines}</svg>\
              {root_html}{nodes_html}\
            </div></div>\
          </div>\
        </details>",
    )
}

/// Hand-written, deliberately non-technical - a curious viewer or a
/// player deciding whether to !join should be able to read this without
/// knowing anything about how the game's actually coded. Numbers here
/// (thresholds, add counts, percentages) are pulled directly from
/// adventure.rs's own constants (TWO_BOSS_STAGE/THREE_BOSS_STAGE,
/// LICH_MAX_ADDS, the Fire Demon/Dragon aura magnitudes, Cthulhu's -90%)
/// - keep this in sync if any of those ever change.
fn render_wiki_bosses() -> String {
    "<div class=\"card\"><h1>Wiki</h1><p class=\"muted\"><a href=\"/\">&larr; Back to your character</a></p></div>\
    <div class=\"card\">\
      <h2>Bosses</h2>\
      <p>Every world boss fight pits the party against one or more of five named bosses. Each one plays completely differently - here's what to watch for.</p>\
      <h3 id=\"dragon\">🐲 The Dragon</h3>\
      <p>Hovers above the battlefield instead of standing on the ground, sweeping from one side to the other. Every 5 seconds it rears back and breathes fire across the <strong>whole party at once</strong> - unlike a normal attack, which only hits one hero, nobody is safe from this one. It also radiates a passive aura for the entire fight that slows everyone's attack speed. Two looks can show up in a Dragon fight - the classic purple dragon, or the rarer, fully-animated Bahamut - purely cosmetic, same fight either way.</p>\
      <h3 id=\"cthulhu\">🐙 Cthulhu</h3>\
      <p>Every 5 seconds, a purple bubble locks onto whoever's currently dealt the most damage this fight and permanently crushes THEIR damage output by 90% for the rest of it. The instant someone else takes the damage lead, the bubble follows them instead - no single hero can carry a Cthulhu fight alone; the whole party has to pull their weight.</p>\
      <h3 id=\"lich\">💀 The Lich</h3>\
      <p>Raises 5 skeletons to fight alongside it every 2 seconds, up to 20 in a single battle. Each one is individually weak - a fraction of the Lich's own health and attack - but they pile up fast if the fight drags on, so speed matters against this one.</p>\
      <h3 id=\"fire-demon\">🔥 Fire Demon</h3>\
      <p>No flashy abilities - just relentless heat. Its aura cuts the entire party's healing power in half for the whole fight, so healers get a lot less mileage out of every cast.</p>\
      <h3 id=\"gelatinous-cube\">🧊 The Gelatinous Cube</h3>\
      <p>Crawls across the field absorbing party members into its body - every few seconds it rotates a fresh batch of heroes into itself (scaled to how many are still standing), and while trapped they can't act, though allies can still damage or heal them normally. It also lashes out with a wide splash attack hitting several heroes at once, and every hit it lands stacks a shred on that hero's defenses, up to a steep cap - the longer a fight against the Cube drags on, the softer the party gets.</p>\
      <h3>Multiple bosses at once</h3>\
      <p>As the world stage climbs, more bosses join the fight at once - it's not a fixed number for a given stage, either: every fight rolls its own boss count with some real variance, trending upward the higher the stage, so two fights at the same stage can genuinely differ in size. It's always a different mix - the game never repeats the last fight's boss back-to-back, and won't repeat a kind WITHIN one fight either, unless there are more boss slots than distinct kinds left to fill them (only relevant at the very highest stages, since there are only five named kinds total).</p>\
      <p class=\"muted\">These five only show up in real world-boss fights. The smaller, more frequent filler encounters between them use weaker, unnamed enemies instead.</p>\
    </div>"
        .to_string()
}

/// Second wiki section - the crafting/item system, ported from the
/// standalone "Crafting Codex" reference doc into the in-game wiki so
/// players don't need a separate link to find it. Same "hand-written
/// prose baked into the binary" reasoning as `render_wiki_bosses` - the
/// numbers here (dust costs, crit chances, disenchant multipliers) are
/// transcribed from the actual constants/formulas in `item.rs`/
/// `character.rs`/`manager.rs` and need a code change to stay in sync,
/// same maintenance contract as the boss section above.
fn render_wiki_crafting() -> String {
    "<div class=\"card wiki-wide\">\
      <h2>Crafting</h2>\
      <p>Every currency, every action, and the one rule that governs them all: no item can ever carry more than seven modifiers, however you get there.</p>\
      <h3 id=\"currencies\">Currencies</h3>\
      <div class=\"wiki-currency-grid\">\
        <div class=\"wiki-currency-card\"><h4>Dust</h4><p>Earned from wins and boss kills. Pays for every currency-crafting action, Reforge, and Recombine.</p></div>\
        <div class=\"wiki-currency-card\"><h4>Sand</h4><p>Earned from wins and disenchanting. Spent exclusively on Polishing.</p></div>\
        <div class=\"wiki-currency-card\"><h4>Craft Tokens</h4><p>One kind per action (Transmute, Scour, Augment, Regal, Exalt, Krangle). Spending a token skips that action's dust cost entirely.</p></div>\
        <div class=\"wiki-currency-card\"><h4>Celestial Shard</h4><p>A rare token, separate from every other currency. The only way to grant a Unique Affix.</p></div>\
      </div>\
      <h3 id=\"ceiling\">The Modifier Ceiling</h3>\
      <p>The single rule worth memorizing before anything else here: <strong>four base modifiers, plus at most one each from three independent bonus sources.</strong> Seven, and never more &mdash; on any single item, for its entire life, no matter how many times it gets reforged or recombined.</p>\
      <div class=\"wiki-ceiling\">\
        <div class=\"wiki-ceiling-row\">\
          <span class=\"wiki-slot wiki-slot--base\">1</span><span class=\"wiki-slot wiki-slot--base\">2</span><span class=\"wiki-slot wiki-slot--base\">3</span><span class=\"wiki-slot wiki-slot--base\">4</span>\
          <span class=\"wiki-slot-plus\">+</span>\
          <span class=\"wiki-slot wiki-slot--bonus\" title=\"Reforge crit\">R</span><span class=\"wiki-slot wiki-slot--bonus\" title=\"Recombine crit\">C</span><span class=\"wiki-slot wiki-slot--bonus\" title=\"Krangle\">K</span>\
        </div>\
        <ul>\
          <li><strong>1&ndash;4</strong> &mdash; Transmute / Augment / Regal / Exalt, ordinary currency crafting.</li>\
          <li><strong>R</strong> &mdash; Reforge's rare crit, can land once, ever, on this item's whole lineage.</li>\
          <li><strong>C</strong> &mdash; Recombine's own separate crit, same one-time rule, tracked independently of Reforge's.</li>\
          <li><strong>K</strong> &mdash; Krangle, guarantees a bonus modifier, but permanently locks the item afterward.</li>\
        </ul>\
        <p class=\"wiki-ceiling-cap\">Hard ceiling: <strong>7</strong> modifiers total.</p>\
      </div>\
      <p>The R and C slots each carry a memory: once either crit has landed on an item, it can never land again &mdash; even if that item is later merged into something new by Recombine. A merged item inherits \"already used\" from either parent, so pairing a just-crit'd item with a fresh one can't reset its odds.</p>\
      <p>A Unique Affix (from a Celestial Shard) and Sacred's implicit affix live entirely outside this pool &mdash; they never count toward the seven, and no crafting action can touch them.</p>\
      <h3 id=\"currency-crafting\">Currency Crafting</h3>\
      <p>Six actions, each gated by exactly how many modifiers the target item currently has. Every one of them also bumps the item's tier as a side effect &mdash; <code>+3</code> below tier 25, <code>+2</code> below tier 50, <code>+1</code> beyond that.</p>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Action</th><th>Effect</th><th>Requires</th><th>Dust</th><th>Veilable</th></tr></thead>\
        <tbody>\
          <tr><td>Transmute</td><td>Adds a random modifier to a bare item.</td><td>0 modifiers</td><td>250</td><td>Yes</td></tr>\
          <tr><td>Augment</td><td>Adds a 2nd modifier.</td><td>1 modifier</td><td>500</td><td>Yes</td></tr>\
          <tr><td>Regal</td><td>Adds a 3rd modifier.</td><td>2 modifiers</td><td>750</td><td>Yes</td></tr>\
          <tr><td>Exalt</td><td>Adds a 4th modifier.</td><td>3 modifiers</td><td>1,250</td><td>Yes</td></tr>\
          <tr><td>Scour</td><td>Strips every modifier back to none.</td><td>1+ modifiers</td><td>250</td><td>No</td></tr>\
          <tr><td>Krangle</td><td>Adds one final modifier beyond the normal 4, then permanently locks the item &mdash; no further crafting of any kind, ever.</td><td>Any unlocked item</td><td>2,500</td><td>Yes</td></tr>\
        </tbody>\
      </table></div>\
      <p class=\"muted\">Every action above also costs <code>3&times;tier</code> extra dust on top of its base price &mdash; unless you spend that action's own Craft Token instead, which skips the dust entirely. A locked (Krangled) item is permanently excluded from all six.</p>\
      <h3 id=\"reforge\">Reforge</h3>\
      <p>Raises an item's tier and rescales everything it already has to match &mdash; the closest thing to a straightforward power-up. Comes in two forms.</p>\
      <p><strong>Reforge Now</strong> &mdash; a free action (via the dashboard button or its matching channel-points redemption), or <code>1,000</code> dust on demand. Always targets a random unlocked equipped item, replacing it with a freshly reforged version at the same slot.</p>\
      <p><strong>Crafting-Panel Reforge</strong> &mdash; costs <code>30&times;tier</code> dust and lets you pick the exact item, equipped or bagged, rather than leaving it to chance. No cooldown, unlike the free button.</p>\
      <p><strong>Tier jump:</strong> +2 to +4 below tier 50, +1 to +2 at tier 50&ndash;99, +1 at tier 100+.</p>\
      <p>Every reforge also has a chance to add a bonus modifier on top of the tier increase &mdash; the \"R\" slot from the ceiling above. The chance scales with the item's own quality:</p>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Item quality</th><th>Crit chance</th></tr></thead>\
        <tbody>\
          <tr><td>0% quality</td><td>1.0%</td></tr>\
          <tr><td>50% quality</td><td>1.5%</td></tr>\
          <tr><td>100% quality</td><td>2.0%</td></tr>\
          <tr><td><span class=\"gear-unique\">Perfect</span></td><td>2.2%</td></tr>\
        </tbody>\
      </table></div>\
      <p class=\"muted\">Once it fires, that's it for this item's lineage &mdash; permanently. Higher-quality gear is both better to begin with <em>and</em> slightly likelier to pick up that bonus modifier, for as long as it hasn't already.</p>\
      <h3 id=\"recombine\">Recombine</h3>\
      <p>Forges two items of the same slot into one, consuming both. The result inherits the better parts of each &mdash; not everything.</p>\
      <ul>\
        <li><strong>Tier:</strong> the average of the two sources, rounded down, plus one.</li>\
        <li><strong>Modifiers:</strong> any modifier type present on <em>both</em> sources is guaranteed to carry over (keeping the stronger of the two values). Each source's own unique modifiers get a coin-flip chance each, capped so the transferred pool never exceeds four.</li>\
        <li><strong>Power roll:</strong> a 50/50 coin flip between the two sources' rolls.</li>\
        <li><strong>Unique Affix &amp; durability:</strong> either source's Unique Affix carries over; the result is indestructible if either source was.</li>\
        <li><strong>Perfect &amp; Sacred never carry over.</strong> The result is always an ordinary item, regardless of what went in.</li>\
      </ul>\
      <p>Costs <code>500</code> dust, or one of your free recombines if you have one banked. A rare <strong>5% crit</strong> can add a bonus modifier on top &mdash; the \"C\" slot from the ceiling, tracked and gated exactly like Reforge's own crit, independently.</p>\
      <p class=\"muted\">Veiled Recombine guarantees the higher of the two power rolls and the certain transfer of every shared modifier &mdash; see Veiling below for the cost.</p>\
      <h3 id=\"polishing\">Polishing</h3>\
      <p>The only action priced in Sand instead of Dust, and the only one that improves rolls you already have rather than adding new ones.</p>\
      <p>On an ordinary item, Polishing nudges the primary stat's roll upward by a fixed step, and does the same to one random modifier that still has room to climb. On a <span class=\"gear-unique\">Perfect</span> item &mdash; whose primary stat is already maxed &mdash; it instead nudges up to two modifiers at once. An affix already sitting at its own cap is skipped automatically.</p>\
      <p class=\"muted\">Cost scales with how much room is left to improve: <code>ceil(quality% &divide; 10)</code> sand on a normal item (1 to 10), or a flat <code>12</code> sand on a Perfect one. Bypasses tokens and veiling entirely.</p>\
      <h3 id=\"celestial-shard\">Celestial Shard</h3>\
      <p>The only way to grant a Unique Affix &mdash; a permanent, implicit bonus that lives entirely outside the normal modifier pool.</p>\
      <p>Consumes a Celestial Shard token (never dust) to grant <strong>Celestial Conversion</strong>, whose effect depends on the wielder's role:</p>\
      <ul>\
        <li><strong>Healers</strong> convert 10% of every heal into bonus damage against a random enemy.</li>\
        <li><strong>Every other archetype</strong> instead lands a follow-up hit on whatever they just struck, for 10% of that hit's damage &mdash; a real second hit that can trigger Leech, elemental procs, and anything else an on-hit effect would.</li>\
      </ul>\
      <p class=\"muted\">Mutually exclusive with Krangle: an item carrying a Unique Affix can never be Krangled, and vice versa.</p>\
      <h3 id=\"item-tiers\">Item Tiers</h3>\
      <p>Beyond the normal 0&ndash;100% quality roll, two rarer tiers exist &mdash; both earned from boss kills, never crafted.</p>\
      <div class=\"wiki-tier-grid\">\
        <div class=\"wiki-tier-card\"><h4>Normal</h4><p>Rolls somewhere in the standard quality band. What every drop starts as.</p></div>\
        <div class=\"wiki-tier-card\"><h4><span class=\"gear-unique\">Perfect Quality</span></h4><p>Primary stat pinned to its maximum roll, plus a flat 20% bonus to that stat <em>and</em> every modifier. Guaranteed on stage-100+ boss kills &mdash; once per kill, and once more the first time each character personally takes part in one. Once Sacred also starts dropping (stage 300+), that per-kill guarantee only fires half as often.</p></div>\
        <div class=\"wiki-tier-card\"><h4><span class=\"gear-sacred\">Sacred</span></h4><p>Everything Perfect Quality gets, plus one further modifier &mdash; drawn from the full affix pool regardless of slot, rolled at its own maximum, shown as its own implicit line. Outside the 4-modifier pool entirely; no crafting action can ever touch it. Same drop mechanism as Perfect, gated to stage 300+.</p></div>\
      </div>\
      <h3 id=\"veiling\">Veiling</h3>\
      <p>Most crafts are a blind gamble &mdash; you commit, then see what you got. Veiling flips that: pay extra to see the exact result first, and choose whether to keep it.</p>\
      <p>Available on Transmute, Augment, Regal, Exalt, Krangle, and Recombine. <strong>Not</strong> available on Scour (nothing to choose &mdash; it's fully deterministic), Celestial Shard (always the same grant), Polishing (bypasses this system entirely), or Reforge.</p>\
      <p class=\"muted\">A veiled Recombine costs an extra <code>500</code> dust flat, plus <code>500</code> per modifier in the guaranteed-transfer pool.</p>\
      <h3 id=\"disenchanting\">Disenchanting</h3>\
      <p>Turns an unwanted item straight into Dust, valued by how much was invested in it.</p>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Normal modifiers</th><th>Ordinary</th><th>Perfect</th><th>Sacred</th></tr></thead>\
        <tbody>\
          <tr><td>0</td><td>1&times;</td><td>5&times;</td><td>25&times;</td></tr>\
          <tr><td>1</td><td>5&times;</td><td>25&times;</td><td>50&times;</td></tr>\
          <tr><td>2</td><td>10&times;</td><td>50&times;</td><td>75&times;</td></tr>\
          <tr><td>3</td><td>15&times;</td><td>75&times;</td><td>100&times;</td></tr>\
          <tr><td>4</td><td>20&times;</td><td>100&times;</td><td>125&times;</td></tr>\
        </tbody>\
      </table></div>\
      <p class=\"muted\">Sacred's bonus implicit counts as one more modifier toward this same table &mdash; a Sacred item with <em>N</em> normal modifiers is always worth exactly what a Perfect item with <em>N+1</em> would be.</p>\
      <p>Toggle <strong>Keep</strong> on any item to protect it from both single and bulk disenchanting &mdash; independent of Krangle's lock, and reversible any time. A Krangled item can still be disenchanted; locking only ever blocks further crafting.</p>\
      <p><strong>Auto-disenchant</strong> can be set to a threshold &mdash; Quality, Perfect, or Sacred &mdash; so any new drop at or below that tier is converted to Dust automatically the moment it's picked up, instead of filling the bag.</p>\
      <h3 id=\"quick-reference\">Quick Reference</h3>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Action</th><th>Currency</th><th>Base cost</th><th>Notable rule</th></tr></thead>\
        <tbody>\
          <tr><td>Transmute</td><td>Dust</td><td>250 + 3/tier</td><td>Bare items only</td></tr>\
          <tr><td>Augment</td><td>Dust</td><td>500 + 3/tier</td><td>1 modifier only</td></tr>\
          <tr><td>Regal</td><td>Dust</td><td>750 + 3/tier</td><td>2 modifiers only</td></tr>\
          <tr><td>Exalt</td><td>Dust</td><td>1,250 + 3/tier</td><td>3 modifiers only</td></tr>\
          <tr><td>Scour</td><td>Dust</td><td>250 + 3/tier</td><td>Not veilable</td></tr>\
          <tr><td>Krangle</td><td>Dust</td><td>2,500 + 3/tier</td><td>Locks the item forever</td></tr>\
          <tr><td>Reforge (free)</td><td>&mdash;</td><td>0</td><td>Random equipped item</td></tr>\
          <tr><td>Reforge (on demand)</td><td>Dust</td><td>1,000</td><td>Random equipped item</td></tr>\
          <tr><td>Reforge (crafting panel)</td><td>Dust</td><td>30/tier</td><td>Choose the exact item</td></tr>\
          <tr><td>Recombine</td><td>Dust</td><td>500</td><td>+500 veil, +500/modifier</td></tr>\
          <tr><td>Polishing</td><td>Sand</td><td>1&ndash;10, or 12 flat if Perfect</td><td>Not veilable</td></tr>\
          <tr><td>Celestial Shard</td><td>Shard token</td><td>1 token</td><td>Excludes Krangle</td></tr>\
        </tbody>\
      </table></div>\
    </div>"
        .to_string()
}

/// Third wiki section - general combat mechanics that apply across every
/// archetype, positioned right before Passives so a reader hits the
/// shared rules before running into each archetype's own shield-granting
/// skills (Divine Shield, Overflowing Grace, Arcane Shield, Consecration,
/// Seed of Life) down there. Same "hand-written prose baked into the
/// binary" reasoning as the other wiki sections.
fn render_wiki_healing() -> String {
    "<div class=\"card\">\
      <h2>Healing</h2>\
      <h3 id=\"shields\">Shields</h3>\
      <p>Shielding an already-shielded target adds the new amount on top of what's left (capped at 400% of their max HP) - shield amounts stack, they don't overwrite. Duration works differently: every shield cast resets the timer to a full fresh duration instead of adding to what's left, so re-shielding someone with only a second left doesn't stack more time on top of it - it just starts the clock over. That's still worth doing even on an already-maxed shield: no extra HP gets through, but the timer resets back to full for free.</p>\
    </div>"
        .to_string()
}
