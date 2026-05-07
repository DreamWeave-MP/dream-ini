use std::process::ExitCode;

use eframe::egui;

pub(crate) fn run() -> ExitCode {
    let options = eframe::NativeOptions::default();
    let result = eframe::run_native(
        "dream-ini",
        options,
        Box::new(|_creation_context| Ok(Box::new(GuiApp))),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

struct GuiApp;

impl eframe::App for GuiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("dream-ini");
            ui.label("GUI support is enabled.");
        });
    }
}
