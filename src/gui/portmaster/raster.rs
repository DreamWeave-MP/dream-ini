// SPDX-License-Identifier: GPL-3.0-only

use std::cmp::Ordering;
use std::io;

use super::surface::SoftwareSurface;
use super::texture::TextureImage;

const UV_AFFINE_EPSILON: f32 = 1.0 / 1_048_576.0;

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RasterStats {
    pub(super) solid_rect_calls: usize,
    pub(super) solid_rect_px: usize,
    pub(super) textured_rect_calls: usize,
    pub(super) textured_rect_px: usize,
    pub(super) solid_triangle_calls: usize,
    pub(super) solid_triangle_bbox_px: usize,
    pub(super) solid_triangle_covered_px: usize,
    pub(super) solid_triangle_span_rows: usize,
    pub(super) textured_triangle_calls: usize,
    pub(super) textured_triangle_bbox_px: usize,
    pub(super) textured_triangle_covered_px: usize,
    pub(super) degenerate_triangle_skips: usize,
    pub(super) fully_clipped_triangle_skips: usize,
    pub(super) opaque_px: usize,
    pub(super) translucent_px: usize,
    pub(super) transparent_px: usize,
}

impl RasterStats {
    fn record_alpha_px(&mut self, alpha: u8, count: usize) {
        match alpha {
            0 => self.transparent_px += count,
            u8::MAX => self.opaque_px += count,
            _ => self.translucent_px += count,
        }
    }

    pub(super) fn log_line(&self) -> String {
        format!(
            "software renderer raster_stats solid_rect_calls={} solid_rect_px={} textured_rect_calls={} textured_rect_px={} solid_triangle_calls={} solid_triangle_bbox_px={} solid_triangle_covered_px={} solid_triangle_span_rows={} textured_triangle_calls={} textured_triangle_bbox_px={} textured_triangle_covered_px={} degenerate_triangle_skips={} fully_clipped_triangle_skips={} opaque_px={} translucent_px={} transparent_px={}",
            self.solid_rect_calls,
            self.solid_rect_px,
            self.textured_rect_calls,
            self.textured_rect_px,
            self.solid_triangle_calls,
            self.solid_triangle_bbox_px,
            self.solid_triangle_covered_px,
            self.solid_triangle_span_rows,
            self.textured_triangle_calls,
            self.textured_triangle_bbox_px,
            self.textured_triangle_covered_px,
            self.degenerate_triangle_skips,
            self.fully_clipped_triangle_skips,
            self.opaque_px,
            self.translucent_px,
            self.transparent_px,
        )
    }
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

    if let Some(color) = solid_triangle_color(v0, v1, v2, texture) {
        if let Some(stats) = stats.as_deref_mut() {
            stats.solid_triangle_calls += 1;
            stats.solid_triangle_bbox_px += bounds.pixel_area();
        }
        rasterize_solid_triangle(surface, v0, v1, v2, bounds, area, color, stats);
        return;
    }

    if let Some(stats) = stats.as_deref_mut() {
        stats.textured_triangle_calls += 1;
        stats.textured_triangle_bbox_px += bounds.pixel_area();
    }
    rasterize_textured_triangle(surface, v0, v1, v2, texture, bounds, area, stats);
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
    rasterize_solid_rect(
        surface,
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y,
        clip,
        color,
        stats,
    );
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
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    clip: ClipBounds,
    color: [u8; 4],
    mut stats: Option<&mut RasterStats>,
) {
    let start_x = solid_rect_boundary_index(min_x, clip.max_x).max(clip.min_x);
    let end_x = solid_rect_boundary_index(max_x, clip.max_x).min(clip.max_x);
    let start_y = solid_rect_boundary_index(min_y, clip.max_y).max(clip.min_y);
    let end_y = solid_rect_boundary_index(max_y, clip.max_y).min(clip.max_y);
    if start_x >= end_x || start_y >= end_y {
        return;
    }
    if let Some(stats) = stats.as_deref_mut() {
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
    if let Some(stats) = stats.as_deref_mut() {
        stats.textured_rect_calls += 1;
        stats.textured_rect_px += (end_x - start_x) * (end_y - start_y);
    }

    let inv_width = 1.0 / (max_x - min_x);
    let inv_height = 1.0 / (max_y - min_y);
    let vertex_color = color_to_array(corners.tl.color);
    for y in start_y..end_y {
        let sy = (usize_to_f32(y) + 0.5 - min_y) * inv_height;
        for x in start_x..end_x {
            let sx = (usize_to_f32(x) + 0.5 - min_x) * inv_width;
            let tl_weight = 1.0 - sx - sy;
            let uv = egui::pos2(
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
            );
            let color = modulate_color(vertex_color, sample_nearest(texture, uv));
            if let Some(stats) = stats.as_deref_mut() {
                stats.record_alpha_px(color[3], 1);
            }
            surface.blend_pixel(x, y, color);
        }
    }
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
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    bounds: TriangleRasterBounds,
    area: f32,
    color: [u8; 4],
    mut stats: Option<&mut RasterStats>,
) {
    let inv_area = 1.0 / area;
    let start = egui::pos2(
        usize_to_f32(bounds.min_x) + 0.5,
        usize_to_f32(bounds.min_y) + 0.5,
    );
    let w0_step_x = edge_step_x(v1.pos, v2.pos);
    let w1_step_x = edge_step_x(v2.pos, v0.pos);
    let w2_step_x = edge_step_x(v0.pos, v1.pos);
    let w0_step_y = edge_step_y(v1.pos, v2.pos);
    let w1_step_y = edge_step_y(v2.pos, v0.pos);
    let w2_step_y = edge_step_y(v0.pos, v1.pos);
    let mut row_edge0 = edge(v1.pos, v2.pos, start);
    let mut row_edge1 = edge(v2.pos, v0.pos, start);
    let mut row_edge2 = edge(v0.pos, v1.pos, start);
    let edge0_includes_boundary = edge_includes_boundary(v1.pos, v2.pos, area);
    let edge1_includes_boundary = edge_includes_boundary(v2.pos, v0.pos, area);
    let edge2_includes_boundary = edge_includes_boundary(v0.pos, v1.pos, area);

    for y in bounds.min_y..bounds.max_y {
        let mut pixel_edge0 = row_edge0;
        let mut pixel_edge1 = row_edge1;
        let mut pixel_edge2 = row_edge2;
        let mut span_start = None;
        let mut span_end = bounds.min_x;
        for x in bounds.min_x..bounds.max_x {
            let w0 = pixel_edge0 * inv_area;
            let w1 = pixel_edge1 * inv_area;
            let w2 = pixel_edge2 * inv_area;
            if edge_covers_pixel(w0, edge0_includes_boundary)
                && edge_covers_pixel(w1, edge1_includes_boundary)
                && edge_covers_pixel(w2, edge2_includes_boundary)
            {
                if span_start.is_none() {
                    span_start = Some(x);
                }
                span_end = x + 1;
            } else if span_start.is_some() {
                break;
            }
            pixel_edge0 += w0_step_x;
            pixel_edge1 += w1_step_x;
            pixel_edge2 += w2_step_x;
        }
        if let Some(start_x) = span_start {
            surface.blend_span(y, start_x, span_end, color);
            if let Some(stats) = stats.as_deref_mut() {
                let px = span_end - start_x;
                stats.solid_triangle_covered_px += px;
                stats.solid_triangle_span_rows += 1;
                stats.record_alpha_px(color[3], px);
            }
        }
        row_edge0 += w0_step_y;
        row_edge1 += w1_step_y;
        row_edge2 += w2_step_y;
    }
}

fn rasterize_textured_triangle(
    surface: &mut SoftwareSurface,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
    bounds: TriangleRasterBounds,
    area: f32,
    mut stats: Option<&mut RasterStats>,
) {
    let inv_area = 1.0 / area;
    let start = egui::pos2(
        usize_to_f32(bounds.min_x) + 0.5,
        usize_to_f32(bounds.min_y) + 0.5,
    );
    let w0_step_x = edge_step_x(v1.pos, v2.pos);
    let w1_step_x = edge_step_x(v2.pos, v0.pos);
    let w2_step_x = edge_step_x(v0.pos, v1.pos);
    let w0_step_y = edge_step_y(v1.pos, v2.pos);
    let w1_step_y = edge_step_y(v2.pos, v0.pos);
    let w2_step_y = edge_step_y(v0.pos, v1.pos);
    let mut row_edge0 = edge(v1.pos, v2.pos, start);
    let mut row_edge1 = edge(v2.pos, v0.pos, start);
    let mut row_edge2 = edge(v0.pos, v1.pos, start);
    let edge0_includes_boundary = edge_includes_boundary(v1.pos, v2.pos, area);
    let edge1_includes_boundary = edge_includes_boundary(v2.pos, v0.pos, area);
    let edge2_includes_boundary = edge_includes_boundary(v0.pos, v1.pos, area);

    for y in bounds.min_y..bounds.max_y {
        let mut pixel_edge0 = row_edge0;
        let mut pixel_edge1 = row_edge1;
        let mut pixel_edge2 = row_edge2;
        for x in bounds.min_x..bounds.max_x {
            let w0 = pixel_edge0 * inv_area;
            let w1 = pixel_edge1 * inv_area;
            let w2 = pixel_edge2 * inv_area;
            if edge_covers_pixel(w0, edge0_includes_boundary)
                && edge_covers_pixel(w1, edge1_includes_boundary)
                && edge_covers_pixel(w2, edge2_includes_boundary)
            {
                let uv = egui::pos2(
                    v0.uv.x.mul_add(w0, v1.uv.x.mul_add(w1, v2.uv.x * w2)),
                    v0.uv.y.mul_add(w0, v1.uv.y.mul_add(w1, v2.uv.y * w2)),
                );
                let vertex_color = interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2);
                let texture_color = sample_nearest(texture, uv);
                let color = modulate_color(vertex_color, texture_color);
                surface.blend_pixel(x, y, color);
                if let Some(stats) = stats.as_deref_mut() {
                    stats.textured_triangle_covered_px += 1;
                    stats.record_alpha_px(color[3], 1);
                }
            }
            pixel_edge0 += w0_step_x;
            pixel_edge1 += w1_step_x;
            pixel_edge2 += w2_step_x;
        }
        row_edge0 += w0_step_y;
        row_edge1 += w1_step_y;
        row_edge2 += w2_step_y;
    }
}

fn solid_triangle_color(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> Option<[u8; 4]> {
    if v0.color != v1.color || v0.color != v2.color {
        return None;
    }

    if texture.width == 0 || texture.height == 0 {
        return Some(modulate_color(
            color_to_array(v0.color),
            [255, 255, 255, 255],
        ));
    }

    let t0 = nearest_texel(texture, v0.uv);
    if t0 != nearest_texel(texture, v1.uv) || t0 != nearest_texel(texture, v2.uv) {
        return None;
    }

    Some(modulate_color(
        color_to_array(v0.color),
        texel_color(texture, t0),
    ))
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
    let value = f32::from(c0).mul_add(w0, f32::from(c1).mul_add(w1, f32::from(c2) * w2));
    f32_to_u8_round_clamped(value)
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
            solid_triangle_calls: 5,
            solid_triangle_bbox_px: 6,
            solid_triangle_covered_px: 7,
            solid_triangle_span_rows: 8,
            textured_triangle_calls: 9,
            textured_triangle_bbox_px: 10,
            textured_triangle_covered_px: 11,
            degenerate_triangle_skips: 12,
            fully_clipped_triangle_skips: 13,
            opaque_px: 14,
            translucent_px: 15,
            transparent_px: 16,
        };

        assert_eq!(
            stats.log_line(),
            "software renderer raster_stats solid_rect_calls=1 solid_rect_px=2 textured_rect_calls=3 textured_rect_px=4 solid_triangle_calls=5 solid_triangle_bbox_px=6 solid_triangle_covered_px=7 solid_triangle_span_rows=8 textured_triangle_calls=9 textured_triangle_bbox_px=10 textured_triangle_covered_px=11 degenerate_triangle_skips=12 fully_clipped_triangle_skips=13 opaque_px=14 translucent_px=15 transparent_px=16"
        );
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
                &vertices[0],
                &vertices[1],
                &vertices[2],
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
            v0,
            v1,
            v2,
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
