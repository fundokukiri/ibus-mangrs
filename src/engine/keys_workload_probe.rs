use super::*;
use crate::bamboo_wrapper::Method;

/// Replay tests/keys-test through the engine the way the server does
/// (chars → process_char, Backspace → backspace(), nav/edit keys →
/// reset() + the key forwards) and count how many times the lone "x"
/// from the {home} scenario reaches the application. Exactly once per
/// round is the contract; the real-app runs showed 0..3.
#[test]
fn full_keys_workload_commits_the_lone_x_exactly_once_per_mode() {
    let keys = std::fs::read_to_string("tests/keys-test").expect("keys-test");
    let mut tokens: Vec<char> = Vec::new();
    let mut chars = keys.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for nc in chars.by_ref() {
                if nc == '}' {
                    break;
                }
                name.push(nc);
            }
            tokens.push(match name.as_str() {
                "backspace" => '\u{8}',
                "home" => '\u{1}',
                "end" | "delete" | "left" | "right" => '\u{1}',
                " " | "space" => ' ',
                other => panic!("token {{{other}}}"),
            });
        } else {
            tokens.push(c);
        }
    }

    for mode in [Mode::Preedit, Mode::NonPreedit, Mode::Surrounding] {
        let mut e = Engine::new(Method::Telex, mode);
        e.set_client(None);
        e.set_capabilities(0b101001);
        let mut x_commits = 0;
        let mut all: Vec<Action> = Vec::new();
        for t in &tokens {
            let actions = match *t {
                '\u{8}' => e.backspace().unwrap_or_default(),
                '\u{1}' => e.reset(),
                c => e.process_char(c),
            };
            for a in &actions {
                if let Action::Commit(text) = a {
                    if text.contains('x') {
                        x_commits += 1;
                    }
                }
            }
            all.extend(actions);
        }
        assert_eq!(x_commits, 1, "{mode:?}: {all:?}");
    }
}
