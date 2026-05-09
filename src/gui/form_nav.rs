// SPDX-License-Identifier: GPL-3.0-only

use dream_ini::TextEncoding;

use super::GuiOutputMode;
use super::localization::UiLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FormControl {
    Language,
    MorrowindIni,
    MorrowindIniBrowse,
    ExistingCfg,
    ExistingCfgBrowse,
    Encoding,
    ImportFonts,
    ImportArchives,
    ImportContentFiles,
    ExplicitSearchPath,
    ExplicitSearchPathBrowse,
    DataLocal,
    DataLocalBrowse,
    Resources,
    ResourcesBrowse,
    UserData,
    UserDataBrowse,
    OutputPreview,
    OutputSaveAs,
    OutputPath,
    OutputPathBrowse,
    OutputUpdateExisting,
    Import,
    ResultTabs,
    CopyResult,
    ClearResult,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FormSelectionStep {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FormAdjustment {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ExistingCfgVisibility {
    Missing,
    Present,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ImportVisibility {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ResultVisibility {
    Hidden,
    Error,
    Success,
}

pub(super) fn cycled_language(language: UiLanguage, adjustment: FormAdjustment) -> UiLanguage {
    cycle_item(
        &[
            UiLanguage::English,
            UiLanguage::French,
            UiLanguage::German,
            UiLanguage::Russian,
            UiLanguage::Spanish,
            UiLanguage::Swedish,
        ],
        language,
        adjustment,
    )
}

pub(super) fn cycled_encoding(
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

pub(super) fn cycled_output_mode(
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

pub(super) fn visible_form_controls(
    output_mode: GuiOutputMode,
    existing_cfg: ExistingCfgVisibility,
    import: ImportVisibility,
    result: ResultVisibility,
) -> Vec<FormControl> {
    let mut controls = vec![
        FormControl::Language,
        FormControl::MorrowindIni,
        FormControl::MorrowindIniBrowse,
        FormControl::ExistingCfg,
        FormControl::ExistingCfgBrowse,
        FormControl::Encoding,
        FormControl::ImportFonts,
        FormControl::ImportArchives,
        FormControl::ImportContentFiles,
        FormControl::ExplicitSearchPath,
        FormControl::ExplicitSearchPathBrowse,
        FormControl::DataLocal,
        FormControl::DataLocalBrowse,
        FormControl::Resources,
        FormControl::ResourcesBrowse,
        FormControl::UserData,
        FormControl::UserDataBrowse,
        FormControl::OutputPreview,
        FormControl::OutputSaveAs,
    ];
    if output_mode == GuiOutputMode::SaveAs {
        controls.push(FormControl::OutputPath);
        controls.push(FormControl::OutputPathBrowse);
    }
    if matches!(existing_cfg, ExistingCfgVisibility::Present) {
        controls.push(FormControl::OutputUpdateExisting);
    }
    if matches!(import, ImportVisibility::Enabled) {
        controls.push(FormControl::Import);
    }
    if matches!(result, ResultVisibility::Error | ResultVisibility::Success) {
        controls.push(FormControl::ResultTabs);
        controls.push(FormControl::ClearResult);
        if matches!(result, ResultVisibility::Success) {
            controls.push(FormControl::CopyResult);
        }
    }
    controls
}

pub(super) fn ensure_available_control(
    selected: FormControl,
    controls: &[FormControl],
) -> FormControl {
    if controls.contains(&selected) {
        selected
    } else {
        controls
            .first()
            .copied()
            .unwrap_or(FormControl::MorrowindIni)
    }
}

pub(super) fn move_form_selection(
    current: FormControl,
    controls: &[FormControl],
    step: FormSelectionStep,
) -> Option<FormControl> {
    if controls.is_empty() {
        return None;
    }
    let current_index = controls.iter().position(|control| *control == current);
    let next_index = match (step, current_index) {
        (FormSelectionStep::Previous, Some(0) | None) => controls.len() - 1,
        (FormSelectionStep::Previous, Some(index)) => index - 1,
        (FormSelectionStep::Next, Some(index)) if index + 1 < controls.len() => index + 1,
        (FormSelectionStep::Next, Some(_) | None) => 0,
    };
    controls.get(next_index).copied()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_controls_include_output_path_controls_only_in_save_as_mode() {
        let preview = visible_form_controls(
            GuiOutputMode::PreviewOnly,
            ExistingCfgVisibility::Missing,
            ImportVisibility::Disabled,
            ResultVisibility::Hidden,
        );
        assert!(!preview.contains(&FormControl::OutputPath));
        assert!(!preview.contains(&FormControl::OutputPathBrowse));

        let save_as = visible_form_controls(
            GuiOutputMode::SaveAs,
            ExistingCfgVisibility::Missing,
            ImportVisibility::Disabled,
            ResultVisibility::Hidden,
        );
        assert!(save_as.contains(&FormControl::OutputPath));
        assert!(save_as.contains(&FormControl::OutputPathBrowse));
    }

    #[test]
    fn visible_controls_include_result_tabs_clear_and_copy_only_for_success() {
        let error = visible_form_controls(
            GuiOutputMode::PreviewOnly,
            ExistingCfgVisibility::Missing,
            ImportVisibility::Disabled,
            ResultVisibility::Error,
        );
        assert!(error.contains(&FormControl::ResultTabs));
        assert!(error.contains(&FormControl::ClearResult));
        assert!(!error.contains(&FormControl::CopyResult));

        let success = visible_form_controls(
            GuiOutputMode::PreviewOnly,
            ExistingCfgVisibility::Missing,
            ImportVisibility::Disabled,
            ResultVisibility::Success,
        );
        assert!(success.contains(&FormControl::ResultTabs));
        assert!(success.contains(&FormControl::ClearResult));
        assert!(success.contains(&FormControl::CopyResult));
    }

    #[test]
    fn next_previous_selection_wraps() {
        let controls = [
            FormControl::Language,
            FormControl::MorrowindIni,
            FormControl::MorrowindIniBrowse,
        ];

        assert_eq!(
            move_form_selection(
                FormControl::MorrowindIniBrowse,
                &controls,
                FormSelectionStep::Next
            ),
            Some(FormControl::Language)
        );
        assert_eq!(
            move_form_selection(
                FormControl::Language,
                &controls,
                FormSelectionStep::Previous
            ),
            Some(FormControl::MorrowindIniBrowse)
        );
    }

    #[test]
    fn cycled_output_mode_excludes_update_existing_without_existing_cfg() {
        assert_eq!(
            cycled_output_mode(GuiOutputMode::SaveAs, FormAdjustment::Next, false),
            GuiOutputMode::PreviewOnly
        );
        assert_eq!(
            cycled_output_mode(GuiOutputMode::PreviewOnly, FormAdjustment::Previous, false),
            GuiOutputMode::SaveAs
        );
    }
}
