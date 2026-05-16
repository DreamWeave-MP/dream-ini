// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use super::mesh_index_to_usize;
use super::stats::{SolidFanProbeRejectReason, SolidFanProbeStats};
use crate::gui::portmaster::raster::{
    ClipBounds, SolidTriangleColorDecision, solid_triangle_color_decision, triangle_raster_bounds,
};
use crate::gui::portmaster::texture::TextureImage;

pub(super) struct SolidFanRun {
    pub(super) polygon_len: usize,
    pub(super) triangle_count: usize,
    pub(super) color: [u8; 4],
}

pub(super) struct SolidFanPolygonScratch<'a> {
    pub(super) polygon: &'a mut Vec<usize>,
    pub(super) seen_boundaries: &'a mut Vec<FanBoundaryKey>,
    pub(super) budget: usize,
}

#[derive(Clone, Copy)]
struct FanCandidate {
    center_slot: usize,
    center_index: u32,
    center_pos: egui::Pos2,
    center_vertex_index: usize,
    triangle_count: usize,
}

impl FanCandidate {
    fn from_scan(seed: &CheapFanSeed<'_>, center_slot: usize, triangle_count: usize) -> Self {
        Self {
            center_slot,
            center_index: seed.center_index,
            center_pos: seed.center_pos,
            center_vertex_index: seed.center_vertex_index,
            triangle_count,
        }
    }
}

#[derive(Clone, Copy)]
struct FanTriangle<'a> {
    indices: [u32; 3],
    vertex_indices: [usize; 3],
    vertices: [&'a egui::epaint::Vertex; 3],
}

struct CheapFanSeed<'a> {
    center_index: u32,
    center_pos: egui::Pos2,
    center_vertex_index: usize,
    previous_boundary: FanVertex<'a>,
    current_boundary: FanVertex<'a>,
    expected_area_sign: i8,
}

pub(super) const SOLID_FAN_MIN_TRIANGLES: usize = 4;

pub(super) fn solid_fan_run<'a>(
    mesh: &'a egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
    mut polygon_scratch: SolidFanPolygonScratch<'a>,
    mut stats: Option<&mut SolidFanProbeStats>,
) -> io::Result<Option<SolidFanRun>> {
    if let Some(stats) = stats.as_deref_mut() {
        stats.probe_calls += 1;
    }
    let candidate_triangles_scanned_before = stats
        .as_deref()
        .map_or(0, |stats| stats.candidate_triangles_scanned);
    for center_slot in 0..3 {
        if let Some(stats) = stats.as_deref_mut() {
            stats.center_slot_attempts += 1;
        }
        if let Some(run) = solid_fan_run_for_center(
            mesh,
            texture,
            clip,
            index_offset,
            center_slot,
            &mut polygon_scratch,
            stats.as_deref_mut(),
        )? {
            if let Some(stats) = stats.as_deref_mut() {
                stats.record_accepted(run.triangle_count);
                stats.record_probe_candidate_triangles(true, candidate_triangles_scanned_before);
            }
            return Ok(Some(run));
        }
    }
    if let Some(stats) = stats {
        stats.rejected_probe_calls += 1;
        stats.record_probe_candidate_triangles(false, candidate_triangles_scanned_before);
    }
    Ok(None)
}

fn solid_fan_run_for_center(
    mesh: &egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
    center_slot: usize,
    polygon_scratch: &mut SolidFanPolygonScratch<'_>,
    mut stats: Option<&mut SolidFanProbeStats>,
) -> io::Result<Option<SolidFanRun>> {
    let Some(candidate) = cheap_solid_fan_candidate(
        mesh,
        index_offset,
        center_slot,
        polygon_scratch.seen_boundaries,
        polygon_scratch.budget,
        stats.as_deref_mut(),
    )?
    else {
        return Ok(None);
    };
    if candidate.triangle_count + 2 > polygon_scratch.budget {
        if let Some(stats) = stats.as_deref_mut() {
            stats.record_reject(SolidFanProbeRejectReason::ScratchOverflow);
        }
        return Ok(None);
    }
    if let Some(stats) = stats.as_deref_mut() {
        stats.polygon_builds += 1;
    }
    solid_fan_polygon(mesh, index_offset, candidate, polygon_scratch.polygon)?;
    polygon_scratch.polygon.push(candidate.center_vertex_index);
    if !solid_fan_polygon_is_safe(&mesh.vertices, polygon_scratch.polygon) {
        if let Some(stats) = stats.as_deref_mut() {
            stats.record_reject(SolidFanProbeRejectReason::UnsafePolygon);
        }
        return Ok(None);
    }
    let Some(color) =
        solid_fan_run_color(mesh, texture, clip, index_offset, candidate.triangle_count)?
    else {
        if let Some(stats) = stats {
            stats.record_reject(SolidFanProbeRejectReason::ColorOrBoundsMismatch);
        }
        return Ok(None);
    };
    Ok(Some(SolidFanRun {
        polygon_len: polygon_scratch.polygon.len(),
        triangle_count: candidate.triangle_count,
        color,
    }))
}

fn cheap_solid_fan_candidate(
    mesh: &egui::Mesh,
    index_offset: usize,
    center_slot: usize,
    seen_boundaries: &mut Vec<FanBoundaryKey>,
    seen_boundary_budget: usize,
    mut stats: Option<&mut SolidFanProbeStats>,
) -> io::Result<Option<FanCandidate>> {
    if let Some(stats) = stats.as_deref_mut() {
        stats.cheap_candidate_attempts += 1;
    }
    let Some(seed) = cheap_solid_fan_seed(mesh, index_offset, center_slot, stats.as_deref_mut())?
    else {
        return Ok(None);
    };
    let center_index = seed.center_index;
    let center_pos = seed.center_pos;
    let expected_area_sign = seed.expected_area_sign;
    let mut previous_boundary = seed.previous_boundary;
    let mut current_boundary = seed.current_boundary;
    if !initialize_seen_boundaries(
        seen_boundaries,
        previous_boundary,
        current_boundary,
        seen_boundary_budget,
        stats.as_deref_mut(),
    ) {
        return Ok(None);
    }
    let mut triangle_count = 1;
    let mut offset = index_offset + 3;
    let mut reject_reason = None;
    while offset + 2 < mesh.indices.len() {
        let Some(candidate) = fan_triangle_at(mesh, offset)? else {
            reject_reason = Some(SolidFanProbeRejectReason::VertexLookupFailure);
            break;
        };
        if let Some(stats) = stats.as_deref_mut() {
            stats.candidate_triangles_scanned += 1;
        }
        if !fan_triangle_positions_are_finite(candidate) {
            reject_reason = Some(SolidFanProbeRejectReason::WindingMismatchOrNonFinite);
            break;
        }
        let Some(candidate_center_slot) =
            candidate_center_slot(candidate, center_index, center_pos)
        else {
            reject_reason = Some(SolidFanProbeRejectReason::NoCandidate);
            break;
        };
        let boundaries = fan_boundaries(candidate, candidate_center_slot);
        let Some(next_boundary) = next_fan_boundary(boundaries, current_boundary) else {
            reject_reason = Some(SolidFanProbeRejectReason::NoCandidate);
            break;
        };
        if fan_boundary_seen(seen_boundaries, next_boundary, stats.as_deref_mut()) {
            reject_reason = Some(SolidFanProbeRejectReason::RepeatedBoundary);
            break;
        }
        let Some(area_sign) = fan_triangle_area_sign(candidate) else {
            reject_reason = Some(SolidFanProbeRejectReason::WindingMismatchOrNonFinite);
            break;
        };
        if area_sign != expected_area_sign {
            reject_reason = Some(SolidFanProbeRejectReason::WindingMismatchOrNonFinite);
            break;
        }
        if triangle_count + 3 > seen_boundary_budget {
            reject_reason = Some(SolidFanProbeRejectReason::ScratchOverflow);
            break;
        }

        previous_boundary = current_boundary;
        current_boundary = next_boundary;
        seen_boundaries.push(FanBoundaryKey::from(next_boundary));
        triangle_count += 1;
        offset += 3;
    }

    if matches!(
        reject_reason,
        Some(SolidFanProbeRejectReason::ScratchOverflow)
    ) {
        record_scratch_overflow(stats.as_deref_mut(), triangle_count);
        return Ok(None);
    }

    if triangle_count < SOLID_FAN_MIN_TRIANGLES
        || same_fan_vertex(previous_boundary, current_boundary)
    {
        if let Some(stats) = stats.as_deref_mut() {
            let reason = reject_reason.unwrap_or(if triangle_count < SOLID_FAN_MIN_TRIANGLES {
                SolidFanProbeRejectReason::TooShort
            } else {
                SolidFanProbeRejectReason::RepeatedBoundary
            });
            stats.record_reject(reason);
            stats.record_candidate_triangles(triangle_count);
        }
        return Ok(None);
    }
    if let Some(stats) = stats {
        stats.record_candidate_triangles(triangle_count);
    }
    Ok(Some(FanCandidate::from_scan(
        &seed,
        center_slot,
        triangle_count,
    )))
}

fn initialize_seen_boundaries(
    seen_boundaries: &mut Vec<FanBoundaryKey>,
    previous_boundary: FanVertex<'_>,
    current_boundary: FanVertex<'_>,
    seen_boundary_budget: usize,
    stats: Option<&mut SolidFanProbeStats>,
) -> bool {
    seen_boundaries.clear();
    if seen_boundary_budget < 2 {
        record_scratch_overflow(stats, 1);
        return false;
    }
    seen_boundaries.push(FanBoundaryKey::from(previous_boundary));
    seen_boundaries.push(FanBoundaryKey::from(current_boundary));
    true
}

fn record_scratch_overflow(stats: Option<&mut SolidFanProbeStats>, triangle_count: usize) {
    if let Some(stats) = stats {
        stats.record_reject(SolidFanProbeRejectReason::ScratchOverflow);
        stats.record_candidate_triangles(triangle_count);
    }
}

fn cheap_solid_fan_seed<'a>(
    mesh: &'a egui::Mesh,
    index_offset: usize,
    center_slot: usize,
    mut stats: Option<&mut SolidFanProbeStats>,
) -> io::Result<Option<CheapFanSeed<'a>>> {
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        if let Some(stats) = stats.as_deref_mut() {
            stats.record_reject(SolidFanProbeRejectReason::VertexLookupFailure);
        }
        return Ok(None);
    };
    if let Some(stats) = stats.as_deref_mut() {
        stats.candidate_triangles_scanned += 1;
    }
    if !fan_triangle_positions_are_finite(first) {
        if let Some(stats) = stats.as_deref_mut() {
            stats.record_reject(SolidFanProbeRejectReason::WindingMismatchOrNonFinite);
        }
        return Ok(None);
    }
    let Some(expected_area_sign) = fan_triangle_area_sign(first) else {
        if let Some(stats) = stats {
            stats.record_reject(SolidFanProbeRejectReason::WindingMismatchOrNonFinite);
        }
        return Ok(None);
    };
    let [previous_boundary, current_boundary] = fan_boundaries(first, center_slot);
    Ok(Some(CheapFanSeed {
        center_index: first.indices[center_slot],
        center_pos: first.vertices[center_slot].pos,
        center_vertex_index: first.vertex_indices[center_slot],
        previous_boundary,
        current_boundary,
        expected_area_sign,
    }))
}

fn fan_boundary_seen(
    seen_boundaries: &[FanBoundaryKey],
    boundary: FanVertex<'_>,
    mut stats: Option<&mut SolidFanProbeStats>,
) -> bool {
    if let Some(stats) = stats.as_deref_mut() {
        stats.repeated_boundary_checks += 1;
    }
    let boundary = FanBoundaryKey::from(boundary);
    let mut comparisons = 0;
    for seen_boundary in seen_boundaries {
        comparisons += 1;
        if *seen_boundary == boundary {
            if let Some(stats) = stats {
                stats.repeated_boundary_comparisons += comparisons;
            }
            return true;
        }
    }
    if let Some(stats) = stats {
        stats.repeated_boundary_comparisons += comparisons;
    }
    false
}

fn solid_fan_polygon(
    mesh: &egui::Mesh,
    index_offset: usize,
    candidate: FanCandidate,
    polygon: &mut Vec<usize>,
) -> io::Result<()> {
    polygon.clear();
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(());
    };
    let [first_boundary, mut current_boundary] = fan_boundaries(first, candidate.center_slot);
    polygon.push(first_boundary.vertex_index);
    polygon.push(current_boundary.vertex_index);

    let mut offset = index_offset + 3;
    for _ in 1..candidate.triangle_count {
        let Some(triangle) = fan_triangle_at(mesh, offset)? else {
            break;
        };
        let Some(center_slot) =
            candidate_center_slot(triangle, candidate.center_index, candidate.center_pos)
        else {
            break;
        };
        let Some(next_boundary) =
            next_fan_boundary(fan_boundaries(triangle, center_slot), current_boundary)
        else {
            break;
        };
        polygon.push(next_boundary.vertex_index);
        current_boundary = next_boundary;
        offset += 3;
    }
    Ok(())
}

fn solid_fan_run_color(
    mesh: &egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
    triangle_count: usize,
) -> io::Result<Option<[u8; 4]>> {
    let mut color = None;
    let mut offset = index_offset;
    for _ in 0..triangle_count {
        let Some(triangle) = fan_triangle_at(mesh, offset)? else {
            return Ok(None);
        };
        let Some(triangle_color) = solid_fan_triangle_color(triangle, texture, clip) else {
            return Ok(None);
        };
        if let Some(color) = color {
            if triangle_color != color {
                return Ok(None);
            }
        } else {
            color = Some(triangle_color);
        }
        offset += 3;
    }
    Ok(color)
}

fn fan_triangle_at(mesh: &egui::Mesh, index_offset: usize) -> io::Result<Option<FanTriangle<'_>>> {
    let indices = [
        mesh.indices[index_offset],
        mesh.indices[index_offset + 1],
        mesh.indices[index_offset + 2],
    ];
    let i0 = mesh_index_to_usize(indices[0])?;
    let i1 = mesh_index_to_usize(indices[1])?;
    let i2 = mesh_index_to_usize(indices[2])?;
    let (Some(v0), Some(v1), Some(v2)) = (
        mesh.vertices.get(i0),
        mesh.vertices.get(i1),
        mesh.vertices.get(i2),
    ) else {
        return Ok(None);
    };
    Ok(Some(FanTriangle {
        indices,
        vertex_indices: [i0, i1, i2],
        vertices: [v0, v1, v2],
    }))
}

fn solid_fan_triangle_color(
    triangle: FanTriangle<'_>,
    texture: &TextureImage,
    clip: ClipBounds,
) -> Option<[u8; 4]> {
    let [v0, v1, v2] = triangle.vertices;
    let SolidTriangleColorDecision::Solid(color) =
        solid_triangle_color_decision(v0, v1, v2, texture)
    else {
        return None;
    };
    triangle_raster_bounds(v0, v1, v2, clip)?;
    Some(color)
}

fn candidate_center_slot(
    triangle: FanTriangle<'_>,
    center_index: u32,
    center_pos: egui::Pos2,
) -> Option<usize> {
    (0..3).find(|slot| {
        triangle.indices[*slot] == center_index
            && same_fan_pos(triangle.vertices[*slot].pos, center_pos)
    })
}

fn fan_boundaries(triangle: FanTriangle<'_>, center_slot: usize) -> [FanVertex<'_>; 2] {
    let [first_slot, second_slot] = match center_slot {
        0 => [1, 2],
        1 => [0, 2],
        2 => [0, 1],
        _ => unreachable!("center slot is probed in the triangle slot range"),
    };
    [
        FanVertex {
            index: triangle.indices[first_slot],
            vertex_index: triangle.vertex_indices[first_slot],
            vertex: triangle.vertices[first_slot],
        },
        FanVertex {
            index: triangle.indices[second_slot],
            vertex_index: triangle.vertex_indices[second_slot],
            vertex: triangle.vertices[second_slot],
        },
    ]
}

fn next_fan_boundary<'a>(
    boundaries: [FanVertex<'a>; 2],
    current_boundary: FanVertex<'_>,
) -> Option<FanVertex<'a>> {
    if same_fan_vertex(boundaries[0], current_boundary) {
        return Some(boundaries[1]);
    }
    if same_fan_vertex(boundaries[1], current_boundary) {
        return Some(boundaries[0]);
    }
    None
}

fn fan_triangle_positions_are_finite(triangle: FanTriangle<'_>) -> bool {
    triangle
        .vertices
        .iter()
        .all(|vertex| vertex.pos.x.is_finite() && vertex.pos.y.is_finite())
}

fn fan_triangle_area_sign(triangle: FanTriangle<'_>) -> Option<i8> {
    let [v0, v1, v2] = triangle.vertices;
    let area = (v2.pos.x - v0.pos.x).mul_add(
        v1.pos.y - v0.pos.y,
        -((v2.pos.y - v0.pos.y) * (v1.pos.x - v0.pos.x)),
    );
    if area.abs() <= f32::EPSILON {
        return None;
    }
    Some(if area.is_sign_positive() { 1 } else { -1 })
}

fn solid_fan_polygon_is_safe(vertices: &[egui::epaint::Vertex], polygon: &[usize]) -> bool {
    polygon.len() >= SOLID_FAN_MIN_TRIANGLES + 2 && polygon_is_strictly_convex(vertices, polygon)
}

fn polygon_is_strictly_convex(vertices: &[egui::epaint::Vertex], polygon: &[usize]) -> bool {
    let Some(expected_direction) = polygon_area_direction(vertices, polygon) else {
        return false;
    };
    for index in 0..polygon.len() {
        let a = vertices[polygon[index]].pos;
        let b = vertices[polygon[(index + 1) % polygon.len()]].pos;
        let c = vertices[polygon[(index + 2) % polygon.len()]].pos;
        let turn = fan_edge(a, b, c);
        if non_zero_float_direction(turn) != Some(expected_direction) {
            return false;
        }
    }
    true
}

fn polygon_area_direction(
    vertices: &[egui::epaint::Vertex],
    polygon: &[usize],
) -> Option<std::cmp::Ordering> {
    let mut twice_area = 0.0;
    for index in 0..polygon.len() {
        let a = vertices[polygon[index]].pos;
        let b = vertices[polygon[(index + 1) % polygon.len()]].pos;
        twice_area += a.x.mul_add(b.y, -(a.y * b.x));
    }
    non_zero_float_direction(-twice_area)
}

fn non_zero_float_direction(value: f32) -> Option<std::cmp::Ordering> {
    if value > f32::EPSILON {
        Some(std::cmp::Ordering::Greater)
    } else if value < -f32::EPSILON {
        Some(std::cmp::Ordering::Less)
    } else {
        None
    }
}

fn fan_edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FanBoundaryKey {
    index: u32,
    pos: egui::Pos2,
}

impl From<FanVertex<'_>> for FanBoundaryKey {
    fn from(vertex: FanVertex<'_>) -> Self {
        Self {
            index: vertex.index,
            pos: vertex.vertex.pos,
        }
    }
}

impl PartialEq for FanBoundaryKey {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && same_fan_pos(self.pos, other.pos)
    }
}

impl Eq for FanBoundaryKey {}

#[derive(Clone, Copy)]
struct FanVertex<'a> {
    index: u32,
    vertex_index: usize,
    vertex: &'a egui::epaint::Vertex,
}

fn same_fan_vertex(left: FanVertex<'_>, right: FanVertex<'_>) -> bool {
    left.index == right.index && same_fan_pos(left.vertex.pos, right.vertex.pos)
}

fn same_fan_pos(left: egui::Pos2, right: egui::Pos2) -> bool {
    matches!(
        left.x.partial_cmp(&right.x),
        Some(std::cmp::Ordering::Equal)
    ) && matches!(
        left.y.partial_cmp(&right.y),
        Some(std::cmp::Ordering::Equal)
    )
}
