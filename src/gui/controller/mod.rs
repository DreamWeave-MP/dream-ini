//! Controller input for the GUI.
//!
//! Platform backends translate device-specific input into these actions.  The
//! GUI is not allowed to learn about evdev, `XInput`, HID usages, or any other
//! device-shaped nonsense.  That way lies soup.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ControllerAction {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Cancel,
}

#[derive(Debug, Default)]
pub(super) struct Controller {
    backend: backend::ControllerBackend,
}

impl Controller {
    pub(super) fn poll(&mut self) -> Vec<ControllerAction> {
        self.backend.poll()
    }
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod gilrs_backend;

#[cfg(target_os = "linux")]
mod backend {
    pub(super) use super::linux::ControllerBackend;
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod backend {
    pub(super) use super::gilrs_backend::ControllerBackend;
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod backend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_controller_starts_quiet() {
        assert!(Controller::default().poll().is_empty());
    }
}
