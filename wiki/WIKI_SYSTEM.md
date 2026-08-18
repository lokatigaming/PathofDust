# How the in-game wiki works

This is a note for future sessions touching `/wiki`, not player-facing content -
it isn't served by any route. The actual system lives in
`src/adventure_web/wiki.rs`.

## Routes

`/wiki` is a landing page/table of contents (`wiki/landing.md`). The real
content lives at `/wiki/bosses`, `/wiki/crafting`, `/wiki/healing`, and
`/wiki/passives` - one page per section, so visiting any one doesn't force a
render of the others. Routes are registered in `adventure_web.rs` next to
every other route; that's the one line outside `wiki.rs` this system normally
touches.

## Where the prose lives

Every section except Passives is hand-written Markdown under this directory:

- `landing.md` - the `/wiki` table of contents
- `bosses.md`, `crafting.md`, `healing.md` - one file per sub-page

**Passives is the deliberate exception.** `/wiki/passives` is fully
code-generated (`render_wiki_passives` / `render_wiki_archetype_graph` in
`wiki.rs`), drawing the exact same node-graph component the interactive
`/passives` page uses, so the two views can never drift apart. There is no
`passives.md` and there shouldn't be one - if you're tempted to hardcode a
class's passive tree as prose, don't; extend the code-generated path instead.

Each `.md` file is read from disk **at request time**, not baked into the
binary at compile time - `render_markdown_page` in `wiki.rs` caches the
rendered HTML keyed by the file's mtime, so editing a `.md` file takes effect
on the very next page load with no rebuild or restart. The Passives section,
having no external input, is cached once per process instead (a `LazyLock`),
since redoing 11 full tree layouts on every request would be pure waste.

Content is parsed as real CommonMark (`pulldown-cmark`, `ENABLE_TABLES`) -
write normal Markdown for headers/paragraphs/lists/tables. Raw HTML is fine
inline wherever a section needs something Markdown can't express (the bespoke
`.wiki-currency-grid`/`.wiki-ceiling`/`.wiki-tier-grid` widgets, or an
`<h3 id="...">` that needs an anchor id) - CommonMark passes block-level raw
HTML through untouched as long as it isn't interrupted by a stray blank line
that would split it into a separate block partway through a tag.

## The `{{PLACEHOLDER}}` mechanism

Any `{{CONSTANT_NAME}}` token in a `.md` file is substituted **before**
Markdown parsing, against the map built by `wiki_placeholder_map()` in
`wiki.rs`. That map pulls real values straight from the game's own code -
`crate::adventure::SOME_CONST`, or `CraftAction::X.base_cost()` for the
token-craft dust costs (deliberately *not* `craft_action_def(X).default_cost`,
since `base_cost()` is the one that honors `adventure-item-balance.toml`
overrides - the wiki should always show the live price, not just the
compiled-in default).

An unresolved name renders loudly as `[MISSING: NAME]` instead of silently
vanishing, so a typo or a renamed/removed constant is obvious on the page
itself, not just in a diff.

**Reachability, in practice:** `combat.rs`, `item.rs`, `craft.rs`,
`character.rs`, and `manager.rs` are all private submodules of `adventure.rs`,
but `adventure.rs` re-exports each with `pub use {module}::*;`. That means any
`pub(crate)` constant in those files is *already* reachable from `wiki.rs` as
`crate::adventure::NAME` with **zero visibility changes needed** - this is how
almost every placeholder in this system got wired without ever touching those
files. The one thing that doesn't work this way is a `const` declared *inside*
a function body (block-local, not a module item) - that has no path at all and
needs hoisting to module scope first before it can be imported anywhere. If
you hit one of these, that's a real code change outside this module's own
boundary; ask before making it rather than doing it unilaterally.

## Adding a new wiki page

1. Write `wiki/<name>.md` - a `<div class="card">...</div>` (or `wiki-wide`
   for anything table-heavy) containing the section's content.
2. Add a handler in `wiki.rs` following the existing pattern (see
   `wiki_bosses_page` etc.): resolve the optional character via
   `resolve_wiki_character`, then render
   `top_nav(...) + wiki_crumb("/wiki", "All wiki sections") + wiki_subnav("<name>") + render_markdown_page("<name>")`.
3. Register `/wiki/<name>` in `adventure_web.rs`'s route list.
4. Add `<name>` to `wiki_subnav`'s four (now five) entries, and add a line to
   `landing.md`'s table of contents.
5. If the content needs a real number, use `{{PLACEHOLDER}}` and add the
   matching entry to `wiki_placeholder_map()` rather than hand-typing it.

## The anchor-compat rule

Chat's boss-alert messages link to `https://.../wiki#<slug>` (see
`BossKind::wiki_slug()` in `manager.rs`) - fragments that predate the
multi-page split, back when `/wiki` was one long page. Since a `#fragment`
never reaches the server, `/wiki`'s landing page carries a small inline script
(`wiki_hash_redirect_script`, in `wiki.rs`) that maps each known anchor id to
the sub-page that now actually contains it, and client-side-redirects there.

**If you add a new `id="..."` anchor to any section**, add its slug to that
script's map too. Nothing keeps the two in sync automatically - the map is a
hand-maintained mirror of every `id` attribute across `bosses.md`/
`crafting.md`/`healing.md`.

## Keeping prose in sync with the game

Other sessions changing player-facing game behavior log it in
`WIKI_IMPACT.md` at the repo root - that file is this system's inbox. When
picking it up: read the entry, check whether the wiki text it names is still
accurate, and either fix the prose (if it's just wrong) or wire a
`{{PLACEHOLDER}}` (if there's now a named constant backing the number). A
change tagged `affects passives` usually needs nothing done at all, since that
section renders live from the same node data the game itself uses - it can't
drift by construction.
