// SPDX-License-Identifier: GPL-3.0-only

use super::controller::ControllerAction;

pub(super) const CONTROLLER_PREVIEW_SCROLL_PIXELS: f32 = 72.0;
pub(super) const CONTROLLER_PREVIEW_PAGE_SCROLL_PIXELS: f32 = 480.0;

#[derive(Debug, Clone, Copy)]
pub(super) enum PreviewScroll {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PreviewPageScroll {
    Up,
    Down,
}

pub(super) fn generated_cfg_scroll_delta(direction: PreviewScroll) -> egui::Vec2 {
    match direction {
        PreviewScroll::Left => egui::vec2(CONTROLLER_PREVIEW_SCROLL_PIXELS, 0.0),
        PreviewScroll::Right => egui::vec2(-CONTROLLER_PREVIEW_SCROLL_PIXELS, 0.0),
        PreviewScroll::Up => egui::vec2(0.0, CONTROLLER_PREVIEW_SCROLL_PIXELS),
        PreviewScroll::Down => egui::vec2(0.0, -CONTROLLER_PREVIEW_SCROLL_PIXELS),
    }
}

pub(super) fn generated_cfg_page_scroll_delta(direction: PreviewPageScroll) -> egui::Vec2 {
    match direction {
        PreviewPageScroll::Up => egui::vec2(0.0, CONTROLLER_PREVIEW_PAGE_SCROLL_PIXELS),
        PreviewPageScroll::Down => egui::vec2(0.0, -CONTROLLER_PREVIEW_PAGE_SCROLL_PIXELS),
    }
}

pub(super) fn path_picker_scroll_delta(actions: &[ControllerAction]) -> egui::Vec2 {
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
                | ControllerAction::Secondary
                | ControllerAction::Space
                | ControllerAction::PagePreviewDown
                | ControllerAction::SelectCurrent
                | ControllerAction::ToggleHiddenDirectories => egui::Vec2::ZERO,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_picker_scroll_delta_uses_only_vertical_preview_scroll_actions() {
        let delta = path_picker_scroll_delta(&[
            ControllerAction::ScrollPreviewDown,
            ControllerAction::ScrollPreviewRight,
            ControllerAction::Down,
            ControllerAction::ScrollPreviewUp,
            ControllerAction::ScrollPreviewUp,
            ControllerAction::PagePreviewDown,
            ControllerAction::ToggleHiddenDirectories,
        ]);

        assert!(delta.x.abs() < f32::EPSILON);
        assert!((delta.y - CONTROLLER_PREVIEW_SCROLL_PIXELS).abs() < f32::EPSILON);
    }
}
