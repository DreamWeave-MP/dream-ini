use std::path::PathBuf;
use std::process::ExitCode;

use dream_ini::{
    ImportError, ImportEvent, ImportOptions, ImportWarning, IniImporter, serialize_cfg,
};
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
    state: ImportFormState,
    result: Option<GuiImportResult>,
    selected_result_panel: ResultPanel,
}

impl eframe::App for GuiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading(self.localizer.text(UiText::AppTitle));
            self.show_form(ui);
        });
    }
}

impl GuiApp {
    fn show_form(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(self.localizer.text(UiText::SourceSection));
        path_file_row(
            ui,
            self.localizer.text(UiText::MorrowindIni),
            self.localizer.text(UiText::Browse),
            &mut self.state.morrowind_ini,
        );
        path_file_row(
            ui,
            self.localizer.text(UiText::ExistingCfg),
            self.localizer.text(UiText::Browse),
            &mut self.state.existing_cfg,
        );

        ui.separator();
        ui.heading(self.localizer.text(UiText::ImportOptions));
        ui.checkbox(
            &mut self.state.import_fonts,
            self.localizer.text(UiText::ImportFallbacks),
        );
        ui.checkbox(
            &mut self.state.import_archives,
            self.localizer.text(UiText::ImportArchives),
        );
        ui.checkbox(
            &mut self.state.import_content_files,
            self.localizer.text(UiText::ImportContentFiles),
        );

        ui.separator();
        ui.heading(self.localizer.text(UiText::Overrides));
        self.show_data_dirs(ui);
        path_folder_row(
            ui,
            self.localizer.text(UiText::DataLocal),
            self.localizer.text(UiText::Browse),
            &mut self.state.data_local,
        );
        path_folder_row(
            ui,
            self.localizer.text(UiText::Resources),
            self.localizer.text(UiText::Browse),
            &mut self.state.resources,
        );
        path_folder_row(
            ui,
            self.localizer.text(UiText::Userdata),
            self.localizer.text(UiText::Browse),
            &mut self.state.userdata,
        );

        ui.separator();
        ui.heading(self.localizer.text(UiText::Output));
        let _ = ui.radio(true, self.localizer.text(UiText::PreviewOnly));

        if ui
            .button(self.localizer.text(UiText::ImportPreview))
            .clicked()
        {
            let result = self.state.import_preview();
            self.selected_result_panel = result.default_panel();
            self.result = Some(result);
        }

        if self.result.is_some() {
            ui.separator();
            self.show_results(ui);
        }
    }

    fn show_results(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text(UiText::Results));
        ui.horizontal(|ui| {
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::Errors,
                self.localizer.text(UiText::Errors),
            );
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::Warnings,
                self.localizer.text(UiText::Warnings),
            );
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::Events,
                self.localizer.text(UiText::Events),
            );
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::GeneratedCfg,
                self.localizer.text(UiText::GeneratedCfg),
            );
        });
        ui.separator();

        let Some(result) = &mut self.result else {
            return;
        };
        match self.selected_result_panel {
            ResultPanel::Errors => show_error_panel(ui, self.localizer, result),
            ResultPanel::Warnings => show_warning_panel(ui, self.localizer, result),
            ResultPanel::Events => show_event_panel(ui, self.localizer, result),
            ResultPanel::GeneratedCfg => show_generated_cfg_panel(ui, self.localizer, result),
        }
    }

    fn show_data_dirs(&mut self, ui: &mut egui::Ui) {
        ui.label(self.localizer.text(UiText::DataDirs));
        let mut remove_index = None;
        for (index, data_dir) in self.state.data_dirs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(data_dir);
                if ui.button(self.localizer.text(UiText::Browse)).clicked()
                    && let Some(path) = pick_folder()
                {
                    *data_dir = path;
                }
                if ui.button("−").clicked() {
                    remove_index = Some(index);
                }
            });
        }
        if let Some(index) = remove_index {
            self.state.data_dirs.remove(index);
        }
        if ui.button("+").clicked() {
            self.state.data_dirs.push(String::new());
        }
    }
}

#[derive(Debug, Clone)]
struct ImportFormState {
    morrowind_ini: String,
    existing_cfg: String,
    import_fonts: bool,
    import_archives: bool,
    import_content_files: bool,
    data_dirs: Vec<String>,
    data_local: String,
    resources: String,
    userdata: String,
}

impl Default for ImportFormState {
    fn default() -> Self {
        Self {
            morrowind_ini: String::new(),
            existing_cfg: String::new(),
            import_fonts: false,
            import_archives: true,
            import_content_files: false,
            data_dirs: Vec::new(),
            data_local: String::new(),
            resources: String::new(),
            userdata: String::new(),
        }
    }
}

impl ImportFormState {
    fn import_preview(&self) -> GuiImportResult {
        let Some(ini_path) = optional_path(&self.morrowind_ini) else {
            return GuiImportResult::Error {
                error: GuiImportError::Validation(
                    "Select a Morrowind.ini file before importing.".to_owned(),
                ),
            };
        };
        let cfg_path = optional_path(&self.existing_cfg);
        let importer = IniImporter::new(self.import_options());

        match importer.import_optional_cfg_path(&ini_path, cfg_path.as_deref()) {
            Ok(result) => GuiImportResult::Success {
                cfg_text: serialize_cfg(&result.cfg),
                warnings: result.warnings,
                events: result.events,
            },
            Err(error) => GuiImportResult::Error {
                error: GuiImportError::Import(error),
            },
        }
    }

    fn import_options(&self) -> ImportOptions {
        ImportOptions {
            import_game_files: self.import_content_files,
            import_fonts: self.import_fonts,
            import_archives: self.import_archives,
            data_dirs: self
                .data_dirs
                .iter()
                .filter_map(|value| optional_path(value))
                .collect(),
            data_local: optional_path(&self.data_local),
            resources: optional_path(&self.resources),
            userdata: optional_path(&self.userdata),
            verbose: true,
            ..ImportOptions::default()
        }
    }
}

#[derive(Debug)]
enum GuiImportResult {
    Success {
        cfg_text: String,
        warnings: Vec<ImportWarning>,
        events: Vec<ImportEvent>,
    },
    Error {
        error: GuiImportError,
    },
}

impl GuiImportResult {
    const fn default_panel(&self) -> ResultPanel {
        match self {
            Self::Success { .. } => ResultPanel::GeneratedCfg,
            Self::Error { .. } => ResultPanel::Errors,
        }
    }
}

#[derive(Debug)]
enum GuiImportError {
    Validation(String),
    Import(ImportError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ResultPanel {
    Errors,
    Warnings,
    Events,
    #[default]
    GeneratedCfg,
}

fn result_tab(ui: &mut egui::Ui, selected: &mut ResultPanel, panel: ResultPanel, label: &str) {
    if ui.selectable_label(*selected == panel, label).clicked() {
        *selected = panel;
    }
}

fn show_error_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
    match result {
        GuiImportResult::Success { .. } => {
            ui.label("No errors.");
        }
        GuiImportResult::Error { error } => {
            ui.colored_label(egui::Color32::RED, error_title(localizer, error));
        }
    }
}

fn show_warning_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
    let GuiImportResult::Success { warnings, .. } = result else {
        ui.label("No warnings.");
        return;
    };
    if warnings.is_empty() {
        ui.label("No warnings.");
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for warning in warnings {
            ui.label(localizer.warning_title(warning));
        }
    });
}

fn show_event_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
    let GuiImportResult::Success { events, .. } = result else {
        ui.label("No events.");
        return;
    };
    if events.is_empty() {
        ui.label("No events.");
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for event in events {
            ui.label(localizer.event_title(event));
        }
    });
}

fn show_generated_cfg_panel(ui: &mut egui::Ui, localizer: Localizer, result: &mut GuiImportResult) {
    let GuiImportResult::Success { cfg_text, .. } = result else {
        ui.label("No generated cfg.");
        return;
    };
    if ui.button(localizer.text(UiText::Copy)).clicked() {
        ui.ctx().copy_text(cfg_text.clone());
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(cfg_text)
                .font(egui::TextStyle::Monospace)
                .desired_rows(18)
                .interactive(false),
        );
    });
}

fn error_title(localizer: Localizer, error: &GuiImportError) -> String {
    match error {
        GuiImportError::Validation(message) => message.clone(),
        GuiImportError::Import(error) => localizer.error_title(error),
    }
}

fn path_file_row(ui: &mut egui::Ui, label: &str, browse: &str, value: &mut String) {
    path_row(ui, label, browse, value, pick_file);
}

fn path_folder_row(ui: &mut egui::Ui, label: &str, browse: &str, value: &mut String) {
    path_row(ui, label, browse, value, pick_folder);
}

fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    browse: &str,
    value: &mut String,
    pick: impl FnOnce() -> Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
        if ui.button(browse).clicked()
            && let Some(path) = pick()
        {
            *value = path;
        }
    });
}

fn pick_file() -> Option<String> {
    rfd::FileDialog::new()
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
}

fn pick_folder() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

fn optional_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}
