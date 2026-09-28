#[derive(Debug, Default)]
pub struct AltTap {
    held: bool,
    foreground: Option<isize>,
}

impl AltTap {
    pub fn cancel(&mut self) {
        self.foreground = None;
    }

    pub fn update(
        &mut self,
        left_alt: bool,
        pressed: bool,
        foreground: Option<isize>,
        other_keys_down: bool,
    ) -> bool {
        if !left_alt {
            self.cancel();
            return false;
        }
        if pressed {
            if !self.held {
                self.held = true;
                self.foreground = if other_keys_down { None } else { foreground };
            }
            false
        } else {
            self.held = false;
            let start = self.foreground.take();
            start.is_some() && start == foreground && !other_keys_down
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_fires_once_on_release_not_on_repeat() {
        let mut tap = AltTap::default();
        for _ in 0..3 {
            assert!(!tap.update(true, true, Some(1), false));
            assert!(!tap.update(true, true, Some(1), false));
            assert!(tap.update(true, false, Some(1), false));
            assert!(!tap.update(true, false, Some(1), false));
        }
    }

    #[test]
    fn another_key_cancels_even_if_released_before_alt() {
        let mut tap = AltTap::default();
        assert!(!tap.update(true, true, Some(1), false));
        assert!(!tap.update(false, true, None, false));
        assert!(!tap.update(false, false, None, false));
        assert!(!tap.update(true, true, Some(1), false));
        assert!(!tap.update(true, false, Some(1), false));
    }

    #[test]
    fn already_held_keys_and_mouse_buttons_disqualify_tap() {
        let mut tap = AltTap::default();
        assert!(!tap.update(true, true, Some(1), true));
        assert!(!tap.update(true, false, Some(1), false));
        assert!(!tap.update(true, true, Some(1), false));
        tap.cancel();
        assert!(!tap.update(true, false, Some(1), false));
        assert!(!tap.update(true, true, Some(1), false));
        assert!(!tap.update(true, false, Some(1), true));
    }

    #[test]
    fn excluded_apps_and_changed_foreground_disqualify_tap() {
        let mut tap = AltTap::default();
        assert!(!tap.update(true, true, None, false));
        assert!(!tap.update(true, false, Some(1), false));
        assert!(!tap.update(true, true, Some(1), false));
        assert!(!tap.update(true, false, None, false));
        assert!(!tap.update(true, true, Some(1), false));
        assert!(!tap.update(true, false, Some(2), false));
    }

    #[test]
    fn right_alt_and_injected_alt_do_not_arm_a_tap() {
        let mut tap = AltTap::default();
        assert!(!tap.update(false, true, Some(1), false));
        assert!(!tap.update(false, false, Some(1), false));
        assert!(!tap.update(true, false, Some(1), false));
    }
}
