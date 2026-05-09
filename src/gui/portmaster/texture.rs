// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::io;

use super::{f32_to_usize_round_clamped, usize_to_f32};

const MAX_TEXTURE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Default)]
pub(super) struct TextureStore {
    textures: HashMap<egui::TextureId, TextureImage>,
}

impl TextureStore {
    pub(super) fn apply(&mut self, delta: &egui::TexturesDelta) -> io::Result<()> {
        for (id, image_delta) in &delta.set {
            self.set(*id, image_delta)?;
        }
        Ok(())
    }

    fn set(&mut self, id: egui::TextureId, delta: &egui::epaint::ImageDelta) -> io::Result<()> {
        let metadata = TextureImageMetadata::from_image_data(&delta.image)?;
        if let Some(pos) = delta.pos {
            self.textures
                .get(&id)
                .ok_or_else(|| io::Error::other("partial texture update for missing texture"))?
                .validate_update_bounds(pos, metadata.width, metadata.height)?;
            let image = TextureImage::from_image_data(&delta.image, metadata);
            let texture = self
                .textures
                .get_mut(&id)
                .ok_or_else(|| io::Error::other("partial texture update for missing texture"))?;
            texture.update(pos, &image)
        } else {
            let bytes_used = self.bytes_used();
            let old_len = self
                .textures
                .get(&id)
                .map_or(0, |texture| texture.pixels.len());
            check_texture_budget(bytes_used, old_len, metadata.byte_len)?;
            let image = TextureImage::from_image_data(&delta.image, metadata);
            self.textures.insert(id, image);
            Ok(())
        }
    }

    pub(super) fn free(&mut self, id: egui::TextureId) {
        self.textures.remove(&id);
    }

    pub(super) fn get(&self, id: &egui::TextureId) -> Option<&TextureImage> {
        self.textures.get(id)
    }

    pub(super) fn len(&self) -> usize {
        self.textures.len()
    }

    pub(super) fn bytes_used(&self) -> usize {
        self.textures
            .values()
            .map(|texture| texture.pixels.len())
            .sum()
    }
}

#[derive(Debug, Clone, Copy)]
struct TextureImageMetadata {
    width: usize,
    height: usize,
    byte_len: usize,
}

impl TextureImageMetadata {
    fn from_image_data(image: &egui::ImageData) -> io::Result<Self> {
        match image {
            egui::ImageData::Color(color_image) => {
                let [width, height] = color_image.size;
                let pixel_count = width
                    .checked_mul(height)
                    .ok_or_else(|| io::Error::other("texture pixel count overflow"))?;
                if color_image.pixels.len() != pixel_count {
                    return Err(io::Error::other(format!(
                        "texture pixel count mismatch: {} != {pixel_count}",
                        color_image.pixels.len()
                    )));
                }
                let byte_len = pixel_count
                    .checked_mul(4)
                    .ok_or_else(|| io::Error::other("texture byte count overflow"))?;
                Ok(Self {
                    width,
                    height,
                    byte_len,
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TextureImage {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) pixels: Vec<u8>,
}

impl TextureImage {
    fn from_image_data(image: &egui::ImageData, metadata: TextureImageMetadata) -> Self {
        match image {
            egui::ImageData::Color(color_image) => {
                let mut pixels = Vec::with_capacity(metadata.byte_len);
                for color in &color_image.pixels {
                    pixels.extend_from_slice(&[color.r(), color.g(), color.b(), color.a()]);
                }
                Self {
                    width: metadata.width,
                    height: metadata.height,
                    pixels,
                }
            }
        }
    }

    fn update(&mut self, pos: [usize; 2], image: &Self) -> io::Result<()> {
        self.validate_update_bounds(pos, image.width, image.height)?;
        for row in 0..image.height {
            let destination = ((pos[1] + row) * self.width + pos[0]) * 4;
            let source = row * image.width * 4;
            let byte_count = image.width * 4;
            self.pixels[destination..destination + byte_count]
                .copy_from_slice(&image.pixels[source..source + byte_count]);
        }
        Ok(())
    }

    fn validate_update_bounds(
        &self,
        pos: [usize; 2],
        width: usize,
        height: usize,
    ) -> io::Result<()> {
        let x_end = pos[0]
            .checked_add(width)
            .ok_or_else(|| io::Error::other("partial texture x range overflow"))?;
        let y_end = pos[1]
            .checked_add(height)
            .ok_or_else(|| io::Error::other("partial texture y range overflow"))?;
        if x_end > self.width || y_end > self.height {
            return Err(io::Error::other(
                "partial texture update exceeds texture bounds",
            ));
        }
        Ok(())
    }

    pub(super) fn sample_nearest(&self, uv: egui::Pos2) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [255, 255, 255, 255];
        }
        let x = f32_to_usize_round_clamped(
            uv.x.clamp(0.0, 1.0) * usize_to_f32(self.width.saturating_sub(1)),
            self.width.saturating_sub(1),
        );
        let y = f32_to_usize_round_clamped(
            uv.y.clamp(0.0, 1.0) * usize_to_f32(self.height.saturating_sub(1)),
            self.height.saturating_sub(1),
        );
        let offset = (y * self.width + x) * 4;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ]
    }
}

fn check_texture_budget(bytes_used: usize, old_len: usize, new_len: usize) -> io::Result<()> {
    let used = bytes_used
        .checked_sub(old_len)
        .and_then(|bytes| bytes.checked_add(new_len))
        .ok_or_else(|| io::Error::other("texture byte budget overflow"))?;
    if used > MAX_TEXTURE_BYTES {
        return Err(io::Error::other(format!(
            "texture byte budget exceeded: {used} > {MAX_TEXTURE_BYTES}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn texture_partial_update_checks_bounds() {
        let mut texture = TextureImage {
            width: 2,
            height: 2,
            pixels: vec![0; 16],
        };
        let patch = TextureImage {
            width: 2,
            height: 1,
            pixels: vec![255; 8],
        };

        let error = texture
            .update([1, 1], &patch)
            .expect_err("patch crosses bounds");

        assert_eq!(
            error.to_string(),
            "partial texture update exceeds texture bounds"
        );
    }

    #[test]
    fn texture_budget_rejects_excess_bytes() {
        let error = check_texture_budget(MAX_TEXTURE_BYTES, 0, 4).expect_err("over budget");

        assert_eq!(
            error.to_string(),
            format!(
                "texture byte budget exceeded: {} > {MAX_TEXTURE_BYTES}",
                MAX_TEXTURE_BYTES + 4
            )
        );
    }

    #[test]
    fn texture_preflight_rejects_mismatched_pixel_count() {
        let image = egui::ImageData::Color(Arc::new(egui::ColorImage {
            size: [2, 2],
            source_size: egui::vec2(2.0, 2.0),
            pixels: vec![egui::Color32::WHITE; 3],
        }));

        let error = TextureImageMetadata::from_image_data(&image).expect_err("mismatched pixels");

        assert_eq!(error.to_string(), "texture pixel count mismatch: 3 != 4");
    }

    #[test]
    fn texture_store_rejects_over_budget_before_storing() {
        let mut store = TextureStore::default();
        let pixel_count = (MAX_TEXTURE_BYTES / 4) + 1;
        let image =
            egui::ColorImage::new([pixel_count, 1], vec![egui::Color32::WHITE; pixel_count]);
        let delta = egui::epaint::ImageDelta::full(image, egui::TextureOptions::NEAREST);

        let error = store
            .set(egui::TextureId::Managed(0), &delta)
            .expect_err("over-budget texture");

        assert_eq!(
            error.to_string(),
            format!(
                "texture byte budget exceeded: {} > {MAX_TEXTURE_BYTES}",
                MAX_TEXTURE_BYTES + 4
            )
        );
        assert_eq!(store.len(), 0);
    }
}
