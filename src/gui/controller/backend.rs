use std::sync::mpsc::SyncSender;

use eframe::egui;

use super::ControllerEvent;

#[derive(Debug)]
pub(super) struct ControllerWorker;

impl ControllerWorker {
    pub(super) fn spawn(
        _sender: SyncSender<ControllerEvent>,
        _context: egui::Context,
    ) -> Option<Self> {
        None
    }
}
