# Window Switcher

Forked from [sigoden/window-switcher](https://github.com/sigoden/window-switcher)
and maintained independently, with persistent window cycling, a single-key Alt
shortcut, manual F-key window assignments, and app allowlist/denylist configuration.

Window Switcher runs on Windows. It switches between windows of the **same app**,
with an optional separate shortcut for switching between apps.

## Quick configuration

To tap **left Alt** and keep cycling through windows of selected apps, put this
in `window-switcher.ini` beside `window-switcher.exe`:

```ini
[switch-windows]
hotkey = alt
persistent_cycle = yes
allowlist = chrome.exe,code.exe,notepad.exe
denylist =
```

Replace the executable names with your apps. Leave `allowlist` empty to enable
window switching in all apps. Restart Window Switcher after changing the INI.

The hotkey and cycling behavior are independent. To keep the original
Alt+backtick shortcut with persistent cycling instead:

```ini
[switch-windows]
hotkey = alt+`
persistent_cycle = yes
allowlist = chrome.exe,code.exe,notepad.exe
denylist =
```

## Installation

**There is no prebuilt download for this version yet.** Upstream releases, the
upstream installer, and the Scoop package do not contain these changes. For now,
build this repository on Windows.

### Build from source on Windows

Install Git, [Rust with rustup](https://www.rust-lang.org/tools/install), and the
Microsoft Visual Studio Build Tools with the **Desktop development with C++**
workload and Windows SDK.

Run in PowerShell:

```powershell
git clone https://github.com/andytung/window-switcher.git
cd window-switcher
rustup toolchain install 1.97.0 --profile minimal
cargo +1.97.0 build --locked --release
Copy-Item .\window-switcher.ini .\target\release\window-switcher.ini
```

Edit `target\release\window-switcher.ini` using one of the examples above, then
launch `target\release\window-switcher.exe`. The executable is portable: you can
copy it and the INI together to a folder of your choice. No installer is required.
Exit any running copy before launching a replacement.

The repository's `install.ps1` targets releases of **this repository**, not
upstream. It will not work until a release is published; use the source-build
instructions above in the meantime.

## Configuration reference

The complete default configuration is in
[`window-switcher.ini`](window-switcher.ini). The INI must be beside the
executable, not merely in the current working directory.

### Window switching

All these settings belong in `[switch-windows]`.

| Setting | Default | Behavior |
|---------|---------|----------|
| `hotkey` | ``alt+` `` | Shortcut used to switch windows. |
| `persistent_cycle` | `no` | Set to `yes` to continue the cycle across key releases. |
| `manual_slots` | `no` | Enable manual Ctrl+Alt+Fn assignment and Fn recall for F1 through F24. |
| `allowlist` | Empty | Exact executable names where window switching is enabled. Empty permits all apps. |
| `denylist` | Empty | Executable names where window switching is disabled. Takes precedence over the allowlist. |
| `ignore_minimal` | `no` | Set to `yes` to exclude minimized windows. |
| `only_current_desktop` | `auto` | `yes` limits switching to the current virtual desktop; `no` includes all desktops; `auto` follows Windows settings. |

With `persistent_cycle = yes`, successive shortcut presses cycle A, B, C, A,
even if you release every key between presses. Window order remains stable
during the cycle. Closed or ineligible windows are removed, and new windows are
appended. If the foreground window is no longer the last selected window, or
the app changes, the next shortcut starts a fresh cycle.

With `persistent_cycle = no`, releasing the modifier retains the original
previous-window behavior, such as A, B, A. Holding the modifier while repeatedly
pressing the shortcut still cycles through windows.

### Hotkeys

`hotkey` selects the shortcut; there is no separate toggle for Alt-only mode.

| Example | Result |
|---------|--------|
| ``hotkey = alt+` `` | Alt+backtick, the original shortcut. |
| `hotkey = ctrl+space` | Ctrl+Space. |
| `hotkey = alt` | One switch when left Alt is tapped and released alone. |
| ``hotkey = alt || alt+` `` | Enable both Alt alone and Alt+backtick. |

Chord shortcuts support one `alt`, `ctrl`, or `win` modifier plus a supported
key. Add Shift while using a chord shortcut to reverse direction. Multiple
hotkeys are separated by `||`.

For `hotkey = alt`, holding Alt does not repeat. Another key, a mouse click, or
the mouse wheel cancels the tap, including keys or mouse buttons already held
when Alt is pressed. Alt+Tab, Alt+F4, Alt+letter menu shortcuts, and Alt+mouse
gestures therefore do not trigger tap switching. Shift+Alt alone is not a tap.
Right Alt is not a tap shortcut, preserving AltGr.

Inside enabled apps, a bare left-Alt tap replaces native menu activation.
Outside the allowlist or inside the denylist, Alt behaves normally.
Arbitrary standalone keys other than left Alt are not supported by `hotkey`.
Manual F-key assignments use the separate `manual_slots` setting below.

### Manual F-key assignments

Enable this independently of the cycling shortcut:

```ini
[switch-windows]
manual_slots = yes
allowlist = chrome.exe,code.exe,notepad.exe
denylist =
```

Focus a window and press **Ctrl+left Alt+F1** to assign it to F1. Focus another
window and press **Ctrl+left Alt+F2** to assign it to F2. Afterwards, press **F1**
or **F2** alone to jump directly to that window. F1 through F24 are supported.
Right Alt is not an assignment modifier, so AltGr combinations remain available.

Assignments are per app, not global: Chrome and VS Code can each have their own
F1 window. Slots use the same app grouping as normal cycling, so browser profiles
and installed web apps can have separate assignments.

- Nothing is assigned automatically. An unassigned F-key keeps its normal app
  behavior.
- Assigning an occupied slot replaces only that slot. Other assignments do not
  move when focus changes, new windows open, or another slot is reassigned.
- Closing a window loses all its assignments. New windows never inherit them,
  even if Windows reuses the closed window's internal handle.
- Assignments are kept only in memory. Exiting or restarting Window Switcher
  clears them; there are no saved mappings or title-matching rules.
- The allowlist, denylist, minimized-window filter, and virtual-desktop setting
  also apply to manual slots. An assigned but currently excluded window retains
  its assignment, but its F-key passes through until the window is eligible.

Holding an assignment or recall key performs its action once, not repeatedly.
Other modified F-key combinations keep their existing behavior. Manual slot
shortcuts take precedence over conflicting cycling shortcuts when a slot action
can be performed. Regular cycling remains available and never assigns slots.

### App filtering and compatibility

`allowlist` and `denylist` accept comma-separated executable names, matched
exactly and case-insensitively, such as `chrome.exe,Code.exe`. They do not accept
window titles, full paths, or wildcards. If both lists contain an app, it is
denied.

The legacy name `blacklist` remains accepted when `denylist` is absent. An
explicit `denylist`, even empty, overrides the legacy setting.

Filtering, persistent cycling, and manual slots affect same-app window switching
only. They do not change the separate `[switch-apps]` feature.

### Other settings

The default configuration keeps `[switch-apps] enable = no`. Enable it to use
the app switcher, whose default hotkey is `alt+tab`. It also supports minimized
window filtering, virtual-desktop selection, and icon overrides.

`trayicon` controls the tray icon. The `[log]` section controls log level and
file location. See the comments in the default INI for details.

Invalid INI files are reported at startup instead of silently using defaults.
A missing INI uses the defaults, including Alt+backtick and
`persistent_cycle = no`; the new behavior is opt-in.

## Administrator access and startup

Window Switcher runs in standard user mode by default. Run it as administrator
to also manage windows belonging to applications running as administrator.

The tray menu can enable launch at startup. If startup is enabled in standard
user mode, subsequent launches are also in standard mode. To enable startup
with administrator privileges, launch as administrator first.

## Attribution and license

Based on [sigoden/window-switcher](https://github.com/sigoden/window-switcher).
The original project and this independently maintained version use the
[MIT License](LICENSE). The original license and copyright notice are retained.

Copyright (c) 2023-2026 window-switcher developers.
