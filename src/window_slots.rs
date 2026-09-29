use std::collections::HashMap;

pub const SLOT_COUNT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotWindow {
    pub handle: isize,
    pub process_id: u32,
    pub marker: usize,
}

#[derive(Debug, Default)]
pub struct WindowSlots {
    apps: HashMap<String, HashMap<usize, SlotWindow>>,
}

impl WindowSlots {
    pub fn assign(&mut self, app: &str, slot: usize, window: SlotWindow) -> Option<SlotWindow> {
        self.apps
            .entry(app.to_lowercase())
            .or_default()
            .insert(slot, window)
    }

    pub fn toggle_assignment(
        &mut self,
        app: &str,
        slot: usize,
        window: SlotWindow,
    ) -> Option<SlotWindow> {
        let app = app.to_lowercase();
        if let Some(slots) = self.apps.get_mut(&app) {
            if slots.get(&slot) == Some(&window) {
                let previous = slots.remove(&slot);
                if slots.is_empty() {
                    self.apps.remove(&app);
                }
                return previous;
            }
        }
        self.assign(&app, slot, window)
    }

    pub fn get(&self, app: &str, slot: usize) -> Option<SlotWindow> {
        self.apps.get(&app.to_lowercase())?.get(&slot).copied()
    }

    pub fn windows(&self) -> impl Iterator<Item = SlotWindow> + '_ {
        self.apps.values().flat_map(|slots| slots.values().copied())
    }

    pub fn retain_live(&mut self, mut is_live: impl FnMut(SlotWindow) -> bool) {
        self.apps.retain(|_, slots| {
            slots.retain(|_, window| is_live(*window));
            !slots.is_empty()
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotAction {
    Recall,
    Assign,
}

impl SlotAction {
    pub fn from_modifiers(
        control: bool,
        left_alt: bool,
        right_alt: bool,
        shift: bool,
        win: bool,
    ) -> Option<Self> {
        if right_alt || shift || win {
            return None;
        }
        match (control, left_alt) {
            (false, false) => Some(Self::Recall),
            (true, true) => Some(Self::Assign),
            _ => None,
        }
    }
}

pub fn function_key_slot(virtual_key: u32) -> Option<usize> {
    virtual_key
        .checked_sub(0x70)
        .filter(|slot| *slot < SLOT_COUNT as u32)
        .map(|slot| slot as usize)
}

#[derive(Debug, Default)]
pub struct SlotKeyState {
    pressed: [Option<bool>; SLOT_COUNT],
}

impl SlotKeyState {
    pub fn existing_decision(&mut self, slot: usize, down: bool) -> Option<bool> {
        if down {
            self.pressed[slot]
        } else {
            Some(self.pressed[slot].take().unwrap_or(false))
        }
    }

    pub fn remember(&mut self, slot: usize, handled: bool) {
        self.pressed[slot] = Some(handled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(handle: isize) -> SlotWindow {
        SlotWindow {
            handle,
            process_id: 10,
            marker: handle as usize,
        }
    }

    #[test]
    fn starts_empty_and_lookup_never_assigns() {
        let slots = WindowSlots::default();
        for slot in 0..SLOT_COUNT {
            assert_eq!(slots.get("app.exe", slot), None);
        }
        assert_eq!(slots.windows().count(), 0);
    }

    #[test]
    fn assignments_are_stable_and_per_app() {
        let mut slots = WindowSlots::default();
        slots.assign("app.exe", 0, window(1));
        slots.assign("app.exe", 1, window(2));
        slots.assign("other.exe", 0, window(3));
        for _ in 0..3 {
            assert_eq!(slots.get("APP.EXE", 1), Some(window(2)));
            assert_eq!(slots.get("app.exe", 0), Some(window(1)));
            assert_eq!(slots.get("other.exe", 0), Some(window(3)));
        }
    }

    #[test]
    fn browser_profiles_and_web_app_groups_have_independent_slots() {
        let mut slots = WindowSlots::default();
        slots.assign("chrome.exe::Default", 0, window(1));
        slots.assign("chrome.exe::Profile 1", 0, window(2));
        assert_eq!(slots.get("chrome.exe::Default", 0), Some(window(1)));
        assert_eq!(slots.get("chrome.exe::Profile 1", 0), Some(window(2)));
    }

    #[test]
    fn reassigning_replaces_only_the_requested_slot() {
        let mut slots = WindowSlots::default();
        slots.assign("app.exe", 0, window(1));
        slots.assign("app.exe", 1, window(2));
        assert_eq!(slots.assign("app.exe", 0, window(3)), Some(window(1)));
        assert_eq!(slots.get("app.exe", 0), Some(window(3)));
        assert_eq!(slots.get("app.exe", 1), Some(window(2)));
    }

    #[test]
    fn repeating_assignment_toggles_the_same_window_off_and_on() {
        let mut slots = WindowSlots::default();
        assert_eq!(slots.toggle_assignment("app.exe", 0, window(1)), None);
        assert_eq!(slots.get("app.exe", 0), Some(window(1)));
        assert_eq!(
            slots.toggle_assignment("APP.EXE", 0, window(1)),
            Some(window(1))
        );
        assert_eq!(slots.get("app.exe", 0), None);
        assert!(slots.apps.is_empty());
        assert_eq!(slots.toggle_assignment("app.exe", 0, window(1)), None);
        assert_eq!(slots.get("app.exe", 0), Some(window(1)));
    }

    #[test]
    fn toggling_a_different_window_replaces_the_assignment() {
        let mut slots = WindowSlots::default();
        slots.toggle_assignment("app.exe", 0, window(1));
        assert_eq!(
            slots.toggle_assignment("app.exe", 0, window(2)),
            Some(window(1))
        );
        assert_eq!(slots.get("app.exe", 0), Some(window(2)));
    }

    #[test]
    fn clearing_one_slot_preserves_other_slots_and_window_references() {
        let mut slots = WindowSlots::default();
        slots.assign("app.exe", 0, window(1));
        slots.assign("app.exe", 1, window(1));
        slots.assign("other.exe", 0, window(2));
        assert_eq!(
            slots.toggle_assignment("app.exe", 0, window(1)),
            Some(window(1))
        );
        assert_eq!(slots.get("app.exe", 0), None);
        assert_eq!(slots.get("app.exe", 1), Some(window(1)));
        assert_eq!(slots.get("other.exe", 0), Some(window(2)));
        assert!(slots.windows().any(|entry| entry == window(1)));
        slots.toggle_assignment("app.exe", 1, window(1));
        assert!(!slots.windows().any(|entry| entry == window(1)));
    }

    #[test]
    fn a_new_window_lifetime_is_reassigned_not_toggled_off() {
        let mut slots = WindowSlots::default();
        let old = window(1);
        let replacement = SlotWindow { marker: 99, ..old };
        slots.assign("app.exe", 0, old);
        assert_eq!(
            slots.toggle_assignment("app.exe", 0, replacement),
            Some(old)
        );
        assert_eq!(slots.get("app.exe", 0), Some(replacement));
    }

    #[test]
    fn key_repeat_does_not_toggle_an_assignment_multiple_times() {
        let mut slots = WindowSlots::default();
        let mut keys = SlotKeyState::default();
        for expected in [Some(window(1)), None] {
            assert_eq!(keys.existing_decision(0, true), None);
            slots.toggle_assignment("app.exe", 0, window(1));
            keys.remember(0, true);
            for _ in 0..5 {
                assert_eq!(keys.existing_decision(0, true), Some(true));
                assert_eq!(slots.get("app.exe", 0), expected);
            }
            assert_eq!(keys.existing_decision(0, false), Some(true));
        }
    }

    #[test]
    fn closing_a_window_clears_all_its_slots_without_renumbering() {
        let mut slots = WindowSlots::default();
        slots.assign("app.exe", 0, window(1));
        slots.assign("app.exe", 1, window(2));
        slots.assign("app.exe", 2, window(1));
        slots.retain_live(|window| window.handle != 1);
        assert_eq!(slots.get("app.exe", 0), None);
        assert_eq!(slots.get("app.exe", 1), Some(window(2)));
        assert_eq!(slots.get("app.exe", 2), None);
        slots.assign("app.exe", 3, window(3));
        assert_eq!(slots.get("app.exe", 0), None);
    }

    #[test]
    fn recycled_window_handles_do_not_inherit_assignments() {
        let mut slots = WindowSlots::default();
        let old = window(1);
        let replacement = SlotWindow { marker: 99, ..old };
        slots.assign("app.exe", 0, old);
        slots.retain_live(|window| window == replacement);
        assert_eq!(slots.get("app.exe", 0), None);
        slots.assign("app.exe", 0, replacement);
        assert_eq!(slots.get("app.exe", 0), Some(replacement));
    }

    #[test]
    fn independent_instances_do_not_restore_assignments() {
        let mut original = WindowSlots::default();
        original.assign("app.exe", 0, window(1));
        assert_eq!(WindowSlots::default().get("app.exe", 0), None);
    }

    #[test]
    fn recognizes_only_f1_through_f24() {
        assert_eq!(function_key_slot(0x70), Some(0));
        assert_eq!(function_key_slot(0x87), Some(23));
        for key in [0, 0x6f, 0x88, u32::MAX] {
            assert_eq!(function_key_slot(key), None);
        }
    }

    #[test]
    fn only_bare_function_keys_and_control_left_alt_are_slot_shortcuts() {
        assert_eq!(
            SlotAction::from_modifiers(false, false, false, false, false),
            Some(SlotAction::Recall)
        );
        assert_eq!(
            SlotAction::from_modifiers(true, true, false, false, false),
            Some(SlotAction::Assign)
        );
        for modifiers in [
            (true, false, false, false, false),
            (false, true, false, false, false),
            (true, false, true, false, false),
            (true, true, true, false, false),
            (false, false, false, true, false),
            (false, false, false, false, true),
            (true, true, false, true, false),
            (true, true, false, false, true),
        ] {
            assert_eq!(
                SlotAction::from_modifiers(
                    modifiers.0,
                    modifiers.1,
                    modifiers.2,
                    modifiers.3,
                    modifiers.4,
                ),
                None
            );
        }
    }

    #[test]
    fn handled_key_consumes_repeats_and_its_release() {
        let mut keys = SlotKeyState::default();
        assert_eq!(keys.existing_decision(0, true), None);
        keys.remember(0, true);
        assert_eq!(keys.existing_decision(0, true), Some(true));
        assert_eq!(keys.existing_decision(0, false), Some(true));
        assert_eq!(keys.existing_decision(0, true), None);
    }

    #[test]
    fn unhandled_press_stays_native_even_if_modifiers_or_foreground_change() {
        let mut keys = SlotKeyState::default();
        assert_eq!(keys.existing_decision(0, true), None);
        keys.remember(0, false);
        assert_eq!(keys.existing_decision(0, true), Some(false));
        assert_eq!(keys.existing_decision(0, false), Some(false));
        assert_eq!(keys.existing_decision(0, false), Some(false));
        assert_eq!(keys.existing_decision(0, true), None);
    }

    #[test]
    fn simultaneous_function_keys_keep_independent_consumption_state() {
        let mut keys = SlotKeyState::default();
        keys.remember(0, true);
        keys.remember(23, false);
        assert_eq!(keys.existing_decision(23, false), Some(false));
        assert_eq!(keys.existing_decision(0, false), Some(true));
    }
}
