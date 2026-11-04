//! One file per settings page. Every builder composes adw preference
//! groups into an `adw::PreferencesPage`, which handles scrolling and the
//! centered column natively.

// Re-export the gui module's namespace (Ui, adw, widget helpers) so each
// page file can `use super::*;` and stay focused on its own widgets.
use super::*;

pub(super) mod tone;
pub(super) mod typing;
