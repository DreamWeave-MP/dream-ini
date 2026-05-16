// SPDX-License-Identifier: GPL-3.0-only

use std::io;

// Covers 640x480 and 1280x720 handheld fbdev targets, while still rejecting surprise desktop-sized framebuffers.
const MAX_RENDER_PIXELS: usize = 1280 * 720;
const MAX_BLEND_PRODUCT: u16 = 255 * 255;
const BLEND_LOOKUP_TABLE_SIDE: usize = 256;
const BLEND_LOOKUP_TABLE_SIDE_U16: u16 = 256;
const BLEND_LOOKUP_TABLE_LEN: usize = BLEND_LOOKUP_TABLE_SIDE * BLEND_LOOKUP_TABLE_SIDE;
#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
const SPAN_NEON_PIXELS: usize = 16;

macro_rules! blend_lookup_table {
    ($($inverse_alpha:literal),* $(,)?) => {
        [$(build_blend_lookup_row($inverse_alpha)),*]
    };
}

static BLEND_PRODUCT_BY_255_ROUNDED: [[u8; BLEND_LOOKUP_TABLE_SIDE]; BLEND_LOOKUP_TABLE_SIDE] = blend_lookup_table!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,
    98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116,
    117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
    136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154,
    155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173,
    174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192,
    193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211,
    212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230,
    231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249,
    250, 251, 252, 253, 254, 255
);

const fn build_blend_lookup_row(inverse_alpha: u16) -> [u8; BLEND_LOOKUP_TABLE_SIDE] {
    let mut row = [0; BLEND_LOOKUP_TABLE_SIDE];
    let mut index = 0;
    let mut destination = 0;
    while destination < BLEND_LOOKUP_TABLE_SIDE_U16 {
        row[index] = divide_blend_product_by_255_rounded(destination * inverse_alpha);
        index += 1;
        destination += 1;
    }
    row
}

fn blend_lookup_entry(destination: u8, inverse_alpha: u8) -> u8 {
    BLEND_PRODUCT_BY_255_ROUNDED[usize::from(inverse_alpha)][usize::from(destination)]
}

#[derive(Debug, Default)]
pub(super) struct SoftwareSurface {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) pixels: Vec<u8>,
}

impl SoftwareSurface {
    pub(super) fn resize(&mut self, width: usize, height: usize) -> io::Result<bool> {
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
            return Ok(true);
        }
        Ok(false)
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

    pub(super) const fn row_offset(&self, y: usize) -> usize {
        y * self.width * 4
    }

    pub(super) fn write_opaque_pixel_at_offset(&mut self, offset: usize, color: [u8; 4]) {
        self.pixels[offset..offset + 4].copy_from_slice(&color);
    }

    pub(super) fn blend_translucent_pixel_at_offset(&mut self, offset: usize, color: [u8; 4]) {
        blend_translucent_premultiplied_over_opaque_destination(
            &mut self.pixels[offset..offset + 4],
            color,
        );
    }

    pub(super) fn blend_constant_color_span_at_offset(
        &mut self,
        offset: usize,
        len: usize,
        color: [u8; 4],
    ) {
        match color[3] {
            0 => {}
            u8::MAX => self.write_opaque_span_at_offset(offset, len, color),
            _ => self.blend_translucent_span_at_offset(offset, len, color),
        }
    }

    fn write_opaque_span_at_offset(&mut self, offset: usize, len: usize, color: [u8; 4]) {
        let end = offset + len * 4;
        for pixel in self.pixels[offset..end].chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
    }

    fn blend_translucent_span_at_offset(&mut self, offset: usize, len: usize, color: [u8; 4]) {
        let end = offset + len * 4;
        blend_constant_premultiplied_span_rgba(&mut self.pixels[offset..end], color);
    }

    pub(super) fn blend_span(&mut self, y: usize, start_x: usize, end_x: usize, color: [u8; 4]) {
        if color[3] == 0 {
            return;
        }

        let start = (y * self.width + start_x) * 4;
        let end = (y * self.width + end_x) * 4;

        if color[3] == u8::MAX {
            for pixel in self.pixels[start..end].chunks_exact_mut(4) {
                pixel.copy_from_slice(&color);
            }
            return;
        }

        blend_constant_premultiplied_span_rgba(&mut self.pixels[start..end], color);
    }
}

fn blend_constant_premultiplied_span_rgba(span: &mut [u8], source: [u8; 4]) {
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    {
        let vector_bytes = span.len() / (SPAN_NEON_PIXELS * 4) * (SPAN_NEON_PIXELS * 4);
        let (vector_span, tail) = span.split_at_mut(vector_bytes);

        // SAFETY: vector_bytes is rounded down to complete 16-pixel RGBA
        // vectors, and tail pixels are handled by the scalar implementation.
        unsafe { blend_constant_premultiplied_span_rgba_neon(vector_span, source) };
        blend_constant_premultiplied_span_rgba_scalar(tail, source);
    }

    #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
    blend_constant_premultiplied_span_rgba_scalar(span, source);
}

fn blend_constant_premultiplied_span_rgba_scalar(span: &mut [u8], source: [u8; 4]) {
    let inverse_alpha = u8::MAX - source[3];
    for pixel in span.chunks_exact_mut(4) {
        blend_translucent_premultiplied_over_opaque_destination_with_inverse_alpha(
            pixel,
            source,
            inverse_alpha,
        );
    }
}

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
unsafe fn blend_constant_premultiplied_span_rgba_neon(span: &mut [u8], source: [u8; 4]) {
    use core::arch::aarch64::{
        uint8x8_t, uint8x16_t, uint8x16x4_t, uint16x8_t, vaddq_u16, vcombine_u8, vdup_n_u8,
        vdupq_n_u8, vdupq_n_u16, vget_high_u8, vget_low_u8, vld4q_u8, vmovn_u16, vmull_u8,
        vqaddq_u8, vshrq_n_u16, vst4q_u8,
    };

    unsafe fn divide_product(product: uint16x8_t) -> uint8x8_t {
        let biased = unsafe { vaddq_u16(product, vdupq_n_u16(128)) };
        let correction = unsafe { vshrq_n_u16(biased, 8) };
        let quotient = unsafe { vshrq_n_u16(vaddq_u16(biased, correction), 8) };
        unsafe { vmovn_u16(quotient) }
    }

    unsafe fn blend_channel(
        destination: uint8x16_t,
        source: uint8x16_t,
        inverse_alpha: uint8x8_t,
    ) -> uint8x16_t {
        let low_product = unsafe { vmull_u8(vget_low_u8(destination), inverse_alpha) };
        let high_product = unsafe { vmull_u8(vget_high_u8(destination), inverse_alpha) };
        let blend =
            unsafe { vcombine_u8(divide_product(low_product), divide_product(high_product)) };
        unsafe { vqaddq_u8(source, blend) }
    }

    debug_assert_eq!(span.len() % (SPAN_NEON_PIXELS * 4), 0);

    let inverse_alpha = unsafe { vdup_n_u8(u8::MAX - source[3]) };
    let red = unsafe { vdupq_n_u8(source[0]) };
    let green = unsafe { vdupq_n_u8(source[1]) };
    let blue = unsafe { vdupq_n_u8(source[2]) };
    let alpha = unsafe { vdupq_n_u8(u8::MAX) };

    for pixel_block in span.chunks_exact_mut(SPAN_NEON_PIXELS * 4) {
        let destination = unsafe { vld4q_u8(pixel_block.as_ptr()) };
        let blended = uint8x16x4_t(
            unsafe { blend_channel(destination.0, red, inverse_alpha) },
            unsafe { blend_channel(destination.1, green, inverse_alpha) },
            unsafe { blend_channel(destination.2, blue, inverse_alpha) },
            alpha,
        );
        unsafe { vst4q_u8(pixel_block.as_mut_ptr(), blended) };
    }
}

fn alpha_blend(destination: &mut [u8], source: [u8; 4]) {
    match source[3] {
        0 => return,
        u8::MAX => {
            destination.copy_from_slice(&source);
            return;
        }
        _ => {}
    }

    let inverse_alpha = u8::MAX - source[3];
    // egui::Color32 stores premultiplied-alpha sRGBA. Do not multiply the
    // source channels by alpha again here unless darker fringes around every
    // translucent primitive sound like entertainment.
    destination[0] = blend_premultiplied_channel(source[0], destination[0], inverse_alpha);
    destination[1] = blend_premultiplied_channel(source[1], destination[1], inverse_alpha);
    destination[2] = blend_premultiplied_channel(source[2], destination[2], inverse_alpha);
    destination[3] = u8::MAX;
}

fn blend_translucent_premultiplied_over_opaque_destination(
    destination: &mut [u8],
    source: [u8; 4],
) {
    let inverse_alpha = u8::MAX - source[3];
    blend_translucent_premultiplied_over_opaque_destination_with_inverse_alpha(
        destination,
        source,
        inverse_alpha,
    );
}

fn blend_translucent_premultiplied_over_opaque_destination_with_inverse_alpha(
    destination: &mut [u8],
    source: [u8; 4],
    inverse_alpha: u8,
) {
    destination[0] = blend_premultiplied_channel(source[0], destination[0], inverse_alpha);
    destination[1] = blend_premultiplied_channel(source[1], destination[1], inverse_alpha);
    destination[2] = blend_premultiplied_channel(source[2], destination[2], inverse_alpha);
    destination[3] = u8::MAX;
}

fn blend_premultiplied_channel(source: u8, destination: u8, inverse_alpha: u8) -> u8 {
    let blend = blend_lookup_entry(destination, inverse_alpha);
    u8::try_from(u16::from(source) + u16::from(blend)).unwrap_or(u8::MAX)
}

const fn divide_blend_product_by_255_rounded(product: u16) -> u8 {
    debug_assert!(product <= MAX_BLEND_PRODUCT);
    let biased = product + 128;
    let quotient = (biased + (biased >> 8)) >> 8;
    debug_assert!(quotient <= 255);
    quotient.to_le_bytes()[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_divide_by_255_matches_reference_for_blend_products() {
        for destination in u8::MIN..=u8::MAX {
            for inverse_alpha in u8::MIN..=u8::MAX {
                let product = u16::from(destination) * u16::from(inverse_alpha);
                let reference = (product + 127) / 255;

                assert_eq!(
                    u16::from(divide_blend_product_by_255_rounded(product)),
                    reference
                );
            }
        }
    }

    #[test]
    fn blend_lookup_table_matches_reference_for_all_entries() {
        assert_eq!(BLEND_PRODUCT_BY_255_ROUNDED.len(), 256);
        assert_eq!(BLEND_PRODUCT_BY_255_ROUNDED[0].len(), 256);
        assert_eq!(
            BLEND_PRODUCT_BY_255_ROUNDED.len() * BLEND_PRODUCT_BY_255_ROUNDED[0].len(),
            BLEND_LOOKUP_TABLE_LEN
        );

        for inverse_alpha in u8::MIN..=u8::MAX {
            for destination in u8::MIN..=u8::MAX {
                let product = u16::from(destination) * u16::from(inverse_alpha);
                let reference = u8::try_from((product + 127) / 255).expect("blend product");

                assert_eq!(blend_lookup_entry(destination, inverse_alpha), reference);
            }
        }
    }

    #[test]
    fn alpha_blend_skips_transparent_source() {
        let mut destination = [11, 22, 33, 255];

        alpha_blend(&mut destination, [255, 0, 0, 0]);

        assert_eq!(destination, [11, 22, 33, 255]);
    }

    #[test]
    fn alpha_blend_overwrites_with_opaque_source() {
        let mut destination = [11, 22, 33, 255];

        alpha_blend(&mut destination, [44, 55, 66, 255]);

        assert_eq!(destination, [44, 55, 66, 255]);
    }

    #[test]
    fn alpha_blend_places_premultiplied_half_alpha_over_opaque_destination() {
        let mut destination = [0, 0, 255, 255];

        alpha_blend(&mut destination, [128, 0, 0, 128]);

        assert_eq!(destination, [128, 0, 127, 255]);
    }

    #[test]
    fn blend_span_skips_transparent_source() {
        let mut surface = SoftwareSurface::default();
        surface.resize(4, 1).expect("surface");
        surface
            .pixels
            .copy_from_slice(&[1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]);

        surface.blend_span(0, 1, 3, [200, 100, 50, 0]);

        assert_eq!(
            surface.pixels,
            [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
    }

    #[test]
    fn blend_span_overwrites_opaque_source_over_multiple_pixels() {
        let mut surface = SoftwareSurface::default();
        surface.resize(4, 1).expect("surface");
        surface
            .pixels
            .copy_from_slice(&[1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]);

        surface.blend_span(0, 1, 3, [44, 55, 66, 255]);

        assert_eq!(
            surface.pixels,
            [
                1, 2, 3, 255, 44, 55, 66, 255, 44, 55, 66, 255, 10, 11, 12, 255
            ]
        );
    }

    #[test]
    fn blend_span_matches_alpha_blend_for_translucent_source() {
        let color = [100, 25, 0, 128];
        let mut surface = SoftwareSurface::default();
        surface.resize(4, 1).expect("surface");
        surface.pixels.copy_from_slice(&[
            1, 2, 3, 255, 20, 40, 60, 255, 70, 80, 90, 255, 10, 11, 12, 255,
        ]);
        let mut expected = surface.pixels.clone();

        for pixel in expected[4..12].chunks_exact_mut(4) {
            alpha_blend(pixel, color);
        }
        surface.blend_span(0, 1, 3, color);

        assert_eq!(surface.pixels, expected);
    }

    #[test]
    fn constant_premultiplied_span_matches_explicit_scalar_reference() {
        for width in [0, 1, 3, 7, 8, 15, 16, 17, 31, 32, 33] {
            for alpha in [1_u8, 2, 63, 127, 128, 191, 254] {
                for source_rgb in [
                    [0, 0, 0],
                    [1, 0, alpha],
                    [alpha / 2, alpha.saturating_sub(1), alpha],
                    [alpha, alpha, alpha],
                    [254, 255, 1],
                    [255, 255, 255],
                ] {
                    for pattern_seed in [0, 17, 251] {
                        let source = [source_rgb[0], source_rgb[1], source_rgb[2], alpha];
                        let mut by_span = patterned_pixels(width, pattern_seed);
                        let mut expected = by_span.clone();

                        blend_constant_premultiplied_span_rgba(&mut by_span, source);
                        blend_constant_premultiplied_span_rgba_reference(&mut expected, source);

                        assert_eq!(by_span, expected, "width {width}, source {source:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn offset_opaque_write_matches_blend_pixel_for_opaque_source() {
        let color = [44, 55, 66, 255];
        let mut by_offset = SoftwareSurface::default();
        by_offset.resize(3, 2).expect("surface");
        by_offset.clear([1, 2, 3, 255]);
        let mut by_blend = SoftwareSurface::default();
        by_blend.resize(3, 2).expect("surface");
        by_blend.clear([1, 2, 3, 255]);

        by_offset.write_opaque_pixel_at_offset(by_offset.row_offset(1) + 2 * 4, color);
        by_blend.blend_pixel(2, 1, color);

        assert_eq!(by_offset.pixels, by_blend.pixels);
    }

    #[test]
    fn offset_translucent_blend_matches_alpha_blend_for_translucent_source() {
        let color = [100, 25, 0, 128];
        let mut by_offset = SoftwareSurface::default();
        by_offset.resize(3, 2).expect("surface");
        by_offset.clear([20, 40, 60, 255]);
        let mut by_blend = SoftwareSurface::default();
        by_blend.resize(3, 2).expect("surface");
        by_blend.clear([20, 40, 60, 255]);

        by_offset.blend_translucent_pixel_at_offset(by_offset.row_offset(1) + 4, color);
        by_blend.blend_pixel(1, 1, color);

        assert_eq!(by_offset.pixels, by_blend.pixels);
    }

    #[test]
    fn offset_span_skips_transparent_source_like_repeated_pixel_blend() {
        assert_offset_span_matches_repeated_pixel_blend([100, 25, 0, 0], 2, 3);
    }

    #[test]
    fn offset_span_overwrites_opaque_source_like_repeated_pixel_write() {
        assert_offset_span_matches_repeated_pixel_blend([44, 55, 66, 255], 2, 3);
    }

    #[test]
    fn offset_span_blends_translucent_source_like_repeated_pixel_blend() {
        assert_offset_span_matches_repeated_pixel_blend([100, 25, 0, 128], 2, 3);
    }

    fn assert_offset_span_matches_repeated_pixel_blend(color: [u8; 4], start_x: usize, len: usize) {
        let mut by_span = SoftwareSurface::default();
        by_span.resize(6, 2).expect("surface");
        by_span.clear([20, 40, 60, 255]);
        let mut by_pixel = SoftwareSurface::default();
        by_pixel.resize(6, 2).expect("surface");
        by_pixel.clear([20, 40, 60, 255]);
        let offset = by_span.row_offset(1) + start_x * 4;

        by_span.blend_constant_color_span_at_offset(offset, len, color);
        let mut pixel_offset = offset;
        for _ in 0..len {
            match color[3] {
                0 => {}
                u8::MAX => by_pixel.write_opaque_pixel_at_offset(pixel_offset, color),
                _ => by_pixel.blend_translucent_pixel_at_offset(pixel_offset, color),
            }
            pixel_offset += 4;
        }

        assert_eq!(by_span.pixels, by_pixel.pixels);
    }

    fn patterned_pixels(width: usize, seed: u8) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(width * 4);
        for index in 0..width {
            let offset = u8::try_from(index).expect("test width fits in u8");
            pixels.extend_from_slice(&[
                seed.wrapping_add(offset.wrapping_mul(3)),
                seed.wrapping_add(offset.wrapping_mul(5)),
                seed.wrapping_add(offset.wrapping_mul(7)),
                u8::MAX,
            ]);
        }
        pixels
    }

    fn blend_constant_premultiplied_span_rgba_reference(span: &mut [u8], source: [u8; 4]) {
        let inverse_alpha = u16::from(u8::MAX - source[3]);
        for pixel in span.chunks_exact_mut(4) {
            for channel in 0..3 {
                let product = u16::from(pixel[channel]) * inverse_alpha;
                let blend = (product + 127) / 255;
                pixel[channel] =
                    u8::try_from(u16::from(source[channel]) + blend).unwrap_or(u8::MAX);
            }
            pixel[3] = u8::MAX;
        }
    }

    #[test]
    fn transparent_source_matches_skipped_offset_write() {
        let mut by_offset = SoftwareSurface::default();
        by_offset.resize(3, 2).expect("surface");
        by_offset.clear([20, 40, 60, 255]);
        let mut by_blend = SoftwareSurface::default();
        by_blend.resize(3, 2).expect("surface");
        by_blend.clear([20, 40, 60, 255]);

        by_blend.blend_pixel(1, 1, [100, 25, 0, 0]);

        assert_eq!(by_offset.pixels, by_blend.pixels);
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
