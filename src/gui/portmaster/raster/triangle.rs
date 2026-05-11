// SPDX-License-Identifier: GPL-3.0-only

use super::coverage::triangle_scanline_x_range;
use super::math::{edge, f32_to_usize_ceil_clamped, f32_to_usize_floor_clamped, usize_to_f32};
use super::solid::solid_triangle_color;
use super::types::{
    ClipBounds, TriangleClassification, TriangleRasterBounds, TriangleScanWorkEstimate,
};
use crate::gui::portmaster::texture::TextureImage;

pub(super) const TRIANGLE_SCANLINE_NARROWING_MIN_AREA: usize = 1024;

#[derive(Clone, Copy)]
pub(super) struct TriangleVertices<'a> {
    pub(super) v0: &'a egui::epaint::Vertex,
    pub(super) v1: &'a egui::epaint::Vertex,
    pub(super) v2: &'a egui::epaint::Vertex,
}

pub(in crate::gui::portmaster) fn triangle_raster_bounds(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    clip: ClipBounds,
) -> Option<TriangleRasterBounds> {
    let min_x = f32_to_usize_floor_clamped(v0.pos.x.min(v1.pos.x).min(v2.pos.x), clip.max_x)
        .max(clip.min_x);
    let max_x =
        f32_to_usize_ceil_clamped(v0.pos.x.max(v1.pos.x).max(v2.pos.x), clip.max_x).min(clip.max_x);
    let min_y = f32_to_usize_floor_clamped(v0.pos.y.min(v1.pos.y).min(v2.pos.y), clip.max_y)
        .max(clip.min_y);
    let max_y =
        f32_to_usize_ceil_clamped(v0.pos.y.max(v1.pos.y).max(v2.pos.y), clip.max_y).min(clip.max_y);
    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    Some(TriangleRasterBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    })
}

pub(in crate::gui::portmaster) fn estimate_triangle_scan_work(
    positions: [egui::Pos2; 3],
    bounds: TriangleRasterBounds,
) -> TriangleScanWorkEstimate {
    let mut estimate = TriangleScanWorkEstimate::default();
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;
    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let candidate_px = end_x - start_x;
        estimate.candidate_px += candidate_px;
        if candidate_px < bounds.max_x - bounds.min_x {
            estimate.narrowed_rows += 1;
        } else {
            estimate.full_scan_rows += 1;
        }
    }
    estimate
}

pub(super) fn triangle_positions(vertices: TriangleVertices<'_>) -> [egui::Pos2; 3] {
    [vertices.v0.pos, vertices.v1.pos, vertices.v2.pos]
}

pub(in crate::gui::portmaster) fn classify_triangle(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> TriangleClassification {
    if edge(v0.pos, v1.pos, v2.pos).abs() <= f32::EPSILON {
        return TriangleClassification::Degenerate;
    }
    if solid_triangle_color(v0, v1, v2, texture).is_some() {
        TriangleClassification::Solid
    } else {
        TriangleClassification::Textured
    }
}
