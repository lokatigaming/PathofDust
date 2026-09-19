// A bare `reqwest::Client::new()` has NO request timeout. A stalled
// response therefore hangs forever, and on 2026-08-13 one stuck
// poe.ninja request froze every chat command for every user until the
// process was restarted. Item 22 gave all eighteen production clients
// in this crate an explicit timeout; this test is the part that keeps
// them, because the failure mode does not show up in any other test —
// a client with no timeout behaves identically until the day something
// stalls.
//
// Shape copied from `song_requests::tests::the_stall_reason_matches_the_overlay`,
// which likewise pins source text rather than behaviour. That one can
// use `include_str!` because it names one file; this one has to walk
// the tree, so it reads `src` at test time from CARGO_MANIFEST_DIR.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &str = "reqwest::Client::new()";

/// The one legitimate bare client: a test fixture in `commands.rs` that
/// never touches the network. Named here so the exemption is a fact in
/// the test rather than a property of the scanner.
const KNOWN_TEST_ONLY_SITE: &str = "commands.rs";

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Splits a source file into (production, tests). The split point is the
/// first `#[cfg(test)]` written at column 0; in this crate every such
/// attribute introduces a `mod` that runs to end of file, so everything
/// past the first one is test code.
///
/// That assumption is not taken on trust — `assert_tail_is_only_tests`
/// re-checks it on every file, and a guard that cannot tell a test
/// client from a production one fails loudly instead of passing.
fn split_at_first_test_mod(src: &str) -> (&str, &str) {
    let mut offset = 0usize;
    for line in src.split_inclusive('\n') {
        if line.trim_end() == "#[cfg(test)]" {
            return (&src[..offset], &src[offset..]);
        }
        offset += line.len();
    }
    (src, "")
}

/// Every column-0 line after the split point must belong to a
/// `#[cfg(test)] mod ... { }`: an attribute, the `mod` line it
/// introduces, a closing brace, a comment, or blank. A `pub fn`, `impl`,
/// `const` or a `mod` without `#[cfg(test)]` above it means production
/// code lives in the region this test exempts, and the exemption is no
/// longer sound.
fn assert_tail_is_only_tests(path: &Path, tail: &str) {
    let mut previous_item_line = "";
    for (i, line) in tail.lines().enumerate() {
        if line.is_empty() || line.starts_with([' ', '\t']) {
            continue;
        }
        let ok = line.starts_with("//")
            || line.starts_with('#')
            || line.starts_with('}')
            || (line.starts_with("mod ") && previous_item_line == "#[cfg(test)]");
        assert!(
            ok,
            "{}: line {} — `{line}` is top-level code after the first #[cfg(test)]. \
             The guard exempts everything past that point as test code, and that is \
             no longer true, so the exemption would hide a real bare client. Move the \
             test module to the end of the file, or teach this guard to brace-match.",
            path.display(),
            i + 1,
        );
        previous_item_line = line.trim_end();
    }
}

fn violations(path: &Path, src: &str) -> Vec<String> {
    let (production, tail) = split_at_first_test_mod(src);
    assert_tail_is_only_tests(path, tail);
    production
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(FORBIDDEN))
        .map(|(i, line)| format!("{}:{} {}", path.display(), i + 1, line.trim()))
        .collect()
}

#[test]
fn no_production_http_client_is_built_without_a_timeout() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src_root, &mut files);
    assert!(files.len() > 10, "expected to walk the whole bot source tree, found {} files", files.len());

    let mut found = Vec::new();
    let mut exempted = 0usize;
    for path in &files {
        let src = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        found.extend(violations(path, &src));
        let (_, tail) = split_at_first_test_mod(&src);
        exempted += tail.matches(FORBIDDEN).count();
        if path.file_name().is_some_and(|n| n == KNOWN_TEST_ONLY_SITE) {
            assert!(
                tail.contains(FORBIDDEN),
                "{KNOWN_TEST_ONLY_SITE}'s test-only bare client has moved or gone. If it was \
                 removed, drop this assertion; if it moved out of the test module, it is now \
                 a real finding."
            );
        }
    }

    assert_eq!(
        exempted, 1,
        "exactly one bare client should be exempted (the {KNOWN_TEST_ONLY_SITE} test fixture); \
         found {exempted}. A new one inside a test module is fine — update this count. \
         The point of pinning it is that the exemption stays a short, read list."
    );

    assert!(
        found.is_empty(),
        "these clients have no request timeout, so a stalled response hangs them forever:\n  {}\n\
         Build them with `reqwest::Client::builder().timeout(Duration::from_secs(N)).build()` \
         instead — see the poe_ninja_http client in main.rs for the precedent and the comment \
         above it for what happened without one.",
        found.join("\n  "),
    );
}

/// The guard above passes when the tree is clean, which is also what it
/// would do if it were broken. These are the positive controls: the
/// scanner must find a bare client in production text, and must not
/// find the one in a test module.
#[test]
fn the_guard_can_actually_tell_the_two_apart() {
    let production = "pub fn a() {\n    let http = reqwest::Client::new();\n}\n";
    assert_eq!(violations(Path::new("fake.rs"), production).len(), 1, "a production client must be caught");

    let test_only = "pub fn a() {}\n\n#[cfg(test)]\nmod tests {\n    fn f() {\n        let http = reqwest::Client::new();\n    }\n}\n";
    assert!(violations(Path::new("fake.rs"), test_only).is_empty(), "a client inside #[cfg(test)] must not be caught");

    let with_timeout = "let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build();\n";
    assert!(violations(Path::new("fake.rs"), with_timeout).is_empty(), "a client WITH a timeout must not be caught");
}

/// And the tail check has to actually object to production code sitting
/// after the test module, or the exemption above is worthless.
#[test]
fn the_guard_rejects_production_code_after_the_test_module() {
    let sneaky = "#[cfg(test)]\nmod tests {\n}\n\npub fn later() {\n    let http = reqwest::Client::new();\n}\n";
    let result = std::panic::catch_unwind(|| violations(Path::new("fake.rs"), sneaky));
    assert!(result.is_err(), "top-level code after #[cfg(test)] must fail the guard, not be exempted");
}
