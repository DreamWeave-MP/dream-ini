// SPDX-License-Identifier: GPL-3.0-only

use super::controller::ControllerAction;
use super::file_picker::PathTarget;
use super::localization::{Localizer, UiText};

const OSK_MIN_WIDTH: f32 = 300.0;
const OSK_MIN_HEIGHT: f32 = 220.0;
const OSK_MARGIN: f32 = 16.0;
const OSK_KEY_WIDTH: f32 = 30.0;
const OSK_KEY_HEIGHT: f32 = 22.0;
const OSK_KEY_SPACING: f32 = 2.0;
#[cfg(test)]
const OSK_CONTENT_HORIZONTAL_PADDING: f32 = 24.0;

#[derive(Debug, Clone)]
pub(super) struct OskState {
    target: PathTarget,
    buffer: String,
    selected_row: usize,
    selected_col: usize,
    caps_lock: bool,
    shift_next: bool,
}

impl OskState {
    pub(super) fn new(target: PathTarget, value: String) -> Self {
        Self {
            target,
            buffer: value,
            selected_row: 0,
            selected_col: 0,
            caps_lock: false,
            shift_next: false,
        }
    }

    pub(super) fn handle_controller_action(&mut self, action: ControllerAction) -> OskOutcome {
        match action {
            ControllerAction::Up => self.move_selection(OskDirection::Up),
            ControllerAction::Down => self.move_selection(OskDirection::Down),
            ControllerAction::Left => self.move_selection(OskDirection::Left),
            ControllerAction::Right => self.move_selection(OskDirection::Right),
            ControllerAction::Accept => return self.press_selected_key(),
            ControllerAction::Cancel => return OskOutcome::Cancel,
            ControllerAction::ClearCurrent => {
                self.buffer.pop();
            }
            ControllerAction::Secondary => self.toggle_shift(),
            ControllerAction::Space => self.push_char(' '),
            ControllerAction::SelectCurrent => return self.commit_outcome(),
            ControllerAction::ToggleHiddenDirectories
            | ControllerAction::PagePreviewDown
            | ControllerAction::ScrollPreviewLeft
            | ControllerAction::ScrollPreviewRight
            | ControllerAction::ScrollPreviewUp
            | ControllerAction::ScrollPreviewDown => {}
        }
        OskOutcome::None
    }

    pub(super) fn commit_outcome(&self) -> OskOutcome {
        OskOutcome::Commit {
            target: self.target,
            value: self.buffer.clone(),
        }
    }

    #[cfg(test)]
    pub(super) fn set_buffer_for_test(&mut self, value: String) {
        self.buffer = value;
    }

    #[cfg(test)]
    pub(super) fn buffer_for_test(&self) -> &str {
        &self.buffer
    }

    #[cfg(test)]
    fn select_key_for_test(&mut self, key: OskKey) {
        for (row_index, row) in OSK_LAYOUT.iter().enumerate() {
            if let Some(col_index) = row.iter().position(|candidate| *candidate == key) {
                self.selected_row = row_index;
                self.selected_col = col_index;
                return;
            }
        }
        panic!("OSK key {key:?} not present");
    }

    #[cfg(test)]
    pub(super) fn select_ok_for_test(&mut self) {
        self.select_key_for_test(OskKey::Ok);
    }

    #[cfg(test)]
    pub(super) fn select_clear_for_test(&mut self) {
        self.select_key_for_test(OskKey::Clear);
    }

    fn move_selection(&mut self, direction: OskDirection) {
        match direction {
            OskDirection::Up => {
                self.selected_row = if self.selected_row == 0 {
                    OSK_LAYOUT.len() - 1
                } else {
                    self.selected_row - 1
                };
                self.selected_col = self
                    .selected_col
                    .min(OSK_LAYOUT[self.selected_row].len() - 1);
            }
            OskDirection::Down => {
                self.selected_row = (self.selected_row + 1) % OSK_LAYOUT.len();
                self.selected_col = self
                    .selected_col
                    .min(OSK_LAYOUT[self.selected_row].len() - 1);
            }
            OskDirection::Left => {
                self.selected_col = if self.selected_col == 0 {
                    OSK_LAYOUT[self.selected_row].len() - 1
                } else {
                    self.selected_col - 1
                };
            }
            OskDirection::Right => {
                self.selected_col = (self.selected_col + 1) % OSK_LAYOUT[self.selected_row].len();
            }
        }
    }

    fn press_selected_key(&mut self) -> OskOutcome {
        match OSK_LAYOUT[self.selected_row][self.selected_col] {
            OskKey::Char(value) => self.push_char(value),
            OskKey::Shift => self.toggle_shift(),
            OskKey::Caps => self.caps_lock = !self.caps_lock,
            OskKey::Backspace => {
                self.buffer.pop();
            }
            OskKey::Clear => self.buffer.clear(),
            OskKey::Cancel => return OskOutcome::Cancel,
            OskKey::Ok => return self.commit_outcome(),
        }
        OskOutcome::None
    }

    fn toggle_shift(&mut self) {
        self.shift_next = !self.shift_next;
    }

    fn push_char(&mut self, value: char) {
        if value.is_ascii_alphabetic() && (self.caps_lock ^ self.shift_next) {
            self.buffer.push(value.to_ascii_uppercase());
        } else {
            self.buffer.push(value);
        }
        self.shift_next = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OskDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OskKey {
    Char(char),
    Shift,
    Caps,
    Backspace,
    Clear,
    Cancel,
    Ok,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OskOutcome {
    None,
    Cancel,
    Commit { target: PathTarget, value: String },
}

const OSK_ROW_0: &[OskKey] = &[
    OskKey::Char('q'),
    OskKey::Char('w'),
    OskKey::Char('e'),
    OskKey::Char('r'),
    OskKey::Char('t'),
    OskKey::Char('y'),
    OskKey::Char('u'),
    OskKey::Char('i'),
    OskKey::Char('o'),
    OskKey::Char('p'),
];
const OSK_ROW_1: &[OskKey] = &[
    OskKey::Char('a'),
    OskKey::Char('s'),
    OskKey::Char('d'),
    OskKey::Char('f'),
    OskKey::Char('g'),
    OskKey::Char('h'),
    OskKey::Char('j'),
    OskKey::Char('k'),
    OskKey::Char('l'),
];
const OSK_ROW_2: &[OskKey] = &[
    OskKey::Char('z'),
    OskKey::Char('x'),
    OskKey::Char('c'),
    OskKey::Char('v'),
    OskKey::Char('b'),
    OskKey::Char('n'),
    OskKey::Char('m'),
];
const OSK_ROW_3: &[OskKey] = &[
    OskKey::Char('0'),
    OskKey::Char('1'),
    OskKey::Char('2'),
    OskKey::Char('3'),
    OskKey::Char('4'),
    OskKey::Char('5'),
    OskKey::Char('6'),
    OskKey::Char('7'),
    OskKey::Char('8'),
    OskKey::Char('9'),
];
const OSK_ROW_4: &[OskKey] = &[
    OskKey::Char('/'),
    OskKey::Char('\\'),
    OskKey::Char('.'),
    OskKey::Char(':'),
    OskKey::Char('~'),
    OskKey::Char('_'),
    OskKey::Char('-'),
    OskKey::Char(' '),
];
const OSK_ROW_5: &[OskKey] = &[
    OskKey::Caps,
    OskKey::Shift,
    OskKey::Backspace,
    OskKey::Clear,
    OskKey::Cancel,
    OskKey::Ok,
];
const OSK_LAYOUT: &[&[OskKey]] = &[
    OSK_ROW_0, OSK_ROW_1, OSK_ROW_2, OSK_ROW_3, OSK_ROW_4, OSK_ROW_5,
];

pub(super) fn show_osk_overlay(
    ui: &mut egui::Ui,
    localizer: Localizer,
    osk: &mut OskState,
) -> OskOutcome {
    let screen_rect = ui.ctx().input(egui::InputState::content_rect);
    let size = osk_overlay_size(screen_rect.size());
    let max_height = (screen_rect.height() - OSK_MARGIN * 2.0).max(OSK_KEY_HEIGHT);
    let mut outcome = OskOutcome::None;
    let modal_id = egui::Id::new("path-osk-modal");
    let modal_area = egui::Modal::default_area(modal_id)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -OSK_MARGIN))
        .fade_in(false);

    egui::Modal::new(modal_id)
        .area(modal_area)
        .backdrop_color(egui::Color32::TRANSPARENT)
        .show(ui.ctx(), |ui| {
            ui.set_min_width(size.x);
            ui.set_max_width(size.x);
            ui.set_max_height(max_height);
            ui.vertical_centered(|ui| {
                ui.heading(localizer.text(UiText::OskTitle));
            });
            ui.separator();
            ui.label(localizer.text(UiText::OskControllerHelp));
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                egui::TextEdit::singleline(&mut osk.buffer),
            );
            ui.separator();

            ui.spacing_mut().item_spacing.y = OSK_KEY_SPACING;
            for (row_index, row) in OSK_LAYOUT.iter().enumerate() {
                show_osk_key_row(ui, localizer, osk, row_index, row, &mut outcome);
            }
        });

    outcome
}

fn show_osk_key_row(
    ui: &mut egui::Ui,
    localizer: Localizer,
    osk: &mut OskState,
    row_index: usize,
    row: &[OskKey],
    outcome: &mut OskOutcome,
) {
    let row_width = osk_row_width(row);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = OSK_KEY_SPACING;
        ui.add_space(((ui.available_width() - row_width) * 0.5).max(0.0));
        for (col_index, key) in row.iter().enumerate() {
            let selected = osk.selected_row == row_index && osk.selected_col == col_index;
            let mut button = egui::Button::new(osk_key_label(osk, localizer, *key));
            if selected {
                button = button.fill(ui.visuals().selection.bg_fill);
            }
            if ui
                .add_sized([OSK_KEY_WIDTH, OSK_KEY_HEIGHT], button)
                .clicked()
            {
                osk.selected_row = row_index;
                osk.selected_col = col_index;
                *outcome = osk.press_selected_key();
            }
        }
    });
}

fn osk_overlay_size(screen_size: egui::Vec2) -> egui::Vec2 {
    let max_width = (screen_size.x - OSK_MARGIN * 2.0).max(1.0);
    let max_height = (screen_size.y - OSK_MARGIN * 2.0).max(1.0);
    egui::vec2(
        (screen_size.x * 0.6).clamp(OSK_MIN_WIDTH.min(max_width), max_width),
        (screen_size.y * 0.4).clamp(OSK_MIN_HEIGHT.min(max_height), max_height),
    )
}

#[cfg(test)]
fn osk_keyboard_height() -> f32 {
    OSK_LAYOUT.iter().fold(0.0, |height, _row| {
        if height == 0.0 {
            OSK_KEY_HEIGHT
        } else {
            height + OSK_KEY_SPACING + OSK_KEY_HEIGHT
        }
    })
}

#[cfg(test)]
fn osk_keyboard_section_height(overlay_size: egui::Vec2) -> f32 {
    overlay_size.y.max(OSK_KEY_HEIGHT)
}

fn osk_row_width(row: &[OskKey]) -> f32 {
    row.iter().fold(0.0, |width, _key| {
        if width == 0.0 {
            OSK_KEY_WIDTH
        } else {
            width + OSK_KEY_SPACING + OSK_KEY_WIDTH
        }
    })
}

#[cfg(test)]
fn osk_keyboard_content_width(overlay_size: egui::Vec2) -> f32 {
    (overlay_size.x - OSK_CONTENT_HORIZONTAL_PADDING).max(OSK_KEY_WIDTH)
}

fn osk_key_label(osk: &OskState, localizer: Localizer, key: OskKey) -> String {
    match key {
        OskKey::Char(' ') => "Spc".to_owned(),
        OskKey::Char(value) if value.is_ascii_alphabetic() && (osk.caps_lock ^ osk.shift_next) => {
            value.to_ascii_uppercase().to_string()
        }
        OskKey::Char(value) => value.to_string(),
        OskKey::Shift => {
            if osk.shift_next {
                "SHFT*".to_owned()
            } else {
                "SHFT".to_owned()
            }
        }
        OskKey::Caps => {
            if osk.caps_lock {
                "CAPS*".to_owned()
            } else {
                "CAPS".to_owned()
            }
        }
        OskKey::Backspace => "Bksp".to_owned(),
        OskKey::Clear => "Clr".to_owned(),
        OskKey::Cancel => "Esc".to_owned(),
        OskKey::Ok => localizer.text(UiText::OskOk).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osk_shift_capitalizes_one_letter() {
        let mut osk = OskState::new(PathTarget::MorrowindIni, String::new());
        osk.select_key_for_test(OskKey::Char('m'));

        assert_eq!(
            osk.handle_controller_action(ControllerAction::Secondary),
            OskOutcome::None
        );
        assert_eq!(
            osk.handle_controller_action(ControllerAction::Accept),
            OskOutcome::None
        );
        assert_eq!(
            osk.handle_controller_action(ControllerAction::Accept),
            OskOutcome::None
        );

        assert_eq!(osk.buffer, "Mm");
        assert!(!osk.shift_next);
    }

    #[test]
    fn osk_space_action_inserts_space() {
        let mut osk = OskState::new(PathTarget::MorrowindIni, "Data".to_owned());

        assert_eq!(
            osk.handle_controller_action(ControllerAction::Space),
            OskOutcome::None
        );

        assert_eq!(osk.buffer, "Data ");
    }

    #[test]
    fn osk_caps_lock_toggles_letter_case() {
        let mut osk = OskState::new(PathTarget::MorrowindIni, String::new());

        osk.select_key_for_test(OskKey::Caps);
        assert_eq!(
            osk.handle_controller_action(ControllerAction::Accept),
            OskOutcome::None
        );
        osk.select_key_for_test(OskKey::Char('o'));
        assert_eq!(
            osk.handle_controller_action(ControllerAction::Accept),
            OskOutcome::None
        );
        assert_eq!(
            osk.handle_controller_action(ControllerAction::Secondary),
            OskOutcome::None
        );
        assert_eq!(
            osk.handle_controller_action(ControllerAction::Accept),
            OskOutcome::None
        );

        assert_eq!(osk.buffer, "Oo");
        assert!(osk.caps_lock);
        assert!(!osk.shift_next);
    }

    #[test]
    fn osk_letter_rows_are_qwerty_ordered() {
        assert_eq!(osk_row_chars(OSK_ROW_0), "qwertyuiop");
        assert_eq!(osk_row_chars(OSK_ROW_1), "asdfghjkl");
        assert_eq!(osk_row_chars(OSK_ROW_2), "zxcvbnm");
    }

    #[test]
    fn osk_layout_fits_640_by_480_overlay_budget() {
        let overlay_size = osk_overlay_size(egui::vec2(640.0, 480.0));
        let content_width = osk_keyboard_content_width(overlay_size);

        assert!(overlay_size.x <= 640.0 - OSK_MARGIN * 2.0);
        assert!(overlay_size.y <= 480.0 - OSK_MARGIN * 2.0);
        for row in OSK_LAYOUT {
            assert!(osk_row_width(row) <= content_width);
        }
        assert!(osk_keyboard_height() <= osk_keyboard_section_height(overlay_size));
    }

    #[test]
    fn osk_layout_keeps_required_path_keys_available() {
        for key in [
            OskKey::Char('0'),
            OskKey::Char('9'),
            OskKey::Char('/'),
            OskKey::Char('\\'),
            OskKey::Char('.'),
            OskKey::Char(':'),
            OskKey::Char('~'),
            OskKey::Char('_'),
            OskKey::Char('-'),
            OskKey::Char(' '),
            OskKey::Caps,
            OskKey::Shift,
            OskKey::Backspace,
            OskKey::Clear,
            OskKey::Cancel,
            OskKey::Ok,
        ] {
            assert!(OSK_LAYOUT.iter().any(|row| row.contains(&key)), "{key:?}");
        }
    }

    fn osk_row_chars(row: &[OskKey]) -> String {
        row.iter()
            .filter_map(|key| match key {
                OskKey::Char(value) if value.is_ascii_alphabetic() => Some(*value),
                OskKey::Char(_)
                | OskKey::Shift
                | OskKey::Caps
                | OskKey::Backspace
                | OskKey::Clear
                | OskKey::Cancel
                | OskKey::Ok => None,
            })
            .collect()
    }
}
