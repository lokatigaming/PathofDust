# Linux build gate — Debian 13 (trixie)

**Date:** 2026-08-31 · **Session:** LINUX-BUILD-GATE · **Branch:** `chore/linux-build-gate`
**Source:** `git archive` of `153e804` · **Scope:** build + test only. The game was never
run; no game data, `.env`, save file, token or secret was copied; no service was started.

**Verdict: YES.** The workspace builds clean on Debian 13 and passes 755/755 — the exact
Windows baseline. **Zero code changes were required.** The
`docs/platform_portability_audit.md` prediction (build-essential, pkg-config, libssl-dev
sufficient) held exactly, and the package names are identical on Debian to the Ubuntu the
audit assumed.

## The box

| | |
|---|---|
| OS | Debian GNU/Linux 13 (trixie), `VERSION_ID=13` |
| Kernel | `6.12.107+deb13-amd64` SMP PREEMPT_DYNAMIC |
| Arch | x86_64 / amd64 |
| CPU | 8 vCPU, QEMU Virtual CPU version 2.5+ |
| RAM | 15 GiB |
| Disk | 314 G total, 300 G free on `/dev/vda4` |
| **Swap** | **none configured** |
| Preinstalled | `curl` only — no gcc, no git, no rustc |

Nothing differed from the briefing except two items worth carrying forward: **there is no
swap**, and **`git` is not installed** (irrelevant here because the source arrived as an
archive, but a provisioning script that expects to clone will need it).

## Provisioning — the exact lines

These are the deliverable. Verified against Debian 13's archive, not assumed.

```sh
apt-get update
apt-get install -y build-essential pkg-config libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile default
. "$HOME/.cargo/env"
```

Nothing else was needed. No second pass, no missing-library failure, no error to work around.

| Package | Debian 13 candidate | Same name as audit assumed? |
|---|---|---|
| `build-essential` | 12.12 | yes |
| `pkg-config` | 1.8.1-4 | yes |
| `libssl-dev` | 3.5.7-1~deb13u2 | yes |

Resulting toolchain: gcc 14.2.0, GNU ld 2.44, `pkg-config --modversion openssl` → **3.5.7**.
`openssl-sys 0.9.117` in `Cargo.lock` handles OpenSSL 3.5 without complaint.

`time 1.9-0.2` was also installed, for `/usr/bin/time -v` measurement only. It is **not** a
build dependency and does not belong in the provisioning script. Noted because Debian's
minimal image does not ship `/usr/bin/time`.

### Rust

```
rustc 1.98.0 (88d9e12ae 2026-08-18)  host: x86_64-unknown-linux-gnu  LLVM 22.1.8
cargo 1.98.0 (797e8a9bc 2026-08-05)
```

**Version skew, recorded deliberately:** this machine's Windows toolchain is **1.97.1**;
rustup gave Linux **1.98.0**. There is no `rust-toolchain.toml` pinning either. The results
below are therefore "Debian 13 + 1.98.0" vs "Windows + 1.97.1" — since both build and test
came out identical the skew cost nothing, but a future divergence must not be attributed to
the platform before the toolchain is ruled out. Pinning a toolchain is the owner's decision,
not something this session changed.

## Shipping the source

`git archive --format=tar.gz HEAD` → **46,132,435 bytes (44 MiB), 333 entries**; 56 MiB
extracted to `/root/dust`. `git archive` excludes `.git/` and `target/` by construction;
this was confirmed against the archive listing rather than trusted. The only environment
file in it is `.env.example`. No credential reached the server, and the server has no GitHub
access. scp took 10.8 s.

## The build

`cargo build --release --workspace` — **succeeded on the first attempt.**

| | |
|---|---|
| Wall clock | **2 min 22.8 s** |
| CPU time | 955.05 s user + 45.30 s system, **700% CPU** (7 of 8 cores) |
| Peak RSS | **1,350,396 KB ≈ 1.29 GiB** |
| Exit status | 0 |
| Errors | none |
| Warnings | 8, all pre-existing `dead_code` / `unused_imports`, identical in kind to Windows, none platform-related |

Peak memory is 1.29 GiB against 15 GiB available, so the absent swap never mattered and a
much smaller box would do. A re-run finished in 0.25 s as a no-op, confirming the build
graph was genuinely complete rather than partially skipped.

| Binary | Size |
|---|---|
| `target/release/game` | 19 M |
| `target/release/twitch-bot-rs` | 23 M |
| `target/release/auth` | 6.2 M |
| `target/release/pseudonymize_characters` | 1.6 M |
| `target/` total | 714 M |

### One correction to the audit's reasoning

The audit's supporting claim that "no ring/rustls/zstd is in the game's graph" is true of the
`game` crate but **not of the workspace**. The root `twitch-bot-rs` crate pulls `ring 0.17.14`,
`rustls 0.21.12` and `0.23.42` (via `twitch-irc`'s `refreshing-token-rustls-webpki-roots`),
`zstd-sys 2.0.16+zstd.1.5.7` and `flate2`.

This changes nothing about the outcome — each builds its C from vendored source with the
compiler `build-essential` already provides, and none needs a system package. But the
audit's *conclusion* was right for a reason slightly wider than the argument it gave, and
anyone re-deriving the dependency list from that sentence should look at the whole workspace
graph, not the game's.

## The test suite

`cargo test --release --workspace --quiet`

| | Windows baseline | Debian 13 |
|---|---|---|
| Passed | 755 | **755** |
| Failed | 0 | **0** |
| Ignored | — | **0** |
| Exit status | — | 0 |

**Every difference: there are none.** No failure, no flake, nothing skipped, nothing
platform-gated out. 31 test binaries; `cargo test --release --workspace -- --list`
independently enumerates 755 test names, confirming the count is the whole suite executing
and not a subset. Wall clock 4 min 52.8 s, peak RSS 1.71 GiB.

Because the count is identical *and* the enumerated list is 755, there is no hidden
compensation — it is not 3 Windows-only tests dropping out while 3 Linux-only ones appear.

### The golden corpus — the finding worth stating loudly

`adventure::golden_corpus::golden_corpus_matches_committed_fixtures` and
`adventure::item::golden_item_baseline::generated_items_match_pre_refactor_baseline`
**both pass on Linux against fixtures generated on Windows.**

This was the significant open risk and it came back clean: the simulation's floating-point
results and its iteration/ordering are reproducible across Windows and Linux, across two
different rustc versions and two different LLVM backends. The corpus is a genuine
cross-platform contract, not a Windows-shaped one. A future golden-corpus mismatch on Linux
can therefore be read as a real behavioural change, not written off as platform float drift.

### `feature/linux-readiness` — first run on its target platform

Both of these had, until now, only ever executed on the platform they were written *not* to
be for. Both behave.

**`cfg(unix)` parent-directory fsync** — `game/src/state.rs`. `sync_parent_dir` opens the
parent and `sync_all()`s it on unix, no-ops on Windows; `ATOMIC_RENAME_ATTEMPTS` is 1 on unix
versus 5 on Windows. All three `state::atomic_save_tests` pass on Linux, including
`an_unwritable_destination_reports_an_error_and_leaves_no_temp`. That last one deserves a
note: **the suite ran as root**, where permission bits are routinely ignored, which would make
a chmod-based failure test vacuous. It is not chmod-based — it blocks the write by standing a
*directory* where the file belongs, so the rename fails for root exactly as for anyone else.
The test is sound as written under root; no change needed.

**Case-insensitive sprite resolver** — `custom_sprite_file_exists` in
`game/src/adventure/character.rs`, covered by `game/tests/custom_sprite_case.rs`. It passes,
and this is the first time it has meant anything: the resolver exists to make lookups behave
on a case-sensitive filesystem the way NTFS gave for free, and Linux is the first
case-sensitive filesystem it has run on. The test reads the real sprite directory and asserts
it is non-empty before concluding, so it cannot pass vacuously on a stripped checkout — the
archive carried real sprites and every case variant resolved identically.

## State left on the server

Source at `/root/dust`, build artifacts at `/root/dust/target` (714 M), toolchain at
`/root/.rustup`, logs at `/root/build.time.log` and `/root/test.log`, archive at
`/root/src.tar.gz`. **No service installed or started; no systemd unit, no cloudflared, no
game data.** The box holds a compiler and this source, nothing operational.

## What this does and does not settle

Settled: it compiles, it links against Debian's system OpenSSL, and the whole suite passes.
The single largest unknown in the World 2 plan is closed, and the provisioning list is three
packages long.

Not settled, and not in this session's scope: nothing has been *run*. Runtime behaviour —
path handling against real game data, file locking, the overlay servers binding ports,
anything touching the live data directory — remains unverified on Linux. A green suite is a
strong signal, but the tests supply their own fixtures; they do not exercise the production
data layout. That is the next gate, and a separate session.
