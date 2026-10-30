# Third-party Licenses

MăngRS is MIT-licensed. It depends on the following crates; their licenses are reproduced or referenced here as required.

## bamboo-core 0.5.3 — MIT

**Author:** nguien (nguyen10t2) <nguyen10t2lhp@gmail.com>  
**GitHub:** https://github.com/nguyen10t2  
**Project:** bamboo-core — A high-performance Vietnamese input method engine core written in Rust.

`bamboo-core` is a Rust rewrite/extraction of the Vietnamese input logic (Telex/VNI/VIQR parsing, tone placement, syllable validation) originally developed for **ibus-bamboo** by **Luong Thanh Lam** <ltlam93@gmail.com> (GitHub: https://github.com/luongthanhlam). The original ibus-bamboo is a full IBus engine written in Go, licensed GPLv3.

`bamboo-core` relicenses the core Vietnamese input algorithm under MIT, making it usable as a library. MăngRS links `bamboo-core` (MIT) instead of copying `ibus-bamboo` (GPLv3) to avoid license incompatibility. The full MIT license text for `bamboo-core` is in the crate (`LICENSE` file in the `bamboo-core` package).

## Other Dependencies

All permissive; see `Cargo.lock` and each crate's `license` field:

- `dbus` (MIT/Apache-2.0)
- `dirs` (MIT/Apache-2.0)
- `serde` (MIT/Apache-2.0)
- `toml` (MIT/Apache-2.0)
- `gtk4` (MIT) — optional, `gui` feature only