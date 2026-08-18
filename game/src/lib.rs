// The `game` crate (2026-08-18, architecture refactor Stage 1-2) - the
// standalone-game addendum's library boundary. Everything here is the
// adventure game's own domain/persistence/web/ws logic, with zero
// dependency on Twitch/chat/bot concerns - `twitch-bot-rs` (the `bot`
// binary) depends on this crate and re-exports every module below under
// its original `crate::X` path (see its own `lib.rs`), so every existing
// `crate::adventure::X`/`crate::adventure_web::X`-style reference
// throughout the bot codebase (main.rs, commands.rs) keeps resolving
// completely unchanged.
//
// `adventure_web`/`adventure_overlay_server` (Stage 2, 2026-08-18) -
// including wiki.rs, a child module of `adventure_web` - moved here
// intact together with `adventure`/`passive_tree`/`state` (Stage 1).
// wiki.rs's own doc (inside `adventure_web/wiki.rs`) covers the owner's
// ruling on why it belongs here rather than staying bot-side, and how
// its 5 formerly-direct bot-side constant reads now flow through
// `adventure::PublishedConstants` instead.
pub mod adventure;
pub mod adventure_overlay_server;
pub mod adventure_web;
pub mod passive_tree;
pub mod state;
