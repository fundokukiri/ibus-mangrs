//! One file per settings page. Every builder composes adw preference
//! groups into an `adw::PreferencesPage`, which handles scrolling and the
//! centered column natively.

// Re-export the gui module's namespace (Ui, adw, widget helpers) so each
// page file can `use super::*;` and stay focused on its own widgets.
use super::*;

pub(super) mod about;
pub(super) mod data;
pub(super) mod emoji;
pub(super) mod hotkeys;
pub(super) mod macros;
pub(super) mod other;
pub(super) mod panel;
pub(super) mod rules;
pub(super) mod tone;
pub(super) mod tune;
pub(super) mod typing;
