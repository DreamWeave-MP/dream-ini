// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dream_ini::{
    ImportError, ImportEvent, ImportOptions, ImportResult, ImportWarning, IniImporter,
    PreservedCfgUpdate, TextEncoding, apply_preserved_cfg_update, load_cfg_document,
    save_cfg_output_to_path, save_preserved_cfg_document_to_path,
    save_resolved_configuration_to_path, serialize_cfg_output, serialize_preserved_cfg_document,
    serialize_resolved_configuration,
};
use eframe::egui;

use self::controller::{ControllerAction, ControllerEvent};
use self::file_picker::{PathPickerState, PathTarget, PickOutcome};
use self::localization::{Localizer, UiLanguage, UiText};
use crate::desktop_entry::{APP_ID, APP_NAME};

mod controller;
mod file_picker;
mod localization;

const CFG_KEY_DATA_LOCAL: &str = "data-local";
const CFG_KEY_RESOURCES: &str = "resources";
const CFG_KEY_USERDATA: &str = "user-data";
const MORROWIND_INI_LABEL: &str = "Morrowind.ini";
const OPENMW_CFG_LABEL: &str = "openmw.cfg";
const CONTROLLER_PREVIEW_SCROLL_PIXELS: f32 = 72.0;

pub(crate) fn run() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id(APP_ID)
            .with_inner_size([760.0, 860.0])
            .with_min_inner_size([480.0, 320.0])
            .with_clamp_size_to_monitor_size(true)
            .with_icon(window_icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        APP_NAME,
        options,
        Box::new(|creation_context| Ok(Box::new(GuiApp::new(creation_context.egui_ctx.clone())))),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

fn window_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../../assets/logo.png"))
        .expect("embedded DreamWeave logo must be a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();

    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

struct GuiApp {
    controller: controller::Controller,
    localizer: Localizer,
    state: ImportFormState,
    result: Option<GuiImportResult>,
    selected_result_panel: ResultPanel,
    selected_form_control: FormControl,
    pending_form_scroll: Option<FormControl>,
    controller_navigation_visible: bool,
    generated_cfg_scroll_delta: egui::Vec2,
    mode: GuiMode,
}

impl Default for GuiApp {
    fn default() -> Self {
        Self::new_without_controller_worker()
    }
}

impl GuiApp {
    fn new(context: egui::Context) -> Self {
        Self {
            controller: controller::Controller::new(context),
            localizer: Localizer::default(),
            state: ImportFormState::default(),
            result: None,
            selected_result_panel: ResultPanel::default(),
            selected_form_control: FormControl::MorrowindIni,
            pending_form_scroll: None,
            controller_navigation_visible: false,
            generated_cfg_scroll_delta: egui::Vec2::ZERO,
            mode: GuiMode::ImportForm,
        }
    }

    fn new_without_controller_worker() -> Self {
        Self {
            controller: controller::Controller::default(),
            localizer: Localizer::default(),
            state: ImportFormState::default(),
            result: None,
            selected_result_panel: ResultPanel::default(),
            selected_form_control: FormControl::MorrowindIni,
            pending_form_scroll: None,
            controller_navigation_visible: false,
            generated_cfg_scroll_delta: egui::Vec2::ZERO,
            mode: GuiMode::ImportForm,
        }
    }
}

enum GuiMode {
    ImportForm,
    PathPicker(PathPickerState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormControl {
    Language,
    MorrowindIni,
    ExistingCfg,
    Encoding,
    ImportFonts,
    ImportArchives,
    ImportContentFiles,
    ExplicitSearchPath,
    DataLocal,
    Resources,
    UserData,
    OutputPreview,
    OutputSaveAs,
    OutputPath,
    OutputUpdateExisting,
    Import,
    ResultTabs,
    CopyResult,
    ClearResult,
}

#[derive(Debug, Clone, Copy)]
enum FormSelectionStep {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy)]
enum FormAdjustment {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy)]
enum PreviewScroll {
    Left,
    Right,
    Up,
    Down,
}

impl eframe::App for GuiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        let controller_actions = self.drain_controller_actions();
        let controller_actions_consumed =
            self.handle_controller_actions(context, &controller_actions);
        self.handle_shortcuts(context);
        let controller_actions_for_ui: &[ControllerAction] = if controller_actions_consumed {
            &[]
        } else {
            &controller_actions
        };
        egui::CentralPanel::default().show(context, |ui| {
            self.show_current_mode(ui, controller_actions_for_ui);
        });
    }
}

impl GuiApp {
    fn drain_controller_actions(&mut self) -> Vec<ControllerAction> {
        let mut actions = Vec::new();
        for event in self.controller.drain_events() {
            match event {
                ControllerEvent::PurgeQueuedActions => {
                    actions.retain(|action: &ControllerAction| !action.is_repeatable());
                }
                event => {
                    if let Some(action) = self.handle_controller_event(event) {
                        actions.push(action);
                    }
                }
            }
        }
        actions
    }

    fn handle_controller_event(&mut self, event: ControllerEvent) -> Option<ControllerAction> {
        match event {
            ControllerEvent::Action(action) => {
                self.controller_navigation_visible = true;
                Some(action)
            }
            ControllerEvent::Available(false) => {
                self.controller_navigation_visible = false;
                None
            }
            ControllerEvent::Available(true) | ControllerEvent::PurgeQueuedActions => None,
        }
    }

    fn handle_controller_actions(
        &mut self,
        context: &egui::Context,
        actions: &[ControllerAction],
    ) -> bool {
        if !matches!(self.mode, GuiMode::ImportForm) {
            return false;
        }
        if actions.is_empty() {
            return false;
        }

        self.ensure_selected_form_control_available();

        for action in actions {
            match action {
                ControllerAction::Cancel => {
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                    return true;
                }
                ControllerAction::Up => self.move_form_selection(FormSelectionStep::Previous),
                ControllerAction::Down => self.move_form_selection(FormSelectionStep::Next),
                ControllerAction::Left => {
                    self.adjust_selected_form_control(FormAdjustment::Previous);
                }
                ControllerAction::Right => self.adjust_selected_form_control(FormAdjustment::Next),
                ControllerAction::Accept => self.activate_selected_form_control(context),
                ControllerAction::ClearCurrent => self.clear_selected_form_control(),
                ControllerAction::SelectCurrent => self.run_import_if_enabled(),
                ControllerAction::ToggleHiddenDirectories => {}
                ControllerAction::ScrollPreviewLeft => {
                    self.scroll_generated_cfg_preview(PreviewScroll::Left);
                }
                ControllerAction::ScrollPreviewRight => {
                    self.scroll_generated_cfg_preview(PreviewScroll::Right);
                }
                ControllerAction::ScrollPreviewUp => {
                    self.scroll_generated_cfg_preview(PreviewScroll::Up);
                }
                ControllerAction::ScrollPreviewDown => {
                    self.scroll_generated_cfg_preview(PreviewScroll::Down);
                }
            }
            if !matches!(self.mode, GuiMode::ImportForm) {
                return true;
            }
        }
        true
    }

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

    fn scroll_generated_cfg_preview(&mut self, direction: PreviewScroll) {
        if self.selected_result_panel != ResultPanel::GeneratedCfg
            || !matches!(self.result, Some(GuiImportResult::Success { .. }))
        {
            return;
        }
        self.generated_cfg_scroll_delta += generated_cfg_scroll_delta(direction);
    }

    fn visible_form_controls(&self) -> Vec<FormControl> {
        let mut controls = vec![
            FormControl::Language,
            FormControl::MorrowindIni,
            FormControl::ExistingCfg,
            FormControl::Encoding,
            FormControl::ImportFonts,
            FormControl::ImportArchives,
            FormControl::ImportContentFiles,
            FormControl::ExplicitSearchPath,
            FormControl::DataLocal,
            FormControl::Resources,
            FormControl::UserData,
            FormControl::OutputPreview,
            FormControl::OutputSaveAs,
        ];
        if self.state.output_mode == GuiOutputMode::SaveAs {
            controls.push(FormControl::OutputPath);
        }
        if optional_path(&self.state.existing_cfg).is_some() {
            controls.push(FormControl::OutputUpdateExisting);
        }
        if self.state.disabled_import_reason().is_none() {
            controls.push(FormControl::Import);
        }
        if self.result.is_some() {
            controls.push(FormControl::ResultTabs);
            controls.push(FormControl::ClearResult);
            if matches!(self.result, Some(GuiImportResult::Success { .. })) {
                controls.push(FormControl::CopyResult);
            }
        }
        controls
    }

    fn ensure_selected_form_control_available(&mut self) {
        let controls = self.visible_form_controls();
        if !controls.contains(&self.selected_form_control) {
            self.selected_form_control = controls
                .into_iter()
                .next()
                .unwrap_or(FormControl::MorrowindIni);
        }
    }

    fn scroll_selected_form_control_into_view(&mut self, ui: &mut egui::Ui, control: FormControl) {
        if self.pending_form_scroll == Some(control) {
            ui.scroll_to_cursor(Some(egui::Align::Center));
            self.pending_form_scroll = None;
        }
    }

    fn move_form_selection(&mut self, step: FormSelectionStep) {
        let controls = self.visible_form_controls();
        if controls.is_empty() {
            return;
        }
        let current_index = controls
            .iter()
            .position(|control| *control == self.selected_form_control);
        let next_index = match (step, current_index) {
            (FormSelectionStep::Previous, Some(0) | None) => controls.len() - 1,
            (FormSelectionStep::Previous, Some(index)) => index - 1,
            (FormSelectionStep::Next, Some(index)) if index + 1 < controls.len() => index + 1,
            (FormSelectionStep::Next, Some(_) | None) => 0,
        };
        self.selected_form_control = controls[next_index];
        self.pending_form_scroll = Some(self.selected_form_control);
    }

    fn activate_selected_form_control(&mut self, context: &egui::Context) {
        match self.selected_form_control {
            FormControl::Language => self.cycle_language(FormAdjustment::Next),
            FormControl::MorrowindIni => self.open_form_path_picker(PathTarget::MorrowindIni),
            FormControl::ExistingCfg => self.open_form_path_picker(PathTarget::ExistingOpenmwCfg),
            FormControl::Encoding => self.cycle_encoding(FormAdjustment::Next),
            FormControl::ImportFonts => self.state.import_fonts = !self.state.import_fonts,
            FormControl::ImportArchives => self.state.import_archives = !self.state.import_archives,
            FormControl::ImportContentFiles => {
                self.state.import_content_files = !self.state.import_content_files;
            }
            FormControl::ExplicitSearchPath => self.open_form_path_picker(PathTarget::GameDataDir),
            FormControl::DataLocal => self.open_form_path_picker(PathTarget::DataLocalDir),
            FormControl::Resources => self.open_form_path_picker(PathTarget::ResourcesDir),
            FormControl::UserData => self.open_form_path_picker(PathTarget::UserDataDir),
            FormControl::OutputPreview => self.state.output_mode = GuiOutputMode::PreviewOnly,
            FormControl::OutputSaveAs => {
                self.state.output_mode = GuiOutputMode::SaveAs;
                self.selected_form_control = FormControl::OutputPath;
                self.pending_form_scroll = Some(FormControl::OutputPath);
            }
            FormControl::OutputPath => self.open_form_path_picker(PathTarget::OutputCfg),
            FormControl::OutputUpdateExisting => {
                if optional_path(&self.state.existing_cfg).is_some() {
                    self.state.output_mode = GuiOutputMode::UpdateExistingCfg;
                }
            }
            FormControl::Import => self.run_import_if_enabled(),
            FormControl::ResultTabs => self.cycle_result_panel(FormAdjustment::Next),
            FormControl::CopyResult => self.copy_result_to_clipboard(context),
            FormControl::ClearResult => self.result = None,
        }
    }

    fn adjust_selected_form_control(&mut self, adjustment: FormAdjustment) {
        match self.selected_form_control {
            FormControl::Language => self.cycle_language(adjustment),
            FormControl::Encoding => self.cycle_encoding(adjustment),
            FormControl::ImportFonts => {
                self.state.import_fonts = matches!(adjustment, FormAdjustment::Next);
            }
            FormControl::ImportArchives => {
                self.state.import_archives = matches!(adjustment, FormAdjustment::Next);
            }
            FormControl::ImportContentFiles => {
                self.state.import_content_files = matches!(adjustment, FormAdjustment::Next);
            }
            FormControl::OutputPreview
            | FormControl::OutputSaveAs
            | FormControl::OutputUpdateExisting => {
                self.cycle_output_mode(adjustment);
            }
            FormControl::ResultTabs => self.cycle_result_panel(adjustment),
            FormControl::MorrowindIni
            | FormControl::ExistingCfg
            | FormControl::ExplicitSearchPath
            | FormControl::DataLocal
            | FormControl::Resources
            | FormControl::UserData
            | FormControl::OutputPath
            | FormControl::Import
            | FormControl::CopyResult
            | FormControl::ClearResult => {}
        }
    }

    fn clear_selected_form_control(&mut self) {
        match self.selected_form_control {
            FormControl::MorrowindIni => self.state.morrowind_ini.clear(),
            FormControl::ExistingCfg => {
                self.state.existing_cfg.clear();
                if self.state.output_mode == GuiOutputMode::UpdateExistingCfg {
                    self.state.output_mode = GuiOutputMode::PreviewOnly;
                }
            }
            FormControl::ExplicitSearchPath => self.state.explicit_search_path.clear(),
            FormControl::DataLocal => self.state.data_local.clear(),
            FormControl::Resources => self.state.resources.clear(),
            FormControl::UserData => self.state.user_data.clear(),
            FormControl::OutputPath => self.state.output_path.clear(),
            FormControl::Language
            | FormControl::Encoding
            | FormControl::ImportFonts
            | FormControl::ImportArchives
            | FormControl::ImportContentFiles
            | FormControl::OutputPreview
            | FormControl::OutputSaveAs
            | FormControl::OutputUpdateExisting
            | FormControl::Import
            | FormControl::ResultTabs
            | FormControl::CopyResult
            | FormControl::ClearResult => {}
        }
        self.ensure_selected_form_control_available();
    }

    fn run_import_if_enabled(&mut self) {
        if self.state.disabled_import_reason().is_none() {
            self.run_import();
        }
    }

    fn open_form_path_picker(&mut self, target: PathTarget) {
        let current_value = match target {
            PathTarget::MorrowindIni => &self.state.morrowind_ini,
            PathTarget::ExistingOpenmwCfg => &self.state.existing_cfg,
            PathTarget::OutputCfg => &self.state.output_path,
            PathTarget::GameDataDir => &self.state.explicit_search_path,
            PathTarget::DataLocalDir => &self.state.data_local,
            PathTarget::ResourcesDir => &self.state.resources,
            PathTarget::UserDataDir => &self.state.user_data,
        }
        .clone();
        self.open_path_picker(target, &current_value);
    }

    fn cycle_language(&mut self, adjustment: FormAdjustment) {
        let language = self.localizer.language();
        self.localizer
            .set_language(cycled_language(language, adjustment));
    }

    fn cycle_encoding(&mut self, adjustment: FormAdjustment) {
        self.state.encoding = cycled_encoding(self.state.encoding, adjustment);
    }

    fn cycle_output_mode(&mut self, adjustment: FormAdjustment) {
        self.state.output_mode = cycled_output_mode(
            self.state.output_mode,
            adjustment,
            optional_path(&self.state.existing_cfg).is_some(),
        );
        self.selected_form_control = match self.state.output_mode {
            GuiOutputMode::PreviewOnly => FormControl::OutputPreview,
            GuiOutputMode::SaveAs => FormControl::OutputSaveAs,
            GuiOutputMode::UpdateExistingCfg => FormControl::OutputUpdateExisting,
        };
        self.pending_form_scroll = Some(self.selected_form_control);
    }

    fn cycle_result_panel(&mut self, adjustment: FormAdjustment) {
        self.selected_result_panel = cycled_result_panel(self.selected_result_panel, adjustment);
    }

    fn copy_result_to_clipboard(&self, context: &egui::Context) {
        if let Some(GuiImportResult::Success { cfg_text, .. }) = &self.result {
            context.copy_text(cfg_text.clone());
        }
    }

    fn show_current_mode(&mut self, ui: &mut egui::Ui, controller_actions: &[ControllerAction]) {
        match &mut self.mode {
            GuiMode::ImportForm => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_form(ui));
            }
            GuiMode::PathPicker(picker) => {
                let controller_scroll_delta = path_picker_scroll_delta(controller_actions);
                let outcome = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if controller_scroll_delta != egui::Vec2::ZERO {
                            ui.scroll_with_delta(controller_scroll_delta);
                        }
                        picker.ui(
                            ui,
                            self.localizer,
                            controller_actions,
                            self.controller_navigation_visible,
                        )
                    })
                    .inner;
                match outcome {
                    PickOutcome::None => {}
                    PickOutcome::Cancelled => self.mode = GuiMode::ImportForm,
                    PickOutcome::Chosen { target, path } => {
                        self.apply_picked_path(target, &path);
                        self.mode = GuiMode::ImportForm;
                    }
                }
            }
        }
    }

    fn show_form(&mut self, ui: &mut egui::Ui) {
        self.ensure_selected_form_control_available();
        self.show_language_selector(ui);
        self.show_controller_help(ui);
        ui.separator();
        let existing_cfg_label = self.existing_cfg_label();
        let path_label_width = self.path_label_width(ui, &existing_cfg_label);

        self.show_source_paths(ui, path_label_width, &existing_cfg_label);

        ui.separator();
        ui.heading(self.localizer.text(UiText::ImportOptions));
        let encoding_label =
            self.form_label(FormControl::Encoding, self.localizer.text(UiText::Encoding));
        encoding_dropdown(
            ui,
            self.localizer,
            &encoding_label,
            &mut self.state.encoding,
        );
        self.scroll_selected_form_control_into_view(ui, FormControl::Encoding);
        let import_fonts_label = self.form_label(
            FormControl::ImportFonts,
            self.localizer.text(UiText::ImportFallbacks),
        );
        ui.checkbox(&mut self.state.import_fonts, import_fonts_label);
        self.scroll_selected_form_control_into_view(ui, FormControl::ImportFonts);
        let import_archives_label = self.form_label(
            FormControl::ImportArchives,
            self.localizer.text(UiText::ImportArchives),
        );
        ui.checkbox(&mut self.state.import_archives, import_archives_label)
            .on_hover_text(self.localizer.text(UiText::ImportArchivesTooltip));
        self.scroll_selected_form_control_into_view(ui, FormControl::ImportArchives);
        let import_content_files_label = self.form_label(
            FormControl::ImportContentFiles,
            self.localizer.text(UiText::ImportContentFiles),
        );
        ui.checkbox(
            &mut self.state.import_content_files,
            import_content_files_label,
        )
        .on_hover_text(self.localizer.text(UiText::ImportContentFilesTooltip));
        self.scroll_selected_form_control_into_view(ui, FormControl::ImportContentFiles);

        ui.separator();
        self.show_override_paths(ui, path_label_width);

        ui.separator();
        ui.heading(self.localizer.text(UiText::Output));
        self.show_output_options(ui, path_label_width);

        let disabled_reason = self.state.disabled_import_reason();
        let import_button = ui.add_enabled(
            disabled_reason.is_none(),
            egui::Button::new(self.form_label(
                FormControl::Import,
                self.localizer.text(UiText::ImportPreview),
            )),
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
        self.scroll_selected_form_control_into_view(ui, FormControl::Import);

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

    fn show_controller_help(&self, ui: &mut egui::Ui) {
        if !self.controller_navigation_visible {
            return;
        }
        ui.small(self.localizer.text(UiText::ControllerHelp));
    }

    fn existing_cfg_label(&self) -> String {
        format!(
            "{} {OPENMW_CFG_LABEL}",
            self.localizer.text(UiText::Existing)
        )
    }

    fn form_label(&self, control: FormControl, label: &str) -> String {
        if !self.controller_navigation_visible {
            return label.to_owned();
        }
        format!(
            "{}{}",
            if self.selected_form_control == control {
                "▶ "
            } else {
                "  "
            },
            label
        )
    }

    fn result_tab_label(&self, panel: ResultPanel, label: &str) -> String {
        if !self.controller_navigation_visible {
            return label.to_owned();
        }
        format!(
            "{}{}",
            if self.selected_form_control == FormControl::ResultTabs
                && self.selected_result_panel == panel
            {
                "▶ "
            } else {
                "  "
            },
            label
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
        ) + if self.controller_navigation_visible {
            controller_marker_width(ui)
        } else {
            0.0
        }
    }

    fn show_source_paths(&mut self, ui: &mut egui::Ui, label_width: f32, existing_cfg_label: &str) {
        ui.heading(self.localizer.text(UiText::SourceSection));
        let morrowind_label = self.form_label(FormControl::MorrowindIni, MORROWIND_INI_LABEL);
        if path_file_row(
            ui,
            label_width,
            &morrowind_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.morrowind_ini,
        ) {
            let current_value = self.state.morrowind_ini.clone();
            self.open_path_picker(PathTarget::MorrowindIni, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::MorrowindIni);
        let existing_cfg_label = self.form_label(FormControl::ExistingCfg, existing_cfg_label);
        if path_file_row(
            ui,
            label_width,
            &existing_cfg_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.existing_cfg,
        ) {
            let current_value = self.state.existing_cfg.clone();
            self.open_path_picker(PathTarget::ExistingOpenmwCfg, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::ExistingCfg);
    }

    fn show_override_paths(&mut self, ui: &mut egui::Ui, label_width: f32) {
        ui.heading(self.localizer.text(UiText::Overrides));
        let explicit_search_label = self.form_label(
            FormControl::ExplicitSearchPath,
            self.localizer.text(UiText::ExplicitSearchPath),
        );
        if path_folder_row(
            ui,
            label_width,
            &explicit_search_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.explicit_search_path,
            Some(self.localizer.text(UiText::ExplicitSearchPathTooltip)),
        ) {
            let current_value = self.state.explicit_search_path.clone();
            self.open_path_picker(PathTarget::GameDataDir, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::ExplicitSearchPath);
        let data_local_label = self.form_label(FormControl::DataLocal, CFG_KEY_DATA_LOCAL);
        if path_folder_row(
            ui,
            label_width,
            &data_local_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.data_local,
            Some(self.localizer.text(UiText::DataLocalTooltip)),
        ) {
            let current_value = self.state.data_local.clone();
            self.open_path_picker(PathTarget::DataLocalDir, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::DataLocal);
        let resources_label = self.form_label(FormControl::Resources, CFG_KEY_RESOURCES);
        if path_folder_row(
            ui,
            label_width,
            &resources_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.resources,
            Some(self.localizer.text(UiText::ResourcesTooltip)),
        ) {
            let current_value = self.state.resources.clone();
            self.open_path_picker(PathTarget::ResourcesDir, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::Resources);
        let user_data_label = self.form_label(FormControl::UserData, CFG_KEY_USERDATA);
        if path_folder_row(
            ui,
            label_width,
            &user_data_label,
            self.localizer.text(UiText::Browse),
            &mut self.state.user_data,
            Some(self.localizer.text(UiText::UserDataTooltip)),
        ) {
            let current_value = self.state.user_data.clone();
            self.open_path_picker(PathTarget::UserDataDir, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::UserData);
    }

    fn show_language_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(self.form_label(FormControl::Language, self.localizer.text(UiText::Language)));
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
                        UiLanguage::Russian,
                        self.localizer.text(UiText::RussianLanguage),
                    );
                    ui.selectable_value(
                        &mut language,
                        UiLanguage::Spanish,
                        self.localizer.text(UiText::SpanishLanguage),
                    );
                });
            self.localizer.set_language(language);
        });
        self.scroll_selected_form_control_into_view(ui, FormControl::Language);
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
        let errors_label =
            self.result_tab_label(ResultPanel::Errors, self.localizer.text(UiText::Errors));
        let warnings_label =
            self.result_tab_label(ResultPanel::Warnings, self.localizer.text(UiText::Warnings));
        let events_label =
            self.result_tab_label(ResultPanel::Events, self.localizer.text(UiText::Events));
        let generated_cfg_label = self.result_tab_label(
            ResultPanel::GeneratedCfg,
            self.localizer.text(UiText::GeneratedCfg),
        );
        let copy_label =
            self.form_label(FormControl::CopyResult, self.localizer.text(UiText::Copy));
        let clear_label =
            self.form_label(FormControl::ClearResult, self.localizer.text(UiText::Clear));
        let mut clear_results = false;
        ui.horizontal(|ui| {
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::Errors,
                &errors_label,
            );
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::Warnings,
                &warnings_label,
            );
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::Events,
                &events_label,
            );
            result_tab(
                ui,
                &mut self.selected_result_panel,
                ResultPanel::GeneratedCfg,
                &generated_cfg_label,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(copy_text.is_some(), egui::Button::new(copy_label.as_str()))
                    .clicked()
                    && let Some(text) = &copy_text
                {
                    ui.ctx().copy_text(text.clone());
                }
                if ui.button(clear_label.as_str()).clicked() {
                    clear_results = true;
                }
            });
        });
        self.scroll_selected_form_control_into_view(ui, FormControl::ResultTabs);
        self.scroll_selected_form_control_into_view(ui, FormControl::CopyResult);
        self.scroll_selected_form_control_into_view(ui, FormControl::ClearResult);
        if clear_results {
            self.result = None;
            return;
        }
        ui.separator();

        let generated_cfg_scroll_delta = if self.selected_result_panel == ResultPanel::GeneratedCfg
        {
            std::mem::take(&mut self.generated_cfg_scroll_delta)
        } else {
            egui::Vec2::ZERO
        };
        let Some(result) = &mut self.result else {
            return;
        };
        match self.selected_result_panel {
            ResultPanel::Errors => show_error_panel(ui, self.localizer, result),
            ResultPanel::Warnings => show_warning_panel(ui, self.localizer, result),
            ResultPanel::Events => show_event_panel(ui, self.localizer, result),
            ResultPanel::GeneratedCfg => {
                show_generated_cfg_panel(ui, self.localizer, result, generated_cfg_scroll_delta);
            }
        }
    }

    fn show_output_options(&mut self, ui: &mut egui::Ui, path_label_width: f32) {
        let preview_label = self.form_label(
            FormControl::OutputPreview,
            self.localizer.text(UiText::PreviewOnly),
        );
        let save_as_label = self.form_label(
            FormControl::OutputSaveAs,
            self.localizer.text(UiText::SaveAs),
        );
        let output_path_label = self.form_label(
            FormControl::OutputPath,
            self.localizer.text(UiText::OutputPath),
        );
        let update_existing_label = self.form_label(
            FormControl::OutputUpdateExisting,
            self.localizer.text(UiText::UpdateExistingCfg),
        );
        ui.radio_value(
            &mut self.state.output_mode,
            GuiOutputMode::PreviewOnly,
            preview_label,
        );
        self.scroll_selected_form_control_into_view(ui, FormControl::OutputPreview);
        ui.radio_value(
            &mut self.state.output_mode,
            GuiOutputMode::SaveAs,
            save_as_label,
        );
        self.scroll_selected_form_control_into_view(ui, FormControl::OutputSaveAs);
        ui.add_enabled_ui(self.state.output_mode == GuiOutputMode::SaveAs, |ui| {
            if path_save_file_row(
                ui,
                path_label_width,
                &output_path_label,
                self.localizer.text(UiText::Browse),
                &mut self.state.output_path,
            ) {
                let current_value = self.state.output_path.clone();
                self.open_path_picker(PathTarget::OutputCfg, &current_value);
            }
        });
        self.scroll_selected_form_control_into_view(ui, FormControl::OutputPath);
        ui.add_enabled_ui(optional_path(&self.state.existing_cfg).is_some(), |ui| {
            ui.radio_value(
                &mut self.state.output_mode,
                GuiOutputMode::UpdateExistingCfg,
                &update_existing_label,
            );
        });
        self.scroll_selected_form_control_into_view(ui, FormControl::OutputUpdateExisting);
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
        UiLanguage::Russian => localizer.text(UiText::RussianLanguage),
        UiLanguage::Spanish => localizer.text(UiText::SpanishLanguage),
    }
}

fn cycled_language(language: UiLanguage, adjustment: FormAdjustment) -> UiLanguage {
    cycle_item(
        &[
            UiLanguage::English,
            UiLanguage::French,
            UiLanguage::German,
            UiLanguage::Russian,
            UiLanguage::Spanish,
        ],
        language,
        adjustment,
    )
}

fn cycled_encoding(
    encoding: Option<TextEncoding>,
    adjustment: FormAdjustment,
) -> Option<TextEncoding> {
    cycle_item(
        &[
            None,
            Some(TextEncoding::Win1250),
            Some(TextEncoding::Win1251),
            Some(TextEncoding::Win1252),
        ],
        encoding,
        adjustment,
    )
}

fn cycled_output_mode(
    output_mode: GuiOutputMode,
    adjustment: FormAdjustment,
    has_existing_cfg: bool,
) -> GuiOutputMode {
    if has_existing_cfg {
        cycle_item(
            &[
                GuiOutputMode::PreviewOnly,
                GuiOutputMode::SaveAs,
                GuiOutputMode::UpdateExistingCfg,
            ],
            output_mode,
            adjustment,
        )
    } else {
        cycle_item(
            &[GuiOutputMode::PreviewOnly, GuiOutputMode::SaveAs],
            output_mode,
            adjustment,
        )
    }
}

fn cycled_result_panel(panel: ResultPanel, adjustment: FormAdjustment) -> ResultPanel {
    cycle_item(
        &[
            ResultPanel::Errors,
            ResultPanel::Warnings,
            ResultPanel::Events,
            ResultPanel::GeneratedCfg,
        ],
        panel,
        adjustment,
    )
}

fn generated_cfg_scroll_delta(direction: PreviewScroll) -> egui::Vec2 {
    match direction {
        PreviewScroll::Left => egui::vec2(CONTROLLER_PREVIEW_SCROLL_PIXELS, 0.0),
        PreviewScroll::Right => egui::vec2(-CONTROLLER_PREVIEW_SCROLL_PIXELS, 0.0),
        PreviewScroll::Up => egui::vec2(0.0, CONTROLLER_PREVIEW_SCROLL_PIXELS),
        PreviewScroll::Down => egui::vec2(0.0, -CONTROLLER_PREVIEW_SCROLL_PIXELS),
    }
}

fn path_picker_scroll_delta(actions: &[ControllerAction]) -> egui::Vec2 {
    actions.iter().fold(egui::Vec2::ZERO, |delta, action| {
        delta
            + match action {
                ControllerAction::ScrollPreviewUp => {
                    egui::vec2(0.0, CONTROLLER_PREVIEW_SCROLL_PIXELS)
                }
                ControllerAction::ScrollPreviewDown => {
                    egui::vec2(0.0, -CONTROLLER_PREVIEW_SCROLL_PIXELS)
                }
                ControllerAction::ScrollPreviewLeft
                | ControllerAction::ScrollPreviewRight
                | ControllerAction::Up
                | ControllerAction::Down
                | ControllerAction::Left
                | ControllerAction::Right
                | ControllerAction::Accept
                | ControllerAction::Cancel
                | ControllerAction::ClearCurrent
                | ControllerAction::SelectCurrent
                | ControllerAction::ToggleHiddenDirectories => egui::Vec2::ZERO,
            }
    })
}

fn cycle_item<T: Copy + PartialEq>(items: &[T], current: T, adjustment: FormAdjustment) -> T {
    let Some(index) = items.iter().position(|item| *item == current) else {
        return current;
    };
    let next_index = match adjustment {
        FormAdjustment::Previous if index == 0 => items.len() - 1,
        FormAdjustment::Previous => index - 1,
        FormAdjustment::Next if index + 1 == items.len() => 0,
        FormAdjustment::Next => index + 1,
    };
    items.get(next_index).copied().unwrap_or(current)
}

#[derive(Debug, Clone)]
struct ImportFormState {
    morrowind_ini: String,
    existing_cfg: String,
    encoding: Option<TextEncoding>,
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
            encoding: None,
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
            if self.relocated_existing_cfg_output() {
                Ok(serialize_resolved_configuration(&config))
            } else {
                Ok(serialize_preserved_cfg_document(
                    &config,
                    &cfg_path,
                    &self.preserved_update(),
                    &result.changed_keys,
                ))
            }
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
                    if same_cfg_context(&cfg_path, &output_path) {
                        save_preserved_cfg_document_to_path(
                            &config,
                            &cfg_path,
                            &output_path,
                            &self.preserved_update(),
                            &result.changed_keys,
                        )
                        .map_err(GuiImportError::Import)?;
                    } else {
                        save_resolved_configuration_to_path(&config, &output_path)
                            .map_err(GuiImportError::Import)?;
                    }
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
                save_preserved_cfg_document_to_path(
                    &config,
                    &cfg_path,
                    &cfg_path,
                    &self.preserved_update(),
                    &result.changed_keys,
                )
                .map_err(GuiImportError::Import)?;
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
            data_dir_base: self.output_context_dir(),
            write_resolved_data_dirs: self.relocated_existing_cfg_output(),
            data_local: optional_path(&self.data_local),
            resources: optional_path(&self.resources),
            user_data: optional_path(&self.user_data),
            encoding: self.encoding,
            verbose: true,
            ..ImportOptions::default()
        }
    }

    fn output_context_dir(&self) -> Option<PathBuf> {
        match self.output_mode {
            GuiOutputMode::SaveAs => optional_path(&self.output_path),
            GuiOutputMode::PreviewOnly | GuiOutputMode::UpdateExistingCfg => {
                optional_path(&self.existing_cfg)
            }
        }
        .map(|path| cfg_parent(&path).to_owned())
    }

    fn relocated_existing_cfg_output(&self) -> bool {
        if self.output_mode != GuiOutputMode::SaveAs {
            return false;
        }
        optional_path(&self.existing_cfg)
            .zip(optional_path(&self.output_path))
            .is_some_and(|(cfg_path, output_path)| !same_cfg_context(&cfg_path, &output_path))
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

fn encoding_dropdown(
    ui: &mut egui::Ui,
    localizer: Localizer,
    label: &str,
    encoding: &mut Option<TextEncoding>,
) {
    ui.horizontal(|ui| {
        ui.label(label)
            .on_hover_text(localizer.text(UiText::EncodingTooltip));
        egui::ComboBox::from_id_salt("import-encoding")
            .selected_text(optional_encoding_label(localizer, *encoding))
            .show_ui(ui, |ui| {
                ui.selectable_value(encoding, None, localizer.text(UiText::EncodingAuto));
                ui.selectable_value(
                    encoding,
                    Some(TextEncoding::Win1250),
                    encoding_label(TextEncoding::Win1250),
                );
                ui.selectable_value(
                    encoding,
                    Some(TextEncoding::Win1251),
                    encoding_label(TextEncoding::Win1251),
                );
                ui.selectable_value(
                    encoding,
                    Some(TextEncoding::Win1252),
                    encoding_label(TextEncoding::Win1252),
                );
            });
    });
}

fn optional_encoding_label(localizer: Localizer, encoding: Option<TextEncoding>) -> &'static str {
    encoding.map_or_else(|| localizer.text(UiText::EncodingAuto), encoding_label)
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

fn show_generated_cfg_panel(
    ui: &mut egui::Ui,
    localizer: Localizer,
    result: &mut GuiImportResult,
    controller_scroll_delta: egui::Vec2,
) {
    let GuiImportResult::Success { cfg_text, .. } = result else {
        ui.label(localizer.text(UiText::NoGeneratedCfg));
        return;
    };
    ui.scope(|ui| {
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if controller_scroll_delta != egui::Vec2::ZERO {
                    ui.scroll_with_delta(controller_scroll_delta);
                }
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

fn controller_marker_width(ui: &egui::Ui) -> f32 {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap("▶ ".to_owned(), font_id, ui.visuals().text_color())
        .size()
        .x
}

fn optional_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

fn same_cfg_context(left: &Path, right: &Path) -> bool {
    equivalent_dirs(cfg_parent(left), cfg_parent(right))
}

fn cfg_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn equivalent_dirs(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_owned());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_owned());
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_gui_encoding_is_not_an_override() {
        assert_eq!(ImportFormState::default().import_options().encoding, None);
    }

    #[test]
    fn embedded_window_icon_is_valid_rgba() {
        let icon = window_icon();

        assert_eq!(icon.width, 512);
        assert_eq!(icon.height, 512);
        assert_eq!(
            icon.rgba.len(),
            icon.width as usize * icon.height as usize * 4
        );
    }

    #[test]
    fn form_controller_selection_moves_through_visible_controls() {
        let mut app = GuiApp::new_without_controller_worker();
        app.state.morrowind_ini = "Morrowind.ini".to_owned();
        assert_eq!(app.selected_form_control, FormControl::MorrowindIni);

        app.move_form_selection(FormSelectionStep::Previous);
        assert_eq!(app.selected_form_control, FormControl::Language);

        app.move_form_selection(FormSelectionStep::Previous);
        assert_eq!(app.selected_form_control, FormControl::Import);

        app.move_form_selection(FormSelectionStep::Next);
        assert_eq!(app.selected_form_control, FormControl::Language);
    }

    #[test]
    fn form_controller_only_exposes_currently_usable_output_controls() {
        let mut app = GuiApp::new_without_controller_worker();

        assert!(
            !app.visible_form_controls()
                .contains(&FormControl::OutputPath)
        );
        assert!(
            !app.visible_form_controls()
                .contains(&FormControl::OutputUpdateExisting)
        );

        app.state.output_mode = GuiOutputMode::SaveAs;
        assert!(
            app.visible_form_controls()
                .contains(&FormControl::OutputPath)
        );

        app.state.existing_cfg = "openmw.cfg".to_owned();
        assert!(
            app.visible_form_controls()
                .contains(&FormControl::OutputUpdateExisting)
        );
    }

    #[test]
    fn form_controller_save_as_activation_selects_output_path() {
        let mut app = GuiApp::new_without_controller_worker();
        app.selected_form_control = FormControl::OutputSaveAs;

        app.activate_selected_form_control(&egui::Context::default());

        assert_eq!(app.state.output_mode, GuiOutputMode::SaveAs);
        assert_eq!(app.selected_form_control, FormControl::OutputPath);
    }

    #[test]
    fn form_controller_accept_opens_selected_path_picker() {
        let mut app = GuiApp::new_without_controller_worker();
        app.selected_form_control = FormControl::MorrowindIni;

        app.activate_selected_form_control(&egui::Context::default());

        assert!(matches!(app.mode, GuiMode::PathPicker(_)));
    }

    #[test]
    fn form_controller_right_does_not_open_path_picker() {
        let mut app = GuiApp::new_without_controller_worker();
        app.selected_form_control = FormControl::MorrowindIni;

        app.adjust_selected_form_control(FormAdjustment::Next);

        assert!(matches!(app.mode, GuiMode::ImportForm));
    }

    #[test]
    fn form_controller_consumes_action_that_opens_picker() {
        let mut app = GuiApp::new_without_controller_worker();
        app.selected_form_control = FormControl::ExplicitSearchPath;

        let consumed =
            app.handle_controller_actions(&egui::Context::default(), &[ControllerAction::Accept]);

        assert!(consumed);
        assert!(matches!(app.mode, GuiMode::PathPicker(_)));
    }

    #[test]
    fn form_controller_clear_current_clears_selected_path() {
        let mut app = GuiApp::new_without_controller_worker();
        app.state.existing_cfg = "openmw.cfg".to_owned();
        app.state.output_mode = GuiOutputMode::UpdateExistingCfg;
        app.selected_form_control = FormControl::ExistingCfg;

        app.clear_selected_form_control();

        assert!(app.state.existing_cfg.is_empty());
        assert_eq!(app.state.output_mode, GuiOutputMode::PreviewOnly);
    }

    #[test]
    fn controller_selection_marker_requires_controller_input() {
        let mut app = GuiApp::new_without_controller_worker();
        app.selected_form_control = FormControl::MorrowindIni;

        assert_eq!(
            app.form_label(FormControl::MorrowindIni, "Morrowind.ini"),
            "Morrowind.ini"
        );

        app.controller_navigation_visible = true;

        assert_eq!(
            app.form_label(FormControl::MorrowindIni, "Morrowind.ini"),
            "▶ Morrowind.ini"
        );
    }

    #[test]
    fn controller_disconnect_hides_selection_marker() {
        let mut app = GuiApp::new_without_controller_worker();

        assert_eq!(
            app.handle_controller_event(ControllerEvent::Action(ControllerAction::Down)),
            Some(ControllerAction::Down)
        );
        assert!(app.controller_navigation_visible);

        assert_eq!(
            app.handle_controller_event(ControllerEvent::Available(false)),
            None
        );

        assert!(!app.controller_navigation_visible);
    }

    #[test]
    fn controller_purge_event_clears_actions_drained_in_same_frame() {
        let mut app = GuiApp::new_without_controller_worker();
        let (controller, sender) = controller::Controller::with_test_sender();
        app.controller = controller;

        assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Accept)));
        assert!(sender.send(ControllerEvent::PurgeQueuedActions));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Up)));

        assert_eq!(
            app.drain_controller_actions(),
            vec![ControllerAction::Accept, ControllerAction::Up]
        );
    }

    #[test]
    fn result_controls_select_clear_before_copy() {
        let mut app = GuiApp::new_without_controller_worker();
        app.state.morrowind_ini = "Morrowind.ini".to_owned();
        app.result = Some(GuiImportResult::Success {
            cfg_text: String::new(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_form_control = FormControl::ResultTabs;

        app.move_form_selection(FormSelectionStep::Next);
        assert_eq!(app.selected_form_control, FormControl::ClearResult);

        app.move_form_selection(FormSelectionStep::Next);
        assert_eq!(app.selected_form_control, FormControl::CopyResult);

        app.move_form_selection(FormSelectionStep::Next);
        assert_eq!(app.selected_form_control, FormControl::Language);
    }

    #[test]
    fn right_stick_scrolls_generated_cfg_preview() {
        let mut app = GuiApp::new_without_controller_worker();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_result_panel = ResultPanel::GeneratedCfg;

        app.handle_controller_actions(
            &egui::Context::default(),
            &[
                ControllerAction::ScrollPreviewDown,
                ControllerAction::ScrollPreviewRight,
            ],
        );

        assert!(
            (app.generated_cfg_scroll_delta.x + CONTROLLER_PREVIEW_SCROLL_PIXELS).abs()
                < f32::EPSILON
        );
        assert!(
            (app.generated_cfg_scroll_delta.y + CONTROLLER_PREVIEW_SCROLL_PIXELS).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn right_stick_ignores_non_generated_result_panels() {
        let mut app = GuiApp::new_without_controller_worker();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_result_panel = ResultPanel::Warnings;

        app.handle_controller_actions(
            &egui::Context::default(),
            &[ControllerAction::ScrollPreviewDown],
        );

        assert!(app.generated_cfg_scroll_delta.length_sq() < f32::EPSILON);
    }

    #[test]
    fn path_picker_scroll_delta_uses_only_vertical_preview_scroll_actions() {
        let delta = path_picker_scroll_delta(&[
            ControllerAction::ScrollPreviewDown,
            ControllerAction::ScrollPreviewRight,
            ControllerAction::Down,
            ControllerAction::ScrollPreviewUp,
            ControllerAction::ScrollPreviewUp,
            ControllerAction::ToggleHiddenDirectories,
        ]);

        assert!(delta.x.abs() < f32::EPSILON);
        assert!((delta.y - CONTROLLER_PREVIEW_SCROLL_PIXELS).abs() < f32::EPSILON);
    }

    #[test]
    fn disabled_import_button_is_not_controller_selectable() {
        let mut app = GuiApp::new_without_controller_worker();

        assert!(!app.visible_form_controls().contains(&FormControl::Import));

        app.state.morrowind_ini = "Morrowind.ini".to_owned();

        assert!(app.visible_form_controls().contains(&FormControl::Import));
    }

    #[test]
    fn controller_adjustments_cycle_multivalue_controls() {
        assert_eq!(
            cycled_language(UiLanguage::English, FormAdjustment::Previous),
            UiLanguage::Spanish
        );
        assert_eq!(
            cycled_encoding(None, FormAdjustment::Next),
            Some(TextEncoding::Win1250)
        );
        assert_eq!(
            cycled_output_mode(GuiOutputMode::SaveAs, FormAdjustment::Next, false),
            GuiOutputMode::PreviewOnly
        );
        assert_eq!(
            cycled_result_panel(ResultPanel::GeneratedCfg, FormAdjustment::Next),
            ResultPanel::Errors
        );
    }

    #[test]
    fn relocated_save_as_preview_uses_resolved_paths() {
        let dir = unique_test_dir("gui-relocated-preview");
        let source_dir = dir.join("source");
        let output_dir = dir.join("output");
        std::fs::create_dir_all(source_dir.join("mods")).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let cfg = source_dir.join("openmw.cfg");
        std::fs::write(&cfg, "data=mods\n").unwrap();

        let state = ImportFormState {
            existing_cfg: cfg.to_string_lossy().into_owned(),
            output_mode: GuiOutputMode::SaveAs,
            output_path: output_dir.join("openmw.cfg").to_string_lossy().into_owned(),
            ..ImportFormState::default()
        };
        let result = ImportResult {
            cfg: dream_ini::MultiMap::new(),
            warnings: Vec::new(),
            events: Vec::new(),
            changed_keys: BTreeSet::new(),
        };

        let preview = state.serialize_result(&result).unwrap();
        assert!(preview.contains(&format!("data={}\n", source_dir.join("mods").display())));
        assert!(!preview.contains("data=mods\n"));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relocated_save_as_preview_filters_composed_resource_vfs_data_dir() {
        let dir = unique_test_dir("gui-relocated-preview-resource-vfs");
        let source_dir = dir.join("source");
        let output_dir = dir.join("output");
        let resources = source_dir.join("resources");
        std::fs::create_dir_all(resources.join("vfs")).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let cfg = source_dir.join("openmw.cfg");
        std::fs::write(&cfg, "resources=resources\n").unwrap();

        let state = ImportFormState {
            existing_cfg: cfg.to_string_lossy().into_owned(),
            output_mode: GuiOutputMode::SaveAs,
            output_path: output_dir.join("openmw.cfg").to_string_lossy().into_owned(),
            ..ImportFormState::default()
        };
        let result = empty_import_result();

        let preview = state.serialize_result(&result).unwrap();
        assert!(preview.contains(&format!("resources={}\n", resources.display())));
        assert!(!preview.contains(&format!("data={}\n", resources.join("vfs").display())));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relocated_save_as_write_filters_composed_resource_vfs_data_dir() {
        let dir = unique_test_dir("gui-relocated-write-resource-vfs");
        let source_dir = dir.join("source");
        let output_dir = dir.join("output");
        let resources = source_dir.join("resources");
        std::fs::create_dir_all(resources.join("vfs")).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        let cfg = source_dir.join("openmw.cfg");
        let output_cfg = output_dir.join("openmw.cfg");
        std::fs::write(&cfg, "resources=resources\n").unwrap();

        let state = ImportFormState {
            existing_cfg: cfg.to_string_lossy().into_owned(),
            output_mode: GuiOutputMode::SaveAs,
            output_path: output_cfg.to_string_lossy().into_owned(),
            ..ImportFormState::default()
        };
        let result = empty_import_result();

        state.write_output(&result).unwrap();
        let written = std::fs::read_to_string(output_cfg).unwrap();
        assert!(written.contains(&format!("resources={}\n", resources.display())));
        assert!(!written.contains(&format!("data={}\n", resources.join("vfs").display())));

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn empty_import_result() -> ImportResult {
        ImportResult {
            cfg: dream_ini::MultiMap::new(),
            warnings: Vec::new(),
            events: Vec::new(),
            changed_keys: BTreeSet::new(),
        }
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dream-ini-gui-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
