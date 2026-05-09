// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use super::ControllerAction;

pub(super) const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(350);
pub(super) const REPEAT_INTERVAL: Duration = Duration::from_millis(90);

#[derive(Debug, Default)]
pub(super) struct InputActions {
    pub(super) actions: Vec<ControllerAction>,
    pub(super) repeat_actions: Vec<ControllerAction>,
    pub(super) released: bool,
}

impl InputActions {
    pub(super) fn action(action: ControllerAction) -> Self {
        Self {
            actions: vec![action],
            repeat_actions: Vec::new(),
            released: false,
        }
    }

    pub(super) const fn released() -> Self {
        Self {
            actions: Vec::new(),
            repeat_actions: Vec::new(),
            released: true,
        }
    }

    pub(super) fn repeatable_press(actions: Vec<ControllerAction>) -> Self {
        Self {
            actions,
            repeat_actions: Vec::new(),
            released: false,
        }
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.actions.extend(other.actions);
        self.repeat_actions.extend(other.repeat_actions);
        self.released |= other.released;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum AxisDirection {
    Negative,
    #[default]
    Neutral,
    Positive,
}

impl AxisDirection {
    pub(super) fn update(
        &mut self,
        next: Self,
        action: impl Fn(Self) -> Option<ControllerAction>,
        repeater: &mut ActionRepeater,
        now: Instant,
    ) -> InputActions {
        if *self == next {
            return InputActions::default();
        }
        let mut input = InputActions::default();
        if let Some(action) = action(*self) {
            repeater.stop(action);
            input.released = true;
        }
        *self = next;
        if let Some(action) = action(next) {
            input.actions.extend(repeater.start(action, now));
        }
        input
    }
}

#[derive(Debug, Default)]
pub(super) struct ActionRepeater {
    up: HeldAction,
    down: HeldAction,
    left: HeldAction,
    right: HeldAction,
    scroll_preview_left: HeldAction,
    scroll_preview_right: HeldAction,
    scroll_preview_up: HeldAction,
    scroll_preview_down: HeldAction,
}

impl ActionRepeater {
    pub(super) fn start(
        &mut self,
        action: ControllerAction,
        now: Instant,
    ) -> Vec<ControllerAction> {
        let Some(held) = self.held_mut(action) else {
            return vec![action];
        };
        held.start(now);
        vec![action]
    }

    pub(super) fn stop(&mut self, action: ControllerAction) {
        if let Some(held) = self.held_mut(action) {
            held.stop();
        }
    }

    #[cfg(any(target_os = "linux", test))]
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn poll(&mut self, now: Instant) -> Vec<ControllerAction> {
        [
            (ControllerAction::Up, &mut self.up),
            (ControllerAction::Down, &mut self.down),
            (ControllerAction::Left, &mut self.left),
            (ControllerAction::Right, &mut self.right),
            (
                ControllerAction::ScrollPreviewLeft,
                &mut self.scroll_preview_left,
            ),
            (
                ControllerAction::ScrollPreviewRight,
                &mut self.scroll_preview_right,
            ),
            (
                ControllerAction::ScrollPreviewUp,
                &mut self.scroll_preview_up,
            ),
            (
                ControllerAction::ScrollPreviewDown,
                &mut self.scroll_preview_down,
            ),
        ]
        .into_iter()
        .filter_map(|(action, held)| held.poll(now).then_some(action))
        .collect()
    }

    pub(super) fn next_repeat(&self) -> Option<Instant> {
        [
            &self.up,
            &self.down,
            &self.left,
            &self.right,
            &self.scroll_preview_left,
            &self.scroll_preview_right,
            &self.scroll_preview_up,
            &self.scroll_preview_down,
        ]
        .into_iter()
        .filter_map(HeldAction::next_repeat)
        .min()
    }

    fn held_mut(&mut self, action: ControllerAction) -> Option<&mut HeldAction> {
        match action {
            ControllerAction::Up => Some(&mut self.up),
            ControllerAction::Down => Some(&mut self.down),
            ControllerAction::Left => Some(&mut self.left),
            ControllerAction::Right => Some(&mut self.right),
            ControllerAction::ScrollPreviewLeft => Some(&mut self.scroll_preview_left),
            ControllerAction::ScrollPreviewRight => Some(&mut self.scroll_preview_right),
            ControllerAction::ScrollPreviewUp => Some(&mut self.scroll_preview_up),
            ControllerAction::ScrollPreviewDown => Some(&mut self.scroll_preview_down),
            ControllerAction::Accept
            | ControllerAction::Cancel
            | ControllerAction::ClearCurrent
            | ControllerAction::Secondary
            | ControllerAction::Space
            | ControllerAction::SelectCurrent
            | ControllerAction::PagePreviewDown
            | ControllerAction::ToggleHiddenDirectories => None,
        }
    }
}

#[derive(Debug, Default)]
struct HeldAction {
    source_count: u8,
    next_repeat: Option<Instant>,
}

impl HeldAction {
    fn start(&mut self, now: Instant) {
        self.source_count = self.source_count.saturating_add(1);
        self.next_repeat = Some(now + INITIAL_REPEAT_DELAY);
    }

    fn stop(&mut self) {
        self.source_count = self.source_count.saturating_sub(1);
        if self.source_count == 0 {
            self.next_repeat = None;
        }
    }

    fn poll(&mut self, now: Instant) -> bool {
        let Some(next_repeat) = self.next_repeat else {
            return false;
        };
        if now < next_repeat {
            return false;
        }
        self.next_repeat = Some(now + REPEAT_INTERVAL);
        true
    }

    const fn next_repeat(&self) -> Option<Instant> {
        self.next_repeat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_direction_repeats_after_initial_delay() {
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            repeater.start(ControllerAction::Down, now),
            vec![ControllerAction::Down]
        );
        let before_initial_repeat = (now + INITIAL_REPEAT_DELAY)
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert!(repeater.poll(before_initial_repeat).is_empty());
        assert_eq!(
            repeater.poll(now + INITIAL_REPEAT_DELAY),
            vec![ControllerAction::Down]
        );
        assert!(
            repeater
                .poll(now + INITIAL_REPEAT_DELAY + Duration::from_millis(1))
                .is_empty()
        );
        assert_eq!(
            repeater.poll(now + INITIAL_REPEAT_DELAY + REPEAT_INTERVAL),
            vec![ControllerAction::Down]
        );
    }

    #[test]
    fn released_direction_stops_repeating() {
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        repeater.start(ControllerAction::Down, now);
        repeater.stop(ControllerAction::Down);

        assert!(repeater.poll(now + INITIAL_REPEAT_DELAY).is_empty());
    }

    #[test]
    fn clearing_repeater_stops_all_held_actions() {
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        repeater.start(ControllerAction::Down, now);
        repeater.start(ControllerAction::ScrollPreviewRight, now);
        repeater.clear();

        assert!(repeater.poll(now + INITIAL_REPEAT_DELAY).is_empty());
    }
}
