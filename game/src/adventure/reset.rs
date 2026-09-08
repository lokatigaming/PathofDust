// The season reset, as a code path (2026-09-08).
//
// Before this, a reset was `rm` by hand against a runbook and NOTHING in
// the product decided what a season destroys. What survived survived
// because nobody typed its filename. This module makes the decision a
// value in the codebase and the deletion something that can be reviewed
// before it happens.
//
// THREE PROPERTIES, all of them because this deletes:
//
// 1. FAILS CLOSED. If the data directory holds an entry the table does
//    not declare, the reset REFUSES - in dry-run too. An unclassified
//    file means nobody has decided whether a season should destroy it,
//    and guessing either way is worse than stopping. It also refuses if
//    it cannot read the directory at all: "I could not look" must never
//    be mistaken for "there was nothing there".
//
// 2. DRY RUN IS THE DEFAULT. `game reset` prints the plan and deletes
//    nothing. Deleting takes an explicit confirmation token that has no
//    default and no abbreviation, so there is no flag ordering or typo
//    that turns an inspection into a wipe.
//
// 3. IT PRINTS THE CLASSIFICATION IT IS ACTING ON, not just the outcome.
//    An operator sees which entries are about to be destroyed AND which
//    are being kept, with the reason for each, BEFORE anything happens -
//    not a summary afterwards. A reset that says "deleted 39 files" has
//    told the operator nothing they could have disagreed with in time.

use std::path::{Path, PathBuf};

use super::stores::{store_named, Store, StoreKind, StoreScope};

/// The exact token that turns a dry run into a deletion.
///
/// Deliberately not `--yes` or `-f`: this is the most destructive
/// operation in the codebase, and the confirmation should be something
/// nobody types by muscle memory or leaves in a shell history by
/// accident.
pub const CONFIRM_TOKEN: &str = "DELETE-WORLD";

/// Why a reset refused to run. Both variants are refusals to ACT, never
/// partial work - nothing is deleted before the whole directory has been
/// checked.
#[derive(Debug)]
pub enum ResetRefusal {
    /// The data directory holds entries `stores.rs` does not declare.
    /// Carries every one of them, because an operator fixing this needs
    /// the list rather than the first example.
    Undeclared(Vec<String>),
    /// The data directory could not be read.
    Unreadable(std::io::Error),
}

impl std::fmt::Display for ResetRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResetRefusal::Undeclared(names) => {
                writeln!(f, "REFUSING TO RESET: {} entr{} in the data directory {} not declared in stores.rs:", names.len(), if names.len() == 1 { "y" } else { "ies" }, if names.len() == 1 { "is" } else { "are" })?;
                for name in names {
                    writeln!(f, "    {name}")?;
                }
                write!(
                    f,
                    "\nNobody has decided whether a season should destroy {}. Classify {} in stores.rs and run again.\nA reset that guesses is worse than one that stops.",
                    if names.len() == 1 { "it" } else { "them" },
                    if names.len() == 1 { "it" } else { "them" }
                )
            }
            ResetRefusal::Unreadable(err) => write!(f, "REFUSING TO RESET: the data directory could not be read: {err}\nAn unreadable directory is not an empty one - proceeding would delete against an unknown state."),
        }
    }
}

/// One entry the reset has an opinion about, paired with the reason it
/// is classified that way.
#[derive(Debug, Clone)]
pub struct PlannedEntry {
    pub name: String,
    pub kind: StoreKind,
    pub scope: StoreScope,
    pub why: &'static str,
}

/// What a reset would do, computed before anything is touched.
#[derive(Debug)]
pub struct ResetPlan {
    pub data_dir: PathBuf,
    /// Present, declared, and world-scoped: these get destroyed.
    pub to_delete: Vec<PlannedEntry>,
    /// Present, declared, and NOT world-scoped: these are kept. Shown
    /// rather than omitted, because "what survives" is the half an
    /// operator most needs to be able to disagree with.
    pub to_keep: Vec<PlannedEntry>,
}

/// Works out what a reset would destroy, or refuses.
///
/// Note what is NOT checked: whether every declared store exists.
/// Absence is the normal state of most of the table - stores are created
/// lazily on first write, and on a freshly reset world almost none of
/// them exist. Demanding presence would make this refuse on exactly the
/// world it protects. See `stores.rs` for the full reasoning.
pub fn plan_reset(data_dir: &Path) -> Result<ResetPlan, ResetRefusal> {
    let entries = std::fs::read_dir(data_dir).map_err(ResetRefusal::Unreadable)?;

    let mut present: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(ResetRefusal::Unreadable)?;
        present.push(entry.file_name().to_string_lossy().into_owned());
    }
    present.sort();

    // Fail closed FIRST, over the whole directory, before building any
    // delete list - so a refusal can never happen half way through.
    let undeclared: Vec<String> = present.iter().filter(|name| store_named(name).is_none()).cloned().collect();
    if !undeclared.is_empty() {
        return Err(ResetRefusal::Undeclared(undeclared));
    }

    let mut to_delete = Vec::new();
    let mut to_keep = Vec::new();
    for name in present {
        let store = store_named(&name).expect("every present entry is declared - the undeclared check above returned otherwise");
        let planned = PlannedEntry { name, kind: store.kind(), scope: store.scope(), why: store.why() };
        if planned.scope == StoreScope::World {
            to_delete.push(planned);
        } else {
            to_keep.push(planned);
        }
    }

    Ok(ResetPlan { data_dir: data_dir.to_path_buf(), to_delete, to_keep })
}

/// The plan as an operator reads it, before anything is deleted.
pub fn render_plan(plan: &ResetPlan, will_delete: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("Season reset plan for {}\n\n", plan.data_dir.display()));

    out.push_str(&format!("WILL BE DESTROYED ({}):\n", plan.to_delete.len()));
    if plan.to_delete.is_empty() {
        out.push_str("    (nothing - no world-scoped entry is present)\n");
    }
    for entry in &plan.to_delete {
        let marker = if entry.kind == StoreKind::Dir { "/" } else { "" };
        out.push_str(&format!("    {}{}\n        {}\n", entry.name, marker, entry.why));
    }

    out.push_str(&format!("\nWILL BE KEPT ({}):\n", plan.to_keep.len()));
    if plan.to_keep.is_empty() {
        out.push_str("    (nothing)\n");
    }
    for entry in &plan.to_keep {
        let marker = if entry.kind == StoreKind::Dir { "/" } else { "" };
        out.push_str(&format!("    {}{} [{:?}]\n        {}\n", entry.name, marker, entry.scope, entry.why));
    }

    if will_delete {
        out.push_str("\nDELETING NOW.\n");
    } else {
        out.push_str(&format!("\nDRY RUN - nothing has been deleted.\nTo actually destroy the entries above, run again with --confirm={CONFIRM_TOKEN}\n"));
    }
    out
}

/// Destroys exactly the world-scoped entries in `plan`.
///
/// Takes a plan rather than a directory so it cannot be called without
/// one having been computed, printed, and confirmed.
pub fn execute(plan: &ResetPlan) -> std::io::Result<()> {
    for entry in &plan.to_delete {
        let path = plan.data_dir.join(&entry.name);
        match entry.kind {
            StoreKind::Dir => std::fs::remove_dir_all(&path)?,
            StoreKind::File => std::fs::remove_file(&path)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding one of everything: a world file, a
    /// world directory, an account file, a config file and a not-a-store
    /// directory.
    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pod-reset-{}-{}-{label}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        std::fs::write(dir.join("adventure-characters.json"), "{}").unwrap();
        std::fs::write(dir.join("adventure-starter-kit-backfill-marker.json"), "true").unwrap();
        std::fs::create_dir_all(dir.join("adventure-fights-detail")).unwrap();
        std::fs::write(dir.join("adventure-fights-detail").join("fight-0000000001.json"), "{}").unwrap();
        std::fs::write(dir.join("adventure-accounts.json"), "[]").unwrap();
        std::fs::write(dir.join("adventure-live-tunables.toml"), "").unwrap();
        std::fs::create_dir_all(dir.join("wiki")).unwrap();
        std::fs::write(dir.join("wiki").join("bosses.md"), "# bosses").unwrap();
        dir
    }

    // ---- the refusal arm, tested as hard as the delete arm ---------

    /// THE fail-closed property. A single unclassified file stops the
    /// whole reset, and stops it BEFORE anything is deleted.
    #[test]
    fn an_undeclared_file_refuses_the_whole_reset() {
        let dir = scratch("undeclared");
        std::fs::write(dir.join("adventure-something-new.json"), "{}").unwrap();

        let refusal = plan_reset(&dir).expect_err("an unclassified entry must refuse - this is the entire safety property");
        match &refusal {
            ResetRefusal::Undeclared(names) => assert_eq!(names, &vec!["adventure-something-new.json".to_string()], "the refusal must name what it found"),
            other => panic!("expected Undeclared, got {other:?}"),
        }
        assert!(refusal.to_string().contains("adventure-something-new.json"), "the operator-facing message must name the offending entry, not just the count");

        // And nothing was touched on the way to refusing.
        assert!(dir.join("adventure-characters.json").exists(), "a refusal must delete NOTHING - not even the entries it was sure about");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The refusal is not "first one wins" - an operator fixing this
    /// needs the whole list or they fix one and hit the next.
    #[test]
    fn every_undeclared_entry_is_reported_not_just_the_first() {
        let dir = scratch("undeclared-many");
        std::fs::write(dir.join("adventure-aaa-unknown.json"), "{}").unwrap();
        std::fs::write(dir.join("adventure-zzz-unknown.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("some-new-directory")).unwrap();

        match plan_reset(&dir).expect_err("must refuse") {
            ResetRefusal::Undeclared(names) => {
                assert_eq!(names.len(), 3, "all three must be reported, got {names:?}");
                assert!(names.contains(&"adventure-aaa-unknown.json".to_string()));
                assert!(names.contains(&"adventure-zzz-unknown.json".to_string()));
                assert!(names.contains(&"some-new-directory".to_string()), "an unclassified DIRECTORY is as dangerous as an unclassified file");
            }
            other => panic!("expected Undeclared, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An unreadable directory refuses rather than reporting an empty
    /// plan. An empty plan would read as "nothing to do" and let a
    /// caller proceed against an unknown state.
    #[test]
    fn an_unreadable_directory_refuses() {
        let missing = std::env::temp_dir().join("pod-reset-definitely-missing-4a7c2e");
        let _ = std::fs::remove_dir_all(&missing);
        match plan_reset(&missing) {
            Err(ResetRefusal::Unreadable(_)) => {}
            other => panic!("a missing directory must refuse, got {other:?}"),
        }
    }

    // ---- the plan itself ------------------------------------------

    #[test]
    fn the_plan_destroys_world_state_and_keeps_everything_else() {
        let dir = scratch("plan");
        let plan = plan_reset(&dir).expect("a fully-declared directory must plan cleanly");

        let deleted: Vec<&str> = plan.to_delete.iter().map(|e| e.name.as_str()).collect();
        let kept: Vec<&str> = plan.to_keep.iter().map(|e| e.name.as_str()).collect();

        assert!(deleted.contains(&"adventure-characters.json"), "the roster is what a reset is for");
        assert!(deleted.contains(&"adventure-starter-kit-backfill-marker.json"), "markers are world-scoped - a fresh world must re-run its grants");
        assert!(deleted.contains(&"adventure-fights-detail"), "this world's fight history goes with the world");

        assert!(kept.contains(&"adventure-accounts.json"), "ACCOUNTS MUST SURVIVE - this is the defect the whole exercise started from");
        assert!(kept.contains(&"adventure-live-tunables.toml"), "operator configuration is not player state");
        assert!(kept.contains(&"wiki"), "the owner edits wiki content by hand and no game backup covers it");

        assert!(plan.to_delete.iter().all(|e| e.scope == StoreScope::World), "nothing but world-scoped entries may be in the delete list");
        assert!(plan.to_keep.iter().all(|e| e.scope != StoreScope::World), "nothing world-scoped may be in the keep list");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A declared store that is simply absent must not appear anywhere
    /// in the plan - the one-directional ruling, asserted rather than
    /// assumed. The scratch directory holds 6 of the ~52 declared
    /// entries; the other ~46 are absent and must be silent.
    #[test]
    fn absent_declared_stores_are_not_in_the_plan_and_are_not_an_error() {
        let dir = scratch("absent");
        let plan = plan_reset(&dir).expect("absence must never be a refusal - most of the table is absent on a fresh world");
        let all: Vec<&str> = plan.to_delete.iter().chain(plan.to_keep.iter()).map(|e| e.name.as_str()).collect();

        assert!(!all.contains(&"adventure-rampage-state.json"), "declared but absent must not appear in the plan");
        assert!(!all.contains(&"adventure-world.json"), "declared but absent must not appear in the plan");
        assert_eq!(all.len(), 6, "the plan must describe exactly what is present, got {all:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- the printed classification -------------------------------

    /// The operator must see the classification BEFORE it acts, with a
    /// reason per entry, and must see what is being KEPT as well as
    /// destroyed.
    #[test]
    fn the_rendered_plan_shows_both_lists_with_reasons() {
        let dir = scratch("render");
        let plan = plan_reset(&dir).unwrap();
        let rendered = render_plan(&plan, false);

        assert!(rendered.contains("WILL BE DESTROYED"), "the destroy list must be labelled");
        assert!(rendered.contains("WILL BE KEPT"), "what SURVIVES is the half an operator most needs to be able to disagree with");
        assert!(rendered.contains("adventure-characters.json"));
        assert!(rendered.contains("adventure-accounts.json"));
        assert!(rendered.contains("the roster - every character in this world"), "each entry must carry its reason, not just its name");
        assert!(rendered.contains("adventure-fights-detail/"), "a directory must be visibly a directory - removing one is a much larger mistake than removing a file");
        assert!(rendered.contains("DRY RUN"), "a dry run must say so unmistakably");
        assert!(rendered.contains(CONFIRM_TOKEN), "and must say exactly what to type to proceed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_confirmed_run_does_not_claim_to_be_a_dry_run() {
        let dir = scratch("render-confirmed");
        let plan = plan_reset(&dir).unwrap();
        let rendered = render_plan(&plan, true);
        assert!(rendered.contains("DELETING NOW"), "a real run must say it is deleting");
        assert!(!rendered.contains("DRY RUN"), "and must not also claim to be a dry run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- the delete arm -------------------------------------------

    #[test]
    fn execute_destroys_exactly_the_world_scoped_entries() {
        let dir = scratch("execute");
        let plan = plan_reset(&dir).unwrap();
        execute(&plan).expect("execute must succeed on a clean plan");

        assert!(!dir.join("adventure-characters.json").exists(), "the roster must be gone");
        assert!(!dir.join("adventure-starter-kit-backfill-marker.json").exists(), "markers must be gone");
        assert!(!dir.join("adventure-fights-detail").exists(), "the fight directory must be gone, contents and all");

        assert!(dir.join("adventure-accounts.json").exists(), "ACCOUNTS MUST SURVIVE THE RESET - the defect this whole exercise exists to fix");
        assert!(dir.join("adventure-live-tunables.toml").exists(), "operator configuration must survive");
        assert!(dir.join("wiki").join("bosses.md").exists(), "wiki content must survive, contents and all");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Planning alone deletes nothing. Separating the two is what makes
    /// a dry run possible at all, so it is asserted rather than assumed
    /// from the call graph.
    #[test]
    fn planning_alone_deletes_nothing() {
        let dir = scratch("dry");
        let _plan = plan_reset(&dir).unwrap();
        assert!(dir.join("adventure-characters.json").exists(), "planning must not delete - the default path through this module is inspection");
        assert!(dir.join("adventure-fights-detail").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The confirmation token has no abbreviation and no default. If
    /// this ever becomes `--yes` or an empty string, the dry-run default
    /// stops protecting anything.
    #[test]
    fn the_confirmation_token_is_not_something_typed_by_accident() {
        assert_eq!(CONFIRM_TOKEN, "DELETE-WORLD");
        assert!(CONFIRM_TOKEN.len() > 4, "a short token defeats the point of requiring one");
        assert!(!["y", "yes", "true", "1", "force", "f"].contains(&CONFIRM_TOKEN.to_ascii_lowercase().as_str()), "the token must not be a word an operator types reflexively");
    }
}
