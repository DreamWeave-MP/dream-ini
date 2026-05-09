use std::sync::mpsc::Sender;

use eframe::egui;

use super::ControllerAction;

#[derive(Debug)]
pub(super) struct ControllerWorker;

impl ControllerWorker {
    pub(super) fn spawn(_sender: Sender<ControllerAction>, _context: egui::Context) -> Self {
        Self
    }
}
