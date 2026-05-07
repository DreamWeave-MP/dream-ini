use std::process::ExitCode;

use eframe::egui;

use self::localization::{Localizer, UiText};

mod localization;

pub(crate) fn run() -> ExitCode {
    let options = eframe::NativeOptions::default();
    let result = eframe::run_native(
        "dream-ini",
        options,
        Box::new(|_creation_context| Ok(Box::new(GuiApp::default()))),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct GuiApp {
    localizer: Localizer,
}

impl eframe::App for GuiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading(self.localizer.text(UiText::AppTitle));
            ui.label(self.localizer.text(UiText::GuiReady));
        });
    }
}
