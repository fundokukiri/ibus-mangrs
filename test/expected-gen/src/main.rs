//! Generate the expected on-screen text for a keystroke file by replaying it
//! through the project's own `BambooWrapper` — the exact typing pipeline the
//! engine runs, including undo-mark handling and auto-restore — into a small
//! text-buffer model with a caret, so `{backspace}`, `{delete}` and caret
//! moves work too. There is deliberately no Enter token.
//!
//! Usage:
//!   cargo run --offline -- <keys-file>           # print expected text
//!   cargo run --offline -- --check <pairs.tsv>   # verify keys<TAB>word pairs
use ibus_mangrs::bamboo_wrapper::{BambooWrapper, Method, Outcome};

/// The text the committed word contributes: valid words commit as composed,
/// invalid ones auto-restore to the raw keystrokes — except a word in which
/// a mark was undone, which must keep what is on screen ("need", "tool").
fn committed(wrapper: &BambooWrapper, out: Outcome) -> String {
    let Outcome::Finish { word, raw, valid } = out else { return String::new() };
    if valid || wrapper.undo_in_word() { word } else { raw }
}

/// Commit whatever composition is pending, exactly like the engine does
/// before a caret move or focus loss: the composed text goes in as-is.
fn commit_pending(wrapper: &mut BambooWrapper, left: &mut Vec<char>) {
    let pending = wrapper.output();
    if !pending.is_empty() {
        left.extend(pending.chars());
        wrapper.reset();
    }
}

fn feed(wrapper: &mut BambooWrapper, keys: &str) -> String {
    let mut left: Vec<char> = Vec::new();
    let mut right: Vec<char> = Vec::new();

    for token in tokenize(keys) {
        match token {
            Token::Char(c) => match wrapper.process_char(c) {
                out @ Outcome::Finish { .. } => {
                    left.extend(committed(wrapper, out).chars());
                    left.push(c); // the terminator (or digit itself) forwards
                }
                // a key that leaves the composition unchanged is forwarded:
                // the application appends the raw character
                Outcome::PassThrough => left.push(c),
                Outcome::Edit { .. } => {}
            },
            Token::Backspace => {
                // Mirror the engine's backspace (composition loses its last
                // output character). Known divergences from naive editing are
                // logged in docs/open-questions.md — the poison class:
                // "toool"+BS stays "tool", retype after BS corrupts marks.
                if wrapper.output().is_empty() {
                    left.pop(); // nothing composing: the app deletes a char
                } else {
                    let _ = wrapper.backspace();
                }
            }
            Token::Delete => {
                commit_pending(wrapper, &mut left);
                if !right.is_empty() {
                    right.remove(0);
                }
            }
            Token::Left => {
                if !wrapper.output().is_empty() {
                    // the engine consumes the first navigation key while a
                    // composition is pending (the commit must land before
                    // the caret moves) — mirror that: commit, no move
                    commit_pending(wrapper, &mut left);
                } else {
                    if let Some(c) = left.pop() {
                    right.insert(0, c);
                }
                }
            }
            Token::Right => {
                if !wrapper.output().is_empty() {
                    // the engine consumes the first navigation key while a
                    // composition is pending (the commit must land before
                    // the caret moves) — mirror that: commit, no move
                    commit_pending(wrapper, &mut left);
                } else {
                    if !right.is_empty() {
                    left.push(right.remove(0));
                }
                }
            }
            Token::Home => {
                if !wrapper.output().is_empty() {
                    commit_pending(wrapper, &mut left); // swallowed
                } else {
                    let taken: Vec<char> = left.drain(..).collect();
                    let mut new_right = taken;
                    new_right.extend(right);
                    right = new_right;
                }
            }
            Token::End => {
                if !wrapper.output().is_empty() {
                    commit_pending(wrapper, &mut left); // swallowed
                } else {
                    left.extend(right.drain(..));
                }
            }
        }
    }
    // a trailing composition without a terminator still shows on screen
    commit_pending(wrapper, &mut left);
    left.into_iter().collect::<String>() + &right.into_iter().collect::<String>()
}

enum Token {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

fn tokenize(text: &str) -> Vec<Token> {
    fn push_token(tokens: &mut Vec<Token>, name: &str) {
        match name {
            "backspace" => tokens.push(Token::Backspace),
            "delete" => tokens.push(Token::Delete),
            "left" => tokens.push(Token::Left),
            "right" => tokens.push(Token::Right),
            "home" => tokens.push(Token::Home),
            "end" => tokens.push(Token::End),
            _ => tokens.extend(format!("{{{}}}", name).chars().map(Token::Char)),
        }
    }
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            tokens.push(Token::Char(c));
            continue;
        }
        let mut name = String::new();
        let mut closed = false;
        for nc in chars.by_ref() {
            if nc == '}' {
                closed = true;
                break;
            }
            name.push(nc);
        }
        if closed {
            push_token(&mut tokens, &name);
        } else {
            // unterminated brace: type it out literally
            tokens.push(Token::Char('{'));
            tokens.extend(name.chars().map(Token::Char));
        }
    }
    tokens
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--check") => {
            let path = args.get(2).expect("usage: --check pairs.tsv");
            let content = std::fs::read_to_string(path).expect("cannot read pairs");
            let (mut fails, mut total) = (0, 0);
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                let (keys, want) = line.split_once('\t').expect("pairs are keys<TAB>word");
                let mut wrapper = BambooWrapper::new(Method::Telex, Default::default());
                let got = feed(&mut wrapper, keys);
                total += 1;
                if got.trim_end() != want.trim_end() {
                    fails += 1;
                    println!("FAIL {keys}\twant={want}\tgot={}", got.trim_end());
                }
            }
            println!("{total} pairs checked, {fails} mismatches");
            if fails > 0 { std::process::exit(1); }
        }
        Some(path) => {
            let keys = std::fs::read_to_string(path).expect("cannot read keys file");
            let mut wrapper = BambooWrapper::new(Method::Telex, Default::default());
            print!("{}", feed(&mut wrapper, keys.trim_end_matches('\n')));
        }
        _ => eprintln!("usage: expected-gen [--check] <file>"),
    }
}
