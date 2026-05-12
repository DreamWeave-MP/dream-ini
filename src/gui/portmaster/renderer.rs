// SPDX-License-Identifier: GPL-3.0-only

use std::io;
use std::time::{Duration, Instant};

mod fan;
mod quad;
mod stats;

use super::log::write_log;
use super::pacing::format_repaint_delay;
use super::raster::{
    ClipBounds, RasterStats, SolidFanRasterParams, SolidFanSpanCache, classify_triangle,
    polygon_raster_bounds, rasterize_solid_fan_with_cache, rasterize_triangle, usize_to_f32,
};
use super::surface::SoftwareSurface;
use super::texture::{TextureDeltaStats, TextureImage, TextureStore};
use super::{GuiFrame, GuiShell};
use fan::{FanBoundaryKey, SolidFanPolygonScratch, SolidFanRun, solid_fan_run};
use quad::try_rasterize_quad_window;
use stats::{
    PrimitiveStats, RasterTimings, SolidFanRasterRecord, SolidFanRasterWork, TriangleSource,
};

#[derive(Debug)]
pub(super) struct SoftwareRenderer {
    surface: SoftwareSurface,
    textures: TextureStore,
    solid_fan_polygon_scratch: Vec<usize>,
    solid_fan_seen_boundary_scratch: Vec<FanBoundaryKey>,
    solid_fan_span_cache: SolidFanSpanCache,
}

const SOLID_FAN_POLYGON_SCRATCH_CAPACITY: usize = 4096;

impl Default for SoftwareRenderer {
    fn default() -> Self {
        Self {
            surface: SoftwareSurface::default(),
            textures: TextureStore::default(),
            solid_fan_polygon_scratch: Vec::with_capacity(SOLID_FAN_POLYGON_SCRATCH_CAPACITY),
            solid_fan_seen_boundary_scratch: Vec::with_capacity(SOLID_FAN_POLYGON_SCRATCH_CAPACITY),
            solid_fan_span_cache: SolidFanSpanCache::default(),
        }
    }
}

impl SoftwareRenderer {
    pub(super) fn render<S: GuiShell>(
        &mut self,
        width: usize,
        height: usize,
        frame: &mut GuiFrame<'_, S>,
    ) -> io::Result<RenderOutcome> {
        let log_timings = frame.log_frame || frame.hitch_log_threshold.is_some();
        let total_start = log_timings.then(Instant::now);
        let stage_start = log_timings.then(Instant::now);
        self.surface.resize(width, height)?;
        self.surface.clear([17, 20, 28, 255]);
        let resize_clear_elapsed = elapsed_micros(stage_start);

        let stage_start = log_timings.then(Instant::now);
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

        let stage_start = log_timings.then(Instant::now);
        let texture_delta_stats = self.textures.apply(&output.textures_delta)?;
        let texture_apply_elapsed = elapsed_micros(stage_start);

        let stage_start = log_timings.then(Instant::now);
        let primitives = frame.context.tessellate(output.shapes, 1.0);
        let primitive_count = primitives.len();
        let tessellate_elapsed = elapsed_micros(stage_start);
        log_surface_stats(
            frame,
            &self.surface,
            &self.textures,
            &texture_delta_stats,
            primitive_count,
        );

        let stage_start = log_timings.then(Instant::now);
        let collect_stats = should_collect_deep_stats(frame.log_render_stats);
        let mut primitive_stats = collect_stats.then(PrimitiveStats::default);
        let mut raster_stats = collect_stats.then(RasterStats::default);
        let mut raster_timings = collect_stats.then(RasterTimings::default);
        let rasterize_result = self.rasterize(
            &primitives,
            primitive_stats.as_mut(),
            raster_stats.as_mut(),
            raster_timings.as_mut(),
        );
        let rasterize_elapsed = elapsed_micros(stage_start);
        log_raster_stats(
            frame,
            &self.surface,
            primitive_stats.as_ref(),
            raster_stats.as_ref(),
            raster_timings.as_ref(),
        );
        rasterize_result?;

        let stage_start = log_timings.then(Instant::now);
        for id in output.textures_delta.free {
            self.textures.free(id);
        }
        let texture_free_elapsed = elapsed_micros(stage_start);
        let texture_evidence = TextureEvidence {
            count: self.textures.len(),
            bytes: self.textures.bytes_used(),
            set_count: texture_delta_stats.set_count,
            set_bytes: texture_delta_stats.set_bytes,
            full_upload_count: texture_delta_stats.full_upload_count,
            partial_update_count: texture_delta_stats.partial_update_count,
        };
        let total_elapsed = elapsed_micros(total_start);
        let timings = log_timings.then_some(RenderTimings {
            resize_clear: resize_clear_elapsed,
            egui_run: egui_run_elapsed,
            texture_apply: texture_apply_elapsed,
            tessellate: tessellate_elapsed,
            rasterize: rasterize_elapsed,
            texture_free: texture_free_elapsed,
            total: total_elapsed,
        });
        log_render_timings(frame, timings, repaint_delay);
        Ok(RenderOutcome {
            repaint_delay,
            timings,
            primitive_count,
            texture_evidence,
        })
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
        if let Some(stats) = raster_stats {
            self.solid_fan_span_cache.record_stats(stats);
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
        self.solid_fan_polygon_scratch.clear();
        self.solid_fan_seen_boundary_scratch.clear();
        let mut context = MeshRasterContext {
            surface: &mut self.surface,
            fan_polygon_scratch: &mut self.solid_fan_polygon_scratch,
            fan_seen_boundary_scratch: &mut self.solid_fan_seen_boundary_scratch,
            fan_span_cache: &mut self.solid_fan_span_cache,
            mesh,
            texture,
            clip,
            primitive_index,
            solid_fan_polygon_scratch_budget: SOLID_FAN_POLYGON_SCRATCH_CAPACITY,
        };
        rasterize_mesh_contents(
            &mut context,
            &mut stats,
            &mut raster_stats,
            &mut raster_timings,
        )?;
        Ok(())
    }
}

fn rasterize_mesh_contents(
    context: &mut MeshRasterContext<'_>,
    stats: &mut Option<&mut PrimitiveStats>,
    raster_stats: &mut Option<&mut RasterStats>,
    raster_timings: &mut Option<&mut RasterTimings>,
) -> io::Result<()> {
    let mut index_offset = 0;
    while index_offset + 2 < context.mesh.indices.len() {
        if context.try_rasterize_quad_at(index_offset, stats, raster_stats, raster_timings)? {
            index_offset += 6;
            continue;
        }

        if let Some(fan_triangle_count) =
            context.try_rasterize_solid_fan_at(index_offset, stats, raster_stats, raster_timings)?
        {
            index_offset += fan_triangle_count * 3;
            continue;
        }

        context.rasterize_generic_triangle_at(index_offset, stats, raster_stats, raster_timings)?;
        index_offset += 3;
    }
    Ok(())
}

struct MeshRasterContext<'a> {
    surface: &'a mut SoftwareSurface,
    fan_polygon_scratch: &'a mut Vec<usize>,
    fan_seen_boundary_scratch: &'a mut Vec<FanBoundaryKey>,
    fan_span_cache: &'a mut SolidFanSpanCache,
    mesh: &'a egui::Mesh,
    texture: &'a TextureImage,
    clip: ClipBounds,
    primitive_index: usize,
    solid_fan_polygon_scratch_budget: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RenderOutcome {
    pub(super) repaint_delay: Duration,
    pub(super) timings: Option<RenderTimings>,
    pub(super) primitive_count: usize,
    pub(super) texture_evidence: TextureEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TextureEvidence {
    pub(super) count: usize,
    pub(super) bytes: usize,
    pub(super) set_count: usize,
    pub(super) set_bytes: usize,
    pub(super) full_upload_count: usize,
    pub(super) partial_update_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RenderTimings {
    pub(super) resize_clear: u128,
    pub(super) egui_run: u128,
    pub(super) texture_apply: u128,
    pub(super) tessellate: u128,
    pub(super) rasterize: u128,
    pub(super) texture_free: u128,
    pub(super) total: u128,
}

fn log_surface_stats<S: GuiShell>(
    frame: &GuiFrame<'_, S>,
    surface: &SoftwareSurface,
    textures: &TextureStore,
    texture_delta_stats: &TextureDeltaStats,
    primitive_count: usize,
) {
    if frame.log_frame {
        write_log(
            frame.log,
            format!(
                "software renderer frame={} surface={}x{} bytes={} textures={} texture_bytes={} primitives={}",
                frame.frame_index,
                surface.width,
                surface.height,
                surface.pixels.len(),
                textures.len(),
                textures.bytes_used(),
                primitive_count,
            ),
        );
    }
    if frame.log_render_stats {
        write_log(
            frame.log,
            format!(
                "software renderer render_stats frame={} surface={}x{} surface_bytes={} texture_sets={} texture_set_bytes={} texture_full_uploads={} texture_partial_updates={} clipped_primitives={} repaint_request_due_before_frame={} requested_repaint_after_egui={}",
                frame.frame_index,
                surface.width,
                surface.height,
                surface.pixels.len(),
                texture_delta_stats.set_count,
                texture_delta_stats.set_bytes,
                texture_delta_stats.full_upload_count,
                texture_delta_stats.partial_update_count,
                primitive_count,
                frame.repaint_request_due_before_frame,
                frame.context.has_requested_repaint(),
            ),
        );
    }
}

fn log_raster_stats<S: GuiShell>(
    frame: &GuiFrame<'_, S>,
    surface: &SoftwareSurface,
    primitive_stats: Option<&PrimitiveStats>,
    raster_stats: Option<&RasterStats>,
    raster_timings: Option<&RasterTimings>,
) {
    if let Some(stats) = primitive_stats {
        write_log(frame.log, stats.log_line(frame.frame_index));
        if let Some(log_line) = stats.solid_triangle_offenders_log_line() {
            write_log(frame.log, log_line);
        }
        if let Some(log_line) = stats.textured_triangle_offenders_log_line() {
            write_log(frame.log, log_line);
        }
        if let Some(log_line) = stats.textured_quad_reject_offenders_log_line() {
            write_log(frame.log, log_line);
        }
        if let Some(log_line) = stats.solid_fan_offenders_log_line() {
            write_log(frame.log, log_line);
        }
    }
    if let Some(stats) = raster_stats {
        if frame.log_render_stats {
            write_log(
                frame.log,
                clear_evidence_log_line(frame.frame_index, surface.width, surface.height, stats),
            );
        }
        write_log(frame.log, stats.log_line());
    }
    if let Some(timings) = raster_timings {
        write_log(frame.log, timings.log_line());
    }
}

fn log_render_timings<S: GuiShell>(
    frame: &GuiFrame<'_, S>,
    timings: Option<RenderTimings>,
    repaint_delay: Duration,
) {
    if !frame.log_frame {
        return;
    }
    let Some(timings) = timings else {
        return;
    };
    let repaint_delay = format_repaint_delay(repaint_delay);
    write_log(
        frame.log,
        format!(
            "software renderer timings frame={} resize_clear_us={} egui_run_us={} texture_apply_us={} tessellate_us={} rasterize_us={} texture_free_us={} repaint_delay={repaint_delay} total_us={}",
            frame.frame_index,
            timings.resize_clear,
            timings.egui_run,
            timings.texture_apply,
            timings.tessellate,
            timings.rasterize,
            timings.texture_free,
            timings.total,
        ),
    );
}

fn clear_evidence_log_line(
    frame_index: u64,
    width: usize,
    height: usize,
    stats: &RasterStats,
) -> String {
    let clear_pixels = width.saturating_mul(height);
    let clear_bytes = clear_pixels.saturating_mul(4);
    let opaque_drawn_pixels = stats.opaque_px;
    let opaque_drawn_percent = opaque_drawn_pixels
        .saturating_mul(100)
        .checked_div(clear_pixels)
        .unwrap_or(0);
    format!(
        "software renderer clear_evidence frame={frame_index} clear_pixels={clear_pixels} clear_bytes={clear_bytes} opaque_drawn_pixels={opaque_drawn_pixels} opaque_drawn_percent_of_surface={opaque_drawn_percent}"
    )
}

impl MeshRasterContext<'_> {
    fn try_rasterize_quad_at(
        &mut self,
        index_offset: usize,
        stats: &mut Option<&mut PrimitiveStats>,
        raster_stats: &mut Option<&mut RasterStats>,
        raster_timings: &mut Option<&mut RasterTimings>,
    ) -> io::Result<bool> {
        if index_offset + 5 >= self.mesh.indices.len() {
            return Ok(false);
        }
        let quad = &self.mesh.indices[index_offset..index_offset + 6];
        let mut instrumentation = RasterInstrumentation {
            primitive_stats: borrow_optional_mut(stats),
            raster_stats: borrow_optional_mut(raster_stats),
            timings: borrow_optional_mut(raster_timings),
        };
        try_rasterize_quad_window(
            self.surface,
            self.mesh,
            self.texture,
            self.clip,
            quad,
            self.source(index_offset),
            &mut instrumentation,
        )
    }

    fn try_rasterize_solid_fan_at(
        &mut self,
        index_offset: usize,
        stats: &mut Option<&mut PrimitiveStats>,
        raster_stats: &mut Option<&mut RasterStats>,
        raster_timings: &mut Option<&mut RasterTimings>,
    ) -> io::Result<Option<usize>> {
        let fan_probe_start = raster_timings.as_ref().map(|_| Instant::now());
        let fan = solid_fan_run(
            self.mesh,
            self.texture,
            self.clip,
            index_offset,
            SolidFanPolygonScratch {
                polygon: &mut *self.fan_polygon_scratch,
                seen_boundaries: &mut *self.fan_seen_boundary_scratch,
                budget: self.solid_fan_polygon_scratch_budget,
            },
            stats.as_deref_mut().map(|stats| &mut stats.solid_fan_probe),
        )?;
        if let (Some(start), Some(timings)) = (fan_probe_start, borrow_optional_mut(raster_timings))
        {
            timings.solid_fan_probe += start.elapsed().as_micros();
        }
        let Some(fan) = fan else {
            self.fan_polygon_scratch.clear();
            return Ok(None);
        };
        let mut instrumentation = RasterInstrumentation {
            primitive_stats: borrow_optional_mut(stats),
            raster_stats: borrow_optional_mut(raster_stats),
            timings: borrow_optional_mut(raster_timings),
        };
        let source = self.source(index_offset);
        rasterize_solid_fan_run(
            SolidFanRasterRunContext {
                surface: self.surface,
                mesh: self.mesh,
                fan_span_cache: self.fan_span_cache,
                clip: self.clip,
                source,
            },
            &fan,
            self.fan_polygon_scratch,
            &mut instrumentation,
        );
        let fan_triangle_count = fan.triangle_count;
        self.fan_polygon_scratch.clear();
        Ok(Some(fan_triangle_count))
    }

    fn rasterize_generic_triangle_at(
        &mut self,
        index_offset: usize,
        stats: &mut Option<&mut PrimitiveStats>,
        raster_stats: &mut Option<&mut RasterStats>,
        raster_timings: &mut Option<&mut RasterTimings>,
    ) -> io::Result<()> {
        let Some([v0, v1, v2]) = mesh_triangle_vertices(self.mesh, index_offset)? else {
            return Ok(());
        };
        let source = self.source(index_offset);
        if let Some(stats) = borrow_optional_mut(stats) {
            stats.record_generic_triangle(v0, v1, v2, self.texture, self.clip, source);
        }
        rasterize_generic_triangle(
            self.surface,
            [v0, v1, v2],
            self.texture,
            self.clip,
            raster_stats,
            raster_timings,
        );
        Ok(())
    }

    const fn source(&self, index_offset: usize) -> TriangleSource {
        TriangleSource {
            primitive_index: self.primitive_index,
            mesh_index_offset: index_offset,
        }
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

fn rasterize_solid_fan_run(
    context: SolidFanRasterRunContext<'_>,
    fan: &SolidFanRun,
    fan_polygon: &[usize],
    instrumentation: &mut RasterInstrumentation<'_>,
) {
    let SolidFanRasterRunContext {
        surface,
        mesh,
        fan_span_cache,
        clip,
        source,
    } = context;
    let fan_work_before = instrumentation
        .raster_stats
        .as_deref()
        .map(SolidFanRasterWork::from_stats);
    let fan_bounds = instrumentation
        .primitive_stats
        .as_ref()
        .and_then(|_| polygon_raster_bounds(&mesh.vertices, &fan_polygon[..fan.polygon_len], clip));
    let fan_start = instrumentation.timing_start();
    rasterize_solid_fan_with_cache(
        surface,
        SolidFanRasterParams {
            vertices: &mesh.vertices,
            polygon: &fan_polygon[..fan.polygon_len],
            triangle_count: fan.triangle_count,
            color: fan.color,
            clip,
        },
        &mut instrumentation.raster_stats(),
        Some(fan_span_cache),
    );
    let elapsed_us = fan_start.map_or(0, |start| start.elapsed().as_micros());
    let fan_work_after = instrumentation
        .raster_stats
        .as_deref()
        .map(SolidFanRasterWork::from_stats);
    let fan_work = fan_work_before
        .zip(fan_work_after)
        .map_or_else(SolidFanRasterWork::default, |(before, after)| {
            after - before
        });
    if let Some(stats) = instrumentation.primitive_stats() {
        stats.solid_fan_runs += 1;
        stats.solid_fan_triangles += fan.triangle_count;
        stats.record_solid_fan(SolidFanRasterRecord {
            source,
            triangle_count: fan.triangle_count,
            polygon_vertices: fan.polygon_len,
            alpha: fan.color[3],
            bounds: fan_bounds,
            work: fan_work,
            elapsed_us,
        });
    }
    if let Some(timings) = instrumentation.timings() {
        timings.solid_fan_raster += elapsed_us;
    }
}

struct SolidFanRasterRunContext<'a> {
    surface: &'a mut SoftwareSurface,
    mesh: &'a egui::Mesh,
    fan_span_cache: &'a mut SolidFanSpanCache,
    clip: ClipBounds,
    source: TriangleSource,
}

struct RasterInstrumentation<'a> {
    primitive_stats: Option<&'a mut PrimitiveStats>,
    raster_stats: Option<&'a mut RasterStats>,
    timings: Option<&'a mut RasterTimings>,
}

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

const fn should_collect_deep_stats(log_render_stats: bool) -> bool {
    log_render_stats
}

fn borrow_optional_mut<'a, T>(option: &'a mut Option<&mut T>) -> Option<&'a mut T> {
    option.as_mut().map(|value| &mut **value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coarse_frame_logging_does_not_enable_deep_stats_collection() {
        assert!(!should_collect_deep_stats(false));
        assert!(should_collect_deep_stats(true));
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
        assert!(
            stats
                .textured_quad_reject_offenders_log_line()
                .expect("textured quad reject offenders")
                .contains("offender0_reason=non_affine_uv")
        );
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
        let mut raster_timings = RasterTimings::default();

        renderer
            .rasterize_mesh(
                &mesh,
                clip_rect,
                0,
                Some(&mut primitive_stats),
                Some(&mut raster_stats),
                Some(&mut raster_timings),
            )
            .expect("rasterize mesh");
        let fan_pixels = renderer.surface.pixels.clone();

        let texture = renderer.textures.get(&texture_id).expect("stored texture");
        let reference = render_solid_fan_reference(&mesh, texture, clip_bounds(12, 12));
        assert_eq!(fan_pixels, reference);
        assert_eq!(primitive_stats.solid_fan_runs, 1);
        assert_eq!(primitive_stats.solid_fan_triangles, 4);
        assert_eq!(primitive_stats.solid_fan_probe.probe_calls, 1);
        assert_eq!(primitive_stats.solid_fan_probe.center_slot_attempts, 2);
        assert_eq!(primitive_stats.solid_fan_probe.cheap_candidate_attempts, 2);
        assert_eq!(
            primitive_stats.solid_fan_probe.candidate_triangles_scanned,
            6
        );
        assert_eq!(primitive_stats.solid_fan_probe.repeated_boundary_checks, 3);
        assert_eq!(
            primitive_stats
                .solid_fan_probe
                .repeated_boundary_comparisons,
            9
        );
        assert_eq!(primitive_stats.solid_fan_probe.reject_no_candidate, 1);
        assert_eq!(primitive_stats.solid_fan_probe.accepted_runs, 1);
        assert_eq!(primitive_stats.solid_fan_probe.accepted_triangles, 4);
        assert_eq!(primitive_stats.solid_fan_probe.polygon_builds, 1);
        assert_eq!(primitive_stats.solid_fan_probe.max_candidate_triangles, 4);
        assert_eq!(primitive_stats.solid_fan_probe.max_accepted_triangles, 4);
        assert_eq!(primitive_stats.generic_triangles_rasterized, 0);
        assert_eq!(raster_stats.solid_fan_calls, 1);
        assert_eq!(raster_stats.solid_fan_triangles, 4);
        assert!(raster_stats.solid_fan_rows > 0);
        assert!(raster_stats.solid_fan_px > 0);
        assert_eq!(raster_stats.translucent_px, raster_stats.solid_fan_px);
        assert_eq!(raster_timings.generic_solid_triangle, 0);
        assert_eq!(raster_timings.generic_textured_triangle, 0);
        assert!(
            primitive_stats
                .solid_fan_offenders_log_line()
                .expect("solid fan offenders")
                .contains("offender0_triangles=4 offender0_polygon_vertices=6 offender0_alpha=128")
        );
    }

    #[test]
    fn renderer_reports_resident_solid_fan_cache_without_fan_call_in_frame() {
        let texture_id = egui::TextureId::Managed(1);
        let mut renderer = SoftwareRenderer::default();
        renderer.surface.resize(12, 12).expect("surface");
        renderer.surface.clear([0, 0, 0, 255]);
        renderer
            .textures
            .apply(&texture_delta(texture_id))
            .expect("texture");
        let clip_rect = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(12.0, 12.0));

        let mut first_frame_stats = RasterStats::default();
        let first_frame = egui::ClippedPrimitive {
            clip_rect,
            primitive: egui::epaint::Primitive::Mesh(solid_fan_mesh(texture_id, [128, 32, 0, 128])),
        };
        renderer
            .rasterize(&[first_frame], None, Some(&mut first_frame_stats), None)
            .expect("rasterize first frame");
        assert_eq!(first_frame_stats.solid_fan_span_cache_misses, 1);
        assert_eq!(first_frame_stats.solid_fan_span_cache_resident_entries, 1);
        assert!(first_frame_stats.solid_fan_span_cache_resident_rows > 0);

        let mut second_frame_stats = RasterStats::default();
        let second_frame = egui::ClippedPrimitive {
            clip_rect,
            primitive: egui::epaint::Primitive::Mesh(textured_quad_mesh(texture_id)),
        };
        renderer
            .rasterize(&[second_frame], None, Some(&mut second_frame_stats), None)
            .expect("rasterize second frame");

        assert_eq!(second_frame_stats.solid_fan_calls, 0);
        assert_eq!(second_frame_stats.solid_fan_span_cache_hits, 0);
        assert_eq!(second_frame_stats.solid_fan_span_cache_misses, 0);
        assert_eq!(second_frame_stats.solid_fan_span_cache_stored_rows, 0);
        assert_eq!(second_frame_stats.solid_fan_span_cache_resident_entries, 1);
        assert_eq!(
            second_frame_stats.solid_fan_span_cache_resident_rows,
            first_frame_stats.solid_fan_span_cache_resident_rows
        );
    }

    #[test]
    fn renderer_solid_fan_scratch_overflow_falls_back_to_generic_triangles() {
        let texture_id = egui::TextureId::Managed(1);
        let mut renderer = SoftwareRenderer::default();
        renderer.surface.resize(12, 12).expect("surface");
        renderer.surface.clear([0, 0, 0, 255]);
        renderer
            .textures
            .apply(&texture_delta(texture_id))
            .expect("texture");
        let mesh = solid_fan_mesh(texture_id, [128, 32, 0, 128]);
        let clip = clip_bounds(12, 12);
        let reference = render_solid_fan_reference(
            &mesh,
            renderer.textures.get(&texture_id).expect("stored texture"),
            clip,
        );
        let mut primitive_stats = PrimitiveStats::default();
        let mut raster_stats = RasterStats::default();

        {
            let mut stats = Some(&mut primitive_stats);
            let mut raster_stats = Some(&mut raster_stats);
            let mut raster_timings = None;
            let mut context = MeshRasterContext {
                surface: &mut renderer.surface,
                fan_polygon_scratch: &mut renderer.solid_fan_polygon_scratch,
                fan_seen_boundary_scratch: &mut renderer.solid_fan_seen_boundary_scratch,
                fan_span_cache: &mut renderer.solid_fan_span_cache,
                mesh: &mesh,
                texture: renderer.textures.get(&texture_id).expect("stored texture"),
                clip,
                primitive_index: 0,
                solid_fan_polygon_scratch_budget: 5,
            };
            rasterize_mesh_contents(
                &mut context,
                &mut stats,
                &mut raster_stats,
                &mut raster_timings,
            )
            .expect("rasterize mesh");
        }

        assert_eq!(renderer.surface.pixels, reference);
        assert_eq!(primitive_stats.solid_fan_probe.reject_scratch_overflow, 1);
        assert_eq!(primitive_stats.solid_fan_probe.polygon_builds, 0);
        assert_eq!(primitive_stats.solid_fan_probe.accepted_runs, 0);
        assert_eq!(primitive_stats.solid_fan_runs, 0);
        assert_eq!(primitive_stats.generic_triangles_rasterized, 4);
        assert_eq!(raster_stats.solid_fan_calls, 0);
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
