use std::sync::mpsc::SyncSender;

use eframe::egui;

use super::ControllerAction;

#[derive(Debug)]
pub(super) struct ControllerWorker;

impl ControllerWorker {
    pub(super) fn spawn(_sender: SyncSender<ControllerAction>, _context: egui::Context) -> Self {
        Self
    }
}
