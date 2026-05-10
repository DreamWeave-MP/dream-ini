// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use super::surface::SoftwareSurface;
use super::texture::TextureImage;

#[derive(Clone, Copy, Debug)]
pub(super) struct ClipBounds {
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
}

impl ClipBounds {
    pub(super) fn new(rect: egui::Rect, width: usize, height: usize) -> io::Result<Self> {
        let min_x = clamp_rect_value(rect.min.x.floor(), width)?;
        let min_y = clamp_rect_value(rect.min.y.floor(), height)?;
        let max_x = clamp_rect_value(rect.max.x.ceil(), width)?;
        let max_y = clamp_rect_value(rect.max.y.ceil(), height)?;
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    pub(super) const fn is_empty(self) -> bool {
        self.min_x >= self.max_x || self.min_y >= self.max_y
    }
}

fn clamp_rect_value(value: f32, max: usize) -> io::Result<usize> {
    if !value.is_finite() {
        return Err(io::Error::other("non-finite clip rectangle value"));
    }
    Ok(f32_to_usize_floor_clamped(value, max))
}

pub(super) fn usize_to_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn f32_to_usize_floor_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.floor(), max)
}

fn f32_to_usize_ceil_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.ceil(), max)
}

fn f32_to_usize_round_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.round(), max)
}

fn f32_to_usize_threshold_clamped(value: f32, max: usize) -> usize {
    if value <= 0.0 {
        return 0;
    }
    let max_value = usize_to_f32(max);
    if value >= max_value {
        return max;
    }
    f32_to_usize_bounded(value.clamp(0.0, max_value))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped to a non-negative finite usize range before casting"
)]
fn f32_to_usize_bounded(value: f32) -> usize {
    value as usize
}

fn f32_to_u8_round_clamped(value: f32) -> u8 {
    let value = value.round().clamp(0.0, 255.0);
    f32_to_u8_bounded(value)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is rounded and clamped to the u8 range before casting"
)]
fn f32_to_u8_bounded(value: f32) -> u8 {
    value as u8
}

fn edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)))
}

fn edge_step_x(a: egui::Pos2, b: egui::Pos2) -> f32 {
    b.y - a.y
}

fn edge_step_y(a: egui::Pos2, b: egui::Pos2) -> f32 {
    -(b.x - a.x)
}

pub(super) fn rasterize_triangle(
    surface: &mut SoftwareSurface,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
    clip: ClipBounds,
) {
    let area = edge(v0.pos, v1.pos, v2.pos);
    if area.abs() <= f32::EPSILON {
        return;
    }
    let min_x = f32_to_usize_floor_clamped(v0.pos.x.min(v1.pos.x).min(v2.pos.x), clip.max_x)
        .max(clip.min_x);
    let max_x =
        f32_to_usize_ceil_clamped(v0.pos.x.max(v1.pos.x).max(v2.pos.x), clip.max_x).min(clip.max_x);
    let min_y = f32_to_usize_floor_clamped(v0.pos.y.min(v1.pos.y).min(v2.pos.y), clip.max_y)
        .max(clip.min_y);
    let max_y =
        f32_to_usize_ceil_clamped(v0.pos.y.max(v1.pos.y).max(v2.pos.y), clip.max_y).min(clip.max_y);
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    if let Some(color) = solid_triangle_color(v0, v1, v2, texture) {
        rasterize_solid_triangle(surface, v0, v1, v2, clip, area, color);
        return;
    }

    rasterize_textured_triangle(surface, v0, v1, v2, texture, clip, area);
}

fn rasterize_solid_triangle(
    surface: &mut SoftwareSurface,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    clip: ClipBounds,
    area: f32,
    color: [u8; 4],
) {
    let min_x = f32_to_usize_floor_clamped(v0.pos.x.min(v1.pos.x).min(v2.pos.x), clip.max_x)
        .max(clip.min_x);
    let max_x =
        f32_to_usize_ceil_clamped(v0.pos.x.max(v1.pos.x).max(v2.pos.x), clip.max_x).min(clip.max_x);
    let min_y = f32_to_usize_floor_clamped(v0.pos.y.min(v1.pos.y).min(v2.pos.y), clip.max_y)
        .max(clip.min_y);
    let max_y =
        f32_to_usize_ceil_clamped(v0.pos.y.max(v1.pos.y).max(v2.pos.y), clip.max_y).min(clip.max_y);

    let inv_area = 1.0 / area;
    let start = egui::pos2(usize_to_f32(min_x) + 0.5, usize_to_f32(min_y) + 0.5);
    let w0_step_x = edge_step_x(v1.pos, v2.pos);
    let w1_step_x = edge_step_x(v2.pos, v0.pos);
    let w2_step_x = edge_step_x(v0.pos, v1.pos);
    let w0_step_y = edge_step_y(v1.pos, v2.pos);
    let w1_step_y = edge_step_y(v2.pos, v0.pos);
    let w2_step_y = edge_step_y(v0.pos, v1.pos);
    let mut row_w0 = edge(v1.pos, v2.pos, start);
    let mut row_w1 = edge(v2.pos, v0.pos, start);
    let mut row_w2 = edge(v0.pos, v1.pos, start);

    for y in min_y..max_y {
        let mut raw_w0 = row_w0;
        let mut raw_w1 = row_w1;
        let mut raw_w2 = row_w2;
        for x in min_x..max_x {
            let w0 = raw_w0 * inv_area;
            let w1 = raw_w1 * inv_area;
            let w2 = raw_w2 * inv_area;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                surface.blend_pixel(x, y, color);
            }
            raw_w0 += w0_step_x;
            raw_w1 += w1_step_x;
            raw_w2 += w2_step_x;
        }
        row_w0 += w0_step_y;
        row_w1 += w1_step_y;
        row_w2 += w2_step_y;
    }
}

fn rasterize_textured_triangle(
    surface: &mut SoftwareSurface,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
    clip: ClipBounds,
    area: f32,
) {
    let min_x = f32_to_usize_floor_clamped(v0.pos.x.min(v1.pos.x).min(v2.pos.x), clip.max_x)
        .max(clip.min_x);
    let max_x =
        f32_to_usize_ceil_clamped(v0.pos.x.max(v1.pos.x).max(v2.pos.x), clip.max_x).min(clip.max_x);
    let min_y = f32_to_usize_floor_clamped(v0.pos.y.min(v1.pos.y).min(v2.pos.y), clip.max_y)
        .max(clip.min_y);
    let max_y =
        f32_to_usize_ceil_clamped(v0.pos.y.max(v1.pos.y).max(v2.pos.y), clip.max_y).min(clip.max_y);

    let inv_area = 1.0 / area;
    let start = egui::pos2(usize_to_f32(min_x) + 0.5, usize_to_f32(min_y) + 0.5);
    let w0_step_x = edge_step_x(v1.pos, v2.pos);
    let w1_step_x = edge_step_x(v2.pos, v0.pos);
    let w2_step_x = edge_step_x(v0.pos, v1.pos);
    let w0_step_y = edge_step_y(v1.pos, v2.pos);
    let w1_step_y = edge_step_y(v2.pos, v0.pos);
    let w2_step_y = edge_step_y(v0.pos, v1.pos);
    let mut row_w0 = edge(v1.pos, v2.pos, start);
    let mut row_w1 = edge(v2.pos, v0.pos, start);
    let mut row_w2 = edge(v0.pos, v1.pos, start);

    for y in min_y..max_y {
        let mut raw_w0 = row_w0;
        let mut raw_w1 = row_w1;
        let mut raw_w2 = row_w2;
        for x in min_x..max_x {
            let w0 = raw_w0 * inv_area;
            let w1 = raw_w1 * inv_area;
            let w2 = raw_w2 * inv_area;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let uv = egui::pos2(
                    v0.uv.x.mul_add(w0, v1.uv.x.mul_add(w1, v2.uv.x * w2)),
                    v0.uv.y.mul_add(w0, v1.uv.y.mul_add(w1, v2.uv.y * w2)),
                );
                let vertex_color = interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2);
                let texture_color = sample_nearest(texture, uv);
                let color = modulate_color(vertex_color, texture_color);
                surface.blend_pixel(x, y, color);
            }
            raw_w0 += w0_step_x;
            raw_w1 += w1_step_x;
            raw_w2 += w2_step_x;
        }
        row_w0 += w0_step_y;
        row_w1 += w1_step_y;
        row_w2 += w2_step_y;
    }
}

fn solid_triangle_color(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> Option<[u8; 4]> {
    if v0.color != v1.color || v0.color != v2.color {
        return None;
    }

    if texture.width == 0 || texture.height == 0 {
        return Some(modulate_color(
            color_to_array(v0.color),
            [255, 255, 255, 255],
        ));
    }

    let t0 = nearest_texel(texture, v0.uv);
    if t0 != nearest_texel(texture, v1.uv) || t0 != nearest_texel(texture, v2.uv) {
        return None;
    }

    Some(modulate_color(
        color_to_array(v0.color),
        texel_color(texture, t0),
    ))
}

fn nearest_texel(texture: &TextureImage, uv: egui::Pos2) -> (usize, usize) {
    let x = f32_to_usize_round_clamped(
        uv.x.clamp(0.0, 1.0) * usize_to_f32(texture.width.saturating_sub(1)),
        texture.width.saturating_sub(1),
    );
    let y = f32_to_usize_round_clamped(
        uv.y.clamp(0.0, 1.0) * usize_to_f32(texture.height.saturating_sub(1)),
        texture.height.saturating_sub(1),
    );
    (x, y)
}

fn texel_color(texture: &TextureImage, texel: (usize, usize)) -> [u8; 4] {
    let offset = (texel.1 * texture.width + texel.0) * 4;
    [
        texture.pixels[offset],
        texture.pixels[offset + 1],
        texture.pixels[offset + 2],
        texture.pixels[offset + 3],
    ]
}

fn color_to_array(color: egui::Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

fn sample_nearest(texture: &TextureImage, uv: egui::Pos2) -> [u8; 4] {
    if texture.width == 0 || texture.height == 0 {
        return [255, 255, 255, 255];
    }
    texel_color(texture, nearest_texel(texture, uv))
}

fn interpolate_color(
    c0: egui::Color32,
    c1: egui::Color32,
    c2: egui::Color32,
    w0: f32,
    w1: f32,
    w2: f32,
) -> [u8; 4] {
    [
        interpolate_channel(c0.r(), c1.r(), c2.r(), w0, w1, w2),
        interpolate_channel(c0.g(), c1.g(), c2.g(), w0, w1, w2),
        interpolate_channel(c0.b(), c1.b(), c2.b(), w0, w1, w2),
        interpolate_channel(c0.a(), c1.a(), c2.a(), w0, w1, w2),
    ]
}

fn interpolate_channel(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> u8 {
    let value = f32::from(c0).mul_add(w0, f32::from(c1).mul_add(w1, f32::from(c2) * w2));
    f32_to_u8_round_clamped(value)
}

fn modulate_color(vertex: [u8; 4], texture: [u8; 4]) -> [u8; 4] {
    [
        multiply_u8(vertex[0], texture[0]),
        multiply_u8(vertex[1], texture[1]),
        multiply_u8(vertex[2], texture[2]),
        multiply_u8(vertex[3], texture[3]),
    ]
}

fn multiply_u8(a: u8, b: u8) -> u8 {
    u8::try_from((u16::from(a) * u16::from(b) + 127) / 255).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usize_conversions_clamp_floor_ceil_and_round() {
        assert_eq!(f32_to_usize_floor_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_floor_clamped(1.75, 10), 1);
        assert_eq!(f32_to_usize_floor_clamped(12.0, 10), 10);

        assert_eq!(f32_to_usize_ceil_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_ceil_clamped(1.25, 10), 2);
        assert_eq!(f32_to_usize_ceil_clamped(12.0, 10), 10);

        assert_eq!(f32_to_usize_round_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_round_clamped(1.49, 10), 1);
        assert_eq!(f32_to_usize_round_clamped(1.5, 10), 2);
        assert_eq!(f32_to_usize_round_clamped(12.0, 10), 10);
    }

    #[test]
    fn u8_conversion_rounds_and_clamps() {
        assert_eq!(f32_to_u8_round_clamped(-1.0), 0);
        assert_eq!(f32_to_u8_round_clamped(1.49), 1);
        assert_eq!(f32_to_u8_round_clamped(1.5), 2);
        assert_eq!(f32_to_u8_round_clamped(300.0), 255);
    }

    #[test]
    fn nearest_texture_sampling_clamps_to_edges() {
        let texture = TextureImage {
            width: 2,
            height: 2,
            pixels: vec![
                1, 0, 0, 255, // top-left
                2, 0, 0, 255, // top-right
                3, 0, 0, 255, // bottom-left
                4, 0, 0, 255, // bottom-right
            ],
        };

        assert_eq!(sample_nearest(&texture, egui::pos2(-1.0, -1.0))[0], 1);
        assert_eq!(sample_nearest(&texture, egui::pos2(2.0, -1.0))[0], 2);
        assert_eq!(sample_nearest(&texture, egui::pos2(-1.0, 2.0))[0], 3);
        assert_eq!(sample_nearest(&texture, egui::pos2(2.0, 2.0))[0], 4);
    }

    #[test]
    fn triangle_rasterizer_draws_into_tiny_surface() {
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(2.0, 0.0);
        let v2 = test_vertex(0.0, 2.0);

        let pixels = render_test_triangle(2, 2, &v0, &v1, &v2);

        assert_eq!(white_pixel_count(&pixels), 3);
    }

    #[test]
    fn triangle_rasterizer_draws_clockwise_and_counter_clockwise_consistently() {
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(3.0, 0.0);
        let v2 = test_vertex(0.0, 3.0);

        let counter_clockwise = render_test_triangle(3, 3, &v0, &v1, &v2);
        let clockwise = render_test_triangle(3, 3, &v0, &v2, &v1);

        assert_eq!(counter_clockwise, clockwise);
        assert_eq!(white_pixel_count(&counter_clockwise), 6);
    }

    #[test]
    fn solid_triangle_fast_path_matches_generic_for_same_texel_uvs() {
        let texture = test_texture_2x2();
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(4.0, 0.0);
        let mut v2 = test_vertex(0.0, 4.0);
        v0.color = egui::Color32::from_rgba_premultiplied(128, 64, 32, 255);
        v1.color = v0.color;
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(0.2, 0.2);
        v2.uv = egui::pos2(0.49, 0.49);

        assert_eq!(
            solid_triangle_color(&v0, &v1, &v2, &texture),
            Some(modulate_color(
                color_to_array(v0.color),
                [64, 128, 192, 255]
            ))
        );
        assert_eq!(
            render_test_triangle_with(4, 4, &v0, &v1, &v2, &texture),
            render_test_triangle_generic(4, 4, &v0, &v1, &v2, &texture)
        );
    }

    #[test]
    fn solid_triangle_fast_path_matches_generic_for_empty_texture() {
        let texture = TextureImage {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(3.0, 0.0);
        let mut v2 = test_vertex(0.0, 3.0);
        v0.color = egui::Color32::from_rgba_premultiplied(20, 40, 60, 128);
        v1.color = v0.color;
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(1.0, 0.0);
        v2.uv = egui::pos2(0.0, 1.0);

        assert_eq!(
            solid_triangle_color(&v0, &v1, &v2, &texture),
            Some(color_to_array(v0.color))
        );
        assert_eq!(
            render_test_triangle_with(3, 3, &v0, &v1, &v2, &texture),
            render_test_triangle_generic(3, 3, &v0, &v1, &v2, &texture)
        );
    }

    #[test]
    fn solid_triangle_fast_path_rejects_non_uniform_color_or_uv() {
        let texture = test_texture_2x2();
        let mut v0 = test_vertex(0.0, 0.0);
        let mut v1 = test_vertex(3.0, 0.0);
        let mut v2 = test_vertex(0.0, 3.0);
        v0.color = egui::Color32::from_rgb(64, 64, 64);
        v1.color = egui::Color32::from_rgb(65, 64, 64);
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(0.0, 0.0);
        v2.uv = egui::pos2(0.0, 0.0);

        assert_eq!(solid_triangle_color(&v0, &v1, &v2, &texture), None);

        v1.color = v0.color;
        v2.color = v0.color;
        v0.uv = egui::pos2(0.0, 0.0);
        v1.uv = egui::pos2(1.0, 0.0);
        v2.uv = egui::pos2(0.0, 0.0);

        assert_eq!(solid_triangle_color(&v0, &v1, &v2, &texture), None);
    }

    fn render_test_triangle(
        width: usize,
        height: usize,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
    ) -> Vec<u8> {
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };

        render_test_triangle_with(width, height, v0, v1, v2, &texture)
    }

    fn render_test_triangle_with(
        width: usize,
        height: usize,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        texture: &TextureImage,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);

        rasterize_triangle(
            &mut surface,
            v0,
            v1,
            v2,
            texture,
            ClipBounds {
                min_x: 0,
                min_y: 0,
                max_x: width,
                max_y: height,
            },
        );

        surface.pixels
    }

    fn render_test_triangle_generic(
        width: usize,
        height: usize,
        v0: &egui::epaint::Vertex,
        v1: &egui::epaint::Vertex,
        v2: &egui::epaint::Vertex,
        texture: &TextureImage,
    ) -> Vec<u8> {
        let mut surface = test_surface(width, height);
        let area = edge(v0.pos, v1.pos, v2.pos);

        rasterize_textured_triangle(
            &mut surface,
            v0,
            v1,
            v2,
            texture,
            ClipBounds {
                min_x: 0,
                min_y: 0,
                max_x: width,
                max_y: height,
            },
            area,
        );

        surface.pixels
    }

    fn test_surface(width: usize, height: usize) -> SoftwareSurface {
        let mut surface = SoftwareSurface::default();
        surface.resize(width, height).expect("surface");
        surface.clear([0, 0, 0, 255]);
        surface
    }

    fn white_pixel_count(pixels: &[u8]) -> usize {
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255)
            .count()
    }

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            color: egui::Color32::WHITE,
            uv: egui::Pos2::ZERO,
        }
    }

    fn test_texture_2x2() -> TextureImage {
        TextureImage {
            width: 2,
            height: 2,
            pixels: vec![
                64, 128, 192, 255, // top-left
                255, 0, 0, 255, // top-right
                0, 255, 0, 255, // bottom-left
                0, 0, 255, 255, // bottom-right
            ],
        }
    }
}
