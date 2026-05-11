// SPDX-License-Identifier: GPL-3.0-only

use std::time::Instant;

use super::coverage::{
    TriangleBoundaryIncludes, TriangleCoverage, TriangleRowSearch, triangle_hint_x_range,
    triangle_row_endpoints, triangle_scanline_x_range,
};
use super::math::{color_to_array, edge_includes_boundary, modulate_color};
use super::sampling::{nearest_texel, texel_color};
use super::types::{SolidTriangleColorDecision, TriangleRasterBounds};
use super::{
    RasterStats, TRIANGLE_SCANLINE_NARROWING_MIN_AREA, TriangleVertices, duration_as_us,
    triangle_positions, usize_to_f32,
};
use crate::gui::portmaster::{surface::SoftwareSurface, texture::TextureImage};

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

pub(super) fn rasterize_solid_triangle(
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

pub(in crate::gui::portmaster) fn solid_triangle_color_decision(
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

pub(super) fn solid_triangle_color(
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
