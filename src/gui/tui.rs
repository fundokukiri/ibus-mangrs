//! The TUI settings UI: a text-mode window (TUI) in a terminal — cursive on
//! the pure-Rust crossterm backend. The owner's bar for this alternative
//! surface is RAM, not looks: an empty GTK4 window already measured 52 MB
//! RSS on the reference machine, this whole UI measures a few megabytes.
//! Chosen per launch by `config.tui` (the switch lives in the modern window
//! and in this UI's main menu); the engine never reads that key.
//!
//! Launched without a TTY (from the panel menu or the desktop entry) it
//! opens a terminal emulator to host itself; with no terminal on PATH it
//! falls back to the modern window rather than doing nothing silently.
//!
//! Coverage matches the pre-rewrite settings window: everything the engine
//! shipped back then is editable here, later features ("Thử nghiệm" rows,
//! emoji editor, custom hotkeys…) stay modern-only. Every change is saved
//! immediately (read-modify-write on config.toml), so the engine
//! hot-reloads it within a second and options this UI never shows pass
//! through untouched.

use crate::config::{
    BackspaceToneConfig, BracketConfig, CharsetConfig, Config, MenuNode, MethodConfig, ModeConfig,
    UnicodeChannel, W2uConfig,
};
use crate::macros::{Dictionary, Macros};
use cursive::traits::*;
use cursive::views::{
    Button, Checkbox, Dialog, EditView, LinearLayout, ListView, ScrollView, SelectView, TextArea,
    TextView,
};
use cursive::Cursive;
use cursive::CursiveExt;
use std::path::PathBuf;
use std::sync::Arc;

// --- label tables: one row per choice, value carried alongside the label.

const METHODS: &[(&str, MethodConfig)] = &[
    ("Telex", MethodConfig::Telex),
    ("Telex 2", MethodConfig::Telex2),
    ("Telex W", MethodConfig::TelexW),
    ("Telex + VNI", MethodConfig::TelexVni),
    ("Telex + VNI + VIQR", MethodConfig::TelexVniViqr),
    ("VNI", MethodConfig::Vni),
    ("VNI bàn phím Pháp", MethodConfig::VniFrench),
    ("VIQR", MethodConfig::Viqr),
    ("Microsoft layout", MethodConfig::Microsoft),
];

const MODES: &[(&str, ModeConfig)] = &[
    ("Không gạch chân", ModeConfig::NonPreedit),
    ("Gạch chân (preedit)", ModeConfig::Preedit),
    ("Surrounding text", ModeConfig::Surrounding),
    ("Flow (uinput)", ModeConfig::Smooth),
    ("Turbo (uinput)", ModeConfig::SuperSmooth),
    ("Select (uinput)", ModeConfig::Select),
    ("Forward Backspace", ModeConfig::ForwardKeyEvent),
    ("Forward Selection", ModeConfig::ForwardSelect),
    ("OFF", ModeConfig::Off),
];

const CHARSETS: &[(&str, CharsetConfig)] = &[
    ("Unicode", CharsetConfig::Unicode),
    ("Unicode tổ hợp", CharsetConfig::Nfd),
    ("TCVN3", CharsetConfig::Tcvn3),
    ("VNI", CharsetConfig::Vni),
    ("VIQR", CharsetConfig::Viqr),
    ("VISCII", CharsetConfig::Viscii),
    ("VPS", CharsetConfig::Vps),
    ("Windows 1258", CharsetConfig::Cp1258),
    ("BKHCM", CharsetConfig::Bkhcm),
    ("Vietware X", CharsetConfig::VietwareX),
    ("Vietware Full", CharsetConfig::VietwareFull),
];

/// w + số: the three positions.
const ON_OFF_W2U: &[(&str, W2uConfig)] = &[
    ("Tắt", W2uConfig::Disabled),
    ("Trừ đầu từ", W2uConfig::NonStart),
    ("Mọi vị trí", W2uConfig::Everywhere),
];

/// Dấu ngoặc: the same three positions, own enum.
const ON_OFF_BRACKET: &[(&str, BracketConfig)] = &[
    ("Tắt", BracketConfig::Disabled),
    ("Trừ đầu từ", BracketConfig::NonStart),
    ("Mọi vị trí", BracketConfig::Everywhere),
];

const CHANNELS: &[(&str, UnicodeChannel)] = &[
    ("CommitText (an toàn)", UnicodeChannel::Commit),
    ("Clipboard (Ctrl+V)", UnicodeChannel::Clipboard),
];

const BACKSPACE_TONES: &[(&str, BackspaceToneConfig)] = &[
    ("Dấu trước, chữ sau", BackspaceToneConfig::ToneFirst),
    ("Xoá cả chữ có dấu", BackspaceToneConfig::WholeChar),
    ("Kiểu Unikey (undo phím)", BackspaceToneConfig::UnikeyWalk),
];

/// Label of the value currently stored in the config, by equality against
/// the table — every table holds each variant exactly once.
fn current_label<'a, T: PartialEq>(table: &'a [(&'a str, T)], value: &T) -> &'a str {
    table
        .iter()
        .find(|(_, v)| v == value)
        .map(|(l, _)| *l)
        .unwrap_or("?")
}

// --- entry point ------------------------------------------------------------

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::IsTerminal;
    // Spawned by the engine or the desktop entry there is no TTY: open a
    // terminal to host the text UI instead of dying unseen.
    if !std::io::stdout().is_terminal() {
        if spawn_in_terminal() {
            return Ok(());
        }
        // No terminal emulator anywhere on PATH: the modern window is
        // better than a silent exit.
        return super::run();
    }
    run_tui();
    Ok(())
}

/// Terminals to try, with whatever flag each one needs before the command.
/// First `Command::spawn` success wins; a missing binary fails with
/// `NotFound` and falls through to the next candidate.
fn spawn_in_terminal() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let script = format!("exec \"{}\" -gui", exe.display());
    const TERMS: &[(&str, &[&str])] = &[
        ("gnome-terminal", &["--"]),
        ("kgx", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("alacritty", &["-e"]),
        ("xterm", &["-e"]),
        ("st", &["-e"]),
        ("kitty", &[]),
    ];
    for (term, prefix) in TERMS {
        let child = std::process::Command::new(term)
            .args(*prefix)
            .args(["sh", "-c", &script])
            .spawn();
        if child.is_ok() {
            return true;
        }
    }
    false
}

fn run_tui() {
    let mut siv = Cursive::new();
    siv.set_user_data(Config::load());
    open_main(&mut siv);
    siv.run();
}

/// Read the config out of user data. `with_user_data` reports `Option`
/// because user data is optional in general — here it is set in `run_tui`
/// before any view exists, so every view can rely on it.
fn cfg<R>(siv: &mut Cursive, read: impl FnOnce(&mut Config) -> R) -> R {
    siv.with_user_data(read)
        .expect("config set at the top of run_tui")
}

/// cursive's `Dialog::info` carries no title; this titled variant doubles
/// as the standard notice box across the TUI.
fn info_box(siv: &mut Cursive, title: &str, text: impl Into<String>) {
    siv.add_layer(
        Dialog::new()
            .title(title)
            .content(TextView::new(text.into()))
            .button("OK", |s| {
                s.pop_layer();
            }),
    );
}

/// The main menu. Rebuilt (never cached) whenever a page returns: every row
/// label shows the current config value.
fn open_main(siv: &mut Cursive) {
    let enabled = cfg(siv, |c: &mut Config| c.enabled);
    let method = cfg(siv, |c: &mut Config| {
        current_label(METHODS, &c.method).to_string()
    });
    let mode = cfg(siv, |c: &mut Config| {
        current_label(MODES, &c.mode).to_string()
    });
    let charset = cfg(siv, |c: &mut Config| {
        current_label(CHARSETS, &c.charset).to_string()
    });

    let mut list = ListView::new();
    list.add_child(
        "",
        Button::new("Thoát", |s| {
            s.quit();
        }),
    );
    list.add_child("Bộ gõ", toggle_row(enabled, |c, v| c.enabled = v));
    list.add_child(
        "Kiểu gõ",
        Button::new(method, move |s| {
            let current = cfg(s, |c: &mut Config| c.method);
            open_choice(s, METHODS, current, |c, v| c.method = v);
        }),
    );
    list.add_child(
        "Chế độ gõ",
        Button::new(mode, move |s| {
            let current = cfg(s, |c: &mut Config| c.mode);
            open_choice(s, MODES, current, |c, v| c.mode = v);
        }),
    );
    list.add_child(
        "Bảng mã",
        Button::new(charset, move |s| {
            let current = cfg(s, |c: &mut Config| c.charset);
            open_choice(s, CHARSETS, current, |c, v| c.charset = v);
        }),
    );
    list.add_child("", Button::new("Dấu & chính tả…", open_tone));
    list.add_child("", Button::new("Gõ tắt & tự động…", open_auto));
    list.add_child("", Button::new("Phím tắt…", open_hotkeys));
    list.add_child("", Button::new("Luật theo app…", open_rules));
    list.add_child("", Button::new("Menu panel…", open_menu_editor));
    list.add_child("", Button::new("Tinh chỉnh…", open_tune));
    list.add_child("", Button::new("Bảng gõ tắt…", open_macros));
    list.add_child("", Button::new("Từ điển…", open_dictionary));
    list.add_child("", Button::new("Sao lưu…", open_backup));
    list.add_child("", Button::new("Giới thiệu…", open_about));
    list.add_child(
        "",
        Button::new(
            "Quay lại cửa sổ hiện đại (tắt giao diện TUI)",
            |s| switch_to_modern(s),
        ),
    );

    siv.add_layer(
        Dialog::new()
            .title("MăngRS cài đặt (TUI)")
            .content(ScrollView::new(list))
            .fixed_width(52),
    );
}

fn switch_to_modern(siv: &mut Cursive) {
    cfg(siv, |c: &mut Config| c.tui = false);
    persist(siv);
    info_box(
        siv,
        "Đã tắt giao diện TUI",
        "Lần mở cài đặt sau sẽ là cửa sổ hiện đại (Libadwaita).",
    );
}

// --- generic row builders ---------------------------------------------------

fn toggle_row(
    initial: bool,
    set: impl Fn(&mut Config, bool) + Copy + Send + Sync + 'static,
) -> impl View {
    // checked() BEFORE on_change: the builder path fires no callback.
    let mut checkbox = Checkbox::new();
    if initial {
        checkbox = checkbox.checked();
    }
    checkbox.on_change(move |siv, on| {
        cfg(siv, |c: &mut Config| set(c, on));
        persist(siv);
    })
}

/// A full-screen select over `table`; picking an entry applies and saves.
/// The entry matching `current` starts highlighted; browsing without Enter
/// never writes.
fn open_choice<T>(
    siv: &mut Cursive,
    table: &'static [(&'static str, T)],
    current: T,
    apply: fn(&mut Config, T),
) where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    let mut select = SelectView::<T>::new();
    for (label, value) in table {
        select.add_item(*label, *value);
    }
    if let Some(i) = table.iter().position(|(_, v)| *v == current) {
        select.set_selection(i);
    }
    siv.add_layer(
        Dialog::new()
            .title("Chọn")
            .content(ScrollView::new(select.on_submit(move |s, v| {
                cfg(s, |c: &mut Config| apply(c, *v));
                persist(s);
                reopen_main(s);
            })))
            .button("Hủy", |s| {
                s.pop_layer();
            }),
    );
}

fn persist(siv: &mut Cursive) {
    let snapshot = cfg(siv, |c: &mut Config| c.clone());
    if let Err(e) = snapshot.save_fresh() {
        info_box(siv, "Không lưu được", e.to_string());
    }
}

fn text_row(
    label: String,
    value: String,
    apply: impl Fn(&mut Config, String) + Copy + Send + Sync + 'static,
) -> impl View {
    Button::new(label, move |s| {
        // A fresh clone per open: the dialog's Fn callbacks may run many
        // times and cannot consume what they capture.
        let value = value.clone();
        s.add_layer(
            Dialog::new()
                .title("Sửa giá trị")
                .content(
                    EditView::new()
                        .content(value.clone())
                        .with_name("tui_edit_value")
                        .fixed_width(40),
                )
                .button("Lưu", move |s| {
                    let text = s
                        .call_on_name("tui_edit_value", |v: &mut EditView| {
                            v.get_content().trim().to_string()
                        })
                        .unwrap_or_default();
                    s.with_user_data(move |c: &mut Config| apply(c, text));
                    persist(s);
                    reopen_main(s);
                })
                .button("Hủy", |s| {
                    s.pop_layer();
                }),
        );
    })
}

fn number_row(
    label: String,
    value: u64,
    apply: impl Fn(&mut Config, u64) + Copy + Send + Sync + 'static,
) -> impl View {
    Button::new(format!("{label}: {value}"), move |s| {
        s.add_layer(
            Dialog::new()
                .title("Sửa số (ms)")
                .content(
                    EditView::new()
                        .content(value.to_string())
                        .with_name("tui_edit_number")
                        .fixed_width(12),
                )
                .button("Lưu", move |s| {
                    let text = s
                        .call_on_name("tui_edit_number", |v: &mut EditView| {
                            v.get_content().trim().to_string()
                        })
                        .unwrap_or_default();
                    match text.parse::<u64>() {
                        Ok(n) => {
                            s.with_user_data(move |c: &mut Config| apply(c, n));
                            persist(s);
                            reopen_main(s);
                        }
                        Err(_) => {
                            info_box(s, "Lỗi", "Cần một số nguyên (ms).");
                        }
                    }
                })
                .button("Hủy", |s| {
                    s.pop_layer();
                }),
        );
    })
}

/// Rebuild the main menu so its rows show the new state. Any page layers
/// still open are stale by design — Esc returns to the fresh main menu.
fn reopen_main(siv: &mut Cursive) {
    while siv.screen().len() > 1 {
        siv.pop_layer();
    }
    open_main(siv);
}

fn reopen_page(siv: &mut Cursive, page: fn(&mut Cursive)) {
    reopen_main(siv);
    page(siv);
}

// --- pages ------------------------------------------------------------------

fn open_tone(siv: &mut Cursive) {
    let (modern, free, correct, restore, abbrev, tone) = cfg(siv, |c: &mut Config| {
        (
            c.modern_tone_style,
            c.free_tone_marking,
            c.auto_correct,
            c.auto_restore,
            c.abbreviations,
            current_label(BACKSPACE_TONES, &c.backspace_tone).to_string(),
        )
    });
    let mut list = ListView::new();
    list.add_child(
        "Kiểu dấu hiện đại",
        toggle_row(modern, |c, v| c.modern_tone_style = v),
    );
    list.add_child(
        "Dấu tự do",
        toggle_row(free, |c, v| c.free_tone_marking = v),
    );
    list.add_child(
        "Tự sửa lỗi chính tả",
        toggle_row(correct, |c, v| c.auto_correct = v),
    );
    list.add_child(
        "Tự khôi phục dấu",
        toggle_row(restore, |c, v| c.auto_restore = v),
    );
    list.add_child(
        "Viết tắt (ddc, đk…)",
        toggle_row(abbrev, |c, v| c.abbreviations = v),
    );
    list.add_child(
        "Backspace gặp từ có dấu",
        Button::new(tone, move |s| {
            let current = cfg(s, |c: &mut Config| c.backspace_tone);
            open_choice(s, BACKSPACE_TONES, current, |c, v| c.backspace_tone = v);
        }),
    );
    siv.add_layer(
        Dialog::new()
            .title("Dấu & chính tả")
            .content(ScrollView::new(list))
            .button("Quay lại", reopen_main)
            .fixed_width(56),
    );
}

fn open_auto(siv: &mut Cursive) {
    let (bypass, em_dash, cap, double_space, w2u, bracket, time_format, date_format) =
        cfg(siv, |c: &mut Config| {
            (
                c.bypass_url,
                c.em_dash,
                c.auto_capitalize,
                c.double_space_period,
                current_label(ON_OFF_W2U, &c.w2u).to_string(),
                current_label(ON_OFF_BRACKET, &c.bracket).to_string(),
                c.time_format.clone(),
                c.date_format.clone(),
            )
        });
    let mut list = ListView::new();
    list.add_child("Né URL khi gõ", toggle_row(bypass, |c, v| c.bypass_url = v));
    list.add_child(
        "Gạch ngang em-dash",
        toggle_row(em_dash, |c, v| c.em_dash = v),
    );
    list.add_child(
        "Viết hoa đầu câu",
        toggle_row(cap, |c, v| c.auto_capitalize = v),
    );
    list.add_child(
        "Hai dấu cách = chấm",
        toggle_row(double_space, |c, v| c.double_space_period = v),
    );
    list.add_child(
        "w + số",
        Button::new(w2u, move |s| {
            let current = cfg(s, |c: &mut Config| c.w2u);
            open_choice(s, ON_OFF_W2U, current, |c, v| c.w2u = v);
        }),
    );
    list.add_child(
        "Dấu ngoặc",
        Button::new(bracket, move |s| {
            let current = cfg(s, |c: &mut Config| c.bracket);
            open_choice(s, ON_OFF_BRACKET, current, |c, v| c.bracket = v);
        }),
    );
    list.add_child(
        "Định dạng giờ",
        text_row("Định dạng giờ".into(), time_format, |c, v| {
            c.time_format = v
        }),
    );
    list.add_child(
        "Định dạng ngày",
        text_row("Định dạng ngày".into(), date_format, |c, v| {
            c.date_format = v
        }),
    );
    siv.add_layer(
        Dialog::new()
            .title("Gõ tắt & tự động")
            .content(ScrollView::new(list))
            .button("Quay lại", reopen_main)
            .fixed_width(56),
    );
}

fn open_hotkeys(siv: &mut Cursive) {
    let (cycle, enable) = cfg(siv, |c: &mut Config| {
        (c.cycle_mode_hotkey.clone(), c.enabled_hotkey.clone())
    });
    let mut list = ListView::new();
    list.add_child(
        "Xoay chế độ gõ",
        text_row("Xoay chế độ gõ".into(), cycle, |c, v| {
            c.cycle_mode_hotkey = v
        }),
    );
    list.add_child(
        "Bật/tắt bộ gõ",
        text_row("Bật/tắt bộ gõ".into(), enable, |c, v| {
            c.enabled_hotkey = v
        }),
    );
    list.add_child(
        "",
        TextView::new("Định dạng: Ctrl+Alt+v, Shift+Space… (để trống = tắt)"),
    );
    siv.add_layer(
        Dialog::new()
            .title("Phím tắt")
            .content(list)
            .button("Quay lại", reopen_main)
            .fixed_width(56),
    );
}

fn open_tune(siv: &mut Cursive) {
    let (commit, backspace, post, channel, x11, xim, debug) = cfg(siv, |c: &mut Config| {
        (
            c.commit_interval_ms,
            c.backspace_interval_ms,
            c.post_delay_ms,
            current_label(CHANNELS, &c.unicode_channel).to_string(),
            c.x11_delete_without_report,
            c.preedit_for_xim,
            c.debug_log,
        )
    });
    let mut list = ListView::new();
    list.add_child(
        "Xả commit (ms)",
        number_row("Xả commit (ms)".into(), commit, |c, v| {
            c.commit_interval_ms = v
        }),
    );
    list.add_child(
        "Xả backspace (ms)",
        number_row("Xả backspace (ms)".into(), backspace, |c, v| {
            c.backspace_interval_ms = v
        }),
    );
    list.add_child(
        "Trễ sau phím (ms)",
        number_row("Trễ sau phím (ms)".into(), post, |c, v| {
            c.post_delay_ms = v
        }),
    );
    list.add_child(
        "Kênh Unicode",
        Button::new(channel, move |s| {
            let current = cfg(s, |c: &mut Config| c.unicode_channel);
            open_choice(s, CHANNELS, current, |c, v| c.unicode_channel = v);
        }),
    );
    list.add_child(
        "Xoá đuôi ở app X11",
        toggle_row(x11, |c, v| c.x11_delete_without_report = v),
    );
    list.add_child(
        "Preedit cho XIM",
        toggle_row(xim, |c, v| c.preedit_for_xim = v),
    );
    list.add_child("Ghi log gỡ lỗi", toggle_row(debug, |c, v| c.debug_log = v));
    siv.add_layer(
        Dialog::new()
            .title("Tinh chỉnh")
            .content(ScrollView::new(list))
            .button("Quay lại", reopen_main)
            .fixed_width(56),
    );
}

// --- macro & dictionary text editors ---------------------------------------

fn open_macros(siv: &mut Cursive) {
    let text = Macros::load()
        .entries()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    open_text_editor(
        siv,
        "Bảng gõ tắt (name=phrase, mỗi dòng một cặp)",
        text,
        |text| {
            let pairs: Vec<(String, String)> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter_map(|l| {
                    let (k, v) = l.split_once('=')?;
                    let (k, v) = (k.trim(), v.trim());
                    (!k.is_empty() && !v.is_empty()).then(|| (k.to_string(), v.to_string()))
                })
                .collect();
            let mut macros = Macros::load();
            macros.set_entries(pairs.into_iter());
            macros.save().map_err(|e| e.to_string())
        },
    );
}

fn open_dictionary(siv: &mut Cursive) {
    let text = Dictionary::load().words().collect::<Vec<_>>().join("\n");
    open_text_editor(siv, "Từ điển (mỗi dòng một từ)", text, |text| {
        let words: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        let mut dict = Dictionary::load();
        dict.set_words(words.into_iter());
        dict.save().map_err(|e| e.to_string())
    });
}

/// One full-screen editor: the textarea plus [Lưu]/[Hủy]. `save` receives
/// the whole text and does its own parsing + file write; an Err keeps the
/// editor open with the message on top.
fn open_text_editor(
    siv: &mut Cursive,
    title: &str,
    text: String,
    save: impl Fn(&str) -> Result<(), String> + Copy + Send + Sync + 'static,
) {
    siv.add_layer(
        Dialog::new()
            .title(title)
            .content(
                TextArea::new()
                    .content(text)
                    .with_name("tui_text_editor")
                    .min_size((50, 16)),
            )
            .button("Lưu", move |s| {
                let text = s
                    .call_on_name("tui_text_editor", |v: &mut TextArea| {
                        v.get_content().to_string()
                    })
                    .unwrap_or_default();
                match save(&text) {
                    Ok(()) => {
                        s.pop_layer();
                    }
                    Err(e) => {
                        info_box(s, "Không lưu được", e);
                    }
                }
            })
            .button("Hủy", |s| {
                s.pop_layer();
            }),
    );
}

// --- app rules --------------------------------------------------------------

fn open_rules(siv: &mut Cursive) {
    let rules = cfg(siv, |c: &mut Config| c.app_rules.clone());
    let mut list = ListView::new();
    if rules.is_empty() {
        list.add_child("", TextView::new("(chưa có luật nào)"));
    }
    for (i, rule) in rules.iter().enumerate() {
        let label = format!("{} → {}", rule.pattern, current_label(MODES, &rule.mode));
        list.add_child(
            "",
            LinearLayout::horizontal()
                .child(TextView::new(label))
                .child(Button::new("Sửa", move |s| edit_rule_at(s, i)))
                .child(Button::new("Xóa", move |s| {
                    cfg(s, |c: &mut Config| {
                        c.app_rules.remove(i);
                    });
                    persist(s);
                    reopen_page(s, open_rules);
                })),
        );
    }
    list.add_child("", Button::new("Thêm luật", |s| edit_rule_dialog(s, None)));
    siv.add_layer(
        Dialog::new()
            .title("Luật theo app (pattern → chế độ)")
            .content(ScrollView::new(list))
            .button("Quay lại", reopen_main)
            .fixed_width(60),
    );
}

fn edit_rule_at(siv: &mut Cursive, index: usize) {
    let rule = cfg(siv, |c: &mut Config| {
        c.app_rules.get(index).map(|r| (r.pattern.clone(), r.mode))
    });
    if let Some((pattern, mode)) = rule {
        edit_rule_dialog(siv, Some((index, pattern, mode)));
    }
}

/// One dialog to add or edit a rule. `None` adds; `Some((index, ...))`
/// rewrites that row.
fn edit_rule_dialog(siv: &mut Cursive, target: Option<(usize, String, ModeConfig)>) {
    let (pattern, mode) = target
        .as_ref()
        .map(|(_, p, m)| (p.clone(), *m))
        .unwrap_or_else(|| (String::new(), ModeConfig::Preedit));
    let mut mode_select: SelectView<ModeConfig> = SelectView::new();
    for (label, value) in MODES {
        mode_select.add_item(*label, *value);
    }
    if let Some(i) = MODES.iter().position(|(_, m)| *m == mode) {
        mode_select.set_selection(i);
    }
    siv.add_layer(
        Dialog::new()
            .title(if target.is_some() {
                "Sửa luật"
            } else {
                "Thêm luật"
            })
            .content(
                LinearLayout::vertical()
                    .child(TextView::new("Tên app (wm_class, wayland:app hoặc xim)"))
                    .child(
                        EditView::new()
                            .content(pattern)
                            .with_name("tui_rule_pattern")
                            .fixed_width(40),
                    )
                    .child(TextView::new("Chế độ gõ (mũi tên, Enter để chọn)"))
                    .child(mode_select.with_name("tui_rule_mode")),
            )
            .button("Lưu", move |s| {
                let pattern = s
                    .call_on_name("tui_rule_pattern", |v: &mut EditView| {
                        v.get_content().trim().to_string()
                    })
                    .unwrap_or_default();
                if pattern.is_empty() {
                    info_box(s, "Lỗi", "Tên app không được trống.");
                    return;
                }
                let Some(mode) = s
                    .call_on_name("tui_rule_mode", |v: &mut SelectView<ModeConfig>| {
                        v.selection().map(|m| *m)
                    })
                    .flatten()
                else {
                    return;
                };
                cfg(s, |c: &mut Config| match &target {
                    Some((i, _, _)) => {
                        if let Some(rule) = c.app_rules.get_mut(*i) {
                            rule.pattern = pattern.clone();
                            rule.mode = mode;
                        }
                    }
                    None => c.app_rules.push(crate::config::AppRule { pattern, mode }),
                });
                persist(s);
                s.pop_layer();
                reopen_page(s, open_rules);
            })
            .button("Hủy", |s| {
                s.pop_layer();
            }),
    );
}

// --- panel menu editor ------------------------------------------------------

/// One-line description of a menu node, for the editor list.
fn node_label(node: &MenuNode) -> String {
    match node {
        MenuNode::Toggle { key } => format!("Bật/tắt: {key}"),
        MenuNode::Radios { group, children } => {
            format!("Nhóm chọn {}: {} mục", group, children.len())
        }
        MenuNode::Action { key } => format!("Hành động: {key}"),
        MenuNode::Separator => "--- đường phân cách ---".to_string(),
        MenuNode::Folder { label, children } => {
            format!("Thư mục {}: {} mục", label, children.len())
        }
    }
}

/// The editable design vector — seeds a `None` menu with the shipped shape
/// so the first edit keeps it, and an emptied design still synthesizes.
fn menu_design_mut(config: &mut Config) -> &mut Vec<MenuNode> {
    if config.menu.is_none() {
        config.menu = Some(config.default_menu());
    }
    match config.menu.as_mut() {
        Some(design) => design,
        // Just set above; the compiler cannot see it.
        None => unreachable!("menu seeded one line above"),
    }
}

fn open_menu_editor(siv: &mut Cursive) {
    let nodes = cfg(siv, |c: &mut Config| {
        c.menu.clone().unwrap_or_else(|| c.default_menu())
    });
    let mut list = ListView::new();
    for (i, node) in nodes.iter().enumerate() {
        list.add_child(
            "",
            LinearLayout::horizontal()
                .child(TextView::new(node_label(node)))
                .child(Button::new("Lên", move |s| move_menu_node(s, i, -1)))
                .child(Button::new("Xuống", move |s| move_menu_node(s, i, 1)))
                .child(Button::new("Sửa", move |s| edit_menu_node(s, Some(i))))
                .child(Button::new("Xóa", move |s| {
                    cfg(s, |c: &mut Config| {
                        let design = menu_design_mut(c);
                        if design.len() > 1 {
                            design.remove(i);
                        }
                    });
                    persist(s);
                    reopen_page(s, open_menu_editor);
                })),
        );
    }
    list.add_child("", Button::new("Thêm mục", |s| edit_menu_node(s, None)));
    siv.add_layer(
        Dialog::new()
            .title("Menu panel (thiết kế)")
            .content(ScrollView::new(list))
            .button("Quay lại", reopen_main)
            .fixed_width(64),
    );
}

fn move_menu_node(siv: &mut Cursive, index: usize, delta: i32) {
    cfg(siv, |c: &mut Config| {
        let design = menu_design_mut(c);
        let to = index as i64 + delta as i64;
        if to >= 0 && (to as usize) < design.len() {
            design.swap(index, to as usize);
        }
    });
    persist(siv);
    reopen_page(siv, open_menu_editor);
}

fn edit_menu_node(siv: &mut Cursive, index: Option<usize>) {
    let node = index.and_then(|i| {
        cfg(siv, |c: &mut Config| {
            c.menu
                .clone()
                .unwrap_or_else(|| c.default_menu())
                .get(i)
                .cloned()
        })
    });
    let heading = match &node {
        Some(n) => format!("Sửa mục: {}", node_label(n)),
        None => "Thêm mục — chọn loại".to_string(),
    };
    let mut select: SelectView<u8> = SelectView::new();
    select.add_item("Bật/tắt", 0);
    select.add_item("Nhóm chọn (display/charset/method)", 1);
    select.add_item("Hành động", 2);
    select.add_item("Đường phân cách", 3);
    select.add_item("Thư mục", 4);
    siv.add_layer(
        Dialog::new()
            .title(heading)
            .content(ScrollView::new(select.on_submit(move |s, kind| {
                edit_menu_node_of_kind(s, index, *kind, node.clone());
            })))
            .button("Hủy", |s| {
                s.pop_layer();
            }),
    );
}

/// Replace (or insert) the node at `index` with a freshly edited one of
/// `kind`, asking for its parameters in a follow-up dialog. Editing keeps
/// what the node already held where the kind allows it.
fn edit_menu_node_of_kind(
    siv: &mut Cursive,
    index: Option<usize>,
    kind: u8,
    old: Option<MenuNode>,
) {
    match kind {
        0 => {
            // Toggle: pick the key from the panel's known quick toggles.
            let mut select: SelectView<String> = SelectView::new();
            select.add_item("enabled (bật/tắt bộ gõ)", "enabled".to_string());
            for toggle in crate::ibus_prop::QUICK_TOGGLES {
                select.add_item(
                    format!("{} ({})", toggle.key, toggle.label),
                    toggle.key.to_string(),
                );
            }
            open_pick_string(
                siv,
                "Chọn khóa bật/tắt",
                select,
                index,
                Arc::new(|key| MenuNode::Toggle { key }),
            );
        }
        1 => {
            let group = match &old {
                Some(MenuNode::Radios { group, .. }) => group.clone(),
                _ => "display".to_string(),
            };
            let children = match old {
                Some(MenuNode::Radios { children, .. }) => children,
                _ => Vec::new(),
            };
            let mut select: SelectView<String> = SelectView::new();
            for g in ["display", "charset", "method"] {
                select.add_item(g, g.to_string());
            }
            if let Some(i) = ["display", "charset", "method"]
                .iter()
                .position(|g| *g == group)
            {
                select.set_selection(i);
            }
            open_pick_string(
                siv,
                "Chọn nhóm",
                select,
                index,
                Arc::new(move |group| MenuNode::Radios {
                    group,
                    children: children.clone(),
                }),
            );
        }
        2 => {
            let mut select: SelectView<String> = SelectView::new();
            for key in ["macro_editor", "settings", "about"] {
                select.add_item(key, key.to_string());
            }
            open_pick_string(
                siv,
                "Chọn hành động",
                select,
                index,
                Arc::new(|key| MenuNode::Action { key }),
            );
        }
        3 => apply_menu_node(siv, index, MenuNode::Separator),
        _ => {
            let (label, children) = match old {
                Some(MenuNode::Folder { label, children }) => (label, children),
                _ => ("Menu con".to_string(), Vec::new()),
            };
            siv.pop_layer(); // close the type picker
            siv.add_layer(
                Dialog::new()
                    .title("Tên thư mục")
                    .content(
                        EditView::new()
                            .content(label)
                            .with_name("tui_folder_label")
                            .fixed_width(40),
                    )
                    .button("Lưu", move |s| {
                        let label = s
                            .call_on_name("tui_folder_label", |v: &mut EditView| {
                                v.get_content().trim().to_string()
                            })
                            .unwrap_or_default();
                        if label.is_empty() {
                            info_box(s, "Lỗi", "Tên không được trống.");
                            return;
                        }
                        apply_menu_node(
                            s,
                            index,
                            MenuNode::Folder {
                                label,
                                children: children.clone(),
                            },
                        );
                    })
                    .button("Hủy", |s| {
                        s.pop_layer();
                    }),
            );
        }
    }
}

/// Shared tail of the menu-node editors: swap the node in (or push), save,
/// rebuild the editor page.
fn apply_menu_node(siv: &mut Cursive, index: Option<usize>, node: MenuNode) {
    cfg(siv, |c: &mut Config| {
        let design = menu_design_mut(c);
        match index {
            Some(i) if i < design.len() => design[i] = node,
            _ => design.push(node),
        }
    });
    persist(siv);
    reopen_page(siv, open_menu_editor);
}

/// Show a select of strings; the picked value builds a node via `build`.
fn open_pick_string(
    siv: &mut Cursive,
    title: &str,
    select: SelectView<String>,
    index: Option<usize>,
    build: Arc<dyn Fn(String) -> MenuNode + Send + Sync>,
) {
    siv.add_layer(
        Dialog::new()
            .title(title)
            .content(ScrollView::new(select.on_submit(move |s, key: &String| {
                let node = build(key.clone());
                apply_menu_node(s, index, node);
            })))
            .button("Hủy", |s| {
                s.pop_layer();
            }),
    );
}

// --- backup -----------------------------------------------------------------

fn open_backup(siv: &mut Cursive) {
    let mut list = ListView::new();
    list.add_child("", Button::new("Xuất cấu hình ra JSON…", backup_export));
    list.add_child("", Button::new("Nhập cấu hình từ JSON…", backup_import));
    list.add_child(
        "",
        Button::new("Khôi phục mặc định…", |s| {
            s.add_layer(
                Dialog::new()
                    .title("Khôi phục mặc định?")
                    .content(TextView::new(
                        "Cấu hình về mặc định. Gõ tắt và từ điển giữ nguyên.",
                    ))
                    .button("Khôi phục", |s| {
                        if let Err(e) = Config::default().save_fresh() {
                            info_box(s, "Không ghi được", e.to_string());
                            return;
                        }
                        s.set_user_data(Config::load());
                        reopen_main(s);
                    })
                    .button("Hủy", |s| {
                        s.pop_layer();
                    }),
            );
        }),
    );
    siv.add_layer(
        Dialog::new()
            .title("Sao lưu")
            .content(list)
            .button("Quay lại", reopen_main)
            .fixed_width(52),
    );
}

/// `~/x` -> $HOME/x; anything else passes through untouched.
fn shellexpand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn backup_path_dialog(
    siv: &mut Cursive,
    title: &str,
    default_path: &str,
    go: impl Fn(&mut Cursive, PathBuf) + Copy + Send + Sync + 'static,
) {
    siv.add_layer(
        Dialog::new()
            .title(title)
            .content(
                EditView::new()
                    .content(default_path)
                    .with_name("tui_backup_path")
                    .fixed_width(46),
            )
            .button("Đồng ý", move |s| {
                let path = s
                    .call_on_name("tui_backup_path", |v: &mut EditView| {
                        v.get_content().trim().to_string()
                    })
                    .unwrap_or_default();
                let path = shellexpand_home(&path);
                s.pop_layer();
                go(s, path);
            })
            .button("Hủy", |s| {
                s.pop_layer();
            }),
    );
}

fn backup_export(siv: &mut Cursive) {
    let default_path = dirs::document_dir()
        .or_else(dirs::home_dir)
        .map(|d| d.join("mangrs-config.json").display().to_string())
        .unwrap_or_else(|| "mangrs-config.json".to_string());
    backup_path_dialog(siv, "Xuất ra tệp", &default_path, |s, path| {
        let text = crate::backup::to_json(
            Config::load(),
            Macros::load()
                .entries()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            Dictionary::load().words().map(str::to_string).collect(),
            crate::emoji::UserEmoji::load()
                .entries()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        );
        match text.and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
            Ok(()) => info_box(s, "Đã xuất", path.display().to_string()),
            Err(e) => info_box(s, "Không xuất được", e),
        }
    });
}

fn backup_import(siv: &mut Cursive) {
    backup_path_dialog(siv, "Nhập từ tệp", "mangrs-config.json", |s, path| {
        let loaded = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| crate::backup::from_json(&text));
        match loaded {
            Err(message) => info_box(s, "Không nhập được", message),
            Ok(backup) => {
                // Same order as the modern import: nothing is written
                // before the file parses; a version-1 file leaves emoji
                // untouched.
                if let Err(e) = backup.config.save() {
                    info_box(s, "Không ghi được", e.to_string());
                    return;
                }
                let mut macros = Macros::load();
                macros.set_entries(backup.macros.into_iter());
                macros.save().ok();
                let mut dict = Dictionary::load();
                dict.set_words(backup.dictionary.into_iter());
                dict.save().ok();
                if backup.version >= 2 {
                    let mut emoji = crate::emoji::UserEmoji::load();
                    emoji.set_entries(backup.emoji.into_iter());
                    emoji.save().ok();
                }
                s.set_user_data(Config::load());
                reopen_main(s);
                info_box(s, "Đã nhập", "Cấu hình đã được khôi phục.");
            }
        }
    });
}

// --- about ------------------------------------------------------------------

fn open_about(siv: &mut Cursive) {
    let text = format!(
        "MăngRS (ibus-mangrs) v{}\n\nBộ gõ tiếng Việt cho IBus, xây trên bamboo-core.\n\
         Giao diện TUI dạng chữ: siêu tiết kiệm RAM.\n\n\
         Cửa sổ hiện đại (Libadwaita) có thêm các mục mới nhất:\n\
         tab Thử nghiệm, trình soạn emoji, phím tắt tùy chỉnh…\n\n\
         Licence: GPL-3.0",
        env!("CARGO_PKG_VERSION")
    );
    siv.add_layer(
        Dialog::new()
            .title("Giới thiệu")
            .content(TextView::new(text))
            .button("Quay lại", |s| {
                s.pop_layer();
            }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_tables_hold_each_variant_exactly_once() {
        let mut modes: Vec<ModeConfig> = Vec::new();
        for (_, v) in MODES {
            assert!(!modes.contains(v), "duplicate mode entry");
            modes.push(*v);
        }
        assert_eq!(modes.len(), 9);
        let mut methods: Vec<MethodConfig> = Vec::new();
        for (_, v) in METHODS {
            assert!(!methods.contains(v), "duplicate method entry");
            methods.push(*v);
        }
        let mut charsets: Vec<CharsetConfig> = Vec::new();
        for (_, v) in CHARSETS {
            assert!(!charsets.contains(v), "duplicate charset entry");
            charsets.push(*v);
        }
    }

    #[test]
    fn current_label_finds_every_table_entry() {
        for (label, value) in MODES {
            assert_eq!(current_label(MODES, value), *label);
        }
        assert_eq!(
            current_label(BACKSPACE_TONES, &BackspaceToneConfig::UnikeyWalk),
            "Kiểu Unikey (undo phím)"
        );
    }

    #[test]
    fn shellexpand_home_only_touches_tilde() {
        let home = dirs::home_dir().unwrap().display().to_string();
        assert_eq!(
            shellexpand_home("~/a.json"),
            PathBuf::from(format!("{home}/a.json"))
        );
        assert_eq!(
            shellexpand_home("/tmp/a.json"),
            PathBuf::from("/tmp/a.json")
        );
        assert_eq!(shellexpand_home("a.json"), PathBuf::from("a.json"));
    }

    #[test]
    fn node_label_describes_every_variant() {
        assert_eq!(
            node_label(&MenuNode::Toggle {
                key: "enabled".into()
            }),
            "Bật/tắt: enabled"
        );
        assert_eq!(
            node_label(&MenuNode::Radios {
                group: "display".into(),
                children: vec![]
            }),
            "Nhóm chọn display: 0 mục"
        );
        assert_eq!(
            node_label(&MenuNode::Action {
                key: "about".into()
            }),
            "Hành động: about"
        );
        assert_eq!(node_label(&MenuNode::Separator), "--- đường phân cách ---");
        assert_eq!(
            node_label(&MenuNode::Folder {
                label: "Cá nhân".into(),
                children: vec![]
            }),
            "Thư mục Cá nhân: 0 mục"
        );
    }
}
