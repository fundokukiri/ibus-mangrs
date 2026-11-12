# Security

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
| Inject key events | virtual keyboard on `/dev/uinput` (Flow, Turbo, Select, and the Backspace fallback) | the device registers only `Backspace`, `Delete`, `Left`, `Shift`, `Ctrl` and `V`; the kernel drops every other keycode, so a bug here cannot type text |
| Commit text | IBus `CommitText` to the focused input context | the focused client only |
| Use the clipboard | opt-in `unicode_channel = "clipboard"`: write the word, press `Ctrl+V`, restore the previous clipboard | off by default; the default channel is `CommitText` |
| Write files | `~/.config/ibus-mangrs/`, `~/.cache/ibus-mangrs/`, exports from the settings window | nothing under `/usr`, `/etc`, or another user's home |

The engine opens no network connections.

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
logind can apply the ACL a moment after the engine starts. If it never becomes available, the
engine falls back to the protocol delete and preedit — it does not fail silently.

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

# 4. the device registers six keys (bit 14, 29, 42, 47, 105, 111)
for f in /sys/class/input/event*/device/name; do
    n=$(cat "$f" 2>/dev/null)
    case "$n" in *mangrs*) cat "${f%name}capabilities/key";; esac
done
# expected: 820000000000 840020004000
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

The Preedit and NonPreedit modes never touch the device, so the engine keeps working without it;
Flow, Turbo, Select and the Backspace fallback degrade to the protocol delete.

## Reporting a vulnerability

Open a private security advisory on the GitHub repository
(`https://github.com/fundokukiri/ibus-mangrs`) instead of a public issue. Include the version,
the distribution and desktop, the mode in use, and a debug log with `debug_log = true`
(leave `debug_log_text` off unless the report needs the typed text).
