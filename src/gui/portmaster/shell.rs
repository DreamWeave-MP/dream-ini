// SPDX-License-Identifier: GPL-3.0-only

use super::super::GuiShell;
use super::log::{SharedLog, write_log};

#[derive(Debug, Default)]
pub(super) struct PortMasterGuiShell {
    exit_requested: bool,
    clipboard_unsupported_logged: bool,
    log: Option<SharedLog>,
}

impl PortMasterGuiShell {
    pub(super) fn new(log: Option<&SharedLog>) -> Self {
        Self {
            exit_requested: false,
            clipboard_unsupported_logged: false,
            log: log.cloned(),
        }
    }

    pub(super) const fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

impl GuiShell for PortMasterGuiShell {
    fn request_exit(&mut self, _context: &egui::Context) {
        self.exit_requested = true;
    }

    fn copy_text(&mut self, _context: &egui::Context, _text: String) {
        if !self.clipboard_unsupported_logged {
            write_log(
                self.log.as_ref(),
                "clipboard requested, but PortMaster framebuffer shell has no clipboard",
            );
            self.clipboard_unsupported_logged = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portmaster_shell_records_exit_and_unsupported_clipboard() {
        let mut shell = PortMasterGuiShell::new(None);

        shell.copy_text(&egui::Context::default(), "fallback=1\n".to_owned());
        shell.request_exit(&egui::Context::default());

        assert!(shell.clipboard_unsupported_logged);
        assert!(shell.exit_requested());
    }
}
