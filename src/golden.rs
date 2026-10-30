//! Golden behaviour tests (TODO.MD Giai đoạn 0): a table of keystroke
//! sequences and the exact text the engine must leave in the application.
//!
//! Every row runs through the real `Engine` in non-preedit mode with Unicode
//! output and is replayed against a simulated application buffer, so a
//! failure means the on-screen text would be wrong, not just the internal
//! state. Three case variants per word: lowercase, Capitalized, UPPER.

use crate::bamboo_wrapper::Method;
use crate::engine::{Action, Engine, Mode};

/// (keystrokes, expected word) for Telex.
#[cfg(test)]
const TELEX: &[(&str, &str)] = &[
    ("tieengs", "tiếng"),
    ("vieejt", "việt"),
    ("dd", "đ"),
    ("nam", "nam"),
    ("nuwocs", "nước"),
    ("hoangf", "hoàng"),
    ("khoong", "không"),
    ("cacs", "các"),
    ("truwongf", "trường"),
    ("hoar", "hỏa"),
    ("cuoocj", "cuộc"),
    ("suwowngs", "sướng"),
    ("tuooir", "tuổi"),
    ("chuyeen", "chuyên"),
    ("lamf", "làm"),
    ("dduwongf", "đường"),
    ("buooir", "buổi"),
    ("muwowif", "mười"),
    ("ddi", "đi"),
    ("anw", "ăn"),
    ("nguwowif", "người"),
    ("tooi", "tôi"),
    ("cho", "cho"),
    ("vowis", "với"),
    ("vaf", "và"),
    ("laf", "là"),
    ("cur", "củ"),
    ("ddax", "đã"),
    ("nayf", "này"),
    ("ddos", "đó"),
    ("raatj", "r\u{1ead}t"),
    ("nhieeuf", "nhiều"),
    ("trung", "trung"),
    ("quoocs", "quốc"),
    ("gioowf", "giờ"),
    ("caanf", "cần"),
    ("phair", "phải"),
    ("vowj", "vợ"),
    ("nhaf", "nhà"),
    ("song", "song"),
    ("hoawcj", "hoặc"),
    ("moojt", "một"),
    ("ddieefu", "điều"),
    ("saau", "sâu"),
    ("hanj", "hạn"),
    ("naof", "nào"),
    ("vaof", "vào"),
    ("daau", "dâu"),
    ("ddee", "đê"),
    ("ddees", "đế"),
    ("oo", "ô"),
    ("oos", "ố"),
    ("oor", "ổ"),
    ("hangf", "hàng"),
    ("saif", "sài"),
    ("goij", "gọi"),
    ("cayf", "cày"),
    ("loaji", "loại"),
    ("ddoongj", "động"),
    ("sair", "sải"),
    ("chuoojng", "chuộng"),
    ("trair", "trải"),
    ("ngayf", "ngày"),
    ("dduwocj", "được"),
    ("ddeem", "đêm"),
    ("tre", "tre"),
    ("quee", "quê"),
    ("ddaa", "đâ"),
    ("nghieemj", "nghiệm"),
    ("howji", "hợi"),
    ("mowf", "mờ"),
    ("daaus", "dấu"),
    ("ruwowuj", "rượu"),
    ("ddowji", "đợi"),
    ("bayf", "bày"),
    ("bowf", "bờ"),
    ("lawxm", "l\u{1eb5}m"),
    ("dduws", "đứ"),
    ("thuwjc", "thực"),
    ("xao", "xao"),
    ("xaof", "xào"),
    ("gaf", "gà"),
    ("gacs", "gác"),
    ("nhor", "nhỏ"),
    ("moong", "mông"),
    ("nguoofn", "nguồn"),
    ("aan", "ân"),
    ("aans", "ấn"),
    ("tieengj", "tiệng"),
    ("vieejc", "việc"),
    ("moor", "mổ"),
    ("trowif", "trời"),
    ("quen", "quen"),
    ("oong", "ông"),
    ("ooms", "ốm"),
    ("chaof", "chào"),
    ("caamf", "c\u{1ea7}m"),
    ("nhawns", "nhắn"),
    ("booif", "bồi"),
    ("ddocj", "đọc"),
    ("xong", "xong"),
    ("quyeern", "quyển"),
];

/// (keystrokes, expected word) for VNI. Digits sit right after the vowel
/// they transform (6 = â/ê/ô, 7 = ơ/ư, 8 = ă, 9 = đ; tones 1..5).
#[cfg(test)]
const VNI: &[(&str, &str)] = &[
    ("tie6ng1", "tiếng"),
    ("vie6t5", "việt"),
    ("d9", "đ"),
    ("nam", "nam"),
    ("nu7o1c", "nước"),
    ("hoa2ng", "hoàng"),
    ("kho6ng", "không"),
    ("ca1c", "các"),
    ("tru72ong", "trường"),
    ("ho3a", "hỏa"),
    ("cuo65c", "cuộc"),
    ("suo71ng", "sướng"),
    ("tuo63i", "tuổi"),
    ("chuye61n", "chuyến"),
    ("la2m", "làm"),
    ("d9u7o2ng", "đường"),
    ("buo63i", "buổi"),
    ("muo72i", "mười"),
    ("d9i", "đi"),
    ("a8n", "ăn"),
    ("ngu7o2i", "người"),
    ("to6i", "tôi"),
    ("vo71i", "với"),
    ("va2", "và"),
    ("la2", "là"),
    ("cu3", "củ"),
    ("d9a4", "đã"),
    ("na2y", "này"),
    ("d9o1", "đó"),
    ("ra65t", "r\u{1ead}t"),
    ("nhie62u", "nhiều"),
    ("trung", "trung"),
    ("quo61c", "quốc"),
    ("gio72", "giờ"),
    ("ca62n", "cần"),
    ("pha3i", "phải"),
    ("vo75", "vợ"),
    ("nha2", "nhà"),
    ("song", "song"),
    ("hoa85c", "hoặc"),
    ("mo65t", "một"),
    ("die62u", "diều"),
    ("sa6u", "sâu"),
    ("ha5n", "hạn"),
    ("na2o", "nào"),
    ("va2o", "vào"),
    ("da6u", "dâu"),
    ("d9e61", "đế"),
    ("o6", "ô"),
    ("o61", "ố"),
    ("o63", "ổ"),
    ("ha2ng", "hàng"),
    ("sa2i", "sài"),
    ("go5i", "gọi"),
    ("ca2y", "cày"),
    ("lo5ai", "loại"),
    ("d9o65ng", "động"),
    ("tu7o1ng", "tướng"),
    ("nguo62n", "nguồn"),
    ("tro72i", "trời"),
    ("bo62i", "bồi"),
    ("d9o75i", "đợi"),
    ("da61u", "dấu"),
    ("ruo75u", "rượu"),
    ("nho7", "nhơ"),
    ("nho74", "nhỡ"),
    ("mo72", "mờ"),
    ("bo72", "bờ"),
    ("d9u71", "đứ"),
    ("thu75c", "thực"),
    ("xao", "xao"),
    ("xao2", "xào"),
    ("ga2", "gà"),
    ("ga1c", "gác"),
    ("nho3", "nhỏ"),
    ("mo6ng", "mông"),
    ("a6n", "ân"),
    ("a61n", "ấn"),
    ("tie65ng", "tiệng"),
    ("vie65c", "việc"),
    ("mo63", "mổ"),
    ("que6", "quê"),
    ("d9a6", "đâ"),
    ("nghie65m", "nghiệm"),
    ("ho75i", "hợi"),
    ("chao2", "chào"),
    ("nha61n", "nhấn"),
    ("d9o5c", "đọc"),
    ("sa1ch", "sách"),
    ("o6ng", "ông"),
    ("o61m", "ốm"),
    ("quye63n", "quyển"),
    ("ma61y", "mấy"),
    ("la81m", "lắm"),
    ("sa3i", "sải"),
    ("bay2", "bày"),
];

/// Feed the keystrokes to the engine, simulating the application buffer: a
/// key whose actions contain `Forward` inserts the raw character.
#[cfg(test)]
fn typed_word(method: Method, keys: &str) -> String {
    let mut e = Engine::new(method, Mode::NonPreedit);
    // Non-relayed in-process client: golden cases exercise the protocol path.
    e.set_client(Some("gtk4-im:golden".into()));
    e.set_capabilities(0b100001); // PREEDIT_TEXT | SURROUNDING_TEXT
    let mut buf: Vec<char> = Vec::new();
    for c in keys.chars() {
        let actions = e.process_char(c);
        let forwarded = actions.contains(&Action::Forward);
        for a in &actions {
            match a {
                Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                Action::DeleteSurrounding { offset, n } => {
                    let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                    let end = (start + *n as usize).min(buf.len());
                    buf.drain(start..end);
                }
                Action::Commit(t) => buf.extend(t.chars()),
                _ => {}
            }
        }
        if forwarded {
            buf.push(c);
        }
        let text: String = buf.iter().collect();
        e.set_surrounding(text, buf.len() as u32);
    }
    buf.into_iter().collect()
}

#[cfg(test)]
fn variants(keys: &str, expected: &str) -> Vec<(String, String)> {
    let mut out = vec![(keys.to_string(), expected.to_string())];
    // Capitalized: first letter (the word itself, tone keys stay lowercase).
    let mut cap_keys = keys.to_string();
    if let Some(first) = cap_keys.chars().next() {
        cap_keys = first.to_uppercase().collect::<String>() + &cap_keys[first.len_utf8()..];
    }
    let cap_expected: String = expected
        .char_indices()
        .next()
        .map(|(i, c)| {
            let mut s = String::new();
            s.extend(c.to_uppercase());
            s + &expected[i + c.len_utf8()..]
        })
        .unwrap_or_else(|| expected.to_string());
    out.push((cap_keys, cap_expected));
    // UPPER: letters upper, tone keys keep their case-insensitive meaning.
    let upper_keys: String = keys.chars().map(|c| c.to_uppercase().next().unwrap_or(c)).collect();
    let upper_expected: String = expected.to_uppercase();
    out.push((upper_keys, upper_expected));
    out
}

#[cfg(test)]
fn run_table(method: Method, table: &[(&str, &str)]) -> Vec<String> {
    let mut failures = Vec::new();
    for (keys, expected) in table {
        for (k, want) in variants(keys, expected) {
            let got = typed_word(method, &k);
            if got != want {
                failures.push(format!("{k} -> {got:?}, expected {want:?}"));
            }
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telex_golden() {
        assert!(TELEX.len() * 3 >= 300, "need >=300 telex cases, have {}", TELEX.len() * 3);
        let failures = run_table(Method::Telex, TELEX);
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    fn vni_golden() {
        assert!(VNI.len() * 3 >= 150, "need >=150 vni cases, have {}", VNI.len() * 3);
        let failures = run_table(Method::Vni, VNI);
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }
}
