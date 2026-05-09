//! Controller input for the GUI.
//!
//! Platform backends translate device-specific input into these actions.  The
//! GUI is not allowed to learn about evdev, `XInput`, HID usages, or any other
//! device-shaped nonsense.  That way lies soup.

use std::sync::mpsc::{self, Receiver};

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
}

#[derive(Debug)]
pub(super) struct Controller {
    receiver: Receiver<ControllerAction>,
    worker: Option<backend::ControllerWorker>,
}

impl Controller {
    pub(super) fn new(context: egui::Context) -> Self {
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_CONTROLLER_ACTIONS);
        Self {
            receiver,
            worker: Some(backend::ControllerWorker::spawn(sender, context)),
        }
    }

    pub(super) fn drain_actions(&mut self) -> Vec<ControllerAction> {
        self.receiver
            .try_iter()
            .take(MAX_DRAINED_CONTROLLER_ACTIONS)
            .collect()
    }
}

impl Default for Controller {
    fn default() -> Self {
        let (_sender, receiver) = mpsc::channel();
        Self {
            receiver,
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
        assert!(Controller::default().drain_actions().is_empty());
    }
}
