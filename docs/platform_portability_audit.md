# Platform portability audit — Windows → Ubuntu LTS

Read-only inventory of every Windows-specific assumption in this
repository, produced 2026-08-27 on branch `docs/platform-portability-audit`
against `origin/master`. **No behaviour was changed by this audit and
nothing here has been implemented.** Every item is file + line, what it
assumes, what breaks on Linux, and effort to fix.

> **SUPERSEDED IN PART — 2026-09-02 (`chore/bot-decoupling`).** The bot no
> longer has an adventure integration of any kind. `AdventureApiClient`,
> `src/published_constants.rs`, the ten adventure chat commands, the three
> adventure channel-point redemptions, chat activity XP and the SSE
> announcements relay are all deleted from `src/**`, and
> `ADVENTURE_API_SECRET`, `ADVENTURE_API_BASE_URL`,
> `CHANNEL_POINTS_REFORGE_REWARD_COST`, `CHANNEL_POINTS_REPAIR_REWARD_COST`
> and `CHANNEL_POINTS_FORCE_BOSS_REWARD_COST` no longer exist in
> `src/config.rs`. The game side went first: `game/src/adventure_web/api.rs`
> and the whole `/api/*` router are gone. Every statement below that
> describes the seam, those env keys or those commands as live describes
> history, not the current tree. The bot itself is unaffected and still
> runs: Twitch chat, song requests, alerts, entrance themes, the two
> surviving channel-point rewards, PoE utilities and OBS control.

Effort scale used throughout:

| Rating | Meaning |
|---|---|
| **Trivial** | One line, no design decision. |
| **Small** | A handful of lines in one or two files. |
| **Medium** | A named change across several files, needs a decision first. |
| **Large** | New structure/ops artefacts; a project stage of its own. |

---

## 0. Headline verdict

> **Scope ruling (settled 2026-08-27, see §16).** **Only the `game`
> process migrates to Linux.** The `twitch-bot-rs` process stays on the
> Windows machine, because OBS runs there and the bot's remaining role is
> Twitch chat and OBS overlays. The bot's 21 path literals and its half of
> the ops layer are **not migration work**. §10.5 and §12 below are stated
> for the game alone; the bot inventories (§10.2) are retained as
> reference, not as a work list.

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

**Network (§14):** `adventure.lokati.net` is served by a `cloudflared`
tunnel fronting port 4005 — established from the repository, not inferred.
The tunnel's *ingress configuration* is not in the repository;
`C:\Users\Administrator\.cloudflared\config.yml` is the file that answers
it. Because `cloudflared` dials outbound, the VPS needs **no inbound ports
open at all**.

**Build (§15):** the game needs `build-essential pkg-config libssl-dev` on
Ubuntu and nothing else. Dependency resolution for
`x86_64-unknown-linux-gnu` succeeds and first-party code is entirely
platform-neutral, but **no Linux build has ever been attempted** — only
`x86_64-pc-windows-msvc` is installed here and there is no CI.

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

### 10.5 What a three-directory split would cost — GAME ONLY

**Restated for the settled scope (§16): only the `game` process moves.**
The bot keeps its current Windows deploy root untouched, so none of §10.2
is migration work. What follows costs out the game alone.

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

**Step 2 — close the Group B gap. 2 files, ~4 lines.**

| Change | File | Size |
|---|---|---|
| Route `adventure-sessions.json` through `data_path` | [game/src/main.rs:165](../game/src/main.rs#L165) | 1 line |
| Route the wings-giveaway marker through `data_path` | [game/src/main.rs:145-152](../game/src/main.rs#L145-L152) | 2 lines |
| Route `patch-notes.json` through `data_path` | [adventure_web.rs:1859](../game/src/adventure_web.rs#L1859) | 1 line |
| ~~Decide `bot-published-constants.json`'s home~~ | — | **SETTLED — no work (§16)** |

`bot-published-constants.json` needs no cross-CWD home: it is part of the
integration surface being deleted, and its only consumer already degrades
gracefully — `wiki_placeholder_map` renders `"varies"` when the file is
absent, and [game/src/main.rs:114-118](../game/src/main.rs#L114-L118)
already logs a warning rather than failing. Leaving it CWD-relative and
unwritten is a supported state today. **No code change.**

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

**Step 4 — bot data. REMOVED FROM SCOPE (§16).**
The bot has no `data_path` equivalent — 20 hardcoded `PathBuf::from("…")`
literals in `src/main.rs` plus one in
[playrandom.rs:274](../src/playrandom.rs#L274) — but it stays on Windows in
its existing deploy root, where CWD-relative resolution keeps working
exactly as it does today. **Zero lines. The inventory in §10.2 is retained
as reference only.**

**Step 5 — logs. 1 line, or zero.**
[game/src/main.rs:69-70](../game/src/main.rs#L69-L70) hardcodes `"logs"`.
Either change the literal to `/var/log/pathofdust` (1 line), or **drop the
file appender entirely** and let the process log to stdout, which systemd
captures into the journal automatically. The second is smaller, more
idiomatic on Linux, and deletes the "logs/ grew to several GB" problem
recorded at [src/main.rs:518](../src/main.rs#L518) — journald rotates by
default. **Effort: Trivial either way.** The `_log_guard` lifetime dance in
`game/src/main.rs` exists only for the file appender; dropping it
simplifies that `main`. The bot's own `logs/`
([src/main.rs:520-521](../src/main.rs#L520-L521)) is untouched — it stays
on Windows.

**Step 6 — `.env`. See §11.** Note the game reads only a small subset of
keys — it does **not** need the bot's ~35 others, so the VPS
`EnvironmentFile` is a subset, not a copy of the existing `.env`.

> **Updated 2026-09-02 (TWITCH-REMOVAL-GAME).** This paragraph listed six
> keys. Four of them — `TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET`,
> `ADVENTURE_API_SECRET` and `ADVENTURE_WEB_PUBLIC_URL` — are read by no
> code at all now that the Twitch login and the `/api/*` seam are deleted,
> and are being removed from the unit and the drop-in. The game's complete
> environment surface is `ADVENTURE_WEB_PORT`,
> `ADVENTURE_OVERLAY_SERVER_PORT`, `OPERATOR_LOGIN`, and the optional
> `GAME_DATA_DIR` and `OPERATOR_BOOTSTRAP`.

#### Revised total — game only

| Step | Change | Lines | Files |
|---|---|---|---|
| 1 | Set `GAME_DATA_DIR=/var/lib/pathofdust` | **0** | — (env only) |
| 2 | Route sessions + wings marker + patch-notes through `data_path` | **4** | `game/src/main.rs`, `game/src/adventure_web.rs` |
| 3 | Custom sprites out of the source tree | **0** (symlink) or ~6 (second mount) | — or `game/src/adventure/character.rs` + `game/src/main.rs` + `game/src/adventure_web.rs` |
| 4 | Bot data | **0** — out of scope | — |
| 5 | Logs → `/var/log/pathofdust`, or stdout/journald | **1** or 0 | `game/src/main.rs` |
| 6 | `.env` → `EnvironmentFile` | **0** | — (env only) |

**Total: ~5 lines across 2 files**, plus one symlink-or-mount decision for
custom sprites. Recommendation: take the symlink for step 3 and stdout for
step 5, which lands the whole data-layout change at **4 lines in 2 files**.

**Files touched, complete list:** `game/src/main.rs`,
`game/src/adventure_web.rs` — and `game/src/adventure/character.rs` only if
step 3 takes the second-mount route rather than the symlink.

Tracked separately, not part of the layout change: the §8 custom-sprite
case fix (1 function in `game/src/adventure/character.rs`) and the optional
§6 directory-fsync hardening (1 function in `game/src/state.rs`).

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
| `OBS_WEBSOCKET_URL` | **SETTLED — no change (§16).** Defaults to `ws://127.0.0.1:4455` ([config.rs:288](../src/config.rs#L288)) and stays correct: the bot does not migrate, so it remains co-located with OBS. | **None** |
| `ADVENTURE_API_BASE_URL` | **Changes, and this is now the single largest configuration consequence of the move.** Defaults to `http://127.0.0.1:4005` ([config.rs:279](../src/config.rs#L279)). With the game on a VPS and the bot on Windows, the bot→game seam stops being a loopback call and becomes a **cross-internet** one: plain HTTP, authenticated only by the `x-adventure-api-secret` header ([adventure_client.rs:21](../src/adventure_client.rs#L21)), carrying every `!join`/`!character`/redemption call and an announcements stream. It must be pointed at an HTTPS endpoint (the existing tunnel hostname is the obvious candidate) — **sending that shared secret over plaintext WAN HTTP would be a real credential exposure.** See §14. | **Medium — ops + one env value; no code change** |
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

**Linux equivalent: NOT APPLICABLE — this one STAYS (§16).** The bot does
not migrate, so `watchdog.ps1` and the `TwitchBotRS-Watchdog` task remain
in service on Windows, unchanged. It is listed here only because the order
asked for all four scripts, and because its detection design (port-based,
not image-name-based) is the pattern the Linux side should not need to
reinvent — systemd's `Restart=always` supersedes it for the game.
**Effort: none. Do not touch.**

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

**Linux equivalent: HALF of it goes.** `systemctl stop pathofdust-game`
stops the unit *and* suppresses restart, atomically, with no flag, no
lease, and no elevation problem — so the `-Target Game` half becomes dead
code and REFACTOR_PLAN.md §13's entire step-4 sub-chain (steps 4.1-4.8)
collapses to stop → replace binary → start.

**But the script itself must NOT be deleted (§16):** `-Target Bot` still
drives `bot-watchdog-maintenance.flag` for `watchdog.ps1`, which stays in
service on Windows. The two-flags-never-one design
(`maintenance-flag.ps1:37-45`) turns out to be exactly what makes this
survivable — because the flags were never shared, removing the game's half
cannot disturb the bot's.
**Effort: Small — retire the `Game` target and its default
(`[ValidateSet('Game','Bot')] $Target = 'Game'`, `maintenance-flag.ps1`
param block), leaving `Bot` as the only value. Note the default flips, so
every existing `-Target Bot` invocation keeps working and any bare
invocation must be re-checked.**

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

### Ops layer summary — revised for game-only scope (§16)

| Artefact | Lines | Disposition | Effort |
|---|---|---|---|
| `TwitchBotRS` task | — | **STAYS on Windows**, unchanged | none |
| `TwitchBotRS-Watchdog` task + `watchdog.ps1` | 493 | **STAYS on Windows**, unchanged | none |
| `GameProcess` task | — | → `pathofdust-game.service` (`Restart=always`, `TimeoutStartSec=90`, `WorkingDirectory=/opt/pathofdust`) | Small |
| `GameProcess-Watchdog` task + `game-watchdog.ps1` | 498 | **Deleted** — `Restart=always` replaces it | Trivial |
| `maintenance-flag.ps1` | 307 | **Kept, halved** — `-Target Game` retired, `-Target Bot` stays live | Small |
| `backup-game-data.ps1` + `GameDataBackup` task | 702 | **Rewritten for Linux** — `.timer` + `.service`. Mandatory: the data it protects is exactly what moves | Medium |
| `REFACTOR_PLAN.md` §13 deploy procedure | ~230 | **Rewritten** — step-4 sub-chain collapses to stop/replace/start; no rename-aside (§6) | Medium |
| `docs/ops_backup_and_watchdog.md` | ~570 | **Split** — the game half rewritten for systemd, the bot half kept as-is | Medium |
| `crashdumps/` (gitignored) | — | Windows WER → `systemd-coredump` (game only) | Trivial |

**Net for the game alone: 498 PowerShell lines deleted outright
(`game-watchdog.ps1`), 702 rewritten (`backup-game-data.ps1`), ~150 of
`maintenance-flag.ps1` retired.** The bot's 493 lines and its two tasks are
untouched.

**Consequence of the split that did not exist before:** the game's backup
now runs on the VPS against `/var/lib/pathofdust`, while the bot's data
keeps living on the Windows machine with no scheduled backup at all — the
existing `GameDataBackup` definition covers only game files
(`backup-game-data.ps1:102-182`). That is not a regression introduced by
the move (the bot has never been backed up), but it becomes newly visible
once the two data sets are on different hosts, and it is worth an explicit
decision rather than an omission.

**Also new: the deploy is now remote.** §13's procedure assumes a local
filesystem copy into `target\release\`. Shipping to a VPS needs a transport
(`scp`/`rsync` of a locally cross-built binary, or a build on the VPS
itself) and the "confirm the resolved process path is NOT under
`C:\PathofDust`" safety rule in CLAUDE.md needs a Linux restatement —
`systemctl stop pathofdust-game` is inherently unit-scoped, so it satisfies
the rule's *intent* (never match by image name) natively.

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

## 14. Network and TLS

### Is `adventure.lokati.net` served through a Cloudflare Tunnel?

**Yes — and it is determinable from the repository, without inferring
anything from a directory name.** The repository names `cloudflared`
explicitly, in a file whose whole purpose is to identify the live
deployment:

- **`game-watchdog.ps1:108-116`** — the `-Port` parameter's own
  documentation, the primary citation:

  > *"The port that identifies THIS deployment. 4005 is adventure_web
  > (game/src/main.rs, ADVENTURE_WEB_PORT default) — **the port the
  > cloudflared tunnel fronts** and the one the bot's
  > ADVENTURE_API_BASE_URL points at, i.e. the port whose absence actually
  > means this world is down."*

  This names the daemon (`cloudflared`, not merely "Cloudflare"), names the
  port it fronts (4005), and was written as an operational fact to justify
  a liveness check — not as an aspiration.

- **[adventure_overlay_server.rs:157-160](../game/src/adventure_overlay_server.rs#L157-L160)**
  — corroborates the hostname:

  > *"…so the public dashboard (**port 4005, already tunneled to
  > adventure.lokati.net**) can serve the SAME overlay page/feed without
  > needing its own separate public port/DNS entry."*

- **[adventure_web.rs:1877-1880](../game/src/adventure_web.rs#L1877-L1880)**
  — *"serving the identical file from this ALREADY-public dashboard host
  just works, with **zero new tunnel/DNS/infra changes**."*

- **[config.rs:96-99](../src/config.rs#L96-L99)** — `adventure_web_public_url`
  *"Defaults to localhost, which only the streamer's own PC can reach — set
  this to a real public URL (**behind a tunnel**/reverse proxy/port-forward
  you set up separately)."*

Together these establish: **port 4005 on the game process is fronted by a
`cloudflared` tunnel and published as `adventure.lokati.net`.** TLS is
terminated by Cloudflare at the edge; the origin leg is the tunnel's own
outbound QUIC/HTTPS connection, so the game process itself serves plain
HTTP and holds no certificate — consistent with the code, which has no TLS
listener anywhere (all five `TcpListener::bind` calls are plaintext axum).

### Two pieces of the order's evidence do NOT support the conclusion

Stated plainly, because building on a wrong premise is worse than
correcting it:

- **`cloudflare-paypal-relay/` is not tunnel evidence.** It is a Cloudflare
  *Worker* that receives PayPal webhooks and is polled by the bot
  ([paypal.rs:3-4](../src/paypal.rs#L3-L4), `README.md:25`). It exists
  precisely *because* the bot has **no** public address — the opposite of a
  tunnel. Unrelated product, unrelated purpose.
- **`.env.example:10`'s "Cloudflare-deployed"** describes the **lokati.net
  static site folder** that `commands-data.json` is written into, not the
  game. Also unrelated.

The conclusion rests entirely on the four citations in the previous
subsection.

### What is NOT determinable from the repository

**The tunnel's configuration.** The repository contains no cloudflared
artefact of any kind — `git ls-files` matching `cloudflar|config.ya?ml|ingress`
returns only the three unrelated `cloudflare-paypal-relay/` files. So the
repo proves *that* a tunnel fronts 4005, but not:

- which hostnames map to which local ports (the ingress rules);
- whether any port other than 4005 is published;
- the tunnel UUID or account;
- whether `cloudflared` runs as a Windows service or a console process;
- `noTLSVerify` / origin-request settings.

**The exact file that would answer all of this:**

```
C:\Users\Administrator\.cloudflared\config.yml
```

That is the ingress map and is the single authoritative answer. Supporting
artefacts in the same directory, listed for completeness — **do not print
the contents of the credentials file, it is a secret**:

| File | What it answers |
|---|---|
| `C:\Users\Administrator\.cloudflared\config.yml` | **The ingress rules — hostname → local port. This is the file.** |
| `C:\Users\Administrator\.cloudflared\<TUNNEL-UUID>.json` | Tunnel credentials. Its *filename* gives the UUID. **Secret — do not read or print.** |
| `C:\Users\Administrator\.cloudflared\cert.pem` | Account cert; confirms which Cloudflare account owns the tunnel. **Secret.** |
| `Get-Service cloudflared` / `HKLM:\SYSTEM\CurrentControlSet\Services\Cloudflared` | Whether it runs as a service, and with what arguments — a service may pass `--config` pointing somewhere else entirely, which would override the path above. |

Non-file alternatives that answer it without touching secrets:
`cloudflared tunnel list` and `cloudflared tunnel info <name>`, or the
Zero Trust dashboard.

**This audit did not read any of them** — they are outside the repository,
and the order for this session was repository-scoped.

### Every port bound by either process

| Port | Bound by | Process | Bind address | Env var (default) | Exposure |
|---|---|---|---|---|---|
| **4001** | `start_alert_server` — [alerts.rs:60](../src/alerts.rs#L60) | bot | `0.0.0.0` | `ALERT_SERVER_PORT` (4001) | LAN-reachable. **No repo evidence of public mapping.** Unconditional; binds first. |
| **4002** | `start_song_overlay_server` — [song_overlay_server.rs:49](../src/song_overlay_server.rs#L49) | bot | `0.0.0.0` | `SONG_REQUEST_SERVER_PORT` (4002) | LAN-reachable. **Conditional** — only binds when `YOUTUBE_API_KEYS` is non-empty ([src/main.rs:555](../src/main.rs#L555)). No repo evidence of public mapping. |
| **4003** | `start_chat_overlay_server` — [chat_overlay_server.rs:97](../src/chat_overlay_server.rs#L97) | bot | `0.0.0.0` | `CHAT_OVERLAY_SERVER_PORT` (4003) | LAN-reachable. Binds last, after a Twitch emote fetch. No repo evidence of public mapping. |
| **4004** | `start_adventure_overlay_server` — [adventure_overlay_server.rs:52](../game/src/adventure_overlay_server.rs#L52) | **game** | `0.0.0.0` | `ADVENTURE_OVERLAY_SERVER_PORT` (4004) | LAN-reachable. **Positive evidence it is NOT published:** [adventure_overlay_server.rs:157-160](../game/src/adventure_overlay_server.rs#L157-L160) says 4005 serves the same overlay *"without needing its own separate public port/DNS entry"*. |
| **4005** | `start_adventure_web_server` — [adventure_web.rs:229](../game/src/adventure_web.rs#L229) | **game** | `0.0.0.0` | `ADVENTURE_WEB_PORT` (4005) | **PUBLIC** — fronted by `cloudflared` as `adventure.lokati.net`. Carries the dashboard, `/wiki`, `/overlay`, `/ws`, **and the `/api/*` seam** (nested onto the same router at [adventure_web.rs:156](../game/src/adventure_web.rs#L156) — *not* a separate port). |
| **3000** | `auth` helper binary — [auth.rs:68](../src/bin/auth.rs#L68) | `auth` (manual, transient) | **`127.0.0.1`** | hardcoded `PORT` | **Localhost only, by bind.** OAuth callback; runs only during a manual re-auth. |
| **3001** | `auth_patreon` helper binary — [auth_patreon.rs:56](../src/bin/auth_patreon.rs#L56) | `auth_patreon` (manual, transient) | **`127.0.0.1`** | hardcoded `PORT` | **Localhost only, by bind.** |

**Outbound only — binds nothing:** the OBS WebSocket client
(`OBS_WEBSOCKET_URL`, default `ws://127.0.0.1:4455`,
[config.rs:288](../src/config.rs#L288)), Twitch IRC/EventSub, the
YouTube/Last.fm/poe.ninja/StreamElements HTTP clients, the PayPal Worker
poll, and the bot→game `/api/*` client
([adventure_client.rs](../src/adventure_client.rs)).

**The distinction that matters for the hosting decision:** binding
`0.0.0.0` means *reachable on every interface the host has* — on the
current Windows machine that is the LAN, and the only thing making 4005
reachable from the internet is the tunnel. **On a VPS, `0.0.0.0` means
directly internet-reachable**, so 4004 and 4005 would both be exposed the
moment the process starts.

Two consequences worth pricing into the hosting purchase:

1. **`cloudflared` needs no inbound ports at all.** It dials out. If the
   game keeps its tunnel, the VPS can run a default-deny inbound firewall
   (`ufw default deny incoming`, SSH only) and 4004/4005 never need
   opening. This is the cheapest and most secure shape and requires no code
   change — the `0.0.0.0` binds are then only reachable from the VPS's own
   loopback and the tunnel connector.
2. **The `/api/*` seam is already on the public port.** It is nested onto
   4005, guarded solely by the `x-adventure-api-secret` header
   ([adventure_client.rs:21](../src/adventure_client.rs#L21)); `api::router`
   returns `None` and the mount is skipped when `ADVENTURE_API_SECRET` is
   unset. That is the status quo, not a regression — but after the move the
   bot reaches it from a *different host*, so the secret starts travelling
   over the WAN on every `!join`, redemption and activity-XP call. It must
   go over the HTTPS tunnel hostname, never a bare `http://<vps-ip>:4005`.
   See §11's revised `ADVENTURE_API_BASE_URL` row.

---

## 15. Build

### What is required to produce a Linux binary

| Requirement | Detail |
|---|---|
| **Toolchain** | Stable Rust, edition 2021. **No version is pinned** — there is no `rust-toolchain.toml` and no `.cargo/config.toml` anywhere in the tree. This worktree builds with `rustc 1.97.1 / cargo 1.97.1`. Pinning one before the migration would be cheap insurance, but nothing requires it today. |
| **Command (whole workspace)** | `cargo build --release --workspace` — per CLAUDE.md, a plain `cargo build` misses `game.exe`. Produces four binaries: `twitch-bot-rs`, `auth`, `auth_patreon` (root package) and `game` (the `game` package). |
| **Command (game only — the migrating unit)** | `cargo build --release -p game` produces the single `game` binary, no extension on Linux. |
| **Target** | `x86_64-unknown-linux-gnu`. |
| **Where to build** | **On the Ubuntu host, or in an Ubuntu container.** Cross-compiling from this Windows machine is possible in principle but needs a Linux linker *and* a Linux `libssl` for `openssl-sys` to link against — enough friction that `cross` (Docker) or a native VPS build is the better answer. |
| **glibc** | Build on the same Ubuntu LTS release you deploy to. A binary built against a newer glibc will not start on an older one. |

### Does the workspace build cleanly for Linux as configured today?

**Not verified, and it cannot be verified from this machine.**
`rustup target list --installed` returns exactly one target:
`x86_64-pc-windows-msvc`. There is no CI configuration in the repository.
**A Linux build of this workspace has, as far as the repository and this
machine can show, never been attempted.** Saying otherwise would be a
guess.

What **was** verified here, and is a real signal:

- **Full dependency resolution for `--target x86_64-unknown-linux-gnu`
  succeeds** for both packages, with no unresolved crate and no
  Windows-gated hole. `cargo tree -p game --target x86_64-unknown-linux-gnu`
  produces a complete graph in which `schannel` is correctly swapped for
  `openssl`/`openssl-sys`.
- **First-party code has nothing to fail on:** zero `cfg(windows)`, zero
  `winapi`/`windows-sys`, zero `std::os::windows`, zero Windows API calls,
  zero drive letters, zero backslash separators (§1, §4, §7). Nothing in
  `src/` or `game/src/` is platform-conditional.

**Therefore the expected blockers are system libraries, not code** — the
next subsection lists them exactly. The honest statement for a hosting
decision: *the code is expected to compile unmodified once three apt
packages are present, and that expectation should be confirmed by an actual
build before money is committed to a plan that depends on it.*

### Build-time dependencies satisfied on Windows, needing install on Ubuntu

This is the whole delta. Resolved from `Cargo.lock` and confirmed with
target-specific `cargo tree`.

**For the `game` crate — the only thing migrating:**

| Crate | Version | Why it is in the graph | Windows | Ubuntu requirement |
|---|---|---|---|---|
| `openssl-sys` | 0.9.117 | `reqwest 0.12` with default features → `default-tls` → `native-tls`. On Windows `native-tls` selects `schannel` (nothing to install); on Linux it selects `openssl`. | satisfied by the OS | **`libssl-dev`** + **`pkg-config`** + a **C compiler** (its build script uses `cc`) |
| `openssl` | 0.10.81 | same | — | same as above |
| `flate2` | 1.1.9 | overlay compression | pure Rust | **none** — resolves to `miniz_oxide`, not `libz-sys`. No zlib needed. |

`openssl-src` is **absent** from `Cargo.lock`, which means the `vendored`
feature is **off** — so `openssl-sys` links the *system* libssl and
`libssl-dev` is genuinely required, not optional.

**Complete Ubuntu prerequisite for the game:**

```
apt install build-essential pkg-config libssl-dev
```

That is the entire list. The game's graph contains **no `ring`, no
`rustls`, no `zstd-sys`, no `libz-sys`** — nothing else needs a C toolchain
or a system library.

**For the bot — not migrating (§16), recorded for completeness.** Were it
ever moved, it would additionally need a C toolchain for two more crates:

| Crate | Version | Why | Note |
|---|---|---|---|
| `ring` | 0.17.14 | `twitch-irc` pinned to `refreshing-token-rustls-webpki-roots` | needs `cc`; no system library, no OpenSSL |
| `zstd-sys` | 2.0.16+zstd.1.5.7 | root-only `zstd = "0.13"` | bundles its own C source; needs `cc`, no system library |

Both are covered by the same `build-essential`.

**One option, flagged not recommended** (it changes TLS behaviour, so it is
a decision rather than an audit finding): switching the game's `reqwest` to
`rustls-tls` — or enabling `openssl`'s `vendored` feature — would drop the
`libssl-dev` and `pkg-config` requirements entirely, leaving only
`build-essential`. It also removes the game's dependency on the host's CA
store and OpenSSL patch cadence. Not proposed here; noted because it is the
only lever that shortens the prerequisite list.

### Two things to verify on the first Linux build

Neither is expected to fail. Both are cheap to check and expensive to
discover late.

1. **Golden-corpus reproducibility across platforms.** The fixtures were
   captured on Windows/MSVC. `golden_corpus.rs` compares **structurally**
   (parsed JSON, with a documented 1-ULP tolerance on float *leaves* only —
   every gameplay-facing number is `.round()`-ed to an integer before it
   reaches a `CombatEvent`), so line endings are irrelevant and small float
   drift is already absorbed. The genuine cross-platform risk would be a
   libm difference, and the exposure is minimal: the entire `game` crate
   contains exactly **two** transcendental float calls — one `.sqrt()`
   ([manager.rs](../game/src/adventure/manager.rs), IEEE-754
   exactly-rounded, therefore bit-identical on every platform) and one
   `.ln()` ([pacing.rs:583](../game/src/adventure/pacing.rs#L583)), which
   sits inside `update_dmg_pacing_mult` — a between-fights controller that
   `simulate_battle` never calls. **The snapshot path touches no
   libm-variable function, so the fixtures are expected to reproduce
   bit-for-bit.** Per CLAUDE.md, if any fixture *does* mismatch it is
   reported with an attributed cause and **never regenerated** outside a
   merge.
2. **Full-suite run:** `cargo test --release --workspace --quiet`. All test
   scaffolding is already portable — `std::env::temp_dir()` (13 sites),
   `set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))` (12
   files), and `Child::kill()` in `killed_process_smoke.rs` (which becomes
   `SIGKILL` instead of `TerminateProcess`, exercising the same
   abrupt-death path the test exists to cover). Known flaky-under-parallel
   tests per CLAUDE.md still apply and must be confirmed in isolation
   before being reported as Linux regressions.

---

## 16. Decisions recorded — SETTLED

Ruled by the owner on 2026-08-27, in response to the questions this audit
raised. **These are settled, not open.** Earlier sections have been revised
to match; where a section still shows the pre-ruling analysis it is marked
as reference only.

| # | Decision | Consequence for this document |
|---|---|---|
| **D1** | **Only the `game` process migrates to Linux.** The bot stays on the Windows machine, because OBS runs there and the bot's remaining role is Twitch chat and OBS overlays only. The bot's path literals and its share of the ops layer are **not migration work**. | §10.5 rewritten as game-only (step 4 removed, ~5 lines across 2 files). §12 rewritten: `watchdog.ps1` and `TwitchBotRS`/`TwitchBotRS-Watchdog` **stay untouched on Windows**; `maintenance-flag.ps1` is halved rather than deleted. §10.2's bot inventory is retained as **reference, not a work list**. |
| **D2** | **`bot-published-constants.json` needs no cross-CWD home.** It is part of the integration surface being deleted, and its consumer already falls back gracefully to `"varies"`. | §10.5 step 2 drops from 4 changes to 3 (~4 lines). The file stays CWD-relative and unrouted; an absent file is a supported state — [game/src/main.rs:114-118](../game/src/main.rs#L114-L118) warns rather than fails, and `wiki_placeholder_map` renders `"varies"`. **No code change.** |
| **D3** | **`OBS_WEBSOCKET_URL=ws://127.0.0.1:4455` remains correct and needs no change**, since the bot stays co-located with OBS. | §11's "Decision needed" row replaced with a settled "no change" row. |

### One consequence of D1 that is new work, not removed work

D1 removes the bot's data and ops from scope, but it **creates** a
requirement that did not exist while both processes shared a host: the
bot→game `/api/*` seam becomes a cross-internet link. It is plain HTTP
authenticated by a single shared-secret header, and it carries every
`!join`, `!character`, redemption and activity-XP call plus an
announcements stream ([adventure_client.rs](../src/adventure_client.rs)).
`ADVENTURE_API_BASE_URL` must therefore point at an HTTPS endpoint — the
existing tunnel hostname is the obvious candidate, since 4005 already
carries the seam (§14). This is recorded as a consequence of the ruling,
not a reopening of it.

Two further points follow from D1 and are flagged for a separate decision,
not assumed here: the bot's own data has **no scheduled backup** and
becomes the only un-backed-up state once the game's backup moves to the
VPS (§12); and the deploy procedure becomes a *remote* one, needing a
transport that `REFACTOR_PLAN.md` §13 does not currently describe (§12).

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
- For §14: a repo-wide case-insensitive sweep for
  `cloudflar|tunnel|lokati\.net|trycloudflare|ngrok` across `.rs`, `.md`,
  `.toml`, `.ps1`, `.html`, `.js` and `.env.example`, then targeted reads of
  every hit; `git ls-files` matched against `cloudflar|config.ya?ml|ingress`
  to confirm no tunnel artefact is tracked. Every `TcpListener::bind` call
  site read for its bind address. **No file outside the repository was
  read** — in particular nothing under `C:\Users\Administrator\.cloudflared`.
- For §15: `cargo tree -p <pkg> -e normal[,dev] --target
  x86_64-unknown-linux-gnu` for both packages, to resolve the dependency
  graph as Linux would see it rather than as Windows does;
  `rustup target list --installed`; `rustc -V`/`cargo -V`; `Cargo.lock`
  inspected for `openssl-src` (absent → system libssl required) and for the
  `cc`/`pkg-config` build-script dependents. A repo-wide sweep for
  transcendental float calls (`powf|exp|ln|log*|sqrt|sin|cos|tan|cbrt|hypot`)
  to bound cross-platform float risk in the golden corpus. **No Linux
  compilation was attempted** — the target is not installed on this machine.
- Targeted range reads of `state.rs`, `paths.rs`, `render.rs`, `wiki.rs`,
  `fight_storage.rs`, `tunables.rs`, `balance.rs`, `passive_overrides.rs`,
  `config.rs`, both `main.rs` files, and the four PowerShell headers and
  parameter blocks. Per CLAUDE.md, `combat.rs`, `manager.rs`,
  `character.rs`, `item.rs` and `adventure_web.rs` were never read whole —
  only grepped and read in narrow ranges around hits.
