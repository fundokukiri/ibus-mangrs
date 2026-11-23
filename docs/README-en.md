<p align="center">
  <img src="../data/icons/logo.png" alt="MăngRS logo" width="360">
</p>

# MăngRS

[Tiếng Việt](../README.md) | **English**

A Vietnamese input method engine for **IBus**, written in **Rust** on top of
[`bamboo-core`](https://crates.io/crates/bamboo-core). 9 input methods, 11 output charsets,
5 display modes, a GTK4 settings window, macros, an emoji picker at the caret and per-app
rules.

> Developed and tested on **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; X11 smoke-tested).
> The only hard requirements are IBus ≥ 1.5 and GTK4 + libadwaita — it should work wherever
> IBus works. Reports from other distros are welcome.

The full feature list and the usage guide live in **[FEATURES.en.md](FEATURES.en.md)** — this
file focuses on installation and troubleshooting.

## Requirements

| Distro          | Packages (the one-line installer installs them)               |
| --------------- | ------------------------------------------------------------- |
| Arch            | `ibus` `gtk4` `libadwaita` `dbus` + `gcc` `make` `pkgconf`    |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev` + build-essential |
| Fedora          | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel` + `gcc` `make` |

The installer detects the distro (apt / pacman / dnf / zypper), installs only what is missing
and sets up Rust through rustup. GTK 4.10+ and libadwaita 1.5+ are required (Ubuntu 22.04
ships 4.6). Only Arch is tested. IBus must be the system's input-method framework (on GNOME
it is).

## Install

### One line (online)

Downloads the source, builds and installs; missing dependencies are installed too, so only
`curl` (or `wget`) and `sudo` are needed up front. The `/dev/uinput` udev rule is set up (the
session's user gets the device through logind, no group membership) and the `uinput` module is
loaded right away and at every boot, build-only packages
installed by the run are removed afterwards, and re-running installs nothing twice. `--demo`
previews the whole run without touching anything:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
# or, without curl:
wget -qO- https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
```

(Piping a script from the network to `sh` runs code from the network — review it first.)

To remove everything again:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/uninstall.sh | sh
```

Add `PURGE_CONFIG=1` before the command to also delete `~/.config/ibus-mangrs`; the plain
run keeps your config. `--demo` previews the removal without deleting anything.

### From a checkout

```sh
# Arch / Manjaro / EndeavourOS
sudo pacman -S --needed gcc pkgconf ibus dbus gtk4 libadwaita desktop-file-utils

# Debian / Ubuntu / Mint / Pop!_OS
sudo apt install gcc libc6-dev pkg-config ibus libdbus-1-dev libgtk-4-dev \
    libadwaita-1-dev desktop-file-utils

# Fedora / RHEL / CentOS
sudo dnf install gcc pkgconf-pkg-config ibus dbus-devel gtk4-devel \
    libadwaita-devel desktop-file-utils

# openSUSE
sudo zypper install gcc pkg-config ibus dbus-1-devel gtk4-devel \
    libadwaita-devel desktop-file-utils

# Rust (all distros)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart
```

The install script copies the engine to `/usr/lib/ibus-mangrs/`, registers the IBus component,
installs the udev rule and removes your user from the `input` group if an older version added it
(effective at next login). Uninstall: `sudo ./scripts/uninstall.sh` (keeps your config). On
Arch: `makepkg -si`.

### /dev/uinput permissions (manual fallback)

The engine presses **real Backspace keys** through `/dev/uinput` for apps that cannot delete
text over IBus; without write permission those apps cannot type tone marks. Access comes from
the active session's user through logind, not from group membership. Create
`/etc/udev/rules.d/70-uinput.rules` (shipped as `data/rules/70-uinput.rules`):

```
KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"
```

Then load the module, reload the rules and log out/in once (the ACL is granted when the
session becomes active):

```sh
sudo modprobe uinput    # load the module now — many distros do not autoload it; without it /dev/uinput is missing
sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=/dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # load at every boot
getfacl -p /dev/uinput            # must list user:<you>:rw-
```

**If an older version told you to join the `input` group, leave it**
(`sudo gpasswd -d $USER input`, then log out and back in) — that group can read every keyboard
of the machine. **If an old `/etc/udev/rules.d/80-uinput.rules` is still around, remove it**:
udev reads every file in the directory, so the old rule stays effective. Details:
[SECURITY.md](SECURITY.md).

## Enable the input source

1. `ibus restart` (after installing or upgrading the binary).
2. **Settings → Keyboard → Input Sources → +** → _Vietnamese_ → **MăngRS**
   (or `ibus engine ibus-mangrs`).
3. `Super+Space` switches between input sources (GNOME's own shortcut); the gear next to the
   engine opens the settings window.

## Configuration files

```
~/.config/ibus-mangrs/config.toml     # all settings, hot-reloaded within ~1 s
~/.config/ibus-mangrs/macros.toml     # one key=expansion per line, $TIME / $DATE
~/.config/ibus-mangrs/emoji.toml      # name = "glyph" pairs
~/.config/ibus-mangrs/dictionary.txt  # words never auto-restored
```

## Troubleshooting

- **Engine not listed in Input Sources**: run `ibus write-cache --system` then `ibus-daemon -drx`;
  make sure `/usr/share/ibus/component/ibus-mangrs.xml` exists.
- **Marks do not appear in some apps** (Sublime, XWayland clients): `/dev/uinput` is missing or
  not writable — see the permissions section above. Check the session ACL with
  `getfacl -p /dev/uinput`.
- **Just installed/upgraded but it still behaves like the old version**: the running engine is
  the old code — `ibus restart` after every binary replacement.
- **Mode changes seem ignored**: an App Rule for the app overrides the global mode (see
  FEATURES-en.md, _App Rules_).
- **X11/XIM apps (JetBrains) misbehave**: try the two _X11/XIM compatibility_ switches in the
  GUI _Tuning_ page.

## Other documents

- [FEATURES-en.md](FEATURES-en.md) — the full feature list, the five display modes, per-feature usage.
- [README.md](../README.md) — the Vietnamese readme (the repo's primary readme).
- [SECURITY-en.md](SECURITY-en.md) — the trust boundary, residual risks, verification and revocation.
- [ARCHITECTURE.md](../ARCHITECTURE.md) — engine architecture (for code changes).
- [CHANGELOG.md](../CHANGELOG.md) — changes per release.

## License

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) — [GPL-3.0 License](../LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Copyright (C) 2018 Luong Thanh Lam, [GPL-3.0 License](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file).
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) Vietnamese input algorithm library in Rust, [MIT License](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file).
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Copyright (C) 2018 Luong Thanh Lam, [MIT License](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file).
