use bamboo_core::{Config, InputMethod, Mode};
use ibus_mangrs::bamboo_wrapper::BambooWrapper;
fn main() {
    let cfg = Config::builder()
        .free_tone_marking(true)
        .tone_style(bamboo_core::ToneStyle::Modern)
        .auto_correct(true)
        .build();
    let mut w = BambooWrapper::new(ibus_mangrs::bamboo_wrapper::Method::Telex, cfg);
    for c in "toool".chars() {
        let o = w.process_char(c);
        println!("{c}: out={:?} outcome={o:?}", w.output());
    }
    let _ = InputMethod::telex();
    let _ = Mode::Vietnamese;
}
