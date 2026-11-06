use bamboo_core::{Engine as B, InputMethod, Mode as M};
#[test]
fn probe_digit_bamboo() {
    let mut e = B::new(InputMethod::telex());
    e.process_key('x', M::Vietnamese);
    {
        let mut e4 = B::new(InputMethod::telex());
        for c in "loixf".chars() {
            let rep = {
                let d = e4.process_key_edit(c, M::Vietnamese);
                (d.delete_chars, d.insert.chars().collect::<String>())
            };
            let out = e4.output_str().to_string();
            println!("loixf {c:?}: del={} ins={:?} out={out:?}", rep.0, rep.1);
        }
    }
    for s in ["tool", "toool"] {
        let mut e3 = B::new(InputMethod::telex());
        let o = e3.process_str(s, M::Vietnamese);
        println!("process_str({s:?}) = {o:?}");
    }
    for c in ['1', '2', '0', '9', '"', '\''] {
        let mut e2 = B::new(InputMethod::telex());
        e2.process_key('x', M::Vietnamese);
        let rep = {
            let d = e2.process_key_edit(c, M::Vietnamese);
            (d.delete_chars, d.insert.chars().collect::<String>())
        };
        let out = e2.output_str().to_string();
        println!("x+{c:?}: del={} ins={:?} out={out:?}", rep.0, rep.1);
    }
}
