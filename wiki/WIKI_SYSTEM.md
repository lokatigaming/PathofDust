# How the in-game wiki works

This is a note for future sessions touching `/wiki`, not player-facing content -
it isn't served by any route. The actual system lives in
`src/adventure_web/wiki.rs`.

## Routes

Three routes cover the whole wiki, registered in `adventure_web.rs`:

- `/wiki` - the landing page/table of contents (`wiki_page`, content in `wiki/landing.md`).
- `/wiki/passives` - its own route since it's code-generated, not a `.md` file (see below).
- `/wiki/:page` - **dynamic**, serves any `wiki/{page}.md` that exists (`wiki_dynamic_page`), 404s otherwise.

Axum's router always prefers a literal path segment over a `:param` one
regardless of registration order, so `/wiki/passives` reaching its own
explicit handler instead of the dynamic route isn't a fragile ordering thing
- it can't be shadowed.

**A new prose page needs zero Rust changes and zero rebuild** - write the
`.md` file, add it to `wiki/nav.json` if you want it in nav, done. See
"Adding a new wiki page" below.

## Where the prose lives

Every page except Passives is hand-written Markdown under this directory,
one file per page (`landing.md`, `bosses.md`, `crafting.md`, `healing.md`,
`classes.md`, `combat.md`, `commands.md`, `dashboard.md`, `getting-started.md`,
`items.md`, ...).

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
inline wherever a page needs something Markdown can't express (the bespoke
`.wiki-currency-grid`/`.wiki-ceiling`/`.wiki-tier-grid` widgets, or an
`<h3 id="...">` that needs an anchor id) - CommonMark passes block-level raw
HTML through untouched as long as it isn't interrupted by a stray blank line
that would split it into a separate block partway through a tag.

**`wiki_dynamic_page` only ever serves a file whose name passes
`is_valid_wiki_slug`** - ASCII lowercase letters, digits, and hyphens only.
This is what keeps request input from ever reaching the filesystem
unvalidated (no `.`/`/`/`\` means no path-traversal string is even
expressible), and as a side effect it's also why this very file and
`QUESTIONS_FOR_OWNER.md` can safely live in `wiki/` without ever becoming
accidentally web-servable - their uppercase/underscore names simply fail the
slug check, no separate exclusion list needed.

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
`crate::adventure::NAME` with **zero visibility changes needed**. A few
placeholders (chat-command cooldowns, mainly) needed a one-line
private-to-`pub(crate)` visibility flip in their own top-level module first -
still zero behavior change, same boundary exception. The one thing that
genuinely can't be wired without a real code change is a `const` declared
*inside* a function body (block-local, not a module item) - that has no path
at all and needs hoisting to module scope first. If you hit one of these,
that's outside this module's own boundary; ask before making it rather than
doing it unilaterally, or (as this project did throughout) log the specific
bare literal that needs a name to `WIKI_IMPACT.md` as a request and describe
the mechanic qualitatively in the meantime instead of hardcoding a number.

## Adding a new wiki page

1. Write `wiki/<name>.md` - a `<div class="card">...</div>` (or `wiki-wide`
   for anything table-heavy) containing the page's content. Use
   `{{PLACEHOLDER}}` for any real number; see above.
2. **That's it for the page to exist and be reachable** - `/wiki/<name>` now
   serves it, live, no rebuild.
3. To put it in the nav bar, add a `{"slug": "<name>", "label": "<emoji + label>"}`
   entry to `wiki/nav.json` - also no rebuild, `wiki_subnav` re-reads the
   manifest on every request.
4. To put it on the landing page's table of contents too (a fuller
   description than nav's short label), add a line to `landing.md` by hand -
   this one *is* hand-maintained prose, not manifest-driven, since each ToC
   entry carries its own descriptive sentence rather than just a label.
5. If the content needs a real number wired to a game constant that isn't in
   `wiki_placeholder_map()` yet, that step alone needs a `wiki.rs` code
   change (and a rebuild) - everything else above doesn't.

## The anchor-compat rule

Chat's boss-alert messages link to `https://.../wiki#<slug>` (see
`BossKind::wiki_slug()` in `manager.rs`) - fragments that predate the
multi-page split, back when `/wiki` was one long page. Since a `#fragment`
never reaches the server, `/wiki`'s landing page carries a small inline script
(`wiki_hash_redirect_script`, in `wiki.rs`) that maps each known anchor id to
the page that now actually contains it, and client-side-redirects there.

**If you add a new `id="..."` anchor to any page**, add its slug to that
script's map too. Nothing keeps the two in sync automatically - the map is a
hand-maintained mirror of every `id` attribute across `bosses.md`/
`crafting.md`/`healing.md` (the only pages old fragment links ever pointed
into).

## Keeping prose in sync with the game

Other sessions changing player-facing game behavior log it in
`WIKI_IMPACT.md` at the repo root - that file is this system's inbox. When
picking it up: read the entry, check whether the wiki text it names is still
accurate, and either fix the prose (if it's just wrong) or wire a
`{{PLACEHOLDER}}` (if there's now a named constant backing the number). A
change tagged `affects passives` usually needs nothing done at all, since that
section renders live from the same node data the game itself uses - it can't
drift by construction.

The same file is also this session's *outbox* for one specific thing: when a
`.md` page needs a real number that's currently a bare, unnamed literal in
the game code, log a hoist request there (see the "WIKI SESSION REQUEST"
block partway through the file for the pattern) instead of hardcoding the
number - `wiki_placeholder_map()`'s own doc comment explains why.

## Coverage

`QUESTIONS_FOR_OWNER.md` in this directory tracks bugs found while auditing
wiki content against the actual game code (routed to the primary session to
fix) and open design questions the wiki works around by documenting current
behavior rather than guessing. Worth a skim before writing new content in an
area it already covers.
