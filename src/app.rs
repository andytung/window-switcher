use crate::config::{edit_config_file, Config};
use crate::foreground::{is_window_allowed, ForegroundWatcher};
use crate::keyboard::KeyboardListener;
use crate::painter::GdiAAPainter;
use crate::startup::Startup;
use crate::trayicon::TrayIcon;
use crate::utils::{
    check_error, get_app_icon, get_foreground_window, get_window_pid, get_window_user_data,
    is_iconic_window, is_running_as_admin, list_windows, set_foreground_window,
    set_window_user_data,
};
use crate::window_cycle::WindowCycle;
use crate::window_slots::{SlotWindow, WindowSlots, SLOT_COUNT};

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use windows::core::{w, PCWSTR};
use windows::Win32::{
    Foundation::{GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyIcon, DispatchMessageW, GetMessageW, GetPropW,
        GetWindowLongPtrW, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW,
        RegisterWindowMessageW, RemovePropW, SetPropW, SetWindowLongPtrW, TranslateMessage,
        CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWL_STYLE, HICON, HTCLIENT, IDC_ARROW, MSG,
        WINDOW_STYLE, WM_COMMAND, WM_ERASEBKGND, WM_LBUTTONUP, WM_NCHITTEST, WM_RBUTTONUP,
        WNDCLASSW, WS_CAPTION, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    },
};

pub const NAME: PCWSTR = w!("Window Switcher");
pub const WM_USER_TRAYICON: u32 = 6000;
pub const WM_USER_REGISTER_TRAYICON: u32 = 6001;
pub const WM_USER_SWITCH_APPS: u32 = 6010;
pub const WM_USER_SWITCH_APPS_DONE: u32 = 6011;
pub const WM_USER_SWITCH_APPS_CANCEL: u32 = 6012;
pub const WM_USER_SWITCH_WINDOWS: u32 = 6020;
pub const WM_USER_SWITCH_WINDOWS_DONE: u32 = 6021;
pub const WM_USER_SWITCH_WINDOWS_TAP: u32 = 6022;
pub const WM_USER_WINDOW_SLOT: u32 = 6023;
const WINDOW_SLOT_PROPERTY: PCWSTR = w!("WindowSwitcher.ManualSlotIdentity");
pub const IDM_EXIT: u32 = 1;
pub const IDM_STARTUP: u32 = 2;
pub const IDM_CONFIGURE: u32 = 3;

pub fn start(config: &Config) -> Result<()> {
    info!("start config={config:?}");
    App::start(config)
}

/// Listen to this message to recreate the tray icon since the taskbar has been recreated.
static mut WM_TASKBARCREATED: u32 = 0;

pub struct App {
    hwnd: HWND,
    is_admin: bool,
    trayicon: Option<TrayIcon>,
    startup: Startup,
    config: Config,
    switch_windows_state: WindowCycle,
    window_slots: WindowSlots,
    next_slot_marker: usize,
    switch_apps_state: Option<SwitchAppsState>,
    cached_icons: HashMap<String, HICON>,
    painter: GdiAAPainter,
}

impl App {
    pub fn start(config: &Config) -> Result<()> {
        let hwnd = Self::create_window()?;
        let painter = GdiAAPainter::new(hwnd)?;

        let _foreground_watcher = ForegroundWatcher::init(
            &config.switch_windows_denylist,
            &config.switch_windows_allowlist,
        )?;
        let _keyboard_listener = KeyboardListener::init(
            hwnd,
            &config.to_hotkeys(),
            config.switch_windows_manual_slots,
        )?;

        let trayicon = match config.trayicon {
            true => Some(TrayIcon::create()),
            false => None,
        };

        let is_admin = is_running_as_admin()?;
        debug!("is_admin {is_admin}");

        let startup = Startup::init(is_admin)?;

        let mut app = App {
            hwnd,
            is_admin,
            trayicon,
            startup,
            config: config.clone(),
            switch_windows_state: WindowCycle::default(),
            window_slots: WindowSlots::default(),
            next_slot_marker: 0,
            switch_apps_state: None,
            cached_icons: Default::default(),
            painter,
        };

        app.set_trayicon();

        let app_ptr = Box::into_raw(Box::new(app)) as _;
        check_error(|| set_window_user_data(hwnd, app_ptr))
            .map_err(|err| anyhow!("Failed to set window ptr, {err}"))?;

        Self::eventloop()
    }

    fn eventloop() -> Result<()> {
        let mut message = MSG::default();
        loop {
            let ret = unsafe { GetMessageW(&mut message, None, 0, 0) };
            match ret.0 {
                -1 => {
                    unsafe { GetLastError() }.ok()?;
                }
                0 => break,
                _ => unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                },
            }
        }

        Ok(())
    }

    fn create_window() -> Result<HWND> {
        unsafe { WM_TASKBARCREATED = RegisterWindowMessageW(w!("TaskbarCreated")) };

        let hinstance = unsafe { GetModuleHandleW(None) }
            .map_err(|err| anyhow!("Failed to get current module handle, {err}"))?;

        let hcursor = unsafe { LoadCursorW(None, IDC_ARROW) }
            .map_err(|err| anyhow!("Failed to load arrow cursor, {err}"))?;

        let window_class = WNDCLASSW {
            hCursor: hcursor,
            hInstance: HINSTANCE(hinstance.0),
            lpszClassName: NAME,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(App::window_proc),
            ..Default::default()
        };

        let atom = check_error(|| unsafe { RegisterClassW(&window_class) })
            .map_err(|err| anyhow!("Failed to register class, {err}"))?;

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                PCWSTR(atom as _),
                NAME,
                WINDOW_STYLE(0),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
        }
        .map_err(|err| anyhow!("Failed to create windows, {err}"))?;

        // hide caption
        let mut style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
        style &= !WS_CAPTION.0;
        unsafe { SetWindowLongPtrW(hwnd, GWL_STYLE, style as _) };

        Ok(hwnd)
    }

    fn set_trayicon(&mut self) {
        if let Some(trayicon) = self.trayicon.as_mut() {
            match trayicon.register(self.hwnd) {
                Ok(()) => info!("trayicon registered"),
                Err(err) => {
                    if !trayicon.exist() {
                        error!("{err}, retrying in 3 second");
                        let hwnd = self.hwnd.0 as isize;
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            let _ = unsafe {
                                PostMessageW(
                                    Some(HWND(hwnd as _)),
                                    WM_USER_REGISTER_TRAYICON,
                                    WPARAM(0),
                                    LPARAM(0),
                                )
                            };
                        });
                    }
                }
            }
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match Self::handle_message(hwnd, msg, wparam, lparam) {
            Ok(ret) => ret,
            Err(err) => {
                error!("{err}");
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    }

    fn handle_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Result<LRESULT> {
        match msg {
            WM_USER_TRAYICON => {
                let app = get_app(hwnd)?;
                if let Some(trayicon) = app.trayicon.as_mut() {
                    let keycode = lparam.0 as u32;
                    if keycode == WM_LBUTTONUP || keycode == WM_RBUTTONUP {
                        trayicon.show(app.startup.is_enable)?;
                    }
                }
                return Ok(LRESULT(0));
            }
            WM_USER_SWITCH_APPS => {
                debug!("message WM_USER_SWITCH_APPS");
                let app = get_app(hwnd)?;
                let reverse = lparam.0 == 1;
                app.switch_apps(reverse)?;
                if let Some(state) = &app.switch_apps_state {
                    app.painter.paint(state);
                }
            }
            WM_USER_SWITCH_APPS_DONE => {
                debug!("message WM_USER_SWITCH_APPS_DONE");
                let app = get_app(hwnd)?;
                app.do_switch_app();
            }
            WM_USER_SWITCH_APPS_CANCEL => {
                debug!("message WM_USER_SWITCH_APPS_CANCEL");
                let app = get_app(hwnd)?;
                app.cancel_switch_app();
            }
            WM_USER_SWITCH_WINDOWS => {
                debug!("message WM_USER_SWITCH_WINDOWS");
                let app = get_app(hwnd)?;
                let reverse = lparam.0 == 1;
                let hwnd = app
                    .switch_apps_state
                    .as_ref()
                    .and_then(|state| state.apps.get(state.index).map(|(_, id)| *id))
                    .unwrap_or_else(get_foreground_window);
                app.switch_windows(hwnd, reverse)?;
                app.cancel_switch_app();
            }
            WM_USER_SWITCH_WINDOWS_DONE => {
                debug!("message WM_USER_SWITCH_WINDOWS_DONE");
                let app = get_app(hwnd)?;
                app.switch_windows_state.modifier_released = true;
            }
            WM_USER_SWITCH_WINDOWS_TAP => {
                let app = get_app(hwnd)?;
                let source = HWND(wparam.0 as _);
                if get_foreground_window() == source {
                    app.switch_windows(source, false)?;
                    app.switch_windows_state.modifier_released = true;
                }
            }
            WM_USER_WINDOW_SLOT => {
                let app = get_app(hwnd)?;
                let handled = app.window_slot(wparam.0, lparam.0 == 1)?;
                return Ok(LRESULT(isize::from(handled)));
            }
            WM_NCHITTEST => {
                return Ok(LRESULT(HTCLIENT as _));
            }
            WM_LBUTTONUP => {
                let app = get_app(hwnd)?;
                app.click();
            }
            WM_COMMAND => {
                let value = wparam.0 as u32;
                let kind = ((value >> 16) & 0xffff) as u16;
                let id = value & 0xffff;
                if kind == 0 {
                    match id {
                        IDM_EXIT => {
                            if let Ok(app) = get_app(hwnd) {
                                unsafe { drop(Box::from_raw(app)) }
                            }
                            unsafe { PostQuitMessage(0) }
                        }
                        IDM_STARTUP => {
                            let app = get_app(hwnd)?;
                            app.startup.toggle()?;
                        }
                        IDM_CONFIGURE => {
                            if let Err(err) = edit_config_file() {
                                alert!("{err}");
                            }
                        }
                        _ => {}
                    }
                }
            }
            WM_ERASEBKGND => {
                return Ok(LRESULT(0));
            }
            _ if msg == WM_USER_REGISTER_TRAYICON || unsafe { msg == WM_TASKBARCREATED } => {
                let app = get_app(hwnd)?;
                app.set_trayicon();
            }
            _ => {}
        }
        Ok(unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) })
    }

    fn window_slot(&mut self, slot: usize, assign: bool) -> Result<bool> {
        if slot >= SLOT_COUNT {
            return Err(anyhow!("Invalid window slot: {slot}"));
        }
        if !self.config.switch_windows_manual_slots || self.switch_apps_state.is_some() {
            return Ok(false);
        }
        self.window_slots.retain_live(is_slot_window_alive);
        if !assign && self.window_slots.windows().next().is_none() {
            return Ok(false);
        }
        let foreground = get_foreground_window();
        if !is_window_allowed(foreground) {
            return Ok(false);
        }
        let windows = list_windows(
            self.config.switch_windows_ignore_minimal,
            self.config.switch_windows_only_current_desktop(),
            self.is_admin,
        )?;
        let Some((app, app_windows)) = windows
            .iter()
            .find(|(_, windows)| windows.iter().any(|(id, _)| *id == foreground))
        else {
            debug!("Foreground window is not eligible for manual slots");
            return Ok(false);
        };
        let target = if assign {
            foreground
        } else if let Some(window) = self.window_slots.get(app, slot) {
            HWND(window.handle as _)
        } else {
            return Ok(false);
        };
        if !is_window_allowed(target) || !app_windows.iter().any(|(id, _)| *id == target) {
            debug!("Window slot F{} target is not currently eligible", slot + 1);
            return Ok(false);
        }

        if assign {
            let existing = self
                .window_slots
                .windows()
                .find(|window| window.handle == foreground.0 as isize);
            let window = match existing {
                Some(window) => window,
                None => {
                    self.next_slot_marker = self
                        .next_slot_marker
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("Window slot identity counter exhausted"))?;
                    let window = SlotWindow {
                        handle: foreground.0 as isize,
                        process_id: get_window_pid(foreground),
                        marker: self.next_slot_marker,
                    };
                    // Window properties disappear with the window, unlike reusable HWND values.
                    unsafe {
                        SetPropW(
                            foreground,
                            WINDOW_SLOT_PROPERTY,
                            Some(HANDLE(window.marker as _)),
                        )
                    }
                    .map_err(|err| anyhow!("Failed to assign window to F{}, {err}", slot + 1))?;
                    window
                }
            };
            if !is_slot_window_alive(window) {
                return Err(anyhow!("Window closed while assigning F{}", slot + 1));
            }
            if let Some(previous) = self.window_slots.toggle_assignment(app, slot, window) {
                if !self.window_slots.windows().any(|window| window == previous) {
                    remove_slot_window_property(previous);
                }
            }
            if self.window_slots.get(app, slot).is_some() {
                info!("assigned {app} F{} to {foreground:?}", slot + 1);
            } else {
                info!("unassigned {app} F{} from {foreground:?}", slot + 1);
            }
        } else {
            // Recheck after enumeration: closing a window must never redirect a slot to a reused HWND.
            let Some(window) = self.window_slots.get(app, slot) else {
                return Ok(false);
            };
            if !is_slot_window_alive(window) {
                self.window_slots.retain_live(is_slot_window_alive);
                return Ok(false);
            }
            if get_foreground_window() != target && !set_foreground_window(target) {
                error!("Failed to focus {app} window assigned to F{}", slot + 1);
            }
            self.switch_windows_state = WindowCycle::default();
        }
        Ok(true)
    }

    fn switch_windows(&mut self, hwnd: HWND, reverse: bool) -> Result<bool> {
        if !is_window_allowed(hwnd) {
            self.switch_windows_state = WindowCycle::default();
            return Ok(false);
        }
        let windows = list_windows(
            self.config.switch_windows_ignore_minimal,
            self.config.switch_windows_only_current_desktop(),
            self.is_admin,
        )?;
        debug!(
            "switch windows: hwnd:{hwnd:?} reverse:{reverse} state:{:?}",
            self.switch_windows_state
        );
        let module_path = match windows
            .iter()
            .find(|(_, v)| v.iter().any(|(id, _)| *id == hwnd))
            .map(|(k, _)| k.clone())
        {
            Some(v) => v,
            None => {
                self.switch_windows_state = WindowCycle::default();
                return Ok(false);
            }
        };
        match windows.get(&module_path) {
            None => Ok(false),
            Some(windows) => {
                let window_ids: Vec<isize> = windows.iter().map(|(id, _)| id.0 as isize).collect();
                let Some(target) = self.switch_windows_state.next(
                    &module_path,
                    &window_ids,
                    hwnd.0 as isize,
                    reverse,
                    self.config.switch_windows_persistent_cycle,
                ) else {
                    return Ok(false);
                };
                set_foreground_window(HWND(target as _));

                Ok(true)
            }
        }
    }

    fn switch_apps(&mut self, reverse: bool) -> Result<()> {
        debug!(
            "switch apps: reverse:{reverse}, state:{:?}",
            self.switch_apps_state
        );
        if let Some(state) = self.switch_apps_state.as_mut() {
            if reverse {
                if state.index == 0 {
                    state.index = state.apps.len() - 1;
                } else {
                    state.index -= 1;
                }
            } else if state.index == state.apps.len() - 1 {
                state.index = 0;
            } else {
                state.index += 1;
            };
            debug!("switch apps: new index:{}", state.index);
            return Ok(());
        }
        let windows = list_windows(
            self.config.switch_apps_ignore_minimal,
            self.config.switch_apps_only_current_desktop(),
            self.is_admin,
        )?;
        let mut apps = vec![];
        for (module_path, hwnds) in windows.iter() {
            let module_hwnd = if is_iconic_window(hwnds[0].0) {
                hwnds[hwnds.len() - 1].0
            } else {
                hwnds[0].0
            };
            let module_hicon = self
                .cached_icons
                .entry(module_path.clone())
                .or_insert_with(|| {
                    get_app_icon(
                        &self.config.switch_apps_override_icons,
                        module_path,
                        module_hwnd,
                    )
                });
            apps.push((*module_hicon, module_hwnd));
        }
        let num_apps = apps.len() as i32;
        if num_apps == 0 {
            return Ok(());
        }

        let index = if apps.len() == 1 {
            0
        } else if reverse {
            apps.len() - 1
        } else {
            1
        };

        let state = SwitchAppsState { apps, index };
        self.switch_apps_state = Some(state);
        debug!("switch apps, new state:{:?}", self.switch_apps_state);
        Ok(())
    }

    fn click(&mut self) {
        if let Some(state) = self.switch_apps_state.as_mut() {
            if let Some(i) = self.painter.find_clicked_app_index(state) {
                state.index = i;
                self.do_switch_app();
            }
        }
    }

    fn do_switch_app(&mut self) {
        if let Some(state) = self.switch_apps_state.take() {
            if let Some((_, id)) = state.apps.get(state.index) {
                set_foreground_window(*id);
            }
            self.painter.unpaint(state);
        }
    }

    fn cancel_switch_app(&mut self) {
        if let Some(state) = self.switch_apps_state.take() {
            self.painter.unpaint(state);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for window in self.window_slots.windows() {
            remove_slot_window_property(window);
        }
        for (_, icon) in self.cached_icons.drain() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

fn is_slot_window_alive(window: SlotWindow) -> bool {
    let hwnd = HWND(window.handle as _);
    window.marker != 0
        && get_window_pid(hwnd) == window.process_id
        && unsafe { GetPropW(hwnd, WINDOW_SLOT_PROPERTY).0 as usize == window.marker }
}

fn remove_slot_window_property(window: SlotWindow) {
    if is_slot_window_alive(window) {
        if let Err(err) = unsafe { RemovePropW(HWND(window.handle as _), WINDOW_SLOT_PROPERTY) } {
            error!("Failed to remove window slot identity, {err}");
        }
    }
}

fn get_app(hwnd: HWND) -> Result<&'static mut App> {
    unsafe {
        let ptr = check_error(|| get_window_user_data(hwnd))
            .map_err(|err| anyhow!("Failed to get window ptr, {err}"))?;
        let tx: &mut App = &mut *(ptr as *mut App);
        Ok(tx)
    }
}

#[derive(Debug)]
pub struct SwitchAppsState {
    pub apps: Vec<(HICON, HWND)>,
    pub index: usize,
}

#[cfg(test)]
mod window_slot_tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

    fn create_window() -> HWND {
        unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("Window slot identity test"),
                WINDOW_STYLE(0),
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap()
    }

    #[test]
    fn slot_identity_expires_when_the_window_closes() {
        let hwnd = create_window();
        let window = SlotWindow {
            handle: hwnd.0 as isize,
            process_id: get_window_pid(hwnd),
            marker: 1,
        };
        unsafe { SetPropW(hwnd, WINDOW_SLOT_PROPERTY, Some(HANDLE(window.marker as _))) }.unwrap();
        assert!(is_slot_window_alive(window));
        unsafe { DestroyWindow(hwnd) }.unwrap();
        assert!(!is_slot_window_alive(window));
    }

    #[test]
    fn stale_identity_cannot_match_or_remove_a_new_window_identity() {
        let hwnd = create_window();
        let old = SlotWindow {
            handle: hwnd.0 as isize,
            process_id: get_window_pid(hwnd),
            marker: 1,
        };
        let new = SlotWindow { marker: 2, ..old };
        unsafe { SetPropW(hwnd, WINDOW_SLOT_PROPERTY, Some(HANDLE(new.marker as _))) }.unwrap();
        assert!(!is_slot_window_alive(old));
        remove_slot_window_property(old);
        assert!(is_slot_window_alive(new));
        remove_slot_window_property(new);
        assert!(!is_slot_window_alive(new));
        unsafe { DestroyWindow(hwnd) }.unwrap();
    }
}
