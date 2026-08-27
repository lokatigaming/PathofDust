# Platform portability audit — Windows → Ubuntu LTS

Read-only inventory of every Windows-specific assumption in this
repository, produced 2026-08-27 on branch `docs/platform-portability-audit`
against `origin/master`. **No behaviour was changed by this audit and
nothing here has been implemented.** Every item is file + line, what it
assumes, what breaks on Linux, and effort to fix.

Effort scale used throughout:

| Rating | Meaning |
|---|---|
| **Trivial** | One line, no design decision. |
| **Small** | A handful of lines in one or two files. |
| **Medium** | A named change across several files, needs a decision first. |
| **Large** | New structure/ops artefacts; a project stage of its own. |

---

## 0. Headline verdict

The **Rust code is close to portable already**. There is no `cfg(windows)`
block, no Windows-only crate, no `Command::new("powershell")`, no drive
letter, and no backslash path separator anywhere in either crate. The
atomic save path is *better* on Linux than on Windows, not worse.

Three things actually block the move, in descending order:

1. **The ops layer is 100% Windows** — 4 PowerShell scripts (~1,940 lines)
   and 4 registered scheduled tasks. All of it is replaced, none of it
   ports. (§12)
2. **Data layout: code directory == data directory, by construction.**
   Every persisted file resolves against the process CWD, and the CWD is
   the source tree. A mechanism to redirect *game* data exists
   (`GAME_DATA_DIR`) but covers roughly 80% of the game's files and 0% of
   the bot's. (§10)
3. **One live case-sensitivity hazard** in stored data (custom sprites),
   plus one config value that is a literal Windows path. (§8, §11)

Everything else is small or trivial.

---

## 1. Hardcoded absolute paths, drive letters, backslash separators

### Rust — clean

Swept both crates for `"<letter>:[\\/]"` and for backslashes in string
literals. **Zero hits in `src/` and `game/src/`.** Every backslash in Rust
source is an escaped quote inside a message string, or a regex class. The
only literal backslash used as a *path* concept is a rejection check,
which is correct on both platforms:

- [character.rs:842](../game/src/adventure/character.rs#L842) —
  `name.contains('/') || name.contains('\\') || name.contains("..")`
  rejects traversal in a custom-sprite name. Rejecting `\` on Linux is
  harmless (it is a legal filename byte there, and rejecting it is
  strictly safer). **No change needed.**

### Non-Rust — one real hit

| Location | Assumes | Breaks on Linux | Effort |
|---|---|---|---|
| `.env.example:17` — `PUBLIC_SITE_DIR='C:\Users\Administrator\Downloads\SIK Claude'` | A Windows user-profile path, with backslashes and an embedded space, as the directory the bot writes `commands-data.json` / `themes-data.json` into. | Path does not exist; both writes fail (logged, non-fatal). The published command list and theme list on the public site silently stop updating. | **Trivial** for the value itself — but see §10, this directory is *outside* the deploy root and needs a home in the new layout. |
| `docs/ops_backup_and_watchdog.md`, `REFACTOR_PLAN.md` §13 | `C:\PathofDust`, `C:\PathofDust2`, `C:\pod-backups\PathofDust`, `target\release\game.exe` throughout. | Documentation only — no runtime effect. Both are rewritten wholesale with the ops move. | **Medium** (doc rewrite, tracked under §12). |

---

## 2. Paths built by string concatenation rather than `PathBuf`/`Path::join`

All of these concatenate with a **forward slash**, valid on both platforms,
so none is a break. Listed for completeness because the order asked.

| Location | Form | Verdict |
|---|---|---|
| [wiki.rs:58](../game/src/adventure_web/wiki.rs#L58) | `format!("{WIKI_MD_DIR}/{name}.md")` | Safe. `name` is gated by `is_valid_wiki_slug` (ASCII lowercase, digits, hyphen only) — no separator is even expressible. |
| [wiki.rs:397](../game/src/adventure_web/wiki.rs#L397) | `format!("{WIKI_MD_DIR}/{page}.md")` | Same gate. Safe. |
| [fight_storage.rs:96](../game/src/adventure/fight_storage.rs#L96) | `Path::new(dir).join(format!("fight-{seq:010}.json"))` | Uses `join`. Safe. |
| [fight_storage.rs:293](../game/src/adventure/fight_storage.rs#L293) | `data_path(PINNED_FIGHTS_DIR).join(format!("{tier}-{}", …))` | Uses `join`. Safe. |
| [fight_storage.rs:365](../game/src/adventure/fight_storage.rs#L365) | `data_path(&format!("{LAST_FIGHTS_LOG_PATH}.bak"))` | Filename suffix, not a separator. Safe. |
| [character.rs:849](../game/src/adventure/character.rs#L849) | `dir.join(format!("{name}.png"))` | Uses `join`. Safe — but see §8; the *case* of `name` is the problem, not the separator. |
| [render.rs:38](../game/src/adventure_web/render.rs#L38) | `concat!(env!("CARGO_MANIFEST_DIR"), "/../templates")` — test-only | Safe. Cargo emits a native path; the appended `/../templates` resolves on both. |

**One conversion worth knowing about, not a break today:**

- [fight_storage.rs:34-36](../game/src/adventure/fight_storage.rs#L34-L36) —
  `fn resolved(name: &str) -> String { data_path(name).to_string_lossy().into_owned() }`.
  A `PathBuf` is flattened to a `String` and later re-parsed by `Path::new`.
  On Linux a path is an arbitrary byte string, not necessarily UTF-8, so
  `to_string_lossy` can corrupt a data directory whose name contains
  non-UTF-8 bytes. **Only reachable if `GAME_DATA_DIR` is set to such a
  path.** Keep the data directory ASCII and it never fires.
  **Effort to harden: Small** (thread `&Path` through the six plumbing fns
  instead of `&str`). Not required for the move.

---

## 3. Shelling out to PowerShell, cmd, or a Windows executable

**Zero occurrences in shipped code.** Swept for `process::Command`,
`Command::new`, `powershell`, `cmd.exe`, `taskkill`, `CREATE_NO_WINDOW`
across both crates. Exactly one hit, and it is a test:

- [killed_process_smoke.rs:35](../game/tests/killed_process_smoke.rs#L35) —
  `Command::new(env!("CARGO_BIN_EXE_game"))`. Cargo supplies the correct
  binary path per platform. The test *simulates* an abrupt kill; its own
  comment mentions `taskkill` only as narrative. **Portable as-is.**

One near-miss worth flagging:

- [auth.rs:74](../src/bin/auth.rs#L74) and
  [auth_patreon.rs:62](../src/bin/auth_patreon.rs#L62) —
  `open::that(url)` launches the system browser for the OAuth consent
  screen. The `open` crate handles Linux via `xdg-open`, so it compiles and
  runs, but on a headless VPS there is no browser and the call no-ops
  (`let _ =`). The operator copies the printed URL by hand.
  **Effort: Trivial** — the URL is already printed.
  Both binaries bind `127.0.0.1:3000` / `:3001` for the OAuth callback
  ([auth.rs:68](../src/bin/auth.rs#L68),
  [auth_patreon.rs:56](../src/bin/auth_patreon.rs#L56)), so re-authing on
  the VPS needs an SSH tunnel. **Effort: Small** (documented procedure, no
  code change).

---

## 4. Windows-only crates and `cfg` blocks

**None.** Swept both `Cargo.toml` files and both crates for `winapi`,
`windows-sys`, `std::os::windows`, `cfg(windows)`, `cfg(target_os = …)`,
`cfg(target_family = …)`.

- `Cargo.toml` — 31 dependencies, all cross-platform. The only ones with a
  native component are `reqwest`, `tokio-tungstenite` (`native-tls`),
  `zstd`, `flate2`, `sha2`. On Ubuntu these need `build-essential`,
  `pkg-config` and `libssl-dev` at build time.
- `game/Cargo.toml` — 26 dependencies plus one dev-dependency
  (`tokio-tungstenite`). The game crate does **not** pull `native-tls`
  outside dev-dependencies.
- `twitch-irc` is pinned to the `refreshing-token-rustls-webpki-roots`
  feature — pure-Rust TLS, no system OpenSSL, no CA-store dependency.
  **This is the good case:** the bot's Twitch connection needs nothing from
  the host.

**Effort: Trivial** — install `build-essential pkg-config libssl-dev` in
the build image. No source change.

---

## 5. `.exe` assumptions in process or binary names

**Zero in Rust.** The two occurrences in the repository are both inside
PowerShell comment text, describing what the script reads for a log line:

- `game-watchdog.ps1:48` — mentions `twitch-bot-rs.exe`
- `backup-game-data.ps1:47` — mentions `game.exe`

Both files are replaced entirely (§12). Cargo produces `game` and
`twitch-bot-rs` with no extension on Linux, and nothing in the Rust code
constructs or matches a binary name.

**Effort: none in code.** The deploy procedure (`REFACTOR_PLAN.md` §13
steps 5-6) references `target\release\game.exe` and is rewritten with the
ops layer.

---

## 6. File locking, replacement, and rename semantics

### The atomic save path: **strictly better on Linux**

[state.rs:146-186](../game/src/state.rs#L146-L186) — `write_atomic` does
temp-write → `sync_all` (fsync) → `rename`, with the temp file deliberately
placed in the *same directory* as the target so the rename stays within one
filesystem.

- **Steps 1-2 (temp + fsync)** — identical semantics on both platforms.
- **Step 3 (rename)** — `std::fs::rename` maps to `MoveFileEx`-with-replace
  on Windows and `rename(2)` on Linux. Both replace atomically. On Linux
  `rename(2)` is atomic unconditionally; on Windows it fails with
  `ACCESS_DENIED` while any other process holds the destination open
  without `FILE_SHARE_DELETE`.
- **The retry loop exists only because of that Windows limitation.**
  [state.rs:91-104](../game/src/state.rs#L91-L104) documents it explicitly:
  `ATOMIC_RENAME_ATTEMPTS = 5`, `ATOMIC_RENAME_RETRY = 20ms`, added because
  `backup-game-data.ps1`, `game-watchdog.ps1`, or a mod with a save open in
  an editor could each make a legitimate rename fail. **On Linux an open
  reader never blocks a rename** — the loop becomes dead code that costs
  nothing and never fires. Leave it; removing it is optional cleanup, not a
  port requirement.
- **The one thing that gets *worse*, and the source already flags it:**
  [state.rs:136-140](../game/src/state.rs#L136-L140) —

  > *"Not done: fsync of the containing DIRECTORY. On unix that is what
  > makes the rename itself durable across a power loss (the file's data
  > would survive, the directory entry might not); this codebase ships on
  > Windows, where NTFS journals the rename's metadata and there is no
  > directory handle to sync anyway. **Worth knowing before anyone ports
  > it.**"*

  This is the single most important line in the file for this migration.
  On ext4/xfs, after `rename()` returns the file's *data* is durable
  (because of the fsync) but the *directory entry* may not survive a power
  loss until the directory itself is fsynced. On a VPS this is a
  hypervisor/host-crash scenario rather than a wall-plug one, and ext4's
  default `data=ordered` keeps the window small, but the guarantee is
  genuinely weaker than NTFS's journalled rename.
  **Effort: Small** — open the parent directory and `sync_all` it after the
  rename, inside a `#[cfg(unix)]` block. ~8 lines in one function. The
  author already scoped the work; only the decision to do it is open.

### Non-atomic writers — three files bypass `write_atomic`

These use plain `std::fs::write` (truncate-then-stream) and are exposed to
the crash-truncation class the atomic path was built to fix. Not a
*portability* break — same on both platforms — but they are data files that
move in §10.

| File written | Code | Note |
|---|---|---|
| `adventure-live-tunables.toml` | [tunables.rs:610](../game/src/adventure/tunables.rs#L610) | `std::fs::write` |
| `adventure-passive-overrides.toml` | [passive_overrides.rs:193](../game/src/adventure/passive_overrides.rs#L193) | `std::fs::write` |
| *(read-only)* `adventure-item-balance.toml` | [balance.rs:60](../game/src/adventure/balance.rs#L60) | never written by the game |

**Stale documentation, worth correcting during the move:**
`backup-game-data.ps1:31-33` still says *"The game persists with
`std::fs::write` (game/src/state.rs), which truncates and then writes"* and
builds its whole verify-then-prune design on that premise. That is no
longer true for JSON state — `write_atomic` replaced it. The verification
is still worth keeping (it now catches different failures), but the stated
rationale is out of date and must not be carried verbatim into the Linux
replacement.

### Other locking assumptions

- **Production binaries are file-locked on Windows** (CLAUDE.md BRANCH
  DISCIPLINE: *"own `--target-dir` (production binaries are file-locked)"*,
  and §13's "rename the old binary aside — do not overwrite").
  **On Linux this constraint disappears.** A running ELF can be `rename`d
  or `unlink`ed freely; the kernel keeps the inode alive for the running
  process. Deploy becomes "write new binary, restart unit" with no
  rename-aside dance and no separate `--target-dir` needed to dodge a
  locked file. **This removes work rather than adding it.**
- No `flock`, `LockFile`, `.lock` file, or advisory-locking scheme exists
  anywhere. Concurrency between the two processes is managed by *not
  sharing writable files* (§10) — platform-independent, survives the move
  unchanged.

---

## 7. Windows environment variables

**None read anywhere.** Swept both crates for `USERPROFILE`, `APPDATA`,
`LOCALAPPDATA`, `PROGRAMDATA`, `HOMEDRIVE`, `HOMEPATH`, `TEMP`, `TMP`.

Every `std::env::var` call in shipped code is either a project-specific key
([config.rs:200](../src/config.rs#L200),
[game/src/main.rs:29](../game/src/main.rs#L29)) or `GAME_DATA_DIR`
([game/src/main.rs:88](../game/src/main.rs#L88)).

`std::env::temp_dir()` appears **only in tests** (13 call sites across
`game/tests/*` and `#[cfg(test)]` modules). It resolves `%TEMP%` on Windows
and `$TMPDIR` or `/tmp` on Linux — correct on both.

**Effort: none.**

---

## 8. Case sensitivity — HIGH PRIORITY

Method: (a) extracted every path-shaped string literal from all `.rs`,
`.html`, `.js` and `.json` sources and tested each for exact-case existence
on disk; (b) compared the 90-entry `ALL_SPRITES` table and the boss-sprite
table against the actual sprite directory byte-for-byte; (c) checked the
git index for any two tracked paths differing only in case; (d) checked
`wiki/nav.json` slugs against the wiki directory.

### Results — the static references are clean

| Check | Result |
|---|---|
| 87 distinct path literals in code | **All 87 exist with exact case.** Zero mismatches. |
| `ALL_SPRITES` (90 names) vs `public_adventure_overlay/sprites/*.png` | **All 90 match exactly.** Disk has 3 extra (`enemy-ogre-brute`, `enemy-orc-axeman`, `enemy-orc-warrior`) not in the table — unused, not a case issue. |
| Boss sprite names ([manager.rs:5842-5860](../game/src/adventure/manager.rs#L5842-L5860)) | All lowercase, all match `sprites/bosses/` exactly. |
| Two tracked paths differing only in case | **None.** |
| `templates/base.html`, `templates/characters.html` | Match [render.rs:59](../game/src/adventure_web/render.rs#L59) and [render.rs:148](../game/src/adventure_web/render.rs#L148) exactly. |
| `wiki/*.md` vs `nav.json` slugs | All lowercase-hyphenated, all match. |

### The one real hazard: custom sprites

[character.rs:840-850](../game/src/adventure/character.rs#L840-L850) —
`is_valid_custom_sprite`:

```rust
pub(crate) fn is_valid_custom_sprite(owner_id: &str, model: &str) -> bool {
    let Some(name) = model.strip_prefix("custom/") else { return false };
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") { return false; }
    if !custom_sprite_is_owned_by(owner_id, name) { return false; }   // <- lowercases
    let dir = std::path::Path::new(CUSTOM_SPRITE_DIR);
    dir.join(format!("{name}.png")).exists() || dir.join(format!("{name}.gif")).exists()   // <- does NOT
}
```

The **ownership check lowercases** (`custom_sprite_is_owned_by` →
`name.to_ascii_lowercase()`,
[character.rs:828](../game/src/adventure/character.rs#L828)) but the
**existence check does not**. On NTFS,
`Path::new("…/custom").join("Kibukah.png").exists()` returns `true` for the
on-disk `kibukah.png`. The validated, mixed-case string `custom/Kibukah` is
then **stored in the character's `model` field**, and the overlay later
requests `/sprites/custom/Kibukah.png`.

- **On Windows:** works. Always has.
- **On Linux:** the `.exists()` check fails, so *new* mixed-case
  submissions are correctly rejected — but any character whose `model` was
  **already stored** in mixed case renders a broken sprite (404) on the
  overlay and the dashboard, permanently and silently.

The normal path is safe: the picker
([adventure_web.rs:5266-5283](../game/src/adventure_web.rs#L5266-L5283))
lists directory entries and emits `path.file_stem()` verbatim, so it can
only ever produce the exact on-disk case. **Only a hand-crafted POST to
`/change-model` reaches the mixed-case state** — which the code's own
comment at
[character.rs:834-838](../game/src/adventure/character.rs#L834-L838)
confirms is a real, anticipated request shape.

**All 9 files currently in `public_adventure_overlay/sprites/custom/` are
lowercase**, so the *directory* is clean. The risk lives entirely in
`adventure-characters.json`, which is not in this repository and was not
inspected.

**Action required before cutover — ordered work, not done here:**

1. Grep the live `adventure-characters.json` for `"model": "custom/…"`
   values containing an uppercase letter. **Effort: Trivial** (one grep).
2. Fix the asymmetry so validation and storage agree on case.
   **Effort: Small** — lowercase `name` once at the top of
   `is_valid_custom_sprite`, or resolve it against a directory listing the
   way the picker already does. Touches one function.

### Secondary case notes (no action needed)

- `wiki/QUESTIONS_FOR_OWNER.md` and `wiki/WIKI_SYSTEM.md` sit in the wiki
  directory but contain uppercase, so `is_valid_wiki_slug`
  ([wiki.rs:410](../game/src/adventure_web/wiki.rs#L410),
  ASCII-lowercase-only) makes them unreachable via `/wiki/:page` on
  **both** platforms. Unchanged by the move. These are the wiki session's
  files — flagged, not touched.
- `wiki/nav.json` lists a `"passives"` slug with no `passives.md` on disk;
  that route is code-generated
  ([wiki.rs:356-358](../game/src/adventure_web/wiki.rs#L356-L358)), not a
  missing file. Not a case issue.
- `is_valid_custom_sprite`'s rejection of `\` (§1) means a Linux filename
  legitimately containing a backslash could never be selected. Cosmetic;
  do not "fix" it — the rejection is the safer default.

---

## 9. Line endings

### What `.gitattributes` enforces today

The entire file is one line:

```
*.rs text eol=lf
```

Verified with `git ls-files --eol`:

| Path | Index | Working tree | Attribute |
|---|---|---|---|
| `game/src/state.rs` | `lf` | **`lf`** | `text eol=lf` |
| `templates/base.html` | `lf` | `crlf` | *(none)* |
| `public_adventure_overlay/overlay.html` | `lf` | `crlf` | *(none)* |
| `wiki/nav.json` | `lf` | `crlf` | *(none)* |
| `.env.example` | `lf` | `crlf` | *(none)* |
| `game-watchdog.ps1` | `lf` | `crlf` | *(none)* |
| `Cargo.toml` | `lf` | `crlf` | *(none)* |

**The index is LF for everything.** The CRLF in the working tree comes from
this machine's global `core.autocrlf = true`, applied to every file
`.gitattributes` does not pin. A checkout on Ubuntu — where `core.autocrlf`
defaults to `false` — produces **LF everywhere**, which is what every
consumer wants.

**Conclusion: line endings are a non-issue for the move.** The repository
is already LF-normalised at rest; only this Windows worktree materialises
CRLF, and only for non-Rust files.

### Does any code parse in a way that depends on CRLF?

Swept for `\r`, `.lines()`, and newline-splitting in both crates.

| Location | What it does | CRLF dependency |
|---|---|---|
| [combat.rs:20420](../game/src/adventure/combat.rs#L20420) | `include_str!("combat.rs").replace("\r\n", "\n")` — a self-inspection test | **Defensive, already correct.** Normalises both ways. |
| [character.rs:3786](../game/src/adventure/character.rs#L3786) | `include_str!("character.rs").lines()` — visibility guard test | `str::lines()` strips a trailing `\r`. Correct on both. |
| [adventure_client.rs:177](../src/adventure_client.rs#L177) | `frame.lines()` over an SSE frame | Network data, not a file. Correct on both. |
| TOML config files | `toml` crate | Handles both. |
| Markdown wiki pages | `pulldown-cmark` | Handles both. |
| JSON state files | `serde_json` | Whitespace-insensitive. |

**Zero code paths depend on CRLF.** No change needed.

### One optional gap worth closing during the move

`.gitattributes` has no `* text=auto` line and marks nothing `binary`.
Git's own binary heuristic has protected the 117 PNG/GIF/MP3/MP4 assets so
far, and no corruption is visible in the tree. Adding `* text=auto` plus
explicit `-text` for asset extensions would make that protection
declarative rather than incidental. **Effort: Trivial.** Not required —
listed because the order asked what `.gitattributes` enforces, and the
honest answer is "almost nothing".

---

## 10. Data layout — HIGH PRIORITY

### The direct answer to the question asked

> *Say plainly whether any code assumes the data directory and the source
> directory are the same directory.*

**Yes. Emphatically, and in both processes.**

Every persisted file in both crates is a **bare relative filename**
resolved against the process's current working directory. The same CWD must
also contain `templates/`, `wiki/`, `public/`, `public_adventure_overlay/`,
`public_chat_overlay/`, `public_song_overlay/` and `.env` — all of which
are *source*, checked into git. There is no separation, and the source
comments say so directly:

- [render.rs:24-27](../game/src/adventure_web/render.rs#L24-L27) —
  *"Bare relative literal in production - resolves against the real
  process's own CWD, **which is always the repo/workspace root at deploy
  time**, exactly as it always has been."*
- [paths.rs:43-47](../game/src/adventure/paths.rs#L43-L47) — `data_path`
  defaults to an **empty** base path, so `data_path("x")` is
  *"byte-for-byte identical to the bare literal `"x"` it replaces -
  today's exact CWD-relative resolution, unchanged."*

One partial escape hatch exists and is already wired: `GAME_DATA_DIR` →
`set_data_dir` ([game/src/main.rs:88-92](../game/src/main.rs#L88-L92) →
[paths.rs:38](../game/src/adventure/paths.rs#L38)). It redirects everything
that goes through `data_path` — **but not everything the game writes**, and
nothing the bot writes. The gaps are itemised below and are the crux of the
split-directory work.

### 10.1 Complete inventory — GAME process (`game`)

**Group A — routed through `data_path`, so `GAME_DATA_DIR` already moves
them.** Resolution: CWD-relative by default, **configurable** via
`GAME_DATA_DIR`.

| File / directory | Resolved at | Constant declared at |
|---|---|---|
| `adventure-characters.json` | [game/src/main.rs:122](../game/src/main.rs#L122) → [manager.rs:1797](../game/src/adventure/manager.rs#L1797) | *(caller-supplied)* |
| `adventure-world.json` | [game/src/main.rs:123](../game/src/main.rs#L123) → [manager.rs:1798](../game/src/adventure/manager.rs#L1798) | *(caller-supplied)* |
| `adventure-reforge-cooldown.json` | [game/src/main.rs:124](../game/src/main.rs#L124) → [manager.rs:1799](../game/src/adventure/manager.rs#L1799) | *(caller-supplied)* |
| `adventure-rampage-state.json` | [manager.rs:2111](../game/src/adventure/manager.rs#L2111) read, [manager.rs:4572](../game/src/adventure/manager.rs#L4572) write | [manager.rs:206](../game/src/adventure/manager.rs#L206) |
| `adventure-live-tunables.toml` | [tunables.rs:593](../game/src/adventure/tunables.rs#L593) read, [tunables.rs:610](../game/src/adventure/tunables.rs#L610) write | [tunables.rs:572](../game/src/adventure/tunables.rs#L572) |
| `adventure-item-balance.toml` | [balance.rs:60](../game/src/adventure/balance.rs#L60) read only | [balance.rs:54](../game/src/adventure/balance.rs#L54) |
| `adventure-passive-overrides.toml` | [passive_overrides.rs:148](../game/src/adventure/passive_overrides.rs#L148) read, [:193](../game/src/adventure/passive_overrides.rs#L193) write | [passive_overrides.rs:45](../game/src/adventure/passive_overrides.rs#L45) |
| `adventure-fights-coarse/` (dir) | [fight_storage.rs:36](../game/src/adventure/fight_storage.rs#L36) via `resolved()` | [fight_storage.rs:39](../game/src/adventure/fight_storage.rs#L39) |
| `adventure-fights-detail/` (dir) | same | [fight_storage.rs:40](../game/src/adventure/fight_storage.rs#L40) |
| `adventure-fights-summary/` (dir) | same | [fight_storage.rs:41](../game/src/adventure/fight_storage.rs#L41) |
| `adventure-fights-bundle/` (dir) | same | [fight_storage.rs:45](../game/src/adventure/fight_storage.rs#L45) |
| `adventure-fights-pinned/` (dir) | [fight_storage.rs:293](../game/src/adventure/fight_storage.rs#L293), [:308](../game/src/adventure/fight_storage.rs#L308), [:332](../game/src/adventure/fight_storage.rs#L332) | [fight_storage.rs:261](../game/src/adventure/fight_storage.rs#L261) |
| `adventure-fights-coarse-seq.json` | via `resolved()` | [fight_storage.rs:42](../game/src/adventure/fight_storage.rs#L42) |
| `adventure-fights-detail-seq.json` | via `resolved()` | [fight_storage.rs:43](../game/src/adventure/fight_storage.rs#L43) |
| `adventure-fights-summary-seq.json` | via `resolved()` | [fight_storage.rs:44](../game/src/adventure/fight_storage.rs#L44) |
| `adventure-fights-bundle-seq.json` | via `resolved()` | [fight_storage.rs:46](../game/src/adventure/fight_storage.rs#L46) |
| `adventure-fights-storage-migration-marker.json` | [fight_storage.rs:355](../game/src/adventure/fight_storage.rs#L355), [:370](../game/src/adventure/fight_storage.rs#L370) | [fight_storage.rs:339](../game/src/adventure/fight_storage.rs#L339) |
| `adventure-last-fights.json` (legacy, pre-split) | [fight_storage.rs:358](../game/src/adventure/fight_storage.rs#L358), [:364](../game/src/adventure/fight_storage.rs#L364) | [manager.rs:1508](../game/src/adventure/manager.rs#L1508) |
| `adventure-last-fights.json.bak` | [fight_storage.rs:365](../game/src/adventure/fight_storage.rs#L365) | *(derived)* |
| `adventure-sprite-count.json` | [manager.rs:2066](../game/src/adventure/manager.rs#L2066), [:2091](../game/src/adventure/manager.rs#L2091) | [manager.rs:2065](../game/src/adventure/manager.rs#L2065) |

**Group A, continued — one-time migration / giveaway markers, all via
`data_path`:**

| Marker file | Declared at |
|---|---|
| `adventure-crit-reforge-equipped-backfill-marker.json` | [manager.rs:1850](../game/src/adventure/manager.rs#L1850) |
| `adventure-craft-token-backfill-marker.json` | [manager.rs:1894](../game/src/adventure/manager.rs#L1894) |
| `adventure-craft-token-backfill-v2-marker.json` | [manager.rs:1919](../game/src/adventure/manager.rs#L1919) |
| `adventure-pity-launch-marker.json` | [manager.rs:1944](../game/src/adventure/manager.rs#L1944) |
| `adventure-wings-launch-grant-marker.json` | [manager.rs:1964](../game/src/adventure/manager.rs#L1964) |
| `adventure-passive-key-rename-marker.json` | [manager.rs:1995](../game/src/adventure/manager.rs#L1995) |
| `adventure-kibukah-compensation-marker.json` | [manager.rs:2030](../game/src/adventure/manager.rs#L2030) |
| `adventure-celestial-shard-first-award-marker.json` | [manager.rs:2285](../game/src/adventure/manager.rs#L2285) |
| `adventure-unique-shard-first-award-marker.json` | [manager.rs:2303](../game/src/adventure/manager.rs#L2303) (`ITEM_LAUNCH_GIVEAWAYS`) |
| `adventure-helm-rebalance-v2-marker.json` | [migrations.rs:212](../game/src/adventure/migrations.rs#L212) (`ITEM_MIGRATIONS`) |
| `adventure-power-roll-backfill-marker.json` | [migrations.rs:213](../game/src/adventure/migrations.rs#L213) |
| `adventure-krangle-accuracy-marker.json` | [migrations.rs:214](../game/src/adventure/migrations.rs#L214) |
| `adventure-item-accuracy-marker.json` | [migrations.rs:215](../game/src/adventure/migrations.rs#L215) |
| `adventure-crit-value-nerf-marker.json` | [migrations.rs:216](../game/src/adventure/migrations.rs#L216) |
| `adventure-gloves-speed-rebalance-marker.json` | [migrations.rs:217](../game/src/adventure/migrations.rs#L217) |
| `adventure-crit-lineage-backfill-marker.json` | [migrations.rs:218](../game/src/adventure/migrations.rs#L218) |
| `adventure-crit-flag-to-affix-tracking-marker.json` | [migrations.rs:219](../game/src/adventure/migrations.rs#L219) |
| `adventure-flowlikewater-swap-marker.json` | [migrations.rs:401](../game/src/adventure/migrations.rs#L401) (`CHARACTER_MIGRATIONS`) |
| `adventure-celestial-shard-into-unique-shard-marker.json` | [migrations.rs:402](../game/src/adventure/migrations.rs#L402) |
| `adventure-duplicate-unique-effects-cleanup-marker.json` | [migrations.rs:403](../game/src/adventure/migrations.rs#L403) |
| `adventure-lingering-effect-to-echo-marker.json` | [migrations.rs:404](../game/src/adventure/migrations.rs#L404) |

Markers are read at
[migrations.rs:236](../game/src/adventure/migrations.rs#L236) /
[:415](../game/src/adventure/migrations.rs#L415) and written at
[:245](../game/src/adventure/migrations.rs#L245) /
[:424](../game/src/adventure/migrations.rs#L424).

**Group B — game files that do NOT go through `data_path`. `GAME_DATA_DIR`
does not move these. This is the gap.**

| File / directory | Code location | Resolution | Why it is not routed |
|---|---|---|---|
| `adventure-sessions.json` | [game/src/main.rs:165](../game/src/main.rs#L165) → [adventure_web.rs:141](../game/src/adventure_web.rs#L141) load, [adventure_web.rs:92](../game/src/adventure_web.rs#L92) save | **Hardcoded, CWD-relative** | Passed as a `PathBuf` straight into the web server, never touched by `AdventureManager`. |
| `adventure-wings-giveaway-marker.json` | [game/src/main.rs:145-152](../game/src/main.rs#L145-L152) | **Hardcoded, CWD-relative** | Uses `game::state::load_json`/`save_json` directly from `main`, bypassing `data_path`. |
| `patch-notes.json` | [adventure_web.rs:1859](../game/src/adventure_web.rs#L1859) | **Hardcoded, CWD-relative** | Read fresh per request; [:1854](../game/src/adventure_web.rs#L1854) says this is deliberate for live editing. |
| `bot-published-constants.json` | write [api.rs:394](../game/src/adventure_web/api.rs#L394); read [game/src/main.rs:114](../game/src/main.rs#L114) and `wiki.rs` | **Hardcoded, CWD-relative** | Deliberately excluded — [published_constants.rs:24](../game/src/adventure/published_constants.rs#L24) states it is bot-domain data, not game data. |
| `public_adventure_overlay/sprites/custom/` | [character.rs:792](../game/src/adventure/character.rs#L792); listed at [adventure_web.rs:5267](../game/src/adventure_web.rs#L5267); probed at [character.rs:848-849](../game/src/adventure/character.rs#L848-L849) | **Hardcoded, CWD-relative, inside the source tree** | A drop-in folder for operator-supplied PNG/GIFs. **Mutable data living in a git-tracked source directory.** |
| `logs/` + `logs/game.log.YYYY-MM-DD` | [game/src/main.rs:69-70](../game/src/main.rs#L69-L70) | **Hardcoded, CWD-relative** | `tracing_appender::rolling::daily("logs", "game.log")`. |

**Group C — read-only source assets the game process needs in its CWD:**

| Directory | Code location | Notes |
|---|---|---|
| `templates/` | [render.rs:36](../game/src/adventure_web/render.rs#L36); watched at [render.rs:55](../game/src/adventure_web/render.rs#L55) | Live-reloading via `minijinja-autoreload`; edits take effect with no restart. |
| `wiki/` (`*.md` + `nav.json`) | [wiki.rs:30](../game/src/adventure_web/wiki.rs#L30), [:58](../game/src/adventure_web/wiki.rs#L58), [:397](../game/src/adventure_web/wiki.rs#L397) | Live-reloading via `WIKI_MD_CACHE`. Owner edits these directly. |
| `public_adventure_overlay/` | [game/src/main.rs:157](../game/src/main.rs#L157) | `ServeDir` mount + `sprites/`. |

### 10.2 Complete inventory — BOT process (`twitch-bot-rs`)

**None of these are routed through `data_path`. All are hardcoded,
CWD-relative, with no configuration hook whatsoever.**

| File | Code location | Contains secrets? |
|---|---|---|
| `tokens.json` | [src/main.rs:539](../src/main.rs#L539); written by [auth.rs:149](../src/bin/auth.rs#L149) | **Yes** — Twitch OAuth refresh token |
| `patreon-tokens.json` | [src/main.rs:599](../src/main.rs#L599); written by [auth_patreon.rs:131](../src/bin/auth_patreon.rs#L131) | **Yes** — Patreon OAuth tokens |
| `commands.json` | [src/main.rs:552](../src/main.rs#L552) | No |
| `bugreports.json` | [src/main.rs:553](../src/main.rs#L553) | No |
| `song-queue.json` | [src/main.rs:564](../src/main.rs#L564) | No |
| `search-cache.json` | [src/main.rs:565](../src/main.rs#L565) | No |
| `patreon-seen.json` | [src/main.rs:600](../src/main.rs#L600) | No |
| `tips-history.json` | [src/main.rs:633](../src/main.rs#L633) | No |
| `paypal-tips-history.json` | [src/main.rs:676](../src/main.rs#L676) | No |
| `entrance-themes.json` | [src/main.rs:706](../src/main.rs#L706) | No |
| `daily-greeted.json` | [src/main.rs:707](../src/main.rs#L707) | No |
| `channel-points-theme-reward.json` | [src/main.rs:768](../src/main.rs#L768) | No |
| `channel-points-interrupt-reward.json` | [src/main.rs:784](../src/main.rs#L784) | No |
| `channel-points-reforge-reward.json` | [src/main.rs:796](../src/main.rs#L796) | No |
| `channel-points-repair-reward.json` | [src/main.rs:802](../src/main.rs#L802) | No |
| `channel-points-force-boss-reward.json` | [src/main.rs:811](../src/main.rs#L811) | No |
| `personal-playlists.json` | [src/main.rs:974](../src/main.rs#L974) | No |
| `playrandom-state.json` | [playrandom.rs:274](../src/playrandom.rs#L274) | No |
| `logs/` + `logs/bot.log.YYYY-MM-DD` | [src/main.rs:520-521](../src/main.rs#L520-L521) | No |
| `.env` | [config.rs:227](../src/config.rs#L227) via `dotenvy::dotenv()` | **Yes** — every API key |

**Bot read-only source assets in CWD:**

| Directory | Code location |
|---|---|
| `public/` (`alert-box.html`, media) | [src/main.rs:550](../src/main.rs#L550) → [alerts.rs:57](../src/alerts.rs#L57), [alerts.rs:73](../src/alerts.rs#L73) |
| `public_song_overlay/` (`overlay.html`, `dock.html`) | [src/main.rs:569](../src/main.rs#L569) → [song_overlay_server.rs:46](../src/song_overlay_server.rs#L46), [:62](../src/song_overlay_server.rs#L62), [:66](../src/song_overlay_server.rs#L66) |
| `public_chat_overlay/` (`overlay.html`) | [src/main.rs:582](../src/main.rs#L582) → [chat_overlay_server.rs:94](../src/chat_overlay_server.rs#L94), [:109](../src/chat_overlay_server.rs#L109) |

**Bot writes OUTSIDE the deploy root — the only configurable data path in
either process:**

| File | Code location | Resolution |
|---|---|---|
| `<PUBLIC_SITE_DIR>/commands-data.json` | [commands.rs:228](../src/commands.rs#L228); dir from [config.rs:254](../src/config.rs#L254) | **Configurable** via `PUBLIC_SITE_DIR`; currently a Windows user-profile path (§1). |
| `<PUBLIC_SITE_DIR>/themes-data.json` | [entrance_themes.rs:223](../src/entrance_themes.rs#L223) | Same. |

### 10.3 Summary by resolution class

| Class | Count | Notes |
|---|---|---|
| **Configurable** (`GAME_DATA_DIR`) | ~40 game files + 5 fight directories | Mechanism exists, is wired into `game`'s `main`, and is currently unset in production. |
| **Configurable** (`PUBLIC_SITE_DIR`) | 2 bot files | The only bot-side hook. |
| **Hardcoded, CWD-relative, no hook** | 6 game files/dirs + 20 bot files + 2 log dirs | The actual porting surface. |
| **Read-only source in CWD** | 6 directories | `templates/`, `wiki/`, `public/`, `public_adventure_overlay/`, `public_chat_overlay/`, `public_song_overlay/` |

### 10.4 The current deployment root, as evidenced by `.gitignore`

This audit did not inspect `C:\PathofDust` (out of scope for this session).
`.gitignore` is a faithful proxy and confirms the order's description:

- **44** `/backup-pre-*` directory entries — deploy-time snapshots, sitting
  beside `Cargo.toml`.
- **3** `/target*` entries — `/target`, `/target-bake`,
  `/target-elementalist` — build output in the same directory as source and
  data.
- **~37** runtime data patterns (`adventure-*.json`,
  `adventure-*-marker.json`, `adventure-fights-*/`, `tokens.json`, `logs/`,
  `crashdumps/`, …).

The deploy root is therefore, simultaneously: a git working tree, the data
directory, the log directory, the build output directory, and a backup
archive. Every one of those is a different lifecycle.

### 10.5 What a three-directory split would cost

Target shape — **not implemented; this is the estimate the order asked
for**:

```
/opt/pathofdust/        code + read-only assets (templates, wiki, public*)
/var/lib/pathofdust/    mutable state (JSON, TOML, fight archives, custom sprites)
/var/log/pathofdust/    logs
/etc/pathofdust/        .env / EnvironmentFile (see §11)
```

**Step 1 — game data. Zero code change.**
`GAME_DATA_DIR=/var/lib/pathofdust` already moves Group A: every character
save, world state, reforge cooldown, rampage state, all three TOMLs, all
five fight directories, all four sequence counters, and all 21 migration
markers. **Effort: Trivial.** The mechanism is built, documented
(`game/src/adventure/paths.rs`), and functionally verified — see that
file's closing comment. It is simply not set in production today.

**Step 2 — close the Group B gap. 4 files, ~8 lines.**

| Change | File | Size |
|---|---|---|
| Route `adventure-sessions.json` through `data_path` | [game/src/main.rs:165](../game/src/main.rs#L165) | 1 line |
| Route the wings-giveaway marker through `data_path` | [game/src/main.rs:145-152](../game/src/main.rs#L145-L152) | 2 lines |
| Route `patch-notes.json` through `data_path` | [adventure_web.rs:1859](../game/src/adventure_web.rs#L1859) | 1 line |
| Decide `bot-published-constants.json`'s home | [api.rs:394](../game/src/adventure_web/api.rs#L394) + [game/src/main.rs:114](../game/src/main.rs#L114) + the `wiki.rs` read | 2-3 lines **+ a decision** |

The last one is the only one that is not mechanical: the file is written by
the *game* (via the API seam) but is conceptually *bot* data, and
[published_constants.rs:24](../game/src/adventure/published_constants.rs#L24)
deliberately keeps it out of `data_path`. With the processes potentially in
different working directories under systemd, "the bot's CWD" and "the
game's CWD" stop being the same place, so this needs an explicit answer.
**Effort: Small, gated on one decision.**

**Step 3 — custom sprites. 1 constant + a decision.**
[character.rs:792](../game/src/adventure/character.rs#L792) points at
`public_adventure_overlay/sprites/custom` — mutable, operator-supplied data
inside a git-tracked asset directory. Under a read-only `/opt` it has to
move to `/var/lib/pathofdust/custom-sprites`, which means the `ServeDir`
mount at [game/src/main.rs:157](../game/src/main.rs#L157) needs a second
mount or a symlink so `/sprites/custom/...` still resolves. A symlink is
the zero-code option. **Effort: Small** (symlink) or **Medium** (second
`ServeDir` mount + the constant + the listing at
[adventure_web.rs:5267](../game/src/adventure_web.rs#L5267)).

**Step 4 — bot data. This is the real work.**
The bot has **no `data_path` equivalent at all**. Twenty hardcoded
`PathBuf::from("…")` literals in `src/main.rs` plus one in
[playrandom.rs:274](../src/playrandom.rs#L274). Two options:

- **(a) No code change; set the systemd unit's `WorkingDirectory` to
  `/var/lib/pathofdust`.** Every bot data file lands there. But the bot
  also `ServeDir`s `public/`, `public_song_overlay/` and
  `public_chat_overlay/` from its CWD, so those three asset directories
  would need symlinking into the data directory — or `/var/lib` would have
  to contain code. **Effort: Trivial in code, slightly ugly in ops.**
- **(b) Mirror `paths.rs` into the bot crate** and wrap all 21 literals.
  Touches `src/main.rs` (20 sites), `src/playrandom.rs` (1 site), plus a
  new `src/paths.rs`. The precedent is exact and already written — copy
  `game/src/adventure/paths.rs`. **Effort: Medium** — mechanical, but 21
  call sites and a new env var to document.

**(a) is the recommendation if the goal is to ship the migration; (b) if
the goal is a clean layout.** They are not exclusive — (a) now, (b) later.

**Step 5 — logs. 2 lines, or zero.**
[src/main.rs:520-521](../src/main.rs#L520-L521) and
[game/src/main.rs:69-70](../game/src/main.rs#L69-L70) both hardcode
`"logs"`. Either change the literal to `/var/log/pathofdust` (2 lines, one
per process), or **drop the file appender entirely** and let both processes
log to stdout, which systemd captures into the journal automatically. The
second is smaller, more idiomatic on Linux, and deletes the "logs/ grew to
several GB" problem recorded at
[src/main.rs:518](../src/main.rs#L518) — journald rotates by default.
**Effort: Trivial either way.** Note the `_log_guard` lifetime dance in
both files exists only for the file appender; dropping it simplifies both
`main`s.

**Step 6 — `.env`. See §11.**

**Total: 4 code files touched for the game (~8 lines), 2 for logs, 0-22 for
the bot depending on the option chosen, plus one symlink-or-mount decision
for custom sprites.** The genuinely large part of the migration is §12, not
this.

**Files touched, complete list:** `game/src/main.rs`,
`game/src/adventure_web.rs`, `game/src/adventure_web/api.rs`,
`game/src/adventure/character.rs`, `src/main.rs`, `src/playrandom.rs`, and
(option b only) a new `src/paths.rs`.

---

## 11. Configuration and secrets

**No secret values are reproduced in this document.**

### How secrets reach each process today

Both processes use the **same mechanism and the same file**:
`dotenvy::dotenv()`, which loads a `.env` found by walking up from the
process's current working directory.

| Process | Call site |
|---|---|
| `twitch-bot-rs` | [config.rs:227](../src/config.rs#L227), inside `Config::load()` |
| `game` | [game/src/main.rs:83](../game/src/main.rs#L83) |
| `auth` (helper binary) | [auth.rs:41](../src/bin/auth.rs#L41) |
| `auth_patreon` (helper binary) | [auth_patreon.rs:34](../src/bin/auth_patreon.rs#L34) |

**Nothing is passed as a scheduled-task argument.** The `GameProcess` and
`TwitchBotRS` task actions carry no script arguments at all — confirmed by
`docs/ops_backup_and_watchdog.md:429-432` and `:458-461`, where both
watchdogs were deliberately given defaults matching the live tasks
*because* the tasks pass nothing. The task's **working directory** is what
makes `.env` resolve: an implicit dependency that becomes explicit under
systemd.

`.env` itself is gitignored (`.gitignore:2`). `.env.example` documents ~40
keys across Twitch, Patreon, PayPal, YouTube, Last.fm, StreamElements and
OBS.

Two files also hold long-lived credentials on disk and are **not** `.env`:
`tokens.json` (Twitch refresh token,
[src/main.rs:539](../src/main.rs#L539)) and `patreon-tokens.json`
([src/main.rs:599](../src/main.rs#L599)). Both are gitignored. Both move
with the bot's data directory (§10 step 4) and both need `chmod 600` under
a dedicated service user.

### What changes on Linux

| Item | Change | Effort |
|---|---|---|
| `.env` discovery | `dotenvy` walks *up* from CWD. With `WorkingDirectory=/var/lib/pathofdust`, a stray `.env` in `/var/lib` or `/` would be picked up. Prefer systemd's `EnvironmentFile=/etc/pathofdust/env` (mode 0600, owned by the service user) — the processes read plain env vars identically and `dotenv()` becomes a harmless no-op. **No code change required.** | **Trivial** |
| `PUBLIC_SITE_DIR` | Currently a Windows profile path (§1). Needs a real Linux target or must be left unset — the two writes are `Option`-gated at [config.rs:254](../src/config.rs#L254) and no-op when absent ([commands.rs:216](../src/commands.rs#L216)). | **Trivial** |
| `tokens.json` / `patreon-tokens.json` | Re-authing on a headless box needs an SSH tunnel to `127.0.0.1:3000`/`:3001` (§3), or copying the files across. | **Small** (procedure) |
| File permissions | Windows ACLs → `chmod 600` + a dedicated non-root service user. Nothing in the code reads or sets permissions. | **Small** |
| Localhost URLs | `ADVENTURE_API_BASE_URL` defaults to `http://127.0.0.1:4005` ([config.rs:279](../src/config.rs#L279)) — correct if both units stay on one host. `OBS_WEBSOCKET_URL` defaults to `ws://127.0.0.1:4455` ([config.rs:288](../src/config.rs#L288)) — **this will not resolve.** OBS runs on the streamer's Windows machine, not the VPS, so the bot's OBS integration (`src/obs_websocket.rs`) stops working unless the VPS can reach it. A *topology* consequence of the move, not a code defect. **Flagged as a scoping question for the owner, not a code fix.** | **Decision needed** |
| Port binding | All servers bind `0.0.0.0` on ports 4001-4005 ([alerts.rs:60](../src/alerts.rs#L60), [song_overlay_server.rs:49](../src/song_overlay_server.rs#L49), [chat_overlay_server.rs:97](../src/chat_overlay_server.rs#L97), [adventure_overlay_server.rs:52](../game/src/adventure_overlay_server.rs#L52), [adventure_web.rs:229](../game/src/adventure_web.rs#L229)). All are >1024, so **no privileged-port capability is needed**. On a public VPS these become internet-reachable — a firewall and/or reverse proxy is now required where the LAN previously provided isolation. | **Medium** (ops, not code) |

### The TOML config files and their reload semantics

The order says "both TOML config files". **There are three.** All live in
the game's data directory, all are `data_path`-routed, and all fail soft —
a parse error logs a warning and falls back to built-in defaults, never a
boot failure.

| File | Read | Written | Reload semantics |
|---|---|---|---|
| `adventure-live-tunables.toml` | [tunables.rs:593](../game/src/adventure/tunables.rs#L593) | [tunables.rs:610](../game/src/adventure/tunables.rs#L610) (`std::fs::write`, non-atomic) | **Loaded once at startup** into `AdventureManager`'s in-memory copy. The admin page's save writes the file *and* swaps the in-memory copy — persist first, then swap, so a write failure leaves the game on the on-disk values. **Editing the file by hand does NOT take effect until restart.** |
| `adventure-passive-overrides.toml` | [passive_overrides.rs:148](../game/src/adventure/passive_overrides.rs#L148) | [passive_overrides.rs:193](../game/src/adventure/passive_overrides.rs#L193) (`std::fs::write`, non-atomic) | Held in a `LazyLock<RwLock<…>>` ([passive_overrides.rs:141](../game/src/adventure/passive_overrides.rs#L141)), populated on first read. `save_passive_overrides` persists then hot-swaps under the write lock — same order, same reasoning. **Hand edits also need a restart.** |
| `adventure-item-balance.toml` | [balance.rs:60](../game/src/adventure/balance.rs#L60) | **never** | Lazily loaded on first item generation. Read-only from the game's perspective; edited by hand, applied at restart. |

**None of the three is affected by the platform.** The `toml` crate handles
CRLF and LF identically. The only Linux-relevant note is the non-atomic
`std::fs::write` on the first two (§6), and the fact that under a read-only
`/opt` they *must* live in the data directory — which `GAME_DATA_DIR`
already guarantees.

---

## 12. Ops layer

### Scheduled tasks — there are four registered, not two

The order describes "two processes under Windows scheduled tasks". Accurate
for the *processes*, but four tasks are registered and a fifth is
defined-but-not-registered. Full picture:

| Task | Runs | Trigger | Principal | Registered? |
|---|---|---|---|---|
| `TwitchBotRS` | the bot binary | at boot / on demand; native `RestartOnFailure` configured but **proven unreliable** (`watchdog.ps1:4-8`) | `Administrator`, S4U, RunLevel **Limited** | Yes |
| `GameProcess` | `target\release\game.exe` | same | same | Yes |
| `TwitchBotRS-Watchdog` | `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\PathofDust\watchdog.ps1"` — **no arguments** | repeating, short interval | same | Yes |
| `GameProcess-Watchdog` | `powershell.exe … -File "…\game-watchdog.ps1"` — **no arguments** | repeating, `PT2M` (`maintenance-flag.ps1:14`) | same | Yes |
| `GameDataBackup` | `backup-game-data.ps1 -SourceDir "C:\PathofDust"` | hourly | same | **No** — definition only, `docs/ops_backup_and_watchdog.md:165-180` |

A constraint worth carrying forward: **`RunLevel = Limited` means no deploy
session has an elevated token**, which is why `Disable-ScheduledTask` fails
and why `maintenance-flag.ps1` exists at all (`maintenance-flag.ps1:5-12`).
It is also why the watchdogs cannot read another process's executable path
(`game-watchdog.ps1:41-48`). **Both limitations vanish on Linux**, where
`systemctl` on a user-scoped or polkit-permitted unit needs no privilege
escalation and `/proc/<pid>/exe` is readable.

### The four PowerShell scripts

#### `watchdog.ps1` — 493 lines

**Job.** Every ~2 minutes: is anything LISTENING on port 4001? If not,
re-check once after `-RecheckDelaySeconds` (5), honour a startup grace of
`-StartupGraceSeconds` (45), honour the maintenance flag, then
`Start-ScheduledTask -TaskName TwitchBotRS` and append one line to
`watchdog.log`. A healthy run logs nothing.

Port 4001 was chosen deliberately over 4002/4003: 4002 is conditional on
`youtube_api_keys` being non-empty, and 4003 binds after a network
round-trip to Twitch — either would produce false deaths
(`watchdog.ps1:29-45`).

Windows-specific pieces: `Get-NetTCPConnection` with a `netstat -ano`
fallback (`watchdog.ps1:201-209`), `Get-Process`,
`Get-CimInstance Win32_Process`, `Get-ScheduledTaskInfo`,
`Start-ScheduledTask`, `$PSScriptRoot`.

**Linux equivalent: NO LONGER NEEDED.** `Restart=always` + `RestartSec=` in
the unit file does this natively, and systemd restarts on *process exit*,
which is strictly earlier and more reliable than polling a port every two
minutes. The entire class of bug this script works around — Task
Scheduler's `RestartOnFailure` not firing across `STATUS_STACK_OVERFLOW`
(`watchdog.ps1:4-8`) — has no systemd analogue. Optionally add
`WatchdogSec=` with `sd_notify` for liveness rather than mere aliveness,
but that needs code and is not required.
**Effort: Trivial (delete + 2 unit-file lines).**

#### `game-watchdog.ps1` — 498 lines

**Job.** Identical shape, for port 4005 / task `GameProcess`, with a
90-second startup grace instead of 45 because `AdventureManager::new` loads
a 3.3 MB roster and runs migrations before binding
(`docs/ops_backup_and_watchdog.md:440-450`).

**Linux equivalent: NO LONGER NEEDED**, same reasoning. The startup grace
maps to `TimeoutStartSec=`; `Restart=always` handles the rest. If liveness
(not just aliveness) is wanted, `Type=notify` + `WatchdogSec=` is the
systemd-native form.
**Effort: Trivial (delete + 3 unit-file lines).**

#### `maintenance-flag.ps1` — 307 lines

**Job.** Writes/removes `game-watchdog-maintenance.flag` or
`bot-watchdog-maintenance.flag` next to the scripts, which the two
watchdogs honour, suppressing a restart during a binary swap. It exists
**purely** because a non-elevated deploy session cannot run
`Disable-ScheduledTask` (`maintenance-flag.ps1:5-12`). The flag is a
30-minute lease, not a switch, so a forgotten flag cannot disable
protection forever (`maintenance-flag.ps1:20-26`).

**Linux equivalent: NO LONGER NEEDED.** `systemctl stop pathofdust-game`
stops the unit *and* suppresses restart, atomically, with no flag, no
lease, and no elevation problem. The deploy becomes stop → replace binary →
start. The whole two-flags-never-one design
(`maintenance-flag.ps1:37-45`) disappears with it.
**Effort: Trivial (delete). Note this also deletes REFACTOR_PLAN.md §13's
entire step-4 sub-chain, steps 4.1-4.8.**

#### `backup-game-data.ps1` — 702 lines

**Job.** Hourly snapshot of the game's persisted state to
`C:\pod-backups\PathofDust`. Copies (never moves), opens each source with
`FileShare.ReadWrite|Delete` so it can never block a save, **parses every
copy before pruning anything**, refuses to prune if the snapshot is
degraded, writes a `_backup-manifest.json`, and retains 24 hourly + 30
daily. Carries its own curated inventory of the files it backs up
(`backup-game-data.ps1:102-182`) — with code line references — and reports
drift when it sees an unknown `adventure-*-marker.json`, backing it up
anyway.

**Linux equivalent: STILL NEEDED — a systemd timer + a rewritten script.**
This is the only one of the four that does real work systemd does not
replace. What changes:

| Windows concern | Linux |
|---|---|
| `FileShare.ReadWrite\|Delete` to avoid blocking a save | **Unnecessary.** POSIX has no mandatory locking; a reader never blocks a writer. |
| Copy taken mid-`std::fs::write` yields a truncated file | Largely obsolete — `write_atomic` (§6) means a reader sees the old or the new complete file. **Keep the parse-verify anyway** for the two non-atomic TOMLs and as defence in depth. |
| `-SourceDir $PSScriptRoot` | `-SourceDir` becomes `/var/lib/pathofdust`, which after §10 is a clean data-only directory — the file inventory could shrink to "everything in the data dir", removing the hand-maintained list and its drift-report machinery entirely. |
| Hourly `Register-ScheduledTask` with `-MultipleInstances IgnoreNew` | `pathofdust-backup.timer` (`OnCalendar=hourly`) + `.service`. Systemd will not start a second instance of a running service, so `IgnoreNew` is free. |
| Retention pruning | Keep as-is, or replace with `zfs`/`btrfs` snapshots, or `restic`/`borg` if off-host backup is wanted — **which it should be; the current backup lives on the same disk as the data.** |

**Effort: Medium.** The *logic* is well-specified and the rewrite is
smaller than the original — roughly half the file is Windows file-sharing
and process-safety machinery that no longer applies. The parse-verify and
retention policy are worth preserving verbatim in behaviour. **Do not skip
this one:** its own header records that the August 2026 BOM incident was
recovered only by luck (`backup-game-data.ps1:14-16`).

### Ops layer summary

| Artefact | Lines | Linux disposition | Effort |
|---|---|---|---|
| `TwitchBotRS` task | — | `pathofdust-bot.service` | Small |
| `GameProcess` task | — | `pathofdust-game.service` | Small |
| `TwitchBotRS-Watchdog` task + `watchdog.ps1` | 493 | **Deleted** — `Restart=always` | Trivial |
| `GameProcess-Watchdog` task + `game-watchdog.ps1` | 498 | **Deleted** — `Restart=always` | Trivial |
| `maintenance-flag.ps1` | 307 | **Deleted** — `systemctl stop` | Trivial |
| `backup-game-data.ps1` + `GameDataBackup` task | 702 | **Rewritten** — `.timer` + `.service` | Medium |
| `REFACTOR_PLAN.md` §13 deploy procedure | ~230 | **Rewritten** — the 8-step binary-swap chain collapses to stop/replace/start | Medium |
| `docs/ops_backup_and_watchdog.md` | ~570 | **Rewritten** | Medium |
| `crashdumps/` (gitignored) | — | Windows WER → `systemd-coredump` | Trivial |

**Net: ~1,300 of the ~1,940 PowerShell lines are deleted outright, not
ported.** systemd does natively what three of the four scripts were built
to work around.

---

## 13. Residual notes

Items found during the sweep that are true, relevant to the move, and did
not fit a section above. **None was acted on.**

- **`announcements.json`** is gitignored but no code reference exists in
  either crate — `src/announcements.rs` fetches from `ANNOUNCEMENTS_URL`
  over HTTP, not from a file. Likely a stale ignore entry from the Node
  bot. No migration impact.
- **`_backup-manifest.json`** (`backup-game-data.ps1:187`) is **not**
  gitignored. Harmless today because it is written under `C:\pod-backups`,
  outside the repo — but it would become an untracked file if a backup root
  were ever pointed inside the tree. The script already refuses a backup
  root inside the source directory
  (`docs/ops_backup_and_watchdog.md:193`).
- **`cloudflare-paypal-relay/`** (`worker.js`, `wrangler.toml`) is a
  Cloudflare Worker, not a local process. Platform-independent; unaffected
  by the move. The bot reaches it via `PAYPAL_RELAY_URL`
  ([config.rs:260](../src/config.rs#L260)).
- **`tools/*.mjs`** (`gen-bundle-validator.mjs`,
  `bundle-contract.test.mjs`) are Node scripts with no platform
  assumptions. They need `node` on the build host, same as today.
- **Test harness CWD anchoring** — 12 files under `game/tests/` call
  `std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))`.
  Cargo emits a native absolute path and the `/..` suffix resolves on both
  platforms. **Portable as-is**; noted only because it is the mechanism
  that makes the CWD-relative design testable at all.
- **Thread stack size** — both binaries build their Tokio runtime with
  `thread_stack_size(32 * 1024 * 1024)`
  ([game/src/main.rs:55](../game/src/main.rs#L55)) because the real combat
  simulation overflowed the 2 MiB default and caused repeated
  `STATUS_STACK_OVERFLOW` crashes. This is explicit in code and
  platform-independent — **but the *main* thread's stack is set by the
  linker on Windows and by `ulimit -s` (default 8 MiB) on Linux.** The
  runtime is built inside `main`, so the simulation runs on worker threads
  with the explicit 32 MiB and this should be fine; it is worth one live
  soak-test check rather than an assumption, because the failure mode is a
  hard crash and it is the exact class of bug the watchdogs were written
  for.

---

## Appendix — how this audit was performed

Every claim above is grounded in a mechanical sweep of the checked-in tree,
not in inspection of the live deployment. `C:\PathofDust` was not read,
written, or otherwise touched.

- Regex sweeps across both crates for drive letters, backslash literals,
  `cfg(windows)`/`cfg(target_os)`/`winapi`/`std::os::windows`,
  `process::Command`/`powershell`/`cmd.exe`/`taskkill`, `env::var`,
  `create_dir_all`/`read_dir`/`rolling::`, and `const *_PATH`/`*_DIR`.
- Path-literal extraction from all `.rs`/`.html`/`.js`/`.json` sources,
  each of the 87 results tested for exact-case existence on disk.
- `ALL_SPRITES` (90 entries) and the boss-sprite table compared
  byte-for-byte against the sprite directory.
- `git ls-files --eol` for line-ending truth in index vs working tree;
  `git ls-files | tr A-Z a-z | uniq -d` for case-colliding tracked paths.
- Targeted range reads of `state.rs`, `paths.rs`, `render.rs`, `wiki.rs`,
  `fight_storage.rs`, `tunables.rs`, `balance.rs`, `passive_overrides.rs`,
  `config.rs`, both `main.rs` files, and the four PowerShell headers and
  parameter blocks. Per CLAUDE.md, `combat.rs`, `manager.rs`,
  `character.rs`, `item.rs` and `adventure_web.rs` were never read whole —
  only grepped and read in narrow ranges around hits.
