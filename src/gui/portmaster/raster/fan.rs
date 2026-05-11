// SPDX-License-Identifier: GPL-3.0-only

use super::math::{
    edge, edge_covers_pixel, edge_includes_boundary, edge_step_x, f32_to_usize_ceil_clamped,
    f32_to_usize_floor_clamped, same_f32,
};
use super::types::{ClipBounds, TriangleRasterBounds};
use super::{RasterStats, usize_to_f32};
use crate::gui::portmaster::surface::SoftwareSurface;

const SOLID_FAN_PRECOMPUTED_EDGE_BUDGET: usize = 256;

pub(in crate::gui::portmaster) fn rasterize_solid_fan(
    surface: &mut SoftwareSurface,
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    triangle_count: usize,
    color: [u8; 4],
    clip: ClipBounds,
    mut stats: Option<&mut RasterStats>,
) {
    let Some(bounds) = polygon_raster_bounds(vertices, polygon, clip) else {
        return;
    };
    let Some(area_sign) = polygon_area_sign(vertices, polygon) else {
        return;
    };
    if let Some(stats) = &mut stats {
        stats.solid_fan_calls += 1;
        stats.solid_fan_triangles += triangle_count;
    }
    let precomputed_edges = precompute_polygon_edges(vertices, polygon, area_sign);

    for y in bounds.min_y..bounds.max_y {
        let scanline = precomputed_edges.as_ref().map_or_else(
            || polygon_scanline_span(vertices, polygon, bounds, y, area_sign, stats.is_some()),
            |edges| {
                polygon_scanline_span_precomputed(
                    edges,
                    vertices,
                    polygon,
                    bounds,
                    y,
                    area_sign,
                    stats.is_some(),
                )
            },
        );
        let span = if scanline.fell_back {
            if let Some(stats) = &mut stats {
                stats.solid_fan_fallback_rows += 1;
            }
            polygon_fallback_scanline_span(vertices, polygon, bounds, y, area_sign)
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
pub(super) struct PolygonScanlineSpan {
    pub(super) span: Option<(usize, usize)>,
    edge_intersections: usize,
    endpoint_probe_px: usize,
    pub(super) fell_back: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct PrecomputedFanEdge {
    a: egui::Pos2,
    b: egui::Pos2,
    slope_x: f32,
    includes_boundary: bool,
}

struct PrecomputedFanEdges {
    edges: [PrecomputedFanEdge; SOLID_FAN_PRECOMPUTED_EDGE_BUDGET],
    len: usize,
}

impl PrecomputedFanEdges {
    fn as_slice(&self) -> &[PrecomputedFanEdge] {
        &self.edges[..self.len]
    }
}

fn precompute_polygon_edges(
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    area_sign: f32,
) -> Option<PrecomputedFanEdges> {
    if polygon.len() > SOLID_FAN_PRECOMPUTED_EDGE_BUDGET {
        return None;
    }

    let mut edges = [PrecomputedFanEdge::default(); SOLID_FAN_PRECOMPUTED_EDGE_BUDGET];
    for edge_index in 0..polygon.len() {
        let a = vertices[polygon[edge_index]].pos;
        let b = vertices[polygon[(edge_index + 1) % polygon.len()]].pos;
        let slope_x = edge_step_x(a, b) * area_sign;
        if !slope_x.is_finite() {
            return None;
        }
        edges[edge_index] = PrecomputedFanEdge {
            a,
            b,
            slope_x,
            includes_boundary: edge_includes_boundary(a, b, area_sign),
        };
    }
    Some(PrecomputedFanEdges {
        edges,
        len: polygon.len(),
    })
}

fn polygon_raster_bounds(
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    clip: ClipBounds,
) -> Option<TriangleRasterBounds> {
    if polygon.len() < 3 {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for vertex_index in polygon {
        let vertex = &vertices[*vertex_index];
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

fn polygon_area_sign(vertices: &[egui::epaint::Vertex], polygon: &[usize]) -> Option<f32> {
    let mut twice_area = 0.0;
    for edge_index in 0..polygon.len() {
        let a = vertices[polygon[edge_index]].pos;
        let b = vertices[polygon[(edge_index + 1) % polygon.len()]].pos;
        twice_area += a.x.mul_add(b.y, -(a.y * b.x));
    }
    if twice_area.abs() <= f32::EPSILON {
        return None;
    }
    Some(-twice_area.signum())
}

pub(super) fn polygon_scanline_span(
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
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
        let a = vertices[polygon[edge_index]].pos;
        let b = vertices[polygon[(edge_index + 1) % polygon.len()]].pos;
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
    let endpoint_probe_px = polygon_correct_span_endpoints(
        &mut span,
        y,
        vertices,
        polygon,
        area_sign,
        bounds,
        collect_stats,
    );
    PolygonScanlineSpan {
        span: (span.0 < span.1).then_some(span),
        edge_intersections,
        endpoint_probe_px,
        fell_back: false,
    }
}

fn polygon_scanline_span_precomputed(
    edges: &PrecomputedFanEdges,
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    bounds: TriangleRasterBounds,
    y: usize,
    area_sign: f32,
    collect_stats: bool,
) -> PolygonScanlineSpan {
    let pixel_center_y = usize_to_f32(y) + 0.5;
    let mut lower = None;
    let mut upper = None;
    let mut edge_intersections = 0;
    for edge_data in edges.as_slice() {
        let at_origin = edge(edge_data.a, edge_data.b, egui::pos2(0.0, pixel_center_y)) * area_sign;
        if !at_origin.is_finite() {
            return PolygonScanlineSpan {
                fell_back: true,
                ..PolygonScanlineSpan::default()
            };
        }
        if same_f32(edge_data.slope_x, 0.0) {
            if !edge_covers_pixel(at_origin, edge_data.includes_boundary) {
                return PolygonScanlineSpan::default();
            }
            continue;
        }
        let boundary = -at_origin / edge_data.slope_x;
        if !boundary.is_finite() {
            return PolygonScanlineSpan {
                fell_back: true,
                ..PolygonScanlineSpan::default()
            };
        }
        edge_intersections += 1;
        if edge_data.slope_x > 0.0 {
            lower = Some(tighter_lower_bound(
                lower,
                boundary,
                edge_data.includes_boundary,
            ));
        } else {
            upper = Some(tighter_upper_bound(
                upper,
                boundary,
                edge_data.includes_boundary,
            ));
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
    let endpoint_probe_px = polygon_correct_span_endpoints(
        &mut span,
        y,
        vertices,
        polygon,
        area_sign,
        bounds,
        collect_stats,
    );
    PolygonScanlineSpan {
        span: (span.0 < span.1).then_some(span),
        edge_intersections,
        endpoint_probe_px,
        fell_back: false,
    }
}

pub(super) fn polygon_fallback_scanline_span(
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    bounds: TriangleRasterBounds,
    y: usize,
    area_sign: f32,
) -> Option<(usize, usize)> {
    let mut span_start = None;
    let mut span_end = None;
    for x in bounds.min_x..bounds.max_x {
        if polygon_covers_pixel(x, y, vertices, polygon, area_sign) {
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
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    area_sign: f32,
    bounds: TriangleRasterBounds,
    collect_stats: bool,
) -> usize {
    const MAX_ENDPOINT_CORRECTION_PX: usize = 2;
    let mut probe_px = 0;
    let mut correction_px = 0;
    while span.0 < span.1
        && !polygon_endpoint_probe(
            span.0,
            y,
            vertices,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        )
        && correction_px < MAX_ENDPOINT_CORRECTION_PX
    {
        span.0 += 1;
        correction_px += 1;
    }
    if span.0 < span.1
        && !polygon_endpoint_probe(
            span.0,
            y,
            vertices,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        )
    {
        span.1 = span.0;
        return probe_px;
    }
    correction_px = 0;
    while span.0 < span.1
        && !polygon_endpoint_probe(
            span.1 - 1,
            y,
            vertices,
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
            vertices,
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
            vertices,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        );
    }
    if span.1 < bounds.max_x {
        let _ = polygon_endpoint_probe(
            span.1,
            y,
            vertices,
            polygon,
            area_sign,
            collect_stats,
            &mut probe_px,
        );
    }
    probe_px
}

fn polygon_endpoint_probe(
    x: usize,
    y: usize,
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    area_sign: f32,
    collect_stats: bool,
    probe_px: &mut usize,
) -> bool {
    if collect_stats {
        *probe_px += 1;
    }
    polygon_covers_pixel(x, y, vertices, polygon, area_sign)
}

fn polygon_covers_pixel(
    x: usize,
    y: usize,
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    area_sign: f32,
) -> bool {
    let pixel_center = egui::pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
    for edge_index in 0..polygon.len() {
        let a = vertices[polygon[edge_index]].pos;
        let b = vertices[polygon[(edge_index + 1) % polygon.len()]].pos;
        let weight = edge(a, b, pixel_center) * area_sign;
        if !edge_covers_pixel(weight, edge_includes_boundary(a, b, area_sign)) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precomputed_scanline_solver_matches_reference_solver() {
        let cases = [
            vec![
                test_vertex(32.0, 6.0),
                test_vertex(48.0, 10.0),
                test_vertex(58.0, 26.0),
                test_vertex(56.0, 42.0),
                test_vertex(40.0, 56.0),
                test_vertex(22.0, 54.0),
                test_vertex(8.0, 38.0),
                test_vertex(10.0, 20.0),
            ],
            vec![
                test_vertex(0.5, 5.0),
                test_vertex(1.0, 4.4),
                test_vertex(4.0, 4.1),
                test_vertex(8.0, 4.4),
                test_vertex(9.5, 5.0),
                test_vertex(8.0, 5.6),
                test_vertex(4.0, 5.9),
                test_vertex(1.0, 5.6),
            ],
            vec![
                test_vertex(2.25, 1.25),
                test_vertex(11.75, 2.5),
                test_vertex(10.5, 9.25),
                test_vertex(5.5, 11.5),
                test_vertex(1.25, 7.25),
            ],
        ];
        let bounds = TriangleRasterBounds {
            min_x: 0,
            min_y: 0,
            max_x: 64,
            max_y: 64,
        };

        for vertices in cases {
            let polygon: Vec<_> = (0..vertices.len()).collect();
            let area_sign = polygon_area_sign(&vertices, &polygon).expect("polygon has area");
            let precomputed = precompute_polygon_edges(&vertices, &polygon, area_sign)
                .expect("test polygon fits precompute budget");
            for y in bounds.min_y..bounds.max_y {
                assert_eq!(
                    polygon_scanline_span(&vertices, &polygon, bounds, y, area_sign, true),
                    polygon_scanline_span_precomputed(
                        &precomputed,
                        &vertices,
                        &polygon,
                        bounds,
                        y,
                        area_sign,
                        true,
                    ),
                    "scanline mismatch at y={y}",
                );
            }
        }
    }

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::Pos2::ZERO,
            color: egui::Color32::WHITE,
        }
    }
}
