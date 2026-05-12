// SPDX-License-Identifier: GPL-3.0-only

use super::math::{
    edge, edge_covers_pixel, edge_includes_boundary, edge_step_x, f32_to_usize_ceil_clamped,
    f32_to_usize_floor_clamped, same_f32,
};
use super::types::{ClipBounds, TriangleRasterBounds};
use super::{RasterStats, usize_to_f32};
use crate::gui::portmaster::surface::SoftwareSurface;

const SOLID_FAN_PRECOMPUTED_EDGE_BUDGET: usize = 256;
const SOLID_FAN_SPAN_CACHE_ENTRIES: usize = 128;
const SOLID_FAN_SPAN_CACHE_RESIDENT_ROWS: usize = 4096;
const SOLID_FAN_SPAN_CACHE_MAX_ROWS: usize = 512;

pub(in crate::gui::portmaster) fn rasterize_solid_fan(
    surface: &mut SoftwareSurface,
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
    triangle_count: usize,
    color: [u8; 4],
    clip: ClipBounds,
    mut stats: Option<&mut RasterStats>,
) {
    rasterize_solid_fan_with_cache(
        surface,
        SolidFanRasterParams {
            vertices,
            polygon,
            triangle_count,
            color,
            clip,
        },
        &mut stats,
        None,
    );
}

pub(in crate::gui::portmaster) fn rasterize_solid_fan_with_cache(
    surface: &mut SoftwareSurface,
    params: SolidFanRasterParams<'_>,
    stats: &mut Option<&mut RasterStats>,
    cache: Option<&mut SolidFanSpanCache>,
) {
    let SolidFanRasterParams {
        vertices,
        polygon,
        triangle_count,
        color,
        clip,
    } = params;
    let Some(bounds) = polygon_raster_bounds(vertices, polygon, clip) else {
        return;
    };
    let Some(area_sign) = polygon_area_sign(vertices, polygon) else {
        return;
    };
    if let Some(stats) = stats {
        stats.solid_fan_calls += 1;
        stats.solid_fan_triangles += triangle_count;
    }

    let mut cache = cache;
    let cache_key = solid_fan_span_cache_key(cache.is_some(), params, bounds, stats);
    if let (Some(cache), Some(cache_key)) = (cache.as_mut(), cache_key.as_ref()) {
        if let Some(entry) = cache.get(cache_key) {
            if let Some(stats) = stats {
                stats.solid_fan_span_cache_hits += 1;
                stats.solid_fan_span_cache_hit_rows += entry.spans.len();
            }
            replay_solid_fan_spans(surface, color, entry, stats);
            return;
        }
        if let Some(stats) = stats {
            stats.solid_fan_span_cache_misses += 1;
        }
    }

    let cache_key_available = cache_key.is_some();
    let cache_spans = rasterize_solid_fan_uncached(
        surface,
        params,
        stats,
        bounds,
        area_sign,
        cache_key_available,
    );

    if let (Some(cache), Some(key), Some(spans)) = (cache, cache_key, cache_spans) {
        if let Some(stats) = stats {
            stats.solid_fan_span_cache_stored_rows += spans.len();
        }
        cache.insert(SolidFanSpanCacheEntry { key, bounds, spans });
    }
}

fn solid_fan_span_cache_key(
    cache_enabled: bool,
    params: SolidFanRasterParams<'_>,
    bounds: TriangleRasterBounds,
    stats: &mut Option<&mut RasterStats>,
) -> Option<SolidFanSpanCacheKey> {
    if !cache_enabled {
        return None;
    }
    match solid_fan_span_cache_key_eligibility(params.polygon, bounds) {
        SolidFanSpanCacheKeyEligibility::Eligible => {}
        SolidFanSpanCacheKeyEligibility::TooManyRows => {
            if let Some(stats) = stats {
                stats.solid_fan_span_cache_rejected_too_many_rows += 1;
            }
            return None;
        }
        SolidFanSpanCacheKeyEligibility::TooManyEdges => return None,
    }
    SolidFanSpanCacheKey::new(
        params.vertices,
        params.polygon,
        params.triangle_count,
        params.clip,
        bounds,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SolidFanSpanCacheKeyEligibility {
    Eligible,
    TooManyRows,
    TooManyEdges,
}

fn solid_fan_span_cache_key_eligibility(
    polygon: &[usize],
    bounds: TriangleRasterBounds,
) -> SolidFanSpanCacheKeyEligibility {
    if bounds.max_y - bounds.min_y > SOLID_FAN_SPAN_CACHE_MAX_ROWS {
        return SolidFanSpanCacheKeyEligibility::TooManyRows;
    }
    if polygon.len() > SOLID_FAN_PRECOMPUTED_EDGE_BUDGET {
        return SolidFanSpanCacheKeyEligibility::TooManyEdges;
    }
    SolidFanSpanCacheKeyEligibility::Eligible
}

fn solid_fan_span_cache_key_eligible(polygon: &[usize], bounds: TriangleRasterBounds) -> bool {
    solid_fan_span_cache_key_eligibility(polygon, bounds)
        == SolidFanSpanCacheKeyEligibility::Eligible
}

fn solid_fan_span_cache_key_from_eligible(
    params: SolidFanRasterParams<'_>,
    bounds: TriangleRasterBounds,
) -> Option<SolidFanSpanCacheKey> {
    if !solid_fan_span_cache_key_eligible(params.polygon, bounds) {
        return None;
    }
    SolidFanSpanCacheKey::new(
        params.vertices,
        params.polygon,
        params.triangle_count,
        params.clip,
        bounds,
    )
}

fn rasterize_solid_fan_uncached(
    surface: &mut SoftwareSurface,
    params: SolidFanRasterParams<'_>,
    stats: &mut Option<&mut RasterStats>,
    bounds: TriangleRasterBounds,
    area_sign: f32,
    cache_key_available: bool,
) -> Option<Vec<Option<(usize, usize)>>> {
    let precomputed_edges = precompute_polygon_edges(params.vertices, params.polygon, area_sign);
    record_solid_fan_precompute_stats(stats, &precomputed_edges);
    let mut cache_spans =
        solid_fan_cache_span_storage(bounds, precomputed_edges.is_ok(), cache_key_available);

    for y in bounds.min_y..bounds.max_y {
        let scanline = solid_fan_scanline(params, bounds, y, area_sign, &precomputed_edges, stats);
        let span = if scanline.fell_back {
            cache_spans = None;
            if let Some(stats) = stats {
                stats.solid_fan_fallback_rows += 1;
            }
            polygon_fallback_scanline_span(params.vertices, params.polygon, bounds, y, area_sign)
        } else {
            record_solid_fan_precomputed_row(stats, precomputed_edges.is_ok());
            scanline.span
        };
        if let Some(spans) = &mut cache_spans {
            spans.push(span);
        }
        rasterize_solid_fan_span(surface, params.color, y, span, stats);
        record_solid_fan_scanline_stats(stats, scanline);
    }
    cache_spans
}

fn record_solid_fan_precompute_stats(
    stats: &mut Option<&mut RasterStats>,
    precomputed_edges: &Result<PrecomputedFanEdges, PrecomputeFanEdgesError>,
) {
    if let Some(stats) = stats {
        stats.solid_fan_edge_precompute_calls += 1;
        match precomputed_edges {
            Ok(edges) => stats.solid_fan_edge_precompute_edges += edges.len,
            Err(PrecomputeFanEdgesError::Budget) => {
                stats.solid_fan_edge_precompute_fallback_budget += 1;
            }
            Err(PrecomputeFanEdgesError::NonFinite) => {
                stats.solid_fan_edge_precompute_fallback_non_finite += 1;
            }
        }
    }
}

fn solid_fan_cache_span_storage(
    bounds: TriangleRasterBounds,
    precomputed: bool,
    cache_key_available: bool,
) -> Option<Vec<Option<(usize, usize)>>> {
    if !cache_key_available {
        return None;
    }
    let row_count = bounds.max_y - bounds.min_y;
    if precomputed && row_count <= SOLID_FAN_SPAN_CACHE_MAX_ROWS {
        return Some(Vec::with_capacity(row_count));
    }
    None
}

fn solid_fan_scanline(
    params: SolidFanRasterParams<'_>,
    bounds: TriangleRasterBounds,
    y: usize,
    area_sign: f32,
    precomputed_edges: &Result<PrecomputedFanEdges, PrecomputeFanEdgesError>,
    stats: &mut Option<&mut RasterStats>,
) -> PolygonScanlineSpan {
    let collect_stats = stats.is_some();
    if let Ok(edges) = precomputed_edges {
        return polygon_scanline_span_precomputed(
            edges,
            params.vertices,
            params.polygon,
            bounds,
            y,
            area_sign,
            collect_stats,
        );
    }
    if let Some(stats) = stats {
        stats.solid_fan_edge_precompute_old_solver_rows += 1;
    }
    polygon_scanline_span(
        params.vertices,
        params.polygon,
        bounds,
        y,
        area_sign,
        collect_stats,
    )
}

fn record_solid_fan_precomputed_row(stats: &mut Option<&mut RasterStats>, precomputed: bool) {
    if precomputed && let Some(stats) = stats {
        stats.solid_fan_edge_precompute_used_rows += 1;
    }
}

fn rasterize_solid_fan_span(
    surface: &mut SoftwareSurface,
    color: [u8; 4],
    y: usize,
    span: Option<(usize, usize)>,
    stats: &mut Option<&mut RasterStats>,
) {
    let Some((span_start, span_end)) = span else {
        return;
    };
    surface.blend_span(y, span_start, span_end, color);
    if let Some(stats) = stats {
        let px = span_end - span_start;
        stats.solid_fan_rows += 1;
        stats.solid_fan_px += px;
        stats.record_alpha_px(color[3], px);
    }
}

#[derive(Clone, Copy)]
pub(in crate::gui::portmaster) struct SolidFanRasterParams<'a> {
    pub(in crate::gui::portmaster) vertices: &'a [egui::epaint::Vertex],
    pub(in crate::gui::portmaster) polygon: &'a [usize],
    pub(in crate::gui::portmaster) triangle_count: usize,
    pub(in crate::gui::portmaster) color: [u8; 4],
    pub(in crate::gui::portmaster) clip: ClipBounds,
}

#[derive(Debug, Default)]
pub(in crate::gui::portmaster) struct SolidFanSpanCache {
    entries: Vec<SolidFanSpanCacheEntry>,
    resident_rows: usize,
    total_evictions: usize,
    row_budget_evictions: usize,
}

impl SolidFanSpanCache {
    fn get(&self, key: &SolidFanSpanCacheKey) -> Option<&SolidFanSpanCacheEntry> {
        self.entries.iter().find(|entry| &entry.key == key)
    }

    fn insert(&mut self, entry: SolidFanSpanCacheEntry) {
        self.resident_rows = self.resident_rows.saturating_add(entry.resident_rows());
        self.entries.push(entry);
        while self.entries.len() > SOLID_FAN_SPAN_CACHE_ENTRIES
            || self.resident_rows > SOLID_FAN_SPAN_CACHE_RESIDENT_ROWS
        {
            let row_budget_eviction = self.resident_rows > SOLID_FAN_SPAN_CACHE_RESIDENT_ROWS;
            let removed = self.entries.remove(0);
            self.resident_rows = self.resident_rows.saturating_sub(removed.resident_rows());
            self.total_evictions += 1;
            if row_budget_eviction {
                self.row_budget_evictions += 1;
            }
        }
    }

    pub(in crate::gui::portmaster) fn record_stats(&self, stats: &mut RasterStats) {
        stats.solid_fan_span_cache_resident_entries = self.entries.len();
        stats.solid_fan_span_cache_resident_rows = self.resident_rows;
        stats.solid_fan_span_cache_total_evictions = self.total_evictions;
        stats.solid_fan_span_cache_row_budget_evictions = self.row_budget_evictions;
    }
}

#[derive(Debug)]
struct SolidFanSpanCacheEntry {
    key: SolidFanSpanCacheKey,
    bounds: TriangleRasterBounds,
    spans: Vec<Option<(usize, usize)>>,
}

impl SolidFanSpanCacheEntry {
    fn resident_rows(&self) -> usize {
        self.spans.len()
    }
}

#[derive(Debug, Eq, PartialEq)]
struct SolidFanSpanCacheKey {
    clip: ClipBounds,
    bounds: TriangleRasterBounds,
    triangle_count: usize,
    positions: Vec<(u32, u32)>,
}

impl SolidFanSpanCacheKey {
    fn new(
        vertices: &[egui::epaint::Vertex],
        polygon: &[usize],
        triangle_count: usize,
        clip: ClipBounds,
        bounds: TriangleRasterBounds,
    ) -> Option<Self> {
        let mut positions = Vec::with_capacity(polygon.len());
        for vertex_index in polygon {
            let pos = vertices.get(*vertex_index)?.pos;
            if !pos.x.is_finite() || !pos.y.is_finite() {
                return None;
            }
            positions.push((normalized_f32_bits(pos.x), normalized_f32_bits(pos.y)));
        }
        Some(Self {
            clip,
            bounds,
            triangle_count,
            positions,
        })
    }
}

fn normalized_f32_bits(value: f32) -> u32 {
    if same_f32(value, 0.0) {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

fn replay_solid_fan_spans(
    surface: &mut SoftwareSurface,
    color: [u8; 4],
    entry: &SolidFanSpanCacheEntry,
    stats: &mut Option<&mut RasterStats>,
) {
    for (row_offset, span) in entry.spans.iter().enumerate() {
        let Some((span_start, span_end)) = *span else {
            continue;
        };
        surface.blend_span(entry.bounds.min_y + row_offset, span_start, span_end, color);
        let px = span_end - span_start;
        if let Some(stats) = stats {
            stats.solid_fan_rows += 1;
            stats.solid_fan_px += px;
            stats.solid_fan_span_cache_hit_px += px;
            stats.record_alpha_px(color[3], px);
        }
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
) -> Result<PrecomputedFanEdges, PrecomputeFanEdgesError> {
    if polygon.len() > SOLID_FAN_PRECOMPUTED_EDGE_BUDGET {
        return Err(PrecomputeFanEdgesError::Budget);
    }

    let mut edges = [PrecomputedFanEdge::default(); SOLID_FAN_PRECOMPUTED_EDGE_BUDGET];
    for edge_index in 0..polygon.len() {
        let a = vertices[polygon[edge_index]].pos;
        let b = vertices[polygon[(edge_index + 1) % polygon.len()]].pos;
        let slope_x = edge_step_x(a, b) * area_sign;
        if !slope_x.is_finite() {
            return Err(PrecomputeFanEdgesError::NonFinite);
        }
        edges[edge_index] = PrecomputedFanEdge {
            a,
            b,
            slope_x,
            includes_boundary: edge_includes_boundary(a, b, area_sign),
        };
    }
    Ok(PrecomputedFanEdges {
        edges,
        len: polygon.len(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrecomputeFanEdgesError {
    Budget,
    NonFinite,
}

pub(in crate::gui::portmaster) fn polygon_raster_bounds(
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

    #[test]
    fn span_cache_key_is_gated_by_explicit_budgets() {
        let bounds = TriangleRasterBounds {
            min_x: 0,
            min_y: 0,
            max_x: 8,
            max_y: 8,
        };
        let tall_bounds = TriangleRasterBounds {
            max_y: SOLID_FAN_SPAN_CACHE_MAX_ROWS + 1,
            ..bounds
        };
        let over_budget_polygon: Vec<_> = (0..=SOLID_FAN_PRECOMPUTED_EDGE_BUDGET).collect();
        let params = SolidFanRasterParams {
            vertices: &[],
            polygon: &over_budget_polygon,
            triangle_count: over_budget_polygon.len() - 2,
            color: [255, 255, 255, 255],
            clip: ClipBounds {
                min_x: 0,
                min_y: 0,
                max_x: 8,
                max_y: 8,
            },
        };
        let mut stats = RasterStats::default();

        assert!(solid_fan_span_cache_key(true, params, bounds, &mut Some(&mut stats)).is_none());
        assert_eq!(stats.solid_fan_span_cache_rejected_too_many_rows, 0);

        assert!(
            solid_fan_span_cache_key(true, params, tall_bounds, &mut Some(&mut stats)).is_none()
        );
        assert_eq!(stats.solid_fan_span_cache_rejected_too_many_rows, 1);

        assert!(
            solid_fan_span_cache_key(false, params, tall_bounds, &mut Some(&mut stats)).is_none()
        );
        assert_eq!(stats.solid_fan_span_cache_rejected_too_many_rows, 1);
    }

    #[test]
    fn solid_fan_span_cache_evicts_oldest_entry_at_entry_limit() {
        let mut cache = SolidFanSpanCache::default();

        for index in 0..=SOLID_FAN_SPAN_CACHE_ENTRIES {
            cache.insert(test_span_cache_entry(index, 1));
        }

        assert_eq!(cache.entries.len(), SOLID_FAN_SPAN_CACHE_ENTRIES);
        assert_eq!(cache.resident_rows, SOLID_FAN_SPAN_CACHE_ENTRIES);
        assert_eq!(cache.total_evictions, 1);
        assert_eq!(cache.row_budget_evictions, 0);
        assert!(cache.get(&test_span_cache_key(0)).is_none());
        assert!(cache.get(&test_span_cache_key(1)).is_some());
        assert!(
            cache
                .get(&test_span_cache_key(SOLID_FAN_SPAN_CACHE_ENTRIES))
                .is_some()
        );
    }

    #[test]
    fn solid_fan_span_cache_evicts_oldest_entries_at_row_budget() {
        let mut cache = SolidFanSpanCache::default();

        for index in 0..9 {
            cache.insert(test_span_cache_entry(index, SOLID_FAN_SPAN_CACHE_MAX_ROWS));
        }

        assert_eq!(cache.entries.len(), 8);
        assert_eq!(cache.resident_rows, SOLID_FAN_SPAN_CACHE_RESIDENT_ROWS);
        assert_eq!(cache.total_evictions, 1);
        assert_eq!(cache.row_budget_evictions, 1);
        assert!(cache.get(&test_span_cache_key(0)).is_none());
        assert!(cache.get(&test_span_cache_key(1)).is_some());
    }

    #[test]
    fn solid_fan_span_cache_does_not_retain_oversize_entry() {
        let mut cache = SolidFanSpanCache::default();

        cache.insert(test_span_cache_entry(
            0,
            SOLID_FAN_SPAN_CACHE_RESIDENT_ROWS + 1,
        ));

        assert!(cache.entries.is_empty());
        assert_eq!(cache.resident_rows, 0);
        assert_eq!(cache.total_evictions, 1);
        assert_eq!(cache.row_budget_evictions, 1);
        assert!(cache.get(&test_span_cache_key(0)).is_none());
    }

    fn test_span_cache_entry(index: usize, rows: usize) -> SolidFanSpanCacheEntry {
        SolidFanSpanCacheEntry {
            key: test_span_cache_key(index),
            bounds: TriangleRasterBounds {
                min_x: 0,
                min_y: 0,
                max_x: 1,
                max_y: rows,
            },
            spans: vec![None; rows],
        }
    }

    fn test_span_cache_key(index: usize) -> SolidFanSpanCacheKey {
        SolidFanSpanCacheKey {
            clip: ClipBounds {
                min_x: 0,
                min_y: 0,
                max_x: 1,
                max_y: 1,
            },
            bounds: TriangleRasterBounds {
                min_x: 0,
                min_y: 0,
                max_x: 1,
                max_y: 1,
            },
            triangle_count: 1,
            positions: vec![(u32::try_from(index).expect("test index fits u32"), 0)],
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
