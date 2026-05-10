// SPDX-License-Identifier: GPL-3.0-only

use std::cmp::Ordering;
use std::io;
use std::time::{Duration, Instant};

use super::surface::SoftwareSurface;
use super::texture::TextureImage;

const UV_AFFINE_EPSILON: f32 = 1.0 / 1_048_576.0;
const TRIANGLE_SCANLINE_NARROWING_MIN_AREA: usize = 1024;
const TRIANGLE_SCANLINE_NARROWING_GUARD_PX: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TriangleClassification {
    Degenerate,
    Solid,
    Textured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TexturedQuadFastPathRejection {
    NotRectangleDiagonal,
    NotAxisAlignedRectangle,
    CornerAttributeMismatch,
    NonUniformColor,
    NonAffineUv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SolidTriangleColorDecision {
    Solid([u8; 4]),
    NonUniformVertexColor,
    NonUniformTexel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RasterStats {
    pub(super) solid_rect_calls: usize,
    pub(super) solid_rect_px: usize,
    pub(super) textured_rect_calls: usize,
    pub(super) textured_rect_px: usize,
    pub(super) textured_rect_constant_texel_calls: usize,
    pub(super) textured_rect_constant_texel_px: usize,
    pub(super) textured_rect_constant_texel_us: usize,
    pub(super) textured_rect_sampled_calls: usize,
    pub(super) textured_rect_sampled_px: usize,
    pub(super) textured_rect_sampled_us: usize,
    pub(super) textured_rect_white_texel_calls: usize,
    pub(super) textured_rect_white_texel_px: usize,
    pub(super) textured_rect_uniform_color_calls: usize,
    pub(super) textured_rect_uniform_color_px: usize,
    pub(super) solid_triangle_calls: usize,
    pub(super) solid_triangle_bbox_px: usize,
    pub(super) solid_triangle_covered_px: usize,
    pub(super) solid_triangle_span_rows: usize,
    pub(super) solid_triangle_candidate_px: usize,
    pub(super) solid_triangle_hint_rows: usize,
    pub(super) solid_triangle_hint_fallback_rows: usize,
    pub(super) solid_triangle_hint_build_us: usize,
    pub(super) solid_triangle_endpoint_search_us: usize,
    pub(super) solid_triangle_blend_span_us: usize,
    pub(super) solid_triangle_blend_span_calls: usize,
    pub(super) solid_triangle_span_px: usize,
    pub(super) solid_triangle_endpoint_probe_px: usize,
    pub(super) solid_triangle_hint_probe_px: usize,
    pub(super) solid_triangle_canary_probe_px: usize,
    pub(super) solid_triangle_fallback_probe_px: usize,
    pub(super) solid_triangle_direct_probe_px: usize,
    pub(super) solid_triangle_hint_candidate_px: usize,
    pub(super) solid_triangle_narrowed_rows: usize,
    pub(super) solid_triangle_full_scan_rows: usize,
    pub(super) solid_fan_calls: usize,
    pub(super) solid_fan_triangles: usize,
    pub(super) solid_fan_rows: usize,
    pub(super) solid_fan_px: usize,
    pub(super) solid_fan_edge_intersections: usize,
    pub(super) solid_fan_endpoint_probe_px: usize,
    pub(super) solid_fan_fallback_rows: usize,
    pub(super) textured_triangle_calls: usize,
    pub(super) textured_triangle_bbox_px: usize,
    pub(super) textured_triangle_covered_px: usize,
    pub(super) textured_triangle_candidate_px: usize,
    pub(super) textured_triangle_narrowed_rows: usize,
    pub(super) textured_triangle_full_scan_rows: usize,
    pub(super) constant_texel_textured_triangle_calls: usize,
    pub(super) constant_texel_textured_triangle_white_texel_calls: usize,
    pub(super) constant_texel_textured_triangle_non_white_texel_calls: usize,
    pub(super) constant_texel_textured_triangle_candidate_px: usize,
    pub(super) constant_texel_textured_triangle_covered_px: usize,
    pub(super) constant_texel_textured_triangle_white_texel_covered_px: usize,
    pub(super) constant_texel_textured_triangle_non_white_texel_covered_px: usize,
    pub(super) constant_texel_textured_triangle_opaque_px: usize,
    pub(super) constant_texel_textured_triangle_translucent_px: usize,
    pub(super) constant_texel_textured_triangle_transparent_px: usize,
    pub(super) constant_texel_textured_triangle_us: usize,
    pub(super) constant_texel_textured_triangle_white_texel_us: usize,
    pub(super) constant_texel_textured_triangle_non_white_texel_us: usize,
    pub(super) sampled_textured_triangle_calls: usize,
    pub(super) sampled_textured_triangle_candidate_px: usize,
    pub(super) sampled_textured_triangle_covered_px: usize,
    pub(super) sampled_textured_triangle_us: usize,
    pub(super) degenerate_triangle_skips: usize,
    pub(super) fully_clipped_triangle_skips: usize,
    pub(super) opaque_px: usize,
    pub(super) translucent_px: usize,
    pub(super) transparent_px: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TriangleScanWorkEstimate {
    pub(super) candidate_px: usize,
    pub(super) narrowed_rows: usize,
    pub(super) full_scan_rows: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TriangleTexelSample {
    pub(super) texels: Option<[(usize, usize); 3]>,
    pub(super) uniform_color: Option<[u8; 4]>,
}

impl TriangleTexelSample {
    pub(super) const fn is_uniform(self) -> bool {
        self.uniform_color.is_some()
    }
}

impl RasterStats {
    fn record_textured_rect_classification(
        &mut self,
        classification: TexturedRectClassification,
        px: usize,
    ) {
        match classification.kind {
            TexturedRectKind::ConstantTexel => {
                self.textured_rect_constant_texel_calls += 1;
                self.textured_rect_constant_texel_px += px;
            }
            TexturedRectKind::Sampled => {
                self.textured_rect_sampled_calls += 1;
                self.textured_rect_sampled_px += px;
            }
        }
        if classification.white_texel {
            self.textured_rect_white_texel_calls += 1;
            self.textured_rect_white_texel_px += px;
        }
        if classification.uniform_corner_color {
            self.textured_rect_uniform_color_calls += 1;
            self.textured_rect_uniform_color_px += px;
        }
    }

    fn record_textured_rect_elapsed(&mut self, kind: TexturedRectKind, elapsed: Duration) {
        match kind {
            TexturedRectKind::ConstantTexel => {
                self.textured_rect_constant_texel_us += duration_as_us(elapsed);
            }
            TexturedRectKind::Sampled => {
                self.textured_rect_sampled_us += duration_as_us(elapsed);
            }
        }
    }

    fn record_alpha_px(&mut self, alpha: u8, count: usize) {
        match alpha {
            0 => self.transparent_px += count,
            u8::MAX => self.opaque_px += count,
            _ => self.translucent_px += count,
        }
    }

    fn record_constant_texel_alpha_px(&mut self, alpha: u8, count: usize) {
        match alpha {
            0 => self.constant_texel_textured_triangle_transparent_px += count,
            u8::MAX => self.constant_texel_textured_triangle_opaque_px += count,
            _ => self.constant_texel_textured_triangle_translucent_px += count,
        }
    }

    pub(super) fn log_line(&self) -> String {
        format!(
            "software renderer raster_stats solid_rect_calls={} solid_rect_px={} textured_rect_calls={} textured_rect_px={} textured_rect_constant_texel_calls={} textured_rect_constant_texel_px={} textured_rect_constant_texel_us={} textured_rect_sampled_calls={} textured_rect_sampled_px={} textured_rect_sampled_us={} textured_rect_white_texel_calls={} textured_rect_white_texel_px={} textured_rect_uniform_color_calls={} textured_rect_uniform_color_px={} solid_triangle_calls={} solid_triangle_bbox_px={} solid_triangle_covered_px={} solid_triangle_span_rows={} solid_triangle_candidate_px={} solid_triangle_hint_rows={} solid_triangle_hint_fallback_rows={} solid_triangle_hint_build_us={} solid_triangle_endpoint_search_us={} solid_triangle_blend_span_us={} solid_triangle_blend_span_calls={} solid_triangle_span_px={} solid_triangle_endpoint_probe_px={} solid_triangle_hint_probe_px={} solid_triangle_canary_probe_px={} solid_triangle_fallback_probe_px={} solid_triangle_direct_probe_px={} solid_triangle_hint_candidate_px={} solid_triangle_narrowed_rows={} solid_triangle_full_scan_rows={} solid_fan_calls={} solid_fan_triangles={} solid_fan_rows={} solid_fan_px={} solid_fan_edge_intersections={} solid_fan_endpoint_probe_px={} solid_fan_fallback_rows={} textured_triangle_calls={} textured_triangle_bbox_px={} textured_triangle_covered_px={} textured_triangle_candidate_px={} textured_triangle_narrowed_rows={} textured_triangle_full_scan_rows={} constant_texel_textured_triangle_calls={} constant_texel_textured_triangle_white_texel_calls={} constant_texel_textured_triangle_non_white_texel_calls={} constant_texel_textured_triangle_candidate_px={} constant_texel_textured_triangle_covered_px={} constant_texel_textured_triangle_white_texel_covered_px={} constant_texel_textured_triangle_non_white_texel_covered_px={} constant_texel_textured_triangle_opaque_px={} constant_texel_textured_triangle_translucent_px={} constant_texel_textured_triangle_transparent_px={} constant_texel_textured_triangle_us={} constant_texel_textured_triangle_white_texel_us={} constant_texel_textured_triangle_non_white_texel_us={} sampled_textured_triangle_calls={} sampled_textured_triangle_candidate_px={} sampled_textured_triangle_covered_px={} sampled_textured_triangle_us={} degenerate_triangle_skips={} fully_clipped_triangle_skips={} opaque_px={} translucent_px={} transparent_px={}",
            self.solid_rect_calls,
            self.solid_rect_px,
            self.textured_rect_calls,
            self.textured_rect_px,
            self.textured_rect_constant_texel_calls,
            self.textured_rect_constant_texel_px,
            self.textured_rect_constant_texel_us,
            self.textured_rect_sampled_calls,
            self.textured_rect_sampled_px,
            self.textured_rect_sampled_us,
            self.textured_rect_white_texel_calls,
            self.textured_rect_white_texel_px,
            self.textured_rect_uniform_color_calls,
            self.textured_rect_uniform_color_px,
            self.solid_triangle_calls,
            self.solid_triangle_bbox_px,
            self.solid_triangle_covered_px,
            self.solid_triangle_span_rows,
            self.solid_triangle_candidate_px,
            self.solid_triangle_hint_rows,
            self.solid_triangle_hint_fallback_rows,
            self.solid_triangle_hint_build_us,
            self.solid_triangle_endpoint_search_us,
            self.solid_triangle_blend_span_us,
            self.solid_triangle_blend_span_calls,
            self.solid_triangle_span_px,
            self.solid_triangle_endpoint_probe_px,
            self.solid_triangle_hint_probe_px,
            self.solid_triangle_canary_probe_px,
            self.solid_triangle_fallback_probe_px,
            self.solid_triangle_direct_probe_px,
            self.solid_triangle_hint_candidate_px,
            self.solid_triangle_narrowed_rows,
            self.solid_triangle_full_scan_rows,
            self.solid_fan_calls,
            self.solid_fan_triangles,
            self.solid_fan_rows,
            self.solid_fan_px,
            self.solid_fan_edge_intersections,
            self.solid_fan_endpoint_probe_px,
            self.solid_fan_fallback_rows,
            self.textured_triangle_calls,
            self.textured_triangle_bbox_px,
            self.textured_triangle_covered_px,
            self.textured_triangle_candidate_px,
            self.textured_triangle_narrowed_rows,
            self.textured_triangle_full_scan_rows,
            self.constant_texel_textured_triangle_calls,
            self.constant_texel_textured_triangle_white_texel_calls,
            self.constant_texel_textured_triangle_non_white_texel_calls,
            self.constant_texel_textured_triangle_candidate_px,
            self.constant_texel_textured_triangle_covered_px,
            self.constant_texel_textured_triangle_white_texel_covered_px,
            self.constant_texel_textured_triangle_non_white_texel_covered_px,
            self.constant_texel_textured_triangle_opaque_px,
            self.constant_texel_textured_triangle_translucent_px,
            self.constant_texel_textured_triangle_transparent_px,
            self.constant_texel_textured_triangle_us,
            self.constant_texel_textured_triangle_white_texel_us,
            self.constant_texel_textured_triangle_non_white_texel_us,
            self.sampled_textured_triangle_calls,
            self.sampled_textured_triangle_candidate_px,
            self.sampled_textured_triangle_covered_px,
            self.sampled_textured_triangle_us,
            self.degenerate_triangle_skips,
            self.fully_clipped_triangle_skips,
            self.opaque_px,
            self.translucent_px,
            self.transparent_px,
        )
    }
}

fn duration_as_us(duration: Duration) -> usize {
    usize::try_from(duration.as_micros()).unwrap_or(usize::MAX)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ClipBounds {
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TriangleRasterBounds {
    pub(super) min_x: usize,
    pub(super) min_y: usize,
    pub(super) max_x: usize,
    pub(super) max_y: usize,
}

impl TriangleRasterBounds {
    pub(super) const fn pixel_area(self) -> usize {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }
}

impl ClipBounds {
    pub(super) fn new(rect: egui::Rect, width: usize, height: usize) -> io::Result<Self> {
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

    pub(super) const fn is_empty(self) -> bool {
        self.min_x >= self.max_x || self.min_y >= self.max_y
    }
}

fn clamp_rect_value(value: f32, max: usize) -> io::Result<usize> {
    if !value.is_finite() {
        return Err(io::Error::other("non-finite clip rectangle value"));
    }
    Ok(f32_to_usize_floor_clamped(value, max))
}

pub(super) fn usize_to_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn f32_to_usize_floor_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.floor(), max)
}

fn f32_to_usize_ceil_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.ceil(), max)
}

fn f32_to_usize_round_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.round(), max)
}

fn f32_to_usize_threshold_clamped(value: f32, max: usize) -> usize {
    if value <= 0.0 {
        return 0;
    }
    let max_value = usize_to_f32(max);
    if value >= max_value {
        return max;
    }
    f32_to_usize_bounded(value.clamp(0.0, max_value))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped to a non-negative finite usize range before casting"
)]
fn f32_to_usize_bounded(value: f32) -> usize {
    value as usize
}

fn f32_to_u8_round_clamped(value: f32) -> u8 {
    let value = value.round().clamp(0.0, 255.0);
    f32_to_u8_bounded(value)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is rounded and clamped to the u8 range before casting"
)]
fn f32_to_u8_bounded(value: f32) -> u8 {
    value as u8
}

fn edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)))
}

fn edge_step_x(a: egui::Pos2, b: egui::Pos2) -> f32 {
    b.y - a.y
}

fn edge_step_y(a: egui::Pos2, b: egui::Pos2) -> f32 {
    -(b.x - a.x)
}

fn edge_is_top_left(a: egui::Pos2, b: egui::Pos2) -> bool {
    a.y < b.y || (same_f32(a.y, b.y) && a.x > b.x)
}

fn edge_includes_boundary(a: egui::Pos2, b: egui::Pos2, area: f32) -> bool {
    if area < 0.0 {
        edge_is_top_left(a, b)
    } else {
        !edge_is_top_left(a, b)
    }
}

fn edge_covers_pixel(weight: f32, includes_boundary: bool) -> bool {
    weight > 0.0 || (same_f32(weight, 0.0) && includes_boundary)
}

fn same_f32(left: f32, right: f32) -> bool {
    matches!(left.partial_cmp(&right), Some(Ordering::Equal))
}

fn same_pos2(left: egui::Pos2, right: egui::Pos2) -> bool {
    same_f32(left.x, right.x) && same_f32(left.y, right.y)
}

fn near_finite_pos2(left: egui::Pos2, right: egui::Pos2, epsilon: f32) -> bool {
    left.x.is_finite()
        && left.y.is_finite()
        && right.x.is_finite()
        && right.y.is_finite()
        && (left.x - right.x).abs() <= epsilon
        && (left.y - right.y).abs() <= epsilon
}

pub(super) fn rasterize_triangle(
    surface: &mut SoftwareSurface,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
    clip: ClipBounds,
    mut stats: Option<&mut RasterStats>,
) {
    let area = edge(v0.pos, v1.pos, v2.pos);
    if area.abs() <= f32::EPSILON {
        if let Some(stats) = stats {
            stats.degenerate_triangle_skips += 1;
        }
        return;
    }
    let Some(bounds) = triangle_raster_bounds(v0, v1, v2, clip) else {
        if let Some(stats) = stats {
            stats.fully_clipped_triangle_skips += 1;
        }
        return;
    };

    let vertices = TriangleVertices { v0, v1, v2 };
    if let Some(color) = solid_triangle_color(v0, v1, v2, texture) {
        if let Some(stats) = &mut stats {
            stats.solid_triangle_calls += 1;
            stats.solid_triangle_bbox_px += bounds.pixel_area();
        }
        rasterize_solid_triangle(surface, vertices, bounds, area, color, stats);
        return;
    }

    if let Some(texture_color) = triangle_nearest_texel_sample(v0, v1, v2, texture).uniform_color {
        record_textured_triangle_call(
            &mut stats,
            bounds,
            TexturedTriangleKind::ConstantTexel {
                white_texel: is_white_texel(texture_color),
            },
        );
        rasterize_constant_texel_textured_triangle(
            surface,
            vertices,
            bounds,
            area,
            texture_color,
            stats,
        );
        return;
    }

    record_textured_triangle_call(&mut stats, bounds, TexturedTriangleKind::Sampled);
    rasterize_textured_triangle(surface, vertices, texture, bounds, area, stats);
}

#[derive(Clone, Copy)]
enum TexturedTriangleKind {
    ConstantTexel { white_texel: bool },
    Sampled,
}

fn record_textured_triangle_call(
    stats: &mut Option<&mut RasterStats>,
    bounds: TriangleRasterBounds,
    kind: TexturedTriangleKind,
) {
    if let Some(stats) = stats.as_deref_mut() {
        stats.textured_triangle_calls += 1;
        stats.textured_triangle_bbox_px += bounds.pixel_area();
        match kind {
            TexturedTriangleKind::ConstantTexel { white_texel } => {
                stats.constant_texel_textured_triangle_calls += 1;
                if white_texel {
                    stats.constant_texel_textured_triangle_white_texel_calls += 1;
                } else {
                    stats.constant_texel_textured_triangle_non_white_texel_calls += 1;
                }
            }
            TexturedTriangleKind::Sampled => {
                stats.sampled_textured_triangle_calls += 1;
            }
        }
    }
}

const fn is_white_texel(texture_color: [u8; 4]) -> bool {
    matches!(texture_color, [255, 255, 255, 255])
}

pub(super) fn triangle_raster_bounds(
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

pub(super) fn estimate_triangle_scan_work(
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

fn triangle_positions(vertices: TriangleVertices<'_>) -> [egui::Pos2; 3] {
    [vertices.v0.pos, vertices.v1.pos, vertices.v2.pos]
}

pub(super) fn classify_triangle(
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

pub(super) fn rasterize_axis_aligned_solid_quad(
    surface: &mut SoftwareSurface,
    vertices: [&egui::epaint::Vertex; 6],
    texture: &TextureImage,
    clip: ClipBounds,
    stats: Option<&mut RasterStats>,
) -> bool {
    let Some(color) = solid_triangle_color(vertices[0], vertices[1], vertices[2], texture) else {
        return false;
    };
    if solid_triangle_color(vertices[3], vertices[4], vertices[5], texture) != Some(color) {
        return false;
    }
    if !triangles_share_rectangle_diagonal(vertices) {
        return false;
    }

    let Some(bounds) = axis_aligned_quad_bounds(vertices) else {
        return false;
    };
    rasterize_solid_rect(surface, bounds, clip, color, stats);
    true
}

pub(super) fn rasterize_axis_aligned_textured_quad(
    surface: &mut SoftwareSurface,
    vertices: [&egui::epaint::Vertex; 6],
    texture: &TextureImage,
    clip: ClipBounds,
    stats: Option<&mut RasterStats>,
) -> bool {
    let Ok(candidate) = textured_quad_fast_path_candidate(vertices) else {
        return false;
    };

    rasterize_textured_rect(
        surface,
        candidate.corners,
        candidate.bounds,
        texture,
        clip,
        stats,
    );
    true
}

pub(super) fn rasterize_solid_fan(
    surface: &mut SoftwareSurface,
    polygon: &[&egui::epaint::Vertex],
    triangle_count: usize,
    color: [u8; 4],
    clip: ClipBounds,
    mut stats: Option<&mut RasterStats>,
) {
    let Some(bounds) = polygon_raster_bounds(polygon, clip) else {
        return;
    };
    let Some(area_sign) = polygon_area_sign(polygon) else {
        return;
    };
    if let Some(stats) = &mut stats {
        stats.solid_fan_calls += 1;
        stats.solid_fan_triangles += triangle_count;
    }

    for y in bounds.min_y..bounds.max_y {
        let scanline = polygon_scanline_span(polygon, bounds, y, area_sign, stats.is_some());
        let span = if scanline.fell_back {
            if let Some(stats) = &mut stats {
                stats.solid_fan_fallback_rows += 1;
            }
            polygon_fallback_scanline_span(polygon, bounds, y, area_sign)
        } else {
            scanline.span
        };
        let Some((span_start, span_end)) = span else {
            record_solid_fan_scanline_stats(&mut stats, scanline);
            continue;
        };
        surface.blend_span(y, span_start, span_end, color);
        if let Some(stats) = &mut stats {
            let px = span_end - span_start;
            stats.solid_fan_rows += 1;
            stats.solid_fan_px += px;
            stats.record_alpha_px(color[3], px);
        }
        record_solid_fan_scanline_stats(&mut stats, scanline);
    }
}

fn record_solid_fan_scanline_stats(
    stats: &mut Option<&mut RasterStats>,
    scanline: PolygonScanlineSpan,
) {
    if let Some(stats) = stats {
        stats.solid_fan_edge_intersections += scanline.edge_intersections;
        stats.solid_fan_endpoint_probe_px += scanline.endpoint_probe_px;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PolygonScanlineSpan {
    span: Option<(usize, usize)>,
    edge_intersections: usize,
    endpoint_probe_px: usize,
    fell_back: bool,
}

fn polygon_raster_bounds(
    polygon: &[&egui::epaint::Vertex],
    clip: ClipBounds,
) -> Option<TriangleRasterBounds> {
    if polygon.len() < 3 {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for vertex in polygon {
        min_x = min_x.min(vertex.pos.x);
        min_y = min_y.min(vertex.pos.y);
        max_x = max_x.max(vertex.pos.x);
        max_y = max_y.max(vertex.pos.y);
    }

    let min_x = f32_to_usize_floor_clamped(min_x, clip.max_x).max(clip.min_x);
    let max_x = f32_to_usize_ceil_clamped(max_x, clip.max_x).min(clip.max_x);
    let min_y = f32_to_usize_floor_clamped(min_y, clip.max_y).max(clip.min_y);
    let max_y = f32_to_usize_ceil_clamped(max_y, clip.max_y).min(clip.max_y);
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

fn polygon_area_sign(polygon: &[&egui::epaint::Vertex]) -> Option<f32> {
    let mut twice_area = 0.0;
    for edge_index in 0..polygon.len() {
        let a = polygon[edge_index].pos;
        let b = polygon[(edge_index + 1) % polygon.len()].pos;
        twice_area += a.x.mul_add(b.y, -(a.y * b.x));
    }
    if twice_area.abs() <= f32::EPSILON {
        return None;
    }
    Some(-twice_area.signum())
}

fn polygon_scanline_span(
    polygon: &[&egui::epaint::Vertex],
    bounds: TriangleRasterBounds,
    y: usize,
    area_sign: f32,
    collect_stats: bool,
) -> PolygonScanlineSpan {
    let pixel_center_y = usize_to_f32(y) + 0.5;
    let mut lower = None;
    let mut upper = None;
    let mut edge_intersections = 0;
    for edge_index in 0..polygon.len() {
        let a = polygon[edge_index].pos;
        let b = polygon[(edge_index + 1) % polygon.len()].pos;
        let slope_x = edge_step_x(a, b) * area_sign;
        let at_origin = edge(a, b, egui::pos2(0.0, pixel_center_y)) * area_sign;
        if !slope_x.is_finite() || !at_origin.is_finite() {
            return PolygonScanlineSpan {
                fell_back: true,
                ..PolygonScanlineSpan::default()
            };
        }
        if same_f32(slope_x, 0.0) {
            if !edge_covers_pixel(at_origin, edge_includes_boundary(a, b, area_sign)) {
                return PolygonScanlineSpan::default();
            }
            continue;
        }
        let boundary = -at_origin / slope_x;
        if !boundary.is_finite() {
            return PolygonScanlineSpan {
                fell_back: true,
                ..PolygonScanlineSpan::default()
            };
        }
        edge_intersections += 1;
        let includes_boundary = edge_includes_boundary(a, b, area_sign);
        if slope_x > 0.0 {
            lower = Some(tighter_lower_bound(lower, boundary, includes_boundary));
        } else {
            upper = Some(tighter_upper_bound(upper, boundary, includes_boundary));
        }
    }
    let start_x = polygon_lower_bound_x(lower, bounds);
    let end_x = polygon_upper_bound_x(upper, bounds).max(start_x);
    if start_x >= end_x {
        return PolygonScanlineSpan {
            edge_intersections,
            ..PolygonScanlineSpan::default()
        };
    }

    let mut span = (start_x, end_x);
    let endpoint_probe_px =
        polygon_correct_span_endpoints(&mut span, y, polygon, area_sign, bounds, collect_stats);
    PolygonScanlineSpan {
        span: (span.0 < span.1).then_some(span),
        edge_intersections,
        endpoint_probe_px,
        fell_back: false,
    }
}

fn polygon_fallback_scanline_span(
    polygon: &[&egui::epaint::Vertex],
    bounds: TriangleRasterBounds,
    y: usize,
    area_sign: f32,
) -> Option<(usize, usize)> {
    let mut span_start = None;
    let mut span_end = None;
    for x in bounds.min_x..bounds.max_x {
        if polygon_covers_pixel(x, y, polygon, area_sign) {
            span_start.get_or_insert(x);
            span_end = Some(x + 1);
        } else if span_start.is_some() {
            break;
        }
    }
    span_start.zip(span_end)
}

fn tighter_lower_bound(
    current: Option<(f32, bool)>,
    boundary: f32,
    includes_boundary: bool,
) -> (f32, bool) {
    match current {
        None => (boundary, includes_boundary),
        Some((value, _include)) if boundary > value => (boundary, includes_boundary),
        Some((value, include)) if same_f32(boundary, value) => {
            (value, include && includes_boundary)
        }
        Some(current) => current,
    }
}

fn tighter_upper_bound(
    current: Option<(f32, bool)>,
    boundary: f32,
    includes_boundary: bool,
) -> (f32, bool) {
    match current {
        None => (boundary, includes_boundary),
        Some((value, _include)) if boundary < value => (boundary, includes_boundary),
        Some((value, include)) if same_f32(boundary, value) => {
            (value, include && includes_boundary)
        }
        Some(current) => current,
    }
}

fn polygon_lower_bound_x(lower: Option<(f32, bool)>, bounds: TriangleRasterBounds) -> usize {
    let Some((center_x, includes_boundary)) = lower else {
        return bounds.min_x;
    };
    let threshold = center_x - 0.5;
    let x = if includes_boundary {
        f32_to_usize_ceil_clamped(threshold, bounds.max_x)
    } else {
        f32_to_usize_floor_clamped(threshold, bounds.max_x).saturating_add(1)
    };
    x.max(bounds.min_x).min(bounds.max_x)
}

fn polygon_upper_bound_x(upper: Option<(f32, bool)>, bounds: TriangleRasterBounds) -> usize {
    let Some((center_x, includes_boundary)) = upper else {
        return bounds.max_x;
    };
    let threshold = center_x - 0.5;
    let x = if includes_boundary {
        f32_to_usize_floor_clamped(threshold, bounds.max_x).saturating_add(1)
    } else {
        f32_to_usize_ceil_clamped(threshold, bounds.max_x)
    };
    x.max(bounds.min_x).min(bounds.max_x)
}

fn polygon_correct_span_endpoints(
    span: &mut (usize, usize),
    y: usize,
    polygon: &[&egui::epaint::Vertex],
    area_sign: f32,
    bounds: TriangleRasterBounds,
    collect_stats: bool,
) -> usize {
    const MAX_ENDPOINT_CORRECTION_PX: usize = 2;
    let mut probe_px = 0;
    let mut correction_px = 0;
    while span.0 < span.1
        && !polygon_endpoint_probe(span.0, y, polygon, area_sign, collect_stats, &mut probe_px)
        && correction_px < MAX_ENDPOINT_CORRECTION_PX
    {
        span.0 += 1;
        correction_px += 1;
    }
    if span.0 < span.1
        && !polygon_endpoint_probe(span.0, y, polygon, area_sign, collect_stats, &mut probe_px)
    {
        span.1 = span.0;
        return probe_px;
    }
    correction_px = 0;
    while span.0 < span.1
        && !polygon_endpoint_probe(
            span.1 - 1,
            y,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        )
        && correction_px < MAX_ENDPOINT_CORRECTION_PX
    {
        span.1 -= 1;
        correction_px += 1;
    }
    if span.0 < span.1
        && !polygon_endpoint_probe(
            span.1 - 1,
            y,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        )
    {
        span.1 = span.0;
        return probe_px;
    }
    if span.0 > bounds.min_x {
        let _ = polygon_endpoint_probe(
            span.0 - 1,
            y,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        );
    }
    if span.1 < bounds.max_x {
        let _ = polygon_endpoint_probe(span.1, y, polygon, area_sign, collect_stats, &mut probe_px);
    }
    probe_px
}

fn polygon_endpoint_probe(
    x: usize,
    y: usize,
    polygon: &[&egui::epaint::Vertex],
    area_sign: f32,
    collect_stats: bool,
    probe_px: &mut usize,
) -> bool {
    if collect_stats {
        *probe_px += 1;
    }
    polygon_covers_pixel(x, y, polygon, area_sign)
}

fn polygon_covers_pixel(
    x: usize,
    y: usize,
    polygon: &[&egui::epaint::Vertex],
    area_sign: f32,
) -> bool {
    let pixel_center = egui::pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
    for edge_index in 0..polygon.len() {
        let a = polygon[edge_index].pos;
        let b = polygon[(edge_index + 1) % polygon.len()].pos;
        let weight = edge(a, b, pixel_center) * area_sign;
        if !edge_covers_pixel(weight, edge_includes_boundary(a, b, area_sign)) {
            return false;
        }
    }
    true
}

pub(super) fn textured_quad_fast_path_rejection(
    vertices: [&egui::epaint::Vertex; 6],
) -> Option<TexturedQuadFastPathRejection> {
    textured_quad_fast_path_candidate(vertices).err()
}

pub(super) fn is_axis_aligned_quad(vertices: [&egui::epaint::Vertex; 6]) -> bool {
    triangles_share_rectangle_diagonal(vertices) && axis_aligned_quad_bounds(vertices).is_some()
}

fn triangles_share_rectangle_diagonal(vertices: [&egui::epaint::Vertex; 6]) -> bool {
    if edge(vertices[0].pos, vertices[1].pos, vertices[2].pos).abs() <= f32::EPSILON
        || edge(vertices[3].pos, vertices[4].pos, vertices[5].pos).abs() <= f32::EPSILON
    {
        return false;
    }

    let first = [vertices[0].pos, vertices[1].pos, vertices[2].pos];
    let second = [vertices[3].pos, vertices[4].pos, vertices[5].pos];
    if same_pos2(first[0], first[1])
        || same_pos2(first[0], first[2])
        || same_pos2(first[1], first[2])
        || same_pos2(second[0], second[1])
        || same_pos2(second[0], second[2])
        || same_pos2(second[1], second[2])
    {
        return false;
    }

    let mut shared = [egui::Pos2::ZERO; 2];
    let mut shared_count = 0;
    for position in first {
        if second
            .iter()
            .any(|candidate| same_pos2(*candidate, position))
        {
            if shared_count == shared.len() {
                return false;
            }
            shared[shared_count] = position;
            shared_count += 1;
        }
    }

    shared_count == shared.len()
        && !same_f32(shared[0].x, shared[1].x)
        && !same_f32(shared[0].y, shared[1].y)
}

fn textured_quad_fast_path_candidate(
    vertices: [&egui::epaint::Vertex; 6],
) -> Result<TexturedQuadFastPathCandidate, TexturedQuadFastPathRejection> {
    if !triangles_share_rectangle_diagonal(vertices) {
        return Err(TexturedQuadFastPathRejection::NotRectangleDiagonal);
    }
    let Some(bounds) = axis_aligned_quad_bounds(vertices) else {
        return Err(TexturedQuadFastPathRejection::NotAxisAlignedRectangle);
    };
    let Some(corners) = textured_quad_corners(
        vertices,
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y,
    ) else {
        return Err(TexturedQuadFastPathRejection::CornerAttributeMismatch);
    };
    if corners.tl.color != corners.tr.color
        || corners.tl.color != corners.bl.color
        || corners.tl.color != corners.br.color
    {
        return Err(TexturedQuadFastPathRejection::NonUniformColor);
    }
    let affine_br_uv = egui::pos2(
        corners.tr.uv.x + corners.bl.uv.x - corners.tl.uv.x,
        corners.tr.uv.y + corners.bl.uv.y - corners.tl.uv.y,
    );
    if !near_finite_pos2(corners.br.uv, affine_br_uv, UV_AFFINE_EPSILON) {
        return Err(TexturedQuadFastPathRejection::NonAffineUv);
    }

    Ok(TexturedQuadFastPathCandidate { corners, bounds })
}

fn axis_aligned_quad_bounds(vertices: [&egui::epaint::Vertex; 6]) -> Option<QuadBounds> {
    let mut positions = [egui::Pos2::ZERO; 4];
    let mut position_count = 0;
    for vertex in vertices {
        if !vertex.pos.x.is_finite() || !vertex.pos.y.is_finite() {
            return None;
        }
        if positions[..position_count]
            .iter()
            .any(|position| same_pos2(*position, vertex.pos))
        {
            continue;
        }
        if position_count == positions.len() {
            return None;
        }
        positions[position_count] = vertex.pos;
        position_count += 1;
    }
    if position_count != positions.len() {
        return None;
    }

    let mut xs = [0.0; 2];
    let mut ys = [0.0; 2];
    let mut x_count = 0;
    let mut y_count = 0;
    for position in positions {
        if !push_unique_f32(&mut xs, &mut x_count, position.x)
            || !push_unique_f32(&mut ys, &mut y_count, position.y)
        {
            return None;
        }
    }
    if x_count != xs.len()
        || y_count != ys.len()
        || same_f32(xs[0], xs[1])
        || same_f32(ys[0], ys[1])
    {
        return None;
    }

    let min_x = xs[0].min(xs[1]);
    let max_x = xs[0].max(xs[1]);
    let min_y = ys[0].min(ys[1]);
    let max_y = ys[0].max(ys[1]);
    for x in xs {
        for y in ys {
            if !positions
                .iter()
                .any(|position| same_pos2(*position, egui::pos2(x, y)))
            {
                return None;
            }
        }
    }
    Some(QuadBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    })
}

#[derive(Clone, Copy)]
struct TexturedQuadFastPathCandidate {
    corners: TexturedQuadCorners,
    bounds: QuadBounds,
}

#[derive(Clone, Copy)]
struct QuadBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

#[derive(Clone, Copy)]
struct TriangleVertices<'a> {
    v0: &'a egui::epaint::Vertex,
    v1: &'a egui::epaint::Vertex,
    v2: &'a egui::epaint::Vertex,
}

#[derive(Clone, Copy)]
struct TriangleBoundaryIncludes {
    edge0: bool,
    edge1: bool,
    edge2: bool,
}

#[derive(Clone, Copy)]
struct TriangleCoverage {
    inv_area: f32,
    includes_boundary: TriangleBoundaryIncludes,
}

#[derive(Clone, Copy)]
struct TriangleRowSearch<'a> {
    vertices: TriangleVertices<'a>,
    coverage: TriangleCoverage,
    y: usize,
    candidate_start_x: usize,
    candidate_end_x: usize,
    probe_start_x: usize,
    probe_end_x: usize,
    hinted: bool,
    collect_stats: bool,
}

#[derive(Clone, Copy)]
struct TriangleHintSearch<'a> {
    vertices: TriangleVertices<'a>,
    inv_area: f32,
    bounds: TriangleRasterBounds,
    pixel_center_y: f32,
    start_x: usize,
    end_x: usize,
    narrow_scanlines: bool,
}

struct TriangleRowEndpoints {
    span: Option<(usize, usize)>,
    endpoint_probe_px: usize,
    hint_probe_px: usize,
    canary_probe_px: usize,
    fallback_probe_px: usize,
    direct_probe_px: usize,
    fell_back: bool,
}

#[derive(Clone, Copy)]
struct TexturedQuadCorners {
    tl: egui::epaint::Vertex,
    tr: egui::epaint::Vertex,
    bl: egui::epaint::Vertex,
    br: egui::epaint::Vertex,
}

fn textured_quad_corners(
    vertices: [&egui::epaint::Vertex; 6],
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Option<TexturedQuadCorners> {
    let tl = matching_corner(vertices, egui::pos2(min_x, min_y))?;
    let tr = matching_corner(vertices, egui::pos2(max_x, min_y))?;
    let bl = matching_corner(vertices, egui::pos2(min_x, max_y))?;
    let br = matching_corner(vertices, egui::pos2(max_x, max_y))?;
    Some(TexturedQuadCorners { tl, tr, bl, br })
}

fn matching_corner(
    vertices: [&egui::epaint::Vertex; 6],
    position: egui::Pos2,
) -> Option<egui::epaint::Vertex> {
    let mut corner = None;
    for vertex in vertices {
        if !same_pos2(vertex.pos, position) {
            continue;
        }
        if let Some(existing) = corner {
            if !same_vertex_attributes(existing, *vertex) {
                return None;
            }
        } else {
            corner = Some(*vertex);
        }
    }
    corner
}

fn same_vertex_attributes(left: egui::epaint::Vertex, right: egui::epaint::Vertex) -> bool {
    left.color == right.color && same_pos2(left.uv, right.uv)
}

fn push_unique_f32(values: &mut [f32; 2], count: &mut usize, value: f32) -> bool {
    if values[..*count]
        .iter()
        .any(|candidate| same_f32(*candidate, value))
    {
        return true;
    }
    if *count == values.len() {
        return false;
    }
    values[*count] = value;
    *count += 1;
    true
}

fn rasterize_solid_rect(
    surface: &mut SoftwareSurface,
    bounds: QuadBounds,
    clip: ClipBounds,
    color: [u8; 4],
    mut stats: Option<&mut RasterStats>,
) {
    let QuadBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    } = bounds;
    let start_x = solid_rect_boundary_index(min_x, clip.max_x).max(clip.min_x);
    let end_x = solid_rect_boundary_index(max_x, clip.max_x).min(clip.max_x);
    let start_y = solid_rect_boundary_index(min_y, clip.max_y).max(clip.min_y);
    let end_y = solid_rect_boundary_index(max_y, clip.max_y).min(clip.max_y);
    if start_x >= end_x || start_y >= end_y {
        return;
    }
    if let Some(stats) = &mut stats {
        let px = (end_x - start_x) * (end_y - start_y);
        stats.solid_rect_calls += 1;
        stats.solid_rect_px += px;
        stats.record_alpha_px(color[3], px);
    }
    for y in start_y..end_y {
        surface.blend_span(y, start_x, end_x, color);
    }
}

fn rasterize_textured_rect(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    bounds: QuadBounds,
    texture: &TextureImage,
    clip: ClipBounds,
    stats: Option<&mut RasterStats>,
) {
    let QuadBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    } = bounds;
    let start_x = solid_rect_boundary_index(min_x, clip.max_x).max(clip.min_x);
    let end_x = solid_rect_boundary_index(max_x, clip.max_x).min(clip.max_x);
    let start_y = solid_rect_boundary_index(min_y, clip.max_y).max(clip.min_y);
    let end_y = solid_rect_boundary_index(max_y, clip.max_y).min(clip.max_y);
    if start_x >= end_x || start_y >= end_y {
        return;
    }
    let inv_width = 1.0 / (max_x - min_x);
    let inv_height = 1.0 / (max_y - min_y);
    let vertex_color = color_to_array(corners.tl.color);
    if let Some(stats) = stats {
        let px = (end_x - start_x) * (end_y - start_y);
        let classification = classify_textured_rect(corners, texture);
        stats.textured_rect_calls += 1;
        stats.textured_rect_px += px;
        stats.record_textured_rect_classification(classification, px);
        let raster_start = Instant::now();
        rasterize_textured_rect_with_stats(
            surface,
            corners,
            texture,
            RectRasterRange {
                start_x,
                end_x,
                start_y,
                end_y,
            },
            RectUvBasis {
                min_x,
                min_y,
                inv_width,
                inv_height,
            },
            vertex_color,
            stats,
        );
        stats.record_textured_rect_elapsed(classification.kind, raster_start.elapsed());
    } else {
        rasterize_textured_rect_no_stats(
            surface,
            corners,
            texture,
            RectRasterRange {
                start_x,
                end_x,
                start_y,
                end_y,
            },
            RectUvBasis {
                min_x,
                min_y,
                inv_width,
                inv_height,
            },
            vertex_color,
        );
    }
}

#[derive(Clone, Copy)]
struct TexturedRectClassification {
    kind: TexturedRectKind,
    white_texel: bool,
    uniform_corner_color: bool,
}

#[derive(Clone, Copy)]
enum TexturedRectKind {
    ConstantTexel,
    Sampled,
}

fn classify_textured_rect(
    corners: TexturedQuadCorners,
    texture: &TextureImage,
) -> TexturedRectClassification {
    let uniform_corner_color = corners.tl.color == corners.tr.color
        && corners.tl.color == corners.bl.color
        && corners.tl.color == corners.br.color;
    let Some(texture_color) = textured_rect_constant_texel_color(corners, texture) else {
        return TexturedRectClassification {
            kind: TexturedRectKind::Sampled,
            white_texel: false,
            uniform_corner_color,
        };
    };

    TexturedRectClassification {
        kind: TexturedRectKind::ConstantTexel,
        white_texel: texture_color == [255, 255, 255, 255],
        uniform_corner_color,
    }
}

fn textured_rect_constant_texel_color(
    corners: TexturedQuadCorners,
    texture: &TextureImage,
) -> Option<[u8; 4]> {
    if texture.width == 0 || texture.height == 0 {
        return Some([255, 255, 255, 255]);
    }
    let texel = nearest_texel(texture, corners.tl.uv);
    let same_texel = [corners.tr.uv, corners.bl.uv, corners.br.uv]
        .into_iter()
        .all(|uv| nearest_texel(texture, uv) == texel);
    same_texel.then(|| texel_color(texture, texel))
}

#[derive(Clone, Copy)]
struct RectRasterRange {
    start_x: usize,
    end_x: usize,
    start_y: usize,
    end_y: usize,
}

#[derive(Clone, Copy)]
struct RectUvBasis {
    min_x: f32,
    min_y: f32,
    inv_width: f32,
    inv_height: f32,
}

#[derive(Clone, Copy)]
struct RectUvRow {
    uv: egui::Pos2,
    step_x: egui::Vec2,
}

impl RectUvRow {
    fn uv_at(self, x: usize, start_x: usize) -> egui::Pos2 {
        let dx = usize_to_f32(x - start_x);
        egui::pos2(
            self.step_x.x.mul_add(dx, self.uv.x),
            self.step_x.y.mul_add(dx, self.uv.y),
        )
    }
}

fn rasterize_textured_rect_no_stats(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    vertex_color: [u8; 4],
) {
    if vertex_color == [255, 255, 255, 255] {
        rasterize_textured_rect_no_stats_with_color(
            surface,
            corners,
            texture,
            range,
            uv_basis,
            sample_nearest,
        );
    } else {
        rasterize_textured_rect_no_stats_with_color(
            surface,
            corners,
            texture,
            range,
            uv_basis,
            |texture, uv| modulate_color(vertex_color, sample_nearest(texture, uv)),
        );
    }
}

fn rasterize_textured_rect_no_stats_with_color(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    pixel_color: impl Fn(&TextureImage, egui::Pos2) -> [u8; 4],
) {
    for y in range.start_y..range.end_y {
        let row = textured_rect_uv_row(corners, range.start_x, y, uv_basis);
        for x in range.start_x..range.end_x {
            let color = pixel_color(texture, row.uv_at(x, range.start_x));
            surface.blend_pixel(x, y, color);
        }
    }
}

fn rasterize_textured_rect_with_stats(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    vertex_color: [u8; 4],
    stats: &mut RasterStats,
) {
    if vertex_color == [255, 255, 255, 255] {
        rasterize_textured_rect_with_stats_and_color(
            surface,
            corners,
            texture,
            range,
            uv_basis,
            stats,
            sample_nearest,
        );
    } else {
        rasterize_textured_rect_with_stats_and_color(
            surface,
            corners,
            texture,
            range,
            uv_basis,
            stats,
            |texture, uv| modulate_color(vertex_color, sample_nearest(texture, uv)),
        );
    }
}

fn rasterize_textured_rect_with_stats_and_color(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    stats: &mut RasterStats,
    pixel_color: impl Fn(&TextureImage, egui::Pos2) -> [u8; 4],
) {
    for y in range.start_y..range.end_y {
        let row = textured_rect_uv_row(corners, range.start_x, y, uv_basis);
        for x in range.start_x..range.end_x {
            let color = pixel_color(texture, row.uv_at(x, range.start_x));
            stats.record_alpha_px(color[3], 1);
            surface.blend_pixel(x, y, color);
        }
    }
}

fn textured_rect_uv_row(
    corners: TexturedQuadCorners,
    start_x: usize,
    y: usize,
    uv_basis: RectUvBasis,
) -> RectUvRow {
    let sx = (usize_to_f32(start_x) + 0.5 - uv_basis.min_x) * uv_basis.inv_width;
    let sy = (usize_to_f32(y) + 0.5 - uv_basis.min_y) * uv_basis.inv_height;
    RectUvRow {
        uv: textured_rect_uv(corners, sx, sy),
        step_x: egui::vec2(
            (corners.tr.uv.x - corners.tl.uv.x) * uv_basis.inv_width,
            (corners.tr.uv.y - corners.tl.uv.y) * uv_basis.inv_width,
        ),
    }
}

fn textured_rect_uv(corners: TexturedQuadCorners, sx: f32, sy: f32) -> egui::Pos2 {
    let tl_weight = 1.0 - sx - sy;
    egui::pos2(
        corners
            .tl
            .uv
            .x
            .mul_add(tl_weight, corners.tr.uv.x.mul_add(sx, corners.bl.uv.x * sy)),
        corners
            .tl
            .uv
            .y
            .mul_add(tl_weight, corners.tr.uv.y.mul_add(sx, corners.bl.uv.y * sy)),
    )
}

fn solid_rect_boundary_index(boundary: f32, clip_max: usize) -> usize {
    let threshold = boundary - 0.5;
    if threshold < 0.0 {
        0
    } else {
        f32_to_usize_floor_clamped(threshold, clip_max).saturating_add(1)
    }
}

fn rasterize_solid_triangle(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    color: [u8; 4],
    mut stats: Option<&mut RasterStats>,
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let inv_area = 1.0 / area;
    let coverage = TriangleCoverage {
        inv_area,
        includes_boundary: TriangleBoundaryIncludes {
            edge0: edge_includes_boundary(v1.pos, v2.pos, area),
            edge1: edge_includes_boundary(v2.pos, v0.pos, area),
            edge2: edge_includes_boundary(v0.pos, v1.pos, area),
        },
    };
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        if let Some(stats) = &mut stats {
            let candidate_px = end_x - start_x;
            stats.solid_triangle_candidate_px += candidate_px;
            if candidate_px < bounds.max_x - bounds.min_x {
                stats.solid_triangle_narrowed_rows += 1;
            } else {
                stats.solid_triangle_full_scan_rows += 1;
            }
        }

        let collect_stats = stats.is_some();
        let hint_range = triangle_hint_range_for_row(
            TriangleHintSearch {
                vertices,
                inv_area: coverage.inv_area,
                bounds,
                pixel_center_y: usize_to_f32(y) + 0.5,
                start_x,
                end_x,
                narrow_scanlines,
            },
            &mut stats,
        );
        let (probe_start_x, probe_end_x) = hint_range.unwrap_or((start_x, end_x));
        if let (Some((hint_start_x, hint_end_x)), Some(stats)) = (hint_range, &mut stats) {
            stats.solid_triangle_hint_rows += 1;
            stats.solid_triangle_hint_candidate_px += hint_end_x - hint_start_x;
        }

        let search = TriangleRowSearch {
            vertices,
            coverage,
            y,
            candidate_start_x: start_x,
            candidate_end_x: end_x,
            probe_start_x,
            probe_end_x,
            hinted: hint_range.is_some(),
            collect_stats,
        };
        let endpoints = if let Some(stats) = &mut stats {
            let start = Instant::now();
            let endpoints = triangle_row_endpoints(search);
            stats.solid_triangle_endpoint_search_us += duration_as_us(start.elapsed());
            endpoints
        } else {
            triangle_row_endpoints(search)
        };
        if endpoints.fell_back
            && let Some(stats) = &mut stats
        {
            stats.solid_triangle_hint_fallback_rows += 1;
        }
        if let Some((span_start, span_end)) = endpoints.span {
            if let Some(stats) = &mut stats {
                let start = Instant::now();
                surface.blend_span(y, span_start, span_end, color);
                stats.solid_triangle_blend_span_us += duration_as_us(start.elapsed());
                let px = span_end - span_start;
                stats.solid_triangle_endpoint_probe_px += endpoints.endpoint_probe_px;
                stats.solid_triangle_hint_probe_px += endpoints.hint_probe_px;
                stats.solid_triangle_canary_probe_px += endpoints.canary_probe_px;
                stats.solid_triangle_fallback_probe_px += endpoints.fallback_probe_px;
                stats.solid_triangle_direct_probe_px += endpoints.direct_probe_px;
                stats.solid_triangle_covered_px += px;
                stats.solid_triangle_span_rows += 1;
                stats.solid_triangle_blend_span_calls += 1;
                stats.solid_triangle_span_px += px;
                stats.record_alpha_px(color[3], px);
            } else {
                surface.blend_span(y, span_start, span_end, color);
            }
        } else if let Some(stats) = &mut stats {
            stats.solid_triangle_endpoint_probe_px += endpoints.endpoint_probe_px;
            stats.solid_triangle_hint_probe_px += endpoints.hint_probe_px;
            stats.solid_triangle_canary_probe_px += endpoints.canary_probe_px;
            stats.solid_triangle_fallback_probe_px += endpoints.fallback_probe_px;
            stats.solid_triangle_direct_probe_px += endpoints.direct_probe_px;
        }
    }
}

fn triangle_hint_range_for_row(
    search: TriangleHintSearch<'_>,
    stats: &mut Option<&mut RasterStats>,
) -> Option<(usize, usize)> {
    if !search.narrow_scanlines {
        return None;
    }
    if let Some(stats) = stats {
        let start = Instant::now();
        let hint_range = triangle_hint_x_range(
            search.vertices,
            search.inv_area,
            search.bounds,
            search.pixel_center_y,
            search.start_x,
            search.end_x,
        );
        stats.solid_triangle_hint_build_us += duration_as_us(start.elapsed());
        hint_range
    } else {
        triangle_hint_x_range(
            search.vertices,
            search.inv_area,
            search.bounds,
            search.pixel_center_y,
            search.start_x,
            search.end_x,
        )
    }
}

fn triangle_row_endpoints(search: TriangleRowSearch<'_>) -> TriangleRowEndpoints {
    let (mut span_start, first_probe_px) = triangle_first_covered_x(
        search.probe_start_x,
        search.probe_end_x,
        search.y,
        search.vertices,
        search.coverage,
        search.collect_stats,
    );
    let mut hint_probe_px = 0;
    let mut canary_probe_px = 0;
    let mut fallback_probe_px = 0;
    let mut direct_probe_px = 0;
    if search.hinted {
        hint_probe_px += first_probe_px;
    } else {
        direct_probe_px += first_probe_px;
    }
    let mut fell_back = false;
    if search.hinted {
        fell_back = triangle_hint_needs_fallback(&search, span_start, &mut canary_probe_px);
        if fell_back {
            let (fallback_span_start, fallback_first_probe_px) = triangle_first_covered_x(
                search.candidate_start_x,
                search.candidate_end_x,
                search.y,
                search.vertices,
                search.coverage,
                search.collect_stats,
            );
            fallback_probe_px += fallback_first_probe_px;
            span_start = fallback_span_start;
        }
    }

    let Some(span_start) = span_start else {
        return TriangleRowEndpoints {
            span: None,
            endpoint_probe_px: hint_probe_px
                + canary_probe_px
                + fallback_probe_px
                + direct_probe_px,
            hint_probe_px,
            canary_probe_px,
            fallback_probe_px,
            direct_probe_px,
            fell_back,
        };
    };
    let (last_start_x, last_end_x) = if fell_back {
        (search.candidate_start_x, search.candidate_end_x)
    } else {
        (search.probe_start_x, search.probe_end_x)
    };
    let (last_x, last_probe_px) = triangle_last_covered_x(
        last_start_x,
        last_end_x,
        search.y,
        search.vertices,
        search.coverage,
        search.collect_stats,
    );
    if fell_back {
        fallback_probe_px += last_probe_px;
    } else if search.hinted {
        hint_probe_px += last_probe_px;
    } else {
        direct_probe_px += last_probe_px;
    }
    let span_end = last_x.map_or(span_start + 1, |last_x| last_x.max(span_start) + 1);
    TriangleRowEndpoints {
        span: Some((span_start, span_end)),
        endpoint_probe_px: hint_probe_px + canary_probe_px + fallback_probe_px + direct_probe_px,
        hint_probe_px,
        canary_probe_px,
        fallback_probe_px,
        direct_probe_px,
        fell_back,
    }
}

fn triangle_hint_needs_fallback(
    search: &TriangleRowSearch<'_>,
    span_start: Option<usize>,
    endpoint_probe_px: &mut usize,
) -> bool {
    if span_start.is_none() {
        return true;
    }
    if search.probe_start_x > search.candidate_start_x {
        if search.collect_stats {
            *endpoint_probe_px += 1;
        }
        if triangle_covers_pixel(
            search.probe_start_x - 1,
            search.y,
            search.vertices,
            search.coverage,
        ) {
            return true;
        }
    }
    if search.probe_end_x < search.candidate_end_x {
        if search.collect_stats {
            *endpoint_probe_px += 1;
        }
        if triangle_covers_pixel(
            search.probe_end_x,
            search.y,
            search.vertices,
            search.coverage,
        ) {
            return true;
        }
    }
    false
}

fn triangle_first_covered_x(
    start_x: usize,
    end_x: usize,
    y: usize,
    vertices: TriangleVertices<'_>,
    coverage: TriangleCoverage,
    collect_stats: bool,
) -> (Option<usize>, usize) {
    let mut probe_px = 0;
    for x in start_x..end_x {
        if collect_stats {
            probe_px += 1;
        }
        if triangle_covers_pixel(x, y, vertices, coverage) {
            return (Some(x), probe_px);
        }
    }
    (None, probe_px)
}

fn triangle_last_covered_x(
    start_x: usize,
    end_x: usize,
    y: usize,
    vertices: TriangleVertices<'_>,
    coverage: TriangleCoverage,
    collect_stats: bool,
) -> (Option<usize>, usize) {
    let mut probe_px = 0;
    for x in (start_x..end_x).rev() {
        if collect_stats {
            probe_px += 1;
        }
        if triangle_covers_pixel(x, y, vertices, coverage) {
            return (Some(x), probe_px);
        }
    }
    (None, probe_px)
}

fn triangle_hint_x_range(
    vertices: TriangleVertices<'_>,
    inv_area: f32,
    bounds: TriangleRasterBounds,
    pixel_center_y: f32,
    candidate_start_x: usize,
    candidate_end_x: usize,
) -> Option<(usize, usize)> {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let mut min_center_x = f32::NEG_INFINITY;
    let mut max_center_x = f32::INFINITY;
    for (a, b) in [(v1.pos, v2.pos), (v2.pos, v0.pos), (v0.pos, v1.pos)] {
        let slope_x = edge_step_x(a, b) * inv_area;
        let at_origin = edge(a, b, egui::pos2(0.0, pixel_center_y)) * inv_area;
        if !slope_x.is_finite() || !at_origin.is_finite() {
            return None;
        }
        if same_f32(slope_x, 0.0) {
            if at_origin < 0.0 {
                return None;
            }
            continue;
        }
        let boundary_x = -at_origin / slope_x;
        if !boundary_x.is_finite() {
            return None;
        }
        if slope_x > 0.0 {
            min_center_x = min_center_x.max(boundary_x);
        } else {
            max_center_x = max_center_x.min(boundary_x);
        }
    }
    if !min_center_x.is_finite() || !max_center_x.is_finite() || min_center_x > max_center_x {
        return None;
    }

    let start_x = f32_to_usize_ceil_clamped(min_center_x - 0.5, bounds.max_x)
        .max(candidate_start_x)
        .saturating_sub(TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
        .max(candidate_start_x)
        .max(bounds.min_x);
    let end_x = f32_to_usize_floor_clamped(max_center_x - 0.5, bounds.max_x)
        .saturating_add(1 + TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
        .min(candidate_end_x)
        .min(bounds.max_x)
        .max(start_x);
    if start_x >= end_x {
        return None;
    }
    Some((start_x, end_x))
}

fn triangle_covers_pixel(
    x: usize,
    y: usize,
    vertices: TriangleVertices<'_>,
    coverage: TriangleCoverage,
) -> bool {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let pixel_center = egui::pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
    let w0 = edge(v1.pos, v2.pos, pixel_center) * coverage.inv_area;
    let w1 = edge(v2.pos, v0.pos, pixel_center) * coverage.inv_area;
    let w2 = edge(v0.pos, v1.pos, pixel_center) * coverage.inv_area;
    edge_covers_pixel(w0, coverage.includes_boundary.edge0)
        && edge_covers_pixel(w1, coverage.includes_boundary.edge1)
        && edge_covers_pixel(w2, coverage.includes_boundary.edge2)
}

fn triangle_scanline_x_range(
    positions: [egui::Pos2; 3],
    bounds: TriangleRasterBounds,
    pixel_center_y: f32,
) -> (usize, usize) {
    let mut intersections = [0.0; 3];
    let mut count = 0;
    for (a, b) in [
        (positions[0], positions[1]),
        (positions[1], positions[2]),
        (positions[2], positions[0]),
    ] {
        if same_f32(a.y, b.y) {
            continue;
        }
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        if pixel_center_y < min_y || pixel_center_y > max_y {
            continue;
        }
        let t = (pixel_center_y - a.y) / (b.y - a.y);
        let intersection = a.x + (b.x - a.x) * t;
        if !intersection.is_finite() {
            return (bounds.min_x, bounds.max_x);
        }
        intersections[count] = intersection;
        count += 1;
    }
    if count < 2 {
        return (bounds.min_x, bounds.max_x);
    }

    let mut min_x = intersections[0];
    let mut max_x = intersections[0];
    for intersection in intersections.iter().take(count).skip(1) {
        min_x = min_x.min(*intersection);
        max_x = max_x.max(*intersection);
    }

    let start_x = f32_to_usize_floor_clamped(min_x - 0.5, bounds.max_x)
        .max(bounds.min_x)
        .saturating_sub(TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
        .max(bounds.min_x);
    let end_x = f32_to_usize_ceil_clamped(max_x - 0.5, bounds.max_x)
        .saturating_add(1 + TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
        .min(bounds.max_x)
        .max(start_x);
    if start_x >= end_x {
        return (bounds.min_x, bounds.max_x);
    }
    (start_x, end_x)
}

fn rasterize_textured_triangle(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    texture: &TextureImage,
    bounds: TriangleRasterBounds,
    area: f32,
    stats: Option<&mut RasterStats>,
) {
    if let Some(stats) = stats {
        let raster_start = Instant::now();
        rasterize_textured_triangle_with_stats(surface, vertices, texture, bounds, area, stats);
        stats.sampled_textured_triangle_us += duration_as_us(raster_start.elapsed());
    } else {
        rasterize_textured_triangle_no_stats(surface, vertices, texture, bounds, area);
    }
}

fn rasterize_constant_texel_textured_triangle(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    texture_color: [u8; 4],
    stats: Option<&mut RasterStats>,
) {
    if let Some(stats) = stats {
        let white_texel = is_white_texel(texture_color);
        let raster_start = Instant::now();
        rasterize_constant_texel_textured_triangle_with_stats(
            surface,
            vertices,
            bounds,
            area,
            texture_color,
            white_texel,
            stats,
        );
        let elapsed_us = duration_as_us(raster_start.elapsed());
        stats.constant_texel_textured_triangle_us += elapsed_us;
        if white_texel {
            stats.constant_texel_textured_triangle_white_texel_us += elapsed_us;
        } else {
            stats.constant_texel_textured_triangle_non_white_texel_us += elapsed_us;
        }
    } else {
        rasterize_constant_texel_textured_triangle_no_stats(
            surface,
            vertices,
            bounds,
            area,
            texture_color,
        );
    }
}

fn rasterize_constant_texel_textured_triangle_no_stats(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    texture_color: [u8; 4],
) {
    if texture_color == [255, 255, 255, 255] {
        rasterize_white_constant_texel_textured_triangle_no_stats(surface, vertices, bounds, area);
    } else {
        rasterize_constant_texel_textured_triangle_no_stats_with_color(
            surface,
            vertices,
            bounds,
            area,
            |v0, v1, v2, w0, w1, w2| {
                modulate_color(
                    interpolate_constant_texel_vertex_color(v0, v1, v2, w0, w1, w2),
                    texture_color,
                )
            },
        );
    }
}

fn rasterize_white_constant_texel_textured_triangle_no_stats(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let raster = TriangleRasterState::new(v0, v1, v2, bounds, area);
    let mut row_edge0 = raster.row_edge0;
    let mut row_edge1 = raster.row_edge1;
    let mut row_edge2 = raster.row_edge2;
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;
    let color_step = white_constant_texel_color_step(v0, v1, v2, &raster);

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let (mut pixel_edge0, mut pixel_edge1, mut pixel_edge2) = if narrow_scanlines {
            let pixel_center = egui::pos2(usize_to_f32(start_x) + 0.5, usize_to_f32(y) + 0.5);
            (
                edge(v1.pos, v2.pos, pixel_center),
                edge(v2.pos, v0.pos, pixel_center),
                edge(v0.pos, v1.pos, pixel_center),
            )
        } else {
            (row_edge0, row_edge1, row_edge2)
        };
        let row_color = white_constant_texel_row_color(
            v0,
            v1,
            v2,
            &raster,
            (pixel_edge0, pixel_edge1, pixel_edge2),
        );
        let row_offset = surface.row_offset(y);
        let mut run_start = None;
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if edge_covers_pixel(w0, raster.edge0_includes_boundary)
                && edge_covers_pixel(w1, raster.edge1_includes_boundary)
                && edge_covers_pixel(w2, raster.edge2_includes_boundary)
            {
                run_start.get_or_insert(x);
            } else if let Some(start) = run_start.take() {
                emit_white_constant_texel_run_no_stats(
                    surface,
                    row_color,
                    color_step,
                    start - start_x,
                    x - start,
                    row_offset + start * 4,
                );
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        if let Some(start) = run_start {
            emit_white_constant_texel_run_no_stats(
                surface,
                row_color,
                color_step,
                start - start_x,
                end_x - start,
                row_offset + start * 4,
            );
        }
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

fn rasterize_constant_texel_textured_triangle_no_stats_with_color(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    pixel_color: impl Fn(
        &egui::epaint::Vertex,
        &egui::epaint::Vertex,
        &egui::epaint::Vertex,
        f32,
        f32,
        f32,
    ) -> [u8; 4],
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let raster = TriangleRasterState::new(v0, v1, v2, bounds, area);
    let mut row_edge0 = raster.row_edge0;
    let mut row_edge1 = raster.row_edge1;
    let mut row_edge2 = raster.row_edge2;
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let (mut pixel_edge0, mut pixel_edge1, mut pixel_edge2) = if narrow_scanlines {
            let pixel_center = egui::pos2(usize_to_f32(start_x) + 0.5, usize_to_f32(y) + 0.5);
            (
                edge(v1.pos, v2.pos, pixel_center),
                edge(v2.pos, v0.pos, pixel_center),
                edge(v0.pos, v1.pos, pixel_center),
            )
        } else {
            (row_edge0, row_edge1, row_edge2)
        };
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if edge_covers_pixel(w0, raster.edge0_includes_boundary)
                && edge_covers_pixel(w1, raster.edge1_includes_boundary)
                && edge_covers_pixel(w2, raster.edge2_includes_boundary)
            {
                let color = pixel_color(v0, v1, v2, w0, w1, w2);
                surface.blend_pixel(x, y, color);
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

fn rasterize_constant_texel_textured_triangle_with_stats(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    texture_color: [u8; 4],
    white_texel: bool,
    stats: &mut RasterStats,
) {
    if white_texel {
        rasterize_white_constant_texel_textured_triangle_with_stats(
            surface, vertices, bounds, area, stats,
        );
    } else {
        rasterize_constant_texel_textured_triangle_with_stats_and_color(
            surface,
            vertices,
            bounds,
            area,
            white_texel,
            stats,
            |v0, v1, v2, w0, w1, w2| {
                modulate_color(
                    interpolate_constant_texel_vertex_color(v0, v1, v2, w0, w1, w2),
                    texture_color,
                )
            },
        );
    }
}

fn rasterize_white_constant_texel_textured_triangle_with_stats(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    stats: &mut RasterStats,
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let raster = TriangleRasterState::new(v0, v1, v2, bounds, area);
    let mut row_edge0 = raster.row_edge0;
    let mut row_edge1 = raster.row_edge1;
    let mut row_edge2 = raster.row_edge2;
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;
    let color_step = white_constant_texel_color_step(v0, v1, v2, &raster);

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let candidate_px = end_x - start_x;
        stats.textured_triangle_candidate_px += candidate_px;
        stats.constant_texel_textured_triangle_candidate_px += candidate_px;
        if candidate_px < bounds.max_x - bounds.min_x {
            stats.textured_triangle_narrowed_rows += 1;
        } else {
            stats.textured_triangle_full_scan_rows += 1;
        }
        let (mut pixel_edge0, mut pixel_edge1, mut pixel_edge2) = if narrow_scanlines {
            let pixel_center = egui::pos2(usize_to_f32(start_x) + 0.5, usize_to_f32(y) + 0.5);
            (
                edge(v1.pos, v2.pos, pixel_center),
                edge(v2.pos, v0.pos, pixel_center),
                edge(v0.pos, v1.pos, pixel_center),
            )
        } else {
            (row_edge0, row_edge1, row_edge2)
        };
        let row_color = white_constant_texel_row_color(
            v0,
            v1,
            v2,
            &raster,
            (pixel_edge0, pixel_edge1, pixel_edge2),
        );
        let row_offset = surface.row_offset(y);
        let mut run_start = None;
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if edge_covers_pixel(w0, raster.edge0_includes_boundary)
                && edge_covers_pixel(w1, raster.edge1_includes_boundary)
                && edge_covers_pixel(w2, raster.edge2_includes_boundary)
            {
                run_start.get_or_insert(x);
            } else if let Some(start) = run_start.take() {
                emit_white_constant_texel_run_with_stats(
                    surface,
                    row_color,
                    color_step,
                    start - start_x,
                    x - start,
                    row_offset + start * 4,
                    stats,
                );
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        if let Some(start) = run_start {
            emit_white_constant_texel_run_with_stats(
                surface,
                row_color,
                color_step,
                start - start_x,
                end_x - start,
                row_offset + start * 4,
                stats,
            );
        }
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

fn emit_white_constant_texel_run_no_stats(
    surface: &mut SoftwareSurface,
    row_color: [f32; 4],
    color_step: [f32; 4],
    start_dx: usize,
    len: usize,
    mut pixel_offset: usize,
) {
    for run_dx in 0..len {
        let dx = start_dx + run_dx;
        let alpha = white_constant_texel_pixel_alpha(row_color, color_step, dx);
        match alpha {
            0 => {}
            u8::MAX => {
                let color =
                    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha);
                surface.write_opaque_pixel_at_offset(pixel_offset, color);
            }
            _ => {
                let color =
                    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha);
                surface.blend_translucent_pixel_at_offset(pixel_offset, color);
            }
        }
        pixel_offset += 4;
    }
}

fn emit_white_constant_texel_run_with_stats(
    surface: &mut SoftwareSurface,
    row_color: [f32; 4],
    color_step: [f32; 4],
    start_dx: usize,
    len: usize,
    mut pixel_offset: usize,
    stats: &mut RasterStats,
) {
    stats.textured_triangle_covered_px += len;
    stats.constant_texel_textured_triangle_covered_px += len;
    stats.constant_texel_textured_triangle_white_texel_covered_px += len;
    for run_dx in 0..len {
        let dx = start_dx + run_dx;
        let alpha = white_constant_texel_pixel_alpha(row_color, color_step, dx);
        match alpha {
            0 => {
                stats.transparent_px += 1;
                stats.constant_texel_textured_triangle_transparent_px += 1;
            }
            u8::MAX => {
                stats.opaque_px += 1;
                stats.constant_texel_textured_triangle_opaque_px += 1;
                let color =
                    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha);
                surface.write_opaque_pixel_at_offset(pixel_offset, color);
            }
            _ => {
                stats.translucent_px += 1;
                stats.constant_texel_textured_triangle_translucent_px += 1;
                let color =
                    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha);
                surface.blend_translucent_pixel_at_offset(pixel_offset, color);
            }
        }
        pixel_offset += 4;
    }
}

fn rasterize_constant_texel_textured_triangle_with_stats_and_color(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    bounds: TriangleRasterBounds,
    area: f32,
    white_texel: bool,
    stats: &mut RasterStats,
    pixel_color: impl Fn(
        &egui::epaint::Vertex,
        &egui::epaint::Vertex,
        &egui::epaint::Vertex,
        f32,
        f32,
        f32,
    ) -> [u8; 4],
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let raster = TriangleRasterState::new(v0, v1, v2, bounds, area);
    let mut row_edge0 = raster.row_edge0;
    let mut row_edge1 = raster.row_edge1;
    let mut row_edge2 = raster.row_edge2;
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let candidate_px = end_x - start_x;
        stats.textured_triangle_candidate_px += candidate_px;
        stats.constant_texel_textured_triangle_candidate_px += candidate_px;
        if candidate_px < bounds.max_x - bounds.min_x {
            stats.textured_triangle_narrowed_rows += 1;
        } else {
            stats.textured_triangle_full_scan_rows += 1;
        }
        let (mut pixel_edge0, mut pixel_edge1, mut pixel_edge2) = if narrow_scanlines {
            let pixel_center = egui::pos2(usize_to_f32(start_x) + 0.5, usize_to_f32(y) + 0.5);
            (
                edge(v1.pos, v2.pos, pixel_center),
                edge(v2.pos, v0.pos, pixel_center),
                edge(v0.pos, v1.pos, pixel_center),
            )
        } else {
            (row_edge0, row_edge1, row_edge2)
        };
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if edge_covers_pixel(w0, raster.edge0_includes_boundary)
                && edge_covers_pixel(w1, raster.edge1_includes_boundary)
                && edge_covers_pixel(w2, raster.edge2_includes_boundary)
            {
                let color = pixel_color(v0, v1, v2, w0, w1, w2);
                surface.blend_pixel(x, y, color);
                stats.textured_triangle_covered_px += 1;
                stats.constant_texel_textured_triangle_covered_px += 1;
                if white_texel {
                    stats.constant_texel_textured_triangle_white_texel_covered_px += 1;
                } else {
                    stats.constant_texel_textured_triangle_non_white_texel_covered_px += 1;
                }
                stats.record_alpha_px(color[3], 1);
                stats.record_constant_texel_alpha_px(color[3], 1);
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

fn rasterize_textured_triangle_no_stats(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    texture: &TextureImage,
    bounds: TriangleRasterBounds,
    area: f32,
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let raster = TriangleRasterState::new(v0, v1, v2, bounds, area);
    let mut row_edge0 = raster.row_edge0;
    let mut row_edge1 = raster.row_edge1;
    let mut row_edge2 = raster.row_edge2;
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let (mut pixel_edge0, mut pixel_edge1, mut pixel_edge2) = if narrow_scanlines {
            let pixel_center = egui::pos2(usize_to_f32(start_x) + 0.5, usize_to_f32(y) + 0.5);
            (
                edge(v1.pos, v2.pos, pixel_center),
                edge(v2.pos, v0.pos, pixel_center),
                edge(v0.pos, v1.pos, pixel_center),
            )
        } else {
            (row_edge0, row_edge1, row_edge2)
        };
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if edge_covers_pixel(w0, raster.edge0_includes_boundary)
                && edge_covers_pixel(w1, raster.edge1_includes_boundary)
                && edge_covers_pixel(w2, raster.edge2_includes_boundary)
            {
                let color = textured_triangle_pixel_color(v0, v1, v2, texture, w0, w1, w2);
                surface.blend_pixel(x, y, color);
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

fn rasterize_textured_triangle_with_stats(
    surface: &mut SoftwareSurface,
    vertices: TriangleVertices<'_>,
    texture: &TextureImage,
    bounds: TriangleRasterBounds,
    area: f32,
    stats: &mut RasterStats,
) {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let raster = TriangleRasterState::new(v0, v1, v2, bounds, area);
    let mut row_edge0 = raster.row_edge0;
    let mut row_edge1 = raster.row_edge1;
    let mut row_edge2 = raster.row_edge2;
    let positions = triangle_positions(vertices);
    let narrow_scanlines = bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA;

    for y in bounds.min_y..bounds.max_y {
        let (start_x, end_x) = if narrow_scanlines {
            triangle_scanline_x_range(positions, bounds, usize_to_f32(y) + 0.5)
        } else {
            (bounds.min_x, bounds.max_x)
        };
        let candidate_px = end_x - start_x;
        stats.textured_triangle_candidate_px += candidate_px;
        stats.sampled_textured_triangle_candidate_px += candidate_px;
        if candidate_px < bounds.max_x - bounds.min_x {
            stats.textured_triangle_narrowed_rows += 1;
        } else {
            stats.textured_triangle_full_scan_rows += 1;
        }
        let (mut pixel_edge0, mut pixel_edge1, mut pixel_edge2) = if narrow_scanlines {
            let pixel_center = egui::pos2(usize_to_f32(start_x) + 0.5, usize_to_f32(y) + 0.5);
            (
                edge(v1.pos, v2.pos, pixel_center),
                edge(v2.pos, v0.pos, pixel_center),
                edge(v0.pos, v1.pos, pixel_center),
            )
        } else {
            (row_edge0, row_edge1, row_edge2)
        };
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if edge_covers_pixel(w0, raster.edge0_includes_boundary)
                && edge_covers_pixel(w1, raster.edge1_includes_boundary)
                && edge_covers_pixel(w2, raster.edge2_includes_boundary)
            {
                let color = textured_triangle_pixel_color(v0, v1, v2, texture, w0, w1, w2);
                surface.blend_pixel(x, y, color);
                stats.textured_triangle_covered_px += 1;
                stats.sampled_textured_triangle_covered_px += 1;
                stats.record_alpha_px(color[3], 1);
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

struct TriangleRasterState {
    inv_area: f32,
    w0_step_x: f32,
    w1_step_x: f32,
    w2_step_x: f32,
    w0_step_y: f32,
    w1_step_y: f32,
    w2_step_y: f32,
    row_edge0: f32,
    row_edge1: f32,
    row_edge2: f32,
    edge0_includes_boundary: bool,
    edge1_includes_boundary: bool,
    edge2_includes_boundary: bool,
}

impl TriangleRasterState {
    fn new(
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        bounds: TriangleRasterBounds,
        area: f32,
    ) -> Self {
        let start = egui::pos2(
            usize_to_f32(bounds.min_x) + 0.5,
            usize_to_f32(bounds.min_y) + 0.5,
        );
        Self {
            inv_area: 1.0 / area,
            w0_step_x: edge_step_x(v1.pos, v2.pos),
            w1_step_x: edge_step_x(v2.pos, v0.pos),
            w2_step_x: edge_step_x(v0.pos, v1.pos),
            w0_step_y: edge_step_y(v1.pos, v2.pos),
            w1_step_y: edge_step_y(v2.pos, v0.pos),
            w2_step_y: edge_step_y(v0.pos, v1.pos),
            row_edge0: edge(v1.pos, v2.pos, start),
            row_edge1: edge(v2.pos, v0.pos, start),
            row_edge2: edge(v0.pos, v1.pos, start),
            edge0_includes_boundary: edge_includes_boundary(v1.pos, v2.pos, area),
            edge1_includes_boundary: edge_includes_boundary(v2.pos, v0.pos, area),
            edge2_includes_boundary: edge_includes_boundary(v0.pos, v1.pos, area),
        }
    }
}

// White constant-texel triangles compute each pixel color from the actual scanline start plus
// `dx * step`. Do not change this to accumulated `color += step`; accumulated color drift can
// cross u8 rounding boundaries on long rows and diverge from full barycentric interpolation.
fn white_constant_texel_pixel_color(
    row_color: [f32; 4],
    color_step: [f32; 4],
    dx: usize,
) -> [u8; 4] {
    let alpha = white_constant_texel_pixel_alpha(row_color, color_step, dx);
    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha)
}

fn white_constant_texel_pixel_color_with_alpha(
    row_color: [f32; 4],
    color_step: [f32; 4],
    dx: usize,
    alpha: u8,
) -> [u8; 4] {
    let dx = usize_to_f32(dx);
    [
        f32_to_u8_round_clamped(color_step[0].mul_add(dx, row_color[0])),
        f32_to_u8_round_clamped(color_step[1].mul_add(dx, row_color[1])),
        f32_to_u8_round_clamped(color_step[2].mul_add(dx, row_color[2])),
        alpha,
    ]
}

fn white_constant_texel_pixel_alpha(row_color: [f32; 4], color_step: [f32; 4], dx: usize) -> u8 {
    f32_to_u8_round_clamped(color_step[3].mul_add(usize_to_f32(dx), row_color[3]))
}

fn white_constant_texel_row_color(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    raster: &TriangleRasterState,
    row_edges: (f32, f32, f32),
) -> [f32; 4] {
    let w0 = row_edges.0 * raster.inv_area;
    let w1 = row_edges.1 * raster.inv_area;
    let w2 = row_edges.2 * raster.inv_area;
    [
        interpolate_channel_value(v0.color.r(), v1.color.r(), v2.color.r(), w0, w1, w2),
        interpolate_channel_value(v0.color.g(), v1.color.g(), v2.color.g(), w0, w1, w2),
        interpolate_channel_value(v0.color.b(), v1.color.b(), v2.color.b(), w0, w1, w2),
        interpolate_channel_value(v0.color.a(), v1.color.a(), v2.color.a(), w0, w1, w2),
    ]
}

fn white_constant_texel_color_step(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    raster: &TriangleRasterState,
) -> [f32; 4] {
    let w0_step = raster.w0_step_x * raster.inv_area;
    let w1_step = raster.w1_step_x * raster.inv_area;
    let w2_step = raster.w2_step_x * raster.inv_area;
    [
        interpolate_channel_value(
            v0.color.r(),
            v1.color.r(),
            v2.color.r(),
            w0_step,
            w1_step,
            w2_step,
        ),
        interpolate_channel_value(
            v0.color.g(),
            v1.color.g(),
            v2.color.g(),
            w0_step,
            w1_step,
            w2_step,
        ),
        interpolate_channel_value(
            v0.color.b(),
            v1.color.b(),
            v2.color.b(),
            w0_step,
            w1_step,
            w2_step,
        ),
        interpolate_channel_value(
            v0.color.a(),
            v1.color.a(),
            v2.color.a(),
            w0_step,
            w1_step,
            w2_step,
        ),
    ]
}

fn textured_triangle_pixel_color(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
    w0: f32,
    w1: f32,
    w2: f32,
) -> [u8; 4] {
    let uv = egui::pos2(
        v0.uv.x.mul_add(w0, v1.uv.x.mul_add(w1, v2.uv.x * w2)),
        v0.uv.y.mul_add(w0, v1.uv.y.mul_add(w1, v2.uv.y * w2)),
    );
    let vertex_color = interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2);
    let texture_color = sample_nearest(texture, uv);
    modulate_color(vertex_color, texture_color)
}

fn interpolate_constant_texel_vertex_color(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    w0: f32,
    w1: f32,
    w2: f32,
) -> [u8; 4] {
    interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2)
}

pub(super) fn solid_triangle_color_decision(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> SolidTriangleColorDecision {
    if v0.color != v1.color || v0.color != v2.color {
        return SolidTriangleColorDecision::NonUniformVertexColor;
    }

    if texture.width == 0 || texture.height == 0 {
        return SolidTriangleColorDecision::Solid(modulate_color(
            color_to_array(v0.color),
            [255, 255, 255, 255],
        ));
    }

    let t0 = nearest_texel(texture, v0.uv);
    if t0 != nearest_texel(texture, v1.uv) || t0 != nearest_texel(texture, v2.uv) {
        return SolidTriangleColorDecision::NonUniformTexel;
    }

    SolidTriangleColorDecision::Solid(modulate_color(
        color_to_array(v0.color),
        texel_color(texture, t0),
    ))
}

pub(super) fn triangle_nearest_texel_sample(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> TriangleTexelSample {
    if texture.width == 0 || texture.height == 0 {
        return TriangleTexelSample {
            texels: None,
            uniform_color: Some([255, 255, 255, 255]),
        };
    }

    let texels = [
        nearest_texel(texture, v0.uv),
        nearest_texel(texture, v1.uv),
        nearest_texel(texture, v2.uv),
    ];
    let uniform_color = if texels[0] == texels[1] && texels[0] == texels[2] {
        Some(texel_color(texture, texels[0]))
    } else {
        None
    };

    TriangleTexelSample {
        texels: Some(texels),
        uniform_color,
    }
}

fn solid_triangle_color(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> Option<[u8; 4]> {
    match solid_triangle_color_decision(v0, v1, v2, texture) {
        SolidTriangleColorDecision::Solid(color) => Some(color),
        SolidTriangleColorDecision::NonUniformVertexColor
        | SolidTriangleColorDecision::NonUniformTexel => None,
    }
}

fn nearest_texel(texture: &TextureImage, uv: egui::Pos2) -> (usize, usize) {
    let x = f32_to_usize_round_clamped(
        uv.x.clamp(0.0, 1.0) * usize_to_f32(texture.width.saturating_sub(1)),
        texture.width.saturating_sub(1),
    );
    let y = f32_to_usize_round_clamped(
        uv.y.clamp(0.0, 1.0) * usize_to_f32(texture.height.saturating_sub(1)),
        texture.height.saturating_sub(1),
    );
    (x, y)
}

fn texel_color(texture: &TextureImage, texel: (usize, usize)) -> [u8; 4] {
    let offset = (texel.1 * texture.width + texel.0) * 4;
    [
        texture.pixels[offset],
        texture.pixels[offset + 1],
        texture.pixels[offset + 2],
        texture.pixels[offset + 3],
    ]
}

fn color_to_array(color: egui::Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

fn sample_nearest(texture: &TextureImage, uv: egui::Pos2) -> [u8; 4] {
    if texture.width == 0 || texture.height == 0 {
        return [255, 255, 255, 255];
    }
    texel_color(texture, nearest_texel(texture, uv))
}

fn interpolate_color(
    c0: egui::Color32,
    c1: egui::Color32,
    c2: egui::Color32,
    w0: f32,
    w1: f32,
    w2: f32,
) -> [u8; 4] {
    [
        interpolate_channel(c0.r(), c1.r(), c2.r(), w0, w1, w2),
        interpolate_channel(c0.g(), c1.g(), c2.g(), w0, w1, w2),
        interpolate_channel(c0.b(), c1.b(), c2.b(), w0, w1, w2),
        interpolate_channel(c0.a(), c1.a(), c2.a(), w0, w1, w2),
    ]
}

fn interpolate_channel(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> u8 {
    f32_to_u8_round_clamped(interpolate_channel_value(c0, c1, c2, w0, w1, w2))
}

fn interpolate_channel_value(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> f32 {
    f32::from(c0).mul_add(w0, f32::from(c1).mul_add(w1, f32::from(c2) * w2))
}

fn modulate_color(vertex: [u8; 4], texture: [u8; 4]) -> [u8; 4] {
    [
        multiply_u8(vertex[0], texture[0]),
        multiply_u8(vertex[1], texture[1]),
        multiply_u8(vertex[2], texture[2]),
        multiply_u8(vertex[3], texture[3]),
    ]
}

fn multiply_u8(a: u8, b: u8) -> u8 {
    u8::try_from((u16::from(a) * u16::from(b) + 127) / 255).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usize_conversions_clamp_floor_ceil_and_round() {
        assert_eq!(f32_to_usize_floor_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_floor_clamped(1.75, 10), 1);
        assert_eq!(f32_to_usize_floor_clamped(12.0, 10), 10);

        assert_eq!(f32_to_usize_ceil_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_ceil_clamped(1.25, 10), 2);
        assert_eq!(f32_to_usize_ceil_clamped(12.0, 10), 10);

        assert_eq!(f32_to_usize_round_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_round_clamped(1.49, 10), 1);
        assert_eq!(f32_to_usize_round_clamped(1.5, 10), 2);
        assert_eq!(f32_to_usize_round_clamped(12.0, 10), 10);
    }

    #[test]
    fn u8_conversion_rounds_and_clamps() {
        assert_eq!(f32_to_u8_round_clamped(-1.0), 0);
        assert_eq!(f32_to_u8_round_clamped(1.49), 1);
        assert_eq!(f32_to_u8_round_clamped(1.5), 2);
        assert_eq!(f32_to_u8_round_clamped(300.0), 255);
    }

    #[test]
    fn nearest_texture_sampling_clamps_to_edges() {
        let texture = TextureImage {
            width: 2,
            height: 2,
            pixels: vec![
                1, 0, 0, 255, // top-left
                2, 0, 0, 255, // top-right
                3, 0, 0, 255, // bottom-left
                4, 0, 0, 255, // bottom-right
            ],
        };

        assert_eq!(sample_nearest(&texture, egui::pos2(-1.0, -1.0))[0], 1);
        assert_eq!(sample_nearest(&texture, egui::pos2(2.0, -1.0))[0], 2);
        assert_eq!(sample_nearest(&texture, egui::pos2(-1.0, 2.0))[0], 3);
        assert_eq!(sample_nearest(&texture, egui::pos2(2.0, 2.0))[0], 4);
    }

    #[test]
    fn triangle_rasterizer_draws_into_tiny_surface() {
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(2.0, 0.0);
        let v2 = test_vertex(0.0, 2.0);

        let pixels = render_test_triangle(2, 2, &v0, &v1, &v2);

        assert_eq!(white_pixel_count(&pixels), 3);
    }

    #[test]
    fn triangle_rasterizer_draws_clockwise_and_counter_clockwise_consistently() {
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(3.0, 0.0);
        let v2 = test_vertex(0.0, 3.0);

        let counter_clockwise = render_test_triangle(3, 3, &v0, &v1, &v2);
        let clockwise = render_test_triangle(3, 3, &v0, &v2, &v1);

        assert_eq!(counter_clockwise, clockwise);
        assert_eq!(white_pixel_count(&counter_clockwise), 6);
    }

    #[test]
    fn solid_triangle_fast_path_matches_generic_for_same_texel_uvs() {
        let texture = test_texture_2x2();
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(4.0, 0.0);
        let mut v2 = test_vertex(0.0, 4.0);
        v0.color = egui::Color32::from_rgba_premultiplied(128, 64, 32, 255);
        v1.color = v0.color;
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(0.2, 0.2);
        v2.uv = egui::pos2(0.49, 0.49);

        assert_eq!(
            solid_triangle_color(&v0, &v1, &v2, &texture),
            Some(modulate_color(
                color_to_array(v0.color),
                [64, 128, 192, 255]
            ))
        );
        assert_eq!(
            render_test_triangle_with(4, 4, &v0, &v1, &v2, &texture),
            render_test_triangle_generic(4, 4, &v0, &v1, &v2, &texture)
        );
    }

    #[test]
    fn solid_triangle_fast_path_matches_generic_for_empty_texture() {
        let texture = TextureImage {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(3.0, 0.0);
        let mut v2 = test_vertex(0.0, 3.0);
        v0.color = egui::Color32::from_rgba_premultiplied(20, 40, 60, 128);
        v1.color = v0.color;
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(1.0, 0.0);
        v2.uv = egui::pos2(0.0, 1.0);

        assert_eq!(
            solid_triangle_color(&v0, &v1, &v2, &texture),
            Some(color_to_array(v0.color))
        );
        assert_eq!(
            render_test_triangle_with(3, 3, &v0, &v1, &v2, &texture),
            render_test_triangle_generic(3, 3, &v0, &v1, &v2, &texture)
        );
    }

    #[test]
    fn solid_triangle_fast_path_rejects_non_uniform_color_or_uv() {
        let texture = test_texture_2x2();
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(3.0, 0.0);
        let mut v2 = test_vertex(0.0, 3.0);
        v0.color = egui::Color32::from_rgb(64, 64, 64);
        v1.color = egui::Color32::from_rgb(65, 64, 64);
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(0.0, 0.0);
        v2.uv = egui::pos2(0.0, 0.0);

        assert_eq!(solid_triangle_color(&v0, &v1, &v2, &texture), None);

        v1.color = v0.color;
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(1.0, 0.0);
        v2.uv = egui::pos2(0.0, 0.0);

        assert_eq!(solid_triangle_color(&v0, &v1, &v2, &texture), None);
    }

    #[test]
    fn constant_texel_textured_triangle_matches_generic_for_white_texel() {
        let texture = test_solid_2x2_texture([255, 255, 255, 255]);
        let mut vertices = [
            solid_vertex(0.5, 0.25, [32, 80, 120, 255]),
            solid_vertex(5.25, 1.0, [200, 48, 16, 192]),
            solid_vertex(1.25, 5.5, [64, 180, 220, 96]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);

        assert_eq!(
            solid_triangle_color(&vertices[0], &vertices[1], &vertices[2], &texture),
            None
        );
        assert_eq!(
            triangle_nearest_texel_sample(&vertices[0], &vertices[1], &vertices[2], &texture)
                .uniform_color,
            Some([255, 255, 255, 255])
        );
        assert_eq!(
            render_test_triangle_with(7, 7, &vertices[0], &vertices[1], &vertices[2], &texture,),
            render_test_triangle_generic(7, 7, &vertices[0], &vertices[1], &vertices[2], &texture,)
        );
    }

    #[test]
    fn constant_texel_textured_triangle_matches_generic_for_wide_white_texel_row() {
        let texture = test_solid_2x2_texture([255, 255, 255, 255]);
        let mut vertices = [
            solid_vertex(1.0, 1.0, [3, 251, 17, 37]),
            solid_vertex(258.0, 3.0, [249, 5, 229, 241]),
            solid_vertex(2.0, 24.0, [71, 137, 43, 149]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);

        assert_eq!(
            triangle_nearest_texel_sample(&vertices[0], &vertices[1], &vertices[2], &texture)
                .uniform_color,
            Some([255, 255, 255, 255])
        );
        assert_eq!(
            render_test_triangle_with(260, 28, &vertices[0], &vertices[1], &vertices[2], &texture),
            render_test_triangle_generic(
                260,
                28,
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &texture
            )
        );
    }

    #[test]
    fn constant_texel_textured_triangle_matches_generic_for_narrowed_white_texel_scanlines() {
        let texture = test_solid_2x2_texture([255, 255, 255, 255]);
        let mut vertices = [
            solid_vertex(9.25, 11.5, [11, 239, 31, 53]),
            solid_vertex(185.75, 38.25, [227, 19, 211, 233]),
            solid_vertex(36.5, 149.75, [83, 151, 67, 127]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);
        let bounds = triangle_raster_bounds(
            &vertices[0],
            &vertices[1],
            &vertices[2],
            full_clip(200, 160),
        )
        .expect("triangle bounds");

        assert!(bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA);
        assert_eq!(
            triangle_nearest_texel_sample(&vertices[0], &vertices[1], &vertices[2], &texture)
                .uniform_color,
            Some([255, 255, 255, 255])
        );
        assert_eq!(
            render_test_triangle_with(200, 160, &vertices[0], &vertices[1], &vertices[2], &texture),
            render_test_triangle_generic(
                200,
                160,
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &texture
            )
        );
    }

    #[test]
    fn constant_texel_textured_triangle_matches_generic_for_non_white_translucent_texel() {
        let texture_color = [77, 131, 199, 113];
        let texture = test_solid_2x2_texture(texture_color);
        let mut vertices = [
            solid_vertex(0.5, 0.25, [17, 91, 203, 251]),
            solid_vertex(5.25, 1.0, [241, 37, 71, 173]),
            solid_vertex(1.25, 5.5, [83, 219, 29, 67]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);

        let routed =
            render_test_triangle_with(7, 7, &vertices[0], &vertices[1], &vertices[2], &texture);
        let generic =
            render_test_triangle_generic(7, 7, &vertices[0], &vertices[1], &vertices[2], &texture);
        let premodulated_vertices = vertices.map(|mut vertex| {
            let color = modulate_color(color_to_array(vertex.color), texture_color);
            vertex.color =
                egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
            vertex
        });
        let premodulated = render_test_triangle_with(
            7,
            7,
            &premodulated_vertices[0],
            &premodulated_vertices[1],
            &premodulated_vertices[2],
            &test_white_texture(),
        );

        assert_eq!(
            triangle_nearest_texel_sample(&vertices[0], &vertices[1], &vertices[2], &texture)
                .uniform_color,
            Some(texture_color)
        );
        assert_eq!(routed, generic);
        assert_ne!(routed, premodulated);
    }

    #[test]
    fn constant_texel_textured_triangle_stats_use_textured_counters() {
        let texture = test_solid_2x2_texture([77, 131, 199, 113]);
        let mut vertices = [
            solid_vertex(0.0, 0.0, [17, 91, 203, 251]),
            solid_vertex(4.0, 0.0, [241, 37, 71, 173]),
            solid_vertex(0.0, 4.0, [83, 219, 29, 67]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);
        let mut surface = test_surface(5, 5);
        let mut stats = RasterStats::default();

        assert_eq!(
            triangle_nearest_texel_sample(&vertices[0], &vertices[1], &vertices[2], &texture)
                .uniform_color,
            Some([77, 131, 199, 113])
        );

        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            full_clip(5, 5),
            Some(&mut stats),
        );

        assert_eq!(stats.solid_triangle_calls, 0);
        assert_eq!(stats.textured_triangle_calls, 1);
        assert_eq!(stats.textured_triangle_bbox_px, 16);
        assert!(stats.textured_triangle_candidate_px >= stats.textured_triangle_covered_px);
        assert!(stats.textured_triangle_covered_px > 0);
        assert_eq!(stats.constant_texel_textured_triangle_calls, 1);
        assert_eq!(stats.constant_texel_textured_triangle_white_texel_calls, 0);
        assert_eq!(
            stats.constant_texel_textured_triangle_non_white_texel_calls,
            1
        );
        assert_eq!(stats.sampled_textured_triangle_calls, 0);
        assert_eq!(
            stats.textured_triangle_candidate_px,
            stats.constant_texel_textured_triangle_candidate_px
                + stats.sampled_textured_triangle_candidate_px
        );
        assert_eq!(
            stats.textured_triangle_covered_px,
            stats.constant_texel_textured_triangle_covered_px
                + stats.sampled_textured_triangle_covered_px
        );
        assert_eq!(stats.opaque_px, 0);
        assert_eq!(stats.transparent_px, 0);
        assert_eq!(stats.translucent_px, stats.textured_triangle_covered_px);
        assert_eq!(stats.constant_texel_textured_triangle_opaque_px, 0);
        assert_eq!(stats.constant_texel_textured_triangle_transparent_px, 0);
        assert_eq!(
            stats.constant_texel_textured_triangle_translucent_px,
            stats.constant_texel_textured_triangle_covered_px
        );
    }

    #[test]
    fn constant_texel_textured_triangle_stats_split_texel_and_alpha_classes() {
        let mut surface = test_surface(5, 5);
        let mut stats = RasterStats::default();
        let mut vertices = [
            solid_vertex(0.0, 0.0, [255, 0, 0, 255]),
            solid_vertex(4.0, 0.0, [0, 255, 0, 255]),
            solid_vertex(0.0, 4.0, [0, 0, 255, 255]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);

        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_solid_2x2_texture([255, 255, 255, 255]),
            full_clip(5, 5),
            Some(&mut stats),
        );
        let white_covered = stats.constant_texel_textured_triangle_white_texel_covered_px;
        assert!(white_covered > 0);

        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_solid_2x2_texture([128, 128, 128, 128]),
            full_clip(5, 5),
            Some(&mut stats),
        );
        assert!(stats.constant_texel_textured_triangle_non_white_texel_covered_px > 0);
        assert!(stats.constant_texel_textured_triangle_translucent_px > 0);

        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_solid_2x2_texture([128, 128, 128, 0]),
            full_clip(5, 5),
            Some(&mut stats),
        );

        assert_eq!(stats.constant_texel_textured_triangle_calls, 3);
        assert_eq!(stats.constant_texel_textured_triangle_white_texel_calls, 1);
        assert_eq!(
            stats.constant_texel_textured_triangle_non_white_texel_calls,
            2
        );
        assert_eq!(stats.sampled_textured_triangle_calls, 0);
        assert_eq!(
            stats.constant_texel_textured_triangle_covered_px,
            stats.constant_texel_textured_triangle_white_texel_covered_px
                + stats.constant_texel_textured_triangle_non_white_texel_covered_px
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_opaque_px,
            white_covered
        );
        assert!(stats.constant_texel_textured_triangle_translucent_px > 0);
        assert!(stats.constant_texel_textured_triangle_transparent_px > 0);
        assert_eq!(
            stats.constant_texel_textured_triangle_covered_px,
            stats.constant_texel_textured_triangle_opaque_px
                + stats.constant_texel_textured_triangle_translucent_px
                + stats.constant_texel_textured_triangle_transparent_px
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_us,
            stats.constant_texel_textured_triangle_white_texel_us
                + stats.constant_texel_textured_triangle_non_white_texel_us
        );
    }

    #[test]
    fn constant_texel_textured_triangle_stats_match_sampled_for_white_texel_varying_alpha() {
        let texture = test_solid_2x2_texture([255, 255, 255, 255]);
        let mut vertices = [
            solid_vertex(1.0, 1.0, [0, 0, 0, 0]),
            solid_vertex(42.0, 2.0, [255, 0, 0, 255]),
            solid_vertex(3.0, 38.0, [0, 0, 96, 96]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);
        let clip = full_clip(48, 42);
        let bounds = triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], clip)
            .expect("triangle bounds");
        let area = edge(vertices[0].pos, vertices[1].pos, vertices[2].pos);
        let mut constant_surface = test_surface(48, 42);
        let mut sampled_surface = test_surface(48, 42);
        let mut constant_stats = RasterStats::default();
        let mut sampled_stats = RasterStats::default();

        rasterize_triangle(
            &mut constant_surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            clip,
            Some(&mut constant_stats),
        );
        rasterize_textured_triangle(
            &mut sampled_surface,
            TriangleVertices {
                v0: &vertices[0],
                v1: &vertices[1],
                v2: &vertices[2],
            },
            &texture,
            bounds,
            area,
            Some(&mut sampled_stats),
        );

        assert_eq!(constant_surface.pixels, sampled_surface.pixels);
        assert_eq!(constant_stats.textured_triangle_calls, 1);
        assert_eq!(constant_stats.constant_texel_textured_triangle_calls, 1);
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_white_texel_calls,
            1
        );
        assert_eq!(constant_stats.sampled_textured_triangle_calls, 0);
        assert_eq!(
            constant_stats.textured_triangle_candidate_px,
            sampled_stats.textured_triangle_candidate_px
        );
        assert_eq!(
            constant_stats.textured_triangle_covered_px,
            sampled_stats.textured_triangle_covered_px
        );
        assert_eq!(
            constant_stats.textured_triangle_narrowed_rows,
            sampled_stats.textured_triangle_narrowed_rows
        );
        assert_eq!(
            constant_stats.textured_triangle_full_scan_rows,
            sampled_stats.textured_triangle_full_scan_rows
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_candidate_px,
            constant_stats.textured_triangle_candidate_px
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_covered_px,
            constant_stats.textured_triangle_covered_px
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_white_texel_covered_px,
            constant_stats.textured_triangle_covered_px
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_non_white_texel_covered_px,
            0
        );
        assert_eq!(constant_stats.opaque_px, sampled_stats.opaque_px);
        assert_eq!(constant_stats.translucent_px, sampled_stats.translucent_px);
        assert_eq!(constant_stats.transparent_px, sampled_stats.transparent_px);
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_opaque_px,
            constant_stats.opaque_px
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_translucent_px,
            constant_stats.translucent_px
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_transparent_px,
            constant_stats.transparent_px
        );
        assert_eq!(
            constant_stats.textured_triangle_covered_px,
            constant_stats.opaque_px
                + constant_stats.translucent_px
                + constant_stats.transparent_px
        );
    }

    #[test]
    fn constant_texel_textured_triangle_stats_count_skipped_transparent_white_pixels() {
        let texture = test_solid_2x2_texture([255, 255, 255, 255]);
        let mut vertices = [
            solid_vertex(0.0, 0.0, [0, 0, 0, 0]),
            solid_vertex(4.0, 0.0, [64, 0, 0, 0]),
            solid_vertex(0.0, 4.0, [0, 64, 0, 0]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(0.2, 0.1);
        vertices[2].uv = egui::pos2(0.49, 0.49);
        let mut constant_surface = test_surface(5, 5);
        let mut sampled_surface = test_surface(5, 5);
        let mut constant_stats = RasterStats::default();
        let mut sampled_stats = RasterStats::default();

        assert_eq!(
            triangle_nearest_texel_sample(&vertices[0], &vertices[1], &vertices[2], &texture)
                .uniform_color,
            Some([255, 255, 255, 255])
        );
        assert_eq!(
            solid_triangle_color(&vertices[0], &vertices[1], &vertices[2], &texture),
            None
        );

        rasterize_triangle(
            &mut constant_surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            full_clip(5, 5),
            Some(&mut constant_stats),
        );
        rasterize_textured_triangle(
            &mut sampled_surface,
            TriangleVertices {
                v0: &vertices[0],
                v1: &vertices[1],
                v2: &vertices[2],
            },
            &texture,
            triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], full_clip(5, 5))
                .expect("triangle bounds"),
            edge(vertices[0].pos, vertices[1].pos, vertices[2].pos),
            Some(&mut sampled_stats),
        );

        assert_eq!(constant_surface.pixels, sampled_surface.pixels);
        assert_eq!(constant_surface.pixels, test_surface(5, 5).pixels);
        assert_eq!(constant_stats.textured_triangle_covered_px, 10);
        assert_eq!(
            constant_stats.textured_triangle_covered_px,
            sampled_stats.textured_triangle_covered_px
        );
        assert_eq!(
            constant_stats.transparent_px,
            constant_stats.textured_triangle_covered_px
        );
        assert_eq!(constant_stats.opaque_px, 0);
        assert_eq!(constant_stats.translucent_px, 0);
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_white_texel_covered_px,
            constant_stats.textured_triangle_covered_px
        );
        assert_eq!(
            constant_stats.constant_texel_textured_triangle_transparent_px,
            constant_stats.textured_triangle_covered_px
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_opaque_triangle() {
        assert_solid_triangle_matches_reference(
            7,
            6,
            full_clip(7, 6),
            [24, 48, 96, 255],
            [
                solid_vertex(1.0, 1.0, [180, 40, 20, 255]),
                solid_vertex(5.0, 1.0, [180, 40, 20, 255]),
                solid_vertex(2.0, 5.0, [180, 40, 20, 255]),
            ],
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_translucent_triangle() {
        assert_solid_triangle_matches_reference(
            7,
            6,
            full_clip(7, 6),
            [20, 80, 140, 255],
            [
                solid_vertex(1.0, 1.0, [96, 48, 24, 128]),
                solid_vertex(5.0, 1.0, [96, 48, 24, 128]),
                solid_vertex(2.0, 5.0, [96, 48, 24, 128]),
            ],
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_both_windings() {
        let vertices = [
            solid_vertex(1.0, 1.0, [40, 120, 80, 192]),
            solid_vertex(5.0, 2.0, [40, 120, 80, 192]),
            solid_vertex(2.0, 5.0, [40, 120, 80, 192]),
        ];

        assert_solid_triangle_matches_reference(
            7,
            6,
            full_clip(7, 6),
            [90, 30, 120, 255],
            vertices,
        );
        assert_solid_triangle_matches_reference(
            7,
            6,
            full_clip(7, 6),
            [90, 30, 120, 255],
            [vertices[0], vertices[2], vertices[1]],
        );
    }

    #[test]
    fn solid_triangle_reference_covers_shared_translucent_diagonal_once() {
        let color = [128, 0, 0, 128];
        let vertices = [
            solid_vertex(0.0, 0.0, color),
            solid_vertex(3.0, 0.0, color),
            solid_vertex(0.0, 3.0, color),
            solid_vertex(3.0, 3.0, color),
        ];
        let clip = full_clip(3, 3);
        let texture = test_white_texture();
        let mut production = test_surface_with_background(3, 3, [0, 0, 255, 255]);
        let mut reference = test_surface_with_background(3, 3, [0, 0, 255, 255]);

        rasterize_triangle(
            &mut production,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            clip,
            None,
        );
        rasterize_triangle(
            &mut production,
            &vertices[1],
            &vertices[3],
            &vertices[2],
            &texture,
            clip,
            None,
        );
        reference_rasterize_solid_triangle(
            &mut reference,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            clip,
            color,
        );
        reference_rasterize_solid_triangle(
            &mut reference,
            &vertices[1],
            &vertices[3],
            &vertices[2],
            clip,
            color,
        );

        assert_eq!(production.pixels, reference.pixels);
        assert_eq!(pixel_at(&production.pixels, 3, 2, 0), [128, 0, 127, 255]);
        assert_eq!(pixel_at(&production.pixels, 3, 1, 1), [128, 0, 127, 255]);
        assert_eq!(pixel_at(&production.pixels, 3, 0, 2), [128, 0, 127, 255]);
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_clipped_triangle() {
        assert_solid_triangle_matches_reference(
            7,
            6,
            ClipBounds {
                min_x: 2,
                min_y: 1,
                max_x: 6,
                max_y: 5,
            },
            [80, 20, 100, 255],
            [
                solid_vertex(-1.0, 0.0, [32, 160, 80, 160]),
                solid_vertex(6.0, 1.0, [32, 160, 80, 160]),
                solid_vertex(1.0, 7.0, [32, 160, 80, 160]),
            ],
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_fractional_coordinates() {
        assert_solid_triangle_matches_reference(
            8,
            7,
            full_clip(8, 7),
            [12, 64, 120, 255],
            [
                solid_vertex(1.25, 0.5, [100, 20, 140, 128]),
                solid_vertex(6.5, 2.25, [100, 20, 140, 128]),
                solid_vertex(2.5, 5.75, [100, 20, 140, 128]),
            ],
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_boundary_drift_triangle() {
        let vertices = [
            solid_vertex(159.34012, 59.640_804, [88, 144, 200, 255]),
            solid_vertex(98.330_84, 448.938_54, [88, 144, 200, 255]),
            solid_vertex(482.737_18, 307.795_17, [88, 144, 200, 255]),
        ];
        let clip = full_clip(512, 512);

        assert_solid_triangle_matches_reference(512, 512, clip, [30, 90, 150, 255], vertices);

        let bounds = triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], clip)
            .expect("triangle bounds");
        assert!(bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA);
        assert!(bounds.min_x <= 98 && bounds.max_x > 482);
        assert!(bounds.min_y <= 59 && bounds.max_y > 448);

        let mut surface = test_surface(512, 512);
        let mut stats = RasterStats::default();
        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_white_texture(),
            clip,
            Some(&mut stats),
        );

        assert_eq!(stats.solid_triangle_calls, 1);
        assert!(stats.solid_triangle_covered_px > 0);
        assert!(stats.solid_triangle_candidate_px < stats.solid_triangle_bbox_px);
    }

    #[test]
    fn solid_triangle_hint_stats_count_large_safe_triangle() {
        let vertices = [
            solid_vertex(48.25, 38.5, [88, 144, 200, 255]),
            solid_vertex(462.75, 92.25, [88, 144, 200, 255]),
            solid_vertex(137.5, 430.75, [88, 144, 200, 255]),
        ];
        let clip = full_clip(512, 512);
        let mut surface = test_surface(512, 512);
        let mut stats = RasterStats::default();

        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_white_texture(),
            clip,
            Some(&mut stats),
        );

        assert_eq!(stats.solid_triangle_calls, 1);
        assert!(stats.solid_triangle_hint_rows > 0);
        assert!(stats.solid_triangle_hint_candidate_px > 0);
        assert!(stats.solid_triangle_endpoint_probe_px > 0);
        assert_eq!(
            stats.solid_triangle_span_px,
            stats.solid_triangle_covered_px
        );
        assert_eq!(
            stats.solid_triangle_blend_span_calls,
            stats.solid_triangle_span_rows
        );
        assert_eq!(
            stats.solid_triangle_endpoint_probe_px,
            stats.solid_triangle_hint_probe_px
                + stats.solid_triangle_canary_probe_px
                + stats.solid_triangle_fallback_probe_px
                + stats.solid_triangle_direct_probe_px
        );
        assert!(stats.solid_triangle_candidate_px >= stats.solid_triangle_hint_candidate_px);
        assert!(stats.solid_triangle_hint_fallback_rows < stats.solid_triangle_hint_rows);
    }

    #[test]
    fn solid_triangle_stats_count_endpoint_probes_without_hints() {
        let vertices = [
            solid_vertex(1.0, 1.0, [160, 32, 64, 255]),
            solid_vertex(6.0, 1.5, [160, 32, 64, 255]),
            solid_vertex(2.0, 6.0, [160, 32, 64, 255]),
        ];
        let clip = full_clip(8, 8);
        let mut surface = test_surface(8, 8);
        let mut stats = RasterStats::default();

        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_white_texture(),
            clip,
            Some(&mut stats),
        );

        assert_eq!(stats.solid_triangle_calls, 1);
        assert!(stats.solid_triangle_endpoint_probe_px > 0);
        assert_eq!(
            stats.solid_triangle_span_px,
            stats.solid_triangle_covered_px
        );
        assert_eq!(
            stats.solid_triangle_blend_span_calls,
            stats.solid_triangle_span_rows
        );
        assert_eq!(
            stats.solid_triangle_endpoint_probe_px,
            stats.solid_triangle_direct_probe_px
        );
        assert_eq!(stats.solid_triangle_hint_rows, 0);
        assert_eq!(stats.solid_triangle_hint_fallback_rows, 0);
        assert_eq!(stats.solid_triangle_hint_candidate_px, 0);
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_thin_sliver() {
        assert_solid_triangle_matches_reference(
            8,
            8,
            full_clip(8, 8),
            [30, 90, 150, 255],
            [
                solid_vertex(1.0, 1.0, [160, 32, 64, 192]),
                solid_vertex(6.0, 1.25, [160, 32, 64, 192]),
                solid_vertex(1.5, 1.75, [160, 32, 64, 192]),
            ],
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_giant_fan_sliver() {
        let vertices = [
            solid_vertex(124.7, 213.3, [160, 32, 64, 192]),
            solid_vertex(517.5, 457.5, [160, 32, 64, 192]),
            solid_vertex(125.5, 212.9, [160, 32, 64, 192]),
        ];
        let clip = full_clip(640, 480);

        assert_solid_triangle_matches_reference(640, 480, clip, [30, 90, 150, 255], vertices);

        let bounds = triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], clip)
            .expect("triangle bounds");
        assert_eq!(
            bounds,
            TriangleRasterBounds {
                min_x: 124,
                min_y: 212,
                max_x: 518,
                max_y: 458,
            }
        );

        let mut surface = test_surface(640, 480);
        let mut stats = RasterStats::default();
        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &test_white_texture(),
            clip,
            Some(&mut stats),
        );

        assert_eq!(stats.solid_triangle_calls, 1);
        assert_eq!(stats.solid_triangle_bbox_px, 96_924);
        assert!(stats.solid_triangle_covered_px > 0);
        assert!(stats.solid_triangle_candidate_px < stats.solid_triangle_bbox_px);
        assert!(stats.solid_triangle_bbox_px >= stats.solid_triangle_covered_px * 8);
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_logged_fan_sliver_both_windings() {
        let vertices = [
            solid_vertex(124.7, 213.3, [88, 144, 200, 255]),
            solid_vertex(517.5, 457.5, [88, 144, 200, 255]),
            solid_vertex(125.5, 212.9, [88, 144, 200, 255]),
        ];
        let reversed = [vertices[0], vertices[2], vertices[1]];
        let clip = full_clip(640, 480);

        assert_solid_triangle_matches_reference(640, 480, clip, [30, 90, 150, 255], vertices);
        assert_solid_triangle_matches_reference(640, 480, clip, [30, 90, 150, 255], reversed);
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_clipped_logged_fan_sliver() {
        assert_solid_triangle_matches_reference(
            640,
            480,
            ClipBounds {
                min_x: 123,
                min_y: 212,
                max_x: 220,
                max_y: 274,
            },
            [30, 90, 150, 255],
            [
                solid_vertex(124.7, 213.3, [88, 144, 200, 255]),
                solid_vertex(517.5, 457.5, [88, 144, 200, 255]),
                solid_vertex(125.5, 212.9, [88, 144, 200, 255]),
            ],
        );
    }

    #[test]
    fn solid_triangle_rasterizer_matches_reference_for_translucent_logged_fan_sliver() {
        assert_solid_triangle_matches_reference(
            640,
            480,
            full_clip(640, 480),
            [30, 90, 150, 255],
            [
                solid_vertex(124.7, 213.3, [192, 96, 48, 160]),
                solid_vertex(517.5, 457.5, [192, 96, 48, 160]),
                solid_vertex(125.5, 212.9, [192, 96, 48, 160]),
            ],
        );
    }

    #[test]
    fn solid_triangle_hint_path_matches_reference_for_fractional_sweep() {
        let backgrounds = [[0, 0, 0, 255], [24, 48, 96, 255]];
        let clips = [
            full_clip(160, 144),
            ClipBounds {
                min_x: 11,
                min_y: 7,
                max_x: 151,
                max_y: 132,
            },
        ];
        for case in 0..24 {
            let offset = usize_to_f32(case) * 0.37;
            let color = if case % 3 == 0 {
                [96, 160, 224, 176]
            } else {
                [96, 160, 224, 255]
            };
            let vertices = [
                solid_vertex(7.125 + offset, 8.25 + offset * 0.5, color),
                solid_vertex(142.75 - offset * 0.25, 18.625 + offset, color),
                solid_vertex(35.5 + offset * 0.75, 136.875 - offset * 0.3, color),
            ];
            let ordered = if case % 2 == 0 {
                vertices
            } else {
                [vertices[0], vertices[2], vertices[1]]
            };
            let clip = clips[case % clips.len()];
            let background = backgrounds[case % backgrounds.len()];

            assert_solid_triangle_matches_reference(160, 144, clip, background, ordered);

            let bounds = triangle_raster_bounds(&ordered[0], &ordered[1], &ordered[2], clip)
                .expect("triangle bounds");
            assert!(bounds.pixel_area() > TRIANGLE_SCANLINE_NARROWING_MIN_AREA);
        }
    }

    #[test]
    fn textured_triangle_rasterizer_matches_reference_for_giant_sliver() {
        let mut vertices = [
            solid_vertex(124.7, 213.3, [192, 96, 48, 224]),
            solid_vertex(517.5, 457.5, [192, 96, 48, 224]),
            solid_vertex(125.5, 212.9, [192, 96, 48, 224]),
        ];
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(1.0, 0.0);
        vertices[2].uv = egui::pos2(0.0, 1.0);
        let clip = full_clip(640, 480);
        let texture = test_texture_4x4();

        assert_eq!(
            classify_triangle(&vertices[0], &vertices[1], &vertices[2], &texture),
            TriangleClassification::Textured
        );
        assert_textured_triangle_matches_reference(
            640,
            480,
            clip,
            [30, 90, 150, 255],
            vertices,
            &texture,
        );

        let bounds = triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], clip)
            .expect("triangle bounds");
        assert_eq!(
            bounds,
            TriangleRasterBounds {
                min_x: 124,
                min_y: 212,
                max_x: 518,
                max_y: 458,
            }
        );
        assert!(bounds.pixel_area() > 1024);

        let mut surface = test_surface_with_background(640, 480, [30, 90, 150, 255]);
        let mut stats = RasterStats::default();
        rasterize_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            clip,
            Some(&mut stats),
        );

        assert_eq!(stats.textured_triangle_calls, 1);
        assert_eq!(stats.textured_triangle_bbox_px, 96_924);
        assert!(stats.textured_triangle_covered_px > 0);
        assert!(stats.textured_triangle_candidate_px < stats.textured_triangle_bbox_px);
        assert!(stats.textured_triangle_bbox_px >= stats.textured_triangle_covered_px * 8);
    }

    #[test]
    fn triangle_classification_matches_rasterizer_solid_and_textured_paths() {
        let texture = test_texture_2x2();
        let v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(3.0, 0.0);
        let mut v2 = test_vertex(0.0, 3.0);

        assert_eq!(
            classify_triangle(&v0, &v1, &v2, &texture),
            TriangleClassification::Solid
        );

        v1.uv = egui::pos2(1.0, 0.0);
        assert_eq!(
            classify_triangle(&v0, &v1, &v2, &texture),
            TriangleClassification::Textured
        );

        v2.pos = v0.pos;
        assert_eq!(
            classify_triangle(&v0, &v1, &v2, &texture),
            TriangleClassification::Degenerate
        );
    }

    #[test]
    fn raster_stats_log_line_includes_all_counters() {
        let stats = RasterStats {
            solid_rect_calls: 1,
            solid_rect_px: 2,
            textured_rect_calls: 3,
            textured_rect_px: 4,
            textured_rect_constant_texel_calls: 43,
            textured_rect_constant_texel_px: 44,
            textured_rect_constant_texel_us: 45,
            textured_rect_sampled_calls: 46,
            textured_rect_sampled_px: 47,
            textured_rect_sampled_us: 48,
            textured_rect_white_texel_calls: 49,
            textured_rect_white_texel_px: 50,
            textured_rect_uniform_color_calls: 51,
            textured_rect_uniform_color_px: 52,
            solid_triangle_calls: 5,
            solid_triangle_bbox_px: 6,
            solid_triangle_covered_px: 7,
            solid_triangle_span_rows: 8,
            solid_triangle_candidate_px: 9,
            solid_triangle_hint_rows: 10,
            solid_triangle_hint_fallback_rows: 11,
            solid_triangle_hint_build_us: 12,
            solid_triangle_endpoint_search_us: 13,
            solid_triangle_blend_span_us: 14,
            solid_triangle_blend_span_calls: 15,
            solid_triangle_span_px: 16,
            solid_triangle_endpoint_probe_px: 17,
            solid_triangle_hint_probe_px: 18,
            solid_triangle_canary_probe_px: 19,
            solid_triangle_fallback_probe_px: 20,
            solid_triangle_direct_probe_px: 21,
            solid_triangle_hint_candidate_px: 22,
            solid_triangle_narrowed_rows: 23,
            solid_triangle_full_scan_rows: 24,
            solid_fan_calls: 25,
            solid_fan_triangles: 26,
            solid_fan_rows: 27,
            solid_fan_px: 28,
            solid_fan_edge_intersections: 29,
            solid_fan_endpoint_probe_px: 30,
            solid_fan_fallback_rows: 31,
            textured_triangle_calls: 32,
            textured_triangle_bbox_px: 33,
            textured_triangle_covered_px: 34,
            textured_triangle_candidate_px: 35,
            textured_triangle_narrowed_rows: 36,
            textured_triangle_full_scan_rows: 37,
            constant_texel_textured_triangle_calls: 53,
            constant_texel_textured_triangle_white_texel_calls: 61,
            constant_texel_textured_triangle_non_white_texel_calls: 62,
            constant_texel_textured_triangle_candidate_px: 54,
            constant_texel_textured_triangle_covered_px: 55,
            constant_texel_textured_triangle_white_texel_covered_px: 63,
            constant_texel_textured_triangle_non_white_texel_covered_px: 64,
            constant_texel_textured_triangle_opaque_px: 65,
            constant_texel_textured_triangle_translucent_px: 66,
            constant_texel_textured_triangle_transparent_px: 67,
            constant_texel_textured_triangle_us: 56,
            constant_texel_textured_triangle_white_texel_us: 68,
            constant_texel_textured_triangle_non_white_texel_us: 69,
            sampled_textured_triangle_calls: 57,
            sampled_textured_triangle_candidate_px: 58,
            sampled_textured_triangle_covered_px: 59,
            sampled_textured_triangle_us: 60,
            degenerate_triangle_skips: 38,
            fully_clipped_triangle_skips: 39,
            opaque_px: 40,
            translucent_px: 41,
            transparent_px: 42,
        };

        assert_eq!(
            stats.log_line(),
            "software renderer raster_stats solid_rect_calls=1 solid_rect_px=2 textured_rect_calls=3 textured_rect_px=4 textured_rect_constant_texel_calls=43 textured_rect_constant_texel_px=44 textured_rect_constant_texel_us=45 textured_rect_sampled_calls=46 textured_rect_sampled_px=47 textured_rect_sampled_us=48 textured_rect_white_texel_calls=49 textured_rect_white_texel_px=50 textured_rect_uniform_color_calls=51 textured_rect_uniform_color_px=52 solid_triangle_calls=5 solid_triangle_bbox_px=6 solid_triangle_covered_px=7 solid_triangle_span_rows=8 solid_triangle_candidate_px=9 solid_triangle_hint_rows=10 solid_triangle_hint_fallback_rows=11 solid_triangle_hint_build_us=12 solid_triangle_endpoint_search_us=13 solid_triangle_blend_span_us=14 solid_triangle_blend_span_calls=15 solid_triangle_span_px=16 solid_triangle_endpoint_probe_px=17 solid_triangle_hint_probe_px=18 solid_triangle_canary_probe_px=19 solid_triangle_fallback_probe_px=20 solid_triangle_direct_probe_px=21 solid_triangle_hint_candidate_px=22 solid_triangle_narrowed_rows=23 solid_triangle_full_scan_rows=24 solid_fan_calls=25 solid_fan_triangles=26 solid_fan_rows=27 solid_fan_px=28 solid_fan_edge_intersections=29 solid_fan_endpoint_probe_px=30 solid_fan_fallback_rows=31 textured_triangle_calls=32 textured_triangle_bbox_px=33 textured_triangle_covered_px=34 textured_triangle_candidate_px=35 textured_triangle_narrowed_rows=36 textured_triangle_full_scan_rows=37 constant_texel_textured_triangle_calls=53 constant_texel_textured_triangle_white_texel_calls=61 constant_texel_textured_triangle_non_white_texel_calls=62 constant_texel_textured_triangle_candidate_px=54 constant_texel_textured_triangle_covered_px=55 constant_texel_textured_triangle_white_texel_covered_px=63 constant_texel_textured_triangle_non_white_texel_covered_px=64 constant_texel_textured_triangle_opaque_px=65 constant_texel_textured_triangle_translucent_px=66 constant_texel_textured_triangle_transparent_px=67 constant_texel_textured_triangle_us=56 constant_texel_textured_triangle_white_texel_us=68 constant_texel_textured_triangle_non_white_texel_us=69 sampled_textured_triangle_calls=57 sampled_textured_triangle_candidate_px=58 sampled_textured_triangle_covered_px=59 sampled_textured_triangle_us=60 degenerate_triangle_skips=38 fully_clipped_triangle_skips=39 opaque_px=40 translucent_px=41 transparent_px=42"
        );
    }

    #[test]
    fn solid_fan_matches_per_triangle_reference() {
        let texture = test_white_texture();
        let vertices = solid_fan_vertices([220, 40, 20, 255]);
        assert_eq!(
            render_test_solid_fan(12, 12, &vertices, &texture),
            render_test_solid_fan_reference(12, 12, &vertices, &texture)
        );
    }

    #[test]
    fn solid_fan_matches_per_triangle_reference_for_translucent_slivers() {
        let texture = test_white_texture();
        let vertices = vec![
            solid_vertex(0.5, 5.0, [80, 20, 0, 128]),
            solid_vertex(1.0, 4.4, [80, 20, 0, 128]),
            solid_vertex(4.0, 4.1, [80, 20, 0, 128]),
            solid_vertex(8.0, 4.4, [80, 20, 0, 128]),
            solid_vertex(9.5, 5.0, [80, 20, 0, 128]),
            solid_vertex(8.0, 5.6, [80, 20, 0, 128]),
            solid_vertex(4.0, 5.9, [80, 20, 0, 128]),
            solid_vertex(1.0, 5.6, [80, 20, 0, 128]),
        ];

        assert_eq!(
            render_test_solid_fan(11, 8, &vertices, &texture),
            render_test_solid_fan_reference(11, 8, &vertices, &texture)
        );
    }

    #[test]
    fn solid_fan_matches_reference_for_fractional_internal_radials() {
        let texture = test_white_texture();
        let vertices = vec![
            solid_vertex(1.5, 1.5, [96, 32, 0, 128]),
            solid_vertex(5.5, 0.5, [96, 32, 0, 128]),
            solid_vertex(9.5, 3.5, [96, 32, 0, 128]),
            solid_vertex(8.5, 8.5, [96, 32, 0, 128]),
            solid_vertex(3.5, 9.5, [96, 32, 0, 128]),
            solid_vertex(0.5, 5.5, [96, 32, 0, 128]),
        ];

        assert_eq!(
            render_test_solid_fan(12, 12, &vertices, &texture),
            render_test_solid_fan_reference(12, 12, &vertices, &texture)
        );
    }

    #[test]
    fn solid_fan_matches_reference_for_fractional_near_radial_edges() {
        let texture = test_white_texture();
        let vertices = vec![
            solid_vertex(2.25, 1.75, [40, 120, 20, 192]),
            solid_vertex(6.75, 0.75, [40, 120, 20, 192]),
            solid_vertex(10.25, 4.25, [40, 120, 20, 192]),
            solid_vertex(9.75, 8.75, [40, 120, 20, 192]),
            solid_vertex(4.25, 10.25, [40, 120, 20, 192]),
            solid_vertex(0.75, 6.75, [40, 120, 20, 192]),
        ];

        assert_eq!(
            render_test_solid_fan(12, 12, &vertices, &texture),
            render_test_solid_fan_reference(12, 12, &vertices, &texture)
        );
    }

    #[test]
    fn solid_fan_non_finite_scanline_uses_real_fallback() {
        let vertices = [
            solid_vertex(0.0, 0.0, [255, 255, 255, 255]),
            solid_vertex(f32::INFINITY, 0.0, [255, 255, 255, 255]),
            solid_vertex(0.0, 2.0, [255, 255, 255, 255]),
        ];
        let polygon: Vec<_> = vertices.iter().collect();
        let bounds = TriangleRasterBounds {
            min_x: 0,
            min_y: 0,
            max_x: 3,
            max_y: 3,
        };
        let scanline = polygon_scanline_span(&polygon, bounds, 0, 1.0, true);

        assert!(scanline.fell_back);
        assert_eq!(scanline.span, None);
        assert_eq!(
            polygon_fallback_scanline_span(&polygon, bounds, 0, 1.0),
            None
        );

        let mut surface = test_surface(3, 3);
        let mut stats = RasterStats::default();
        rasterize_solid_fan(
            &mut surface,
            &polygon,
            1,
            [255, 255, 255, 255],
            full_clip(3, 3),
            Some(&mut stats),
        );

        assert!(stats.solid_fan_fallback_rows > 0);
        assert_eq!(stats.solid_fan_rows, 0);
        assert_eq!(stats.solid_fan_px, 0);
        assert_eq!(stats.solid_fan_endpoint_probe_px, 0);
    }

    #[test]
    fn solid_fan_scanline_solver_keeps_endpoint_probes_bounded() {
        let texture = test_white_texture();
        let vertices = vec![
            solid_vertex(32.0, 6.0, [64, 16, 0, 128]),
            solid_vertex(48.0, 10.0, [64, 16, 0, 128]),
            solid_vertex(58.0, 26.0, [64, 16, 0, 128]),
            solid_vertex(56.0, 42.0, [64, 16, 0, 128]),
            solid_vertex(40.0, 56.0, [64, 16, 0, 128]),
            solid_vertex(22.0, 54.0, [64, 16, 0, 128]),
            solid_vertex(8.0, 38.0, [64, 16, 0, 128]),
            solid_vertex(10.0, 20.0, [64, 16, 0, 128]),
        ];
        let mut surface = test_surface(64, 64);
        let mut stats = RasterStats::default();
        let clip = full_clip(64, 64);
        let triangles = solid_fan_test_triangles(&vertices);
        let SolidTriangleColorDecision::Solid(color) = solid_triangle_color_decision(
            triangles[0][0],
            triangles[0][1],
            triangles[0][2],
            &texture,
        ) else {
            panic!("solid fan test triangle must be solid");
        };
        let mut polygon: Vec<_> = vertices[1..].iter().collect();
        polygon.push(&vertices[0]);

        rasterize_solid_fan(
            &mut surface,
            &polygon,
            triangles.len(),
            color,
            clip,
            Some(&mut stats),
        );

        assert_eq!(
            surface.pixels,
            render_test_solid_fan_reference(64, 64, &vertices, &texture)
        );
        assert_eq!(stats.solid_fan_fallback_rows, 0);
        assert!(stats.solid_fan_rows > 0);
        assert!(stats.solid_fan_edge_intersections <= polygon.len() * stats.solid_fan_rows);
        assert!(stats.solid_fan_endpoint_probe_px <= stats.solid_fan_rows * 6);
        assert!(stats.solid_fan_endpoint_probe_px < stats.solid_fan_px);
        assert_eq!(stats.translucent_px, stats.solid_fan_px);
    }

    #[test]
    fn raster_stats_rect_px_equals_clipped_rect_area() {
        let vertices = test_quad_vertices();
        let texture = test_white_texture();
        let clip = ClipBounds {
            min_x: 2,
            min_y: 2,
            max_x: 4,
            max_y: 3,
        };
        let mut surface = test_surface(5, 5);
        let mut stats = RasterStats::default();

        let accepted = rasterize_axis_aligned_solid_quad(
            &mut surface,
            [
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &vertices[1],
                &vertices[3],
                &vertices[2],
            ],
            &texture,
            clip,
            Some(&mut stats),
        );

        assert!(accepted);
        assert_eq!(stats.solid_rect_calls, 1);
        assert_eq!(stats.solid_rect_px, 2);
    }

    #[test]
    fn raster_stats_textured_rect_px_equals_clipped_rect_area() {
        let vertices = textured_quad_vertices(1.0, 1.0, 4.0, 3.0, [255, 255, 255, 255]);
        let texture = test_texture_2x2();
        let clip = ClipBounds {
            min_x: 2,
            min_y: 2,
            max_x: 4,
            max_y: 3,
        };
        let mut surface = test_surface(5, 5);
        let mut stats = RasterStats::default();

        let accepted = rasterize_axis_aligned_textured_quad(
            &mut surface,
            [
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &vertices[1],
                &vertices[3],
                &vertices[2],
            ],
            &texture,
            clip,
            Some(&mut stats),
        );

        assert!(accepted);
        assert_eq!(stats.textured_rect_calls, 1);
        assert_eq!(stats.textured_rect_px, 2);
    }

    #[test]
    fn textured_rect_stats_split_constant_and_sampled_texels() {
        let mut surface = test_surface(4, 4);
        let mut stats = RasterStats::default();
        let mut constant = textured_quad_vertices(0.0, 0.0, 2.0, 2.0, [255, 255, 255, 255]);
        for vertex in &mut constant {
            vertex.uv = egui::pos2(0.0, 0.0);
        }

        assert!(rasterize_axis_aligned_textured_quad(
            &mut surface,
            quad_triangles(&constant),
            &test_white_texture(),
            full_clip(4, 4),
            Some(&mut stats),
        ));

        let sampled = textured_quad_vertices(2.0, 0.0, 4.0, 2.0, [255, 255, 255, 255]);
        assert!(rasterize_axis_aligned_textured_quad(
            &mut surface,
            quad_triangles(&sampled),
            &test_texture_2x2(),
            full_clip(4, 4),
            Some(&mut stats),
        ));

        assert_eq!(stats.textured_rect_calls, 2);
        assert_eq!(stats.textured_rect_px, 8);
        assert_eq!(stats.textured_rect_constant_texel_calls, 1);
        assert_eq!(stats.textured_rect_constant_texel_px, 4);
        assert_eq!(stats.textured_rect_sampled_calls, 1);
        assert_eq!(stats.textured_rect_sampled_px, 4);
        assert_eq!(
            stats.textured_rect_px,
            stats.textured_rect_constant_texel_px + stats.textured_rect_sampled_px
        );
        assert_eq!(stats.textured_rect_white_texel_calls, 1);
        assert_eq!(stats.textured_rect_white_texel_px, 4);
        assert_eq!(stats.textured_rect_uniform_color_calls, 2);
        assert_eq!(stats.textured_rect_uniform_color_px, 8);
    }

    #[test]
    fn raster_stats_alpha_classes_count_emitted_source_pixels() {
        let mut surface = test_surface(4, 4);
        let mut stats = RasterStats::default();
        let opaque_rect = translucent_quad_vertices(0.0, 0.0, 2.0, 1.0, [255, 255, 255, 255]);

        assert!(rasterize_axis_aligned_solid_quad(
            &mut surface,
            quad_triangles(&opaque_rect),
            &test_white_texture(),
            full_clip(4, 4),
            Some(&mut stats),
        ));

        let translucent_triangle = [
            solid_vertex(0.0, 0.0, [128, 0, 0, 128]),
            solid_vertex(2.0, 0.0, [128, 0, 0, 128]),
            solid_vertex(0.0, 2.0, [128, 0, 0, 128]),
        ];
        rasterize_triangle(
            &mut surface,
            &translucent_triangle[0],
            &translucent_triangle[1],
            &translucent_triangle[2],
            &test_white_texture(),
            full_clip(4, 4),
            Some(&mut stats),
        );

        let textured_rect = textured_quad_vertices(0.0, 2.0, 3.0, 3.0, [255, 255, 255, 255]);
        let alpha_texture = TextureImage {
            width: 3,
            height: 1,
            pixels: vec![255, 0, 0, 0, 0, 255, 0, 128, 0, 0, 255, 255],
        };
        assert!(rasterize_axis_aligned_textured_quad(
            &mut surface,
            quad_triangles(&textured_rect),
            &alpha_texture,
            full_clip(4, 4),
            Some(&mut stats),
        ));

        assert_eq!(stats.opaque_px, 3);
        assert_eq!(stats.translucent_px, 4);
        assert_eq!(stats.transparent_px, 1);
    }

    #[test]
    fn raster_stats_triangle_bbox_covers_emitted_pixels() {
        let mut surface = test_surface(5, 5);
        let texture = test_white_texture();
        let mut stats = RasterStats::default();
        let solid = [
            test_vertex(0.0, 0.0),
            test_vertex(4.0, 0.0),
            test_vertex(0.0, 4.0),
        ];

        rasterize_triangle(
            &mut surface,
            &solid[0],
            &solid[1],
            &solid[2],
            &texture,
            full_clip(5, 5),
            Some(&mut stats),
        );

        assert_eq!(stats.solid_triangle_calls, 1);
        assert!(stats.solid_triangle_covered_px > 0);
        assert!(stats.solid_triangle_bbox_px >= stats.solid_triangle_covered_px);
        assert!(stats.solid_triangle_span_rows > 0);

        let texture = test_texture_2x2();
        let mut textured = solid;
        textured[1].uv = egui::pos2(1.0, 0.0);
        textured[2].uv = egui::pos2(0.0, 1.0);
        rasterize_triangle(
            &mut surface,
            &textured[0],
            &textured[1],
            &textured[2],
            &texture,
            full_clip(5, 5),
            Some(&mut stats),
        );

        assert_eq!(stats.textured_triangle_calls, 1);
        assert!(stats.textured_triangle_covered_px > 0);
        assert!(stats.textured_triangle_bbox_px >= stats.textured_triangle_covered_px);
        assert_eq!(stats.constant_texel_textured_triangle_calls, 0);
        assert_eq!(stats.sampled_textured_triangle_calls, 1);
        assert_eq!(
            stats.textured_triangle_candidate_px,
            stats.constant_texel_textured_triangle_candidate_px
                + stats.sampled_textured_triangle_candidate_px
        );
        assert_eq!(
            stats.textured_triangle_covered_px,
            stats.constant_texel_textured_triangle_covered_px
                + stats.sampled_textured_triangle_covered_px
        );
    }

    #[test]
    fn raster_stats_triangle_skips_match_raster_decisions() {
        let mut surface = test_surface(4, 4);
        let texture = test_white_texture();
        let mut stats = RasterStats::default();
        let degenerate = [
            test_vertex(1.0, 1.0),
            test_vertex(2.0, 2.0),
            test_vertex(2.0, 2.0),
        ];
        let clipped = [
            test_vertex(10.0, 10.0),
            test_vertex(11.0, 10.0),
            test_vertex(10.0, 11.0),
        ];

        rasterize_triangle(
            &mut surface,
            &degenerate[0],
            &degenerate[1],
            &degenerate[2],
            &texture,
            full_clip(4, 4),
            Some(&mut stats),
        );
        rasterize_triangle(
            &mut surface,
            &clipped[0],
            &clipped[1],
            &clipped[2],
            &texture,
            full_clip(4, 4),
            Some(&mut stats),
        );

        assert_eq!(stats.degenerate_triangle_skips, 1);
        assert_eq!(stats.fully_clipped_triangle_skips, 1);
        assert_eq!(stats.solid_triangle_calls, 0);
        assert_eq!(stats.textured_triangle_calls, 0);
    }

    #[test]
    fn solid_triangle_edges_cover_shared_diagonal_once() {
        let color = egui::Color32::from_rgba_premultiplied(128, 0, 0, 128);
        let mut top_left = test_vertex(0.0, 0.0);
        let mut top_right = test_vertex(3.0, 0.0);
        let mut bottom_left = test_vertex(0.0, 3.0);
        let mut bottom_right = test_vertex(3.0, 3.0);
        for vertex in [
            &mut top_left,
            &mut top_right,
            &mut bottom_left,
            &mut bottom_right,
        ] {
            vertex.color = color;
        }
        let texture = test_white_texture();
        let mut surface = test_surface(3, 3);

        rasterize_triangle(
            &mut surface,
            &top_left,
            &top_right,
            &bottom_left,
            &texture,
            full_clip(3, 3),
            None,
        );
        rasterize_triangle(
            &mut surface,
            &top_right,
            &bottom_right,
            &bottom_left,
            &texture,
            full_clip(3, 3),
            None,
        );

        assert_eq!(red_pixel_count(&surface.pixels), 9);
        assert_eq!(pixel_at(&surface.pixels, 3, 2, 0), [128, 0, 0, 255]);
        assert_eq!(pixel_at(&surface.pixels, 3, 1, 1), [128, 0, 0, 255]);
        assert_eq!(pixel_at(&surface.pixels, 3, 0, 2), [128, 0, 0, 255]);
    }

    #[test]
    fn textured_triangle_edges_cover_shared_diagonal_once() {
        let color = egui::Color32::from_rgba_premultiplied(128, 0, 0, 128);
        let mut top_left = test_vertex(0.0, 0.0);
        let mut top_right = test_vertex(3.0, 0.0);
        let mut bottom_left = test_vertex(0.0, 3.0);
        let mut bottom_right = test_vertex(3.0, 3.0);
        top_left.uv = egui::pos2(0.0, 0.0);
        top_right.uv = egui::pos2(1.0, 0.0);
        bottom_left.uv = egui::pos2(0.0, 1.0);
        bottom_right.uv = egui::pos2(1.0, 1.0);
        for vertex in [
            &mut top_left,
            &mut top_right,
            &mut bottom_left,
            &mut bottom_right,
        ] {
            vertex.color = color;
        }
        let texture = test_solid_2x2_texture([255, 255, 255, 255]);
        let mut surface = test_surface(3, 3);

        assert_eq!(
            solid_triangle_color(&top_left, &top_right, &bottom_left, &texture),
            None
        );
        rasterize_triangle(
            &mut surface,
            &top_left,
            &top_right,
            &bottom_left,
            &texture,
            full_clip(3, 3),
            None,
        );
        rasterize_triangle(
            &mut surface,
            &top_right,
            &bottom_right,
            &bottom_left,
            &texture,
            full_clip(3, 3),
            None,
        );

        assert_eq!(red_pixel_count(&surface.pixels), 9);
        assert_eq!(pixel_at(&surface.pixels, 3, 2, 0), [128, 0, 0, 255]);
        assert_eq!(pixel_at(&surface.pixels, 3, 1, 1), [128, 0, 0, 255]);
        assert_eq!(pixel_at(&surface.pixels, 3, 0, 2), [128, 0, 0, 255]);
    }

    #[test]
    fn axis_aligned_quad_fast_path_accepts_opaque_solid_rectangle() {
        let vertices = test_quad_vertices();

        let (accepted, pixels) = render_test_quad(5, 5, vertices, full_clip(5, 5));

        assert!(accepted);
        assert_eq!(white_pixel_count(&pixels), 6);
    }

    #[test]
    fn axis_aligned_quad_fast_path_clips_solid_rectangle() {
        let vertices = test_quad_vertices();

        let (accepted, pixels) = render_test_quad(
            5,
            5,
            vertices,
            ClipBounds {
                min_x: 2,
                min_y: 2,
                max_x: 4,
                max_y: 3,
            },
        );

        assert!(accepted);
        assert_eq!(white_pixel_count(&pixels), 2);
    }

    #[test]
    fn axis_aligned_quad_fast_path_rejects_non_axis_aligned_quad() {
        let mut vertices = test_quad_vertices();
        vertices[3].pos.x = 4.5;

        let (accepted, pixels) = render_test_quad(5, 5, vertices, full_clip(5, 5));

        assert!(!accepted);
        assert_eq!(white_pixel_count(&pixels), 0);
    }

    #[test]
    fn axis_aligned_quad_fast_path_rejects_non_solid_quad() {
        let texture = test_texture_2x2();
        let mut vertices = test_quad_vertices();
        vertices[3].uv = egui::pos2(1.0, 1.0);
        let mut surface = test_surface(5, 5);

        let accepted = rasterize_axis_aligned_solid_quad(
            &mut surface,
            [
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &vertices[1],
                &vertices[3],
                &vertices[2],
            ],
            &texture,
            full_clip(5, 5),
            None,
        );

        assert!(!accepted);
        assert_eq!(white_pixel_count(&surface.pixels), 0);
    }

    #[test]
    fn axis_aligned_quad_classification_matches_fast_path_shape_requirements() {
        let mut vertices = test_quad_vertices();
        assert!(is_axis_aligned_quad([
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &vertices[1],
            &vertices[3],
            &vertices[2],
        ]));

        vertices[3].pos.x = 4.5;
        assert!(!is_axis_aligned_quad([
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &vertices[1],
            &vertices[3],
            &vertices[2],
        ]));
    }

    #[test]
    fn axis_aligned_quad_fast_path_matches_generic_translucent_solid_rectangle() {
        let mut vertices = test_quad_vertices();
        for vertex in &mut vertices {
            vertex.color = egui::Color32::from_rgba_premultiplied(128, 128, 128, 128);
        }

        let (accepted, pixels) = render_test_quad(5, 5, vertices, full_clip(5, 5));
        let generic = render_test_quad_generic(5, 5, vertices, full_clip(5, 5));

        assert!(accepted);
        assert_eq!(pixels, generic);
        assert_eq!(pixel_at(&pixels, 5, 1, 1), [128, 128, 128, 255]);
        assert_eq!(pixel_at(&pixels, 5, 3, 2), [128, 128, 128, 255]);
    }

    #[test]
    fn axis_aligned_quad_fast_path_matches_generic_clipped_translucent_solid_rectangle() {
        let mut vertices = test_quad_vertices();
        for vertex in &mut vertices {
            vertex.color = egui::Color32::from_rgba_premultiplied(64, 32, 16, 128);
        }
        let clip = ClipBounds {
            min_x: 2,
            min_y: 2,
            max_x: 4,
            max_y: 3,
        };

        let (accepted, pixels) = render_test_quad(5, 5, vertices, clip);
        let generic = render_test_quad_generic(5, 5, vertices, clip);

        assert!(accepted);
        assert_eq!(pixels, generic);
        assert_eq!(pixel_at(&pixels, 5, 2, 2), [64, 32, 16, 255]);
        assert_eq!(pixel_at(&pixels, 5, 1, 2), [0, 0, 0, 255]);
    }

    #[test]
    fn axis_aligned_quad_fast_path_matches_generic_half_pixel_translucent_solid_rectangle() {
        let vertices = translucent_quad_vertices(1.5, 1.5, 4.5, 4.5, [96, 48, 24, 128]);

        let (accepted, pixels) = render_test_quad(6, 6, vertices, full_clip(6, 6));
        let generic = render_test_quad_generic(6, 6, vertices, full_clip(6, 6));

        assert!(accepted);
        assert_eq!(pixels, generic);
        assert_eq!(pixel_at(&pixels, 6, 1, 1), [0, 0, 0, 255]);
        assert_eq!(pixel_at(&pixels, 6, 4, 4), [96, 48, 24, 255]);
    }

    #[test]
    fn axis_aligned_quad_fast_path_matches_generic_fractional_clipped_translucent_solid_rectangle()
    {
        let vertices = translucent_quad_vertices(1.25, 1.5, 4.5, 4.25, [32, 80, 120, 128]);
        let clip = ClipBounds {
            min_x: 2,
            min_y: 1,
            max_x: 5,
            max_y: 4,
        };

        let (accepted, pixels) = render_test_quad(6, 6, vertices, clip);
        let generic = render_test_quad_generic(6, 6, vertices, clip);

        assert!(accepted);
        assert_eq!(pixels, generic);
        assert_eq!(pixel_at(&pixels, 6, 2, 1), [0, 0, 0, 255]);
        assert_eq!(pixel_at(&pixels, 6, 4, 2), [32, 80, 120, 255]);
    }

    #[test]
    fn textured_quad_fast_path_matches_generic_atlas_rectangle() {
        let vertices = textured_quad_vertices(1.0, 1.0, 5.0, 4.0, [192, 96, 48, 128]);
        let texture = test_texture_4x4();

        let (accepted, pixels) =
            render_test_textured_quad(7, 6, vertices, &texture, full_clip(7, 6));
        let generic = render_test_textured_quad_generic(7, 6, vertices, &texture, full_clip(7, 6));

        assert!(accepted);
        assert_eq!(pixels, generic);
    }

    #[test]
    fn textured_quad_fast_path_matches_generic_with_white_vertex_color() {
        let vertices = textured_quad_vertices(1.0, 1.0, 5.0, 4.0, [255, 255, 255, 255]);
        let texture = test_texture_4x4();

        let (accepted, pixels) =
            render_test_textured_quad(7, 6, vertices, &texture, full_clip(7, 6));
        let generic = render_test_textured_quad_generic(7, 6, vertices, &texture, full_clip(7, 6));

        assert!(accepted);
        assert_eq!(pixels, generic);
    }

    #[test]
    fn textured_quad_fast_path_matches_generic_at_nearest_half_threshold() {
        let vertices = textured_quad_vertices(1.0, 1.0, 24.0, 2.0, [255, 255, 255, 255]);
        let texture = test_alpha_texture_2x1();

        assert_eq!(nearest_texel(&texture, egui::pos2(0.5, 0.0)), (1, 0));

        let (accepted, pixels) =
            render_test_textured_quad(26, 4, vertices, &texture, full_clip(26, 4));
        let generic =
            render_test_textured_quad_generic(26, 4, vertices, &texture, full_clip(26, 4));

        assert!(accepted);
        assert_eq!(pixels, generic);
    }

    #[test]
    fn textured_quad_fast_path_matches_generic_clipped_atlas_rectangle() {
        let vertices = textured_quad_vertices(1.25, 1.5, 5.25, 4.5, [128, 128, 64, 160]);
        let texture = test_texture_4x4();
        let clip = ClipBounds {
            min_x: 2,
            min_y: 2,
            max_x: 5,
            max_y: 4,
        };

        let (accepted, pixels) = render_test_textured_quad(7, 6, vertices, &texture, clip);
        let generic = render_test_textured_quad_generic(7, 6, vertices, &texture, clip);

        assert!(accepted);
        assert_eq!(pixels, generic);
    }

    #[test]
    fn textured_rect_stats_match_clipped_nearest_half_threshold_alpha_samples() {
        let vertices = textured_quad_vertices(1.0, 1.0, 24.0, 2.0, [255, 255, 255, 255]);
        let texture = test_alpha_texture_2x1();
        let clip = ClipBounds {
            min_x: 10,
            min_y: 1,
            max_x: 15,
            max_y: 2,
        };
        let mut surface = test_surface(26, 4);
        let mut stats = RasterStats::default();

        let accepted = rasterize_axis_aligned_textured_quad(
            &mut surface,
            quad_triangles(&vertices),
            &texture,
            clip,
            Some(&mut stats),
        );
        let generic = render_test_textured_quad_generic(26, 4, vertices, &texture, clip);

        assert!(accepted);
        assert_eq!(surface.pixels, generic);
        assert_eq!(stats.textured_rect_calls, 1);
        assert_eq!(stats.textured_rect_px, 5);
        assert_eq!(stats.textured_rect_sampled_calls, 1);
        assert_eq!(stats.textured_rect_sampled_px, 5);
        assert_eq!(stats.transparent_px, 2);
        assert_eq!(stats.opaque_px, 3);
        assert_eq!(stats.translucent_px, 0);
    }

    #[test]
    fn textured_quad_fast_path_accepts_rounded_affine_uv() {
        let mut vertices = textured_quad_vertices(1.0, 1.0, 5.0, 4.0, [192, 96, 48, 128]);
        vertices[0].uv = egui::pos2(0.02, 0.02);
        vertices[1].uv = egui::pos2(0.10, 0.02);
        vertices[2].uv = egui::pos2(0.02, 0.10);
        vertices[3].uv = egui::pos2(
            0.10 + UV_AFFINE_EPSILON * 0.5,
            0.10 - UV_AFFINE_EPSILON * 0.5,
        );
        let texture = test_texture_4x4();

        let (accepted, pixels) =
            render_test_textured_quad(7, 6, vertices, &texture, full_clip(7, 6));
        let generic = render_test_textured_quad_generic(7, 6, vertices, &texture, full_clip(7, 6));

        assert!(accepted);
        assert_eq!(pixels, generic);
    }

    #[test]
    fn textured_quad_fast_path_rejects_non_affine_uv() {
        let mut vertices = textured_quad_vertices(1.0, 1.0, 5.0, 4.0, [255, 255, 255, 255]);
        vertices[3].uv = egui::pos2(0.75, 0.5);
        let texture = test_texture_4x4();

        let (accepted, pixels) =
            render_test_textured_quad(7, 6, vertices, &texture, full_clip(7, 6));

        assert!(!accepted);
        assert_eq!(white_pixel_count(&pixels), 0);
        assert_eq!(
            textured_quad_fast_path_rejection(quad_vertex_refs(&vertices)),
            Some(TexturedQuadFastPathRejection::NonAffineUv)
        );
    }

    #[test]
    fn textured_quad_fast_path_rejects_non_uniform_vertex_color() {
        let mut vertices = textured_quad_vertices(1.0, 1.0, 5.0, 4.0, [255, 255, 255, 255]);
        vertices[2].color = egui::Color32::from_rgba_premultiplied(128, 128, 128, 255);
        let texture = test_texture_4x4();

        let (accepted, pixels) =
            render_test_textured_quad(7, 6, vertices, &texture, full_clip(7, 6));

        assert!(!accepted);
        assert_eq!(white_pixel_count(&pixels), 0);
        assert_eq!(
            textured_quad_fast_path_rejection(quad_vertex_refs(&vertices)),
            Some(TexturedQuadFastPathRejection::NonUniformColor)
        );
    }

    #[test]
    fn textured_quad_fast_path_rejection_accepts_eligible_quad() {
        let vertices = textured_quad_vertices(1.0, 1.0, 5.0, 4.0, [255, 255, 255, 255]);

        assert_eq!(
            textured_quad_fast_path_rejection(quad_vertex_refs(&vertices)),
            None
        );
    }

    fn quad_vertex_refs(vertices: &[egui::epaint::Vertex; 4]) -> [&egui::epaint::Vertex; 6] {
        [
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &vertices[1],
            &vertices[3],
            &vertices[2],
        ]
    }

    fn render_test_triangle(
        width: usize,
        height: usize,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
    ) -> Vec<u8> {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };

        render_test_triangle_with(width, height, v0, v1, v2, &texture)
    }

    fn render_test_triangle_with(
        width: usize,
        height: usize,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        texture: &TextureImage,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);

        rasterize_triangle(
            &mut surface,
            v0,
            v1,
            v2,
            texture,
            ClipBounds {
                min_x: 0,
                min_y: 0,
                max_x: width,
                max_y: height,
            },
            None,
        );

        surface.pixels
    }

    fn assert_solid_triangle_matches_reference(
        width: usize,
        height: usize,
        clip: ClipBounds,
        background: [u8; 4],
        vertices: [egui::epaint::Vertex; 3],
    ) {
        let texture = test_white_texture();
        let color = solid_triangle_color(&vertices[0], &vertices[1], &vertices[2], &texture)
            .expect("solid triangle");
        let mut routed = test_surface_with_background(width, height, background);
        let mut direct = test_surface_with_background(width, height, background);
        let mut reference = test_surface_with_background(width, height, background);

        rasterize_triangle(
            &mut routed,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            clip,
            None,
        );
        if let Some(bounds) = triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], clip)
        {
            rasterize_solid_triangle(
                &mut direct,
                TriangleVertices {
                    v0: &vertices[0],
                    v1: &vertices[1],
                    v2: &vertices[2],
                },
                bounds,
                edge(vertices[0].pos, vertices[1].pos, vertices[2].pos),
                color,
                None,
            );
        }
        reference_rasterize_solid_triangle(
            &mut reference,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            clip,
            color,
        );

        assert_eq!(routed.pixels, reference.pixels);
        assert_eq!(direct.pixels, reference.pixels);
    }

    fn assert_textured_triangle_matches_reference(
        width: usize,
        height: usize,
        clip: ClipBounds,
        background: [u8; 4],
        vertices: [egui::epaint::Vertex; 3],
        texture: &TextureImage,
    ) {
        assert_eq!(
            classify_triangle(&vertices[0], &vertices[1], &vertices[2], texture),
            TriangleClassification::Textured
        );
        let mut routed = test_surface_with_background(width, height, background);
        let mut direct = test_surface_with_background(width, height, background);
        let mut reference = test_surface_with_background(width, height, background);

        rasterize_triangle(
            &mut routed,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            texture,
            clip,
            None,
        );
        if let Some(bounds) = triangle_raster_bounds(&vertices[0], &vertices[1], &vertices[2], clip)
        {
            rasterize_textured_triangle(
                &mut direct,
                TriangleVertices {
                    v0: &vertices[0],
                    v1: &vertices[1],
                    v2: &vertices[2],
                },
                texture,
                bounds,
                edge(vertices[0].pos, vertices[1].pos, vertices[2].pos),
                None,
            );
        }
        reference_rasterize_textured_triangle(
            &mut reference,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            clip,
            texture,
        );

        assert_eq!(routed.pixels, reference.pixels);
        assert_eq!(direct.pixels, reference.pixels);
    }

    fn reference_rasterize_solid_triangle(
        surface: &mut SoftwareSurface,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        clip: ClipBounds,
        color: [u8; 4],
    ) {
        let area = edge(v0.pos, v1.pos, v2.pos);
        if area.abs() <= f32::EPSILON {
            return;
        }
        let Some(bounds) = triangle_raster_bounds(v0, v1, v2, clip) else {
            return;
        };
        let inv_area = 1.0 / area;
        let edge0_includes_boundary = edge_includes_boundary(v1.pos, v2.pos, area);
        let edge1_includes_boundary = edge_includes_boundary(v2.pos, v0.pos, area);
        let edge2_includes_boundary = edge_includes_boundary(v0.pos, v1.pos, area);

        for y in bounds.min_y..bounds.max_y {
            for x in bounds.min_x..bounds.max_x {
                let pixel_center = egui::pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
                let w0 = edge(v1.pos, v2.pos, pixel_center) * inv_area;
                let w1 = edge(v2.pos, v0.pos, pixel_center) * inv_area;
                let w2 = edge(v0.pos, v1.pos, pixel_center) * inv_area;
                if edge_covers_pixel(w0, edge0_includes_boundary)
                    && edge_covers_pixel(w1, edge1_includes_boundary)
                    && edge_covers_pixel(w2, edge2_includes_boundary)
                {
                    surface.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn reference_rasterize_textured_triangle(
        surface: &mut SoftwareSurface,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        clip: ClipBounds,
        texture: &TextureImage,
    ) {
        let area = edge(v0.pos, v1.pos, v2.pos);
        if area.abs() <= f32::EPSILON {
            return;
        }
        let Some(bounds) = triangle_raster_bounds(v0, v1, v2, clip) else {
            return;
        };
        let inv_area = 1.0 / area;
        let edge0_includes_boundary = edge_includes_boundary(v1.pos, v2.pos, area);
        let edge1_includes_boundary = edge_includes_boundary(v2.pos, v0.pos, area);
        let edge2_includes_boundary = edge_includes_boundary(v0.pos, v1.pos, area);

        for y in bounds.min_y..bounds.max_y {
            for x in bounds.min_x..bounds.max_x {
                let pixel_center = egui::pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
                let w0 = edge(v1.pos, v2.pos, pixel_center) * inv_area;
                let w1 = edge(v2.pos, v0.pos, pixel_center) * inv_area;
                let w2 = edge(v0.pos, v1.pos, pixel_center) * inv_area;
                if edge_covers_pixel(w0, edge0_includes_boundary)
                    && edge_covers_pixel(w1, edge1_includes_boundary)
                    && edge_covers_pixel(w2, edge2_includes_boundary)
                {
                    let color = textured_triangle_pixel_color(v0, v1, v2, texture, w0, w1, w2);
                    surface.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn render_test_quad(
        width: usize,
        height: usize,
        vertices: [egui::epaint::Vertex; 4],
        clip: ClipBounds,
    ) -> (bool, Vec<u8>) {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let mut surface = test_surface(width, height);

        let accepted = rasterize_axis_aligned_solid_quad(
            &mut surface,
            [
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &vertices[1],
                &vertices[3],
                &vertices[2],
            ],
            &texture,
            clip,
            None,
        );

        (accepted, surface.pixels)
    }

    fn quad_triangles(vertices: &[egui::epaint::Vertex; 4]) -> [&egui::epaint::Vertex; 6] {
        [
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &vertices[1],
            &vertices[3],
            &vertices[2],
        ]
    }

    fn render_test_quad_generic(
        width: usize,
        height: usize,
        vertices: [egui::epaint::Vertex; 4],
        clip: ClipBounds,
    ) -> Vec<u8> {
        let texture = test_white_texture();
        let mut surface = test_surface(width, height);

        rasterize_test_textured_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            &texture,
            clip,
        );
        rasterize_test_textured_triangle(
            &mut surface,
            &vertices[1],
            &vertices[3],
            &vertices[2],
            &texture,
            clip,
        );

        surface.pixels
    }

    fn render_test_textured_quad(
        width: usize,
        height: usize,
        vertices: [egui::epaint::Vertex; 4],
        texture: &TextureImage,
        clip: ClipBounds,
    ) -> (bool, Vec<u8>) {
        let mut surface = test_surface(width, height);

        let accepted = rasterize_axis_aligned_textured_quad(
            &mut surface,
            [
                &vertices[0],
                &vertices[1],
                &vertices[2],
                &vertices[1],
                &vertices[3],
                &vertices[2],
            ],
            texture,
            clip,
            None,
        );

        (accepted, surface.pixels)
    }

    fn render_test_textured_quad_generic(
        width: usize,
        height: usize,
        vertices: [egui::epaint::Vertex; 4],
        texture: &TextureImage,
        clip: ClipBounds,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);

        rasterize_test_textured_triangle(
            &mut surface,
            &vertices[0],
            &vertices[1],
            &vertices[2],
            texture,
            clip,
        );
        rasterize_test_textured_triangle(
            &mut surface,
            &vertices[1],
            &vertices[3],
            &vertices[2],
            texture,
            clip,
        );

        surface.pixels
    }

    const fn full_clip(width: usize, height: usize) -> ClipBounds {
        ClipBounds {
            min_x: 0,
            min_y: 0,
            max_x: width,
            max_y: height,
        }
    }

    fn render_test_triangle_generic(
        width: usize,
        height: usize,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        texture: &TextureImage,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);
        rasterize_test_textured_triangle(
            &mut surface,
            v0,
            v1,
            v2,
            texture,
            full_clip(width, height),
        );

        surface.pixels
    }

    fn rasterize_test_textured_triangle(
        surface: &mut SoftwareSurface,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        texture: &TextureImage,
        clip: ClipBounds,
    ) {
        rasterize_textured_triangle(
            surface,
            TriangleVertices { v0, v1, v2 },
            texture,
            triangle_raster_bounds(v0, v1, v2, clip).expect("triangle bounds"),
            edge(v0.pos, v1.pos, v2.pos),
            None,
        );
    }

    fn test_surface(width: usize, height: usize) -> SoftwareSurface {
        test_surface_with_background(width, height, [0, 0, 0, 255])
    }

    fn test_surface_with_background(
        width: usize,
        height: usize,
        background: [u8; 4],
    ) -> SoftwareSurface {
        let mut surface = SoftwareSurface::default();
        surface.resize(width, height).expect("surface");
        surface.clear(background);
        surface
    }

    fn white_pixel_count(pixels: &[u8]) -> usize {
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255)
            .count()
    }

    fn red_pixel_count(pixels: &[u8]) -> usize {
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] == 128 && pixel[1] == 0 && pixel[2] == 0)
            .count()
    }

    fn pixel_at(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let offset = (y * width + x) * 4;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    }

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            color: egui::Color32::WHITE,
            uv: egui::Pos2::ZERO,
        }
    }

    fn solid_vertex(x: f32, y: f32, color: [u8; 4]) -> egui::epaint::Vertex {
        let mut vertex = test_vertex(x, y);
        vertex.color =
            egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
        vertex
    }

    fn test_quad_vertices() -> [egui::epaint::Vertex; 4] {
        [
            test_vertex(1.0, 1.0),
            test_vertex(4.0, 1.0),
            test_vertex(1.0, 3.0),
            test_vertex(4.0, 3.0),
        ]
    }

    fn translucent_quad_vertices(
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        color: [u8; 4],
    ) -> [egui::epaint::Vertex; 4] {
        let mut vertices = [
            test_vertex(min_x, min_y),
            test_vertex(max_x, min_y),
            test_vertex(min_x, max_y),
            test_vertex(max_x, max_y),
        ];
        for vertex in &mut vertices {
            vertex.color =
                egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
        }
        vertices
    }

    fn textured_quad_vertices(
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        color: [u8; 4],
    ) -> [egui::epaint::Vertex; 4] {
        let mut vertices = translucent_quad_vertices(min_x, min_y, max_x, max_y, color);
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(1.0, 0.0);
        vertices[2].uv = egui::pos2(0.0, 1.0);
        vertices[3].uv = egui::pos2(1.0, 1.0);
        vertices
    }

    fn solid_fan_vertices(color: [u8; 4]) -> Vec<egui::epaint::Vertex> {
        vec![
            solid_vertex(1.0, 5.0, color),
            solid_vertex(2.0, 1.0, color),
            solid_vertex(6.0, 1.0, color),
            solid_vertex(9.0, 3.0, color),
            solid_vertex(8.0, 8.0, color),
            solid_vertex(3.0, 9.0, color),
        ]
    }

    fn render_test_solid_fan(
        width: usize,
        height: usize,
        vertices: &[egui::epaint::Vertex],
        texture: &TextureImage,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);
        let clip = ClipBounds {
            min_x: 0,
            min_y: 0,
            max_x: width,
            max_y: height,
        };
        let triangles = solid_fan_test_triangles(vertices);
        let SolidTriangleColorDecision::Solid(color) = solid_triangle_color_decision(
            triangles[0][0],
            triangles[0][1],
            triangles[0][2],
            texture,
        ) else {
            panic!("solid fan test triangle must be solid");
        };
        let mut polygon: Vec<_> = vertices[1..].iter().collect();
        polygon.push(&vertices[0]);
        rasterize_solid_fan(&mut surface, &polygon, triangles.len(), color, clip, None);
        surface.pixels
    }

    fn render_test_solid_fan_reference(
        width: usize,
        height: usize,
        vertices: &[egui::epaint::Vertex],
        texture: &TextureImage,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);
        let clip = ClipBounds {
            min_x: 0,
            min_y: 0,
            max_x: width,
            max_y: height,
        };
        for [v0, v1, v2] in solid_fan_test_triangles(vertices) {
            rasterize_triangle(&mut surface, v0, v1, v2, texture, clip, None);
        }
        surface.pixels
    }

    fn solid_fan_test_triangles(
        vertices: &[egui::epaint::Vertex],
    ) -> Vec<[&egui::epaint::Vertex; 3]> {
        (1..vertices.len() - 1)
            .map(|index| [&vertices[index], &vertices[0], &vertices[index + 1]])
            .collect()
    }

    fn test_texture_2x2() -> TextureImage {
        TextureImage {
            width: 2,
            height: 2,
            pixels: vec![
                64, 128, 192, 255, // top-left
                255, 0, 0, 255, // top-right
                0, 255, 0, 255, // bottom-left
                0, 0, 255, 255, // bottom-right
            ],
        }
    }

    fn test_texture_4x4() -> TextureImage {
        TextureImage {
            width: 4,
            height: 4,
            pixels: vec![
                20, 10, 0, 255, 60, 10, 0, 255, 100, 10, 0, 255, 140, 10, 0, 255, 20, 50, 0, 255,
                60, 50, 0, 255, 100, 50, 0, 255, 140, 50, 0, 255, 20, 90, 0, 255, 60, 90, 0, 255,
                100, 90, 0, 255, 140, 90, 0, 255, 20, 130, 0, 255, 60, 130, 0, 255, 100, 130, 0,
                255, 140, 130, 0, 255,
            ],
        }
    }

    fn test_alpha_texture_2x1() -> TextureImage {
        TextureImage {
            width: 2,
            height: 1,
            pixels: vec![255, 0, 0, 0, 0, 255, 0, 255],
        }
    }

    fn test_white_texture() -> TextureImage {
        TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        }
    }

    fn test_solid_2x2_texture(color: [u8; 4]) -> TextureImage {
        TextureImage {
            width: 2,
            height: 2,
            pixels: color.repeat(4),
        }
    }
}
