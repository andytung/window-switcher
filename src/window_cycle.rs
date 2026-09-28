#[derive(Debug)]
pub struct WindowCycle {
    cache: Option<(String, isize, usize, Vec<isize>)>,
    pub modifier_released: bool,
}

impl Default for WindowCycle {
    fn default() -> Self {
        Self {
            cache: None,
            modifier_released: true,
        }
    }
}

impl WindowCycle {
    pub fn next(
        &mut self,
        module_path: &str,
        windows: &[isize],
        foreground: isize,
        reverse: bool,
        persistent: bool,
    ) -> Option<isize> {
        if windows.len() < 2 || !windows.contains(&foreground) {
            *self = Self::default();
            return None;
        }
        let current_id = if persistent { foreground } else { windows[0] };
        let mut index = 1;
        let mut state_id = current_id;
        let mut state_windows = Vec::new();

        if windows.len() > 2 || persistent {
            if let Some((cache_module_path, cache_id, cache_index, cache_windows)) = &self.cache {
                // A different foreground window starts a fresh persistent cycle.
                if cache_module_path == module_path
                    && (!persistent || cache_windows.get(*cache_index) == Some(&foreground))
                {
                    if self.modifier_released && !persistent {
                        if *cache_id != current_id {
                            if let Some(i) = windows.iter().position(|id| id == cache_id) {
                                index = i;
                            }
                        }
                    } else {
                        state_id = *cache_id;
                        state_windows.extend(
                            cache_windows
                                .iter()
                                .copied()
                                .filter(|id| windows.contains(id)),
                        );
                        for id in windows {
                            if !state_windows.contains(id) {
                                state_windows.push(*id);
                            }
                        }
                        // Re-anchor after removing closed windows, rather than reusing an index.
                        let anchor = state_windows.iter().position(|id| *id == foreground)?;
                        index = step(anchor, windows.len(), reverse);
                    }
                }
            }
        }
        if state_windows.is_empty() {
            state_windows = windows.to_vec();
            if persistent {
                index = step(
                    windows.iter().position(|id| *id == foreground)?,
                    windows.len(),
                    reverse,
                );
            }
        }
        let target = state_windows[index];
        self.cache = Some((module_path.to_string(), state_id, index, state_windows));
        self.modifier_released = false;
        Some(target)
    }
}

fn step(index: usize, len: usize, reverse: bool) -> usize {
    if reverse {
        if index == 0 {
            len - 1
        } else {
            index - 1
        }
    } else if index >= len - 1 {
        0
    } else {
        index + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(cycle: &mut WindowCycle, windows: &[isize], persistent: bool) -> Option<isize> {
        cycle.modifier_released = true;
        cycle.next("app", windows, windows[0], false, persistent)
    }

    #[test]
    fn persistent_cycle_survives_releases_and_z_order_changes() {
        let mut cycle = WindowCycle::default();
        assert_eq!(press(&mut cycle, &[1, 2, 3], true), Some(2));
        assert_eq!(press(&mut cycle, &[2, 1, 3], true), Some(3));
        assert_eq!(press(&mut cycle, &[3, 2, 1], true), Some(1));
        assert_eq!(press(&mut cycle, &[1, 3, 2], true), Some(2));
    }

    #[test]
    fn default_still_switches_to_previous_window_after_release() {
        let mut cycle = WindowCycle::default();
        assert_eq!(press(&mut cycle, &[1, 2, 3], false), Some(2));
        assert_eq!(press(&mut cycle, &[2, 1, 3], false), Some(1));
        assert_eq!(press(&mut cycle, &[1, 2, 3], false), Some(2));
    }

    #[test]
    fn default_still_cycles_while_modifier_is_held() {
        let mut cycle = WindowCycle::default();
        assert_eq!(cycle.next("app", &[1, 2, 3], 1, false, false), Some(2));
        assert_eq!(cycle.next("app", &[2, 1, 3], 2, false, false), Some(3));
        assert_eq!(cycle.next("app", &[3, 2, 1], 3, false, false), Some(1));
    }

    #[test]
    fn held_reverse_cycle_reanchors_when_multiple_windows_close() {
        let mut cycle = WindowCycle::default();
        for foreground in 1..=5 {
            let windows: Vec<_> = std::iter::once(foreground)
                .chain((1..=6).filter(|id| *id != foreground))
                .collect();
            assert_eq!(
                cycle.next("app", &windows, foreground, false, false),
                Some(foreground + 1)
            );
        }
        assert_eq!(cycle.next("app", &[6, 1, 2], 6, true, false), Some(2));
    }

    #[test]
    fn closed_windows_do_not_shift_the_cycle_anchor() {
        let mut cycle = WindowCycle::default();
        assert_eq!(press(&mut cycle, &[1, 2, 3, 4], true), Some(2));
        assert_eq!(press(&mut cycle, &[2, 3, 4], true), Some(3));
        assert_eq!(press(&mut cycle, &[3, 2], true), Some(2));
    }

    #[test]
    fn new_windows_are_appended_to_existing_order() {
        let mut cycle = WindowCycle::default();
        assert_eq!(press(&mut cycle, &[1, 2, 3], true), Some(2));
        assert_eq!(press(&mut cycle, &[2, 4, 1, 3], true), Some(3));
        assert_eq!(press(&mut cycle, &[3, 2, 4, 1], true), Some(4));
    }

    #[test]
    fn external_focus_and_app_changes_restart_cycle() {
        let mut cycle = WindowCycle::default();
        assert_eq!(press(&mut cycle, &[1, 2, 3], true), Some(2));
        assert_eq!(press(&mut cycle, &[3, 2, 1], true), Some(2));
        assert_eq!(cycle.next("other-app", &[2, 1, 3], 2, false, true), Some(1));
    }

    #[test]
    fn reverse_cycle_uses_actual_foreground_and_wraps() {
        let mut cycle = WindowCycle::default();
        assert_eq!(cycle.next("app", &[1, 2, 3], 1, true, true), Some(3));
        cycle.modifier_released = true;
        assert_eq!(cycle.next("app", &[3, 1, 2], 3, true, true), Some(2));
        assert_eq!(cycle.next("app", &[2, 3, 1], 2, false, true), Some(3));
    }

    #[test]
    fn first_switch_uses_actual_foreground_even_when_not_first_in_list() {
        let mut cycle = WindowCycle::default();
        assert_eq!(cycle.next("app", &[1, 2, 3], 2, false, true), Some(3));
    }

    #[test]
    fn closing_the_selected_window_restarts_from_the_new_foreground() {
        let mut cycle = WindowCycle::default();
        assert_eq!(press(&mut cycle, &[1, 2, 3, 4], true), Some(2));
        assert_eq!(press(&mut cycle, &[1, 3, 4], true), Some(3));
    }

    #[test]
    fn empty_single_and_missing_foreground_reset_safely() {
        let mut cycle = WindowCycle::default();
        assert_eq!(cycle.next("app", &[], 1, false, true), None);
        assert_eq!(cycle.next("app", &[1], 1, false, true), None);
        assert_eq!(cycle.next("app", &[1, 2], 3, false, true), None);
        assert_eq!(press(&mut cycle, &[1, 2], true), Some(2));
        assert_eq!(press(&mut cycle, &[2, 1], true), Some(1));
    }
}
