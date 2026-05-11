// SPDX-License-Identifier: GPL-3.0-only

use std::time::Instant;

use super::coverage::triangle_scanline_x_range;
use super::math::{
    edge, edge_covers_pixel, edge_includes_boundary, edge_step_x, edge_step_y,
    f32_to_u8_round_clamped, interpolate_channel_value, interpolate_color, modulate_color,
};
use super::sampling::sample_nearest;
use super::stats::RasterStats;
use super::triangle::{TRIANGLE_SCANLINE_NARROWING_MIN_AREA, TriangleVertices, triangle_positions};
use super::types::TriangleRasterBounds;
use super::{duration_as_us, usize_to_f32};
use crate::gui::portmaster::{surface::SoftwareSurface, texture::TextureImage};

#[derive(Clone, Copy)]
pub(super) enum TexturedTriangleKind {
    ConstantTexel { white_texel: bool },
    Sampled,
}

pub(super) fn record_textured_triangle_call(
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

pub(super) const fn is_white_texel(texture_color: [u8; 4]) -> bool {
    matches!(texture_color, [255, 255, 255, 255])
}

pub(super) fn rasterize_textured_triangle(
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

pub(super) fn rasterize_constant_texel_textured_triangle(
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
    if white_constant_texel_alpha_only_vertices(vertices) {
        rasterize_white_constant_texel_alpha_only_textured_triangle_no_stats(
            surface, vertices, bounds, area,
        );
        return;
    }

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

fn rasterize_white_constant_texel_alpha_only_textured_triangle_no_stats(
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
    let alpha_step = white_constant_texel_alpha_step(v0, v1, v2, &raster);

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
        let row_alpha = white_constant_texel_row_alpha(
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
                emit_white_constant_texel_alpha_only_run_no_stats(
                    surface,
                    row_alpha,
                    alpha_step,
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
            emit_white_constant_texel_alpha_only_run_no_stats(
                surface,
                row_alpha,
                alpha_step,
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
    if white_constant_texel_alpha_only_vertices(vertices) {
        rasterize_white_constant_texel_alpha_only_textured_triangle_with_stats(
            surface, vertices, bounds, area, stats,
        );
        return;
    }

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

fn rasterize_white_constant_texel_alpha_only_textured_triangle_with_stats(
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
    let alpha_step = white_constant_texel_alpha_step(v0, v1, v2, &raster);

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
        let row_alpha = white_constant_texel_row_alpha(
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
                emit_white_constant_texel_alpha_only_run_with_stats(
                    surface,
                    row_alpha,
                    alpha_step,
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
            emit_white_constant_texel_alpha_only_run_with_stats(
                surface,
                row_alpha,
                alpha_step,
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

fn emit_white_constant_texel_alpha_only_run_no_stats(
    surface: &mut SoftwareSurface,
    row_alpha: f32,
    alpha_step: f32,
    start_dx: usize,
    len: usize,
    mut pixel_offset: usize,
) {
    for run_dx in 0..len {
        let alpha =
            white_constant_texel_alpha_only_pixel_alpha(row_alpha, alpha_step, start_dx + run_dx);
        match alpha {
            0 => {}
            u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, [255, 255, 255, alpha]),
            _ => surface.blend_translucent_pixel_at_offset(pixel_offset, [255, 255, 255, alpha]),
        }
        pixel_offset += 4;
    }
}

fn emit_white_constant_texel_alpha_only_run_with_stats(
    surface: &mut SoftwareSurface,
    row_alpha: f32,
    alpha_step: f32,
    start_dx: usize,
    len: usize,
    mut pixel_offset: usize,
    stats: &mut RasterStats,
) {
    stats.textured_triangle_covered_px += len;
    stats.constant_texel_textured_triangle_covered_px += len;
    stats.constant_texel_textured_triangle_white_texel_covered_px += len;
    for run_dx in 0..len {
        let alpha =
            white_constant_texel_alpha_only_pixel_alpha(row_alpha, alpha_step, start_dx + run_dx);
        match alpha {
            0 => {
                stats.transparent_px += 1;
                stats.constant_texel_textured_triangle_transparent_px += 1;
            }
            u8::MAX => {
                stats.opaque_px += 1;
                stats.constant_texel_textured_triangle_opaque_px += 1;
                surface.write_opaque_pixel_at_offset(pixel_offset, [255, 255, 255, alpha]);
            }
            _ => {
                stats.translucent_px += 1;
                stats.constant_texel_textured_triangle_translucent_px += 1;
                surface.blend_translucent_pixel_at_offset(pixel_offset, [255, 255, 255, alpha]);
            }
        }
        pixel_offset += 4;
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

fn white_constant_texel_alpha_only_pixel_alpha(row_alpha: f32, alpha_step: f32, dx: usize) -> u8 {
    f32_to_u8_round_clamped(alpha_step.mul_add(usize_to_f32(dx), row_alpha))
}

pub(super) fn white_constant_texel_alpha_only_vertices(vertices: TriangleVertices<'_>) -> bool {
    let TriangleVertices { v0, v1, v2 } = vertices;
    [v0, v1, v2]
        .iter()
        .all(|vertex| vertex.color.r() == 255 && vertex.color.g() == 255 && vertex.color.b() == 255)
}

fn white_constant_texel_row_alpha(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    raster: &TriangleRasterState,
    row_edges: (f32, f32, f32),
) -> f32 {
    let w0 = row_edges.0 * raster.inv_area;
    let w1 = row_edges.1 * raster.inv_area;
    let w2 = row_edges.2 * raster.inv_area;
    interpolate_channel_value(v0.color.a(), v1.color.a(), v2.color.a(), w0, w1, w2)
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

fn white_constant_texel_alpha_step(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    raster: &TriangleRasterState,
) -> f32 {
    let w0_step = raster.w0_step_x * raster.inv_area;
    let w1_step = raster.w1_step_x * raster.inv_area;
    let w2_step = raster.w2_step_x * raster.inv_area;
    interpolate_channel_value(
        v0.color.a(),
        v1.color.a(),
        v2.color.a(),
        w0_step,
        w1_step,
        w2_step,
    )
}

pub(super) fn textured_triangle_pixel_color(
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
