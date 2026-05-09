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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControllerEvent {
    Action(ControllerAction),
    Available(bool),
    PurgeQueuedActions,
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
                events.retain(|queued| !matches!(queued, ControllerEvent::Action(_)));
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
            .retain(|event| !matches!(event, ControllerEvent::Action(_)));
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
        let queue = Arc::new(ControllerEventQueue::default());
        let sender = ControllerEventSender::new(Arc::clone(&queue));
        let mut controller = Controller {
            queue,
            worker: None,
        };

        assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        sender.purge_actions();
        assert!(sender.send(ControllerEvent::PurgeQueuedActions));

        assert_eq!(
            controller.drain_events(),
            vec![ControllerEvent::PurgeQueuedActions]
        );
    }
}
