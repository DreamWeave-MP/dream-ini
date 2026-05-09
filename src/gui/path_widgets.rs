// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    all(feature = "portmaster-gui", not(feature = "gui")),
    allow(dead_code)
)]

pub(super) fn path_file_row(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
) -> bool {
    path_row_plain(ui, label_width, label, browse, value)
}

pub(super) fn path_folder_row(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    browse: &str,
    value: &mut String,
    tooltip: Option<&str>,
) -> bool {
    path_row(ui, label_width, label, browse, value, tooltip)
}

pub(super) fn path_save_file_row(
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

pub(super) fn path_label_width(ui: &egui::Ui, labels: &[&str]) -> f32 {
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

pub(super) fn controller_marker_width(ui: &egui::Ui) -> f32 {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap("▶ ".to_owned(), font_id, ui.visuals().text_color())
        .size()
        .x
}
