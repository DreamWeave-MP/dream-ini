// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use super::math::f32_to_usize_floor_clamped;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) enum TriangleClassification {
    Degenerate,
    Solid,
    Textured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) enum TexturedQuadFastPathRejection {
    NotRectangleDiagonal,
    NotAxisAlignedRectangle,
    CornerAttributeMismatch,
    NonUniformColor,
    NonAffineUv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) enum SolidTriangleColorDecision {
    Solid([u8; 4]),
    NonUniformVertexColor,
    NonUniformTexel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::gui::portmaster) struct TriangleScanWorkEstimate {
    pub(in crate::gui::portmaster) candidate_px: usize,
    pub(in crate::gui::portmaster) narrowed_rows: usize,
    pub(in crate::gui::portmaster) full_scan_rows: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::gui::portmaster) struct TriangleTexelSample {
    pub(in crate::gui::portmaster) texels: Option<[(usize, usize); 3]>,
    pub(in crate::gui::portmaster) uniform_color: Option<[u8; 4]>,
}

impl TriangleTexelSample {
    pub(in crate::gui::portmaster) const fn is_uniform(self) -> bool {
        self.uniform_color.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) struct ClipBounds {
    pub(super) min_x: usize,
    pub(super) min_y: usize,
    pub(super) max_x: usize,
    pub(super) max_y: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) struct TriangleRasterBounds {
    pub(in crate::gui::portmaster) min_x: usize,
    pub(in crate::gui::portmaster) min_y: usize,
    pub(in crate::gui::portmaster) max_x: usize,
    pub(in crate::gui::portmaster) max_y: usize,
}

impl TriangleRasterBounds {
    pub(in crate::gui::portmaster) const fn pixel_area(self) -> usize {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }
}

impl ClipBounds {
    pub(in crate::gui::portmaster) fn new(
        rect: egui::Rect,
        width: usize,
        height: usize,
    ) -> io::Result<Self> {
        let min_x = clamp_rect_value(rect.min.x.floor(), width)?;
        let min_y = clamp_rect_value(rect.min.y.floor(), height)?;
        let max_x = clamp_rect_value(rect.max.x.ceil(), width)?;
        let max_y = clamp_rect_value(rect.max.y.ceil(), height)?;
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    pub(in crate::gui::portmaster) const fn is_empty(self) -> bool {
        self.min_x >= self.max_x || self.min_y >= self.max_y
    }
}

fn clamp_rect_value(value: f32, max: usize) -> io::Result<usize> {
    if !value.is_finite() {
        return Err(io::Error::other("non-finite clip rectangle value"));
    }
    Ok(f32_to_usize_floor_clamped(value, max))
}
