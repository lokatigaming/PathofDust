// The `game` crate (2026-08-18, architecture refactor Stage 1) - the
// standalone-game addendum's library boundary. Everything here is the
// adventure game's own domain/persistence logic, with zero dependency on
// Twitch/chat/bot concerns - `twitch-bot-rs` (the `bot` binary) depends on
// this crate and re-exports these three modules under their original
// `crate::adventure`/`crate::passive_tree`/`crate::state` paths (see its
// own `lib.rs`), so every existing `crate::adventure::X`-style reference
// throughout the bot codebase (main.rs, commands.rs, adventure_web.rs,
// and - not to be touched independently of this move, see CLAUDE.md's
// multi-session coordination rules - wiki.rs) keeps resolving completely
// unchanged. This is a mechanical move: file locations changed, nothing
// else did.
pub mod adventure;
pub mod passive_tree;
pub mod state;
