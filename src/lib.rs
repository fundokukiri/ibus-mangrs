//! Vietnamese input engine for IBus, built on `bamboo-core`.
//!
//! Layering, from the outside in:
//!
//! * [`server`] — D-Bus. Owns the connection, answers IBus calls, emits signals.
//! * [`engine`] — key events to [`engine::Action`]s. No D-Bus types.
//! * [`bamboo_wrapper`] — the only code that touches `bamboo-core`.
//! * [`keymap`] — X11 keysyms to `char`.
//! * [`ibus_text`] — `GVariant` encoding of `IBusText`.
//! * [`config`] — TOML settings shared with the GTK4 GUI.

pub mod backup;
pub mod bamboo_wrapper;
pub mod config;
pub mod debug_log;
pub mod engine;
#[cfg(test)]
mod golden;
#[cfg(feature = "gui")]
pub mod gui;
pub mod ibus_prop;
pub mod ibus_text;
pub mod keymap;
pub mod macros;
pub mod paste;
pub mod server;
pub mod spellcheck;
pub mod uinput;
