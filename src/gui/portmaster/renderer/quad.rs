// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use super::stats::{
    TriangleSource, record_quad_probe_elapsed, record_solid_quad_elapsed,
    record_solid_quad_reject_elapsed, record_textured_quad_elapsed,
    record_textured_quad_reject_elapsed,
};
use super::{RasterInstrumentation, mesh_index_to_usize};
use crate::gui::portmaster::raster::{
    ClipBounds, rasterize_axis_aligned_solid_quad, rasterize_axis_aligned_textured_quad,
    textured_quad_fast_path_rejection,
};
use crate::gui::portmaster::surface::SoftwareSurface;
use crate::gui::portmaster::texture::TextureImage;

pub(super) fn try_rasterize_quad_window(
    surface: &mut SoftwareSurface,
    mesh: &egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    quad: &[u32],
    source: TriangleSource,
    instrumentation: &mut RasterInstrumentation<'_>,
) -> io::Result<bool> {
    let probe_start = instrumentation.timing_start();
    if let Some(stats) = instrumentation.primitive_stats() {
        stats.quad_windows += 1;
    }
    if has_four_unique_indices(quad) {
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.four_unique_quad_windows += 1;
        }
    } else {
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.quad_windows_not_four_unique_indices += 1;
        }
        record_quad_probe_elapsed(instrumentation, probe_start);
        return Ok(false);
    }

    let i0 = mesh_index_to_usize(quad[0])?;
    let i1 = mesh_index_to_usize(quad[1])?;
    let i2 = mesh_index_to_usize(quad[2])?;
    let i3 = mesh_index_to_usize(quad[3])?;
    let i4 = mesh_index_to_usize(quad[4])?;
    let i5 = mesh_index_to_usize(quad[5])?;
    let (Some(v0), Some(v1), Some(v2), Some(v3), Some(v4), Some(v5)) = (
        mesh.vertices.get(i0),
        mesh.vertices.get(i1),
        mesh.vertices.get(i2),
        mesh.vertices.get(i3),
        mesh.vertices.get(i4),
        mesh.vertices.get(i5),
    ) else {
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.quad_window_vertex_lookup_failures += 1;
        }
        record_quad_probe_elapsed(instrumentation, probe_start);
        return Ok(false);
    };

    let vertices = [v0, v1, v2, v3, v4, v5];
    if let Some(stats) = instrumentation.primitive_stats() {
        stats.record_quad_window(vertices, texture);
    }
    record_quad_probe_elapsed(instrumentation, probe_start);

    let solid_start = instrumentation.timing_start();
    if rasterize_axis_aligned_solid_quad(
        surface,
        vertices,
        texture,
        clip,
        instrumentation.raster_stats(),
    ) {
        record_solid_quad_elapsed(instrumentation, solid_start);
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.solid_quad_fast_path_hits += 1;
        }
        return Ok(true);
    }
    record_solid_quad_reject_elapsed(instrumentation, solid_start);

    let textured_start = instrumentation.timing_start();
    if rasterize_axis_aligned_textured_quad(
        surface,
        vertices,
        texture,
        clip,
        instrumentation.raster_stats(),
    ) {
        record_textured_quad_elapsed(instrumentation, textured_start);
        if let Some(stats) = instrumentation.primitive_stats() {
            stats.textured_quad_fast_path_hits += 1;
        }
        return Ok(true);
    }
    record_textured_quad_reject_elapsed(instrumentation, textured_start);
    if let Some(stats) = instrumentation.primitive_stats()
        && let Some(rejection) = textured_quad_fast_path_rejection(vertices)
    {
        stats.record_textured_quad_rejection(rejection, vertices, texture, clip, source);
    }
    Ok(false)
}

pub(super) fn has_four_unique_indices(indices: &[u32]) -> bool {
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
