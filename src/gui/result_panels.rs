// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    all(feature = "portmaster-gui", not(feature = "gui")),
    allow(dead_code)
)]

use super::form_nav::FormAdjustment;
use super::localization::{Localizer, UiText};
use super::{GuiImportError, GuiImportResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ResultPanel {
    Errors,
    Warnings,
    Events,
    #[default]
    GeneratedCfg,
}

pub(super) fn result_tab(
    ui: &mut egui::Ui,
    selected: &mut ResultPanel,
    panel: ResultPanel,
    label: &str,
) {
    if ui.selectable_label(*selected == panel, label).clicked() {
        *selected = panel;
    }
}

pub(super) fn cycled_result_panel(panel: ResultPanel, adjustment: FormAdjustment) -> ResultPanel {
    let items = [
        ResultPanel::Errors,
        ResultPanel::Warnings,
        ResultPanel::Events,
        ResultPanel::GeneratedCfg,
    ];
    let Some(index) = items.iter().position(|item| *item == panel) else {
        return panel;
    };
    let next_index = match adjustment {
        FormAdjustment::Previous if index == 0 => items.len() - 1,
        FormAdjustment::Previous => index - 1,
        FormAdjustment::Next if index + 1 == items.len() => 0,
        FormAdjustment::Next => index + 1,
    };
    items.get(next_index).copied().unwrap_or(panel)
}

pub(super) fn show_error_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
    match result {
        GuiImportResult::Success { .. } => {
            ui.label(localizer.text(UiText::NoErrors));
        }
        GuiImportResult::Error { error } => {
            ui.colored_label(egui::Color32::RED, error_title(localizer, error));
        }
    }
}

pub(super) fn show_warning_panel(
    ui: &mut egui::Ui,
    localizer: Localizer,
    result: &GuiImportResult,
) {
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

pub(super) fn show_event_panel(ui: &mut egui::Ui, localizer: Localizer, result: &GuiImportResult) {
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

pub(super) fn show_generated_cfg_panel(
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
