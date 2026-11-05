use super::*;
use crate::bamboo_wrapper::Method;

/// A lone letter typed after a forwarded caret move (Home) must reach the
/// application exactly once in every display mode. GNOME Text Editor
/// renders this inconsistently (double in preedit, dropped in nonpreedit
/// — app-side, see open-questions #12); the engine's action stream is the
/// contract this test locks in.
#[test]
fn lone_letter_after_caret_move_commits_exactly_once_per_mode() {
    for mode in [Mode::Preedit, Mode::NonPreedit, Mode::Surrounding] {
        let mut e = Engine::new(Method::Telex, mode);
        e.set_client(None);
        e.set_capabilities(0b101001);
        let mut all: Vec<Action> = Vec::new();
        for c in "ab ".chars() {
            all.extend(e.process_char(c));
        }
        all.extend(e.reset()); // Home: commit pending + the key forwards
        all.extend(e.process_char('x'));
        all.extend(e.process_char(' '));
        let x_commits = all
            .iter()
            .filter(|a| matches!(a, Action::Commit(t) if t == "x"))
            .count();
        assert_eq!(x_commits, 1, "{mode:?}: {all:?}");
        // nothing else may re-insert the letter (no second commit, and
        // the preedit path must hide before committing)
        assert!(
            all.windows(2)
                .any(|w| matches!(w, [Action::HidePreedit, Action::Commit(t)] if t == "x"))
                || matches!(mode, Mode::NonPreedit | Mode::Surrounding),
            "{mode:?}: preedit must hide before committing the letter: {all:?}"
        );
    }
}
