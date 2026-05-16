// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use super::math::{
    color_to_array, edge, f32_to_usize_floor_clamped, f32_to_usize_round_clamped, modulate_color,
    near_finite_pos2, same_f32, same_pos2, usize_to_f32,
};
use super::sampling::{nearest_texel, sample_nearest, texel_color};
use super::solid::solid_triangle_color;
use super::types::{ClipBounds, TexturedQuadFastPathRejection};
use super::{RasterStats, UV_AFFINE_EPSILON, duration_as_us};
use crate::gui::portmaster::{surface::SoftwareSurface, texture::TextureImage};

const CLEAR_ELISION_NEAR_FULL_SURFACE_BASIS_POINTS: usize = 9_500;
const TEXTURED_RECT_VECTOR_BLOCK_PX: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) enum ClearElisionQuadRejection {
    NotRect,
    ClipNotFullSurface,
    AlphaNotOpaque,
    TextureNotConstantWhite,
    NotFullSurfaceOpaqueRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) struct ClearElisionQuadEvidence {
    pub(in crate::gui::portmaster) visible_px: usize,
    pub(in crate::gui::portmaster) cover_basis_points: usize,
    pub(in crate::gui::portmaster) opaque_rect: bool,
    pub(in crate::gui::portmaster) full_surface_opaque_rect: bool,
    pub(in crate::gui::portmaster) near_full_surface_opaque_rect: bool,
    pub(in crate::gui::portmaster) rejection: Option<ClearElisionQuadRejection>,
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
                if classification.separable_uv {
                    self.textured_rect_separable_uv_calls += 1;
                    self.textured_rect_separable_uv_px += px;
                } else {
                    self.textured_rect_nonseparable_uv_calls += 1;
                    self.textured_rect_nonseparable_uv_px += px;
                }
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

pub(in crate::gui::portmaster) fn clear_elision_quad_evidence(
    vertices: [&egui::epaint::Vertex; 6],
    texture: &TextureImage,
    clip: ClipBounds,
    surface_width: usize,
    surface_height: usize,
) -> ClearElisionQuadEvidence {
    let Ok(candidate) = textured_quad_fast_path_candidate(vertices) else {
        return ClearElisionQuadEvidence::rejected(ClearElisionQuadRejection::NotRect);
    };
    let visible_px = clipped_quad_pixel_area(candidate.bounds, clip);
    let cover_basis_points = surface_cover_basis_points(visible_px, surface_width, surface_height);
    let texture_color = textured_rect_constant_texel_color(candidate.corners, texture);
    let opaque_white_rect =
        candidate.corners.tl.color.a() == u8::MAX && texture_color == Some([255, 255, 255, 255]);
    let near_full_surface_opaque_rect =
        opaque_white_rect && cover_basis_points >= CLEAR_ELISION_NEAR_FULL_SURFACE_BASIS_POINTS;

    if texture_color != Some([255, 255, 255, 255]) {
        return ClearElisionQuadEvidence {
            visible_px,
            cover_basis_points,
            opaque_rect: opaque_white_rect,
            full_surface_opaque_rect: false,
            near_full_surface_opaque_rect,
            rejection: Some(ClearElisionQuadRejection::TextureNotConstantWhite),
        };
    }
    if candidate.corners.tl.color.a() != u8::MAX {
        return ClearElisionQuadEvidence {
            visible_px,
            cover_basis_points,
            opaque_rect: opaque_white_rect,
            full_surface_opaque_rect: false,
            near_full_surface_opaque_rect,
            rejection: Some(ClearElisionQuadRejection::AlphaNotOpaque),
        };
    }
    if !clip_covers_surface(clip, surface_width, surface_height) {
        return ClearElisionQuadEvidence {
            visible_px,
            cover_basis_points,
            opaque_rect: opaque_white_rect,
            full_surface_opaque_rect: false,
            near_full_surface_opaque_rect,
            rejection: Some(ClearElisionQuadRejection::ClipNotFullSurface),
        };
    }
    if visible_px != surface_width.saturating_mul(surface_height) {
        return ClearElisionQuadEvidence {
            visible_px,
            cover_basis_points,
            opaque_rect: opaque_white_rect,
            full_surface_opaque_rect: false,
            near_full_surface_opaque_rect,
            rejection: Some(ClearElisionQuadRejection::NotFullSurfaceOpaqueRect),
        };
    }

    ClearElisionQuadEvidence {
        visible_px,
        cover_basis_points,
        opaque_rect: true,
        full_surface_opaque_rect: true,
        near_full_surface_opaque_rect,
        rejection: None,
    }
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

impl ClearElisionQuadEvidence {
    const fn rejected(rejection: ClearElisionQuadRejection) -> Self {
        Self {
            visible_px: 0,
            cover_basis_points: 0,
            opaque_rect: false,
            full_surface_opaque_rect: false,
            near_full_surface_opaque_rect: false,
            rejection: Some(rejection),
        }
    }
}

fn clipped_quad_pixel_area(bounds: QuadBounds, clip: ClipBounds) -> usize {
    let start_x = solid_rect_boundary_index(bounds.min_x, clip.max_x).max(clip.min_x);
    let end_x = solid_rect_boundary_index(bounds.max_x, clip.max_x).min(clip.max_x);
    let start_y = solid_rect_boundary_index(bounds.min_y, clip.max_y).max(clip.min_y);
    let end_y = solid_rect_boundary_index(bounds.max_y, clip.max_y).min(clip.max_y);
    if start_x >= end_x || start_y >= end_y {
        return 0;
    }
    (end_x - start_x) * (end_y - start_y)
}

fn surface_cover_basis_points(
    visible_px: usize,
    surface_width: usize,
    surface_height: usize,
) -> usize {
    visible_px
        .saturating_mul(10_000)
        .checked_div(surface_width.saturating_mul(surface_height))
        .unwrap_or(0)
}

const fn clip_covers_surface(
    clip: ClipBounds,
    surface_width: usize,
    surface_height: usize,
) -> bool {
    clip.min_x == 0
        && clip.min_y == 0
        && clip.max_x == surface_width
        && clip.max_y == surface_height
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
            classification,
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

impl TexturedRectClassification {
    const fn is_sampled_separable_uv(self) -> bool {
        matches!(self.kind, TexturedRectKind::Sampled) && self.separable_uv
    }
}

#[derive(Clone, Copy)]
struct TexturedRectClassification {
    kind: TexturedRectKind,
    white_texel: bool,
    uniform_corner_color: bool,
    separable_uv: bool,
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
            separable_uv: separable_textured_rect_uv(corners),
        };
    };

    TexturedRectClassification {
        kind: TexturedRectKind::ConstantTexel,
        white_texel: texture_color == [255, 255, 255, 255],
        uniform_corner_color,
        separable_uv: false,
    }
}

fn separable_textured_rect_uv(corners: TexturedQuadCorners) -> bool {
    [corners.tl.uv, corners.tr.uv, corners.bl.uv, corners.br.uv]
        .into_iter()
        .all(|uv| uv.x.is_finite() && uv.y.is_finite())
        && same_f32(corners.tl.uv.y, corners.tr.uv.y)
        && same_f32(corners.bl.uv.y, corners.br.uv.y)
        && same_f32(corners.tl.uv.x, corners.bl.uv.x)
        && same_f32(corners.tr.uv.x, corners.br.uv.x)
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
    if sampled_separable_textured_rect_uv(corners, texture) {
        rasterize_separable_uv_textured_rect_no_stats(
            surface,
            corners,
            texture,
            range,
            uv_basis,
            vertex_color,
        );
        return;
    }

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

fn sampled_separable_textured_rect_uv(
    corners: TexturedQuadCorners,
    texture: &TextureImage,
) -> bool {
    texture.width > 0
        && texture.height > 0
        && separable_textured_rect_uv(corners)
        && textured_rect_constant_texel_color(corners, texture).is_none()
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
    classification: TexturedRectClassification,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    stats: &mut RasterStats,
) {
    let vertex_color = color_to_array(corners.tl.color);
    if classification.is_sampled_separable_uv() {
        stats.record_textured_rect_separable_vertex_color(
            vertex_color == [255, 255, 255, 255],
            (range.end_x - range.start_x) * (range.end_y - range.start_y),
        );
        rasterize_separable_uv_textured_rect_with_stats(
            surface,
            corners,
            texture,
            range,
            uv_basis,
            vertex_color,
            stats,
        );
        return;
    }

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

fn rasterize_separable_uv_textured_rect_no_stats(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    vertex_color: [u8; 4],
) {
    if vertex_color == [255, 255, 255, 255] {
        rasterize_separable_uv_textured_rect_no_stats_white(
            surface, corners, texture, range, uv_basis,
        );
        return;
    }

    rasterize_separable_uv_textured_rect_no_stats_modulated(
        surface,
        corners,
        texture,
        range,
        uv_basis,
        vertex_color,
    );
}

fn rasterize_separable_uv_textured_rect_no_stats_white(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
) {
    for y in range.start_y..range.end_y {
        let row = textured_rect_uv_row(corners, range.start_x, y, uv_basis);
        let texture_row_offset = separable_uv_texture_row_offset(texture, row.uv.y);
        let mut pixel_offset = surface.row_offset(y) + range.start_x * 4;
        for x in range.start_x..range.end_x {
            let color =
                separable_uv_texel_color(texture, texture_row_offset, row, x, range.start_x);
            match color[3] {
                0 => {}
                u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
            }
            pixel_offset += 4;
        }
    }
}

fn rasterize_separable_uv_textured_rect_no_stats_modulated(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    vertex_color: [u8; 4],
) {
    for y in range.start_y..range.end_y {
        let row = textured_rect_uv_row(corners, range.start_x, y, uv_basis);
        let texture_row_offset = separable_uv_texture_row_offset(texture, row.uv.y);
        let mut pixel_offset = surface.row_offset(y) + range.start_x * 4;
        for x in range.start_x..range.end_x {
            let texel =
                separable_uv_texel_color(texture, texture_row_offset, row, x, range.start_x);
            if texel[3] != 0 {
                let color = modulate_color(vertex_color, texel);
                match color[3] {
                    0 => {}
                    u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                    _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
                }
            }
            pixel_offset += 4;
        }
    }
}

fn rasterize_separable_uv_textured_rect_with_stats(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    vertex_color: [u8; 4],
    stats: &mut RasterStats,
) {
    stats.textured_rect_separable_direct_calls += 1;
    stats.textured_rect_separable_direct_px +=
        (range.end_x - range.start_x) * (range.end_y - range.start_y);

    if vertex_color == [255, 255, 255, 255] {
        rasterize_separable_uv_textured_rect_with_stats_white(
            surface, corners, texture, range, uv_basis, stats,
        );
        return;
    }

    rasterize_separable_uv_textured_rect_with_stats_modulated(
        surface,
        corners,
        texture,
        range,
        uv_basis,
        vertex_color,
        stats,
    );
}

fn rasterize_separable_uv_textured_rect_with_stats_white(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    stats: &mut RasterStats,
) {
    for y in range.start_y..range.end_y {
        let row = textured_rect_uv_row(corners, range.start_x, y, uv_basis);
        let vector_candidate =
            sampled_textured_rect_vector_candidate(texture, row, range.start_x, range.end_x);
        if vector_candidate {
            stats.record_textured_rect_sampled_vector_candidate(true, range.end_x - range.start_x);
        }
        let texture_row_offset = separable_uv_texture_row_offset(texture, row.uv.y);
        let mut pixel_offset = surface.row_offset(y) + range.start_x * 4;
        let mut block_px = 0;
        let mut block_opaque_px = 0;
        let mut block_transparent_px = 0;
        for x in range.start_x..range.end_x {
            let color =
                separable_uv_texel_color(texture, texture_row_offset, row, x, range.start_x);
            stats.record_textured_rect_separable_direct_alpha_px(color[3], 1);
            stats.record_alpha_px(color[3], 1);
            if vector_candidate {
                record_sampled_textured_rect_vector_alpha(
                    stats,
                    color[3],
                    &mut block_px,
                    &mut block_opaque_px,
                    &mut block_transparent_px,
                );
            }
            match color[3] {
                0 => {}
                u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
            }
            pixel_offset += 4;
        }
        if vector_candidate {
            stats.textured_rect_sampled_vector_tail_px += block_px;
        }
    }
}

fn rasterize_separable_uv_textured_rect_with_stats_modulated(
    surface: &mut SoftwareSurface,
    corners: TexturedQuadCorners,
    texture: &TextureImage,
    range: RectRasterRange,
    uv_basis: RectUvBasis,
    vertex_color: [u8; 4],
    stats: &mut RasterStats,
) {
    for y in range.start_y..range.end_y {
        let row = textured_rect_uv_row(corners, range.start_x, y, uv_basis);
        let vector_candidate =
            sampled_textured_rect_vector_candidate(texture, row, range.start_x, range.end_x);
        if vector_candidate {
            stats.record_textured_rect_sampled_vector_candidate(false, range.end_x - range.start_x);
        }
        let texture_row_offset = separable_uv_texture_row_offset(texture, row.uv.y);
        let mut pixel_offset = surface.row_offset(y) + range.start_x * 4;
        let mut block_px = 0;
        let mut block_opaque_px = 0;
        let mut block_transparent_px = 0;
        for x in range.start_x..range.end_x {
            let texel =
                separable_uv_texel_color(texture, texture_row_offset, row, x, range.start_x);
            if texel[3] == 0 {
                stats.record_textured_rect_separable_direct_alpha_px(0, 1);
                stats.record_alpha_px(0, 1);
                if vector_candidate {
                    record_sampled_textured_rect_vector_alpha(
                        stats,
                        0,
                        &mut block_px,
                        &mut block_opaque_px,
                        &mut block_transparent_px,
                    );
                }
            } else {
                let color = modulate_color(vertex_color, texel);
                stats.record_textured_rect_separable_direct_alpha_px(color[3], 1);
                stats.record_alpha_px(color[3], 1);
                if vector_candidate {
                    record_sampled_textured_rect_vector_alpha(
                        stats,
                        color[3],
                        &mut block_px,
                        &mut block_opaque_px,
                        &mut block_transparent_px,
                    );
                }
                match color[3] {
                    0 => {}
                    u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                    _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
                }
            }
            pixel_offset += 4;
        }
        if vector_candidate {
            stats.textured_rect_sampled_vector_tail_px += block_px;
        }
    }
}

fn record_sampled_textured_rect_vector_alpha(
    stats: &mut RasterStats,
    alpha: u8,
    block_px: &mut usize,
    block_opaque_px: &mut usize,
    block_transparent_px: &mut usize,
) {
    *block_px += 1;
    match alpha {
        0 => *block_transparent_px += 1,
        u8::MAX => *block_opaque_px += 1,
        _ => {}
    }
    if *block_px == TEXTURED_RECT_VECTOR_BLOCK_PX {
        stats.record_textured_rect_sampled_vector_block(*block_opaque_px, *block_transparent_px);
        *block_px = 0;
        *block_opaque_px = 0;
        *block_transparent_px = 0;
    }
}

fn sampled_textured_rect_vector_candidate(
    texture: &TextureImage,
    row: RectUvRow,
    start_x: usize,
    end_x: usize,
) -> bool {
    let width = end_x - start_x;
    if width == 0 || texture.width == 0 {
        return false;
    }
    let max_texel_x = texture.width - 1;
    let first_texel_x = nearest_texel_axis(row.uv_at(start_x, start_x).x, max_texel_x);
    let Some(last_texel_x) = first_texel_x.checked_add(width - 1) else {
        return false;
    };
    if last_texel_x >= texture.width {
        return false;
    }

    (1..width).all(|offset| {
        nearest_texel_axis(row.uv_at(start_x + offset, start_x).x, max_texel_x)
            == first_texel_x + offset
    })
}

fn separable_uv_texture_row_offset(texture: &TextureImage, v: f32) -> usize {
    nearest_texel_axis(v, texture.height.saturating_sub(1)) * texture.width * 4
}

fn separable_uv_texel_color(
    texture: &TextureImage,
    texture_row_offset: usize,
    row: RectUvRow,
    x: usize,
    start_x: usize,
) -> [u8; 4] {
    let texture_offset = texture_row_offset
        + nearest_texel_axis(row.uv_at(x, start_x).x, texture.width.saturating_sub(1)) * 4;
    [
        texture.pixels[texture_offset],
        texture.pixels[texture_offset + 1],
        texture.pixels[texture_offset + 2],
        texture.pixels[texture_offset + 3],
    ]
}

fn nearest_texel_axis(uv: f32, max: usize) -> usize {
    f32_to_usize_round_clamped(uv.clamp(0.0, 1.0) * usize_to_f32(max), max)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separable_uv_textured_rect_matches_generic_for_clipped_flipped_modulated_texture() {
        let corners = textured_rect_corners(
            egui::pos2(1.25, 0.75),
            egui::pos2(6.75, 4.25),
            [96, 160, 224, 192],
            [
                egui::pos2(1.0, 1.0),
                egui::pos2(0.0, 1.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 0.0),
            ],
        );
        let texture = mixed_alpha_texture();
        let bounds = QuadBounds {
            min_x: 1.25,
            min_y: 0.75,
            max_x: 6.75,
            max_y: 4.25,
        };
        let clip = ClipBounds {
            min_x: 2,
            min_y: 1,
            max_x: 7,
            max_y: 4,
        };

        assert_separable_uv_matches_generic(corners, bounds, &texture, clip, [96, 160, 224, 192]);
    }

    #[test]
    fn separable_uv_textured_rect_matches_generic_for_white_vertex_color() {
        let corners = textured_rect_corners(
            egui::pos2(0.6, 0.4),
            egui::pos2(5.4, 3.6),
            [255, 255, 255, 255],
            [
                egui::pos2(-0.25, 0.2),
                egui::pos2(1.25, 0.2),
                egui::pos2(-0.25, 0.9),
                egui::pos2(1.25, 0.9),
            ],
        );
        let texture = mixed_alpha_texture();
        let bounds = QuadBounds {
            min_x: 0.6,
            min_y: 0.4,
            max_x: 5.4,
            max_y: 3.6,
        };

        assert_separable_uv_matches_generic(corners, bounds, &texture, full_clip(7, 5), [255; 4]);
    }

    #[test]
    fn separable_direct_textured_rect_matches_generic_for_alternating_alpha_row() {
        let corners = textured_rect_corners(
            egui::pos2(0.0, 2.0),
            egui::pos2(8.0, 3.0),
            [255, 255, 255, 255],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
            ],
        );
        let texture = alternating_alpha_row_texture();
        let bounds = QuadBounds {
            min_x: 0.0,
            min_y: 2.0,
            max_x: 8.0,
            max_y: 3.0,
        };

        assert_separable_uv_matches_generic(corners, bounds, &texture, full_clip(8, 6), [255; 4]);
    }

    #[test]
    fn separable_direct_textured_rect_treats_transparent_texel_rgb_as_no_op() {
        let corners = textured_rect_corners(
            egui::pos2(1.0, 1.0),
            egui::pos2(5.0, 2.0),
            [255, 255, 255, 255],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
            ],
        );
        let texture = transparent_varying_rgb_texture();
        let bounds = QuadBounds {
            min_x: 1.0,
            min_y: 1.0,
            max_x: 5.0,
            max_y: 2.0,
        };

        assert_separable_uv_matches_generic(corners, bounds, &texture, full_clip(8, 6), [255; 4]);
    }

    #[test]
    fn separable_direct_textured_rect_matches_generic_for_modulated_vertex_color() {
        let corners = textured_rect_corners(
            egui::pos2(1.0, 1.0),
            egui::pos2(5.0, 4.0),
            [64, 128, 192, 160],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 1.0),
                egui::pos2(1.0, 1.0),
            ],
        );
        let texture = mixed_alpha_texture();
        let bounds = QuadBounds {
            min_x: 1.0,
            min_y: 1.0,
            max_x: 5.0,
            max_y: 4.0,
        };

        assert_separable_uv_matches_generic(
            corners,
            bounds,
            &texture,
            full_clip(8, 6),
            [64, 128, 192, 160],
        );
    }

    #[test]
    fn separable_direct_textured_rect_matches_generic_for_clipped_rect() {
        let corners = textured_rect_corners(
            egui::pos2(0.0, 0.0),
            egui::pos2(8.0, 5.0),
            [255, 255, 255, 255],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 1.0),
                egui::pos2(1.0, 1.0),
            ],
        );
        let texture = mixed_alpha_texture();
        let bounds = QuadBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 8.0,
            max_y: 5.0,
        };
        let clip = ClipBounds {
            min_x: 2,
            min_y: 1,
            max_x: 6,
            max_y: 4,
        };

        assert_separable_uv_matches_generic(corners, bounds, &texture, clip, [255; 4]);
    }

    #[test]
    fn sampled_textured_rect_stats_split_separable_and_nonseparable_uvs() {
        let texture = mixed_alpha_texture();
        let mut surface = test_surface(8, 6);
        let mut stats = RasterStats::default();
        let bounds = QuadBounds {
            min_x: 1.0,
            min_y: 1.0,
            max_x: 5.0,
            max_y: 4.0,
        };
        let separable = textured_rect_corners(
            egui::pos2(1.0, 1.0),
            egui::pos2(5.0, 4.0),
            [255; 4],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 1.0),
                egui::pos2(1.0, 1.0),
            ],
        );
        let nonseparable = textured_rect_corners(
            egui::pos2(1.0, 1.0),
            egui::pos2(5.0, 4.0),
            [255; 4],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.2),
                egui::pos2(0.2, 1.0),
                egui::pos2(1.2, 1.2),
            ],
        );

        rasterize_textured_rect(
            &mut surface,
            separable,
            bounds,
            &texture,
            full_clip(8, 6),
            Some(&mut stats),
        );
        rasterize_textured_rect(
            &mut surface,
            nonseparable,
            bounds,
            &texture,
            full_clip(8, 6),
            Some(&mut stats),
        );

        assert_eq!(stats.textured_rect_sampled_calls, 2);
        assert_eq!(stats.textured_rect_separable_uv_calls, 1);
        assert_eq!(stats.textured_rect_nonseparable_uv_calls, 1);
        assert_eq!(stats.textured_rect_separable_uv_px, 12);
        assert_eq!(stats.textured_rect_nonseparable_uv_px, 12);
        assert_eq!(stats.textured_rect_separable_run_calls, 0);
        assert_eq!(stats.textured_rect_separable_run_px, 0);
        assert_eq!(stats.textured_rect_separable_direct_calls, 1);
        assert_eq!(stats.textured_rect_separable_direct_px, 12);
        assert_eq!(stats.textured_rect_separable_direct_opaque_px, 4);
        assert_eq!(stats.textured_rect_separable_direct_translucent_px, 6);
        assert_eq!(stats.textured_rect_separable_direct_transparent_px, 2);
        assert_eq!(stats.textured_rect_separable_white_vertex_calls, 1);
        assert_eq!(stats.textured_rect_separable_white_vertex_px, 12);
        assert_eq!(stats.textured_rect_separable_modulated_vertex_calls, 0);
        assert_eq!(stats.textured_rect_separable_modulated_vertex_px, 0);
        assert_eq!(
            stats.textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16,
            [0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn sampled_textured_rect_vector_stats_classify_white_contiguous_blocks_and_tail() {
        let width = 53;
        let texture = alpha_row_texture(
            (0..width)
                .map(|x| {
                    if (16..32).contains(&x) {
                        0
                    } else if (32..48).contains(&x) && x % 2 == 0 {
                        128
                    } else {
                        u8::MAX
                    }
                })
                .collect(),
        );
        let mut surface = test_surface(width, 1);
        let mut stats = RasterStats::default();
        let bounds = QuadBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: usize_to_f32(width),
            max_y: 1.0,
        };
        let corners = vector_eligible_row_corners(width, [255; 4]);

        rasterize_textured_rect(
            &mut surface,
            corners,
            bounds,
            &texture,
            full_clip(width, 1),
            Some(&mut stats),
        );

        assert_eq!(stats.textured_rect_sampled_vector_candidate_px, width);
        assert_eq!(
            stats.textured_rect_sampled_white_vertex_contiguous_px,
            width
        );
        assert_eq!(
            stats.textured_rect_sampled_modulated_vertex_contiguous_px,
            0
        );
        assert_eq!(stats.textured_rect_sampled_vector_blocks, 3);
        assert_eq!(stats.textured_rect_sampled_vector_opaque_blocks, 1);
        assert_eq!(stats.textured_rect_sampled_vector_transparent_blocks, 1);
        assert_eq!(stats.textured_rect_sampled_vector_mixed_blocks, 1);
        assert_eq!(stats.textured_rect_sampled_vector_tail_px, 5);
    }

    #[test]
    fn sampled_textured_rect_vector_stats_count_modulated_contiguous_pixels() {
        let width = 17;
        let texture = alpha_row_texture(
            (0..width)
                .map(|x| if x == width - 1 { 0 } else { u8::MAX })
                .collect(),
        );
        let mut surface = test_surface(width, 1);
        let mut stats = RasterStats::default();
        let bounds = QuadBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: usize_to_f32(width),
            max_y: 1.0,
        };
        let corners = vector_eligible_row_corners(width, [128, 192, 224, 255]);

        rasterize_textured_rect(
            &mut surface,
            corners,
            bounds,
            &texture,
            full_clip(width, 1),
            Some(&mut stats),
        );

        assert_eq!(stats.textured_rect_sampled_vector_candidate_px, width);
        assert_eq!(stats.textured_rect_sampled_white_vertex_contiguous_px, 0);
        assert_eq!(
            stats.textured_rect_sampled_modulated_vertex_contiguous_px,
            width
        );
        assert_eq!(stats.textured_rect_sampled_vector_blocks, 1);
        assert_eq!(stats.textured_rect_sampled_vector_opaque_blocks, 1);
        assert_eq!(stats.textured_rect_sampled_vector_transparent_blocks, 0);
        assert_eq!(stats.textured_rect_sampled_vector_mixed_blocks, 0);
        assert_eq!(stats.textured_rect_sampled_vector_tail_px, 1);
    }

    #[test]
    fn sampled_textured_rect_vector_stats_ignore_non_contiguous_sampled_rows() {
        let texture =
            alpha_row_texture((0..20).map(|x| if x == 0 { 0 } else { u8::MAX }).collect());
        let mut surface = test_surface(32, 1);
        let mut stats = RasterStats::default();
        let bounds = QuadBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 32.0,
            max_y: 1.0,
        };
        let corners = textured_rect_corners(
            egui::pos2(0.0, 0.0),
            egui::pos2(32.0, 1.0),
            [255; 4],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
            ],
        );

        rasterize_textured_rect(
            &mut surface,
            corners,
            bounds,
            &texture,
            full_clip(32, 1),
            Some(&mut stats),
        );

        assert_eq!(stats.textured_rect_sampled_calls, 1);
        assert_eq!(stats.textured_rect_sampled_vector_candidate_px, 0);
        assert_eq!(stats.textured_rect_sampled_white_vertex_contiguous_px, 0);
        assert_eq!(stats.textured_rect_sampled_vector_blocks, 0);
        assert_eq!(stats.textured_rect_sampled_vector_tail_px, 0);
    }

    #[test]
    fn separable_uv_textured_rect_same_sampled_texel_counts_as_constant_texel() {
        let texture = mixed_alpha_texture();
        let mut surface = test_surface(8, 6);
        let mut stats = RasterStats::default();
        let bounds = QuadBounds {
            min_x: 1.0,
            min_y: 1.0,
            max_x: 5.0,
            max_y: 4.0,
        };
        let corners = textured_rect_corners(
            egui::pos2(1.0, 1.0),
            egui::pos2(5.0, 4.0),
            [255; 4],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(0.1, 0.0),
                egui::pos2(0.0, 0.1),
                egui::pos2(0.1, 0.1),
            ],
        );

        rasterize_textured_rect(
            &mut surface,
            corners,
            bounds,
            &texture,
            full_clip(8, 6),
            Some(&mut stats),
        );

        assert!(separable_textured_rect_uv(corners));
        assert!(!sampled_separable_textured_rect_uv(corners, &texture));
        assert_eq!(stats.textured_rect_constant_texel_calls, 1);
        assert_eq!(stats.textured_rect_constant_texel_px, 12);
        assert_eq!(stats.textured_rect_sampled_calls, 0);
        assert_eq!(stats.textured_rect_separable_uv_calls, 0);
        assert_eq!(stats.textured_rect_separable_uv_px, 0);
    }

    #[test]
    fn empty_texture_separable_textured_rect_uses_white_fallback_not_sampled_separable() {
        let texture = empty_texture();
        let mut surface = test_surface(4, 4);
        let mut stats = RasterStats::default();
        let bounds = QuadBounds {
            min_x: 1.0,
            min_y: 1.0,
            max_x: 3.0,
            max_y: 3.0,
        };
        let corners = textured_rect_corners(
            egui::pos2(1.0, 1.0),
            egui::pos2(3.0, 3.0),
            [255; 4],
            [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(0.0, 1.0),
                egui::pos2(1.0, 1.0),
            ],
        );

        rasterize_textured_rect(
            &mut surface,
            corners,
            bounds,
            &texture,
            full_clip(4, 4),
            Some(&mut stats),
        );

        assert!(separable_textured_rect_uv(corners));
        assert!(!sampled_separable_textured_rect_uv(corners, &texture));
        assert_eq!(stats.textured_rect_constant_texel_calls, 1);
        assert_eq!(stats.textured_rect_white_texel_calls, 1);
        assert_eq!(stats.textured_rect_sampled_calls, 0);
        assert_eq!(stats.textured_rect_separable_uv_calls, 0);
        assert_eq!(pixel_at(&surface.pixels, 4, 1, 1), [255; 4]);
        assert_eq!(pixel_at(&surface.pixels, 4, 2, 2), [255; 4]);
    }

    fn assert_separable_uv_matches_generic(
        corners: TexturedQuadCorners,
        bounds: QuadBounds,
        texture: &TextureImage,
        clip: ClipBounds,
        vertex_color: [u8; 4],
    ) {
        let range = rect_range(bounds, clip);
        let uv_basis = rect_uv_basis(bounds);
        let mut generic = test_surface(8, 6);
        let mut specialized = test_surface(8, 6);

        if vertex_color == [255; 4] {
            rasterize_textured_rect_no_stats_with_color(
                &mut generic,
                corners,
                texture,
                range,
                uv_basis,
                sample_nearest,
            );
        } else {
            rasterize_textured_rect_no_stats_with_color(
                &mut generic,
                corners,
                texture,
                range,
                uv_basis,
                |texture, uv| modulate_color(vertex_color, sample_nearest(texture, uv)),
            );
        }
        rasterize_separable_uv_textured_rect_no_stats(
            &mut specialized,
            corners,
            texture,
            range,
            uv_basis,
            vertex_color,
        );

        assert!(separable_textured_rect_uv(corners));
        assert_eq!(specialized.pixels, generic.pixels);
    }

    fn textured_rect_corners(
        min: egui::Pos2,
        max: egui::Pos2,
        color: [u8; 4],
        uvs: [egui::Pos2; 4],
    ) -> TexturedQuadCorners {
        TexturedQuadCorners {
            tl: vertex(min.x, min.y, color, uvs[0]),
            tr: vertex(max.x, min.y, color, uvs[1]),
            bl: vertex(min.x, max.y, color, uvs[2]),
            br: vertex(max.x, max.y, color, uvs[3]),
        }
    }

    fn vector_eligible_row_corners(width: usize, color: [u8; 4]) -> TexturedQuadCorners {
        let max_texel = usize_to_f32(width - 1);
        textured_rect_corners(
            egui::pos2(0.0, 0.0),
            egui::pos2(usize_to_f32(width), 1.0),
            color,
            [
                egui::pos2(-0.5 / max_texel, 0.0),
                egui::pos2((usize_to_f32(width) - 0.5) / max_texel, 0.0),
                egui::pos2(-0.5 / max_texel, 0.0),
                egui::pos2((usize_to_f32(width) - 0.5) / max_texel, 0.0),
            ],
        )
    }

    fn vertex(x: f32, y: f32, color: [u8; 4], uv: egui::Pos2) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            color: egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]),
            uv,
        }
    }

    fn rect_range(bounds: QuadBounds, clip: ClipBounds) -> RectRasterRange {
        RectRasterRange {
            start_x: solid_rect_boundary_index(bounds.min_x, clip.max_x).max(clip.min_x),
            end_x: solid_rect_boundary_index(bounds.max_x, clip.max_x).min(clip.max_x),
            start_y: solid_rect_boundary_index(bounds.min_y, clip.max_y).max(clip.min_y),
            end_y: solid_rect_boundary_index(bounds.max_y, clip.max_y).min(clip.max_y),
        }
    }

    fn rect_uv_basis(bounds: QuadBounds) -> RectUvBasis {
        RectUvBasis {
            min_x: bounds.min_x,
            min_y: bounds.min_y,
            inv_width: 1.0 / (bounds.max_x - bounds.min_x),
            inv_height: 1.0 / (bounds.max_y - bounds.min_y),
        }
    }

    fn test_surface(width: usize, height: usize) -> SoftwareSurface {
        let mut surface = SoftwareSurface::default();
        surface.resize(width, height).expect("surface");
        surface.clear([13, 29, 47, 255]);
        surface
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

    const fn full_clip(width: usize, height: usize) -> ClipBounds {
        ClipBounds {
            min_x: 0,
            min_y: 0,
            max_x: width,
            max_y: height,
        }
    }

    fn mixed_alpha_texture() -> TextureImage {
        TextureImage {
            width: 4,
            height: 3,
            pixels: vec![
                0, 0, 0, 0, 32, 64, 96, 128, 80, 40, 20, 255, 255, 0, 0, 64, 0, 255, 0, 192, 0, 0,
                255, 255, 128, 128, 0, 0, 255, 255, 255, 255, 12, 24, 48, 96, 48, 24, 12, 160, 90,
                120, 150, 224, 200, 20, 100, 255,
            ],
        }
    }

    fn alternating_alpha_row_texture() -> TextureImage {
        TextureImage {
            width: 8,
            height: 1,
            pixels: vec![
                0, 0, 0, 0, 32, 32, 32, 96, 64, 64, 64, 255, 96, 96, 96, 0, 128, 128, 128, 160,
                160, 160, 160, 255, 192, 192, 192, 0, 224, 224, 224, 224,
            ],
        }
    }

    fn alpha_row_texture(alphas: Vec<u8>) -> TextureImage {
        let width = alphas.len();
        let mut pixels = Vec::with_capacity(width * 4);
        for alpha in alphas {
            pixels.extend_from_slice(&[alpha, alpha, alpha, alpha]);
        }
        TextureImage {
            width,
            height: 1,
            pixels,
        }
    }

    fn transparent_varying_rgb_texture() -> TextureImage {
        TextureImage {
            width: 4,
            height: 1,
            pixels: vec![255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 128, 64, 32, 0],
        }
    }

    fn empty_texture() -> TextureImage {
        TextureImage {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
    }
}
