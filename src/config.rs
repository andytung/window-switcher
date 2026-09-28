use std::{collections::HashSet, fs, path::PathBuf, process::Command};

use anyhow::{anyhow, Result};
use indexmap::IndexMap;
use ini::{Ini, ParseOption};
use log::LevelFilter;
use windows::core::w;

use crate::utils::{get_exe_folder, RegKey};

pub const SWITCH_WINDOWS_HOTKEY_ID: u32 = 1;
pub const SWITCH_APPS_HOTKEY_ID: u32 = 2;

const DEFAULT_CONFIG: &str = include_str!("../window-switcher.ini");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub trayicon: bool,
    pub log_level: LevelFilter,
    pub log_file: Option<PathBuf>,
    pub switch_windows_hotkey: Vec<Hotkey>,
    pub switch_windows_denylist: HashSet<String>,
    pub switch_windows_allowlist: HashSet<String>,
    pub switch_windows_persistent_cycle: bool,
    pub switch_windows_manual_slots: bool,
    pub switch_windows_ignore_minimal: bool,
    switch_windows_only_current_desktop: Option<bool>,
    pub switch_apps_enable: bool,
    pub switch_apps_hotkey: Vec<Hotkey>,
    pub switch_apps_ignore_minimal: bool,
    pub switch_apps_override_icons: IndexMap<String, String>,
    switch_apps_only_current_desktop: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            trayicon: true,
            log_level: LevelFilter::Info,
            log_file: None,
            switch_windows_hotkey: vec![Hotkey::create(
                SWITCH_WINDOWS_HOTKEY_ID,
                "switch windows",
                "alt + `",
            )
            .unwrap()],
            switch_windows_denylist: Default::default(),
            switch_windows_allowlist: Default::default(),
            switch_windows_persistent_cycle: false,
            switch_windows_manual_slots: false,
            switch_windows_ignore_minimal: false,
            switch_windows_only_current_desktop: None,
            switch_apps_enable: false,
            switch_apps_hotkey: vec![Hotkey::create(
                SWITCH_APPS_HOTKEY_ID,
                "switch apps",
                "alt + tab",
            )
            .unwrap()],
            switch_apps_ignore_minimal: false,
            switch_apps_override_icons: Default::default(),
            switch_apps_only_current_desktop: None,
        }
    }
}

impl Config {
    pub fn load(ini_conf: &Ini) -> Result<Self> {
        let mut conf = Config::default();
        if let Some(section) = ini_conf.section(None::<String>) {
            if let Some(v) = section.get("trayicon").and_then(Config::to_bool) {
                conf.trayicon = v;
            }
        }

        if let Some(section) = ini_conf.section(Some("log")) {
            if let Some(level) = section.get("level").and_then(|v| v.parse().ok()) {
                conf.log_level = level;
            }
            if let Some(path) = section.get("path").map(normalize_path_value) {
                if !path.trim().is_empty() {
                    let mut path = PathBuf::from(path);
                    if !path.is_absolute() {
                        let parent = get_exe_folder()?;
                        path = parent.join(path);
                    }
                    conf.log_file = Some(path);
                }
            }
        }

        if let Some(section) = ini_conf.section(Some("switch-windows")) {
            if let Some(v) = section.get("hotkey") {
                if !v.trim().is_empty() {
                    conf.switch_windows_hotkey =
                        parse_hotkeys(SWITCH_WINDOWS_HOTKEY_ID, "switch windows", v)?;
                }
            }

            if let Some(v) = section.get("denylist").or_else(|| section.get("blacklist")) {
                conf.switch_windows_denylist = parse_app_list(v);
            }
            if let Some(v) = section.get("allowlist") {
                conf.switch_windows_allowlist = parse_app_list(v);
            }
            if let Some(v) = section.get("persistent_cycle") {
                conf.switch_windows_persistent_cycle = Config::to_bool(v)
                    .ok_or_else(|| anyhow!("Invalid switch windows persistent_cycle: {v}"))?;
            }
            if let Some(v) = section.get("manual_slots") {
                conf.switch_windows_manual_slots = Config::to_bool(v)
                    .ok_or_else(|| anyhow!("Invalid switch windows manual_slots: {v}"))?;
            }
            if let Some(v) = section.get("ignore_minimal").and_then(Config::to_bool) {
                conf.switch_windows_ignore_minimal = v;
            }
            if let Some(v) = section
                .get("only_current_desktop")
                .and_then(Config::to_bool)
            {
                conf.switch_windows_only_current_desktop = Some(v);
            }
        }
        if let Some(section) = ini_conf.section(Some("switch-apps")) {
            if let Some(v) = section.get("enable").and_then(Config::to_bool) {
                conf.switch_apps_enable = v;
            }
            if let Some(v) = section.get("hotkey") {
                if !v.trim().is_empty() {
                    conf.switch_apps_hotkey =
                        parse_hotkeys(SWITCH_APPS_HOTKEY_ID, "switch apps", v)?;
                }
            }
            if let Some(v) = section.get("ignore_minimal").and_then(Config::to_bool) {
                conf.switch_apps_ignore_minimal = v;
            }
            if let Some(v) = section.get("override_icons").map(normalize_path_value) {
                conf.switch_apps_override_icons = v
                    .split([',', ';'])
                    .filter_map(|v| {
                        v.trim()
                            .split_once("=")
                            .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                    })
                    .collect();
            }

            if let Some(v) = section
                .get("only_current_desktop")
                .and_then(Config::to_bool)
            {
                conf.switch_apps_only_current_desktop = Some(v);
            }
        }
        Ok(conf)
    }

    pub fn to_hotkeys(&self) -> Vec<&Hotkey> {
        let mut hotkeys: Vec<&Hotkey> = self.switch_windows_hotkey.iter().collect();
        if self.switch_apps_enable {
            hotkeys.extend(self.switch_apps_hotkey.iter());
        }
        hotkeys
    }

    pub fn to_bool(v: &str) -> Option<bool> {
        match v {
            "yes" | "true" | "on" | "1" => Some(true),
            "no" | "false" | "off" | "0" => Some(false),
            _ => None,
        }
    }

    /// Whether the user has configured app switching to include other desktops.
    /// If the configured value is not a valid bool, the Windows registry will be
    /// used as a fallback.
    pub fn switch_apps_only_current_desktop(&self) -> bool {
        self.switch_apps_only_current_desktop
            .unwrap_or_else(Self::system_switcher_only_current_desktop)
    }

    /// Whether the user has configured window switching to include other desktops.
    /// If the configured value is not a valid bool, the Windows registry will be
    /// used as a fallback.
    pub fn switch_windows_only_current_desktop(&self) -> bool {
        self.switch_windows_only_current_desktop
            .unwrap_or_else(Self::system_switcher_only_current_desktop)
    }

    fn system_switcher_only_current_desktop() -> bool {
        let alt_tab_filter = RegKey::new_hkcu(
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"),
            w!("VirtualDesktopAltTabFilter"),
        )
        .and_then(|k| k.get_int())
        .unwrap_or(1);

        alt_tab_filter != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub id: u32,
    pub name: String,
    pub modifier: [u32; 2],
    pub code: Option<u32>,
}

impl Hotkey {
    pub fn create(id: u32, name: &str, value: &str) -> Result<Self> {
        let (modifier, code) =
            Self::parse(value).ok_or_else(|| anyhow!("Invalid {name} hotkey"))?;
        if code.is_none() && id != SWITCH_WINDOWS_HOTKEY_ID {
            return Err(anyhow!(
                "Alt-only hotkeys are only supported for switch windows"
            ));
        }
        Ok(Self {
            id,
            name: name.to_string(),
            modifier,
            code,
        })
    }

    pub fn get_modifier(&self) -> u32 {
        self.modifier[0]
    }

    pub fn parse(value: &str) -> Option<([u32; 2], Option<u32>)> {
        let value = value
            .to_ascii_lowercase()
            .replace(' ', "")
            .replace("vk_", "");
        if value == "alt" {
            return Some(([0x38, 0x38], None));
        }
        let keys: Vec<&str> = value.split('+').collect();
        if keys.len() != 2 {
            return None;
        }
        let modifier = match keys[0] {
            "win" => [0x5b, 0x5c],
            "alt" => [0x38, 0x38],
            "ctrl" => [0x1d, 0x1d],
            _ => {
                return None;
            }
        };
        // see <https://kbdlayout.info/kbdus/overview+scancodes>
        let code = match keys[1] {
            "esc" | "escape" => 0x01,
            "1" | "!" => 0x02,
            "2" | "@" => 0x03,
            "3" | "#" => 0x04,
            "4" | "$" => 0x05,
            "5" | "%" => 0x06,
            "6" | "^" => 0x07,
            "7" | "&" => 0x08,
            "8" | "*" => 0x09,
            "9" | "(" => 0x0a,
            "0" | ")" => 0x0b,
            "-" | "_" | "oem_minus" => 0x0c,
            "+" | "=" | "oem_plus" => 0x0d,
            "bs" | "backspace" => 0x0e,
            "tab" => 0x0f,
            "q" => 0x10,
            "w" => 0x11,
            "e" => 0x12,
            "r" => 0x13,
            "t" => 0x14,
            "y" => 0x15,
            "u" => 0x16,
            "i" => 0x17,
            "o" => 0x18,
            "p" => 0x19,
            "{" | "[" | "oem_4" => 0x1a,
            "}" | "]" | "oem_6" => 0x1b,
            "enter" | "return" => 0x1c,
            "a" => 0x1e,
            "s" => 0x1f,
            "d" => 0x20,
            "f" => 0x21,
            "g" => 0x22,
            "h" => 0x23,
            "j" => 0x24,
            "k" => 0x25,
            "l" => 0x26,
            ":" | ";" | "oem_1" => 0x27,
            "\"" | "'" | "oem_7" => 0x28,
            "~" | "`" | "oem_3" => 0x29,
            "|" | "\\" | "oem_5" => 0x2b,
            "z" => 0x2c,
            "x" => 0x2d,
            "c" => 0x2e,
            "v" => 0x2f,
            "b" => 0x30,
            "n" => 0x31,
            "m" => 0x32,
            "<" | "," | "oem_comma" => 0x33,
            ">" | "." | "oem_period" => 0x34,
            "?" | "/" | "oem_2" => 0x35,
            "space" => 0x39,
            "capslock" => 0x3a,
            "f1" => 0x3b,
            "f2" => 0x3c,
            "f3" => 0x3d,
            "f4" => 0x3e,
            "f5" => 0x3f,
            "f6" => 0x40,
            "f7" => 0x41,
            "f8" => 0x42,
            "f9" => 0x43,
            "f10" => 0x44,
            "numlock" => 0x45,
            "scrolllock" => 0x46,
            "home" => 0x47,
            "up" => 0x48,
            "pageup" => 0x49,
            "left" => 0x4b,
            "right" => 0x4d,
            "end" => 0x4f,
            "down" => 0x50,
            "pagedown" => 0x51,
            "insert" => 0x52,
            "delete" => 0x53,
            "prtsc" | "printscreen" => 0x54,
            "oem_102" => 0x56,
            "f11" => 0x57,
            "f12" => 0x58,
            "menu" => 0x5d,
            _ => return None,
        };
        Some((modifier, Some(code)))
    }
}

pub fn load_config() -> Result<Config> {
    let filepath = get_config_path()?;
    let opt = ParseOption {
        enabled_escape: false,
        ..Default::default()
    };
    let conf = match Ini::load_from_file_opt(&filepath, opt) {
        Ok(conf) => conf,
        Err(ini::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(err) => {
            return Err(anyhow!(
                "Failed to load config file '{}', {err}",
                filepath.display()
            ));
        }
    };
    Config::load(&conf)
}

pub(crate) fn edit_config_file() -> Result<bool> {
    let filepath = get_config_path()?;
    debug!("open config file '{}'", filepath.display());
    if !filepath.exists() {
        fs::write(&filepath, DEFAULT_CONFIG).map_err(|err| {
            anyhow!(
                "Failed to write config file '{}', {err}",
                filepath.display()
            )
        })?;
    }
    let exit = Command::new("notepad.exe")
        .arg(&filepath)
        .spawn()
        .map_err(|err| anyhow!("Failed to open config file '{}', {err}", filepath.display()))?
        .wait()
        .map_err(|err| {
            anyhow!(
                "Failed to close config file '{}', {err}",
                filepath.display()
            )
        })?;

    Ok(exit.success())
}

fn get_config_path() -> Result<PathBuf> {
    let folder = get_exe_folder()?;
    let config_path = folder.join("window-switcher.ini");
    Ok(config_path)
}

fn normalize_path_value(value: &str) -> String {
    value.replace("\\\\", "\\")
}

fn parse_app_list(value: &str) -> HashSet<String> {
    normalize_path_value(value)
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn parse_hotkeys(id: u32, name: &str, value: &str) -> Result<Vec<Hotkey>> {
    let parts: Vec<&str> = value.split("||").collect();
    let mut hotkeys = vec![];
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        hotkeys.push(Hotkey::create(id, name, part)?);
    }
    if hotkeys.is_empty() {
        return Err(anyhow!("Invalid {name} hotkey"));
    }
    Ok(hotkeys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey() {
        assert_eq!(Hotkey::parse("alt + `"), Some(([0x38, 0x38], Some(0x29))));
        assert_eq!(Hotkey::parse("alt + tab"), Some(([0x38, 0x38], Some(0x0f))));
        assert_eq!(Hotkey::parse(" ALT "), Some(([0x38, 0x38], None)));
        assert!(Hotkey::parse("ctrl").is_none());
        assert!(Hotkey::create(SWITCH_APPS_HOTKEY_ID, "switch apps", "alt").is_err());
    }

    #[test]
    fn test_parse_hotkeys() {
        let hotkeys = parse_hotkeys(1, "test", "alt+` || alt+tab").unwrap();
        assert_eq!(hotkeys.len(), 2);
        assert_eq!(hotkeys[0].modifier, [0x38, 0x38]);
        assert_eq!(hotkeys[0].code, Some(0x29));
        assert_eq!(hotkeys[1].modifier, [0x38, 0x38]);
        assert_eq!(hotkeys[1].code, Some(0x0f));

        let hotkeys = parse_hotkeys(1, "test", "alt+`").unwrap();
        assert_eq!(hotkeys.len(), 1);
        assert_eq!(hotkeys[0].modifier, [0x38, 0x38]);
        assert_eq!(hotkeys[0].code, Some(0x29));
    }

    #[test]
    fn default_ini_preserves_defaults() {
        let ini = Ini::load_from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(Config::load(&ini).unwrap(), Config::default());
    }

    #[test]
    fn persistent_alt_tap_with_allowlist() {
        let ini = Ini::load_from_str(
            "[switch-windows]\nhotkey = alt || alt+`\npersistent_cycle = yes\n\
             allowlist = Notepad.exe, , CODE.EXE,\ndenylist = game.exe",
        )
        .unwrap();
        let config = Config::load(&ini).unwrap();
        assert!(config.switch_windows_persistent_cycle);
        assert_eq!(config.switch_windows_hotkey.len(), 2);
        assert_eq!(config.switch_windows_hotkey[0].code, None);
        assert_eq!(config.switch_windows_hotkey[1].code, Some(0x29));
        assert_eq!(
            config.switch_windows_allowlist,
            HashSet::from(["notepad.exe".into(), "code.exe".into(),])
        );
        assert_eq!(
            config.switch_windows_denylist,
            HashSet::from(["game.exe".into()])
        );
        assert!(!config.switch_apps_enable);
    }

    #[test]
    fn invalid_cycle_option_is_reported() {
        let ini = Ini::load_from_str("[switch-windows]\npersistent_cycle = maybe").unwrap();
        assert!(Config::load(&ini).is_err());
    }

    #[test]
    fn legacy_denylist_name_is_still_accepted() {
        let ini = Ini::load_from_str("[switch-windows]\nblacklist = GAME.EXE, ").unwrap();
        assert_eq!(
            Config::load(&ini).unwrap().switch_windows_denylist,
            HashSet::from(["game.exe".into()])
        );
    }

    #[test]
    fn explicit_denylist_overrides_legacy_name_even_when_empty() {
        for (value, expected) in [
            ("new.exe", HashSet::from(["new.exe".into()])),
            ("", HashSet::new()),
        ] {
            let ini = Ini::load_from_str(&format!(
                "[switch-windows]\nblacklist = old.exe\ndenylist = {value}"
            ))
            .unwrap();
            assert_eq!(
                Config::load(&ini).unwrap().switch_windows_denylist,
                expected
            );
        }
    }

    #[test]
    fn cycle_option_is_independent_of_hotkey() {
        for hotkey in ["alt", "alt+`", "ctrl+space", "win+tab"] {
            for (value, expected) in [("yes", true), ("no", false)] {
                let ini = Ini::load_from_str(&format!(
                    "[switch-windows]\nhotkey = {hotkey}\npersistent_cycle = {value}"
                ))
                .unwrap();
                let config = Config::load(&ini).unwrap();
                assert_eq!(config.switch_windows_persistent_cycle, expected);
                assert_eq!(
                    config.switch_windows_hotkey,
                    vec![
                        Hotkey::create(SWITCH_WINDOWS_HOTKEY_ID, "switch windows", hotkey).unwrap()
                    ],
                );
            }
        }
    }

    #[test]
    fn manual_slots_are_opt_in_and_independent_of_cycling() {
        for (value, expected) in [("yes", true), ("no", false)] {
            let ini = Ini::load_from_str(&format!(
                "[switch-windows]\nmanual_slots = {value}\nallowlist = code.exe",
            ))
            .unwrap();
            let config = Config::load(&ini).unwrap();
            assert_eq!(config.switch_windows_manual_slots, expected);
            assert!(!config.switch_windows_persistent_cycle);
            assert_eq!(
                config.switch_windows_hotkey,
                Config::default().switch_windows_hotkey
            );
            assert_eq!(
                config.switch_windows_allowlist,
                HashSet::from(["code.exe".into()])
            );
        }
        assert!(!Config::default().switch_windows_manual_slots);
    }

    #[test]
    fn invalid_manual_slot_option_is_reported() {
        let ini = Ini::load_from_str("[switch-windows]\nmanual_slots = maybe").unwrap();
        assert!(Config::load(&ini).is_err());
    }
}
