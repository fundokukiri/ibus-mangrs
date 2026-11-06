use super::*;
#[test]
fn debug_digit_after_char() {
    for keys in ["x1", "to1", "a1", "x 1"] {
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        let mut out = Vec::new();
        for c in keys.chars() {
            let acts = e.process_char(c);
            out.push(format!("{c}:{acts:?}"));
        }
        println!("{keys:?} => {}", out.join(" "));
    }
}
