// SPDX-License-Identifier: MIT OR Apache-2.0

use super::ControllerEventSender;

#[derive(Debug)]
pub(super) struct ControllerWorker;

impl ControllerWorker {
    pub(super) fn spawn(_sender: ControllerEventSender, _context: egui::Context) -> Option<Self> {
        None
    }
}
