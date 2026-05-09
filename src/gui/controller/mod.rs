// SPDX-License-Identifier: GPL-3.0-only

//! Controller input for the GUI.
//!
//! Platform backends translate device-specific input into these actions.  The
//! GUI is not allowed to learn about evdev, `XInput`, HID usages, or any other
//! device-shaped nonsense.  That way lies soup.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use eframe::egui;

const MAX_QUEUED_CONTROLLER_ACTIONS: usize = 64;
const MAX_DRAINED_CONTROLLER_ACTIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ControllerAction {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Cancel,
    ClearCurrent,
    SelectCurrent,
    ToggleHiddenDirectories,
    ScrollPreviewLeft,
    ScrollPreviewRight,
    ScrollPreviewUp,
    ScrollPreviewDown,
}

impl ControllerAction {
    pub(super) const fn is_repeatable(self) -> bool {
        matches!(
            self,
            Self::Up
                | Self::Down
                | Self::Left
                | Self::Right
                | Self::ScrollPreviewLeft
                | Self::ScrollPreviewRight
                | Self::ScrollPreviewUp
                | Self::ScrollPreviewDown
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControllerEvent {
    Action(ControllerAction),
    Available(bool),
    PurgeQueuedActions,
}

impl ControllerEvent {
    const fn is_repeatable_action(self) -> bool {
        matches!(self, Self::Action(action) if action.is_repeatable())
    }
}

#[derive(Clone, Debug)]
pub(super) struct ControllerEventSender {
    queue: Arc<ControllerEventQueue>,
}

impl ControllerEventSender {
    fn new(queue: Arc<ControllerEventQueue>) -> Self {
        Self { queue }
    }

    pub(super) fn send(&self, event: ControllerEvent) -> bool {
        self.queue.push(event)
    }

    pub(super) fn purge_actions(&self) {
        self.queue.purge_actions();
    }
}

#[derive(Debug, Default)]
struct ControllerEventQueue {
    events: Mutex<VecDeque<ControllerEvent>>,
}

impl ControllerEventQueue {
    fn push(&self, event: ControllerEvent) -> bool {
        let mut events = self.events.lock().expect("controller event queue poisoned");
        match event {
            ControllerEvent::Action(_) => {
                if events.len() >= MAX_QUEUED_CONTROLLER_ACTIONS {
                    return false;
                }
            }
            ControllerEvent::Available(_) => {
                events.retain(|queued| !matches!(queued, ControllerEvent::Available(_)));
                if events.len() >= MAX_QUEUED_CONTROLLER_ACTIONS {
                    events.pop_front();
                }
            }
            ControllerEvent::PurgeQueuedActions => {
                events.retain(|queued| {
                    !matches!(queued, ControllerEvent::PurgeQueuedActions)
                        && !queued.is_repeatable_action()
                });
                if events.len() >= MAX_QUEUED_CONTROLLER_ACTIONS {
                    events.pop_front();
                }
            }
        }
        events.push_back(event);
        true
    }

    fn purge_actions(&self) {
        self.events
            .lock()
            .expect("controller event queue poisoned")
            .retain(|event| !event.is_repeatable_action());
    }

    fn drain(&self) -> Vec<ControllerEvent> {
        let mut events = self.events.lock().expect("controller event queue poisoned");
        let drain_count = events.len().min(MAX_DRAINED_CONTROLLER_ACTIONS);
        events.drain(..drain_count).collect()
    }
}

#[derive(Debug)]
pub(super) struct Controller {
    queue: Arc<ControllerEventQueue>,
    worker: Option<backend::ControllerWorker>,
}

impl Controller {
    pub(super) fn new(context: egui::Context) -> Self {
        let queue = Arc::new(ControllerEventQueue::default());
        let sender = ControllerEventSender::new(Arc::clone(&queue));
        Self {
            queue,
            worker: backend::ControllerWorker::spawn(sender, context),
        }
    }

    pub(super) fn drain_events(&mut self) -> Vec<ControllerEvent> {
        self.queue.drain()
    }

    #[cfg(test)]
    pub(super) fn with_test_sender() -> (Self, ControllerEventSender) {
        let queue = Arc::new(ControllerEventQueue::default());
        let sender = ControllerEventSender::new(Arc::clone(&queue));
        (
            Self {
                queue,
                worker: None,
            },
            sender,
        )
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            queue: Arc::new(ControllerEventQueue::default()),
            worker: None,
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        drop(self.worker.take());
    }
}

#[cfg(target_os = "linux")]
mod linux;

mod common;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod gilrs_backend;

#[cfg(target_os = "linux")]
mod backend {
    pub(super) use super::linux::ControllerWorker;
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod backend {
    pub(super) use super::gilrs_backend::ControllerWorker;
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod backend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_controller_starts_quiet() {
        assert!(Controller::default().drain_events().is_empty());
    }

    #[test]
    fn purge_event_drops_queued_actions() {
        let (mut controller, sender) = Controller::with_test_sender();

        assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        sender.purge_actions();
        assert!(sender.send(ControllerEvent::PurgeQueuedActions));

        assert_eq!(
            controller.drain_events(),
            vec![ControllerEvent::PurgeQueuedActions]
        );
    }

    #[test]
    fn action_queue_refuses_events_at_capacity() {
        let (mut controller, sender) = Controller::with_test_sender();

        for _ in 0..MAX_QUEUED_CONTROLLER_ACTIONS {
            assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        }
        assert!(!sender.send(ControllerEvent::Action(ControllerAction::Up)));

        assert_eq!(
            controller.drain_events().len(),
            MAX_DRAINED_CONTROLLER_ACTIONS
        );
    }

    #[test]
    fn availability_events_coalesce_to_latest_state() {
        let (mut controller, sender) = Controller::with_test_sender();

        assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        assert!(sender.send(ControllerEvent::Available(true)));
        assert!(sender.send(ControllerEvent::Available(false)));

        assert_eq!(
            controller.drain_events(),
            vec![
                ControllerEvent::Action(ControllerAction::Down),
                ControllerEvent::Available(false),
            ]
        );
    }

    #[test]
    fn purge_event_preserves_non_action_events() {
        let (mut controller, sender) = Controller::with_test_sender();

        assert!(sender.send(ControllerEvent::Available(true)));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Accept)));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Up)));
        assert!(sender.send(ControllerEvent::PurgeQueuedActions));
        assert!(sender.send(ControllerEvent::PurgeQueuedActions));

        assert_eq!(
            controller.drain_events(),
            vec![
                ControllerEvent::Available(true),
                ControllerEvent::Action(ControllerAction::Accept),
                ControllerEvent::PurgeQueuedActions,
            ]
        );
    }
}
