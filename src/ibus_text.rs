//! Wire encoding for IBus serializable objects.
//!
//! IBus does not use plain strings for text: every `v` argument is a
//! `GVariant` of a tuple whose first element names the C type. The layouts
//! below were captured from `ibus_serializable_serialize_object()` on the
//! installed libibus 1.5.34:
//!
//! ```text
//! IBusText      ('IBusText', @a{sv} {}, 'str', <('IBusAttrList', @a{sv} {}, @av [])>)
//! IBusAttribute ('IBusAttribute', @a{sv} {}, uint32 type, uint32 value, uint32 start, uint32 end)
//! ```
//!
//! The trailing `<...>` is a variant, so the attribute list is nested one
//! level deeper than the other fields.

use std::collections::HashMap;

use dbus::arg::{RefArg, Variant};

/// `a{sv}` with no entries — every IBus serializable starts with an empty
/// property map, so share one type alias.
pub type Props = HashMap<String, Variant<Box<dyn RefArg>>>;

/// A single `IBusAttribute`. Only `IBUS_ATTR_TYPE_UNDERLINE` (=1) is used.
#[derive(Debug, Clone, Copy)]
pub struct Attr {
    pub kind: u32,
    pub value: u32,
    pub start: u32,
    pub end: u32,
}

impl Attr {
    pub fn underline(start: u32, end: u32) -> Self {
        Self {
            kind: 1,
            value: 1,
            start,
            end,
        }
    }
}

/// Encode an `IBusText` as the `(sa{sv}sv)` tuple IBus expects inside a `v`.
pub fn text_variant(s: &str, attrs: &[Attr]) -> Variant<Box<dyn RefArg>> {
    let attr_list: Vec<Variant<Box<dyn RefArg>>> = attrs
        .iter()
        .map(|a| {
            // The type tag must be a `String`: `&'static str` implements
            // RefArg but `str` does not, and the tuple macro needs an owned
            // value here.
            Variant(Box::new((
                "IBusAttribute".to_string(),
                Props::new(),
                a.kind,
                a.value,
                a.start,
                a.end,
            )) as Box<dyn RefArg>)
        })
        .collect();

    // ('IBusAttrList', @a{sv} {}, @av [...])
    let list = ("IBusAttrList".to_string(), Props::new(), attr_list);

    // ('IBusText', @a{sv} {}, "text", <list>)
    Variant(Box::new((
        "IBusText".to_string(),
        Props::new(),
        s.to_string(),
        Variant(Box::new(list) as Box<dyn RefArg>),
    )) as Box<dyn RefArg>)
}

/// Encode an `IBusText` with no attributes.
pub fn plain_text(s: &str) -> Variant<Box<dyn RefArg>> {
    text_variant(s, &[])
}

/// Decode the string out of an `IBusText` variant received from a client
/// (`('IBusText', a{sv}, "str", <attrs>)` — field 2 is the text).
pub fn text_from_variant(v: &Variant<Box<dyn RefArg>>) -> Option<String> {
    let mut iter = v.0.as_iter()?;
    let third = iter.nth(2)?;
    third.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbus::arg::RefArg;

    fn render(v: &Variant<Box<dyn RefArg>>) -> String {
        format!("{:?}", v.0)
    }

    #[test]
    fn text_has_four_fields_with_type_tag() {
        let s = render(&plain_text("tiếng"));
        assert!(s.contains("IBusText"), "{s}");
        assert!(s.contains("tiếng"), "{s}");
        assert!(s.contains("IBusAttrList"), "{s}");
    }

    #[test]
    fn attributes_are_encoded_inside_the_list() {
        let s = render(&text_variant("abc", &[Attr::underline(0, 3)]));
        assert!(s.contains("IBusAttribute"), "{s}");
    }
}
