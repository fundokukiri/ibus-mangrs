# Installation

MăngRS is developed and tested on **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; X11
smoke-tested). Any distro with IBus ≥ 1.5 and GTK ≥ 4.6 should work; reports from other distros
are welcome.

## Prerequisites

| Distro | Packages |
|---|---|
| Arch | `ibus` `gtk4` (build: `rust`) |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` (build: `cargo rustc`) |
| Fedora | `ibus` `gtk4-devel` (build: `rust cargo`) |
| openSUSE | `ibus` `gtk4-devel` (build: `rust cargo`) |

The package names above are the standard ones but are unverified on those distros. IBus must be
your active input-method framework (on GNOME it is by default).

## One-line install (online)

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/dev/scripts/install-online.sh | sh
```

Downloads the source, builds it, installs and restarts IBus. Requires `curl`, Rust (`cargo`),
`ibus`, `gtk4` and `sudo`. Review the script before piping it to `sh` if that matters to you.

## From a local checkout

```sh
git clone https://github.com/fundokukiri/ibus-mangrs
cd ibus-mangrs
cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart
```

After that, re-running `./scripts/autoinstall.sh` rebuilds and upgrades in one step.

Arch users can also build the package:

```sh
makepkg -si
```

The installer copies the engine to `/usr/lib/ibus-mangrs/`, registers the component in
`/usr/share/ibus/component/`, installs the `/dev/uinput` udev rule and adds your user to the
`input` group (group membership takes effect at next login). Uninstall:
`sudo ./scripts/uninstall.sh` — keeps your configuration.

## /dev/uinput permissions (manual fallback)

The engine presses **real Backspace keys** through `/dev/uinput` when an app cannot delete text
over IBus. Without write permission those apps cannot type tone marks. Create
`/etc/udev/rules.d/80-uinput.rules` (shipped as `data/80-uinput.rules`):

```
KERNEL=="uinput", MODE="0660", GROUP="input"
```

Then:

```sh
sudo modprobe uinput                      # load the kernel module now
sudo usermod -aG input $USER              # takes effect at next login
sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # load at every boot
```

Verify: `ls -l /dev/uinput` must show `crw-rw---- root input`. If `lsmod | grep uinput` shows
nothing, the module is not loaded — `sudo modprobe uinput` first.

## Enable the input source

1. `ibus restart` (after installing or upgrading the binary).
2. **Settings → Keyboard → Input Sources → +** → *Vietnamese* → **MăngRS**
   (or `ibus engine ibus-mangrs`).
3. `Super+Space` switches input sources; the gear next to the engine opens the settings window.

## Upgrade

Re-run the one-line installer or `./scripts/autoinstall.sh` after pulling new commits, then
`ibus restart`. Your configuration in `~/.config/ibus-mangrs/` is never touched by the installer.
