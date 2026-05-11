// SPDX-License-Identifier: GPL-3.0-only

use std::io;
use std::time::{Duration, Instant};

mod quad;
mod stats;

use super::log::write_log;
use super::pacing::format_repaint_delay;
use super::raster::{
    ClipBounds, RasterStats, SolidTriangleColorDecision, classify_triangle, rasterize_solid_fan,
    rasterize_triangle, solid_triangle_color_decision, triangle_raster_bounds, usize_to_f32,
};
use super::surface::SoftwareSurface;
use super::texture::TextureImage;
use super::texture::TextureStore;
use super::{GuiFrame, GuiShell};
use quad::try_rasterize_quad_window;
use stats::{PrimitiveStats, RasterTimings, TriangleSource};

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

fn mesh_index_to_usize(index: u32) -> io::Result<usize> {
    usize::try_from(index).map_err(|_| io::Error::other("mesh index does not fit usize"))
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

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::Pos2::ZERO,
            color: egui::Color32::WHITE,
        }
    }
}
