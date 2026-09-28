use crate::{
    alt_tap::AltTap,
    app::{
        WM_USER_SWITCH_APPS, WM_USER_SWITCH_APPS_CANCEL, WM_USER_SWITCH_APPS_DONE,
        WM_USER_SWITCH_WINDOWS, WM_USER_SWITCH_WINDOWS_DONE, WM_USER_SWITCH_WINDOWS_TAP,
        WM_USER_WINDOW_SLOT,
    },
    config::{Hotkey, SWITCH_APPS_HOTKEY_ID, SWITCH_WINDOWS_HOTKEY_ID},
    foreground::{is_foreground_allowed, is_window_allowed},
    utils::get_foreground_window,
    window_slots::{function_key_slot, SlotAction, SlotKeyState},
};

use anyhow::{anyhow, Result};
use indexmap::IndexSet;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    LazyLock,
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
            KEYEVENTF_KEYUP, SCANCODE_LSHIFT, SCANCODE_RSHIFT, VIRTUAL_KEY, VK_CONTROL, VK_LMENU,
            VK_LWIN, VK_MENU, VK_RMENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, PostMessageW, SendMessageTimeoutW, SetWindowsHookExW,
            UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_UP,
            SMTO_ABORTIFHUNG, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
            WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
        },
    },
};

static KEYBOARD_STATE: LazyLock<Mutex<Vec<HotKeyState>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static ALT_TAP: LazyLock<Mutex<AltTap>> = LazyLock::new(|| Mutex::new(AltTap::default()));
static ALT_TAP_ENABLED: AtomicBool = AtomicBool::new(false);
static MANUAL_SLOTS_ENABLED: AtomicBool = AtomicBool::new(false);
static SLOT_KEYS: LazyLock<Mutex<SlotKeyState>> =
    LazyLock::new(|| Mutex::new(SlotKeyState::default()));
const MENU_MASK_TAG: usize = 0x57535450;
static mut WINDOW: HWND = HWND(0 as _);
static mut IS_SHIFT_PRESSED: bool = false;
static mut IS_SWITCHING_APPS: bool = false;
static mut PREVIOUS_KEYCODE: u32 = 0;

#[derive(Debug)]
pub struct KeyboardListener {
    hook: HHOOK,
    mouse_hook: HHOOK,
}

impl KeyboardListener {
    pub fn init(hwnd: HWND, hotkeys: &[&Hotkey], manual_slots: bool) -> Result<Self> {
        unsafe { WINDOW = hwnd }
        let alt_tap_enabled = hotkeys.iter().any(|hotkey| hotkey.code.is_none());
        ALT_TAP_ENABLED.store(alt_tap_enabled, Ordering::Relaxed);
        *ALT_TAP.lock() = AltTap::default();
        MANUAL_SLOTS_ENABLED.store(manual_slots, Ordering::Relaxed);
        *SLOT_KEYS.lock() = SlotKeyState::default();

        let keyboard_state = hotkeys
            .iter()
            .map(|hotkey| HotKeyState {
                hotkey: (*hotkey).clone(),
                is_modifier_pressed: false,
            })
            .collect();
        *KEYBOARD_STATE.lock() = keyboard_state;

        let hook = unsafe {
            let hinstance = { GetModuleHandleW(None) }
                .map_err(|err| anyhow!("Failed to get module handle, {err}"))?;
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                Some(hinstance.into()),
                0,
            )
        }
        .map_err(|err| anyhow!("Failed to set windows hook, {err}"))?;
        let mut listener = Self {
            hook,
            mouse_hook: HHOOK::default(),
        };
        if alt_tap_enabled {
            let hinstance = unsafe { GetModuleHandleW(None) }?;
            listener.mouse_hook = unsafe {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(hinstance.into()), 0)
            }
            .map_err(|err| anyhow!("Failed to watch mouse input for Alt taps, {err}"))?;
        }
        info!("keyboard listener start");

        Ok(listener)
    }
}

impl Drop for KeyboardListener {
    fn drop(&mut self) {
        debug!("keyboard listener destroyed");
        if !self.hook.is_invalid() {
            let _ = unsafe { UnhookWindowsHookEx(self.hook) };
        }
        if !self.mouse_hook.is_invalid() {
            let _ = unsafe { UnhookWindowsHookEx(self.mouse_hook) };
        }
    }
}

#[derive(Debug)]
struct HotKeyState {
    hotkey: Hotkey,
    is_modifier_pressed: bool,
}

unsafe fn send_message_timeout(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) {
    let mut result: usize = 0;
    let _ = SendMessageTimeoutW(
        hwnd,
        msg,
        wparam,
        lparam,
        SMTO_ABORTIFHUNG,
        500,
        Some(&mut result as *mut _ as *mut _),
    );
}

unsafe extern "system" fn keyboard_proc(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, w_param, l_param);
    }
    let kbd_data: &KBDLLHOOKSTRUCT = &*(l_param.0 as *const _);
    if kbd_data.dwExtraInfo == MENU_MASK_TAG {
        return CallNextHookEx(None, code, w_param, l_param);
    }
    debug!("keyboard {kbd_data:?}");
    let mut is_modifier = false;
    let scan_code = kbd_data.scanCode;
    let is_key_pressed = || kbd_data.flags.0 & LLKHF_UP.0 == 0;
    if ALT_TAP_ENABLED.load(Ordering::Relaxed) {
        let left_alt =
            kbd_data.vkCode == VK_LMENU.0 as u32 && kbd_data.flags.0 & LLKHF_INJECTED.0 == 0;
        let foreground = if left_alt {
            let hwnd = get_foreground_window();
            is_window_allowed(hwnd).then_some(hwnd.0 as isize)
        } else {
            None
        };
        let other_keys_down = left_alt
            && (1..=254).any(|key| {
                key != VK_MENU.0 as i32 && key != VK_LMENU.0 as i32 && GetAsyncKeyState(key) < 0
            });
        let tap = ALT_TAP
            .lock()
            .update(left_alt, is_key_pressed(), foreground, other_keys_down);
        if tap && mask_alt_menu() {
            // Defer switching until the original Alt-up has passed through the hook.
            if let Some(source) = foreground {
                if let Err(err) = PostMessageW(
                    Some(WINDOW),
                    WM_USER_SWITCH_WINDOWS_TAP,
                    WPARAM(source as usize),
                    LPARAM(0),
                ) {
                    error!("Failed to queue Alt-tap window switch, {err}");
                }
            }
        }
    }
    if [SCANCODE_LSHIFT, SCANCODE_RSHIFT].contains(&scan_code) {
        IS_SHIFT_PRESSED = is_key_pressed();
    }
    if MANUAL_SLOTS_ENABLED.load(Ordering::Relaxed) {
        if let Some(slot) = function_key_slot(kbd_data.vkCode) {
            if handle_slot_key(slot, is_key_pressed()) {
                return LRESULT(1);
            }
        }
    }
    let mut keyboard_state = KEYBOARD_STATE.lock();
    let mut send_done_hotkeys: IndexSet<u32> = IndexSet::new();
    let mut send_action_message: Option<(u32, isize, bool)> = None;

    for state in keyboard_state.iter_mut() {
        if state.hotkey.code.is_none() {
            continue;
        }
        if state.hotkey.modifier.contains(&scan_code) {
            is_modifier = true;
            if is_key_pressed() {
                state.is_modifier_pressed = true;
            } else {
                state.is_modifier_pressed = false;
                if Some(PREVIOUS_KEYCODE) == state.hotkey.code {
                    send_done_hotkeys.insert(state.hotkey.id);
                }
            }
        }
    }
    if !is_modifier {
        for state in keyboard_state.iter_mut() {
            if is_key_pressed() && state.is_modifier_pressed {
                let id = state.hotkey.id;
                if Some(scan_code) == state.hotkey.code {
                    let reverse = if IS_SHIFT_PRESSED { 1 } else { 0 };
                    if id == SWITCH_APPS_HOTKEY_ID
                        || (id == SWITCH_WINDOWS_HOTKEY_ID && is_foreground_allowed())
                    {
                        send_action_message = Some((id, reverse, false));
                        PREVIOUS_KEYCODE = scan_code;
                        break;
                    };
                } else if id == SWITCH_APPS_HOTKEY_ID {
                    if scan_code == 0x01 {
                        // escape key
                        send_action_message = Some((id, 0, true));
                        PREVIOUS_KEYCODE = scan_code;
                        break;
                    } else if [0x48, 0x4b, 0x4d, 0x50].contains(&scan_code) && IS_SWITCHING_APPS {
                        // arrow keys
                        let reverse = if scan_code == 0x48 || scan_code == 0x4b {
                            1
                        } else {
                            0
                        };
                        send_action_message = Some((id, reverse, false));
                        break;
                    }
                }
            }
        }
    }
    drop(keyboard_state);

    for id in send_done_hotkeys {
        if id == SWITCH_APPS_HOTKEY_ID {
            send_message_timeout(WINDOW, WM_USER_SWITCH_APPS_DONE, WPARAM(0), LPARAM(0));
            IS_SWITCHING_APPS = false;
        } else if id == SWITCH_WINDOWS_HOTKEY_ID {
            send_message_timeout(WINDOW, WM_USER_SWITCH_WINDOWS_DONE, WPARAM(0), LPARAM(0));
        }
    }

    if let Some((id, reverse, is_cancel)) = send_action_message {
        if id == SWITCH_APPS_HOTKEY_ID {
            if is_cancel {
                send_message_timeout(WINDOW, WM_USER_SWITCH_APPS_CANCEL, WPARAM(0), LPARAM(0));
                IS_SWITCHING_APPS = false;
            } else {
                send_message_timeout(WINDOW, WM_USER_SWITCH_APPS, WPARAM(0), LPARAM(reverse));
                IS_SWITCHING_APPS = true;
            }
            return LRESULT(1);
        } else if id == SWITCH_WINDOWS_HOTKEY_ID {
            send_message_timeout(WINDOW, WM_USER_SWITCH_WINDOWS, WPARAM(0), LPARAM(reverse));
            IS_SWITCHING_APPS = false;
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

unsafe fn handle_slot_key(slot: usize, down: bool) -> bool {
    if let Some(handled) = SLOT_KEYS.lock().existing_decision(slot, down) {
        return handled;
    }
    let action = SlotAction::from_modifiers(
        GetAsyncKeyState(VK_CONTROL.0 as i32) < 0,
        GetAsyncKeyState(VK_LMENU.0 as i32) < 0,
        GetAsyncKeyState(VK_RMENU.0 as i32) < 0,
        GetAsyncKeyState(VK_SHIFT.0 as i32) < 0,
        GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0,
    );
    let mut handled = false;
    if let Some(action) = action {
        if is_foreground_allowed() {
            let mut result = 0;
            let sent = SendMessageTimeoutW(
                WINDOW,
                WM_USER_WINDOW_SLOT,
                WPARAM(slot),
                LPARAM(isize::from(action == SlotAction::Assign)),
                SMTO_ABORTIFHUNG,
                500,
                Some(&mut result),
            );
            if sent.0 == 0 {
                error!("Failed to process manual window slot F{}", slot + 1);
            } else {
                handled = result != 0;
                if handled && action == SlotAction::Assign {
                    mask_alt_menu();
                }
            }
        }
    }
    SLOT_KEYS.lock().remember(slot, handled);
    handled
}

unsafe fn mask_alt_menu() -> bool {
    // An unassigned virtual key suppresses Alt's menu activation without a real shortcut.
    let key = KEYBDINPUT {
        wVk: VIRTUAL_KEY(0xe8),
        dwExtraInfo: MENU_MASK_TAG,
        ..Default::default()
    };
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: key },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    dwFlags: KEYEVENTF_KEYUP,
                    ..key
                },
            },
        },
    ];
    if SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) != inputs.len() as u32 {
        error!("Failed to suppress Alt menu activation");
        return false;
    }
    true
}

unsafe extern "system" fn mouse_proc(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code >= 0
        && matches!(
            w_param.0 as u32,
            WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_XBUTTONDOWN
                | WM_MOUSEWHEEL
                | WM_MOUSEHWHEEL
        )
    {
        ALT_TAP.lock().cancel();
    }
    CallNextHookEx(None, code, w_param, l_param)
}
