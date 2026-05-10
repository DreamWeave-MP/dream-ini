// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::io;
use std::time::{Duration, Instant};

use super::log::write_log;
use super::pacing::format_repaint_delay;
use super::raster::{
    ClipBounds, RasterStats, SolidTriangleColorDecision, TexturedQuadFastPathRejection,
    TriangleClassification, TriangleRasterBounds, TriangleScanWorkEstimate, TriangleTexelSample,
    classify_triangle, estimate_triangle_scan_work, is_axis_aligned_quad,
    rasterize_axis_aligned_solid_quad, rasterize_axis_aligned_textured_quad, rasterize_solid_fan,
    rasterize_triangle, solid_triangle_color_decision, textured_quad_fast_path_rejection,
    triangle_nearest_texel_sample, triangle_raster_bounds, usize_to_f32,
};
use super::surface::SoftwareSurface;
use super::texture::TextureImage;
use super::texture::TextureStore;
use super::{GuiFrame, GuiShell};

#[derive(Debug, Default)]
pub(super) struct SoftwareRenderer {
    surface: SoftwareSurface,
    textures: TextureStore,
}

impl SoftwareRenderer {
    pub(super) fn render<S: GuiShell>(
        &mut self,
        width: usize,
        height: usize,
        frame: &mut GuiFrame<'_, S>,
    ) -> io::Result<Duration> {
        let log_frame = frame.log_frame;
        let total_start = log_frame.then(Instant::now);
        let stage_start = log_frame.then(Instant::now);
        self.surface.resize(width, height)?;
        self.surface.clear([17, 20, 28, 255]);
        let resize_clear_elapsed = elapsed_micros(stage_start);

        let stage_start = log_frame.then(Instant::now);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(usize_to_f32(width), usize_to_f32(height)),
            )),
            max_texture_side: Some(1024),
            ..Default::default()
        };
        let output = frame
            .context
            .run_ui(raw_input, |ui| frame.app.ui(ui, frame.shell));
        let repaint_delay = root_repaint_delay(&output);
        let egui_run_elapsed = elapsed_micros(stage_start);

        let stage_start = log_frame.then(Instant::now);
        self.textures.apply(&output.textures_delta)?;
        let texture_apply_elapsed = elapsed_micros(stage_start);

        let stage_start = log_frame.then(Instant::now);
        let primitives = frame.context.tessellate(output.shapes, 1.0);
        let tessellate_elapsed = elapsed_micros(stage_start);
        if log_frame {
            write_log(
                frame.log,
                format!(
                    "software renderer surface={}x{} bytes={} textures={} texture_bytes={} primitives={}",
                    self.surface.width,
                    self.surface.height,
                    self.surface.pixels.len(),
                    self.textures.len(),
                    self.textures.bytes_used(),
                    primitives.len()
                ),
            );
        }

        let stage_start = log_frame.then(Instant::now);
        let mut primitive_stats = log_frame.then(PrimitiveStats::default);
        let mut raster_stats = log_frame.then(RasterStats::default);
        let mut raster_timings = log_frame.then(RasterTimings::default);
        let rasterize_result = self.rasterize(
            &primitives,
            primitive_stats.as_mut(),
            raster_stats.as_mut(),
            raster_timings.as_mut(),
        );
        let rasterize_elapsed = elapsed_micros(stage_start);
        if let Some(stats) = primitive_stats {
            write_log(frame.log, stats.log_line());
            if let Some(log_line) = stats.solid_triangle_offenders_log_line() {
                write_log(frame.log, log_line);
            }
            if let Some(log_line) = stats.textured_triangle_offenders_log_line() {
                write_log(frame.log, log_line);
            }
        }
        if let Some(stats) = raster_stats {
            write_log(frame.log, stats.log_line());
        }
        if let Some(timings) = raster_timings {
            write_log(frame.log, timings.log_line());
        }
        rasterize_result?;

        let stage_start = log_frame.then(Instant::now);
        for id in output.textures_delta.free {
            self.textures.free(id);
        }
        let texture_free_elapsed = elapsed_micros(stage_start);
        let total_elapsed = elapsed_micros(total_start);
        if log_frame {
            let repaint_delay = format_repaint_delay(repaint_delay);
            write_log(
                frame.log,
                format!(
                    "software renderer timings resize_clear_us={resize_clear_elapsed} egui_run_us={egui_run_elapsed} texture_apply_us={texture_apply_elapsed} tessellate_us={tessellate_elapsed} rasterize_us={rasterize_elapsed} texture_free_us={texture_free_elapsed} repaint_delay={repaint_delay} total_us={total_elapsed}"
                ),
            );
        }
        Ok(repaint_delay)
    }

    pub(super) const fn surface(&self) -> &SoftwareSurface {
        &self.surface
    }

    fn rasterize(
        &mut self,
        primitives: &[egui::ClippedPrimitive],
        mut stats: Option<&mut PrimitiveStats>,
        mut raster_stats: Option<&mut RasterStats>,
        mut raster_timings: Option<&mut RasterTimings>,
    ) -> io::Result<()> {
        for (primitive_index, primitive) in primitives.iter().enumerate() {
            match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    if let Some(stats) = borrow_optional_mut(&mut stats) {
                        stats.mesh_primitives += 1;
                    }
                    self.rasterize_mesh(
                        mesh,
                        primitive.clip_rect,
                        primitive_index,
                        borrow_optional_mut(&mut stats),
                        borrow_optional_mut(&mut raster_stats),
                        borrow_optional_mut(&mut raster_timings),
                    )?;
                }
                egui::epaint::Primitive::Callback(_) => {
                    if let Some(stats) = borrow_optional_mut(&mut stats) {
                        stats.callback_primitives += 1;
                    }
                    return Err(io::Error::other(
                        "unsupported egui paint callback in software renderer",
                    ));
                }
            }
        }
        Ok(())
    }

    fn rasterize_mesh(
        &mut self,
        mesh: &egui::Mesh,
        clip_rect: egui::Rect,
        primitive_index: usize,
        mut stats: Option<&mut PrimitiveStats>,
        mut raster_stats: Option<&mut RasterStats>,
        mut raster_timings: Option<&mut RasterTimings>,
    ) -> io::Result<()> {
        if let Some(stats) = borrow_optional_mut(&mut stats) {
            stats.mesh_indices += mesh.indices.len();
        }
        let Some(texture) = self.textures.get(&mesh.texture_id) else {
            if let Some(stats) = borrow_optional_mut(&mut stats) {
                stats.missing_texture_meshes += 1;
            }
            return Ok(());
        };
        let clip = ClipBounds::new(clip_rect, self.surface.width, self.surface.height)?;
        if clip.is_empty() {
            if let Some(stats) = borrow_optional_mut(&mut stats) {
                stats.empty_clip_meshes += 1;
            }
            return Ok(());
        }
        let surface = &mut self.surface;
        let mut index_offset = 0;
        while index_offset + 2 < mesh.indices.len() {
            if index_offset + 5 < mesh.indices.len() {
                let quad = &mesh.indices[index_offset..index_offset + 6];
                let mut instrumentation = RasterInstrumentation {
                    primitive_stats: borrow_optional_mut(&mut stats),
                    raster_stats: borrow_optional_mut(&mut raster_stats),
                    timings: borrow_optional_mut(&mut raster_timings),
                };
                if try_rasterize_quad_window(
                    surface,
                    mesh,
                    texture,
                    clip,
                    quad,
                    &mut instrumentation,
                )? {
                    index_offset += 6;
                    continue;
                }
            }

            if let Some(fan) = solid_fan_run(mesh, texture, clip, index_offset)? {
                let fan_start = raster_timings.as_ref().map(|_| Instant::now());
                rasterize_solid_fan(
                    surface,
                    &fan.polygon,
                    fan.triangle_count,
                    fan.color,
                    clip,
                    borrow_optional_mut(&mut raster_stats),
                );
                if let Some(stats) = borrow_optional_mut(&mut stats) {
                    stats.solid_fan_runs += 1;
                    stats.solid_fan_triangles += fan.triangle_count;
                }
                if let (Some(start), Some(timings)) =
                    (fan_start, borrow_optional_mut(&mut raster_timings))
                {
                    timings.solid_fan += start.elapsed().as_micros();
                }
                index_offset += fan.triangle_count * 3;
                continue;
            }

            let Some([v0, v1, v2]) = mesh_triangle_vertices(mesh, index_offset)? else {
                index_offset += 3;
                continue;
            };
            let source = TriangleSource {
                primitive_index,
                mesh_index_offset: index_offset,
            };
            if let Some(stats) = borrow_optional_mut(&mut stats) {
                stats.record_generic_triangle(v0, v1, v2, texture, clip, source);
            }
            rasterize_generic_triangle(
                surface,
                [v0, v1, v2],
                texture,
                clip,
                &mut raster_stats,
                &mut raster_timings,
            );
            index_offset += 3;
        }
        Ok(())
    }
}

fn mesh_triangle_vertices(
    mesh: &egui::Mesh,
    index_offset: usize,
) -> io::Result<Option<[&egui::epaint::Vertex; 3]>> {
    let triangle = &mesh.indices[index_offset..index_offset + 3];
    let i0 = mesh_index_to_usize(triangle[0])?;
    let i1 = mesh_index_to_usize(triangle[1])?;
    let i2 = mesh_index_to_usize(triangle[2])?;
    let Some(v0) = mesh.vertices.get(i0) else {
        return Ok(None);
    };
    let Some(v1) = mesh.vertices.get(i1) else {
        return Ok(None);
    };
    let Some(v2) = mesh.vertices.get(i2) else {
        return Ok(None);
    };
    Ok(Some([v0, v1, v2]))
}

fn rasterize_generic_triangle(
    surface: &mut SoftwareSurface,
    vertices: [&egui::epaint::Vertex; 3],
    texture: &TextureImage,
    clip: ClipBounds,
    raster_stats: &mut Option<&mut RasterStats>,
    raster_timings: &mut Option<&mut RasterTimings>,
) {
    let [v0, v1, v2] = vertices;
    let triangle_classification = raster_timings
        .as_ref()
        .map(|_| classify_triangle(v0, v1, v2, texture));
    let triangle_start = raster_timings.as_ref().map(|_| Instant::now());
    rasterize_triangle(
        surface,
        v0,
        v1,
        v2,
        texture,
        clip,
        borrow_optional_mut(raster_stats),
    );
    if let (Some(start), Some(classification), Some(timings)) = (
        triangle_start,
        triangle_classification,
        borrow_optional_mut(raster_timings),
    ) {
        timings.record_generic_triangle(classification, start.elapsed().as_micros());
    }
}

struct RasterInstrumentation<'a> {
    primitive_stats: Option<&'a mut PrimitiveStats>,
    raster_stats: Option<&'a mut RasterStats>,
    timings: Option<&'a mut RasterTimings>,
}

struct SolidFanRun<'a> {
    polygon: Vec<&'a egui::epaint::Vertex>,
    triangle_count: usize,
    color: [u8; 4],
}

#[derive(Clone, Copy)]
struct FanCandidate<'a> {
    center_slot: usize,
    center_index: u32,
    center_pos: egui::Pos2,
    center_vertex: &'a egui::epaint::Vertex,
    triangle_count: usize,
}

#[derive(Clone, Copy)]
struct FanTriangle<'a> {
    indices: [u32; 3],
    vertices: [&'a egui::epaint::Vertex; 3],
}

const SOLID_FAN_MIN_TRIANGLES: usize = 4;

impl RasterInstrumentation<'_> {
    fn primitive_stats(&mut self) -> Option<&mut PrimitiveStats> {
        borrow_optional_mut(&mut self.primitive_stats)
    }

    fn raster_stats(&mut self) -> Option<&mut RasterStats> {
        borrow_optional_mut(&mut self.raster_stats)
    }

    fn timings(&mut self) -> Option<&mut RasterTimings> {
        borrow_optional_mut(&mut self.timings)
    }

    fn timing_start(&self) -> Option<Instant> {
        self.timings.as_ref().map(|_| Instant::now())
    }
}

fn try_rasterize_quad_window(
    surface: &mut SoftwareSurface,
    mesh: &egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    quad: &[u32],
    instrumentation: &mut RasterInstrumentation<'_>,
) -> io::Result<bool> {
    let probe_start = instrumentation.timing_start();
    if let Some(stats) = instrumentation.primitive_stats() {
        stats.quad_windows += 1;
    }
    if has_four_unique_indices(quad) {
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.four_unique_quad_windows += 1;
        }
    } else {
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.quad_windows_not_four_unique_indices += 1;
        }
        record_quad_probe_elapsed(instrumentation, probe_start);
        return Ok(false);
    }

    let i0 = mesh_index_to_usize(quad[0])?;
    let i1 = mesh_index_to_usize(quad[1])?;
    let i2 = mesh_index_to_usize(quad[2])?;
    let i3 = mesh_index_to_usize(quad[3])?;
    let i4 = mesh_index_to_usize(quad[4])?;
    let i5 = mesh_index_to_usize(quad[5])?;
    let (Some(v0), Some(v1), Some(v2), Some(v3), Some(v4), Some(v5)) = (
        mesh.vertices.get(i0),
        mesh.vertices.get(i1),
        mesh.vertices.get(i2),
        mesh.vertices.get(i3),
        mesh.vertices.get(i4),
        mesh.vertices.get(i5),
    ) else {
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.quad_window_vertex_lookup_failures += 1;
        }
        record_quad_probe_elapsed(instrumentation, probe_start);
        return Ok(false);
    };

    let vertices = [v0, v1, v2, v3, v4, v5];
    if let Some(stats) = instrumentation.primitive_stats() {
        stats.record_quad_window(vertices, texture);
    }
    record_quad_probe_elapsed(instrumentation, probe_start);

    let solid_start = instrumentation.timing_start();
    if rasterize_axis_aligned_solid_quad(
        surface,
        vertices,
        texture,
        clip,
        instrumentation.raster_stats(),
    ) {
        record_solid_quad_elapsed(instrumentation, solid_start);
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.solid_quad_fast_path_hits += 1;
        }
        return Ok(true);
    }
    record_solid_quad_reject_elapsed(instrumentation, solid_start);

    let textured_start = instrumentation.timing_start();
    if rasterize_axis_aligned_textured_quad(
        surface,
        vertices,
        texture,
        clip,
        instrumentation.raster_stats(),
    ) {
        record_textured_quad_elapsed(instrumentation, textured_start);
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.textured_quad_fast_path_hits += 1;
        }
        return Ok(true);
    }
    record_textured_quad_reject_elapsed(instrumentation, textured_start);
    if let Some(stats) = instrumentation.primitive_stats()
        && let Some(rejection) = textured_quad_fast_path_rejection(vertices)
    {
        stats.record_textured_quad_rejection(rejection);
    }
    Ok(false)
}

fn solid_fan_run<'a>(
    mesh: &'a egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
) -> io::Result<Option<SolidFanRun<'a>>> {
    for center_slot in 0..3 {
        if let Some(run) = solid_fan_run_for_center(mesh, texture, clip, index_offset, center_slot)?
        {
            return Ok(Some(run));
        }
    }
    Ok(None)
}

fn solid_fan_run_for_center<'a>(
    mesh: &'a egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
    center_slot: usize,
) -> io::Result<Option<SolidFanRun<'a>>> {
    let Some(candidate) = cheap_solid_fan_candidate(mesh, index_offset, center_slot)? else {
        return Ok(None);
    };
    let mut polygon = solid_fan_polygon(mesh, index_offset, candidate)?;
    polygon.push(candidate.center_vertex);
    if !solid_fan_polygon_is_safe(&polygon) {
        return Ok(None);
    }
    let Some(color) =
        solid_fan_run_color(mesh, texture, clip, index_offset, candidate.triangle_count)?
    else {
        return Ok(None);
    };
    Ok(Some(SolidFanRun {
        polygon,
        triangle_count: candidate.triangle_count,
        color,
    }))
}

fn cheap_solid_fan_candidate(
    mesh: &egui::Mesh,
    index_offset: usize,
    center_slot: usize,
) -> io::Result<Option<FanCandidate<'_>>> {
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(None);
    };
    if !fan_triangle_positions_are_finite(first) {
        return Ok(None);
    }
    let center_index = first.indices[center_slot];
    let center_vertex = first.vertices[center_slot];
    let center_pos = first.vertices[center_slot].pos;
    let [mut previous_boundary, mut current_boundary] = fan_boundaries(first, center_slot);
    let Some(expected_area_sign) = fan_triangle_area_sign(first) else {
        return Ok(None);
    };
    let mut triangle_count = 1;
    let mut offset = index_offset + 3;
    while offset + 2 < mesh.indices.len() {
        let Some(candidate) = fan_triangle_at(mesh, offset)? else {
            break;
        };
        if !fan_triangle_positions_are_finite(candidate) {
            break;
        }
        let Some(candidate_center_slot) =
            candidate_center_slot(candidate, center_index, center_pos)
        else {
            break;
        };
        let boundaries = fan_boundaries(candidate, candidate_center_slot);
        let Some(next_boundary) = next_fan_boundary(boundaries, current_boundary) else {
            break;
        };
        if fan_boundary_seen(
            mesh,
            index_offset,
            triangle_count,
            center_slot,
            center_index,
            center_pos,
            next_boundary,
        )? {
            break;
        }
        let Some(area_sign) = fan_triangle_area_sign(candidate) else {
            break;
        };
        if area_sign != expected_area_sign {
            break;
        }

        previous_boundary = current_boundary;
        current_boundary = next_boundary;
        triangle_count += 1;
        offset += 3;
    }

    if triangle_count < SOLID_FAN_MIN_TRIANGLES
        || same_fan_vertex(previous_boundary, current_boundary)
    {
        return Ok(None);
    }
    Ok(Some(FanCandidate {
        center_slot,
        center_index,
        center_pos,
        center_vertex,
        triangle_count,
    }))
}

fn fan_boundary_seen(
    mesh: &egui::Mesh,
    index_offset: usize,
    triangle_count: usize,
    center_slot: usize,
    center_index: u32,
    center_pos: egui::Pos2,
    boundary: FanVertex<'_>,
) -> io::Result<bool> {
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(false);
    };
    let [first_boundary, mut current_boundary] = fan_boundaries(first, center_slot);
    if same_fan_vertex(first_boundary, boundary) || same_fan_vertex(current_boundary, boundary) {
        return Ok(true);
    }

    let mut offset = index_offset + 3;
    for _ in 1..triangle_count {
        let Some(triangle) = fan_triangle_at(mesh, offset)? else {
            return Ok(false);
        };
        let Some(candidate_center_slot) = candidate_center_slot(triangle, center_index, center_pos)
        else {
            return Ok(false);
        };
        let Some(next_boundary) = next_fan_boundary(
            fan_boundaries(triangle, candidate_center_slot),
            current_boundary,
        ) else {
            return Ok(false);
        };
        if same_fan_vertex(next_boundary, boundary) {
            return Ok(true);
        }
        current_boundary = next_boundary;
        offset += 3;
    }
    Ok(false)
}

fn solid_fan_polygon<'a>(
    mesh: &'a egui::Mesh,
    index_offset: usize,
    candidate: FanCandidate<'a>,
) -> io::Result<Vec<&'a egui::epaint::Vertex>> {
    let mut polygon = Vec::with_capacity(candidate.triangle_count + 2);
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(polygon);
    };
    let [first_boundary, mut current_boundary] = fan_boundaries(first, candidate.center_slot);
    polygon.push(first_boundary.vertex);
    polygon.push(current_boundary.vertex);

    let mut offset = index_offset + 3;
    for _ in 1..candidate.triangle_count {
        let Some(triangle) = fan_triangle_at(mesh, offset)? else {
            break;
        };
        let Some(center_slot) =
            candidate_center_slot(triangle, candidate.center_index, candidate.center_pos)
        else {
            break;
        };
        let Some(next_boundary) =
            next_fan_boundary(fan_boundaries(triangle, center_slot), current_boundary)
        else {
            break;
        };
        polygon.push(next_boundary.vertex);
        current_boundary = next_boundary;
        offset += 3;
    }
    Ok(polygon)
}

fn solid_fan_run_color(
    mesh: &egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
    triangle_count: usize,
) -> io::Result<Option<[u8; 4]>> {
    let mut color = None;
    let mut offset = index_offset;
    for _ in 0..triangle_count {
        let Some(triangle) = fan_triangle_at(mesh, offset)? else {
            return Ok(None);
        };
        let Some(triangle_color) = solid_fan_triangle_color(triangle, texture, clip) else {
            return Ok(None);
        };
        if let Some(color) = color {
            if triangle_color != color {
                return Ok(None);
            }
        } else {
            color = Some(triangle_color);
        }
        offset += 3;
    }
    Ok(color)
}

fn fan_triangle_at(mesh: &egui::Mesh, index_offset: usize) -> io::Result<Option<FanTriangle<'_>>> {
    let indices = [
        mesh.indices[index_offset],
        mesh.indices[index_offset + 1],
        mesh.indices[index_offset + 2],
    ];
    let i0 = mesh_index_to_usize(indices[0])?;
    let i1 = mesh_index_to_usize(indices[1])?;
    let i2 = mesh_index_to_usize(indices[2])?;
    let (Some(v0), Some(v1), Some(v2)) = (
        mesh.vertices.get(i0),
        mesh.vertices.get(i1),
        mesh.vertices.get(i2),
    ) else {
        return Ok(None);
    };
    Ok(Some(FanTriangle {
        indices,
        vertices: [v0, v1, v2],
    }))
}

fn solid_fan_triangle_color(
    triangle: FanTriangle<'_>,
    texture: &TextureImage,
    clip: ClipBounds,
) -> Option<[u8; 4]> {
    let [v0, v1, v2] = triangle.vertices;
    let SolidTriangleColorDecision::Solid(color) =
        solid_triangle_color_decision(v0, v1, v2, texture)
    else {
        return None;
    };
    triangle_raster_bounds(v0, v1, v2, clip)?;
    Some(color)
}

fn candidate_center_slot(
    triangle: FanTriangle<'_>,
    center_index: u32,
    center_pos: egui::Pos2,
) -> Option<usize> {
    (0..3).find(|slot| {
        triangle.indices[*slot] == center_index
            && same_fan_pos(triangle.vertices[*slot].pos, center_pos)
    })
}

fn fan_boundaries(triangle: FanTriangle<'_>, center_slot: usize) -> [FanVertex<'_>; 2] {
    let [first_slot, second_slot] = match center_slot {
        0 => [1, 2],
        1 => [0, 2],
        2 => [0, 1],
        _ => unreachable!("center slot is probed in the triangle slot range"),
    };
    [
        FanVertex {
            index: triangle.indices[first_slot],
            vertex: triangle.vertices[first_slot],
        },
        FanVertex {
            index: triangle.indices[second_slot],
            vertex: triangle.vertices[second_slot],
        },
    ]
}

fn next_fan_boundary<'a>(
    boundaries: [FanVertex<'a>; 2],
    current_boundary: FanVertex<'_>,
) -> Option<FanVertex<'a>> {
    if same_fan_vertex(boundaries[0], current_boundary) {
        return Some(boundaries[1]);
    }
    if same_fan_vertex(boundaries[1], current_boundary) {
        return Some(boundaries[0]);
    }
    None
}

fn fan_triangle_positions_are_finite(triangle: FanTriangle<'_>) -> bool {
    triangle
        .vertices
        .iter()
        .all(|vertex| vertex.pos.x.is_finite() && vertex.pos.y.is_finite())
}

fn fan_triangle_area_sign(triangle: FanTriangle<'_>) -> Option<i8> {
    let [v0, v1, v2] = triangle.vertices;
    let area = (v2.pos.x - v0.pos.x).mul_add(
        v1.pos.y - v0.pos.y,
        -((v2.pos.y - v0.pos.y) * (v1.pos.x - v0.pos.x)),
    );
    if area.abs() <= f32::EPSILON {
        return None;
    }
    Some(if area.is_sign_positive() { 1 } else { -1 })
}

fn solid_fan_polygon_is_safe(polygon: &[&egui::epaint::Vertex]) -> bool {
    polygon.len() >= SOLID_FAN_MIN_TRIANGLES + 2 && polygon_is_strictly_convex(polygon)
}

fn polygon_is_strictly_convex(polygon: &[&egui::epaint::Vertex]) -> bool {
    let Some(expected_direction) = polygon_area_direction(polygon) else {
        return false;
    };
    for index in 0..polygon.len() {
        let a = polygon[index].pos;
        let b = polygon[(index + 1) % polygon.len()].pos;
        let c = polygon[(index + 2) % polygon.len()].pos;
        let turn = fan_edge(a, b, c);
        if non_zero_float_direction(turn) != Some(expected_direction) {
            return false;
        }
    }
    true
}

fn polygon_area_direction(polygon: &[&egui::epaint::Vertex]) -> Option<std::cmp::Ordering> {
    let mut twice_area = 0.0;
    for index in 0..polygon.len() {
        let a = polygon[index].pos;
        let b = polygon[(index + 1) % polygon.len()].pos;
        twice_area += a.x.mul_add(b.y, -(a.y * b.x));
    }
    non_zero_float_direction(-twice_area)
}

fn non_zero_float_direction(value: f32) -> Option<std::cmp::Ordering> {
    if value > f32::EPSILON {
        Some(std::cmp::Ordering::Greater)
    } else if value < -f32::EPSILON {
        Some(std::cmp::Ordering::Less)
    } else {
        None
    }
}

fn fan_edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)))
}

#[derive(Clone, Copy)]
struct FanVertex<'a> {
    index: u32,
    vertex: &'a egui::epaint::Vertex,
}

fn same_fan_vertex(left: FanVertex<'_>, right: FanVertex<'_>) -> bool {
    left.index == right.index && same_fan_pos(left.vertex.pos, right.vertex.pos)
}

fn same_fan_pos(left: egui::Pos2, right: egui::Pos2) -> bool {
    matches!(
        left.x.partial_cmp(&right.x),
        Some(std::cmp::Ordering::Equal)
    ) && matches!(
        left.y.partial_cmp(&right.y),
        Some(std::cmp::Ordering::Equal)
    )
}

#[derive(Debug, Default)]
struct RasterTimings {
    quad_window_probe: u128,
    solid_quad: u128,
    solid_quad_reject: u128,
    textured_quad: u128,
    textured_quad_reject: u128,
    solid_fan: u128,
    generic_solid_triangle: u128,
    generic_textured_triangle: u128,
    generic_degenerate_triangle: u128,
}

impl RasterTimings {
    fn record_solid_quad_reject(&mut self, elapsed_us: u128) {
        self.solid_quad_reject += elapsed_us;
    }

    fn record_textured_quad_reject(&mut self, elapsed_us: u128) {
        self.textured_quad_reject += elapsed_us;
    }

    fn record_generic_triangle(
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

    fn log_line(&self) -> String {
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

fn record_quad_probe_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.quad_window_probe += start.elapsed().as_micros();
    }
}

fn record_solid_quad_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.solid_quad += start.elapsed().as_micros();
    }
}

fn record_solid_quad_reject_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.record_solid_quad_reject(start.elapsed().as_micros());
    }
}

fn record_textured_quad_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.textured_quad += start.elapsed().as_micros();
    }
}

fn record_textured_quad_reject_elapsed(
    instrumentation: &mut RasterInstrumentation<'_>,
    start: Option<Instant>,
) {
    if let (Some(timings), Some(start)) = (instrumentation.timings(), start) {
        timings.record_textured_quad_reject(start.elapsed().as_micros());
    }
}

#[derive(Debug, Default)]
struct PrimitiveStats {
    mesh_primitives: usize,
    callback_primitives: usize,
    missing_texture_meshes: usize,
    empty_clip_meshes: usize,
    mesh_indices: usize,
    quad_windows: usize,
    four_unique_quad_windows: usize,
    quad_windows_not_four_unique_indices: usize,
    quad_window_vertex_lookup_failures: usize,
    axis_aligned_quad_windows: usize,
    solid_axis_aligned_quad_windows: usize,
    textured_axis_aligned_quad_windows: usize,
    solid_quad_fast_path_hits: usize,
    textured_quad_fast_path_hits: usize,
    textured_quad_reject_not_rectangle_diagonal: usize,
    textured_quad_reject_not_axis_aligned_rectangle: usize,
    textured_quad_reject_corner_attribute_mismatch: usize,
    textured_quad_reject_non_uniform_color: usize,
    textured_quad_reject_non_affine_uv: usize,
    solid_fan_runs: usize,
    solid_fan_triangles: usize,
    generic_triangles_rasterized: usize,
    generic_solid_triangles: usize,
    generic_textured_triangles: usize,
    generic_textured_solid_reject_non_uniform_vertex_color: usize,
    generic_textured_solid_reject_non_uniform_texel: usize,
    generic_textured_non_uniform_color_constant_texel: usize,
    generic_textured_non_uniform_color_varying_texel: usize,
    degenerate_triangles: usize,
    generic_triangle_bbox_px_buckets: TriangleBboxBuckets,
    generic_solid_triangle_bbox_px_buckets: TriangleBboxBuckets,
    generic_textured_triangle_bbox_px_buckets: TriangleBboxBuckets,
    generic_degenerate_triangle_bbox_px_buckets: TriangleBboxBuckets,
    generic_triangle_bbox_non_finite: usize,
    solid_triangle_offenders: SolidTriangleOffenders,
    textured_triangle_offenders: TexturedTriangleOffenders,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TriangleSource {
    primitive_index: usize,
    mesh_index_offset: usize,
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
struct TriangleBboxBuckets {
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
    fn record_quad_window(&mut self, vertices: [&egui::epaint::Vertex; 6], texture: &TextureImage) {
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

    fn record_generic_triangle(
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

    fn record_textured_quad_rejection(&mut self, rejection: TexturedQuadFastPathRejection) {
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

    fn log_line(&self) -> String {
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

    fn solid_triangle_offenders_log_line(&self) -> Option<String> {
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

    fn textured_triangle_offenders_log_line(&self) -> Option<String> {
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

fn mesh_index_to_usize(index: u32) -> io::Result<usize> {
    usize::try_from(index).map_err(|_| io::Error::other("mesh index does not fit usize"))
}

fn has_four_unique_indices(indices: &[u32]) -> bool {
    let mut unique = [0; 4];
    let mut count = 0;
    for &index in indices {
        if unique[..count].contains(&index) {
            continue;
        }
        if count == unique.len() {
            return false;
        }
        unique[count] = index;
        count += 1;
    }
    count == unique.len()
}

fn root_repaint_delay(output: &egui::FullOutput) -> Duration {
    output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map_or(Duration::MAX, |output| output.repaint_delay)
}

fn elapsed_micros(start: Option<Instant>) -> u128 {
    start.map_or(0, |start| start.elapsed().as_micros())
}

fn borrow_optional_mut<'a, T>(option: &'a mut Option<&mut T>) -> Option<&'a mut T> {
    option.as_mut().map(|value| &mut **value)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn renderer_stats_count_textured_quad_fast_path_without_generic_triangles() {
        let texture_id = egui::TextureId::Managed(1);
        let mut renderer = SoftwareRenderer::default();
        renderer.surface.resize(8, 8).expect("surface");
        renderer.surface.clear([0, 0, 0, 255]);
        renderer
            .textures
            .apply(&texture_delta(texture_id))
            .expect("texture");
        let mesh = textured_quad_mesh(texture_id);
        let mut stats = PrimitiveStats::default();

        renderer
            .rasterize_mesh(
                &mesh,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(8.0, 8.0)),
                0,
                Some(&mut stats),
                None,
                None,
            )
            .expect("rasterize mesh");

        assert_eq!(stats.textured_quad_fast_path_hits, 1);
        assert_eq!(stats.generic_triangles_rasterized, 0);
        assert_eq!(stats.generic_textured_triangles, 0);
    }

    #[test]
    fn renderer_stats_count_textured_quad_fast_path_rejection() {
        let texture_id = egui::TextureId::Managed(1);
        let mut renderer = SoftwareRenderer::default();
        renderer.surface.resize(8, 8).expect("surface");
        renderer.surface.clear([0, 0, 0, 255]);
        renderer
            .textures
            .apply(&texture_delta(texture_id))
            .expect("texture");
        let mut mesh = textured_quad_mesh(texture_id);
        mesh.vertices[3].uv = egui::pos2(0.75, 1.0);
        let mut stats = PrimitiveStats::default();

        renderer
            .rasterize_mesh(
                &mesh,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(8.0, 8.0)),
                0,
                Some(&mut stats),
                None,
                None,
            )
            .expect("rasterize mesh");

        assert_eq!(stats.textured_quad_fast_path_hits, 0);
        assert_eq!(stats.textured_quad_reject_non_affine_uv, 1);
        assert_eq!(stats.generic_triangles_rasterized, 2);
    }

    #[test]
    fn renderer_solid_fan_path_matches_per_triangle_reference() {
        let texture_id = egui::TextureId::Managed(1);
        let mut renderer = SoftwareRenderer::default();
        renderer.surface.resize(12, 12).expect("surface");
        renderer.surface.clear([0, 0, 0, 255]);
        renderer
            .textures
            .apply(&texture_delta(texture_id))
            .expect("texture");
        let mesh = solid_fan_mesh(texture_id, [128, 32, 0, 128]);
        let clip_rect = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(12.0, 12.0));
        let mut primitive_stats = PrimitiveStats::default();
        let mut raster_stats = RasterStats::default();

        renderer
            .rasterize_mesh(
                &mesh,
                clip_rect,
                0,
                Some(&mut primitive_stats),
                Some(&mut raster_stats),
                None,
            )
            .expect("rasterize mesh");
        let fan_pixels = renderer.surface.pixels.clone();

        let texture = renderer.textures.get(&texture_id).expect("stored texture");
        let reference = render_solid_fan_reference(&mesh, texture, clip_bounds(12, 12));
        assert_eq!(fan_pixels, reference);
        assert_eq!(primitive_stats.solid_fan_runs, 1);
        assert_eq!(primitive_stats.solid_fan_triangles, 4);
        assert_eq!(primitive_stats.generic_triangles_rasterized, 0);
        assert_eq!(raster_stats.solid_fan_calls, 1);
        assert_eq!(raster_stats.solid_fan_triangles, 4);
        assert!(raster_stats.solid_fan_rows > 0);
        assert!(raster_stats.solid_fan_px > 0);
        assert_eq!(raster_stats.translucent_px, raster_stats.solid_fan_px);
    }

    fn quad_vertices() -> [egui::epaint::Vertex; 4] {
        [
            test_vertex(1.0, 1.0),
            test_vertex(4.0, 1.0),
            test_vertex(1.0, 3.0),
            test_vertex(4.0, 3.0),
        ]
    }

    fn textured_quad_mesh(texture_id: egui::TextureId) -> egui::Mesh {
        let mut vertices = quad_vertices();
        vertices[0].uv = egui::pos2(0.0, 0.0);
        vertices[1].uv = egui::pos2(1.0, 0.0);
        vertices[2].uv = egui::pos2(0.0, 1.0);
        vertices[3].uv = egui::pos2(1.0, 1.0);
        egui::Mesh {
            indices: vec![0, 1, 2, 1, 3, 2],
            vertices: vertices.to_vec(),
            texture_id,
        }
    }

    fn solid_fan_mesh(texture_id: egui::TextureId, color: [u8; 4]) -> egui::Mesh {
        let color = egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
        let vertices = [
            egui::pos2(1.0, 5.0),
            egui::pos2(2.0, 1.0),
            egui::pos2(6.0, 1.0),
            egui::pos2(9.0, 3.0),
            egui::pos2(8.0, 8.0),
            egui::pos2(3.0, 9.0),
        ]
        .map(|pos| egui::epaint::Vertex {
            pos,
            uv: egui::Pos2::ZERO,
            color,
        });
        egui::Mesh {
            indices: vec![1, 0, 2, 2, 0, 3, 3, 0, 4, 4, 0, 5],
            vertices: vertices.to_vec(),
            texture_id,
        }
    }

    fn render_solid_fan_reference(
        mesh: &egui::Mesh,
        texture: &TextureImage,
        clip: ClipBounds,
    ) -> Vec<u8> {
        let mut surface = SoftwareSurface::default();
        surface.resize(12, 12).expect("surface");
        surface.clear([0, 0, 0, 255]);
        for triangle in mesh.indices.chunks_exact(3) {
            let v0 = &mesh.vertices[usize::try_from(triangle[0]).expect("index")];
            let v1 = &mesh.vertices[usize::try_from(triangle[1]).expect("index")];
            let v2 = &mesh.vertices[usize::try_from(triangle[2]).expect("index")];
            rasterize_triangle(&mut surface, v0, v1, v2, texture, clip, None);
        }
        surface.pixels
    }

    fn texture_delta(texture_id: egui::TextureId) -> egui::TexturesDelta {
        let image = egui::ColorImage::new(
            [2, 2],
            vec![
                egui::Color32::from_rgb(10, 0, 0),
                egui::Color32::from_rgb(20, 0, 0),
                egui::Color32::from_rgb(30, 0, 0),
                egui::Color32::from_rgb(40, 0, 0),
            ],
        );
        egui::TexturesDelta {
            set: vec![(
                texture_id,
                egui::epaint::ImageDelta::full(image, egui::TextureOptions::NEAREST),
            )],
            free: Vec::new(),
        }
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
