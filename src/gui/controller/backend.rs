// SPDX-License-Identifier: GPL-3.0-only

use super::ControllerEventSender;

#[derive(Debug)]
pub(super) struct ControllerWorker;

impl ControllerWorker {
    pub(super) fn spawn(_sender: ControllerEventSender, _context: egui::Context) -> Option<Self> {
        None
    }
}
