// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    all(feature = "portmaster-gui", not(feature = "gui")),
    allow(dead_code, unused_imports)
)]

use std::path::Path;
#[cfg(feature = "gui")]
use std::process::ExitCode;

use dream_ini::TextEncoding;

use self::controller::{ControllerAction, ControllerEvent};
use self::file_picker::{PathPickerState, PathTarget, PickOutcome};
use self::form_nav::{
    ExistingCfgVisibility, FormAdjustment, FormControl, FormSelectionStep, ImportVisibility,
    ResultVisibility, cycled_encoding, cycled_language, cycled_output_mode,
};
use self::form_state::{GuiImportResult, GuiOutputMode, ImportFormState};
use self::localization::{Localizer, UiLanguage, UiText};
use self::osk::{OskOutcome, OskState, show_osk_overlay};
use self::path_helpers::optional_path;
use self::path_widgets::{
    controller_marker_width, path_file_row, path_folder_row, path_label_width, path_save_file_row,
};
use self::result_panels::{
    ResultPanel, cycled_result_panel, result_tab, show_error_panel, show_event_panel,
    show_generated_cfg_panel, show_warning_panel,
};
#[cfg(test)]
use self::scroll::{CONTROLLER_PREVIEW_PAGE_SCROLL_PIXELS, CONTROLLER_PREVIEW_SCROLL_PIXELS};
use self::scroll::{
    PreviewPageScroll, PreviewScroll, generated_cfg_page_scroll_delta, generated_cfg_scroll_delta,
    path_picker_scroll_delta,
};
#[cfg(feature = "gui")]
use crate::desktop_entry::{APP_ID, APP_NAME};

mod controller;
mod file_picker;
mod form_nav;
mod form_state;
mod localization;
mod osk;
mod path_helpers;
mod path_widgets;
#[cfg(feature = "portmaster-gui")]
#[cfg_attr(feature = "gui", allow(dead_code))]
mod portmaster;
mod result_panels;
mod scroll;

const CFG_KEY_DATA_LOCAL: &str = "data-local";
const CFG_KEY_RESOURCES: &str = "resources";
const CFG_KEY_USERDATA: &str = "user-data";
const MORROWIND_INI_LABEL: &str = "Morrowind.ini";
const OPENMW_CFG_LABEL: &str = "openmw.cfg";

#[cfg(all(feature = "portmaster-gui", not(feature = "gui")))]
pub(crate) use portmaster::run as run_portmaster_gui;

#[cfg(feature = "gui")]
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
        Box::new(|creation_context| {
            Ok(Box::new(DesktopGuiApp::new(
                creation_context.egui_ctx.clone(),
            )))
        }),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "gui")]
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

#[cfg(feature = "gui")]
struct DesktopGuiApp {
    app: GuiApp,
    shell: DesktopGuiShell,
}

#[cfg(feature = "gui")]
impl DesktopGuiApp {
    fn new(context: egui::Context) -> Self {
        Self {
            app: GuiApp::new(context),
            shell: DesktopGuiShell,
        }
    }
}

#[cfg(feature = "gui")]
impl eframe::App for DesktopGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui, &mut self.shell);
    }
}

trait GuiShell {
    fn request_exit(&mut self, context: &egui::Context);

    fn copy_text(&mut self, context: &egui::Context, text: String);
}

#[cfg(feature = "gui")]
#[derive(Debug, Default)]
struct DesktopGuiShell;

#[cfg(feature = "gui")]
impl GuiShell for DesktopGuiShell {
    fn request_exit(&mut self, context: &egui::Context) {
        context.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn copy_text(&mut self, context: &egui::Context, text: String) {
        context.copy_text(text);
    }
}

enum GuiMode {
    ImportForm,
    PathPicker(PathPickerState),
    Osk(OskState),
}

impl GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, shell: &mut impl GuiShell) {
        let context = ui.ctx().clone();
        let controller_actions = self.drain_controller_actions();
        let controller_actions_consumed =
            self.handle_controller_actions(shell, &context, &controller_actions);
        self.handle_shortcuts(shell, &context);
        let controller_actions_for_ui: &[ControllerAction] = if controller_actions_consumed {
            &[]
        } else {
            &controller_actions
        };
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.show_current_mode(ui, shell, controller_actions_for_ui);
        });
    }
}

impl GuiApp {
    fn drain_controller_actions(&mut self) -> Vec<ControllerAction> {
        let mut actions = Vec::<(ControllerAction, bool)>::new();
        for event in self.controller.drain_events() {
            match event {
                ControllerEvent::PurgeQueuedActions => {
                    actions.retain(|(_, repeat)| !*repeat);
                }
                ControllerEvent::RepeatAction(action) => {
                    self.controller_navigation_visible = true;
                    actions.push((action, true));
                }
                event => {
                    if let Some(action) = self.handle_controller_event(event) {
                        actions.push((action, false));
                    }
                }
            }
        }
        actions.into_iter().map(|(action, _)| action).collect()
    }

    fn handle_controller_event(&mut self, event: ControllerEvent) -> Option<ControllerAction> {
        match event {
            ControllerEvent::Action(action) | ControllerEvent::RepeatAction(action) => {
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
        shell: &mut impl GuiShell,
        context: &egui::Context,
        actions: &[ControllerAction],
    ) -> bool {
        if actions.is_empty() {
            return false;
        }

        if matches!(self.mode, GuiMode::Osk(_)) {
            self.handle_osk_controller_actions(actions);
            return true;
        }

        if !matches!(self.mode, GuiMode::ImportForm) {
            return false;
        }

        self.ensure_selected_form_control_available();

        for action in actions {
            match action {
                ControllerAction::Cancel => {
                    shell.request_exit(context);
                    return true;
                }
                ControllerAction::Up => self.move_form_selection(FormSelectionStep::Previous),
                ControllerAction::Down => self.move_form_selection(FormSelectionStep::Next),
                ControllerAction::Left => {
                    self.adjust_selected_form_control(FormAdjustment::Previous);
                }
                ControllerAction::Right => self.adjust_selected_form_control(FormAdjustment::Next),
                ControllerAction::Accept => self.activate_selected_form_control(shell, context),
                ControllerAction::ClearCurrent => self.clear_selected_form_control(),
                ControllerAction::Secondary | ControllerAction::Space => {}
                ControllerAction::SelectCurrent => self.run_import_if_enabled(),
                ControllerAction::ToggleHiddenDirectories => {
                    self.page_generated_cfg_preview(PreviewPageScroll::Up);
                }
                ControllerAction::PagePreviewDown => {
                    self.page_generated_cfg_preview(PreviewPageScroll::Down);
                }
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

    fn handle_shortcuts(&mut self, shell: &mut impl GuiShell, context: &egui::Context) {
        if matches!(self.mode, GuiMode::Osk(_)) {
            if context.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.mode = GuiMode::ImportForm;
            } else if context.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.commit_osk();
            }
            return;
        }
        if !matches!(self.mode, GuiMode::ImportForm) {
            return;
        }
        if context.input(|input| input.key_pressed(egui::Key::Escape)) {
            shell.request_exit(context);
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

    fn page_generated_cfg_preview(&mut self, direction: PreviewPageScroll) {
        if self.selected_result_panel != ResultPanel::GeneratedCfg
            || !matches!(self.result, Some(GuiImportResult::Success { .. }))
        {
            return;
        }
        self.generated_cfg_scroll_delta += generated_cfg_page_scroll_delta(direction);
    }

    fn visible_form_controls(&self) -> Vec<FormControl> {
        form_nav::visible_form_controls(
            self.state.output_mode,
            if optional_path(&self.state.existing_cfg).is_some() {
                ExistingCfgVisibility::Present
            } else {
                ExistingCfgVisibility::Missing
            },
            if self.state.disabled_import_reason().is_none() {
                ImportVisibility::Enabled
            } else {
                ImportVisibility::Disabled
            },
            match self.result {
                Some(GuiImportResult::Success { .. }) => ResultVisibility::Success,
                Some(GuiImportResult::Error { .. }) => ResultVisibility::Error,
                None => ResultVisibility::Hidden,
            },
        )
    }

    fn ensure_selected_form_control_available(&mut self) {
        let controls = self.visible_form_controls();
        self.selected_form_control =
            form_nav::ensure_available_control(self.selected_form_control, &controls);
    }

    fn scroll_selected_form_control_into_view(&mut self, ui: &mut egui::Ui, control: FormControl) {
        if self.pending_form_scroll == Some(control) {
            ui.scroll_to_cursor(Some(egui::Align::Center));
            self.pending_form_scroll = None;
        }
    }

    fn move_form_selection(&mut self, step: FormSelectionStep) {
        let controls = self.visible_form_controls();
        if let Some(next_control) =
            form_nav::move_form_selection(self.selected_form_control, &controls, step)
        {
            self.selected_form_control = next_control;
            self.pending_form_scroll = Some(self.selected_form_control);
        }
    }

    fn activate_selected_form_control(
        &mut self,
        shell: &mut impl GuiShell,
        context: &egui::Context,
    ) {
        match self.selected_form_control {
            FormControl::Language => self.cycle_language(FormAdjustment::Next),
            FormControl::MorrowindIni => self.open_osk_for_path(PathTarget::MorrowindIni),
            FormControl::MorrowindIniBrowse => self.open_form_path_picker(PathTarget::MorrowindIni),
            FormControl::ExistingCfg => self.open_osk_for_path(PathTarget::ExistingOpenmwCfg),
            FormControl::ExistingCfgBrowse => {
                self.open_form_path_picker(PathTarget::ExistingOpenmwCfg);
            }
            FormControl::Encoding => self.cycle_encoding(FormAdjustment::Next),
            FormControl::ImportFonts => self.state.import_fonts = !self.state.import_fonts,
            FormControl::ImportArchives => self.state.import_archives = !self.state.import_archives,
            FormControl::ImportContentFiles => {
                self.state.import_content_files = !self.state.import_content_files;
            }
            FormControl::ExplicitSearchPath => self.open_osk_for_path(PathTarget::GameDataDir),
            FormControl::ExplicitSearchPathBrowse => {
                self.open_form_path_picker(PathTarget::GameDataDir);
            }
            FormControl::DataLocal => self.open_osk_for_path(PathTarget::DataLocalDir),
            FormControl::DataLocalBrowse => self.open_form_path_picker(PathTarget::DataLocalDir),
            FormControl::Resources => self.open_osk_for_path(PathTarget::ResourcesDir),
            FormControl::ResourcesBrowse => self.open_form_path_picker(PathTarget::ResourcesDir),
            FormControl::UserData => self.open_osk_for_path(PathTarget::UserDataDir),
            FormControl::UserDataBrowse => self.open_form_path_picker(PathTarget::UserDataDir),
            FormControl::OutputPreview => self.state.output_mode = GuiOutputMode::PreviewOnly,
            FormControl::OutputSaveAs => {
                self.state.output_mode = GuiOutputMode::SaveAs;
                self.selected_form_control = FormControl::OutputPath;
                self.pending_form_scroll = Some(FormControl::OutputPath);
            }
            FormControl::OutputPath => self.open_osk_for_path(PathTarget::OutputCfg),
            FormControl::OutputPathBrowse => self.open_form_path_picker(PathTarget::OutputCfg),
            FormControl::OutputUpdateExisting => {
                if optional_path(&self.state.existing_cfg).is_some() {
                    self.state.output_mode = GuiOutputMode::UpdateExistingCfg;
                }
            }
            FormControl::Import => self.run_import_if_enabled(),
            FormControl::ResultTabs => self.cycle_result_panel(FormAdjustment::Next),
            FormControl::CopyResult => self.copy_result_to_clipboard(shell, context),
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
            | FormControl::MorrowindIniBrowse
            | FormControl::ExistingCfg
            | FormControl::ExistingCfgBrowse
            | FormControl::ExplicitSearchPath
            | FormControl::ExplicitSearchPathBrowse
            | FormControl::DataLocal
            | FormControl::DataLocalBrowse
            | FormControl::Resources
            | FormControl::ResourcesBrowse
            | FormControl::UserData
            | FormControl::UserDataBrowse
            | FormControl::OutputPath
            | FormControl::OutputPathBrowse
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
            | FormControl::MorrowindIniBrowse
            | FormControl::ExistingCfgBrowse
            | FormControl::Encoding
            | FormControl::ImportFonts
            | FormControl::ImportArchives
            | FormControl::ImportContentFiles
            | FormControl::ExplicitSearchPathBrowse
            | FormControl::DataLocalBrowse
            | FormControl::ResourcesBrowse
            | FormControl::UserDataBrowse
            | FormControl::OutputPreview
            | FormControl::OutputSaveAs
            | FormControl::OutputPathBrowse
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

    fn open_osk_for_path(&mut self, target: PathTarget) {
        self.mode = GuiMode::Osk(OskState::new(target, self.path_value(target).to_owned()));
    }

    fn handle_osk_controller_actions(&mut self, actions: &[ControllerAction]) {
        let mut outcome = OskOutcome::None;
        if let GuiMode::Osk(osk) = &mut self.mode {
            for action in actions {
                outcome = osk.handle_controller_action(*action);
                if !matches!(outcome, OskOutcome::None) {
                    break;
                }
            }
        }
        self.apply_osk_outcome(outcome);
    }

    fn commit_osk(&mut self) {
        let outcome = match &self.mode {
            GuiMode::Osk(osk) => osk.commit_outcome(),
            GuiMode::ImportForm | GuiMode::PathPicker(_) => OskOutcome::None,
        };
        self.apply_osk_outcome(outcome);
    }

    fn apply_osk_outcome(&mut self, outcome: OskOutcome) {
        match outcome {
            OskOutcome::None => {}
            OskOutcome::Cancel => self.mode = GuiMode::ImportForm,
            OskOutcome::Commit { target, value } => {
                *self.path_value_mut(target) = value;
                self.mode = GuiMode::ImportForm;
            }
        }
    }

    fn path_value(&self, target: PathTarget) -> &str {
        match target {
            PathTarget::MorrowindIni => &self.state.morrowind_ini,
            PathTarget::ExistingOpenmwCfg => &self.state.existing_cfg,
            PathTarget::OutputCfg => &self.state.output_path,
            PathTarget::GameDataDir => &self.state.explicit_search_path,
            PathTarget::DataLocalDir => &self.state.data_local,
            PathTarget::ResourcesDir => &self.state.resources,
            PathTarget::UserDataDir => &self.state.user_data,
        }
    }

    fn path_value_mut(&mut self, target: PathTarget) -> &mut String {
        match target {
            PathTarget::MorrowindIni => &mut self.state.morrowind_ini,
            PathTarget::ExistingOpenmwCfg => &mut self.state.existing_cfg,
            PathTarget::OutputCfg => &mut self.state.output_path,
            PathTarget::GameDataDir => &mut self.state.explicit_search_path,
            PathTarget::DataLocalDir => &mut self.state.data_local,
            PathTarget::ResourcesDir => &mut self.state.resources,
            PathTarget::UserDataDir => &mut self.state.user_data,
        }
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

    fn copy_result_to_clipboard(&self, shell: &mut impl GuiShell, context: &egui::Context) {
        if let Some(GuiImportResult::Success { cfg_text, .. }) = &self.result {
            shell.copy_text(context, cfg_text.clone());
        }
    }

    fn show_current_mode(
        &mut self,
        ui: &mut egui::Ui,
        shell: &mut impl GuiShell,
        controller_actions: &[ControllerAction],
    ) {
        match &mut self.mode {
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
            GuiMode::ImportForm | GuiMode::Osk(_) => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_form(ui, shell));
                let outcome = if let GuiMode::Osk(osk) = &mut self.mode {
                    show_osk_overlay(ui, self.localizer, osk)
                } else {
                    OskOutcome::None
                };
                self.apply_osk_outcome(outcome);
            }
        }
    }

    fn show_form(&mut self, ui: &mut egui::Ui, shell: &mut impl GuiShell) {
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
            self.show_results(ui, shell);
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
            &self.form_label(
                FormControl::MorrowindIniBrowse,
                self.localizer.text(UiText::Browse),
            ),
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
            &self.form_label(
                FormControl::ExistingCfgBrowse,
                self.localizer.text(UiText::Browse),
            ),
            &mut self.state.existing_cfg,
        ) {
            let current_value = self.state.existing_cfg.clone();
            self.open_path_picker(PathTarget::ExistingOpenmwCfg, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::ExistingCfg);
        self.scroll_selected_form_control_into_view(ui, FormControl::MorrowindIniBrowse);
        self.scroll_selected_form_control_into_view(ui, FormControl::ExistingCfgBrowse);
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
            &self.form_label(
                FormControl::ExplicitSearchPathBrowse,
                self.localizer.text(UiText::Browse),
            ),
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
            &self.form_label(
                FormControl::DataLocalBrowse,
                self.localizer.text(UiText::Browse),
            ),
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
            &self.form_label(
                FormControl::ResourcesBrowse,
                self.localizer.text(UiText::Browse),
            ),
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
            &self.form_label(
                FormControl::UserDataBrowse,
                self.localizer.text(UiText::Browse),
            ),
            &mut self.state.user_data,
            Some(self.localizer.text(UiText::UserDataTooltip)),
        ) {
            let current_value = self.state.user_data.clone();
            self.open_path_picker(PathTarget::UserDataDir, &current_value);
        }
        self.scroll_selected_form_control_into_view(ui, FormControl::UserData);
        self.scroll_selected_form_control_into_view(ui, FormControl::ExplicitSearchPathBrowse);
        self.scroll_selected_form_control_into_view(ui, FormControl::DataLocalBrowse);
        self.scroll_selected_form_control_into_view(ui, FormControl::ResourcesBrowse);
        self.scroll_selected_form_control_into_view(ui, FormControl::UserDataBrowse);
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
                    ui.selectable_value(
                        &mut language,
                        UiLanguage::Swedish,
                        self.localizer.text(UiText::SwedishLanguage),
                    );
                });
            self.localizer.set_language(language);
        });
        self.scroll_selected_form_control_into_view(ui, FormControl::Language);
    }

    fn show_results(&mut self, ui: &mut egui::Ui, shell: &mut impl GuiShell) {
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
                    shell.copy_text(ui.ctx(), text.clone());
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
                &self.form_label(
                    FormControl::OutputPathBrowse,
                    self.localizer.text(UiText::Browse),
                ),
                &mut self.state.output_path,
            ) {
                let current_value = self.state.output_path.clone();
                self.open_path_picker(PathTarget::OutputCfg, &current_value);
            }
        });
        self.scroll_selected_form_control_into_view(ui, FormControl::OutputPath);
        self.scroll_selected_form_control_into_view(ui, FormControl::OutputPathBrowse);
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
        UiLanguage::Swedish => localizer.text(UiText::SwedishLanguage),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use dream_ini::ImportResult;

    #[derive(Debug, Default)]
    struct TestGuiShell {
        exit_requested: bool,
        copied_text: Option<String>,
    }

    impl GuiShell for TestGuiShell {
        fn request_exit(&mut self, _context: &egui::Context) {
            self.exit_requested = true;
        }

        fn copy_text(&mut self, _context: &egui::Context, text: String) {
            self.copied_text = Some(text);
        }
    }

    #[test]
    fn default_gui_encoding_is_not_an_override() {
        assert_eq!(ImportFormState::default().import_options().encoding, None);
    }

    #[test]
    #[cfg(feature = "gui")]
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
        assert!(
            app.visible_form_controls()
                .contains(&FormControl::OutputPathBrowse)
        );

        app.state.existing_cfg = "openmw.cfg".to_owned();
        assert!(
            app.visible_form_controls()
                .contains(&FormControl::OutputUpdateExisting)
        );
    }

    #[test]
    fn controller_path_fields_and_browse_controls_are_separate() {
        let app = GuiApp::new_without_controller_worker();
        let controls = app.visible_form_controls();

        assert!(controls.contains(&FormControl::MorrowindIni));
        assert!(controls.contains(&FormControl::MorrowindIniBrowse));
        assert!(controls.contains(&FormControl::ExistingCfg));
        assert!(controls.contains(&FormControl::ExistingCfgBrowse));
        assert!(controls.contains(&FormControl::ExplicitSearchPath));
        assert!(controls.contains(&FormControl::ExplicitSearchPathBrowse));
    }

    #[test]
    fn form_controller_save_as_activation_selects_output_path() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.selected_form_control = FormControl::OutputSaveAs;

        app.activate_selected_form_control(&mut shell, &egui::Context::default());

        assert_eq!(app.state.output_mode, GuiOutputMode::SaveAs);
        assert_eq!(app.selected_form_control, FormControl::OutputPath);
    }

    #[test]
    fn form_controller_accept_opens_path_field_osk() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.selected_form_control = FormControl::MorrowindIni;

        app.activate_selected_form_control(&mut shell, &egui::Context::default());

        assert!(matches!(app.mode, GuiMode::Osk(_)));
    }

    #[test]
    fn form_controller_accept_opens_selected_path_browse_picker() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.selected_form_control = FormControl::MorrowindIniBrowse;

        app.activate_selected_form_control(&mut shell, &egui::Context::default());

        assert!(matches!(app.mode, GuiMode::PathPicker(_)));
    }

    #[test]
    fn form_controller_cancel_requests_shell_exit() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();

        let consumed = app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::Cancel],
        );

        assert!(consumed);
        assert!(shell.exit_requested);
    }

    #[test]
    fn form_controller_copy_result_uses_shell_clipboard() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_form_control = FormControl::CopyResult;

        app.activate_selected_form_control(&mut shell, &egui::Context::default());

        assert_eq!(shell.copied_text.as_deref(), Some("fallback=1\n"));
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
        let mut shell = TestGuiShell::default();
        app.selected_form_control = FormControl::ExplicitSearchPathBrowse;

        let consumed = app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::Accept],
        );

        assert!(consumed);
        assert!(matches!(app.mode, GuiMode::PathPicker(_)));
    }

    #[test]
    fn form_controller_consumes_action_that_opens_osk() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.selected_form_control = FormControl::ExplicitSearchPath;

        let consumed = app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::Accept],
        );

        assert!(consumed);
        assert!(matches!(app.mode, GuiMode::Osk(_)));
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
    fn osk_cancel_does_not_mutate_path() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.state.morrowind_ini = "original.ini".to_owned();
        app.selected_form_control = FormControl::MorrowindIni;
        app.activate_selected_form_control(&mut shell, &egui::Context::default());
        if let GuiMode::Osk(osk) = &mut app.mode {
            osk.set_buffer_for_test("changed.ini".to_owned());
        }

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::Cancel],
        );

        assert_eq!(app.state.morrowind_ini, "original.ini");
        assert!(matches!(app.mode, GuiMode::ImportForm));
        assert!(!shell.exit_requested);
    }

    #[test]
    fn osk_start_commits_path() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.state.output_path = "old.cfg".to_owned();
        app.open_osk_for_path(PathTarget::OutputCfg);
        if let GuiMode::Osk(osk) = &mut app.mode {
            osk.set_buffer_for_test("new.cfg".to_owned());
        }

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::SelectCurrent],
        );

        assert_eq!(app.state.output_path, "new.cfg");
        assert!(matches!(app.mode, GuiMode::ImportForm));
    }

    #[test]
    fn osk_ok_button_commits_path() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.open_osk_for_path(PathTarget::DataLocalDir);
        if let GuiMode::Osk(osk) = &mut app.mode {
            osk.set_buffer_for_test("data-local".to_owned());
            osk.select_ok_for_test();
        }

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::Accept],
        );

        assert_eq!(app.state.data_local, "data-local");
        assert!(matches!(app.mode, GuiMode::ImportForm));
    }

    #[test]
    fn osk_backspace_and_clear_edit_scratch_buffer_only() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.state.resources = "original".to_owned();
        app.open_osk_for_path(PathTarget::ResourcesDir);

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::ClearCurrent],
        );
        if let GuiMode::Osk(osk) = &mut app.mode {
            assert_eq!(osk.buffer_for_test(), "origina");
            osk.select_clear_for_test();
        }
        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::Accept],
        );

        assert_eq!(app.state.resources, "original");
        assert!(matches!(&app.mode, GuiMode::Osk(osk) if osk.buffer_for_test().is_empty()));
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
    fn controller_purge_event_collapses_repeatable_actions_drained_in_same_frame() {
        let mut app = GuiApp::new_without_controller_worker();
        let (controller, sender) = controller::Controller::with_test_sender();
        app.controller = controller;

        assert!(sender.send(ControllerEvent::Action(ControllerAction::Down)));
        assert!(sender.send(ControllerEvent::RepeatAction(ControllerAction::Down)));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Accept)));
        assert!(sender.send(ControllerEvent::PurgeQueuedActions));
        assert!(sender.send(ControllerEvent::Action(ControllerAction::Up)));

        assert_eq!(
            app.drain_controller_actions(),
            vec![
                ControllerAction::Down,
                ControllerAction::Accept,
                ControllerAction::Up,
            ]
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
        let mut shell = TestGuiShell::default();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_result_panel = ResultPanel::GeneratedCfg;

        app.handle_controller_actions(
            &mut shell,
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
        let mut shell = TestGuiShell::default();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_result_panel = ResultPanel::Warnings;

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::ScrollPreviewDown],
        );

        assert!(app.generated_cfg_scroll_delta.length_sq() < f32::EPSILON);
    }

    #[test]
    fn shoulder_buttons_page_generated_cfg_preview() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_result_panel = ResultPanel::GeneratedCfg;

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::ToggleHiddenDirectories],
        );
        assert!(
            (app.generated_cfg_scroll_delta.y - CONTROLLER_PREVIEW_PAGE_SCROLL_PIXELS).abs()
                < f32::EPSILON
        );

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[ControllerAction::PagePreviewDown],
        );
        assert!(app.generated_cfg_scroll_delta.y.abs() < f32::EPSILON);
    }

    #[test]
    fn shoulder_buttons_ignore_non_generated_result_panels() {
        let mut app = GuiApp::new_without_controller_worker();
        let mut shell = TestGuiShell::default();
        app.result = Some(GuiImportResult::Success {
            cfg_text: "fallback=1\n".to_owned(),
            warnings: Vec::new(),
            events: Vec::new(),
            output_path: None,
        });
        app.selected_result_panel = ResultPanel::Warnings;

        app.handle_controller_actions(
            &mut shell,
            &egui::Context::default(),
            &[
                ControllerAction::ToggleHiddenDirectories,
                ControllerAction::PagePreviewDown,
            ],
        );

        assert!(app.generated_cfg_scroll_delta.length_sq() < f32::EPSILON);
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
            UiLanguage::Swedish
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
        let temp_dir = std::env::temp_dir();
        let temp_dir = temp_dir.canonicalize().unwrap_or(temp_dir);
        temp_dir.join(format!(
            "dream-ini-gui-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
