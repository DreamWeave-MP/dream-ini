// SPDX-License-Identifier: GPL-3.0-only

use std::io;

// Covers 640x480 and 1280x720 handheld fbdev targets, while still rejecting surprise desktop-sized framebuffers.
const MAX_RENDER_PIXELS: usize = 1280 * 720;

#[derive(Debug, Default)]
pub(super) struct SoftwareSurface {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) pixels: Vec<u8>,
}

impl SoftwareSurface {
    pub(super) fn resize(&mut self, width: usize, height: usize) -> io::Result<()> {
        let pixels = width
            .checked_mul(height)
            .ok_or_else(|| io::Error::other("software surface pixel count overflow"))?;
        if pixels > MAX_RENDER_PIXELS {
            return Err(io::Error::other(format!(
                "software surface pixel budget exceeded: {pixels} > {MAX_RENDER_PIXELS}"
            )));
        }
        let bytes = pixels
            .checked_mul(4)
            .ok_or_else(|| io::Error::other("software surface byte count overflow"))?;
        if self.width != width || self.height != height || self.pixels.len() != bytes {
            self.pixels.resize(bytes, 0);
            self.width = width;
            self.height = height;
        }
        Ok(())
    }

    pub(super) fn clear(&mut self, color: [u8; 4]) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
    }

    pub(super) fn blend_pixel(&mut self, x: usize, y: usize, color: [u8; 4]) {
        let offset = (y * self.width + x) * 4;
        alpha_blend(&mut self.pixels[offset..offset + 4], color);
    }
}

fn alpha_blend(destination: &mut [u8], source: [u8; 4]) {
    let inverse_alpha = u16::from(u8::MAX - source[3]);
    // egui::Color32 stores premultiplied-alpha sRGBA. Do not multiply the
    // source channels by alpha again here unless darker fringes around every
    // translucent primitive sound like entertainment.
    destination[0] = blend_premultiplied_channel(source[0], destination[0], inverse_alpha);
    destination[1] = blend_premultiplied_channel(source[1], destination[1], inverse_alpha);
    destination[2] = blend_premultiplied_channel(source[2], destination[2], inverse_alpha);
    destination[3] = u8::MAX;
}

fn blend_premultiplied_channel(source: u8, destination: u8, inverse_alpha: u16) -> u8 {
    u8::try_from(u16::from(source) + ((u16::from(destination) * inverse_alpha + 127) / 255))
        .unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_blend_places_premultiplied_half_alpha_over_opaque_destination() {
        let mut destination = [0, 0, 255, 255];

        alpha_blend(&mut destination, [128, 0, 0, 128]);

        assert_eq!(destination, [128, 0, 127, 255]);
    }

    #[test]
    fn software_surface_accepts_hd_handheld_framebuffer() {
        let mut surface = SoftwareSurface::default();

        surface.resize(1280, 720).expect("HD handheld surface");

        assert_eq!(surface.width, 1280);
        assert_eq!(surface.height, 720);
        assert_eq!(surface.pixels.len(), MAX_RENDER_PIXELS * 4);
    }

    #[test]
    fn software_surface_rejects_pixel_budget_overflow() {
        let mut surface = SoftwareSurface::default();

        let error = surface
            .resize(MAX_RENDER_PIXELS + 1, 1)
            .expect_err("oversized surface");

        assert_eq!(
            error.to_string(),
            format!(
                "software surface pixel budget exceeded: {} > {MAX_RENDER_PIXELS}",
                MAX_RENDER_PIXELS + 1
            )
        );
    }
}
