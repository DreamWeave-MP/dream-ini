// SPDX-License-Identifier: GPL-3.0-only

use super::texture::TextureImage;
use crate::gui::portmaster::raster::usize_to_f32;

const TEXTURE_WIDTH: usize = 64;
const TEXTURE_HEIGHT: usize = 4;
const VERTEX_COLOR: [u8; 4] = [192, 208, 224, 255];
const BACKGROUND_COLOR: [u8; 4] = [17, 20, 28, 255];
const LT4_RECTS: usize = 20;
const MID_RECTS: usize = 152;
const WIDE_RECTS: usize = 20;
const LT4_WIDTH: usize = 2;
const MID_WIDTH: usize = 5;
const WIDE_WIDTH: usize = 10;
const RECT_GAP: usize = 1;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SampledRectModulatedWorkload {
    viewport_width: usize,
    viewport_height: usize,
    mesh: egui::Mesh,
    texture: TextureImage,
    stats: SampledRectModulatedWorkloadStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SampledRectModulatedWorkloadStats {
    pub(super) rects: usize,
    pub(super) pixels: usize,
    pub(super) lt4_pixels: usize,
    pub(super) mid_pixels: usize,
    pub(super) wide_pixels: usize,
    pub(super) texture_transparent_lanes: usize,
    pub(super) texture_translucent_lanes: usize,
    pub(super) texture_opaque_lanes: usize,
}

impl SampledRectModulatedWorkload {
    pub(super) const DESCRIPTION: &'static str = "deterministic sampled separable textured rects with uniform non-white vertex modulation; fixed span mix target lt4=4% 4..7=76% 8..15=20%; 64x4 contiguous mixed-alpha texture";

    pub(super) fn new(viewport_width: usize, viewport_height: usize) -> Self {
        let texture = benchmark_texture();
        let (mesh, stats) = benchmark_mesh(viewport_width, viewport_height, &texture);
        Self {
            viewport_width,
            viewport_height,
            mesh,
            texture,
            stats,
        }
    }

    pub(super) const fn matches_viewport(&self, width: usize, height: usize) -> bool {
        self.viewport_width == width && self.viewport_height == height
    }

    pub(super) fn mesh(&self) -> &egui::Mesh {
        &self.mesh
    }

    pub(super) fn texture(&self) -> &TextureImage {
        &self.texture
    }

    pub(super) const fn stats(&self) -> SampledRectModulatedWorkloadStats {
        self.stats
    }

    pub(super) fn config_log_line(&self, frame_limit: u64) -> String {
        format!(
            "portmaster render benchmark config name=sampled-rect-modulated frames={} workload={:?} viewport={}x{} rects={} pixels={} span_px_lt4={} span_px_4_7={} span_px_8_15={} target_distribution=lt4:4%,4..7:76%,8..15:20%,16_plus:0% vertex_color_rgba={:?} texture={}x{} texture_pattern=deterministic-contiguous-mixed-alpha texture_alpha_lanes transparent={} translucent={} opaque={} fixed_workload_per_frame=true",
            frame_limit,
            Self::DESCRIPTION,
            self.viewport_width,
            self.viewport_height,
            self.stats.rects,
            self.stats.pixels,
            self.stats.lt4_pixels,
            self.stats.mid_pixels,
            self.stats.wide_pixels,
            VERTEX_COLOR,
            self.texture.width,
            self.texture.height,
            self.stats.texture_transparent_lanes,
            self.stats.texture_translucent_lanes,
            self.stats.texture_opaque_lanes,
        )
    }
}

fn benchmark_mesh(
    viewport_width: usize,
    viewport_height: usize,
    texture: &TextureImage,
) -> (egui::Mesh, SampledRectModulatedWorkloadStats) {
    let rects = LT4_RECTS + MID_RECTS + WIDE_RECTS;
    let mut mesh = egui::Mesh {
        indices: Vec::with_capacity(rects * 6),
        vertices: Vec::with_capacity(rects * 4),
        texture_id: egui::TextureId::Managed(0),
    };
    let mut cursor = MeshCursor::new(viewport_width, viewport_height);
    let mut stats = SampledRectModulatedWorkloadStats {
        rects,
        pixels: 0,
        lt4_pixels: 0,
        mid_pixels: 0,
        wide_pixels: 0,
        texture_transparent_lanes: 0,
        texture_translucent_lanes: 0,
        texture_opaque_lanes: 0,
    };

    for _ in 0..LT4_RECTS {
        push_rect(&mut mesh, &mut cursor, LT4_WIDTH, 0, texture.width);
        stats.lt4_pixels += LT4_WIDTH;
    }
    for index in 0..MID_RECTS {
        push_rect(&mut mesh, &mut cursor, MID_WIDTH, index, texture.width);
        stats.mid_pixels += MID_WIDTH;
    }
    for index in 0..WIDE_RECTS {
        push_rect(&mut mesh, &mut cursor, WIDE_WIDTH, index, texture.width);
        stats.wide_pixels += WIDE_WIDTH;
    }
    stats.pixels = stats.lt4_pixels + stats.mid_pixels + stats.wide_pixels;
    let (transparent, translucent, opaque) = texture_alpha_lane_counts(texture);
    stats.texture_transparent_lanes = transparent;
    stats.texture_translucent_lanes = translucent;
    stats.texture_opaque_lanes = opaque;
    (mesh, stats)
}

struct MeshCursor {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl MeshCursor {
    const fn new(width: usize, height: usize) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn place(&mut self, rect_width: usize) -> (usize, usize) {
        let row_width = self.width.max(rect_width);
        if self.x + rect_width > row_width {
            self.x = 0;
            self.y = self.y.saturating_add(1);
        }
        if self.y >= self.height.max(1) {
            self.y = 0;
        }
        let position = (self.x, self.y);
        self.x = self.x.saturating_add(rect_width + RECT_GAP);
        position
    }
}

fn push_rect(
    mesh: &mut egui::Mesh,
    cursor: &mut MeshCursor,
    rect_width: usize,
    index: usize,
    texture_width: usize,
) {
    let (x, y) = cursor.place(rect_width);
    let base = u32::try_from(mesh.vertices.len()).expect("benchmark mesh vertex count fits u32");
    let max_texel = usize_to_f32(texture_width - 1);
    let source_x = index % (texture_width - rect_width);
    let uv_left = (usize_to_f32(source_x) - 0.5) / max_texel;
    let uv_right = usize_to_f32(source_x + rect_width) - 0.5;
    let uv_right = uv_right / max_texel;
    let texel_y = (index / texture_width) % TEXTURE_HEIGHT;
    let uv_y = usize_to_f32(texel_y) / usize_to_f32(TEXTURE_HEIGHT - 1);
    let min = egui::pos2(usize_to_f32(x), usize_to_f32(y));
    let max = egui::pos2(usize_to_f32(x + rect_width), usize_to_f32(y + 1));
    mesh.vertices.extend_from_slice(&[
        vertex(min.x, min.y, uv_left, uv_y),
        vertex(max.x, min.y, uv_right, uv_y),
        vertex(min.x, max.y, uv_left, uv_y),
        vertex(max.x, max.y, uv_right, uv_y),
    ]);
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
}

fn vertex(x: f32, y: f32, uv_x: f32, uv_y: f32) -> egui::epaint::Vertex {
    egui::epaint::Vertex {
        pos: egui::pos2(x, y),
        uv: egui::pos2(uv_x, uv_y),
        color: egui::Color32::from_rgba_premultiplied(
            VERTEX_COLOR[0],
            VERTEX_COLOR[1],
            VERTEX_COLOR[2],
            VERTEX_COLOR[3],
        ),
    }
}

fn benchmark_texture() -> TextureImage {
    let mut pixels = Vec::with_capacity(TEXTURE_WIDTH * TEXTURE_HEIGHT * 4);
    for y in 0..TEXTURE_HEIGHT {
        for x in 0..TEXTURE_WIDTH {
            let alpha = match (x + y * 3) % 8 {
                0 | 5 => 0,
                2 | 6 => 255,
                1 => 96,
                3 => 160,
                4 => 64,
                _ => 208,
            };
            let red = 32 + u8::try_from((x * 37 + y * 19) % 192).expect("red fits u8");
            let green = 48 + u8::try_from((x * 17 + y * 43) % 176).expect("green fits u8");
            let blue = 64 + u8::try_from((x * 29 + y * 11) % 160).expect("blue fits u8");
            pixels.extend_from_slice(&[red, green, blue, alpha]);
        }
    }
    TextureImage {
        width: TEXTURE_WIDTH,
        height: TEXTURE_HEIGHT,
        pixels,
    }
}

fn texture_alpha_lane_counts(texture: &TextureImage) -> (usize, usize, usize) {
    let mut transparent = 0;
    let mut translucent = 0;
    let mut opaque = 0;
    for pixel in texture.pixels.chunks_exact(4) {
        match pixel[3] {
            0 => transparent += 1,
            255 => opaque += 1,
            _ => translucent += 1,
        }
    }
    (transparent, translucent, opaque)
}

pub(super) const fn background_color() -> [u8; 4] {
    BACKGROUND_COLOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_rect_modulated_workload_has_deterministic_span_buckets() {
        let left = SampledRectModulatedWorkload::new(640, 480);
        let right = SampledRectModulatedWorkload::new(640, 480);

        assert_eq!(left.stats(), right.stats());
        assert_eq!(left.mesh.vertices, right.mesh.vertices);
        assert_eq!(left.mesh.indices, right.mesh.indices);
        assert_eq!(left.texture.pixels, right.texture.pixels);
        assert_eq!(left.stats.rects, 192);
        assert_eq!(left.stats.pixels, 1_000);
        assert_eq!(left.stats.lt4_pixels, 40);
        assert_eq!(left.stats.mid_pixels, 760);
        assert_eq!(left.stats.wide_pixels, 200);
    }

    #[test]
    fn sampled_rect_modulated_texture_is_mixed_alpha_dominant() {
        let workload = SampledRectModulatedWorkload::new(640, 480);
        let stats = workload.stats();

        assert!(stats.texture_translucent_lanes > stats.texture_transparent_lanes);
        assert!(stats.texture_translucent_lanes > stats.texture_opaque_lanes);
        assert!(stats.texture_transparent_lanes > 0);
        assert!(stats.texture_opaque_lanes > 0);
    }
}
