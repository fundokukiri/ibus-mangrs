# Security

[Tiếng Việt](docs/SECURITY.md) | **English**

MăngRS is an input engine: it sees every keystroke typed into an application and it can put
text and key events into the focused window. This file states what it does with that position,
what it refuses to do, and where the trust boundary is.

## Trust boundary

The boundary is the **user account**. The engine runs as the logged-in user (started by
`ibus-daemon`), never as root and never through a setuid binary. It does not try to defend
against code running as the same user, because that code has other channels that no input
engine can close:

- registering an IBus engine and switching to it (`RegisterComponent`, `SetGlobalEngine`) —
  it then receives every keystroke and can commit text;
- `XTEST` / `XRecord` on X11 — injection and keylogging from any X client;
- `AT-SPI` text interfaces on Wayland — text insertion into GTK applications;
- editing `~/.config/ibus-mangrs/*.toml`, which the engine hot-reloads within about a second.

A design goal that follows: the engine must not need, and must not ask for, a privilege that is
larger than what those channels already give an ordinary process of the account.

## What the engine can do

| Action | Mechanism | Limit |
| --- | --- | --- |
| Inject key events | one of the injection backends chosen per session — `/dev/uinput` (kernel device), XTest (X11), the wlroots virtual-keyboard protocol (Hyprland/wlroots), or libei over mutter's direct API (GNOME) — see `ARCHITECTURE.md` "Injection transports" | every backend is held to the same whitelist by the engine: Backspace and Delete only — text always travels over IBus, never through injection. On `/dev/uinput` the kernel enforces it (only those keycodes are registered); on the protocol backends the trait method set is the enforcement |
| Commit text | IBus `CommitText` to the focused input context | the focused client only |
| Write files | `~/.config/ibus-mangrs/`, `~/.cache/ibus-mangrs/`, exports from the settings window | nothing under `/usr`, `/etc`, or another user's home |

The engine opens no network connections. The debug log (`debug_log = true`) is off by default
and created `0600`; its separate typed-text tier (`debug_log_text`) can contain what you typed.

## libei on GNOME

On GNOME Wayland the EI (Emulated Input) backend speaks to mutter through the compositor's
own `org.gnome.Mutter.RemoteDesktop` session API; it is registered but never auto-selected
there until an indicator-free acquisition exists and mutter stops segfaulting on EI client
vanish (15/11: the engine-held session keeps the screen-sharing indicator on and twice
crashed the shell). Measured on GNOME 51: that
API grants input injection to any session-bus client without a consent dialog — the
compositor trusts processes of the session, which is exactly the trust boundary this file
already assumes (the engine runs as the logged-in user, and such a process could use
XTEST/XRecord on X11 or `/dev/uinput` with the uaccess ACL anyway). What the engine adds on
top is the same whitelist as every other backend: only Backspace and Delete,
enforced by the backend trait's method set — no backend call can type text. The
`libei.so.1` library is loaded at runtime (dlopen): a system without it builds and runs the
engine fine, and the libei backend simply never activates.

## `/dev/uinput` access

The udev rule `70-uinput.rules` marks the device with `uaccess`. `systemd-logind` then grants a
POSIX ACL to the user of the **active session**, and removes it when the session ends:

```
KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"
```

No group membership is involved. An older version of the installer added the user to the
`input` group, which grants read access to every `/dev/input/event*` device on the machine —
a system-wide keylogger, still readable on the lock screen, and it survives logout. The
installer now removes that membership and deletes a leftover `80-uinput.rules`; udev reads every
file in `rules.d`, so an old copy would keep the group access alive next to the new rule.

`/dev/uinput` is opened lazily and retried every two seconds while it is unavailable, because
logind can apply the ACL a moment after the engine starts. If it never becomes available,
commit-as-you-type typing falls back to the protocol delete where the client reports its text
and to Backspace keys forwarded through IBus otherwise — it does not fail silently.

## Residual risks (accepted)

1. **The ACL is a per-user capability, not a per-process one.** Any process of the same account
   can open `/dev/uinput` and create its own virtual keyboard, mouse included. This is the same
   class of exposure as the IBus and X11 channels above.
2. **The ACL stays while the session is locked** (a locked session is still active), so injected
   keys can reach the lock screen or a password prompt. The engine itself does nothing while it
   has no composition to rewrite, but the capability exists.
3. **Injection follows focus.** Paced rewrites span milliseconds; if focus moves mid-word the
   remaining keys land in the new window. This is why the engine verifies the client it writes
   to and skips the rewrite when it cannot.
4. **A compromised account is a compromised engine.** Configuration, the engine binary and the
   debug log are user-writable by design.
5. **X11 offers no per-client isolation at all** (`XTEST`, `XRecord`); the hardening here is
   meaningful on Wayland sessions.

## Verify it on your machine

```sh
# 1. no group membership (nothing printed, exit code 1)
id -nG | tr ' ' '\n' | grep -x input; echo "exit=$?"

# 2. keyboards are not readable by the account
head -c 1 /dev/input/event0          # Permission denied

# 3. the session ACL exists for you and for brltty (a real user of the device)
getfacl -p /dev/uinput | grep -E 'user:'

# 4. the device registers two keys (bit 14, 111)
for f in /sys/class/input/event*/device/name; do
    n=$(cat "$f" 2>/dev/null)
    case "$n" in *mangrs*) cat "${f%name}capabilities/key";; esac
done
# expected: 800000000000 4000
```

A normal engine start logs `engine start uinput=true` and no warning about the `input` group
(`debug_log = true`).

## Revoke access

```sh
# remove the session ACL and stop using the device
sudo rm /etc/udev/rules.d/70-uinput.rules
sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=/dev/uinput
ibus restart
```

The Preedit and Forward Backspace modes never touch the device, and without it Surrounding
falls back to the protocol delete or the forwarded Backspace keys while Flow falls back to
Forward Backspace — the engine keeps working.

## Reporting a vulnerability

Open a private security advisory on the GitHub repository
(`https://github.com/fundokukiri/ibus-mangrs`) instead of a public issue. Include the version,
the distribution and desktop, the mode in use, and a debug log with `debug_log = true`
(leave `debug_log_text` off unless the report needs the typed text).
