// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::time::Instant;

use super::RasterInstrumentation;
use crate::gui::portmaster::raster::{
    ClipBounds, SolidTriangleColorDecision, TexturedQuadFastPathRejection, TriangleClassification,
    TriangleRasterBounds, TriangleScanWorkEstimate, TriangleTexelSample, classify_triangle,
    estimate_triangle_scan_work, is_axis_aligned_quad, solid_triangle_color_decision,
    triangle_nearest_texel_sample, triangle_raster_bounds,
};
use crate::gui::portmaster::texture::TextureImage;

#[derive(Debug, Default)]
pub(super) struct RasterTimings {
    pub(super) quad_window_probe: u128,
    pub(super) solid_quad: u128,
    pub(super) solid_quad_reject: u128,
    pub(super) textured_quad: u128,
    pub(super) textured_quad_reject: u128,
    pub(super) solid_fan: u128,
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

    pub(super) fn log_line(&self) -> String {
        format!(
            "software renderer raster_timings quad_window_probe_us={} solid_quad_us={} solid_quad_reject_us={} textured_quad_us={} textured_quad_reject_us={} solid_fan_us={} generic_solid_triangle_us={} generic_textured_triangle_us={} generic_degenerate_triangle_us={}",
            self.quad_window_probe,
            self.solid_quad,
            self.solid_quad_reject,
            self.textured_quad,
            self.textured_quad_reject,
            self.solid_fan,
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
    pub(super) solid_fan_runs: usize,
    pub(super) solid_fan_triangles: usize,
    pub(super) generic_triangles_rasterized: usize,
    pub(super) generic_solid_triangles: usize,
    pub(super) generic_textured_triangles: usize,
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
            }
        }
        if classification == TriangleClassification::Degenerate {
            return;
        }
        self.record_generic_triangle_bbox(GenericTriangleBboxRecord {
            vertices: [v0, v1, v2],
            texture,
            classification,
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
                let texel_sample = triangle_nearest_texel_sample(v0, v1, v2, record.texture);
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
    }

    pub(super) fn log_line(&self) -> String {
        format!(
            "software renderer primitive_stats mesh_primitives={} callback_primitives={} missing_texture_meshes={} empty_clip_meshes={} mesh_indices={} quad_windows={} four_unique_quad_windows={} quad_windows_not_four_unique_indices={} quad_window_vertex_lookup_failures={} axis_aligned_quad_windows={} solid_axis_aligned_quad_windows={} textured_axis_aligned_quad_windows={} solid_quad_fast_path_hits={} textured_quad_fast_path_hits={} textured_quad_reject_not_rectangle_diagonal={} textured_quad_reject_not_axis_aligned_rectangle={} textured_quad_reject_corner_attribute_mismatch={} textured_quad_reject_non_uniform_color={} textured_quad_reject_non_affine_uv={} solid_fan_runs={} solid_fan_triangles={} generic_triangles_rasterized={} generic_solid_triangles={} generic_textured_triangles={} generic_textured_solid_reject_non_uniform_vertex_color={} generic_textured_solid_reject_non_uniform_texel={} generic_textured_non_uniform_color_constant_texel={} generic_textured_non_uniform_color_varying_texel={} degenerate_triangles={} generic_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_solid_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_textured_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_degenerate_triangle_bbox_px_buckets_le4_le16_le64_le256_le1024_gt1024={} generic_triangle_bbox_non_finite={}",
            self.mesh_primitives,
            self.callback_primitives,
            self.missing_texture_meshes,
            self.empty_clip_meshes,
            self.mesh_indices,
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
            self.solid_fan_runs,
            self.solid_fan_triangles,
            self.generic_triangles_rasterized,
            self.generic_solid_triangles,
            self.generic_textured_triangles,
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
        let log_line = stats.log_line();
        assert!(log_line.contains("generic_solid_triangles=1"));
        assert!(log_line.contains("generic_textured_triangles=1"));
        assert!(log_line.contains("generic_textured_solid_reject_non_uniform_vertex_color=1"));
        assert!(log_line.contains("generic_textured_solid_reject_non_uniform_texel=0"));
        assert!(log_line.contains("generic_textured_non_uniform_color_constant_texel=1"));
        assert!(log_line.contains("generic_textured_non_uniform_color_varying_texel=0"));
        assert!(log_line.contains("textured_quad_fast_path_hits=2"));
        assert!(log_line.contains("textured_quad_reject_non_affine_uv=1"));
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
    fn renderer_stats_raster_timings_log_line() {
        let timings = RasterTimings {
            quad_window_probe: 1,
            solid_quad: 2,
            solid_quad_reject: 3,
            textured_quad: 4,
            textured_quad_reject: 5,
            solid_fan: 6,
            generic_solid_triangle: 7,
            generic_textured_triangle: 8,
            generic_degenerate_triangle: 9,
        };

        assert_eq!(
            timings.log_line(),
            "software renderer raster_timings quad_window_probe_us=1 solid_quad_us=2 solid_quad_reject_us=3 textured_quad_us=4 textured_quad_reject_us=5 solid_fan_us=6 generic_solid_triangle_us=7 generic_textured_triangle_us=8 generic_degenerate_triangle_us=9"
        );
    }

    #[test]
    fn renderer_stats_raster_timings_routes_rejections_separately() {
        let mut timings = RasterTimings::default();

        timings.record_solid_quad_reject(2);
        timings.record_textured_quad_reject(3);
        timings.record_generic_triangle(TriangleClassification::Solid, 5);
        timings.record_generic_triangle(TriangleClassification::Textured, 7);
        timings.record_generic_triangle(TriangleClassification::Degenerate, 11);

        assert_eq!(timings.quad_window_probe, 0);
        assert_eq!(timings.solid_quad, 0);
        assert_eq!(timings.solid_quad_reject, 2);
        assert_eq!(timings.textured_quad, 0);
        assert_eq!(timings.textured_quad_reject, 3);
        assert_eq!(timings.solid_fan, 0);
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

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::Pos2::ZERO,
            color: egui::Color32::WHITE,
        }
    }
}
