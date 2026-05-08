use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dream_ini::{
    ImportError, ImportEvent, ImportOptions, ImportResult, ImportWarning, IniImporter,
    PreservedCfgUpdate, TextEncoding, apply_preserved_cfg_update, load_cfg_document,
    save_cfg_output_to_path, serialize_cfg_output,
};
use eframe::egui;

use self::file_picker::{PathPickerState, PathTarget, PickOutcome};
use self::localization::{Localizer, UiLanguage, UiText};

mod file_picker;
mod localization;

const CFG_KEY_DATA_LOCAL: &str = "data-local";
const CFG_KEY_RESOURCES: &str = "resources";
const CFG_KEY_USERDATA: &str = "user-data";
const MORROWIND_INI_LABEL: &str = "Morrowind.ini";
const OPENMW_CFG_LABEL: &str = "openmw.cfg";

pub(crate) fn run() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 860.0])
            .with_min_inner_size([640.0, 600.0]),
        ..Default::default()
    };
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

struct GuiApp {
    localizer: Localizer,
    state: ImportFormState,
    result: Option<GuiImportResult>,
    selected_result_panel: ResultPanel,
    mode: GuiMode,
}

impl Default for GuiApp {
    fn default() -> Self {
        Self {
            localizer: Localizer::default(),
            state: ImportFormState::default(),
            result: None,
            selected_result_panel: ResultPanel::default(),
            mode: GuiMode::ImportForm,
        }
    }
}

enum GuiMode {
    ImportForm,
    PathPicker(PathPickerState),
}

impl eframe::App for GuiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(context);
        egui::CentralPanel::default().show(context, |ui| {
            self.show_current_mode(ui);
        });
    }
}

impl GuiApp {
    fn handle_shortcuts(&mut self, context: &egui::Context) {
        if !matches!(self.mode, GuiMode::ImportForm) {
            return;
        }
        if context.input(|input| input.key_pressed(egui::Key::Escape)) {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if context.input(|input| input.key_pressed(egui::Key::Enter))
            && self.state.disabled_import_reason().is_none()
        {
            self.run_import();
        }
    }

    fn show_current_mode(&mut self, ui: &mut egui::Ui) {
        match &mut self.mode {
            GuiMode::ImportForm => self.show_form(ui),
            GuiMode::PathPicker(picker) => match picker.ui(ui, self.localizer) {
                PickOutcome::None => {}
                PickOutcome::Cancelled => self.mode = GuiMode::ImportForm,
                PickOutcome::Chosen { target, path } => {
                    self.apply_picked_path(target, &path);
                    self.mode = GuiMode::ImportForm;
                }
            },
        }
    }

    fn show_form(&mut self, ui: &mut egui::Ui) {
        self.show_language_selector(ui);
        ui.separator();
        let existing_cfg_label = self.existing_cfg_label();
        let path_label_width = self.path_label_width(ui, &existing_cfg_label);

        self.show_source_paths(ui, path_label_width, &existing_cfg_label);

        ui.separator();
        ui.heading(self.localizer.text(UiText::ImportOptions));
        encoding_dropdown(ui, self.localizer, &mut self.state.encoding);
        ui.checkbox(
            &mut self.state.import_fonts,
            self.localizer.text(UiText::ImportFallbacks),
        );
        ui.checkbox(
            &mut self.state.import_archives,
            self.localizer.text(UiText::ImportArchives),
        )
        .on_hover_text(self.localizer.text(UiText::ImportArchivesTooltip));
        ui.checkbox(
            &mut self.state.import_content_files,
            self.localizer.text(UiText::ImportContentFiles),
        )
        .on_hover_text(self.localizer.text(UiText::ImportContentFilesTooltip));

        ui.separator();
        self.show_override_paths(ui, path_label_width);

        ui.separator();
        ui.heading(self.localizer.text(UiText::Output));
        self.show_output_options(ui, path_label_width);

        let disabled_reason = self.state.disabled_import_reason();
        let import_button = ui.add_enabled(
            disabled_reason.is_none(),
            egui::Button::new(self.localizer.text(UiText::ImportPreview)),
        );
        if let Some(reason) = disabled_reason {
            ui.label(format!(
                "{} {}",
                self.localizer.text(UiText::CannotImport),
                self.localizer.text(reason)
            ));
        }
        if import_button.clicked() {
            self.run_import();
        }

        if self.result.is_some() {
            ui.separator();
            self.show_results(ui);
        }
    }

    fn run_import(&mut self) {
        let result = self.state.run_import();
        self.selected_result_panel = result.default_panel();
        self.result = Some(result);
    }

    fn existing_cfg_label(&self) -> String {
        format!(
            "{} {OPENMW_CFG_LABEL}",
            self.localizer.text(UiText::Existing)
        )
    }

    fn path_label_width(&self, ui: &egui::Ui, existing_cfg_label: &str) -> f32 {
        path_label_width(
            ui,
            &[
                MORROWIND_INI_LABEL,
                existing_cfg_label,
                self.localizer.text(UiText::ExplicitSearchPath),
                CFG_KEY_DATA_LOCAL,
                CFG_KEY_RESOURCES,
                CFG_KEY_USERDATA,
                self.localizer.text(UiText::OutputPath),
            ],
        )
    }

    fn show_source_paths(&mut self, ui: &mut egui::Ui, label_width: f32, existing_cfg_label: &str) {
        ui.heading(self.localizer.text(UiText::SourceSection));
        if path_file_row(
            ui,
            label_width,
            MORROWIND_INI_LABEL,
            self.localizer.text(UiText::Browse),
            &mut self.state.morrowind_ini,
        ) {
            let current_value = self.state.morrowind_ini.clone();
            self.open_path_picker(PathTarget::MorrowindIni, &current_value);
        }
        if path_file_row(
            ui,
            label_width,
            existing_cfg_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.existing_cfg,
        ) {
            let current_value = self.state.existing_cfg.clone();
            self.open_path_picker(PathTarget::ExistingOpenmwCfg, &current_value);
        }
    }

    fn show_override_paths(&mut self, ui: &mut egui::Ui, label_width: f32) {
        ui.heading(self.localizer.text(UiText::Overrides));
        if path_folder_row(
            ui,
            label_width,
            self.localizer.text(UiText::ExplicitSearchPath),
            self.localizer.text(UiText::Browse),
            &mut self.state.explicit_search_path,
            Some(self.localizer.text(UiText::ExplicitSearchPathTooltip)),
        ) {
            let current_value = self.state.explicit_search_path.clone();
            self.open_path_picker(PathTarget::GameDataDir, &current_value);
        }
        if path_folder_row(
            ui,
            label_width,
            CFG_KEY_DATA_LOCAL,
            self.localizer.text(UiText::Browse),
            &mut self.state.data_local,
            Some(self.localizer.text(UiText::DataLocalTooltip)),
        ) {
            let current_value = self.state.data_local.clone();
            self.open_path_picker(PathTarget::DataLocalDir, &current_value);
        }
        if path_folder_row(
            ui,
            label_width,
            CFG_KEY_RESOURCES,
            self.localizer.text(UiText::Browse),
            &mut self.state.resources,
            Some(self.localizer.text(UiText::ResourcesTooltip)),
        ) {
            let current_value = self.state.resources.clone();
            self.open_path_picker(PathTarget::ResourcesDir, &current_value);
        }
        if path_folder_row(
            ui,
            label_width,
            CFG_KEY_USERDATA,
            self.localizer.text(UiText::Browse),
            &mut self.state.user_data,
            Some(self.localizer.text(UiText::UserDataTooltip)),
        ) {
            let current_value = self.state.user_data.clone();
            self.open_path_picker(PathTarget::UserDataDir, &current_value);
        }
    }

    fn show_language_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(self.localizer.text(UiText::Language));
            let mut language = self.localizer.language();
            egui::ComboBox::from_id_salt("gui-language")
                .selected_text(language_label(self.localizer, language))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut language,
                        UiLanguage::English,
                        self.localizer.text(UiText::EnglishLanguage),
                    );
                    ui.selectable_value(
                        &mut language,
                        UiLanguage::French,
                        self.localizer.text(UiText::FrenchLanguage),
                    );
                    ui.selectable_value(
                        &mut language,
                        UiLanguage::German,
                        self.localizer.text(UiText::GermanLanguage),
                    );
                    ui.selectable_value(
                        &mut language,
                        UiLanguage::Spanish,
                        self.localizer.text(UiText::SpanishLanguage),
                    );
                });
            self.localizer.set_language(language);
        });
    }

    fn show_results(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text(UiText::Results));
        if let Some(GuiImportResult::Success {
            output_path: Some(path),
            ..
        }) = &self.result
        {
            ui.colored_label(
                egui::Color32::GREEN,
                format!(
                    "{} {}",
                    self.localizer.text(UiText::WroteCfgTo),
                    path.display()
                ),
            );
        }
        let copy_text = match &self.result {
            Some(GuiImportResult::Success { cfg_text, .. }) => Some(cfg_text.clone()),
            Some(GuiImportResult::Error { .. }) | None => None,
        };
        let mut clear_results = false;
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        copy_text.is_some(),
                        egui::Button::new(self.localizer.text(UiText::Copy)),
                    )
                    .clicked()
                    && let Some(text) = &copy_text
                {
                    ui.ctx().copy_text(text.clone());
                }
                if ui.button(self.localizer.text(UiText::Clear)).clicked() {
                    clear_results = true;
                }
            });
        });
        if clear_results {
            self.result = None;
            return;
        }
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

    fn show_output_options(&mut self, ui: &mut egui::Ui, path_label_width: f32) {
        ui.radio_value(
            &mut self.state.output_mode,
            GuiOutputMode::PreviewOnly,
            self.localizer.text(UiText::PreviewOnly),
        );
        ui.radio_value(
            &mut self.state.output_mode,
            GuiOutputMode::SaveAs,
            self.localizer.text(UiText::SaveAs),
        );
        ui.add_enabled_ui(self.state.output_mode == GuiOutputMode::SaveAs, |ui| {
            if path_save_file_row(
                ui,
                path_label_width,
                self.localizer.text(UiText::OutputPath),
                self.localizer.text(UiText::Browse),
                &mut self.state.output_path,
            ) {
                let current_value = self.state.output_path.clone();
                self.open_path_picker(PathTarget::OutputCfg, &current_value);
            }
        });
        ui.add_enabled_ui(optional_path(&self.state.existing_cfg).is_some(), |ui| {
            ui.radio_value(
                &mut self.state.output_mode,
                GuiOutputMode::UpdateExistingCfg,
                self.localizer.text(UiText::UpdateExistingCfg),
            );
        });
        if self.state.output_mode == GuiOutputMode::UpdateExistingCfg
            && optional_path(&self.state.existing_cfg).is_none()
        {
            self.state.output_mode = GuiOutputMode::PreviewOnly;
        }
    }

    fn open_path_picker(&mut self, target: PathTarget, current_value: &str) {
        let current_path = optional_path(current_value);
        self.mode = GuiMode::PathPicker(PathPickerState::new(target, current_path.as_deref()));
    }

    fn apply_picked_path(&mut self, target: PathTarget, path: &Path) {
        let value = path.to_string_lossy().into_owned();
        match target {
            PathTarget::MorrowindIni => self.state.morrowind_ini = value,
            PathTarget::ExistingOpenmwCfg => self.state.existing_cfg = value,
            PathTarget::OutputCfg => self.state.output_path = value,
            PathTarget::GameDataDir => self.state.explicit_search_path = value,
            PathTarget::DataLocalDir => self.state.data_local = value,
            PathTarget::ResourcesDir => self.state.resources = value,
            PathTarget::UserDataDir => self.state.user_data = value,
        }
    }
}

fn language_label(localizer: Localizer, language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::English => localizer.text(UiText::EnglishLanguage),
        UiLanguage::French => localizer.text(UiText::FrenchLanguage),
        UiLanguage::German => localizer.text(UiText::GermanLanguage),
        UiLanguage::Spanish => localizer.text(UiText::SpanishLanguage),
    }
}

#[derive(Debug, Clone)]
struct ImportFormState {
    morrowind_ini: String,
    existing_cfg: String,
    encoding: TextEncoding,
    import_fonts: bool,
    import_archives: bool,
    import_content_files: bool,
    explicit_search_path: String,
    data_local: String,
    resources: String,
    user_data: String,
    output_mode: GuiOutputMode,
    output_path: String,
}

impl Default for ImportFormState {
    fn default() -> Self {
        Self {
            morrowind_ini: String::new(),
            existing_cfg: String::new(),
            encoding: TextEncoding::Win1252,
            import_fonts: false,
            import_archives: true,
            import_content_files: false,
            explicit_search_path: String::new(),
            data_local: String::new(),
            resources: String::new(),
            user_data: String::new(),
            output_mode: GuiOutputMode::PreviewOnly,
            output_path: String::new(),
        }
    }
}

impl ImportFormState {
    fn disabled_import_reason(&self) -> Option<UiText> {
        if optional_path(&self.morrowind_ini).is_none() {
            return Some(UiText::SelectMorrowindIniBeforeImporting);
        }
        if self.output_mode == GuiOutputMode::SaveAs && optional_path(&self.output_path).is_none() {
            return Some(UiText::SelectOutputPathBeforeImporting);
        }
        if self.output_mode == GuiOutputMode::UpdateExistingCfg
            && optional_path(&self.existing_cfg).is_none()
        {
            return Some(UiText::SelectExistingCfgBeforeUpdating);
        }
        None
    }

    fn run_import(&self) -> GuiImportResult {
        let Some(ini_path) = optional_path(&self.morrowind_ini) else {
            return GuiImportResult::Error {
                error: GuiImportError::MissingMorrowindIni,
            };
        };
        let cfg_path = optional_path(&self.existing_cfg);
        let importer = IniImporter::new(self.import_options());

        match importer.import_optional_cfg_path(&ini_path, cfg_path.as_deref()) {
            Ok(result) => {
                let cfg_text = match self.serialize_result(&result) {
                    Ok(cfg_text) => cfg_text,
                    Err(error) => {
                        return GuiImportResult::Error {
                            error: GuiImportError::Import(error),
                        };
                    }
                };
                match self.write_output(&result) {
                    Ok(output_path) => GuiImportResult::Success {
                        cfg_text,
                        warnings: result.warnings,
                        events: result.events,
                        output_path,
                    },
                    Err(error) => GuiImportResult::Error { error },
                }
            }
            Err(error) => GuiImportResult::Error {
                error: GuiImportError::Import(error),
            },
        }
    }

    fn serialize_result(&self, result: &ImportResult) -> Result<String, ImportError> {
        if let Some(cfg_path) = optional_path(&self.existing_cfg) {
            let mut config = load_cfg_document(&cfg_path)?;
            apply_preserved_cfg_update(
                &mut config,
                &result.cfg,
                &self.preserved_update(),
                &result.changed_keys,
            )?;
            Ok(config.to_string())
        } else {
            serialize_cfg_output(&result.cfg, &self.output_reference_dir())
        }
    }

    fn write_output(&self, result: &ImportResult) -> Result<Option<PathBuf>, GuiImportError> {
        match self.output_mode {
            GuiOutputMode::PreviewOnly => Ok(None),
            GuiOutputMode::SaveAs => {
                let Some(output_path) = optional_path(&self.output_path) else {
                    return Err(GuiImportError::MissingOutputPath);
                };
                if let Some(cfg_path) = optional_path(&self.existing_cfg) {
                    let mut config =
                        load_cfg_document(&cfg_path).map_err(GuiImportError::Import)?;
                    apply_preserved_cfg_update(
                        &mut config,
                        &result.cfg,
                        &self.preserved_update(),
                        &result.changed_keys,
                    )
                    .map_err(GuiImportError::Import)?;
                    config.save_to_path(&output_path).map_err(|error| {
                        GuiImportError::Import(ImportError::OpenMwConfig(error.to_string()))
                    })?;
                } else {
                    save_cfg_output_to_path(&result.cfg, &output_path)
                        .map_err(GuiImportError::Import)?;
                }
                Ok(Some(output_path))
            }
            GuiOutputMode::UpdateExistingCfg => {
                let Some(cfg_path) = optional_path(&self.existing_cfg) else {
                    return Err(GuiImportError::MissingExistingCfgForUpdate);
                };
                let mut config = load_cfg_document(&cfg_path).map_err(GuiImportError::Import)?;
                apply_preserved_cfg_update(
                    &mut config,
                    &result.cfg,
                    &self.preserved_update(),
                    &result.changed_keys,
                )
                .map_err(GuiImportError::Import)?;
                config.save_to_path(&cfg_path).map_err(|error| {
                    GuiImportError::Import(ImportError::OpenMwConfig(error.to_string()))
                })?;
                Ok(Some(cfg_path))
            }
        }
    }

    fn preserved_update(&self) -> PreservedCfgUpdate {
        PreservedCfgUpdate {
            import_game_files: self.import_content_files,
            import_archives: self.import_archives,
            data_local: optional_path(&self.data_local),
            resources: optional_path(&self.resources),
            user_data: optional_path(&self.user_data),
        }
    }

    fn output_reference_dir(&self) -> PathBuf {
        let reference = match self.output_mode {
            GuiOutputMode::SaveAs => optional_path(&self.output_path),
            GuiOutputMode::PreviewOnly | GuiOutputMode::UpdateExistingCfg => {
                optional_path(&self.existing_cfg)
            }
        };

        reference
            .and_then(|path| path.parent().map(Path::to_owned))
            .unwrap_or_default()
    }

    fn import_options(&self) -> ImportOptions {
        ImportOptions {
            import_game_files: self.import_content_files,
            import_fonts: self.import_fonts,
            import_archives: self.import_archives,
            data_dirs: optional_path(&self.explicit_search_path)
                .into_iter()
                .collect(),
            data_local: optional_path(&self.data_local),
            resources: optional_path(&self.resources),
            user_data: optional_path(&self.user_data),
            encoding: Some(self.encoding),
            verbose: true,
            ..ImportOptions::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum GuiOutputMode {
    #[default]
    PreviewOnly,
    SaveAs,
    UpdateExistingCfg,
}

#[derive(Debug)]
enum GuiImportResult {
    Success {
        cfg_text: String,
        warnings: Vec<ImportWarning>,
        events: Vec<ImportEvent>,
        output_path: Option<PathBuf>,
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
    MissingMorrowindIni,
    MissingOutputPath,
    MissingExistingCfgForUpdate,
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

fn encoding_dropdown(ui: &mut egui::Ui, localizer: Localizer, encoding: &mut TextEncoding) {
    ui.horizontal(|ui| {
        ui.label(localizer.text(UiText::Encoding))
            .on_hover_text(localizer.text(UiText::EncodingTooltip));
        egui::ComboBox::from_id_salt("import-encoding")
            .selected_text(encoding_label(*encoding))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    encoding,
                    TextEncoding::Win1250,
                    encoding_label(TextEncoding::Win1250),
                );
                ui.selectable_value(
                    encoding,
                    TextEncoding::Win1251,
                    encoding_label(TextEncoding::Win1251),
                );
                ui.selectable_value(
                    encoding,
                    TextEncoding::Win1252,
                    encoding_label(TextEncoding::Win1252),
                );
            });
    });
}

const fn encoding_label(encoding: TextEncoding) -> &'static str {
    match encoding {
        TextEncoding::Win1250 => "win1250",
        TextEncoding::Win1251 => "win1251",
        TextEncoding::Win1252 => "win1252",
    }
}

fn show_error_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
    match result {
        GuiImportResult::Success { .. } => {
            ui.label(localizer.text(UiText::NoErrors));
        }
        GuiImportResult::Error { error } => {
            ui.colored_label(egui::Color32::RED, error_title(localizer, error));
        }
    }
}

fn show_warning_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
    let GuiImportResult::Success { warnings, .. } = result else {
        ui.label(localizer.text(UiText::NoWarnings));
        return;
    };
    if warnings.is_empty() {
        ui.label(localizer.text(UiText::NoWarnings));
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
        ui.label(localizer.text(UiText::NoEvents));
        return;
    };
    if events.is_empty() {
        ui.label(localizer.text(UiText::NoEvents));
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
        ui.label(localizer.text(UiText::NoGeneratedCfg));
        return;
    };
    ui.scope(|ui| {
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                show_numbered_cfg(ui, cfg_text);
            });
    });
}

fn show_numbered_cfg(ui: &mut egui::Ui, cfg_text: &str) {
    let line_count = cfg_text.split('\n').count().max(1);
    let number_width = line_count.to_string().len();
    egui::Grid::new("generated-cfg-preview")
        .num_columns(2)
        .spacing([8.0, 0.0])
        .striped(false)
        .show(ui, |ui| {
            for (index, line) in cfg_text.split('\n').enumerate() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("{:>number_width$}", index + 1)).monospace(),
                    )
                    .selectable(false),
                );
                ui.monospace(line);
                ui.end_row();
            }
        });
}

fn error_title(localizer: Localizer, error: &GuiImportError) -> String {
    match error {
        GuiImportError::MissingMorrowindIni => localizer
            .text(UiText::SelectMorrowindIniBeforeImporting)
            .to_owned(),
        GuiImportError::MissingOutputPath => localizer
            .text(UiText::SelectOutputPathBeforeImporting)
            .to_owned(),
        GuiImportError::MissingExistingCfgForUpdate => localizer
            .text(UiText::SelectExistingCfgBeforeUpdating)
            .to_owned(),
        GuiImportError::Import(error) => localizer.error_title(error),
    }
}

fn path_file_row(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
) -> bool {
    path_row_plain(ui, label_width, label, browse, value)
}

fn path_folder_row(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
    tooltip: Option<&str>,
) -> bool {
    path_row(ui, label_width, label, browse, value, tooltip)
}

fn path_save_file_row(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
) -> bool {
    path_row_plain(ui, label_width, label, browse, value)
}

fn path_row_plain(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
) -> bool {
    path_row(ui, label_width, label, browse, value, None)
}

fn path_row(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
    tooltip: Option<&str>,
) -> bool {
    let mut browse_clicked = false;
    ui.horizontal(|ui| {
        let row_height = ui.spacing().interact_size.y;
        let label_response = ui.add_sized([label_width, row_height], egui::Label::new(label));
        if let Some(tooltip) = tooltip {
            label_response.on_hover_text(tooltip);
        }
        let browse_button_width = button_width(ui, browse);
        let text_width = (ui.available_width() - browse_button_width - ui.spacing().item_spacing.x)
            .max(ui.spacing().interact_size.x);
        ui.add_sized([text_width, row_height], egui::TextEdit::singleline(value));
        browse_clicked = ui
            .add_sized([browse_button_width, row_height], egui::Button::new(browse))
            .clicked();
    });
    browse_clicked
}

fn button_width(ui: &egui::Ui, label: &str) -> f32 {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, ui.visuals().text_color())
        .size()
        .x;
    text_width + ui.spacing().button_padding.x * 2.0
}

fn path_label_width(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    (*label).to_owned(),
                    font_id.clone(),
                    ui.visuals().text_color(),
                )
                .size()
                .x
        })
        .fold(0.0, f32::max)
}

fn optional_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}
