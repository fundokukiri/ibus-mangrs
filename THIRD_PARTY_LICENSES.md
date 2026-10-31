# Third-party licenses

Dependencies of `ibus-mangrs` (versions from `Cargo.lock`; licenses read from each
crate's `Cargo.toml` in the cargo registry cache — not a legal opinion, see the note
at the bottom).

| Crate | Version | License | Copyleft |
|---|---|---|---|
| bamboo-core | 0.5.3 | MIT | no |
| dbus | 0.9.12 | Apache-2.0 OR MIT | no |
| chrono | 0.4.45 | MIT OR Apache-2.0 | no |
| dirs | 6.0.0 | MIT OR Apache-2.0 | no |
| libc | 0.2.191 | MIT OR Apache-2.0 | no |
| serde (+ derive) | 1.0.229 / 1.0.154 | MIT OR Apache-2.0 | no |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | no |
| signal-hook | 0.3.18 | Apache-2.0 OR MIT | no |
| toml | 0.8.23 | MIT OR Apache-2.0 | no |
| gtk4 (optional, `gui`) | 0.9.7 | MIT | no |

Transitive dependencies inherit their own licenses (all permissive in the current
lockfile as far as the registry metadata shows; run `cargo license` after any
dependency change to re-verify).

## Non-crate sources referenced by this project

| Source | License | How it is used |
|---|---|---|
| [ibus-bamboo](https://github.com/BambooEngine/ibus-bamboo) (Go) | GPL-3.0 | **Behavioral reference only** — vendored under `analysis/` for comparison; no code copied into MăngRS. Its Rust port `bamboo-core` is MIT. |
| Vietnamese word list (`data/vn-words.txt`) | GPL | Distributed as a separate data file, source and rationale in `data/README.md`; deleting the file only disables the dictionary check. |

## License of this project

MIT (`LICENSE`, `Cargo.toml` `license = "MIT"`). MIT is compatible with depending on
MIT/Apache crates and with the GPL data file being kept separate from the code.

> Note: this table is a technical inventory, not legal advice; the project owner should
> confirm before redistribution.
