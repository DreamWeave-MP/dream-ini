// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use super::mesh_index_to_usize;
use crate::gui::portmaster::raster::{
    ClipBounds, SolidTriangleColorDecision, solid_triangle_color_decision, triangle_raster_bounds,
};
use crate::gui::portmaster::texture::TextureImage;

pub(super) struct SolidFanRun<'a> {
    pub(super) polygon: Vec<&'a egui::epaint::Vertex>,
    pub(super) triangle_count: usize,
    pub(super) color: [u8; 4],
}

#[derive(Clone, Copy)]
struct FanCandidate<'a> {
    center_slot: usize,
    center_index: u32,
    center_pos: egui::Pos2,
    center_vertex: &'a egui::epaint::Vertex,
    triangle_count: usize,
}

#[derive(Clone, Copy)]
struct FanTriangle<'a> {
    indices: [u32; 3],
    vertices: [&'a egui::epaint::Vertex; 3],
}

const SOLID_FAN_MIN_TRIANGLES: usize = 4;

pub(super) fn solid_fan_run<'a>(
    mesh: &'a egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
) -> io::Result<Option<SolidFanRun<'a>>> {
    for center_slot in 0..3 {
        if let Some(run) = solid_fan_run_for_center(mesh, texture, clip, index_offset, center_slot)?
        {
            return Ok(Some(run));
        }
    }
    Ok(None)
}

fn solid_fan_run_for_center<'a>(
    mesh: &'a egui::Mesh,
    texture: &TextureImage,
    clip: ClipBounds,
    index_offset: usize,
    center_slot: usize,
) -> io::Result<Option<SolidFanRun<'a>>> {
    let Some(candidate) = cheap_solid_fan_candidate(mesh, index_offset, center_slot)? else {
        return Ok(None);
    };
    let mut polygon = solid_fan_polygon(mesh, index_offset, candidate)?;
    polygon.push(candidate.center_vertex);
    if !solid_fan_polygon_is_safe(&polygon) {
        return Ok(None);
    }
    let Some(color) =
        solid_fan_run_color(mesh, texture, clip, index_offset, candidate.triangle_count)?
    else {
        return Ok(None);
    };
    Ok(Some(SolidFanRun {
        polygon,
        triangle_count: candidate.triangle_count,
        color,
    }))
}

fn cheap_solid_fan_candidate(
    mesh: &egui::Mesh,
    index_offset: usize,
    center_slot: usize,
) -> io::Result<Option<FanCandidate<'_>>> {
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(None);
    };
    if !fan_triangle_positions_are_finite(first) {
        return Ok(None);
    }
    let center_index = first.indices[center_slot];
    let center_vertex = first.vertices[center_slot];
    let center_pos = first.vertices[center_slot].pos;
    let [mut previous_boundary, mut current_boundary] = fan_boundaries(first, center_slot);
    let Some(expected_area_sign) = fan_triangle_area_sign(first) else {
        return Ok(None);
    };
    let mut triangle_count = 1;
    let mut offset = index_offset + 3;
    while offset + 2 < mesh.indices.len() {
        let Some(candidate) = fan_triangle_at(mesh, offset)? else {
            break;
        };
        if !fan_triangle_positions_are_finite(candidate) {
            break;
        }
        let Some(candidate_center_slot) =
            candidate_center_slot(candidate, center_index, center_pos)
        else {
            break;
        };
        let boundaries = fan_boundaries(candidate, candidate_center_slot);
        let Some(next_boundary) = next_fan_boundary(boundaries, current_boundary) else {
            break;
        };
        if fan_boundary_seen(
            mesh,
            index_offset,
            triangle_count,
            center_slot,
            center_index,
            center_pos,
            next_boundary,
        )? {
            break;
        }
        let Some(area_sign) = fan_triangle_area_sign(candidate) else {
            break;
        };
        if area_sign != expected_area_sign {
            break;
        }

        previous_boundary = current_boundary;
        current_boundary = next_boundary;
        triangle_count += 1;
        offset += 3;
    }

    if triangle_count < SOLID_FAN_MIN_TRIANGLES
        || same_fan_vertex(previous_boundary, current_boundary)
    {
        return Ok(None);
    }
    Ok(Some(FanCandidate {
        center_slot,
        center_index,
        center_pos,
        center_vertex,
        triangle_count,
    }))
}

fn fan_boundary_seen(
    mesh: &egui::Mesh,
    index_offset: usize,
    triangle_count: usize,
    center_slot: usize,
    center_index: u32,
    center_pos: egui::Pos2,
    boundary: FanVertex<'_>,
) -> io::Result<bool> {
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(false);
    };
    let [first_boundary, mut current_boundary] = fan_boundaries(first, center_slot);
    if same_fan_vertex(first_boundary, boundary) || same_fan_vertex(current_boundary, boundary) {
        return Ok(true);
    }

    let mut offset = index_offset + 3;
    for _ in 1..triangle_count {
        let Some(triangle) = fan_triangle_at(mesh, offset)? else {
            return Ok(false);
        };
        let Some(candidate_center_slot) = candidate_center_slot(triangle, center_index, center_pos)
        else {
            return Ok(false);
        };
        let Some(next_boundary) = next_fan_boundary(
            fan_boundaries(triangle, candidate_center_slot),
            current_boundary,
        ) else {
            return Ok(false);
        };
        if same_fan_vertex(next_boundary, boundary) {
            return Ok(true);
        }
        current_boundary = next_boundary;
        offset += 3;
    }
    Ok(false)
}

fn solid_fan_polygon<'a>(
    mesh: &'a egui::Mesh,
    index_offset: usize,
    candidate: FanCandidate<'a>,
) -> io::Result<Vec<&'a egui::epaint::Vertex>> {
    let mut polygon = Vec::with_capacity(candidate.triangle_count + 2);
    let Some(first) = fan_triangle_at(mesh, index_offset)? else {
        return Ok(polygon);
    };
    let [first_boundary, mut current_boundary] = fan_boundaries(first, candidate.center_slot);
    polygon.push(first_boundary.vertex);
    polygon.push(current_boundary.vertex);

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
        polygon.push(next_boundary.vertex);
        current_boundary = next_boundary;
        offset += 3;
    }
    Ok(polygon)
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
            vertex: triangle.vertices[first_slot],
        },
        FanVertex {
            index: triangle.indices[second_slot],
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

fn solid_fan_polygon_is_safe(polygon: &[&egui::epaint::Vertex]) -> bool {
    polygon.len() >= SOLID_FAN_MIN_TRIANGLES + 2 && polygon_is_strictly_convex(polygon)
}

fn polygon_is_strictly_convex(polygon: &[&egui::epaint::Vertex]) -> bool {
    let Some(expected_direction) = polygon_area_direction(polygon) else {
        return false;
    };
    for index in 0..polygon.len() {
        let a = polygon[index].pos;
        let b = polygon[(index + 1) % polygon.len()].pos;
        let c = polygon[(index + 2) % polygon.len()].pos;
        let turn = fan_edge(a, b, c);
        if non_zero_float_direction(turn) != Some(expected_direction) {
            return false;
        }
    }
    true
}

fn polygon_area_direction(polygon: &[&egui::epaint::Vertex]) -> Option<std::cmp::Ordering> {
    let mut twice_area = 0.0;
    for index in 0..polygon.len() {
        let a = polygon[index].pos;
        let b = polygon[(index + 1) % polygon.len()].pos;
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

#[derive(Clone, Copy)]
struct FanVertex<'a> {
    index: u32,
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
