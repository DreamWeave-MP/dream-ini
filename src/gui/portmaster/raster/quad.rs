// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use super::math::{
    color_to_array, edge, f32_to_usize_floor_clamped, modulate_color, near_finite_pos2, same_f32,
    same_pos2, usize_to_f32,
};
use super::sampling::{nearest_texel, sample_nearest, texel_color};
use super::solid::solid_triangle_color;
use super::types::{ClipBounds, TexturedQuadFastPathRejection};
use super::{RasterStats, UV_AFFINE_EPSILON, duration_as_us};
use crate::gui::portmaster::{surface::SoftwareSurface, texture::TextureImage};

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
}

pub(in crate::gui::portmaster) fn rasterize_axis_aligned_solid_quad(
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

pub(in crate::gui::portmaster) fn rasterize_axis_aligned_textured_quad(
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

pub(in crate::gui::portmaster) fn textured_quad_fast_path_rejection(
    vertices: [&egui::epaint::Vertex; 6],
) -> Option<TexturedQuadFastPathRejection> {
    textured_quad_fast_path_candidate(vertices).err()
}

pub(in crate::gui::portmaster) fn is_axis_aligned_quad(
    vertices: [&egui::epaint::Vertex; 6],
) -> bool {
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
        let mut pixel_offset = surface.row_offset(y) + range.start_x * 4;
        for x in range.start_x..range.end_x {
            let color = pixel_color(texture, row.uv_at(x, range.start_x));
            match color[3] {
                0 => {}
                u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
            }
            pixel_offset += 4;
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
        let mut pixel_offset = surface.row_offset(y) + range.start_x * 4;
        for x in range.start_x..range.end_x {
            let color = pixel_color(texture, row.uv_at(x, range.start_x));
            stats.record_alpha_px(color[3], 1);
            match color[3] {
                0 => {}
                u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
            }
            pixel_offset += 4;
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
