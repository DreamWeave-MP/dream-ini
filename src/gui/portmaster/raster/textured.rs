// SPDX-License-Identifier: GPL-3.0-only

use std::time::Instant;

use super::coverage::{
    TriangleBoundaryIncludes, TriangleCoverage, TriangleRowSearch, triangle_row_endpoints,
    triangle_scanline_x_range,
};
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
                    AlphaOnlyRun {
                        alpha_offset: start - start_x,
                        len: x - start,
                        y,
                        x: start,
                    },
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
                AlphaOnlyRun {
                    alpha_offset: start - start_x,
                    len: end_x - start,
                    y,
                    x: start,
                },
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
    let alpha_only = white_constant_texel_alpha_only_vertices(vertices);
    if alpha_only {
        stats.constant_texel_textured_triangle_white_alpha_only_eligible_calls += 1;
        rasterize_white_constant_texel_alpha_only_textured_triangle_with_stats(
            surface, vertices, bounds, area, stats,
        );
        return;
    }
    stats.constant_texel_textured_triangle_white_alpha_only_rejected_rgb_calls += 1;
    if white_constant_texel_uniform_rgb_vertices(vertices) {
        stats.constant_texel_textured_triangle_white_alpha_only_rejected_uniform_rgb_calls += 1;
    } else {
        stats.constant_texel_textured_triangle_white_alpha_only_rejected_varying_rgb_calls += 1;
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
        record_constant_texel_textured_triangle_candidate_row(stats, bounds, candidate_px);
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
        let mut scan_runs = 0;
        let mut scan_span = None;
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if triangle_raster_state_covers_pixel(&raster, w0, w1, w2) {
                run_start.get_or_insert(x);
            } else if let Some(start) = run_start.take() {
                record_white_constant_texel_scan_run(start, x, &mut scan_runs, &mut scan_span);
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
            record_white_constant_texel_scan_run(start, end_x, &mut scan_runs, &mut scan_span);
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
        record_white_constant_texel_endpoint_row(
            stats,
            vertices,
            &raster,
            y,
            (start_x, end_x),
            (scan_runs, scan_span),
        );
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

fn record_constant_texel_textured_triangle_candidate_row(
    stats: &mut RasterStats,
    bounds: TriangleRasterBounds,
    candidate_px: usize,
) {
    stats.textured_triangle_candidate_px += candidate_px;
    stats.constant_texel_textured_triangle_candidate_px += candidate_px;
    if candidate_px < bounds.max_x - bounds.min_x {
        stats.textured_triangle_narrowed_rows += 1;
    } else {
        stats.textured_triangle_full_scan_rows += 1;
    }
}

fn record_white_constant_texel_scan_run(
    start_x: usize,
    end_x: usize,
    scan_runs: &mut usize,
    scan_span: &mut Option<(usize, usize)>,
) {
    *scan_runs += 1;
    if *scan_runs == 1 {
        *scan_span = Some((start_x, end_x));
    }
}

fn record_white_constant_texel_endpoint_row(
    stats: &mut RasterStats,
    vertices: TriangleVertices<'_>,
    raster: &TriangleRasterState,
    y: usize,
    x_range: (usize, usize),
    scan_result: (usize, Option<(usize, usize)>),
) {
    let (start_x, end_x) = x_range;
    let (scan_runs, scan_span) = scan_result;
    let endpoints = triangle_row_endpoints(TriangleRowSearch {
        vertices,
        coverage: TriangleCoverage {
            inv_area: raster.inv_area,
            includes_boundary: TriangleBoundaryIncludes {
                edge0: raster.edge0_includes_boundary,
                edge1: raster.edge1_includes_boundary,
                edge2: raster.edge2_includes_boundary,
            },
        },
        y,
        candidate_start_x: start_x,
        candidate_end_x: end_x,
        probe_start_x: start_x,
        probe_end_x: end_x,
        hinted: false,
        collect_stats: true,
    });

    stats.constant_texel_textured_triangle_white_endpoint_rows += 1;
    stats.constant_texel_textured_triangle_white_endpoint_probe_px += endpoints.endpoint_probe_px;
    stats.constant_texel_textured_triangle_white_scan_runs += scan_runs;
    if let Some((endpoint_start, endpoint_end)) = endpoints.span {
        stats.constant_texel_textured_triangle_white_endpoint_span_px +=
            endpoint_end - endpoint_start;
    } else {
        stats.constant_texel_textured_triangle_white_endpoint_empty_rows += 1;
    }

    let matches = match scan_runs {
        0 => endpoints.span.is_none(),
        1 => endpoints.span == scan_span,
        _ => false,
    };
    if matches {
        stats.constant_texel_textured_triangle_white_endpoint_match_rows += 1;
    } else {
        stats.constant_texel_textured_triangle_white_endpoint_mismatch_rows += 1;
    }
    if scan_runs > 1 {
        stats.constant_texel_textured_triangle_white_scan_multi_run_rows += 1;
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
        record_constant_texel_textured_triangle_candidate_row(stats, bounds, candidate_px);
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
        let mut run_start = None;
        let mut scan_runs = 0;
        let mut scan_span = None;
        for x in start_x..end_x {
            let w0 = pixel_edge0 * raster.inv_area;
            let w1 = pixel_edge1 * raster.inv_area;
            let w2 = pixel_edge2 * raster.inv_area;
            if triangle_raster_state_covers_pixel(&raster, w0, w1, w2) {
                run_start.get_or_insert(x);
            } else if let Some(start) = run_start.take() {
                record_white_constant_texel_scan_run(start, x, &mut scan_runs, &mut scan_span);
                emit_white_constant_texel_alpha_only_run_with_stats(
                    surface,
                    row_alpha,
                    alpha_step,
                    AlphaOnlyRun {
                        alpha_offset: start - start_x,
                        len: x - start,
                        y,
                        x: start,
                    },
                    stats,
                );
            }
            pixel_edge0 += raster.w0_step_x;
            pixel_edge1 += raster.w1_step_x;
            pixel_edge2 += raster.w2_step_x;
        }
        if let Some(start) = run_start {
            record_white_constant_texel_scan_run(start, end_x, &mut scan_runs, &mut scan_span);
            emit_white_constant_texel_alpha_only_run_with_stats(
                surface,
                row_alpha,
                alpha_step,
                AlphaOnlyRun {
                    alpha_offset: start - start_x,
                    len: end_x - start,
                    y,
                    x: start,
                },
                stats,
            );
        }
        record_white_constant_texel_endpoint_row(
            stats,
            vertices,
            &raster,
            y,
            (start_x, end_x),
            (scan_runs, scan_span),
        );
        row_edge0 += raster.w0_step_y;
        row_edge1 += raster.w1_step_y;
        row_edge2 += raster.w2_step_y;
    }
}

#[derive(Clone, Copy)]
struct AlphaOnlyRun {
    alpha_offset: usize,
    len: usize,
    y: usize,
    x: usize,
}

fn emit_white_constant_texel_alpha_only_run_no_stats(
    surface: &mut SoftwareSurface,
    row_alpha: f32,
    alpha_step: f32,
    run: AlphaOnlyRun,
) {
    let mut pixel_offset = surface.row_offset(run.y) + run.x * 4;
    for offset in 0..run.len {
        let alpha = white_constant_texel_alpha_only_pixel_alpha(
            row_alpha,
            alpha_step,
            run.alpha_offset + offset,
        );
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
    run: AlphaOnlyRun,
    stats: &mut RasterStats,
) {
    stats.textured_triangle_covered_px += run.len;
    stats.constant_texel_textured_triangle_covered_px += run.len;
    stats.constant_texel_textured_triangle_white_texel_covered_px += run.len;
    if alpha_step == 0.0 {
        stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls += 1;
        stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px += run.len;
    } else {
        stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls += 1;
        stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px += run.len;
    }

    let mut pixel_offset = surface.row_offset(run.y) + run.x * 4;
    for offset in 0..run.len {
        let alpha = white_constant_texel_alpha_only_pixel_alpha(
            row_alpha,
            alpha_step,
            run.alpha_offset + offset,
        );
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
    pixel_offset: usize,
) {
    if let Some(alpha) =
        white_constant_texel_constant_run_alpha(row_color, color_step, start_dx, len)
    {
        emit_white_constant_texel_constant_alpha_run_no_stats(
            surface,
            row_color,
            color_step,
            WhiteConstantTexelRun {
                start_dx,
                len,
                pixel_offset,
            },
            alpha,
        );
        return;
    }

    emit_white_constant_texel_variable_alpha_run_no_stats(
        surface,
        row_color,
        color_step,
        WhiteConstantTexelRun {
            start_dx,
            len,
            pixel_offset,
        },
    );
}

#[derive(Clone, Copy)]
struct WhiteConstantTexelRun {
    start_dx: usize,
    len: usize,
    pixel_offset: usize,
}

fn white_constant_texel_constant_run_alpha(
    row_color: [f32; 4],
    color_step: [f32; 4],
    start_dx: usize,
    len: usize,
) -> Option<u8> {
    if len == 0 || !row_color[3].is_finite() || !color_step[3].is_finite() {
        return None;
    }

    let last_dx = start_dx.checked_add(len - 1)?;
    let first = white_constant_texel_pixel_alpha(row_color, color_step, start_dx);
    let last = white_constant_texel_pixel_alpha(row_color, color_step, last_dx);
    (first == last).then_some(first)
}

fn emit_white_constant_texel_constant_alpha_run_no_stats(
    surface: &mut SoftwareSurface,
    row_color: [f32; 4],
    color_step: [f32; 4],
    run: WhiteConstantTexelRun,
    alpha: u8,
) {
    if let Some(color) = white_constant_texel_constant_run_color(row_color, color_step, run, alpha)
    {
        emit_white_constant_texel_constant_color_run_no_stats(surface, run, color);
        return;
    }

    emit_white_constant_texel_variable_color_constant_alpha_run_no_stats(
        surface, row_color, color_step, run, alpha,
    );
}

fn white_constant_texel_constant_run_color(
    row_color: [f32; 4],
    color_step: [f32; 4],
    run: WhiteConstantTexelRun,
    alpha: u8,
) -> Option<[u8; 4]> {
    if run.len == 0
        || row_color.iter().any(|value| !value.is_finite())
        || color_step.iter().any(|value| !value.is_finite())
    {
        return None;
    }

    let last_dx = run.start_dx.checked_add(run.len - 1)?;
    let first =
        white_constant_texel_pixel_color_with_alpha(row_color, color_step, run.start_dx, alpha);
    let last = white_constant_texel_pixel_color_with_alpha(row_color, color_step, last_dx, alpha);
    (first == last).then_some(first)
}

fn emit_white_constant_texel_constant_color_run_no_stats(
    surface: &mut SoftwareSurface,
    run: WhiteConstantTexelRun,
    color: [u8; 4],
) {
    surface.blend_constant_color_span_at_offset(run.pixel_offset, run.len, color);
}

fn emit_white_constant_texel_variable_color_constant_alpha_run_no_stats(
    surface: &mut SoftwareSurface,
    row_color: [f32; 4],
    color_step: [f32; 4],
    run: WhiteConstantTexelRun,
    alpha: u8,
) {
    match alpha {
        0 => {}
        u8::MAX => {
            let mut pixel_offset = run.pixel_offset;
            for run_dx in 0..run.len {
                let dx = run.start_dx + run_dx;
                let color =
                    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha);
                surface.write_opaque_pixel_at_offset(pixel_offset, color);
                pixel_offset += 4;
            }
        }
        _ => {
            let mut pixel_offset = run.pixel_offset;
            for run_dx in 0..run.len {
                let dx = run.start_dx + run_dx;
                let color =
                    white_constant_texel_pixel_color_with_alpha(row_color, color_step, dx, alpha);
                surface.blend_translucent_pixel_at_offset(pixel_offset, color);
                pixel_offset += 4;
            }
        }
    }
}

fn emit_white_constant_texel_variable_alpha_run_no_stats(
    surface: &mut SoftwareSurface,
    row_color: [f32; 4],
    color_step: [f32; 4],
    run: WhiteConstantTexelRun,
) {
    let mut pixel_offset = run.pixel_offset;
    for run_dx in 0..run.len {
        let dx = run.start_dx + run_dx;
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
    pixel_offset: usize,
    stats: &mut RasterStats,
) {
    stats.textured_triangle_covered_px += len;
    stats.constant_texel_textured_triangle_covered_px += len;
    stats.constant_texel_textured_triangle_white_texel_covered_px += len;

    let run = WhiteConstantTexelRun {
        start_dx,
        len,
        pixel_offset,
    };
    if let Some(alpha) =
        white_constant_texel_constant_run_alpha(row_color, color_step, start_dx, len)
    {
        stats.constant_texel_textured_triangle_white_constant_alpha_run_calls += 1;
        stats.constant_texel_textured_triangle_white_constant_alpha_run_px += len;
        stats.record_alpha_px(alpha, len);
        stats.record_constant_texel_alpha_px(alpha, len);
        if let Some(color) =
            white_constant_texel_constant_run_color(row_color, color_step, run, alpha)
        {
            stats.constant_texel_textured_triangle_white_constant_color_run_calls += 1;
            stats.constant_texel_textured_triangle_white_constant_color_run_px += len;
            emit_white_constant_texel_constant_color_run_no_stats(surface, run, color);
        } else {
            stats.constant_texel_textured_triangle_white_variable_color_run_calls += 1;
            stats.constant_texel_textured_triangle_white_variable_color_run_px += len;
            emit_white_constant_texel_variable_color_constant_alpha_run_no_stats(
                surface, row_color, color_step, run, alpha,
            );
        }
        return;
    }

    stats.constant_texel_textured_triangle_white_variable_alpha_run_calls += 1;
    stats.constant_texel_textured_triangle_white_variable_alpha_run_px += len;
    emit_white_constant_texel_variable_alpha_run_with_stats(
        surface, row_color, color_step, run, stats,
    );
}

fn emit_white_constant_texel_variable_alpha_run_with_stats(
    surface: &mut SoftwareSurface,
    row_color: [f32; 4],
    color_step: [f32; 4],
    run: WhiteConstantTexelRun,
    stats: &mut RasterStats,
) {
    let mut pixel_offset = run.pixel_offset;
    for run_dx in 0..run.len {
        let dx = run.start_dx + run_dx;
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

fn triangle_raster_state_covers_pixel(
    raster: &TriangleRasterState,
    w0: f32,
    w1: f32,
    w2: f32,
) -> bool {
    edge_covers_pixel(w0, raster.edge0_includes_boundary)
        && edge_covers_pixel(w1, raster.edge1_includes_boundary)
        && edge_covers_pixel(w2, raster.edge2_includes_boundary)
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

fn white_constant_texel_uniform_rgb_vertices(vertices: TriangleVertices<'_>) -> bool {
    let TriangleVertices { v0, v1, v2 } = vertices;
    v0.color.r() == v1.color.r()
        && v0.color.r() == v2.color.r()
        && v0.color.g() == v1.color.g()
        && v0.color.g() == v2.color.g()
        && v0.color.b() == v1.color.b()
        && v0.color.b() == v2.color.b()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::portmaster::raster::math::{
        f32_to_usize_ceil_clamped, f32_to_usize_floor_clamped,
    };

    #[test]
    fn constant_alpha_only_runs_match_reference_for_translucent_alpha() {
        assert_constant_alpha_only_run_matches_reference(117.4, 0.0, 3, 8, 2, 5);
    }

    #[test]
    fn constant_alpha_only_runs_match_reference_for_transparent_alpha() {
        assert_constant_alpha_only_run_matches_reference(-12.0, 0.0, 4, 7, 1, 6);
    }

    #[test]
    fn constant_alpha_only_runs_match_reference_for_opaque_alpha() {
        assert_constant_alpha_only_run_matches_reference(300.0, 0.0, 2, 9, 3, 4);
    }

    #[test]
    fn variable_alpha_only_runs_match_reference_and_classify_counters() {
        let row_alpha = 63.25;
        let alpha_step = 7.5;
        let alpha_offset = 2;
        let len = 11;
        let y = 2;
        let x = 3;
        let run = AlphaOnlyRun {
            alpha_offset,
            len,
            y,
            x,
        };
        let mut actual = test_surface(18, 6);
        let mut actual_no_stats = test_surface(18, 6);
        let mut reference = test_surface(18, 6);
        let mut stats = RasterStats::default();

        emit_white_constant_texel_alpha_only_run_with_stats(
            &mut actual,
            row_alpha,
            alpha_step,
            run,
            &mut stats,
        );
        emit_white_constant_texel_alpha_only_run_no_stats(
            &mut actual_no_stats,
            row_alpha,
            alpha_step,
            run,
        );
        emit_white_constant_texel_alpha_only_run_reference(
            &mut reference,
            row_alpha,
            alpha_step,
            run,
        );

        assert_eq!(actual.pixels, reference.pixels);
        assert_eq!(actual_no_stats.pixels, reference.pixels);
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls,
            0
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px,
            0
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls,
            1
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px,
            len
        );
        assert_eq!(stats.textured_triangle_covered_px, len);
        assert_alpha_stats_match_run(&stats, row_alpha, alpha_step, alpha_offset, len);
    }

    #[test]
    fn white_constant_texel_runs_match_reference_for_transparent_alpha() {
        assert_white_constant_texel_run_matches_reference(
            [20.0, 40.0, 60.0, -8.0],
            [0.0, 0.0, 0.0, 0.0],
            2,
            8,
            (2, 4),
            AlphaRunKind::Constant(0),
            ColorRunKind::Constant([20, 40, 60, 0]),
        );
    }

    #[test]
    fn white_constant_texel_runs_match_reference_for_opaque_alpha() {
        assert_white_constant_texel_run_matches_reference(
            [20.0, 40.0, 60.0, 260.0],
            [0.0, 0.0, 0.0, 0.0],
            2,
            8,
            (2, 4),
            AlphaRunKind::Constant(u8::MAX),
            ColorRunKind::Constant([20, 40, 60, u8::MAX]),
        );
    }

    #[test]
    fn white_constant_texel_runs_match_reference_for_translucent_alpha() {
        assert_white_constant_texel_run_matches_reference(
            [20.0, 40.0, 60.0, 117.4],
            [0.0, 0.0, 0.0, 0.0],
            2,
            8,
            (2, 4),
            AlphaRunKind::Constant(117),
            ColorRunKind::Constant([20, 40, 60, 117]),
        );
    }

    #[test]
    fn white_constant_texel_variable_alpha_runs_match_reference_and_fallback() {
        assert_white_constant_texel_run_matches_reference(
            [12.0, 220.0, 40.0, 63.25],
            [2.5, -3.0, 1.25, 7.5],
            2,
            11,
            (2, 3),
            AlphaRunKind::Variable,
            ColorRunKind::Variable,
        );
    }

    #[test]
    fn white_constant_texel_negative_alpha_step_fixed_rounding_matches_reference() {
        assert_white_constant_texel_run_matches_reference(
            [12.0, 220.0, 40.0, 128.49],
            [2.5, -3.0, 1.25, -0.03],
            4,
            5,
            (2, 3),
            AlphaRunKind::Constant(128),
            ColorRunKind::Variable,
        );
    }

    #[test]
    fn white_constant_texel_nonzero_start_dx_matches_reference() {
        assert_white_constant_texel_run_matches_reference(
            [12.0, 220.0, 40.0, 73.2],
            [2.5, -3.0, 1.25, 0.01],
            7,
            9,
            (2, 3),
            AlphaRunKind::Constant(73),
            ColorRunKind::Variable,
        );
    }

    #[test]
    fn white_constant_texel_len_one_matches_reference() {
        assert_white_constant_texel_run_matches_reference(
            [12.0, 220.0, 40.0, 41.2],
            [2.5, -3.0, 1.25, 12.0],
            6,
            1,
            (2, 3),
            AlphaRunKind::Constant(113),
            ColorRunKind::Constant([27, 202, 48, 113]),
        );
    }

    #[test]
    fn white_constant_texel_threshold_and_clamp_edges_match_reference() {
        assert_white_constant_texel_run_matches_reference(
            [12.0, 220.0, 40.0, -3.0],
            [2.5, -3.0, 1.25, 0.2],
            0,
            10,
            (2, 3),
            AlphaRunKind::Constant(0),
            ColorRunKind::Variable,
        );
        assert_white_constant_texel_run_matches_reference(
            [12.0, 220.0, 40.0, 255.6],
            [2.5, -3.0, 1.25, 0.2],
            0,
            10,
            (3, 3),
            AlphaRunKind::Constant(u8::MAX),
            ColorRunKind::Variable,
        );
    }

    #[test]
    fn white_constant_texel_constant_alpha_detection_rejects_invalid_endpoints() {
        assert_eq!(
            white_constant_texel_constant_run_alpha([0.0; 4], [0.0; 4], 0, 0),
            None
        );
        assert_eq!(
            white_constant_texel_constant_run_alpha([0.0, 0.0, 0.0, f32::INFINITY], [0.0; 4], 0, 1,),
            None
        );
        assert_eq!(
            white_constant_texel_constant_run_alpha([0.0; 4], [0.0, 0.0, 0.0, f32::NAN], 0, 1,),
            None
        );
        assert_eq!(
            white_constant_texel_constant_run_alpha([0.0; 4], [0.0; 4], usize::MAX - 1, 3),
            None
        );
        assert_eq!(
            white_constant_texel_constant_run_color(
                [0.0; 4],
                [0.0; 4],
                WhiteConstantTexelRun {
                    start_dx: usize::MAX - 1,
                    len: 3,
                    pixel_offset: 0,
                },
                0,
            ),
            None
        );
    }

    #[test]
    fn varying_rgb_white_texel_triangles_match_sampled_reference_and_classify_alpha_runs() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let mut optimized = test_surface(32, 24);
        let mut reference = test_surface(32, 24);
        let mut stats = RasterStats::default();

        let constant_alpha_vertices = [
            test_vertex(egui::pos2(2.0, 2.0), [40, 20, 10, 128]),
            test_vertex(egui::pos2(22.0, 3.0), [220, 80, 30, 128]),
            test_vertex(egui::pos2(5.0, 15.0), [80, 200, 160, 128]),
        ];
        rasterize_white_triangle_pair(
            &mut optimized,
            &mut reference,
            &texture,
            &constant_alpha_vertices,
            Some(&mut stats),
        );

        let variable_alpha_vertices = [
            test_vertex(egui::pos2(11.0, 7.0), [30, 210, 80, 16]),
            test_vertex(egui::pos2(29.0, 8.0), [180, 40, 220, 248]),
            test_vertex(egui::pos2(15.0, 22.0), [90, 160, 40, 96]),
        ];
        rasterize_white_triangle_pair(
            &mut optimized,
            &mut reference,
            &texture,
            &variable_alpha_vertices,
            Some(&mut stats),
        );

        assert_eq!(optimized.pixels, reference.pixels);
        assert!(stats.constant_texel_textured_triangle_white_constant_alpha_run_calls > 0);
        assert!(stats.constant_texel_textured_triangle_white_constant_alpha_run_px > 0);
        assert!(stats.constant_texel_textured_triangle_white_variable_color_run_calls > 0);
        assert!(stats.constant_texel_textured_triangle_white_variable_color_run_px > 0);
        assert!(stats.constant_texel_textured_triangle_white_variable_alpha_run_calls > 0);
        assert!(stats.constant_texel_textured_triangle_white_variable_alpha_run_px > 0);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum AlphaRunKind {
        Constant(u8),
        Variable,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ColorRunKind {
        Constant([u8; 4]),
        Variable,
    }

    fn assert_white_constant_texel_run_matches_reference(
        row_color: [f32; 4],
        color_step: [f32; 4],
        start_dx: usize,
        len: usize,
        pos: (usize, usize),
        expected_kind: AlphaRunKind,
        expected_color_kind: ColorRunKind,
    ) {
        let pixel_offset = test_surface(1, 1).row_offset(0);
        let run = WhiteConstantTexelRun {
            start_dx,
            len,
            pixel_offset: pixel_offset + (pos.0 * 20 + pos.1) * 4,
        };
        let mut actual = test_surface(20, 7);
        let mut actual_no_stats = test_surface(20, 7);
        let mut reference = test_surface(20, 7);
        let mut stats = RasterStats::default();
        let mut scalar_stats = RasterStats::default();

        emit_white_constant_texel_run_with_stats(
            &mut actual,
            row_color,
            color_step,
            start_dx,
            len,
            run.pixel_offset,
            &mut stats,
        );
        emit_white_constant_texel_run_no_stats(
            &mut actual_no_stats,
            row_color,
            color_step,
            start_dx,
            len,
            run.pixel_offset,
        );
        emit_white_constant_texel_run_reference(
            &mut reference,
            row_color,
            color_step,
            run,
            Some(&mut scalar_stats),
        );

        assert_eq!(actual.pixels, reference.pixels);
        assert_eq!(actual_no_stats.pixels, reference.pixels);
        assert_eq!(
            white_constant_texel_constant_run_alpha(row_color, color_step, start_dx, len),
            expected_kind.alpha()
        );
        assert_eq!(
            white_constant_texel_constant_run_color(
                row_color,
                color_step,
                run,
                expected_kind.alpha().unwrap_or(0)
            ),
            expected_color_kind.color()
        );
        assert_white_constant_texel_classification_stats(
            &stats,
            expected_kind,
            expected_color_kind,
            len,
        );
        assert_white_constant_texel_alpha_stats_match(&stats, &scalar_stats);
    }

    impl AlphaRunKind {
        const fn alpha(self) -> Option<u8> {
            match self {
                Self::Constant(alpha) => Some(alpha),
                Self::Variable => None,
            }
        }
    }

    impl ColorRunKind {
        const fn color(self) -> Option<[u8; 4]> {
            match self {
                Self::Constant(color) => Some(color),
                Self::Variable => None,
            }
        }
    }

    fn assert_white_constant_texel_classification_stats(
        stats: &RasterStats,
        expected_kind: AlphaRunKind,
        expected_color_kind: ColorRunKind,
        len: usize,
    ) {
        match expected_kind {
            AlphaRunKind::Constant(_) => {
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_alpha_run_calls,
                    1
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_alpha_run_px,
                    len
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_alpha_run_calls,
                    0
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_alpha_run_px,
                    0
                );
                assert_white_constant_texel_color_classification_stats(
                    stats,
                    expected_color_kind,
                    len,
                );
            }
            AlphaRunKind::Variable => {
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_alpha_run_calls,
                    0
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_alpha_run_px,
                    0
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_alpha_run_calls,
                    1
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_alpha_run_px,
                    len
                );
                assert_white_constant_texel_color_classification_stats(
                    stats,
                    ColorRunKind::Variable,
                    0,
                );
            }
        }
    }

    fn assert_white_constant_texel_color_classification_stats(
        stats: &RasterStats,
        expected_kind: ColorRunKind,
        len: usize,
    ) {
        match expected_kind {
            ColorRunKind::Constant(_) => {
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_color_run_calls,
                    1
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_color_run_px,
                    len
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_color_run_calls,
                    0
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_color_run_px,
                    0
                );
            }
            ColorRunKind::Variable => {
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_color_run_calls,
                    0
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_constant_color_run_px,
                    0
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_color_run_calls,
                    usize::from(len > 0)
                );
                assert_eq!(
                    stats.constant_texel_textured_triangle_white_variable_color_run_px,
                    len
                );
            }
        }
    }

    fn assert_white_constant_texel_alpha_stats_match(
        stats: &RasterStats,
        scalar_stats: &RasterStats,
    ) {
        assert_eq!(
            stats.textured_triangle_covered_px,
            scalar_stats.textured_triangle_covered_px
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_covered_px,
            scalar_stats.constant_texel_textured_triangle_covered_px,
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_texel_covered_px,
            scalar_stats.constant_texel_textured_triangle_white_texel_covered_px,
        );
        assert_eq!(stats.opaque_px, scalar_stats.opaque_px);
        assert_eq!(stats.translucent_px, scalar_stats.translucent_px);
        assert_eq!(stats.transparent_px, scalar_stats.transparent_px);
        assert_eq!(
            stats.constant_texel_textured_triangle_opaque_px,
            scalar_stats.constant_texel_textured_triangle_opaque_px,
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_translucent_px,
            scalar_stats.constant_texel_textured_triangle_translucent_px,
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_transparent_px,
            scalar_stats.constant_texel_textured_triangle_transparent_px,
        );
    }

    fn emit_white_constant_texel_run_reference(
        surface: &mut SoftwareSurface,
        row_color: [f32; 4],
        color_step: [f32; 4],
        run: WhiteConstantTexelRun,
        mut stats: Option<&mut RasterStats>,
    ) {
        if let Some(stats) = stats.as_deref_mut() {
            stats.textured_triangle_covered_px += run.len;
            stats.constant_texel_textured_triangle_covered_px += run.len;
            stats.constant_texel_textured_triangle_white_texel_covered_px += run.len;
        }
        let mut pixel_offset = run.pixel_offset;
        for run_dx in 0..run.len {
            let dx = run.start_dx + run_dx;
            let color = white_constant_texel_pixel_color(row_color, color_step, dx);
            if let Some(stats) = stats.as_deref_mut() {
                stats.record_alpha_px(color[3], 1);
                stats.record_constant_texel_alpha_px(color[3], 1);
            }
            match color[3] {
                0 => {}
                u8::MAX => surface.write_opaque_pixel_at_offset(pixel_offset, color),
                _ => surface.blend_translucent_pixel_at_offset(pixel_offset, color),
            }
            pixel_offset += 4;
        }
    }

    fn rasterize_white_triangle_pair(
        optimized: &mut SoftwareSurface,
        reference: &mut SoftwareSurface,
        texture: &TextureImage,
        vertices: &[egui::epaint::Vertex; 3],
        stats: Option<&mut RasterStats>,
    ) {
        let [v0, v1, v2] = vertices;
        let triangle = TriangleVertices { v0, v1, v2 };
        let bounds = test_triangle_bounds(vertices);
        let area = edge(v0.pos, v1.pos, v2.pos);
        rasterize_constant_texel_textured_triangle(
            optimized,
            triangle,
            bounds,
            area,
            [255, 255, 255, 255],
            stats,
        );
        rasterize_textured_triangle(reference, triangle, texture, bounds, area, None);
    }

    fn test_triangle_bounds(vertices: &[egui::epaint::Vertex; 3]) -> TriangleRasterBounds {
        let min_x = vertices
            .iter()
            .map(|vertex| f32_to_usize_floor_clamped(vertex.pos.x, usize::MAX))
            .min()
            .expect("triangle has vertices");
        let min_y = vertices
            .iter()
            .map(|vertex| f32_to_usize_floor_clamped(vertex.pos.y, usize::MAX))
            .min()
            .expect("triangle has vertices");
        let max_x = vertices
            .iter()
            .map(|vertex| f32_to_usize_ceil_clamped(vertex.pos.x, usize::MAX))
            .max()
            .expect("triangle has vertices");
        let max_y = vertices
            .iter()
            .map(|vertex| f32_to_usize_ceil_clamped(vertex.pos.y, usize::MAX))
            .max()
            .expect("triangle has vertices");
        TriangleRasterBounds {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    fn test_vertex(pos: egui::Pos2, color: [u8; 4]) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos,
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]),
        }
    }

    fn assert_constant_alpha_only_run_matches_reference(
        row_alpha: f32,
        alpha_step: f32,
        alpha_offset: usize,
        len: usize,
        y: usize,
        x: usize,
    ) {
        let run = AlphaOnlyRun {
            alpha_offset,
            len,
            y,
            x,
        };
        let mut actual = test_surface(20, 7);
        let mut actual_no_stats = test_surface(20, 7);
        let mut reference = test_surface(20, 7);
        let mut stats = RasterStats::default();

        emit_white_constant_texel_alpha_only_run_with_stats(
            &mut actual,
            row_alpha,
            alpha_step,
            run,
            &mut stats,
        );
        emit_white_constant_texel_alpha_only_run_no_stats(
            &mut actual_no_stats,
            row_alpha,
            alpha_step,
            run,
        );
        emit_white_constant_texel_alpha_only_run_reference(
            &mut reference,
            row_alpha,
            alpha_step,
            run,
        );

        assert_eq!(actual.pixels, reference.pixels);
        assert_eq!(actual_no_stats.pixels, reference.pixels);
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls,
            1
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px,
            len
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls,
            0
        );
        assert_eq!(
            stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px,
            0
        );
        assert_eq!(stats.textured_triangle_covered_px, len);
        assert_alpha_stats_match_run(&stats, row_alpha, alpha_step, alpha_offset, len);
    }

    fn assert_alpha_stats_match_run(
        stats: &RasterStats,
        row_alpha: f32,
        alpha_step: f32,
        alpha_offset: usize,
        len: usize,
    ) {
        let (transparent, opaque, translucent) =
            alpha_counts(row_alpha, alpha_step, alpha_offset, len);
        assert_eq!(stats.opaque_px, opaque);
        assert_eq!(stats.constant_texel_textured_triangle_opaque_px, opaque);
        assert_eq!(stats.transparent_px, transparent);
        assert_eq!(
            stats.constant_texel_textured_triangle_transparent_px,
            transparent
        );
        assert_eq!(stats.translucent_px, translucent);
        assert_eq!(
            stats.constant_texel_textured_triangle_translucent_px,
            translucent
        );
    }

    fn alpha_counts(
        row_alpha: f32,
        alpha_step: f32,
        alpha_offset: usize,
        len: usize,
    ) -> (usize, usize, usize) {
        let mut transparent = 0;
        let mut opaque = 0;
        let mut translucent = 0;
        for offset in 0..len {
            match white_constant_texel_alpha_only_pixel_alpha(
                row_alpha,
                alpha_step,
                alpha_offset + offset,
            ) {
                0 => transparent += 1,
                u8::MAX => opaque += 1,
                _ => translucent += 1,
            }
        }
        (transparent, opaque, translucent)
    }

    fn emit_white_constant_texel_alpha_only_run_reference(
        surface: &mut SoftwareSurface,
        row_alpha: f32,
        alpha_step: f32,
        run: AlphaOnlyRun,
    ) {
        let mut pixel_offset = surface.row_offset(run.y) + run.x * 4;
        for offset in 0..run.len {
            let alpha = white_constant_texel_alpha_only_pixel_alpha(
                row_alpha,
                alpha_step,
                run.alpha_offset + offset,
            );
            match alpha {
                0 => {}
                u8::MAX => {
                    surface.write_opaque_pixel_at_offset(pixel_offset, [255, 255, 255, alpha]);
                }
                _ => {
                    surface.blend_translucent_pixel_at_offset(pixel_offset, [255, 255, 255, alpha]);
                }
            }
            pixel_offset += 4;
        }
    }

    fn test_surface(width: usize, height: usize) -> SoftwareSurface {
        let mut surface = SoftwareSurface::default();
        surface.resize(width, height).expect("surface resize");
        surface.clear([19, 47, 83, 255]);
        surface
    }
}
