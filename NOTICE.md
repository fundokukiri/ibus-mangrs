# Third-party Licenses

MăngRS is MIT-licensed. It depends on the following crates; their licenses are reproduced or referenced here as required.

## bamboo-core 0.5.3 — MIT

Copyright (C) 2018 Luong Thanh Lam <ltlam93@gmail.com>
Copyright (C) 2024 nguien <nguyen10t2lhp@gmail.com>

The Vietnamese input logic (Telex/VNI/VIQR parsing, tone placement, syllable validation) comes from `bamboo-core`. Its full MIT text is in the crate (`LICENSE` in the `bamboo-core` package). MăngRS does not copy source from `ibus-bamboo` (GPLv3); it only links this MIT library.

## Other Dependencies

All permissive; see `Cargo.lock` and each crate's `license` field:

- `dbus` (MIT/Apache-2.0)
- `dirs` (MIT/Apache-2.0)
- `serde` (MIT/Apache-2.0)
- `toml` (MIT/Apache-2.0)
- `gtk4` (MIT) — optional, `gui` feature only