// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::time::Instant;

use super::RasterInstrumentation;
use crate::gui::portmaster::raster::{
    ClipBounds, RasterStats, SolidTriangleColorDecision, TexturedQuadFastPathRejection,
    TriangleClassification, TriangleRasterBounds, TriangleScanWorkEstimate, TriangleTexelSample,
    classify_triangle, estimate_triangle_scan_work, is_axis_aligned_quad,
    solid_triangle_color_decision, triangle_nearest_texel_sample, triangle_raster_bounds,
};
use crate::gui::portmaster::texture::TextureImage;

#[derive(Debug, Default)]
pub(super) struct RasterTimings {
    pub(super) quad_window_probe: u128,
    pub(super) solid_quad: u128,
    pub(super) solid_quad_reject: u128,
    pub(super) textured_quad: u128,
    pub(super) textured_quad_reject: u128,
    pub(super) solid_fan_probe: u128,
    pub(super) solid_fan_accepted_probe: u128,
    pub(super) solid_fan_rejected_probe: u128,
    pub(super) solid_fan_raster: u128,
    pub(super) generic_solid_triangle: u128,
    pub(super) generic_textured_triangle: u128,
    pub(super) generic_degenerate_triangle: u128,
}

impl RasterTimings {
    fn record_solid_quad_reject(&mut self, elapsed_us: u128) {
        self.solid_quad_reject += elapsed_us;
    }

    fn record_textured_quad_reject(&mut self, elapsed_us: u128) {
        self.textured_quad_reject += elapsed_us;
    }

    pub(super) fn record_generic_triangle(
        &mut self,
        classification: TriangleClassification,
        elapsed_us: u128,
    ) {
        match classification {
            TriangleClassification::Degenerate => self.generic_degenerate_triangle += elapsed_us,
            TriangleClassification::Solid => self.generic_solid_triangle += elapsed_us,
            TriangleClassification::Textured => self.generic_textured_triangle += elapsed_us,
        }
    }

    pub(super) fn record_solid_fan_probe(&mut self, accepted: bool, elapsed_us: u128) {
        self.solid_fan_probe += elapsed_us;
        if accepted {
            self.solid_fan_accepted_probe += elapsed_us;
        } else {
            self.solid_fan_rejected_probe += elapsed_us;
        }
    }

    pub(super) fn log_line(&self) -> String {
        format!(
            "software renderer raster_timings quad_window_probe_us={} solid_quad_us={} solid_quad_reject_us={} textured_quad_us={} textured_quad_reject_us={} solid_fan_probe_us={} solid_fan_accepted_probe_us={} solid_fan_rejected_probe_us={} solid_fan_raster_us={} generic_solid_triangle_us={} generic_textured_triangle_us={} generic_degenerate_triangle_us={}",
            self.quad_window_probe,
            self.solid_quad,
            self.solid_quad_reject,
            self.textured_quad,
            self.textured_quad_reject,
            self.solid_fan_probe,
            self.solid_fan_accepted_probe,
            self.solid_fan_rejected_probe,
            self.solid_fan_raster,
            self.generic_solid_triangle,
            self.generic_textured_triangle,
            self.generic_degenerate_triangle,
        )
    }
}

pub(super) fn record_quad_probe_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.quad_window_probe += start.elapsed().as_micros();
    }
}

pub(super) fn record_solid_quad_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.solid_quad += start.elapsed().as_micros();
    }
}

pub(super) fn record_solid_quad_reject_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.record_solid_quad_reject(start.elapsed().as_micros());
    }
}

pub(super) fn record_textured_quad_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.textured_quad += start.elapsed().as_micros();
    }
}

pub(super) fn record_textured_quad_reject_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.record_textured_quad_reject(start.elapsed().as_micros());
    }
}

#[derive(Debug, Default)]
pub(super) struct PrimitiveStats {
    pub(super) mesh_primitives: usize,
    pub(super) callback_primitives: usize,
    pub(super) missing_texture_meshes: usize,
    pub(super) empty_clip_meshes: usize,
    pub(super) mesh_indices: usize,
    pub(super) quad_windows: usize,
    pub(super) four_unique_quad_windows: usize,
    pub(super) quad_windows_not_four_unique_indices: usize,
    pub(super) quad_window_vertex_lookup_failures: usize,
    pub(super) axis_aligned_quad_windows: usize,
    pub(super) solid_axis_aligned_quad_windows: usize,
    pub(super) textured_axis_aligned_quad_windows: usize,
    pub(super) solid_quad_fast_path_hits: usize,
    pub(super) textured_quad_fast_path_hits: usize,
    pub(super) textured_quad_reject_not_rectangle_diagonal: usize,
    pub(super) textured_quad_reject_not_axis_aligned_rectangle: usize,
    pub(super) textured_quad_reject_corner_attribute_mismatch: usize,
    pub(super) textured_quad_reject_non_uniform_color: usize,
    pub(super) textured_quad_reject_non_affine_uv: usize,
    pub(super) solid_fan_probe: SolidFanProbeStats,
    pub(super) solid_fan_runs: usize,
    pub(super) solid_fan_triangles: usize,
    pub(super) generic_triangles_rasterized: usize,
    pub(super) generic_solid_triangles: usize,
    pub(super) generic_textured_triangles: usize,
    pub(super) generic_textured_constant_texel_triangles: usize,
    pub(super) generic_textured_sampled_triangles: usize,
    pub(super) generic_textured_solid_reject_non_uniform_vertex_color: usize,
    pub(super) generic_textured_solid_reject_non_uniform_texel: usize,
    pub(super) generic_textured_non_uniform_color_constant_texel: usize,
    pub(super) generic_textured_non_uniform_color_varying_texel: usize,
    pub(super) degenerate_triangles: usize,
    pub(super) generic_triangle_bbox_px_buckets: TriangleBboxBuckets,
    pub(super) generic_solid_triangle_bbox_px_buckets: TriangleBboxBuckets,
    pub(super) generic_textured_triangle_bbox_px_buckets: TriangleBboxBuckets,
    pub(super) generic_degenerate_triangle_bbox_px_buckets: TriangleBboxBuckets,
    pub(super) generic_triangle_bbox_non_finite: usize,
    solid_triangle_offenders: SolidTriangleOffenders,
    textured_triangle_offenders: TexturedTriangleOffenders,
    textured_quad_reject_offenders: TexturedQuadRejectOffenders,
    solid_fan_offenders: SolidFanOffenders,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SolidFanProbeStats {
    pub(super) probe_calls: usize,
    pub(super) preflight_reject_too_few_triangles: usize,
    pub(super) rejected_probe_calls: usize,
    pub(super) center_slot_attempts: usize,
    pub(super) cheap_candidate_attempts: usize,
    pub(super) candidate_triangles_scanned: usize,
    pub(super) accepted_candidate_triangles_scanned: usize,
    pub(super) rejected_candidate_triangles_scanned: usize,
    pub(super) repeated_boundary_checks: usize,
    pub(super) repeated_boundary_comparisons: usize,
    pub(super) reject_no_candidate: usize,
    pub(super) reject_too_short: usize,
    pub(super) reject_vertex_lookup_failure: usize,
    pub(super) reject_winding_mismatch_or_non_finite: usize,
    pub(super) reject_repeated_boundary: usize,
    pub(super) reject_unsafe_polygon: usize,
    pub(super) reject_color_or_bounds_mismatch: usize,
    pub(super) reject_scratch_overflow: usize,
    pub(super) accepted_runs: usize,
    pub(super) accepted_triangles: usize,
    pub(super) polygon_builds: usize,
    pub(super) max_candidate_triangles: usize,
    pub(super) max_accepted_triangles: usize,
}

impl SolidFanProbeStats {
    pub(super) fn record_reject(&mut self, reason: SolidFanProbeRejectReason) {
        match reason {
            SolidFanProbeRejectReason::NoCandidate => self.reject_no_candidate += 1,
            SolidFanProbeRejectReason::TooShort => self.reject_too_short += 1,
            SolidFanProbeRejectReason::VertexLookupFailure => {
                self.reject_vertex_lookup_failure += 1;
            }
            SolidFanProbeRejectReason::WindingMismatchOrNonFinite => {
                self.reject_winding_mismatch_or_non_finite += 1;
            }
            SolidFanProbeRejectReason::RepeatedBoundary => self.reject_repeated_boundary += 1,
            SolidFanProbeRejectReason::UnsafePolygon => self.reject_unsafe_polygon += 1,
            SolidFanProbeRejectReason::ColorOrBoundsMismatch => {
                self.reject_color_or_bounds_mismatch += 1;
            }
            SolidFanProbeRejectReason::ScratchOverflow => self.reject_scratch_overflow += 1,
        }
    }

    pub(super) fn record_candidate_triangles(&mut self, triangle_count: usize) {
        self.max_candidate_triangles = self.max_candidate_triangles.max(triangle_count);
    }

    pub(super) fn record_probe_candidate_triangles(
        &mut self,
        accepted: bool,
        candidate_triangles_scanned_before: usize,
    ) {
        let scanned = self
            .candidate_triangles_scanned
            .saturating_sub(candidate_triangles_scanned_before);
        if accepted {
            self.accepted_candidate_triangles_scanned += scanned;
        } else {
            self.rejected_candidate_triangles_scanned += scanned;
        }
    }

    pub(super) fn record_accepted(&mut self, triangle_count: usize) {
        self.accepted_runs += 1;
        self.accepted_triangles += triangle_count;
        self.max_accepted_triangles = self.max_accepted_triangles.max(triangle_count);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SolidFanProbeRejectReason {
    NoCandidate,
    TooShort,
    VertexLookupFailure,
    WindingMismatchOrNonFinite,
    RepeatedBoundary,
    UnsafePolygon,
    ColorOrBoundsMismatch,
    ScratchOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TriangleSource {
    pub(super) primitive_index: usize,
    pub(super) mesh_index_offset: usize,
}

#[derive(Clone, Copy)]
struct GenericTriangleBboxRecord<'a> {
    vertices: [&'a egui::epaint::Vertex; 3],
    texture: &'a TextureImage,
    classification: TriangleClassification,
    texel_sample: Option<TriangleTexelSample>,
    clip: ClipBounds,
    source: TriangleSource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SolidTriangleOffender {
    clipped_bbox_px: usize,
    bounds: TriangleRasterBounds,
    source: TriangleSource,
    positions: [egui::Pos2; 3],
}

#[derive(Debug, Default)]
struct SolidTriangleOffenders {
    entries: [Option<SolidTriangleOffender>; SOLID_TRIANGLE_OFFENDER_LIMIT],
    len: usize,
}

const SOLID_TRIANGLE_OFFENDER_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
struct TexturedTriangleOffender {
    clipped_bbox_px: usize,
    scan_work: TriangleScanWorkEstimate,
    bounds: TriangleRasterBounds,
    source: TriangleSource,
    positions: [egui::Pos2; 3],
    uvs: [egui::Pos2; 3],
    texel_sample: TriangleTexelSample,
}

#[derive(Debug, Default)]
struct TexturedTriangleOffenders {
    entries: [Option<TexturedTriangleOffender>; TEXTURED_TRIANGLE_OFFENDER_LIMIT],
    len: usize,
}

const TEXTURED_TRIANGLE_OFFENDER_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TexturedQuadRejectWorkEstimate {
    candidate_px: usize,
    clipped_bbox_px: usize,
    bounds: Option<TriangleRasterBounds>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TexturedQuadRejectOffender {
    rejection: TexturedQuadFastPathRejection,
    work: TexturedQuadRejectWorkEstimate,
    source: TriangleSource,
    positions: [egui::Pos2; 6],
    uvs: [egui::Pos2; 6],
    alphas: [u8; 6],
    uniform_color: bool,
    first_texel_sample: TriangleTexelSample,
    second_texel_sample: TriangleTexelSample,
}

#[derive(Debug, Default)]
struct TexturedQuadRejectOffenders {
    entries: [Option<TexturedQuadRejectOffender>; TEXTURED_QUAD_REJECT_OFFENDER_LIMIT],
    len: usize,
}

const TEXTURED_QUAD_REJECT_OFFENDER_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SolidFanRasterWork {
    px: usize,
    rows: usize,
    edge_intersections: usize,
    endpoint_probe_px: usize,
    fallback_rows: usize,
}

impl SolidFanRasterWork {
    pub(super) const fn from_stats(stats: &RasterStats) -> Self {
        Self {
            px: stats.solid_fan_px,
            rows: stats.solid_fan_rows,
            edge_intersections: stats.solid_fan_edge_intersections,
            endpoint_probe_px: stats.solid_fan_endpoint_probe_px,
            fallback_rows: stats.solid_fan_fallback_rows,
        }
    }
}

impl std::ops::Sub for SolidFanRasterWork {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            px: self.px.saturating_sub(rhs.px),
            rows: self.rows.saturating_sub(rhs.rows),
            edge_intersections: self
                .edge_intersections
                .saturating_sub(rhs.edge_intersections),
            endpoint_probe_px: self.endpoint_probe_px.saturating_sub(rhs.endpoint_probe_px),
            fallback_rows: self.fallback_rows.saturating_sub(rhs.fallback_rows),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SolidFanRasterRecord {
    pub(super) source: TriangleSource,
    pub(super) triangle_count: usize,
    pub(super) polygon_vertices: usize,
    pub(super) alpha: u8,
    pub(super) bounds: Option<TriangleRasterBounds>,
    pub(super) work: SolidFanRasterWork,
    pub(super) elapsed_us: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SolidFanOffender {
    elapsed_us: u128,
    source: TriangleSource,
    triangle_count: usize,
    polygon_vertices: usize,
    alpha: u8,
    bounds: Option<TriangleRasterBounds>,
    shape_key: u64,
    work: SolidFanRasterWork,
}

#[derive(Debug, Default)]
struct SolidFanOffenders {
    entries: [Option<SolidFanOffender>; SOLID_FAN_OFFENDER_LIMIT],
    len: usize,
}

const SOLID_FAN_OFFENDER_LIMIT: usize = 8;

impl SolidTriangleOffenders {
    fn record(&mut self, offender: SolidTriangleOffender) {
        if self.len < SOLID_TRIANGLE_OFFENDER_LIMIT {
            self.entries[self.len] = Some(offender);
            self.len += 1;
            self.sort_used_entries();
            return;
        }

        let Some(last) = self.entries[self.len - 1] else {
            return;
        };
        if compare_solid_triangle_offenders(&offender, &last).is_gt() {
            self.entries[self.len - 1] = Some(offender);
            self.sort_used_entries();
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn sort_used_entries(&mut self) {
        self.entries[..self.len].sort_by(|left, right| match (left, right) {
            (Some(left), Some(right)) => compare_solid_triangle_offenders(right, left),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        });
    }
}

fn compare_solid_triangle_offenders(
    left: &SolidTriangleOffender,
    right: &SolidTriangleOffender,
) -> std::cmp::Ordering {
    left.clipped_bbox_px
        .cmp(&right.clipped_bbox_px)
        .then_with(|| {
            right
                .source
                .primitive_index
                .cmp(&left.source.primitive_index)
        })
        .then_with(|| {
            right
                .source
                .mesh_index_offset
                .cmp(&left.source.mesh_index_offset)
        })
}

impl TexturedTriangleOffenders {
    fn record(&mut self, offender: TexturedTriangleOffender) {
        if self.len < TEXTURED_TRIANGLE_OFFENDER_LIMIT {
            self.entries[self.len] = Some(offender);
            self.len += 1;
            self.sort_used_entries();
            return;
        }

        let Some(last) = self.entries[self.len - 1] else {
            return;
        };
        if compare_textured_triangle_offenders(&offender, &last).is_gt() {
            self.entries[self.len - 1] = Some(offender);
            self.sort_used_entries();
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn sort_used_entries(&mut self) {
        self.entries[..self.len].sort_by(|left, right| match (left, right) {
            (Some(left), Some(right)) => compare_textured_triangle_offenders(right, left),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        });
    }
}

fn compare_textured_triangle_offenders(
    left: &TexturedTriangleOffender,
    right: &TexturedTriangleOffender,
) -> std::cmp::Ordering {
    left.scan_work
        .candidate_px
        .cmp(&right.scan_work.candidate_px)
        .then_with(|| left.clipped_bbox_px.cmp(&right.clipped_bbox_px))
        .then_with(|| {
            right
                .source
                .primitive_index
                .cmp(&left.source.primitive_index)
        })
        .then_with(|| {
            right
                .source
                .mesh_index_offset
                .cmp(&left.source.mesh_index_offset)
        })
}

impl TexturedQuadRejectOffenders {
    fn record(&mut self, offender: &TexturedQuadRejectOffender) {
        if self.len < TEXTURED_QUAD_REJECT_OFFENDER_LIMIT {
            self.entries[self.len] = Some(*offender);
            self.len += 1;
            self.sort_used_entries();
            return;
        }

        let Some(last) = self.entries[self.len - 1] else {
            return;
        };
        if compare_textured_quad_reject_offenders(offender, &last).is_gt() {
            self.entries[self.len - 1] = Some(*offender);
            self.sort_used_entries();
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn sort_used_entries(&mut self) {
        self.entries[..self.len].sort_by(|left, right| match (left, right) {
            (Some(left), Some(right)) => compare_textured_quad_reject_offenders(right, left),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        });
    }
}

fn compare_textured_quad_reject_offenders(
    left: &TexturedQuadRejectOffender,
    right: &TexturedQuadRejectOffender,
) -> std::cmp::Ordering {
    left.work
        .candidate_px
        .cmp(&right.work.candidate_px)
        .then_with(|| left.work.clipped_bbox_px.cmp(&right.work.clipped_bbox_px))
        .then_with(|| {
            right
                .source
                .primitive_index
                .cmp(&left.source.primitive_index)
        })
        .then_with(|| {
            right
                .source
                .mesh_index_offset
                .cmp(&left.source.mesh_index_offset)
        })
}

impl SolidFanOffenders {
    fn record(&mut self, offender: SolidFanOffender) {
        if self.len < SOLID_FAN_OFFENDER_LIMIT {
            self.entries[self.len] = Some(offender);
            self.len += 1;
            self.sort_used_entries();
            return;
        }

        let Some(last) = self.entries[self.len - 1] else {
            return;
        };
        if compare_solid_fan_offenders(&offender, &last).is_gt() {
            self.entries[self.len - 1] = Some(offender);
            self.sort_used_entries();
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn sort_used_entries(&mut self) {
        self.entries[..self.len].sort_by(|left, right| match (left, right) {
            (Some(left), Some(right)) => compare_solid_fan_offenders(right, left),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        });
    }
}

fn compare_solid_fan_offenders(
    left: &SolidFanOffender,
    right: &SolidFanOffender,
) -> std::cmp::Ordering {
    left.elapsed_us
        .cmp(&right.elapsed_us)
        .then_with(|| left.work.px.cmp(&right.work.px))
        .then_with(|| left.work.rows.cmp(&right.work.rows))
        .then_with(|| left.triangle_count.cmp(&right.triangle_count))
        .then_with(|| {
            right
                .source
                .primitive_index
                .cmp(&left.source.primitive_index)
        })
        .then_with(|| {
            right
                .source
                .mesh_index_offset
                .cmp(&left.source.mesh_index_offset)
        })
}

#[derive(Debug, Default)]
pub(super) struct TriangleBboxBuckets {
    counts: [usize; TRIANGLE_BBOX_BUCKETS],
}

const TRIANGLE_BBOX_BUCKETS: usize = 6;
#[cfg(test)]
const TRIANGLE_BBOX_BUCKET_LE4: usize = 0;
const TRIANGLE_BBOX_BUCKET_GT1024: usize = 5;

impl TriangleBboxBuckets {
    fn record(&mut self, clipped_bbox_px: usize) {
        let bucket = if clipped_bbox_px <= 4 {
            0
        } else if clipped_bbox_px <= 16 {
            1
        } else if clipped_bbox_px <= 64 {
            2
        } else if clipped_bbox_px <= 256 {
            3
        } else if clipped_bbox_px <= 1024 {
            4
        } else {
            TRIANGLE_BBOX_BUCKET_GT1024
        };
        self.counts[bucket] += 1;
    }
}

impl fmt::Display for TriangleBboxBuckets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{}",
            self.counts[0],
            self.counts[1],
            self.counts[2],
            self.counts[3],
            self.counts[4],
            self.counts[5]
        )
    }
}

impl PrimitiveStats {
    pub(super) fn record_quad_window(
        &mut self,
        vertices: [&egui::epaint::Vertex; 6],
        texture: &TextureImage,
    ) {
        if !is_axis_aligned_quad(vertices) {
            return;
        }
        self.axis_aligned_quad_windows += 1;
        let first = classify_triangle(vertices[0], vertices[1], vertices[2], texture);
        let second = classify_triangle(vertices[3], vertices[4], vertices[5], texture);
        if first == TriangleClassification::Solid && second == TriangleClassification::Solid {
            self.solid_axis_aligned_quad_windows += 1;
        } else {
            self.textured_axis_aligned_quad_windows += 1;
        }
    }

    pub(super) fn record_generic_triangle(
        &mut self,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        texture: &TextureImage,
        clip: ClipBounds,
        source: TriangleSource,
    ) {
        self.generic_triangles_rasterized += 1;
        let classification = classify_triangle(v0, v1, v2, texture);
        match classification {
            TriangleClassification::Degenerate => self.degenerate_triangles += 1,
            TriangleClassification::Solid => {
                self.generic_solid_triangles += 1;
            }
            TriangleClassification::Textured => {
                self.generic_textured_triangles += 1;
                let texel_sample = triangle_nearest_texel_sample(v0, v1, v2, texture);
                if texel_sample.is_uniform() {
                    self.generic_textured_constant_texel_triangles += 1;
                } else {
                    self.generic_textured_sampled_triangles += 1;
                }
                self.record_generic_triangle_bbox(GenericTriangleBboxRecord {
                    vertices: [v0, v1, v2],
                    texture,
                    classification,
                    texel_sample: Some(texel_sample),
                    clip,
                    source,
                });
                return;
            }
        }
        if classification == TriangleClassification::Degenerate {
            return;
        }
        self.record_generic_triangle_bbox(GenericTriangleBboxRecord {
            vertices: [v0, v1, v2],
            texture,
            classification,
            texel_sample: None,
            clip,
            source,
        });
    }

    fn record_generic_triangle_bbox(&mut self, record: GenericTriangleBboxRecord<'_>) {
        let [v0, v1, v2] = record.vertices;
        let classification = record.classification;
        if classification == TriangleClassification::Degenerate {
            return;
        }
        if !v0.pos.x.is_finite()
            || !v0.pos.y.is_finite()
            || !v1.pos.x.is_finite()
            || !v1.pos.y.is_finite()
            || !v2.pos.x.is_finite()
            || !v2.pos.y.is_finite()
        {
            self.generic_triangle_bbox_non_finite += 1;
            return;
        }

        let Some(bounds) = triangle_raster_bounds(v0, v1, v2, record.clip) else {
            return;
        };
        let clipped_bbox_px = bounds.pixel_area();

        self.generic_triangle_bbox_px_buckets
            .record(clipped_bbox_px);
        match classification {
            TriangleClassification::Degenerate => {
                self.generic_degenerate_triangle_bbox_px_buckets
                    .record(clipped_bbox_px);
            }
            TriangleClassification::Solid => {
                self.generic_solid_triangle_bbox_px_buckets
                    .record(clipped_bbox_px);
                self.solid_triangle_offenders.record(SolidTriangleOffender {
                    clipped_bbox_px,
                    bounds,
                    source: record.source,
                    positions: [v0.pos, v1.pos, v2.pos],
                });
            }
            TriangleClassification::Textured => {
                let texel_sample = record
                    .texel_sample
                    .unwrap_or_else(|| triangle_nearest_texel_sample(v0, v1, v2, record.texture));
                match solid_triangle_color_decision(v0, v1, v2, record.texture) {
                    SolidTriangleColorDecision::Solid(_) => {}
                    SolidTriangleColorDecision::NonUniformVertexColor => {
                        self.generic_textured_solid_reject_non_uniform_vertex_color += 1;
                        if texel_sample.is_uniform() {
                            self.generic_textured_non_uniform_color_constant_texel += 1;
                        } else {
                            self.generic_textured_non_uniform_color_varying_texel += 1;
                        }
                    }
                    SolidTriangleColorDecision::NonUniformTexel => {
                        self.generic_textured_solid_reject_non_uniform_texel += 1;
                    }
                }
                self.generic_textured_triangle_bbox_px_buckets
                    .record(clipped_bbox_px);
                let positions = [v0.pos, v1.pos, v2.pos];
                self.textured_triangle_offenders
                    .record(TexturedTriangleOffender {
                        clipped_bbox_px,
                        scan_work: estimate_triangle_scan_work(positions, bounds),
                        bounds,
                        source: record.source,
                        positions,
                        uvs: [v0.uv, v1.uv, v2.uv],
                        texel_sample,
                    });
            }
        }
    }

    pub(super) fn record_textured_quad_rejection(
        &mut self,
        rejection: TexturedQuadFastPathRejection,
        vertices: [&egui::epaint::Vertex; 6],
        texture: &TextureImage,
        clip: ClipBounds,
        source: TriangleSource,
    ) {
        match rejection {
            TexturedQuadFastPathRejection::NotRectangleDiagonal => {
                self.textured_quad_reject_not_rectangle_diagonal += 1;
            }
            TexturedQuadFastPathRejection::NotAxisAlignedRectangle => {
                self.textured_quad_reject_not_axis_aligned_rectangle += 1;
            }
            TexturedQuadFastPathRejection::CornerAttributeMismatch => {
                self.textured_quad_reject_corner_attribute_mismatch += 1;
            }
            TexturedQuadFastPathRejection::NonUniformColor => {
                self.textured_quad_reject_non_uniform_color += 1;
            }
            TexturedQuadFastPathRejection::NonAffineUv => {
                self.textured_quad_reject_non_affine_uv += 1;
            }
        }
        self.textured_quad_reject_offenders
            .record(&textured_quad_reject_offender(
                rejection, vertices, texture, clip, source,
            ));
    }

    pub(super) fn record_solid_fan(&mut self, record: SolidFanRasterRecord) {
        let shape_key = solid_fan_shape_key(
            record.bounds,
            record.alpha,
            record.polygon_vertices,
            record.triangle_count,
        );
        self.solid_fan_offenders.record(SolidFanOffender {
            elapsed_us: record.elapsed_us,
            source: record.source,
            triangle_count: record.triangle_count,
            polygon_vertices: record.polygon_vertices,
            alpha: record.alpha,
            bounds: record.bounds,
            shape_key,
            work: record.work,
        });
    }

    pub(super) fn log_line(&self, frame_index: u64) -> String {
        let mesh_triangles = self.mesh_indices / 3;
        format!(
            "software renderer primitive_stats frame={frame_index} mesh_primitives={} callback_primitives={} missing_texture_meshes={} empty_clip_meshes={} mesh_indices={} mesh_triangles={} quad_windows={} four_unique_quad_windows={} quad_windows_not_four_unique_indices={} quad_window_vertex_lookup_failures={} axis_aligned_quad_windows={} solid_axis_aligned_quad_windows={} textured_axis_aligned_quad_windows={} solid_quad_fast_path_hits={} textured_quad_fast_path_hits={} textured_quad_reject_not_rectangle_diagonal={} textured_quad_reject_not_axis_aligned_rectangle={} textured_quad_reject_corner_attribute_mismatch={} textured_quad_reject_non_uniform_color={} textured_quad_reject_non_affine_uv={} solid_fan_probe_calls={} solid_fan_preflight_reject_too_few_triangles={} solid_fan_rejected_probe_calls={} solid_fan_center_slot_attempts={} solid_fan_cheap_candidate_attempts={} solid_fan_candidate_triangles_scanned={} solid_fan_accepted_candidate_triangles_scanned={} solid_fan_rejected_candidate_triangles_scanned={} solid_fan_repeated_boundary_checks={} solid_fan_repeated_boundary_comparisons={} solid_fan_reject_no_candidate={} solid_fan_reject_too_short={} solid_fan_reject_vertex_lookup_failure={} solid_fan_reject_winding_mismatch_or_non_finite={} solid_fan_reject_repeated_boundary={} solid_fan_reject_unsafe_polygon={} solid_fan_reject_color_or_bounds_mismatch={} solid_fan_reject_scratch_overflow={} solid_fan_accepted_runs={} solid_fan_accepted_triangles={} solid_fan_polygon_builds={} solid_fan_max_candidate_triangles={} solid_fan_max_accepted_triangles={} solid_fan_runs={} solid_fan_triangles={} generic_triangles_rasterized={} generic_solid_triangles={} generic_textured_triangles={} generic_textured_constant_texel_triangles={} generic_textured_sampled_triangles={} generic_textured_solid_reject_non_uniform_vertex_color={} generic_textured_solid_reject_non_uniform_texel={} generic_textured_non_uniform_color_constant_texel={} generic_textured_non_uniform_color_varying_texel={} degenerate_triangles={} generic_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_solid_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_textured_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_degenerate_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_triangle_bbox_non_finite={}",
            self.mesh_primitives,
            self.callback_primitives,
            self.missing_texture_meshes,
            self.empty_clip_meshes,
            self.mesh_indices,
            mesh_triangles,
            self.quad_windows,
            self.four_unique_quad_windows,
            self.quad_windows_not_four_unique_indices,
            self.quad_window_vertex_lookup_failures,
            self.axis_aligned_quad_windows,
            self.solid_axis_aligned_quad_windows,
            self.textured_axis_aligned_quad_windows,
            self.solid_quad_fast_path_hits,
            self.textured_quad_fast_path_hits,
            self.textured_quad_reject_not_rectangle_diagonal,
            self.textured_quad_reject_not_axis_aligned_rectangle,
            self.textured_quad_reject_corner_attribute_mismatch,
            self.textured_quad_reject_non_uniform_color,
            self.textured_quad_reject_non_affine_uv,
            self.solid_fan_probe.probe_calls,
            self.solid_fan_probe.preflight_reject_too_few_triangles,
            self.solid_fan_probe.rejected_probe_calls,
            self.solid_fan_probe.center_slot_attempts,
            self.solid_fan_probe.cheap_candidate_attempts,
            self.solid_fan_probe.candidate_triangles_scanned,
            self.solid_fan_probe.accepted_candidate_triangles_scanned,
            self.solid_fan_probe.rejected_candidate_triangles_scanned,
            self.solid_fan_probe.repeated_boundary_checks,
            self.solid_fan_probe.repeated_boundary_comparisons,
            self.solid_fan_probe.reject_no_candidate,
            self.solid_fan_probe.reject_too_short,
            self.solid_fan_probe.reject_vertex_lookup_failure,
            self.solid_fan_probe.reject_winding_mismatch_or_non_finite,
            self.solid_fan_probe.reject_repeated_boundary,
            self.solid_fan_probe.reject_unsafe_polygon,
            self.solid_fan_probe.reject_color_or_bounds_mismatch,
            self.solid_fan_probe.reject_scratch_overflow,
            self.solid_fan_probe.accepted_runs,
            self.solid_fan_probe.accepted_triangles,
            self.solid_fan_probe.polygon_builds,
            self.solid_fan_probe.max_candidate_triangles,
            self.solid_fan_probe.max_accepted_triangles,
            self.solid_fan_runs,
            self.solid_fan_triangles,
            self.generic_triangles_rasterized,
            self.generic_solid_triangles,
            self.generic_textured_triangles,
            self.generic_textured_constant_texel_triangles,
            self.generic_textured_sampled_triangles,
            self.generic_textured_solid_reject_non_uniform_vertex_color,
            self.generic_textured_solid_reject_non_uniform_texel,
            self.generic_textured_non_uniform_color_constant_texel,
            self.generic_textured_non_uniform_color_varying_texel,
            self.degenerate_triangles,
            self.generic_triangle_bbox_px_buckets,
            self.generic_solid_triangle_bbox_px_buckets,
            self.generic_textured_triangle_bbox_px_buckets,
            self.generic_degenerate_triangle_bbox_px_buckets,
            self.generic_triangle_bbox_non_finite,
        )
    }

    pub(super) fn solid_triangle_offenders_log_line(&self) -> Option<String> {
        if self.solid_triangle_offenders.is_empty() {
            return None;
        }

        let mut log_line = format!(
            "software renderer solid_triangle_offenders shown={} cap={}",
            self.solid_triangle_offenders.len, SOLID_TRIANGLE_OFFENDER_LIMIT
        );
        for (index, offender) in self.solid_triangle_offenders.entries
            [..self.solid_triangle_offenders.len]
            .iter()
            .flatten()
            .enumerate()
        {
            log_line.push(' ');
            log_line.push_str(&format_solid_triangle_offender(index, *offender));
        }
        Some(log_line)
    }

    pub(super) fn textured_triangle_offenders_log_line(&self) -> Option<String> {
        if self.textured_triangle_offenders.is_empty() {
            return None;
        }

        let mut log_line = format!(
            "software renderer textured_triangle_offenders shown={} cap={}",
            self.textured_triangle_offenders.len, TEXTURED_TRIANGLE_OFFENDER_LIMIT
        );
        for (index, offender) in self.textured_triangle_offenders.entries
            [..self.textured_triangle_offenders.len]
            .iter()
            .flatten()
            .enumerate()
        {
            log_line.push(' ');
            log_line.push_str(&format_textured_triangle_offender(index, *offender));
        }
        Some(log_line)
    }

    pub(super) fn textured_quad_reject_offenders_log_line(&self) -> Option<String> {
        if self.textured_quad_reject_offenders.is_empty() {
            return None;
        }

        let mut log_line = format!(
            "software renderer textured_quad_reject_offenders shown={} cap={}",
            self.textured_quad_reject_offenders.len, TEXTURED_QUAD_REJECT_OFFENDER_LIMIT
        );
        for (index, offender) in self.textured_quad_reject_offenders.entries
            [..self.textured_quad_reject_offenders.len]
            .iter()
            .flatten()
            .enumerate()
        {
            log_line.push(' ');
            log_line.push_str(&format_textured_quad_reject_offender(index, offender));
        }
        Some(log_line)
    }

    pub(super) fn solid_fan_offenders_log_line(&self) -> Option<String> {
        if self.solid_fan_offenders.is_empty() {
            return None;
        }

        let mut log_line = format!(
            "software renderer solid_fan_offenders shown={} cap={}",
            self.solid_fan_offenders.len, SOLID_FAN_OFFENDER_LIMIT
        );
        for (index, offender) in self.solid_fan_offenders.entries[..self.solid_fan_offenders.len]
            .iter()
            .flatten()
            .enumerate()
        {
            log_line.push(' ');
            log_line.push_str(&format_solid_fan_offender(index, *offender));
        }
        Some(log_line)
    }
}

fn format_solid_triangle_offender(index: usize, offender: SolidTriangleOffender) -> String {
    format!(
        "offender{index}_clipped_bbox_px={} offender{index}_bounds={},{},{},{} offender{index}_primitive={} offender{index}_mesh_index_offset={} offender{index}_v0={:.1},{:.1} offender{index}_v1={:.1},{:.1} offender{index}_v2={:.1},{:.1}",
        offender.clipped_bbox_px,
        offender.bounds.min_x,
        offender.bounds.min_y,
        offender.bounds.max_x,
        offender.bounds.max_y,
        offender.source.primitive_index,
        offender.source.mesh_index_offset,
        offender.positions[0].x,
        offender.positions[0].y,
        offender.positions[1].x,
        offender.positions[1].y,
        offender.positions[2].x,
        offender.positions[2].y,
    )
}

fn format_textured_triangle_offender(index: usize, offender: TexturedTriangleOffender) -> String {
    format!(
        "offender{index}_candidate_px={} offender{index}_clipped_bbox_px={} offender{index}_narrowed_rows={} offender{index}_full_scan_rows={} offender{index}_bounds={},{},{},{} offender{index}_primitive={} offender{index}_mesh_index_offset={} offender{index}_v0={:.1},{:.1} offender{index}_uv0={:.3},{:.3} offender{index}_v1={:.1},{:.1} offender{index}_uv1={:.3},{:.3} offender{index}_v2={:.1},{:.1} offender{index}_uv2={:.3},{:.3} offender{index}_texels={} offender{index}_constant_texel={} offender{index}_texel_rgba={}",
        offender.scan_work.candidate_px,
        offender.clipped_bbox_px,
        offender.scan_work.narrowed_rows,
        offender.scan_work.full_scan_rows,
        offender.bounds.min_x,
        offender.bounds.min_y,
        offender.bounds.max_x,
        offender.bounds.max_y,
        offender.source.primitive_index,
        offender.source.mesh_index_offset,
        offender.positions[0].x,
        offender.positions[0].y,
        offender.uvs[0].x,
        offender.uvs[0].y,
        offender.positions[1].x,
        offender.positions[1].y,
        offender.uvs[1].x,
        offender.uvs[1].y,
        offender.positions[2].x,
        offender.positions[2].y,
        offender.uvs[2].x,
        offender.uvs[2].y,
        format_triangle_texels(offender.texel_sample),
        u8::from(offender.texel_sample.is_uniform()),
        format_texel_rgba(offender.texel_sample.uniform_color),
    )
}

fn textured_quad_reject_offender(
    rejection: TexturedQuadFastPathRejection,
    vertices: [&egui::epaint::Vertex; 6],
    texture: &TextureImage,
    clip: ClipBounds,
    source: TriangleSource,
) -> TexturedQuadRejectOffender {
    let first_bounds = triangle_raster_bounds(vertices[0], vertices[1], vertices[2], clip);
    let second_bounds = triangle_raster_bounds(vertices[3], vertices[4], vertices[5], clip);
    TexturedQuadRejectOffender {
        rejection,
        work: textured_quad_reject_work(vertices, first_bounds, second_bounds),
        source,
        positions: vertices.map(|vertex| vertex.pos),
        uvs: vertices.map(|vertex| vertex.uv),
        alphas: vertices.map(|vertex| vertex.color.a()),
        uniform_color: vertices
            .iter()
            .all(|vertex| vertex.color == vertices[0].color),
        first_texel_sample: triangle_nearest_texel_sample(
            vertices[0],
            vertices[1],
            vertices[2],
            texture,
        ),
        second_texel_sample: triangle_nearest_texel_sample(
            vertices[3],
            vertices[4],
            vertices[5],
            texture,
        ),
    }
}

fn textured_quad_reject_work(
    vertices: [&egui::epaint::Vertex; 6],
    first_bounds: Option<TriangleRasterBounds>,
    second_bounds: Option<TriangleRasterBounds>,
) -> TexturedQuadRejectWorkEstimate {
    let first_px = first_bounds.map_or(0, |bounds| {
        estimate_triangle_scan_work([vertices[0].pos, vertices[1].pos, vertices[2].pos], bounds)
            .candidate_px
    });
    let second_px = second_bounds.map_or(0, |bounds| {
        estimate_triangle_scan_work([vertices[3].pos, vertices[4].pos, vertices[5].pos], bounds)
            .candidate_px
    });
    let bounds = union_raster_bounds(first_bounds, second_bounds);
    TexturedQuadRejectWorkEstimate {
        candidate_px: first_px + second_px,
        clipped_bbox_px: bounds.map_or(0, TriangleRasterBounds::pixel_area),
        bounds,
    }
}

fn union_raster_bounds(
    first: Option<TriangleRasterBounds>,
    second: Option<TriangleRasterBounds>,
) -> Option<TriangleRasterBounds> {
    match (first, second) {
        (Some(first), Some(second)) => Some(TriangleRasterBounds {
            min_x: first.min_x.min(second.min_x),
            min_y: first.min_y.min(second.min_y),
            max_x: first.max_x.max(second.max_x),
            max_y: first.max_y.max(second.max_y),
        }),
        (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
        (None, None) => None,
    }
}

fn format_textured_quad_reject_offender(
    index: usize,
    offender: &TexturedQuadRejectOffender,
) -> String {
    let bounds = offender
        .work
        .bounds
        .map_or_else(|| "none".to_owned(), format_bounds);
    format!(
        "offender{index}_candidate_px={} offender{index}_clipped_bbox_px={} offender{index}_bounds={} offender{index}_primitive={} offender{index}_mesh_index_offset={} offender{index}_reason={} offender{index}_uniform_color={} offender{index}_alphas={} offender{index}_v0={:.1},{:.1} offender{index}_uv0={:.3},{:.3} offender{index}_v1={:.1},{:.1} offender{index}_uv1={:.3},{:.3} offender{index}_v2={:.1},{:.1} offender{index}_uv2={:.3},{:.3} offender{index}_v3={:.1},{:.1} offender{index}_uv3={:.3},{:.3} offender{index}_v4={:.1},{:.1} offender{index}_uv4={:.3},{:.3} offender{index}_v5={:.1},{:.1} offender{index}_uv5={:.3},{:.3} offender{index}_tri0_texels={} offender{index}_tri0_constant_texel={} offender{index}_tri0_texel_rgba={} offender{index}_tri1_texels={} offender{index}_tri1_constant_texel={} offender{index}_tri1_texel_rgba={}",
        offender.work.candidate_px,
        offender.work.clipped_bbox_px,
        bounds,
        offender.source.primitive_index,
        offender.source.mesh_index_offset,
        format_textured_quad_rejection(offender.rejection),
        u8::from(offender.uniform_color),
        format_alphas(offender.alphas),
        offender.positions[0].x,
        offender.positions[0].y,
        offender.uvs[0].x,
        offender.uvs[0].y,
        offender.positions[1].x,
        offender.positions[1].y,
        offender.uvs[1].x,
        offender.uvs[1].y,
        offender.positions[2].x,
        offender.positions[2].y,
        offender.uvs[2].x,
        offender.uvs[2].y,
        offender.positions[3].x,
        offender.positions[3].y,
        offender.uvs[3].x,
        offender.uvs[3].y,
        offender.positions[4].x,
        offender.positions[4].y,
        offender.uvs[4].x,
        offender.uvs[4].y,
        offender.positions[5].x,
        offender.positions[5].y,
        offender.uvs[5].x,
        offender.uvs[5].y,
        format_triangle_texels(offender.first_texel_sample),
        u8::from(offender.first_texel_sample.is_uniform()),
        format_texel_rgba(offender.first_texel_sample.uniform_color),
        format_triangle_texels(offender.second_texel_sample),
        u8::from(offender.second_texel_sample.is_uniform()),
        format_texel_rgba(offender.second_texel_sample.uniform_color),
    )
}

fn format_textured_quad_rejection(rejection: TexturedQuadFastPathRejection) -> &'static str {
    match rejection {
        TexturedQuadFastPathRejection::NotRectangleDiagonal => "not_rectangle_diagonal",
        TexturedQuadFastPathRejection::NotAxisAlignedRectangle => "not_axis_aligned_rectangle",
        TexturedQuadFastPathRejection::CornerAttributeMismatch => "corner_attribute_mismatch",
        TexturedQuadFastPathRejection::NonUniformColor => "non_uniform_color",
        TexturedQuadFastPathRejection::NonAffineUv => "non_affine_uv",
    }
}

fn format_bounds(bounds: TriangleRasterBounds) -> String {
    format!(
        "{},{},{},{}",
        bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y
    )
}

fn format_alphas(alphas: [u8; 6]) -> String {
    format!(
        "{},{},{},{},{},{}",
        alphas[0], alphas[1], alphas[2], alphas[3], alphas[4], alphas[5]
    )
}

fn format_solid_fan_offender(index: usize, offender: SolidFanOffender) -> String {
    let bounds = offender
        .bounds
        .map_or_else(|| "none".to_owned(), format_bounds);
    format!(
        "offender{index}_elapsed_us={} offender{index}_primitive={} offender{index}_mesh_index_offset={} offender{index}_triangles={} offender{index}_polygon_vertices={} offender{index}_alpha={} offender{index}_bounds={} offender{index}_bbox_px={} offender{index}_shape_key={:016x} offender{index}_px={} offender{index}_rows={} offender{index}_edge_intersections={} offender{index}_endpoint_probe_px={} offender{index}_fallback_rows={}",
        offender.elapsed_us,
        offender.source.primitive_index,
        offender.source.mesh_index_offset,
        offender.triangle_count,
        offender.polygon_vertices,
        offender.alpha,
        bounds,
        offender.bounds.map_or(0, TriangleRasterBounds::pixel_area),
        offender.shape_key,
        offender.work.px,
        offender.work.rows,
        offender.work.edge_intersections,
        offender.work.endpoint_probe_px,
        offender.work.fallback_rows,
    )
}

fn solid_fan_shape_key(
    bounds: Option<TriangleRasterBounds>,
    alpha: u8,
    polygon_vertices: usize,
    triangle_count: usize,
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    if let Some(bounds) = bounds {
        hash = mix_shape_key(hash, bounds.min_x as u64);
        hash = mix_shape_key(hash, bounds.min_y as u64);
        hash = mix_shape_key(hash, bounds.max_x as u64);
        hash = mix_shape_key(hash, bounds.max_y as u64);
    }
    hash = mix_shape_key(hash, u64::from(alpha));
    hash = mix_shape_key(hash, polygon_vertices as u64);
    mix_shape_key(hash, triangle_count as u64)
}

const fn mix_shape_key(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x0000_0100_0000_01b3)
}

fn format_triangle_texels(sample: TriangleTexelSample) -> String {
    match sample.texels {
        Some([first, second, third]) => format!(
            "{},{};{},{};{},{}",
            first.0, first.1, second.0, second.1, third.0, third.1
        ),
        None => "empty".to_owned(),
    }
}

fn format_texel_rgba(color: Option<[u8; 4]>) -> String {
    match color {
        Some([r, g, b, a]) => format!("{r},{g},{b},{a}"),
        None => "none".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::portmaster::raster::usize_to_f32;

    #[test]
    fn primitive_stats_classifies_axis_aligned_solid_and_textured_quads() {
        let texture = TextureImage {
            width: 2,
            height: 2,
            pixels: vec![
                255, 255, 255, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
            ],
        };
        let solid_vertices = quad_vertices();
        let mut textured_vertices = quad_vertices();
        textured_vertices[1].uv = egui::pos2(1.0, 0.0);

        let mut stats = PrimitiveStats::default();
        stats.record_quad_window(
            [
                &solid_vertices[0],
                &solid_vertices[1],
                &solid_vertices[2],
                &solid_vertices[1],
                &solid_vertices[3],
                &solid_vertices[2],
            ],
            &texture,
        );
        stats.record_quad_window(
            [
                &textured_vertices[0],
                &textured_vertices[1],
                &textured_vertices[2],
                &textured_vertices[1],
                &textured_vertices[3],
                &textured_vertices[2],
            ],
            &texture,
        );

        assert_eq!(stats.axis_aligned_quad_windows, 2);
        assert_eq!(stats.solid_axis_aligned_quad_windows, 1);
        assert_eq!(stats.textured_axis_aligned_quad_windows, 1);
    }

    #[test]
    fn primitive_stats_classifies_generic_triangles_and_log_line() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(2.0, 0.0);
        let mut v2 = test_vertex(0.0, 2.0);
        let clip = clip_bounds(64, 64);
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(&v0, &v1, &v2, &texture, clip, triangle_source(1, 0));
        v2.color = egui::Color32::BLACK;
        stats.record_generic_triangle(&v0, &v1, &v2, &texture, clip, triangle_source(1, 3));
        stats.record_generic_triangle(&v0, &v1, &v0, &texture, clip, triangle_source(1, 6));

        assert_eq!(stats.generic_triangles_rasterized, 3);
        assert_eq!(stats.generic_solid_triangles, 1);
        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(stats.generic_textured_constant_texel_triangles, 1);
        assert_eq!(stats.generic_textured_sampled_triangles, 0);
        assert_eq!(
            stats.generic_textured_solid_reject_non_uniform_vertex_color,
            1
        );
        assert_eq!(stats.generic_textured_solid_reject_non_uniform_texel, 0);
        assert_eq!(stats.generic_textured_non_uniform_color_constant_texel, 1);
        assert_eq!(stats.generic_textured_non_uniform_color_varying_texel, 0);
        assert_eq!(stats.degenerate_triangles, 1);
        assert_eq!(
            stats.generic_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_LE4],
            2
        );
        assert_eq!(
            stats.generic_solid_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_LE4],
            1
        );
        assert_eq!(
            stats.generic_textured_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_LE4],
            1
        );
        assert_eq!(
            stats.generic_degenerate_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_LE4],
            0
        );
        stats.textured_quad_fast_path_hits = 2;
        stats.textured_quad_reject_non_affine_uv = 1;
        stats.solid_fan_probe.probe_calls = 3;
        stats.solid_fan_probe.preflight_reject_too_few_triangles = 4;
        stats.solid_fan_probe.rejected_probe_calls = 2;
        stats.solid_fan_probe.candidate_triangles_scanned = 9;
        stats.solid_fan_probe.accepted_candidate_triangles_scanned = 4;
        stats.solid_fan_probe.rejected_candidate_triangles_scanned = 5;
        stats.solid_fan_probe.accepted_runs = 1;
        stats.solid_fan_probe.max_candidate_triangles = 4;
        let log_line = stats.log_line(7);
        assert!(log_line.contains("frame=7"));
        assert!(log_line.contains("mesh_triangles=0"));
        assert!(log_line.contains("generic_solid_triangles=1"));
        assert!(log_line.contains("generic_textured_triangles=1"));
        assert!(log_line.contains("generic_textured_constant_texel_triangles=1"));
        assert!(log_line.contains("generic_textured_sampled_triangles=0"));
        assert!(log_line.contains("generic_textured_solid_reject_non_uniform_vertex_color=1"));
        assert!(log_line.contains("generic_textured_solid_reject_non_uniform_texel=0"));
        assert!(log_line.contains("generic_textured_non_uniform_color_constant_texel=1"));
        assert!(log_line.contains("generic_textured_non_uniform_color_varying_texel=0"));
        assert!(log_line.contains("textured_quad_fast_path_hits=2"));
        assert!(log_line.contains("textured_quad_reject_non_affine_uv=1"));
        assert!(log_line.contains("solid_fan_probe_calls=3"));
        assert!(log_line.contains("solid_fan_preflight_reject_too_few_triangles=4"));
        assert!(log_line.contains("solid_fan_rejected_probe_calls=2"));
        assert!(log_line.contains("solid_fan_candidate_triangles_scanned=9"));
        assert!(log_line.contains("solid_fan_accepted_candidate_triangles_scanned=4"));
        assert!(log_line.contains("solid_fan_rejected_candidate_triangles_scanned=5"));
        assert!(log_line.contains("solid_fan_accepted_runs=1"));
        assert!(log_line.contains("solid_fan_max_candidate_triangles=4"));
        assert!(log_line.contains("degenerate_triangles=1"));
        assert!(log_line.contains(
            "generic_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024=2,0,0,0,0,0"
        ));
        assert!(log_line.contains(
            "generic_solid_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024=1,0,0,0,0,0"
        ));
        assert!(log_line.contains(
            "generic_textured_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024=1,0,0,0,0,0"
        ));
        assert!(log_line.contains(
            "generic_degenerate_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024=0,0,0,0,0,0"
        ));
        assert!(log_line.contains("generic_triangle_bbox_non_finite=0"));
    }

    #[test]
    fn primitive_stats_counts_textured_triangle_rejected_by_non_uniform_vertex_color() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(2.0, 0.0);
        let mut v2 = test_vertex(0.0, 2.0);
        v2.color = egui::Color32::BLACK;
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(
            &v0,
            &v1,
            &v2,
            &texture,
            clip_bounds(16, 16),
            triangle_source(1, 0),
        );

        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(stats.generic_textured_constant_texel_triangles, 1);
        assert_eq!(stats.generic_textured_sampled_triangles, 0);
        assert_eq!(
            stats.generic_textured_solid_reject_non_uniform_vertex_color,
            1
        );
        assert_eq!(stats.generic_textured_solid_reject_non_uniform_texel, 0);
        assert_eq!(stats.generic_textured_non_uniform_color_constant_texel, 1);
        assert_eq!(stats.generic_textured_non_uniform_color_varying_texel, 0);
    }

    #[test]
    fn primitive_stats_counts_non_uniform_color_with_varying_texels() {
        let texture = TextureImage {
            width: 2,
            height: 1,
            pixels: vec![255, 255, 255, 255, 0, 0, 0, 255],
        };
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(2.0, 0.0);
        let mut v2 = test_vertex(0.0, 2.0);
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(1.0, 0.0);
        v2.uv = egui::pos2(0.0, 0.0);
        v2.color = egui::Color32::BLACK;
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(
            &v0,
            &v1,
            &v2,
            &texture,
            clip_bounds(16, 16),
            triangle_source(1, 0),
        );

        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(stats.generic_textured_constant_texel_triangles, 0);
        assert_eq!(stats.generic_textured_sampled_triangles, 1);
        assert_eq!(
            stats.generic_textured_solid_reject_non_uniform_vertex_color,
            1
        );
        assert_eq!(stats.generic_textured_solid_reject_non_uniform_texel, 0);
        assert_eq!(stats.generic_textured_non_uniform_color_constant_texel, 0);
        assert_eq!(stats.generic_textured_non_uniform_color_varying_texel, 1);
    }

    #[test]
    fn primitive_stats_counts_textured_triangle_rejected_by_non_uniform_texel() {
        let texture = TextureImage {
            width: 2,
            height: 1,
            pixels: vec![255, 255, 255, 255, 0, 0, 0, 255],
        };
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(2.0, 0.0);
        let mut v2 = test_vertex(0.0, 2.0);
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(1.0, 0.0);
        v2.uv = egui::pos2(0.0, 0.0);
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(
            &v0,
            &v1,
            &v2,
            &texture,
            clip_bounds(16, 16),
            triangle_source(1, 0),
        );

        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(stats.generic_textured_constant_texel_triangles, 0);
        assert_eq!(stats.generic_textured_sampled_triangles, 1);
        assert_eq!(
            stats.generic_textured_solid_reject_non_uniform_vertex_color,
            0
        );
        assert_eq!(stats.generic_textured_solid_reject_non_uniform_texel, 1);
        assert_eq!(stats.generic_textured_non_uniform_color_constant_texel, 0);
        assert_eq!(stats.generic_textured_non_uniform_color_varying_texel, 0);
    }

    #[test]
    fn primitive_stats_buckets_large_textured_triangle() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(33.0, 0.0);
        let mut v2 = test_vertex(0.0, 33.0);
        v2.color = egui::Color32::BLACK;
        let clip = clip_bounds(64, 64);
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(&v0, &v1, &v2, &texture, clip, triangle_source(2, 0));

        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(
            stats.generic_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_GT1024],
            1
        );
        assert_eq!(
            stats.generic_textured_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_GT1024],
            1
        );
    }

    #[test]
    fn primitive_stats_buckets_mostly_offscreen_triangle_by_clipped_raster_bounds() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(-4096.0, -4096.0);
        let v1 = test_vertex(8.0, 0.0);
        let mut v2 = test_vertex(0.0, 8.0);
        v2.color = egui::Color32::BLACK;
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(
            &v0,
            &v1,
            &v2,
            &texture,
            clip_bounds(16, 16),
            triangle_source(2, 0),
        );

        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(stats.generic_triangle_bbox_non_finite, 0);
        assert_eq!(
            stats.generic_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_GT1024],
            0
        );
        assert_eq!(
            stats.generic_textured_triangle_bbox_px_buckets.counts[TRIANGLE_BBOX_BUCKET_GT1024],
            0
        );
        assert_eq!(
            triangle_bucket_total(&stats.generic_triangle_bbox_px_buckets),
            1
        );
    }

    #[test]
    fn primitive_stats_classifies_fully_clipped_triangle_without_bbox_bucket() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(20.0, 20.0);
        let v1 = test_vertex(24.0, 20.0);
        let v2 = test_vertex(20.0, 24.0);
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(
            &v0,
            &v1,
            &v2,
            &texture,
            clip_bounds(16, 16),
            triangle_source(3, 0),
        );

        assert_eq!(stats.generic_triangles_rasterized, 1);
        assert_eq!(stats.generic_solid_triangles, 1);
        assert_eq!(stats.generic_textured_triangles, 0);
        assert_eq!(stats.degenerate_triangles, 0);
        assert_eq!(
            triangle_bucket_total(&stats.generic_triangle_bbox_px_buckets),
            0
        );
        assert_eq!(
            triangle_bucket_total(&stats.generic_solid_triangle_bbox_px_buckets),
            0
        );
    }

    #[test]
    fn primitive_stats_classifies_collinear_triangle_without_bbox_bucket() {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(4.0, 4.0);
        let v2 = test_vertex(8.0, 8.0);
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(
            &v0,
            &v1,
            &v2,
            &texture,
            clip_bounds(16, 16),
            triangle_source(3, 0),
        );

        assert_eq!(stats.generic_triangles_rasterized, 1);
        assert_eq!(stats.generic_solid_triangles, 0);
        assert_eq!(stats.generic_textured_triangles, 0);
        assert_eq!(stats.degenerate_triangles, 1);
        assert_eq!(
            triangle_bucket_total(&stats.generic_triangle_bbox_px_buckets),
            0
        );
        assert_eq!(
            triangle_bucket_total(&stats.generic_degenerate_triangle_bbox_px_buckets),
            0
        );
    }

    #[test]
    fn primitive_stats_keeps_top_solid_triangle_offenders_ordered_and_capped() {
        let mut offenders = SolidTriangleOffenders::default();

        for clipped_bbox_px in 1..=10 {
            offenders.record(test_offender(
                clipped_bbox_px,
                clipped_bbox_px,
                clipped_bbox_px * 3,
            ));
        }

        assert_eq!(offenders.len, SOLID_TRIANGLE_OFFENDER_LIMIT);
        let clipped_bbox_px: Vec<_> = offenders.entries[..offenders.len]
            .iter()
            .flatten()
            .map(|offender| offender.clipped_bbox_px)
            .collect();
        assert_eq!(clipped_bbox_px, vec![10, 9, 8, 7, 6, 5, 4, 3]);
    }

    #[test]
    fn primitive_stats_orders_solid_triangle_offender_ties_by_source() {
        let mut offenders = SolidTriangleOffenders::default();

        offenders.record(test_offender(12, 2, 6));
        offenders.record(test_offender(12, 1, 9));
        offenders.record(test_offender(12, 1, 3));

        let sources: Vec<_> = offenders.entries[..offenders.len]
            .iter()
            .flatten()
            .map(|offender| offender.source)
            .collect();
        assert_eq!(
            sources,
            vec![
                triangle_source(1, 3),
                triangle_source(1, 9),
                triangle_source(2, 6)
            ]
        );
    }

    #[test]
    fn primitive_stats_formats_solid_triangle_offenders_log_line() {
        let mut stats = PrimitiveStats::default();
        stats
            .solid_triangle_offenders
            .record(SolidTriangleOffender {
                clipped_bbox_px: 9,
                bounds: TriangleRasterBounds {
                    min_x: 1,
                    min_y: 2,
                    max_x: 4,
                    max_y: 5,
                },
                source: triangle_source(7, 12),
                positions: [
                    egui::pos2(1.25, 2.5),
                    egui::pos2(3.0, 4.0),
                    egui::pos2(5.0, 6.75),
                ],
            });

        assert_eq!(
            stats.solid_triangle_offenders_log_line().as_deref(),
            Some(
                "software renderer solid_triangle_offenders shown=1 cap=8 offender0_clipped_bbox_px=9 offender0_bounds=1,2,4,5 offender0_primitive=7 offender0_mesh_index_offset=12 offender0_v0=1.2,2.5 offender0_v1=3.0,4.0 offender0_v2=5.0,6.8"
            )
        );
    }

    #[test]
    fn primitive_stats_formats_multiple_solid_triangle_offenders_with_indexed_keys() {
        let mut stats = PrimitiveStats::default();
        stats
            .solid_triangle_offenders
            .record(test_offender(12, 2, 6));
        stats
            .solid_triangle_offenders
            .record(test_offender(9, 1, 3));

        let log_line = stats
            .solid_triangle_offenders_log_line()
            .expect("offender log line");

        assert_eq!(
            log_line,
            "software renderer solid_triangle_offenders shown=2 cap=8 offender0_clipped_bbox_px=12 offender0_bounds=0,0,12,1 offender0_primitive=2 offender0_mesh_index_offset=6 offender0_v0=0.0,0.0 offender0_v1=0.0,0.0 offender0_v2=0.0,0.0 offender1_clipped_bbox_px=9 offender1_bounds=0,0,9,1 offender1_primitive=1 offender1_mesh_index_offset=3 offender1_v0=0.0,0.0 offender1_v1=0.0,0.0 offender1_v2=0.0,0.0"
        );
        assert!(!log_line.contains(" area="));
        assert!(!log_line.contains(" count="));
        assert!(!log_line.contains(" bounds="));
        assert!(log_line.contains("offender0_clipped_bbox_px="));
        assert!(log_line.contains("offender1_clipped_bbox_px="));
    }

    #[test]
    fn primitive_stats_skips_solid_triangle_offenders_log_line_when_empty() {
        assert_eq!(
            PrimitiveStats::default().solid_triangle_offenders_log_line(),
            None
        );
    }

    #[test]
    fn primitive_stats_keeps_top_textured_triangle_offenders_ordered_and_capped() {
        let mut offenders = TexturedTriangleOffenders::default();

        for candidate_px in 1..=10 {
            offenders.record(test_textured_offender(
                candidate_px,
                candidate_px * 2,
                candidate_px,
                candidate_px * 3,
            ));
        }

        assert_eq!(offenders.len, TEXTURED_TRIANGLE_OFFENDER_LIMIT);
        let candidate_px: Vec<_> = offenders.entries[..offenders.len]
            .iter()
            .flatten()
            .map(|offender| offender.scan_work.candidate_px)
            .collect();
        assert_eq!(candidate_px, vec![10, 9, 8, 7, 6, 5, 4, 3]);
    }

    #[test]
    fn primitive_stats_orders_textured_triangle_offender_ties_by_source() {
        let mut offenders = TexturedTriangleOffenders::default();

        offenders.record(test_textured_offender(12, 9, 2, 6));
        offenders.record(test_textured_offender(12, 9, 1, 9));
        offenders.record(test_textured_offender(12, 9, 1, 3));

        let sources: Vec<_> = offenders.entries[..offenders.len]
            .iter()
            .flatten()
            .map(|offender| offender.source)
            .collect();
        assert_eq!(
            sources,
            vec![
                triangle_source(1, 3),
                triangle_source(1, 9),
                triangle_source(2, 6)
            ]
        );
    }

    #[test]
    fn primitive_stats_formats_textured_triangle_offenders_log_line() {
        let mut stats = PrimitiveStats::default();
        stats
            .textured_triangle_offenders
            .record(TexturedTriangleOffender {
                clipped_bbox_px: 9,
                scan_work: TriangleScanWorkEstimate {
                    candidate_px: 14,
                    narrowed_rows: 2,
                    full_scan_rows: 1,
                },
                bounds: TriangleRasterBounds {
                    min_x: 1,
                    min_y: 2,
                    max_x: 4,
                    max_y: 5,
                },
                source: triangle_source(7, 12),
                positions: [
                    egui::pos2(1.25, 2.5),
                    egui::pos2(3.0, 4.0),
                    egui::pos2(5.0, 6.75),
                ],
                uvs: [
                    egui::pos2(0.0, 0.5),
                    egui::pos2(1.0, 0.25),
                    egui::pos2(0.75, 1.0),
                ],
                texel_sample: TriangleTexelSample {
                    texels: Some([(0, 1), (2, 3), (4, 5)]),
                    uniform_color: None,
                },
            });

        assert_eq!(
            stats.textured_triangle_offenders_log_line().as_deref(),
            Some(
                "software renderer textured_triangle_offenders shown=1 cap=8 offender0_candidate_px=14 offender0_clipped_bbox_px=9 offender0_narrowed_rows=2 offender0_full_scan_rows=1 offender0_bounds=1,2,4,5 offender0_primitive=7 offender0_mesh_index_offset=12 offender0_v0=1.2,2.5 offender0_uv0=0.000,0.500 offender0_v1=3.0,4.0 offender0_uv1=1.000,0.250 offender0_v2=5.0,6.8 offender0_uv2=0.750,1.000 offender0_texels=0,1;2,3;4,5 offender0_constant_texel=0 offender0_texel_rgba=none"
            )
        );
    }

    #[test]
    fn primitive_stats_formats_multiple_textured_triangle_offenders_with_indexed_keys() {
        let mut stats = PrimitiveStats::default();
        stats
            .textured_triangle_offenders
            .record(test_textured_offender(12, 9, 2, 6));
        stats
            .textured_triangle_offenders
            .record(test_textured_offender(9, 12, 1, 3));

        let log_line = stats
            .textured_triangle_offenders_log_line()
            .expect("offender log line");

        assert_eq!(
            log_line,
            "software renderer textured_triangle_offenders shown=2 cap=8 offender0_candidate_px=12 offender0_clipped_bbox_px=9 offender0_narrowed_rows=1 offender0_full_scan_rows=2 offender0_bounds=0,0,9,1 offender0_primitive=2 offender0_mesh_index_offset=6 offender0_v0=0.0,0.0 offender0_uv0=0.000,0.000 offender0_v1=0.0,0.0 offender0_uv1=0.000,0.000 offender0_v2=0.0,0.0 offender0_uv2=0.000,0.000 offender0_texels=0,0;0,0;0,0 offender0_constant_texel=1 offender0_texel_rgba=255,255,255,255 offender1_candidate_px=9 offender1_clipped_bbox_px=12 offender1_narrowed_rows=1 offender1_full_scan_rows=2 offender1_bounds=0,0,12,1 offender1_primitive=1 offender1_mesh_index_offset=3 offender1_v0=0.0,0.0 offender1_uv0=0.000,0.000 offender1_v1=0.0,0.0 offender1_uv1=0.000,0.000 offender1_v2=0.0,0.0 offender1_uv2=0.000,0.000 offender1_texels=0,0;0,0;0,0 offender1_constant_texel=1 offender1_texel_rgba=255,255,255,255"
        );
        assert!(!log_line.contains(" bounds="));
        assert!(!log_line.contains(" uv0="));
        assert!(log_line.contains("offender0_candidate_px="));
        assert!(log_line.contains("offender0_clipped_bbox_px="));
        assert!(log_line.contains("offender0_uv0="));
        assert!(log_line.contains("offender0_texels="));
        assert!(log_line.contains("offender0_constant_texel="));
        assert!(log_line.contains("offender0_texel_rgba="));
        assert!(log_line.contains("offender1_candidate_px="));
        assert!(log_line.contains("offender1_uv0="));
        assert!(log_line.contains("offender1_texels="));
    }

    #[test]
    fn primitive_stats_skips_textured_triangle_offenders_log_line_when_empty() {
        assert_eq!(
            PrimitiveStats::default().textured_triangle_offenders_log_line(),
            None
        );
    }

    #[test]
    fn primitive_stats_keeps_top_textured_quad_reject_offenders_ordered_and_capped() {
        let mut offenders = TexturedQuadRejectOffenders::default();

        for candidate_px in 1..=10 {
            offenders.record(&test_textured_quad_reject_offender(
                candidate_px,
                candidate_px * 2,
                candidate_px,
                candidate_px * 3,
            ));
        }

        assert_eq!(offenders.len, TEXTURED_QUAD_REJECT_OFFENDER_LIMIT);
        let candidate_px: Vec<_> = offenders.entries[..offenders.len]
            .iter()
            .flatten()
            .map(|offender| offender.work.candidate_px)
            .collect();
        assert_eq!(candidate_px, vec![10, 9, 8, 7, 6, 5, 4, 3]);
    }

    #[test]
    fn primitive_stats_formats_textured_quad_reject_offenders_log_line() {
        let mut stats = PrimitiveStats::default();
        stats
            .textured_quad_reject_offenders
            .record(&TexturedQuadRejectOffender {
                rejection: TexturedQuadFastPathRejection::NotAxisAlignedRectangle,
                work: TexturedQuadRejectWorkEstimate {
                    candidate_px: 14,
                    clipped_bbox_px: 9,
                    bounds: Some(TriangleRasterBounds {
                        min_x: 1,
                        min_y: 2,
                        max_x: 4,
                        max_y: 5,
                    }),
                },
                source: triangle_source(7, 12),
                positions: [
                    egui::pos2(1.25, 2.5),
                    egui::pos2(3.0, 4.0),
                    egui::pos2(5.0, 6.75),
                    egui::pos2(7.0, 8.0),
                    egui::pos2(9.0, 10.0),
                    egui::pos2(11.0, 12.0),
                ],
                uvs: [
                    egui::pos2(0.0, 0.5),
                    egui::pos2(1.0, 0.25),
                    egui::pos2(0.75, 1.0),
                    egui::pos2(0.1, 0.2),
                    egui::pos2(0.3, 0.4),
                    egui::pos2(0.5, 0.6),
                ],
                alphas: [255, 254, 253, 252, 251, 250],
                uniform_color: false,
                first_texel_sample: TriangleTexelSample {
                    texels: Some([(0, 1), (2, 3), (4, 5)]),
                    uniform_color: None,
                },
                second_texel_sample: TriangleTexelSample {
                    texels: Some([(6, 7), (8, 9), (10, 11)]),
                    uniform_color: Some([1, 2, 3, 4]),
                },
            });

        assert_eq!(
            stats.textured_quad_reject_offenders_log_line().as_deref(),
            Some(
                "software renderer textured_quad_reject_offenders shown=1 cap=8 offender0_candidate_px=14 offender0_clipped_bbox_px=9 offender0_bounds=1,2,4,5 offender0_primitive=7 offender0_mesh_index_offset=12 offender0_reason=not_axis_aligned_rectangle offender0_uniform_color=0 offender0_alphas=255,254,253,252,251,250 offender0_v0=1.2,2.5 offender0_uv0=0.000,0.500 offender0_v1=3.0,4.0 offender0_uv1=1.000,0.250 offender0_v2=5.0,6.8 offender0_uv2=0.750,1.000 offender0_v3=7.0,8.0 offender0_uv3=0.100,0.200 offender0_v4=9.0,10.0 offender0_uv4=0.300,0.400 offender0_v5=11.0,12.0 offender0_uv5=0.500,0.600 offender0_tri0_texels=0,1;2,3;4,5 offender0_tri0_constant_texel=0 offender0_tri0_texel_rgba=none offender0_tri1_texels=6,7;8,9;10,11 offender0_tri1_constant_texel=1 offender0_tri1_texel_rgba=1,2,3,4"
            )
        );
    }

    #[test]
    fn primitive_stats_skips_textured_quad_reject_offenders_log_line_when_empty() {
        assert_eq!(
            PrimitiveStats::default().textured_quad_reject_offenders_log_line(),
            None
        );
    }

    #[test]
    fn primitive_stats_keeps_top_solid_fan_offenders_ordered_and_capped() {
        let mut offenders = SolidFanOffenders::default();

        for elapsed_us in 1..=10 {
            offenders.record(test_solid_fan_offender(
                elapsed_us,
                usize::try_from(elapsed_us).expect("elapsed"),
                usize::try_from(elapsed_us * 3).expect("offset"),
            ));
        }

        assert_eq!(offenders.len, SOLID_FAN_OFFENDER_LIMIT);
        let elapsed_us: Vec<_> = offenders.entries[..offenders.len]
            .iter()
            .flatten()
            .map(|offender| offender.elapsed_us)
            .collect();
        assert_eq!(elapsed_us, vec![10, 9, 8, 7, 6, 5, 4, 3]);
    }

    #[test]
    fn primitive_stats_formats_solid_fan_offenders_log_line() {
        let mut stats = PrimitiveStats::default();

        stats.record_solid_fan(SolidFanRasterRecord {
            source: triangle_source(7, 12),
            triangle_count: 4,
            polygon_vertices: 6,
            alpha: 128,
            bounds: Some(TriangleRasterBounds {
                min_x: 1,
                min_y: 2,
                max_x: 7,
                max_y: 9,
            }),
            work: SolidFanRasterWork {
                px: 33,
                rows: 5,
                edge_intersections: 14,
                endpoint_probe_px: 9,
                fallback_rows: 1,
            },
            elapsed_us: 42,
        });

        assert_eq!(
            stats.solid_fan_offenders_log_line(),
            Some(format!(
                "software renderer solid_fan_offenders shown=1 cap=8 offender0_elapsed_us=42 offender0_primitive=7 offender0_mesh_index_offset=12 offender0_triangles=4 offender0_polygon_vertices=6 offender0_alpha=128 offender0_bounds=1,2,7,9 offender0_bbox_px=42 offender0_shape_key={:016x} offender0_px=33 offender0_rows=5 offender0_edge_intersections=14 offender0_endpoint_probe_px=9 offender0_fallback_rows=1",
                solid_fan_shape_key(
                    Some(TriangleRasterBounds {
                        min_x: 1,
                        min_y: 2,
                        max_x: 7,
                        max_y: 9,
                    }),
                    128,
                    6,
                    4,
                )
            ))
        );
    }

    #[test]
    fn primitive_stats_skips_solid_fan_offenders_log_line_when_empty() {
        assert_eq!(
            PrimitiveStats::default().solid_fan_offenders_log_line(),
            None
        );
    }

    #[test]
    fn renderer_stats_raster_timings_log_line() {
        let timings = RasterTimings {
            quad_window_probe: 1,
            solid_quad: 2,
            solid_quad_reject: 3,
            textured_quad: 4,
            textured_quad_reject: 5,
            solid_fan_probe: 6,
            solid_fan_accepted_probe: 7,
            solid_fan_rejected_probe: 8,
            solid_fan_raster: 9,
            generic_solid_triangle: 10,
            generic_textured_triangle: 11,
            generic_degenerate_triangle: 12,
        };

        assert_eq!(
            timings.log_line(),
            "software renderer raster_timings quad_window_probe_us=1 solid_quad_us=2 solid_quad_reject_us=3 textured_quad_us=4 textured_quad_reject_us=5 solid_fan_probe_us=6 solid_fan_accepted_probe_us=7 solid_fan_rejected_probe_us=8 solid_fan_raster_us=9 generic_solid_triangle_us=10 generic_textured_triangle_us=11 generic_degenerate_triangle_us=12"
        );
    }

    #[test]
    fn renderer_stats_raster_timings_routes_rejections_separately() {
        let mut timings = RasterTimings::default();

        timings.record_solid_quad_reject(2);
        timings.record_textured_quad_reject(3);
        timings.record_solid_fan_probe(true, 13);
        timings.record_solid_fan_probe(false, 17);
        timings.record_generic_triangle(TriangleClassification::Solid, 5);
        timings.record_generic_triangle(TriangleClassification::Textured, 7);
        timings.record_generic_triangle(TriangleClassification::Degenerate, 11);

        assert_eq!(timings.quad_window_probe, 0);
        assert_eq!(timings.solid_quad, 0);
        assert_eq!(timings.solid_quad_reject, 2);
        assert_eq!(timings.textured_quad, 0);
        assert_eq!(timings.textured_quad_reject, 3);
        assert_eq!(timings.solid_fan_probe, 30);
        assert_eq!(timings.solid_fan_accepted_probe, 13);
        assert_eq!(timings.solid_fan_rejected_probe, 17);
        assert_eq!(timings.solid_fan_raster, 0);
        assert_eq!(timings.generic_solid_triangle, 5);
        assert_eq!(timings.generic_textured_triangle, 7);
        assert_eq!(timings.generic_degenerate_triangle, 11);
    }

    fn quad_vertices() -> [egui::epaint::Vertex; 4] {
        [
            test_vertex(1.0, 1.0),
            test_vertex(4.0, 1.0),
            test_vertex(1.0, 3.0),
            test_vertex(4.0, 3.0),
        ]
    }

    fn clip_bounds(width: usize, height: usize) -> ClipBounds {
        ClipBounds::new(
            egui::Rect::from_min_max(
                egui::Pos2::ZERO,
                egui::pos2(usize_to_f32(width), usize_to_f32(height)),
            ),
            width,
            height,
        )
        .expect("clip bounds")
    }

    fn triangle_bucket_total(buckets: &TriangleBboxBuckets) -> usize {
        buckets.counts.iter().sum()
    }

    const fn triangle_source(primitive_index: usize, mesh_index_offset: usize) -> TriangleSource {
        TriangleSource {
            primitive_index,
            mesh_index_offset,
        }
    }

    fn test_offender(
        clipped_bbox_px: usize,
        primitive_index: usize,
        mesh_index_offset: usize,
    ) -> SolidTriangleOffender {
        SolidTriangleOffender {
            clipped_bbox_px,
            bounds: TriangleRasterBounds {
                min_x: 0,
                min_y: 0,
                max_x: clipped_bbox_px,
                max_y: 1,
            },
            source: triangle_source(primitive_index, mesh_index_offset),
            positions: [egui::Pos2::ZERO; 3],
        }
    }

    fn test_textured_offender(
        candidate_px: usize,
        clipped_bbox_px: usize,
        primitive_index: usize,
        mesh_index_offset: usize,
    ) -> TexturedTriangleOffender {
        TexturedTriangleOffender {
            clipped_bbox_px,
            scan_work: TriangleScanWorkEstimate {
                candidate_px,
                narrowed_rows: 1,
                full_scan_rows: 2,
            },
            bounds: TriangleRasterBounds {
                min_x: 0,
                min_y: 0,
                max_x: clipped_bbox_px,
                max_y: 1,
            },
            source: triangle_source(primitive_index, mesh_index_offset),
            positions: [egui::Pos2::ZERO; 3],
            uvs: [egui::Pos2::ZERO; 3],
            texel_sample: TriangleTexelSample {
                texels: Some([(0, 0), (0, 0), (0, 0)]),
                uniform_color: Some([255, 255, 255, 255]),
            },
        }
    }

    fn test_textured_quad_reject_offender(
        candidate_px: usize,
        clipped_bbox_px: usize,
        primitive_index: usize,
        mesh_index_offset: usize,
    ) -> TexturedQuadRejectOffender {
        TexturedQuadRejectOffender {
            rejection: TexturedQuadFastPathRejection::NotAxisAlignedRectangle,
            work: TexturedQuadRejectWorkEstimate {
                candidate_px,
                clipped_bbox_px,
                bounds: Some(TriangleRasterBounds {
                    min_x: 0,
                    min_y: 0,
                    max_x: clipped_bbox_px,
                    max_y: 1,
                }),
            },
            source: triangle_source(primitive_index, mesh_index_offset),
            positions: [egui::Pos2::ZERO; 6],
            uvs: [egui::Pos2::ZERO; 6],
            alphas: [255; 6],
            uniform_color: true,
            first_texel_sample: TriangleTexelSample {
                texels: Some([(0, 0), (0, 0), (0, 0)]),
                uniform_color: Some([255, 255, 255, 255]),
            },
            second_texel_sample: TriangleTexelSample {
                texels: Some([(0, 0), (0, 0), (0, 0)]),
                uniform_color: Some([255, 255, 255, 255]),
            },
        }
    }

    fn test_solid_fan_offender(
        elapsed_us: u128,
        primitive_index: usize,
        mesh_index_offset: usize,
    ) -> SolidFanOffender {
        SolidFanOffender {
            elapsed_us,
            source: triangle_source(primitive_index, mesh_index_offset),
            triangle_count: 4,
            polygon_vertices: 6,
            alpha: 255,
            bounds: Some(TriangleRasterBounds {
                min_x: 0,
                min_y: 0,
                max_x: 1,
                max_y: 1,
            }),
            shape_key: solid_fan_shape_key(
                Some(TriangleRasterBounds {
                    min_x: 0,
                    min_y: 0,
                    max_x: 1,
                    max_y: 1,
                }),
                255,
                6,
                4,
            ),
            work: SolidFanRasterWork {
                px: usize::try_from(elapsed_us).expect("elapsed"),
                rows: 1,
                edge_intersections: 2,
                endpoint_probe_px: 2,
                fallback_rows: 0,
            },
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
