// SPDX-License-Identifier: GPL-3.0-only

use std::io;
use std::time::{Duration, Instant};

use super::log::write_log;
use super::raster::{
    ClipBounds, rasterize_axis_aligned_solid_quad, rasterize_triangle, usize_to_f32,
};
use super::surface::SoftwareSurface;
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
        self.rasterize(&primitives)?;
        let rasterize_elapsed = elapsed_micros(stage_start);

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

    fn rasterize(&mut self, primitives: &[egui::ClippedPrimitive]) -> io::Result<()> {
        for primitive in primitives {
            match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    self.rasterize_mesh(mesh, primitive.clip_rect)?;
                }
                egui::epaint::Primitive::Callback(_) => {
                    return Err(io::Error::other(
                        "unsupported egui paint callback in software renderer",
                    ));
                }
            }
        }
        Ok(())
    }

    fn rasterize_mesh(&mut self, mesh: &egui::Mesh, clip_rect: egui::Rect) -> io::Result<()> {
        let Some(texture) = self.textures.get(&mesh.texture_id) else {
            return Ok(());
        };
        let clip = ClipBounds::new(clip_rect, self.surface.width, self.surface.height)?;
        if clip.is_empty() {
            return Ok(());
        }
        let surface = &mut self.surface;
        let mut index_offset = 0;
        while index_offset + 2 < mesh.indices.len() {
            if index_offset + 5 < mesh.indices.len() {
                let quad = &mesh.indices[index_offset..index_offset + 6];
                if has_four_unique_indices(quad) {
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
                    ) && rasterize_axis_aligned_solid_quad(
                        surface,
                        [v0, v1, v2, v3, v4, v5],
                        texture,
                        clip,
                    ) {
                        index_offset += 6;
                        continue;
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
            rasterize_triangle(surface, v0, v1, v2, texture, clip);
            index_offset += 3;
        }
        Ok(())
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
