// SPDX-License-Identifier: GPL-3.0-only

use super::TriangleVertices;
use super::math::{
    edge, edge_covers_pixel, edge_step_x, f32_to_usize_ceil_clamped, f32_to_usize_floor_clamped,
    same_f32, usize_to_f32,
};
use super::types::TriangleRasterBounds;

const TRIANGLE_SCANLINE_NARROWING_GUARD_PX: usize = 2;

#[derive(Clone, Copy)]
pub(super) struct TriangleBoundaryIncludes {
    pub(super) edge0: bool,
    pub(super) edge1: bool,
    pub(super) edge2: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TriangleCoverage {
    pub(super) inv_area: f32,
    pub(super) includes_boundary: TriangleBoundaryIncludes,
}

#[derive(Clone, Copy)]
pub(super) struct TriangleRowSearch<'a> {
    pub(super) vertices: TriangleVertices<'a>,
    pub(super) coverage: TriangleCoverage,
    pub(super) y: usize,
    pub(super) candidate_start_x: usize,
    pub(super) candidate_end_x: usize,
    pub(super) probe_start_x: usize,
    pub(super) probe_end_x: usize,
    pub(super) hinted: bool,
    pub(super) collect_stats: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TriangleRowStateSearch {
    pub(super) coverage: TriangleCoverage,
    pub(super) row_start_edges: (f32, f32, f32),
    pub(super) x_steps: (f32, f32, f32),
    pub(super) candidate_start_x: usize,
    pub(super) candidate_end_x: usize,
    pub(super) collect_stats: bool,
}

pub(super) struct TriangleRowEndpoints {
    pub(super) span: Option<(usize, usize)>,
    pub(super) endpoint_probe_px: usize,
    pub(super) hint_probe_px: usize,
    pub(super) canary_probe_px: usize,
    pub(super) fallback_probe_px: usize,
    pub(super) direct_probe_px: usize,
    pub(super) fell_back: bool,
}

pub(super) fn triangle_row_endpoints(search: TriangleRowSearch<'_>) -> TriangleRowEndpoints {
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

pub(super) fn triangle_row_state_endpoints(search: TriangleRowStateSearch) -> TriangleRowEndpoints {
    let (span_start, first_probe_px) = triangle_row_state_first_covered_x(search);
    let Some(span_start) = span_start else {
        return TriangleRowEndpoints {
            span: None,
            endpoint_probe_px: first_probe_px,
            hint_probe_px: 0,
            canary_probe_px: 0,
            fallback_probe_px: 0,
            direct_probe_px: first_probe_px,
            fell_back: false,
        };
    };

    let (last_x, last_probe_px) = triangle_row_state_last_covered_x(search);
    let span_end = last_x.map_or(span_start + 1, |last_x| last_x.max(span_start) + 1);
    let endpoint_probe_px = first_probe_px + last_probe_px;
    TriangleRowEndpoints {
        span: Some((span_start, span_end)),
        endpoint_probe_px,
        hint_probe_px: 0,
        canary_probe_px: 0,
        fallback_probe_px: 0,
        direct_probe_px: endpoint_probe_px,
        fell_back: false,
    }
}

fn triangle_row_state_first_covered_x(search: TriangleRowStateSearch) -> (Option<usize>, usize) {
    let (mut edge0, mut edge1, mut edge2) = search.row_start_edges;
    let (step0, step1, step2) = search.x_steps;
    let mut probe_px = 0;
    for x in search.candidate_start_x..search.candidate_end_x {
        if search.collect_stats {
            probe_px += 1;
        }
        if triangle_row_state_covers_pixel(search.coverage, edge0, edge1, edge2) {
            return (Some(x), probe_px);
        }
        edge0 += step0;
        edge1 += step1;
        edge2 += step2;
    }
    (None, probe_px)
}

fn triangle_row_state_last_covered_x(search: TriangleRowStateSearch) -> (Option<usize>, usize) {
    let Some(last_candidate_x) = search.candidate_end_x.checked_sub(1) else {
        return (None, 0);
    };
    if last_candidate_x < search.candidate_start_x {
        return (None, 0);
    }

    let dx = usize_to_f32(last_candidate_x - search.candidate_start_x);
    let (step0, step1, step2) = search.x_steps;
    let (row_edge0, row_edge1, row_edge2) = search.row_start_edges;
    let mut edge0 = row_edge0 + step0 * dx;
    let mut edge1 = row_edge1 + step1 * dx;
    let mut edge2 = row_edge2 + step2 * dx;
    let mut probe_px = 0;
    for x in (search.candidate_start_x..search.candidate_end_x).rev() {
        if search.collect_stats {
            probe_px += 1;
        }
        if triangle_row_state_covers_pixel(search.coverage, edge0, edge1, edge2) {
            return (Some(x), probe_px);
        }
        edge0 -= step0;
        edge1 -= step1;
        edge2 -= step2;
    }
    (None, probe_px)
}

fn triangle_row_state_covers_pixel(
    coverage: TriangleCoverage,
    edge0: f32,
    edge1: f32,
    edge2: f32,
) -> bool {
    edge_covers_pixel(edge0 * coverage.inv_area, coverage.includes_boundary.edge0)
        && edge_covers_pixel(edge1 * coverage.inv_area, coverage.includes_boundary.edge1)
        && edge_covers_pixel(edge2 * coverage.inv_area, coverage.includes_boundary.edge2)
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

pub(super) fn triangle_hint_x_range(
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

pub(super) fn triangle_scanline_x_range(
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
