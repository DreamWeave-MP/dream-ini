// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    all(feature = "portmaster-gui", not(feature = "gui")),
    allow(dead_code)
)]

use super::form_nav::FormAdjustment;
use super::form_state::{GuiImportError, GuiImportResult};
use super::localization::{Localizer, UiText};
use std::ops::Range;

const GENERATED_CFG_ROW_OVERSCAN: usize = 2;
const GENERATED_CFG_COLUMN_GAP: f32 = 8.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ResultPanel {
    Errors,
    Warnings,
    Events,
    #[default]
    GeneratedCfg,
}

pub(super) const fn default_result_panel(result: &GuiImportResult) -> ResultPanel {
    match result {
        GuiImportResult::Success { .. } => ResultPanel::GeneratedCfg,
        GuiImportResult::Error { .. } => ResultPanel::Errors,
    }
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

#[derive(Debug, Default)]
pub(super) struct GeneratedCfgPreviewCache {
    line_ranges: Vec<Range<usize>>,
    line_numbers: Vec<String>,
    source_ptr: usize,
    source_len: usize,
    number_width: usize,
    max_line_chars: usize,
}

impl GeneratedCfgPreviewCache {
    pub(super) fn clear(&mut self) {
        self.line_ranges.clear();
        self.line_numbers.clear();
        self.source_ptr = 0;
        self.source_len = 0;
        self.number_width = 0;
        self.max_line_chars = 0;
    }

    fn update(&mut self, cfg_text: &str) {
        let source_ptr = cfg_text.as_ptr() as usize;
        if self.source_ptr == source_ptr && self.source_len == cfg_text.len() {
            return;
        }

        self.source_ptr = source_ptr;
        self.source_len = cfg_text.len();
        self.line_ranges = cfg_line_ranges(cfg_text);
        self.number_width = number_width(self.line_ranges.len());
        self.line_numbers = padded_line_numbers(self.line_ranges.len(), self.number_width);
        self.max_line_chars = self
            .line_ranges
            .iter()
            .map(|range| cfg_text[range.clone()].chars().count())
            .max()
            .unwrap_or(0);
    }

    fn line_count(&self) -> usize {
        self.line_ranges.len()
    }
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
    cache: &mut GeneratedCfgPreviewCache,
    controller_scroll_delta: egui::Vec2,
) {
    let GuiImportResult::Success { cfg_text, .. } = result else {
        cache.clear();
        ui.label(localizer.text(UiText::NoGeneratedCfg));
        return;
    };
    cache.update(cfg_text);
    ui.scope(|ui| {
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                if controller_scroll_delta != egui::Vec2::ZERO {
                    ui.scroll_with_delta(controller_scroll_delta);
                }
                show_numbered_cfg(ui, cfg_text, cache, viewport);
            });
    });
}

fn show_numbered_cfg(
    ui: &mut egui::Ui,
    cfg_text: &str,
    cache: &GeneratedCfgPreviewCache,
    viewport: egui::Rect,
) {
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let char_width = row_height;
    let gutter_width = usize_to_f32(cache.number_width) * char_width;
    let body_width = usize_to_f32(cache.max_line_chars) * char_width;
    let content_size = egui::vec2(
        gutter_width + GENERATED_CFG_COLUMN_GAP + body_width,
        virtual_content_height(cache.line_count(), row_height),
    );
    let (_content_id, content_rect) = ui.allocate_space(content_size);

    let content_top = content_rect.top();
    let visible_top = (viewport.top() - content_top).max(0.0);
    let visible_bottom = (viewport.bottom() - content_top).max(visible_top);
    let visible_rows = visible_row_range(
        visible_top..visible_bottom,
        row_height,
        cache.line_count(),
        GENERATED_CFG_ROW_OVERSCAN,
    );
    let content_left = content_rect.left();
    let body_left = content_left + gutter_width + GENERATED_CFG_COLUMN_GAP;
    let text_color = ui.visuals().text_color();

    for row in visible_rows {
        let y = content_top + usize_to_f32(row) * row_height;
        let line = &cfg_text[cache.line_ranges[row].clone()];
        let number_pos = egui::pos2(content_left + gutter_width, y);
        let body_pos = egui::pos2(body_left, y);
        ui.painter().text(
            number_pos,
            egui::Align2::RIGHT_TOP,
            cache.line_numbers[row].as_str(),
            egui::FontId::monospace(row_height),
            text_color,
        );
        ui.painter().text(
            body_pos,
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(row_height),
            text_color,
        );
    }
}

fn cfg_line_ranges(cfg_text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for line in cfg_text.split('\n') {
        let end = start + line.len();
        ranges.push(start..end);
        start = end + 1;
    }
    ranges
}

fn number_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len()
}

fn padded_line_numbers(line_count: usize, number_width: usize) -> Vec<String> {
    (1..=line_count)
        .map(|line_number| format!("{line_number:>number_width$}"))
        .collect()
}

fn visible_row_range(
    viewport: Range<f32>,
    row_height: f32,
    line_count: usize,
    overscan: usize,
) -> Range<usize> {
    if line_count == 0 || row_height <= 0.0 {
        return 0..0;
    }
    let first = row_index_floor(viewport.start, row_height, line_count);
    let last = row_index_ceil(viewport.end, row_height, line_count);
    first.saturating_sub(overscan)..last.saturating_add(overscan).min(line_count)
}

fn virtual_content_height(line_count: usize, row_height: f32) -> f32 {
    usize_to_f32(line_count) * row_height
}

fn row_index_floor(offset: f32, row_height: f32, line_count: usize) -> usize {
    if offset <= 0.0 {
        return 0;
    }
    let mut low = 0;
    let mut high = line_count;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if usize_to_f32(mid) * row_height <= offset {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

fn row_index_ceil(offset: f32, row_height: f32, line_count: usize) -> usize {
    if offset <= 0.0 {
        return 0;
    }
    let mut low = 0;
    let mut high = line_count;
    while low < high {
        let mid = low + (high - low) / 2;
        if usize_to_f32(mid) * row_height < offset {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn usize_to_f32(value: usize) -> f32 {
    const CHUNK: u16 = u16::MAX;
    let chunk = usize::from(CHUNK);
    let chunks = value / chunk;
    let remainder = value % chunk;
    let remainder = u16::try_from(remainder).expect("remainder must fit in u16");
    if chunks == 0 {
        return f32::from(remainder);
    }
    f32::from(CHUNK) * usize_to_f32(chunks) + f32::from(remainder)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_line_ranges_match_split_newline() {
        assert_eq!(cfg_line_ranges(""), vec![0..0]);
        assert_eq!(cfg_line_ranges("a"), vec![0..1]);
        assert_eq!(cfg_line_ranges("a\n"), vec![0..1, 2..2]);
        assert_eq!(cfg_line_ranges("a\n\n"), vec![0..1, 2..2, 3..3]);
        assert_eq!(cfg_line_ranges("a\nb"), vec![0..1, 2..3]);
    }

    #[test]
    fn padded_line_numbers_use_final_line_count_width() {
        assert_eq!(number_width(1), 1);
        assert_eq!(padded_line_numbers(1, number_width(1)), vec!["1"]);
        assert_eq!(number_width(9), 1);
        assert_eq!(padded_line_numbers(9, number_width(9))[8], "9");
        assert_eq!(number_width(10), 2);
        assert_eq!(
            &padded_line_numbers(10, number_width(10))[0..2],
            [" 1", " 2"]
        );
        assert_eq!(number_width(100), 3);
        assert_eq!(
            &padded_line_numbers(100, number_width(100))[0..3],
            ["  1", "  2", "  3"]
        );
    }

    #[test]
    fn visible_row_range_handles_fractional_scroll_and_bottom_edge() {
        assert_eq!(visible_row_range(0.0..20.0, 10.0, 10, 0), 0..2);
        assert_eq!(visible_row_range(5.5..25.5, 10.0, 10, 0), 0..3);
        assert_eq!(visible_row_range(95.0..100.0, 10.0, 10, 0), 9..10);
        assert_eq!(visible_row_range(25.0..45.0, 10.0, 10, 2), 0..7);
        assert_eq!(visible_row_range(85.0..105.0, 10.0, 10, 2), 6..10);
    }

    #[test]
    fn virtual_content_height_is_line_count_times_row_height() {
        assert!((virtual_content_height(0, 12.0) - 0.0).abs() < f32::EPSILON);
        assert!((virtual_content_height(3, 12.5) - 37.5).abs() < f32::EPSILON);
    }
}
