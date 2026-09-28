use crate::utils::{get_foreground_window, get_window_exe};
use anyhow::{bail, Result};
use once_cell::sync::OnceCell;
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
        WindowsAndMessaging::{
            EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        },
    },
};

static IS_FOREGROUND_ALLOWED: AtomicBool = AtomicBool::new(true);

static APP_FILTER: OnceCell<AppFilter> = OnceCell::new();

#[derive(Debug)]
struct AppFilter {
    denylist: HashSet<String>,
    allowlist: HashSet<String>,
}

impl AppFilter {
    fn is_empty(&self) -> bool {
        self.denylist.is_empty() && self.allowlist.is_empty()
    }

    fn allows(&self, exe: Option<&str>) -> bool {
        if self.is_empty() {
            return true;
        }
        let Some(exe) = exe else {
            return false;
        };
        let exe = exe.to_lowercase();
        !self.denylist.contains(&exe)
            && (self.allowlist.is_empty() || self.allowlist.contains(&exe))
    }
}

pub fn is_window_allowed(hwnd: HWND) -> bool {
    match APP_FILTER.get() {
        Some(filter) if !filter.is_empty() => filter.allows(get_window_exe(hwnd).as_deref()),
        _ => true,
    }
}

pub fn is_foreground_allowed() -> bool {
    // WinEvent delivery can lag the keyboard hook, including at startup.
    update_foreground(get_foreground_window());
    IS_FOREGROUND_ALLOWED.load(Ordering::Relaxed)
}

fn update_foreground(hwnd: HWND) {
    let allowed = is_window_allowed(hwnd);
    IS_FOREGROUND_ALLOWED.store(allowed, Ordering::Relaxed);
    debug!("foreground {hwnd:?} allowed:{allowed}");
}

#[derive(Debug)]
pub struct ForegroundWatcher {
    hook: HWINEVENTHOOK,
}

impl ForegroundWatcher {
    pub fn init(denylist: &HashSet<String>, allowlist: &HashSet<String>) -> Result<Self> {
        let filter = AppFilter {
            denylist: denylist.iter().map(|v| v.to_lowercase()).collect(),
            allowlist: allowlist.iter().map(|v| v.to_lowercase()).collect(),
        };
        let empty = filter.is_empty();
        APP_FILTER
            .set(filter)
            .map_err(|_| anyhow::anyhow!("App filter already initialized"))?;
        update_foreground(get_foreground_window());
        if empty {
            return Ok(Self {
                hook: HWINEVENTHOOK::default(),
            });
        }

        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_invalid() {
            bail!("Failed to watch foreground");
        }

        info!("foreground watcher start");

        Ok(Self { hook })
    }
}

impl Drop for ForegroundWatcher {
    fn drop(&mut self) {
        debug!("foreground watcher destroyed");
        if !self.hook.is_invalid() {
            unsafe {
                let _ = UnhookWinEvent(self.hook);
            }
        }
    }
}

unsafe extern "system" fn win_event_proc(
    _h_win_event_hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _dw_event_thread: u32,
    _dwms_event_time: u32,
) {
    update_foreground(hwnd);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(allowlist: &[&str], denylist: &[&str]) -> AppFilter {
        AppFilter {
            allowlist: allowlist.iter().map(|v| v.to_string()).collect(),
            denylist: denylist.iter().map(|v| v.to_string()).collect(),
        }
    }

    #[test]
    fn empty_lists_preserve_unrestricted_switching() {
        let filter = filter(&[], &[]);
        assert!(filter.allows(Some("any.exe")));
        assert!(filter.allows(None));
    }

    #[test]
    fn allowlist_is_exact_case_insensitive_and_fails_closed() {
        let filter = filter(&["notepad.exe"], &[]);
        assert!(filter.allows(Some("NOTEPAD.EXE")));
        assert!(!filter.allows(Some("other-notepad.exe")));
        assert!(!filter.allows(Some("code.exe")));
        assert!(!filter.allows(None));
    }

    #[test]
    fn denylist_wins_and_still_works_without_allowlist() {
        let both = filter(&["notepad.exe", "code.exe"], &["code.exe"]);
        assert!(both.allows(Some("notepad.exe")));
        assert!(!both.allows(Some("CODE.EXE")));
        let denylist = filter(&[], &["game.exe"]);
        assert!(denylist.allows(Some("notepad.exe")));
        assert!(!denylist.allows(Some("GAME.EXE")));
    }
}
