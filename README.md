# Window Switcher

Window-Switcher offers customizable hotkeys for quickly switching windows on Windows OS:

1. ```Alt+`(Backtick)```: switch between windows of the same app.

![switch-windows](https://github.com/sigoden/window-switcher/assets/4012553/06d387ce-31fd-450b-adf3-01bfcfc4bce3)

2. ```Alt+Tab```: switch between apps. (disabled by default)

![switch-apps](https://github.com/sigoden/window-switcher/assets/4012553/0c74a7ca-3a48-4458-8d2d-b40dc041f067)

**💡 Hold down the `Alt` key and tap the ``` `(Backtick)/Tab ``` key to cycle through windows/apps, Press ```Alt + `(Backtick)/Tab``` and release both keys to switch to the last active window/app.**

## Installation

**Fork builds:** To use the options added by this fork, download the
`window-switcher-windows-x64` artifact from a successful build of
[`andytung/persistent-window-cycling`](https://github.com/andytung/window-switcher/actions/workflows/ci.yaml?query=branch%3Aandytung%2Fpersistent-window-cycling).
Extract the executable and INI into the same folder. The upstream releases and
installers below do not include this fork's additions.

1. **Download:** Visit the [Github Release](https://github.com/sigoden/windows-switcher/releases) and download the `windows-switcher.zip` file.
2. **Extract:** Unzip the downloaded file and extract the `window-switcher.exe` to your preferred location.
3. **Launch:** `window-switcher.exe` is a standalone executable, no installation is required, just double-click the file to run it.

For the tech-savvy, here's a one-liner to automate the installation:
```ps1
iwr -useb https://raw.githubusercontent.com/sigoden/window-switcher/main/install.ps1 | iex
```

You can also install Window-Switcher via [Scoop](https://github.com/ScoopInstaller/Scoop) with these commands:
```ps1
scoop bucket add extras
scoop install extras/window-switcher
```

## Configuration

Window-Switcher offers various customization options to tailor its behavior to your preferences. You can define custom keyboard shortcuts, enable or disable specific features, and fine-tune settings through a configuration file.

To personalize Window-Switcher, you'll need a configuration file named `window-switcher.ini`. This file should be placed in the same directory as the `window-switcher.exe` file. Once you've made changes to the configuration, make sure to restart Window-Switcher so your new settings can take effect.

Here is the default configuration:

```ini
# Whether to show trayicon, yes/no
trayicon = yes 

[switch-windows]

# Hotkey to switch windows
hotkey = alt+`

# Keep cycling across releases instead of returning to the previous window
persistent_cycle = no

# Only enable window switching in these apps; empty means all apps
allowlist =

# List of hotkey conflict apps
# e.g. game1.exe,game2.exe
denylist =

# Ignore minimal windows
ignore_minimal = no

# Only switch within the current virtual desktops: yes/no/auto
only_current_desktop = auto

[switch-apps]

# Whether to enable switching apps
enable = no 

# Hotkey to switch apps
hotkey = alt+tab

# Ignore minimal windows
ignore_minimal = no

# Only switch apps within the current virtual desktops: yes/no/auto
only_current_desktop = auto
```

### Persistent cycling and a single-key shortcut

This fork adds optional cycling across key releases and a left-Alt tap shortcut.
For example:

```ini
[switch-windows]
hotkey = alt
persistent_cycle = yes
allowlist = notepad.exe,code.exe,chrome.exe
denylist =
```

Replace the executable names with the apps you want to use. Restart Window-Switcher
after editing the INI beside the executable.

- `persistent_cycle = yes` keeps a stable same-app cycle across releases:
  A, B, C, A instead of A, B, A. Closed or ineligible windows are removed, and new
  windows are appended. If the foreground window is no longer the last selected
  window, or the app changes, the next shortcut starts a fresh cycle.
- `hotkey = alt` switches once when you release **left Alt** after pressing it
  alone. Holding it does not repeat. Another key, mouse click, or mouse wheel
  cancels the tap, including keys or mouse buttons already held when Alt is
  pressed. Right Alt is not a tap shortcut, so AltGr remains available.
- Alt+Tab, Alt+F4, Alt+letter menu shortcuts, and Alt+mouse gestures do not trigger
  tap switching. In enabled apps, a bare left-Alt tap replaces native menu
  activation. Outside the allowlist, Alt keeps its normal behavior.
- Use ``hotkey = alt || alt+` `` to enable both shortcuts, or keep
  ``hotkey = alt+` `` for the original shortcut. Shift reverses a chord-based
  shortcut; Shift+Alt alone is not a tap.
- `allowlist` is a comma-separated list of exact executable names, ignoring case.
  Empty means all apps. `denylist` takes precedence if an app appears in both
  lists. The legacy name `blacklist` is still accepted when `denylist` is absent;
  an explicit `denylist`, even empty, overrides it. These lists only affect
  same-app window switching, not the separate `[switch-apps]` feature.

All new behavior is opt-in. The default configuration retains the original
Alt+backtick shortcut and previous-window behavior after releasing Alt.
Invalid INI files are reported at startup instead of silently using defaults;
a missing INI still uses the defaults.

## Running as Administrator (Optional)

Window-Switcher works in standard user mode by default. Launch Window-Switcher in administrator mode to also manage applications running in administrator mode.

**Important:** If you enable the startup option while running in standard user mode, Window-Switcher will launch in standard mode upon system reboot. To ensure startup with admin privileges, launch Window-Switcher as administrator first before enabling startup.

## License

Copyright (c) 2023-2026 window-switcher developers.

window-switcher is made available under the terms of the MIT License, at your option.

See the LICENSE files for license details.
