// SPDX-License-Identifier: GPL-3.0-only

use std::io;
use std::time::{Duration, Instant};

use super::log::write_log;
use super::raster::{
    ClipBounds, TriangleClassification, classify_triangle, is_axis_aligned_quad,
    rasterize_axis_aligned_solid_quad, rasterize_triangle, usize_to_f32,
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
        let rasterize_result = self.rasterize(&primitives, primitive_stats.as_mut());
        let rasterize_elapsed = elapsed_micros(stage_start);
        if let Some(stats) = primitive_stats {
            write_log(frame.log, stats.log_line());
        }
        rasterize_result?;

        let stage_start = log_frame.then(Instant::now);
        for id in output.textures_delta.free {
            self.textures.free(id);
        }
        let texture_free_elapsed = elapsed_micros(stage_start);
        let total_elapsed = elapsed_micros(total_start);
        if log_frame {
            write_log(
                frame.log,
                format!(
                    "software renderer timings resize_clear_us={resize_clear_elapsed} egui_run_us={egui_run_elapsed} texture_apply_us={texture_apply_elapsed} tessellate_us={tessellate_elapsed} rasterize_us={rasterize_elapsed} texture_free_us={texture_free_elapsed} repaint_delay={repaint_delay:?} total_us={total_elapsed}"
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
    ) -> io::Result<()> {
        for primitive in primitives {
            match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    if let Some(stats) = stats.as_deref_mut() {
                        stats.mesh_primitives += 1;
                    }
                    self.rasterize_mesh(mesh, primitive.clip_rect, stats.as_deref_mut())?;
                }
                egui::epaint::Primitive::Callback(_) => {
                    if let Some(stats) = stats.as_deref_mut() {
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
        mut stats: Option<&mut PrimitiveStats>,
    ) -> io::Result<()> {
        if let Some(stats) = stats.as_deref_mut() {
            stats.mesh_indices += mesh.indices.len();
        }
        let Some(texture) = self.textures.get(&mesh.texture_id) else {
            if let Some(stats) = stats.as_deref_mut() {
                stats.missing_texture_meshes += 1;
            }
            return Ok(());
        };
        let clip = ClipBounds::new(clip_rect, self.surface.width, self.surface.height)?;
        if clip.is_empty() {
            if let Some(stats) = stats.as_deref_mut() {
                stats.empty_clip_meshes += 1;
            }
            return Ok(());
        }
        let surface = &mut self.surface;
        let mut index_offset = 0;
        while index_offset + 2 < mesh.indices.len() {
            if index_offset + 5 < mesh.indices.len() {
                let quad = &mesh.indices[index_offset..index_offset + 6];
                if let Some(stats) = stats.as_deref_mut() {
                    stats.quad_windows += 1;
                }
                if has_four_unique_indices(quad) {
                    if let Some(stats) = stats.as_deref_mut() {
                        stats.four_unique_quad_windows += 1;
                    }
                    let i0 = mesh_index_to_usize(quad[0])?;
                    let i1 = mesh_index_to_usize(quad[1])?;
                    let i2 = mesh_index_to_usize(quad[2])?;
                    let i3 = mesh_index_to_usize(quad[3])?;
                    let i4 = mesh_index_to_usize(quad[4])?;
                    let i5 = mesh_index_to_usize(quad[5])?;
                    if let (Some(v0), Some(v1), Some(v2), Some(v3), Some(v4), Some(v5)) = (
                        mesh.vertices.get(i0),
                        mesh.vertices.get(i1),
                        mesh.vertices.get(i2),
                        mesh.vertices.get(i3),
                        mesh.vertices.get(i4),
                        mesh.vertices.get(i5),
                    ) {
                        if let Some(stats) = stats.as_deref_mut() {
                            stats.record_quad_window([v0, v1, v2, v3, v4, v5], texture);
                        }
                        if rasterize_axis_aligned_solid_quad(
                            surface,
                            [v0, v1, v2, v3, v4, v5],
                            texture,
                            clip,
                        ) {
                            if let Some(stats) = stats.as_deref_mut() {
                                stats.solid_quad_fast_path_hits += 1;
                            }
                            index_offset += 6;
                            continue;
                        }
                    }
                }
            }

            let triangle = &mesh.indices[index_offset..index_offset + 3];
            let i0 = usize::try_from(triangle[0])
                .map_err(|_| io::Error::other("mesh index does not fit usize"))?;
            let i1 = usize::try_from(triangle[1])
                .map_err(|_| io::Error::other("mesh index does not fit usize"))?;
            let i2 = usize::try_from(triangle[2])
                .map_err(|_| io::Error::other("mesh index does not fit usize"))?;
            let Some(v0) = mesh.vertices.get(i0) else {
                continue;
            };
            let Some(v1) = mesh.vertices.get(i1) else {
                continue;
            };
            let Some(v2) = mesh.vertices.get(i2) else {
                continue;
            };
            if let Some(stats) = stats.as_deref_mut() {
                stats.record_generic_triangle(v0, v1, v2, texture);
            }
            rasterize_triangle(surface, v0, v1, v2, texture, clip);
            index_offset += 3;
        }
        Ok(())
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
    axis_aligned_quad_windows: usize,
    solid_axis_aligned_quad_windows: usize,
    textured_axis_aligned_quad_windows: usize,
    solid_quad_fast_path_hits: usize,
    generic_triangles_rasterized: usize,
    generic_solid_triangles: usize,
    generic_textured_triangles: usize,
    degenerate_triangles: usize,
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
    ) {
        self.generic_triangles_rasterized += 1;
        match classify_triangle(v0, v1, v2, texture) {
            TriangleClassification::Degenerate => self.degenerate_triangles += 1,
            TriangleClassification::Solid => {
                self.generic_solid_triangles += 1;
            }
            TriangleClassification::Textured => {
                self.generic_textured_triangles += 1;
            }
        }
    }

    fn log_line(&self) -> String {
        format!(
            "software renderer primitive_stats mesh_primitives={} callback_primitives={} missing_texture_meshes={} empty_clip_meshes={} mesh_indices={} quad_windows={} four_unique_quad_windows={} axis_aligned_quad_windows={} solid_axis_aligned_quad_windows={} textured_axis_aligned_quad_windows={} solid_quad_fast_path_hits={} generic_triangles_rasterized={} generic_solid_triangles={} generic_textured_triangles={} degenerate_triangles={}",
            self.mesh_primitives,
            self.callback_primitives,
            self.missing_texture_meshes,
            self.empty_clip_meshes,
            self.mesh_indices,
            self.quad_windows,
            self.four_unique_quad_windows,
            self.axis_aligned_quad_windows,
            self.solid_axis_aligned_quad_windows,
            self.textured_axis_aligned_quad_windows,
            self.solid_quad_fast_path_hits,
            self.generic_triangles_rasterized,
            self.generic_solid_triangles,
            self.generic_textured_triangles,
            self.degenerate_triangles,
        )
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
        let mut stats = PrimitiveStats::default();

        stats.record_generic_triangle(&v0, &v1, &v2, &texture);
        v2.color = egui::Color32::BLACK;
        stats.record_generic_triangle(&v0, &v1, &v2, &texture);
        stats.record_generic_triangle(&v0, &v1, &v0, &texture);

        assert_eq!(stats.generic_triangles_rasterized, 3);
        assert_eq!(stats.generic_solid_triangles, 1);
        assert_eq!(stats.generic_textured_triangles, 1);
        assert_eq!(stats.degenerate_triangles, 1);
        assert!(stats.log_line().contains("generic_solid_triangles=1"));
        assert!(stats.log_line().contains("degenerate_triangles=1"));
    }

    fn quad_vertices() -> [egui::epaint::Vertex; 4] {
        [
            test_vertex(1.0, 1.0),
            test_vertex(4.0, 1.0),
            test_vertex(1.0, 3.0),
            test_vertex(4.0, 3.0),
        ]
    }

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::Pos2::ZERO,
            color: egui::Color32::WHITE,
        }
    }
}
