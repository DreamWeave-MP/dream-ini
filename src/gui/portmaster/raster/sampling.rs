// SPDX-License-Identifier: GPL-3.0-only

use super::super::texture::TextureImage;
use super::math::{f32_to_usize_round_clamped, usize_to_f32};
use super::types::TriangleTexelSample;

pub(in crate::gui::portmaster) fn triangle_nearest_texel_sample(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
) -> TriangleTexelSample {
    if texture.width == 0 || texture.height == 0 {
        return TriangleTexelSample {
            texels: None,
            uniform_color: Some([255, 255, 255, 255]),
        };
    }

    let texels = [
        nearest_texel(texture, v0.uv),
        nearest_texel(texture, v1.uv),
        nearest_texel(texture, v2.uv),
    ];
    let uniform_color = if texels[0] == texels[1] && texels[0] == texels[2] {
        Some(texel_color(texture, texels[0]))
    } else {
        None
    };

    TriangleTexelSample {
        texels: Some(texels),
        uniform_color,
    }
}

pub(super) fn nearest_texel(texture: &TextureImage, uv: egui::Pos2) -> (usize, usize) {
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

pub(super) fn texel_color(texture: &TextureImage, texel: (usize, usize)) -> [u8; 4] {
    let offset = (texel.1 * texture.width + texel.0) * 4;
    [
        texture.pixels[offset],
        texture.pixels[offset + 1],
        texture.pixels[offset + 2],
        texture.pixels[offset + 3],
    ]
}

pub(super) fn sample_nearest(texture: &TextureImage, uv: egui::Pos2) -> [u8; 4] {
    if texture.width == 0 || texture.height == 0 {
        return [255, 255, 255, 255];
    }
    texel_color(texture, nearest_texel(texture, uv))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
