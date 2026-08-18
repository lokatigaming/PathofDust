// Page-structure templates (2026-08-18, Phase 1 pilot) - the same
// "live-edit, no rebuild" goal the wiki module already achieves for its
// markdown prose (see wiki.rs's WIKI_MD_CACHE), extended to the app's own
// page structure via a real template engine instead of regex placeholder
// substitution, since these pages need loops/conditionals the wiki's
// simpler content never did. Game logic/routes/data stay in Rust - only
// page STRUCTURE moves to templates/*.html.
//
// Autoescaping is deliberately OFF (see `build_env`) - every value handed
// to a template from Rust is already escaped exactly the way this
// codebase already does it (`escape_html`, see adventure_web.rs), same
// convention as every existing `format!`-built page. Turning minijinja's
// own HTML autoescaping on would double-escape those values and would
// also escape intentionally-raw fragments (e.g. `top_nav`'s own output,
// which stays a Rust-rendered HTML string per this project's boundary -
// see WIKI_IMPACT.md/the project plan) unless every one of them were
// individually marked `|safe`. Off matches today's behavior exactly and
// keeps the migration a pure structure move, not a new escaping regime.

use minijinja::{path_loader, Environment};
use minijinja_autoreload::AutoReloader;
use serde::Serialize;
use std::sync::LazyLock;

const TEMPLATE_DIR: &str = "templates";

fn build_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_loader(path_loader(TEMPLATE_DIR));
    env.set_auto_escape_callback(|_name| minijinja::AutoEscape::None);
    env
}

/// One reloader for the whole process, same "built once, watches its own
/// directory, transparently rebuilt on the next use after a change"
/// shape as the wiki's own `WIKI_MD_CACHE` - `acquire_env()` below is
/// what does the mtime check on every call, so there's no separate
/// invalidation path to maintain here either.
static TEMPLATES: LazyLock<AutoReloader> = LazyLock::new(|| {
    AutoReloader::new(|notifier| {
        notifier.watch_path(TEMPLATE_DIR, true);
        Ok(build_env())
    })
});

/// Renders `name` (a file under `templates/`, e.g. `"characters.html"`)
/// against `ctx` (anything `Serialize` - a `#[derive(Serialize)]` struct
/// or `minijinja::context!{...}`). A missing template or a render error
/// (bad syntax, unknown variable, etc.) never panics a request - logs and
/// returns a visible error card instead, same "loud, not silent" failure
/// mode the wiki's own `[MISSING: NAME]`/`[wiki content missing]` markers
/// use, so a broken template shows up immediately on the page instead of
/// 500ing the whole request.
pub(crate) fn render_template(name: &str, ctx: impl Serialize) -> String {
    let env = match TEMPLATES.acquire_env() {
        Ok(env) => env,
        Err(err) => {
            tracing::error!("Failed to acquire template environment for {name}: {err}");
            return format!("<p class=\"muted\">[template engine error: {name}]</p>");
        }
    };
    let tmpl = match env.get_template(name) {
        Ok(tmpl) => tmpl,
        Err(err) => {
            tracing::error!("Failed to load template {name}: {err}");
            return format!("<p class=\"muted\">[template missing: {name}]</p>");
        }
    };
    match tmpl.render(ctx) {
        Ok(rendered) => rendered,
        Err(err) => {
            tracing::error!("Failed to render template {name}: {err}");
            format!("<p class=\"muted\">[template render error: {name}]</p>")
        }
    }
}

#[cfg(test)]
mod live_reload_tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    /// Proves the actual point of this whole module: editing a template
    /// file on disk changes what the NEXT render call produces, within
    /// the same process, with no rebuild - not just that minijiaja can
    /// render a static file once. Uses its own throwaway template file
    /// (never touches a real page's template) so a failure mid-test can't
    /// leave real page content mutated; the `TEMPLATES` reloader is a
    /// process-wide `static`, watching the whole `templates/` dir, so
    /// this test's own writes are naturally scoped to a file nothing
    /// else reads.
    #[test]
    fn editing_a_template_takes_effect_without_a_rebuild() {
        let path = format!("{TEMPLATE_DIR}/_live_reload_test.html");
        std::fs::write(&path, "before").expect("write initial template");
        let first = render_template("_live_reload_test.html", minijinja::context! {});
        assert_eq!(first, "before", "first render must reflect the file as initially written");

        // Filesystem mtime resolution isn't guaranteed sub-millisecond
        // everywhere - a short sleep guarantees the second write lands at
        // a strictly later mtime than the first, so the reloader has
        // real change evidence to notice.
        thread::sleep(Duration::from_millis(50));
        std::fs::write(&path, "after").expect("rewrite template");
        let second = render_template("_live_reload_test.html", minijinja::context! {});

        std::fs::remove_file(&path).ok();

        assert_eq!(second, "after", "editing the template file must change the very next render - no rebuild, no restart, no explicit cache-bust");
    }
}
