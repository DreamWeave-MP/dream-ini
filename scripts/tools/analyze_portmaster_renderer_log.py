#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Summarize Dream INI PortMaster renderer logs."""

from __future__ import annotations

import argparse
import math
import re
import statistics
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path


SECTION_MARKERS = (
    ("software renderer render_stats", "render_stats"),
    ("software renderer primitive_stats", "primitive_stats"),
    ("software renderer raster_stats", "raster_stats"),
    ("software renderer raster_timings", "raster_timings"),
    ("software renderer timings", "renderer_timings"),
    ("framebuffer draw timings", "framebuffer_draw_timings"),
)

SELECTED_COUNTERS = {
    "render_stats": ("surface_bytes", "texture_sets", "texture_set_bytes", "texture_full_uploads", "texture_partial_updates", "clipped_primitives"),
    "primitive_stats": ("mesh_primitives", "mesh_indices", "mesh_triangles", "quad_windows", "axis_aligned_quad_windows", "solid_quad_fast_path_hits", "textured_quad_fast_path_hits", "textured_quad_reject_not_axis_aligned_rectangle", "textured_quad_reject_non_uniform_color", "solid_fan_rejected_probe_calls", "solid_fan_accepted_candidate_triangles_scanned", "solid_fan_rejected_candidate_triangles_scanned", "solid_fan_accepted_runs", "solid_fan_accepted_triangles", "generic_triangles_rasterized", "generic_solid_triangles", "generic_textured_triangles", "generic_textured_constant_texel_triangles", "generic_textured_sampled_triangles", "degenerate_triangles"),
    "raster_stats": ("solid_rect_calls", "solid_rect_px", "textured_rect_calls", "textured_rect_px", "textured_rect_sampled_us", "textured_rect_separable_run_calls", "textured_rect_separable_run_px", "textured_rect_separable_opaque_run_calls", "textured_rect_separable_opaque_run_px", "textured_rect_separable_translucent_run_calls", "textured_rect_separable_translucent_run_px", "textured_rect_separable_transparent_run_calls", "textured_rect_separable_transparent_run_px", "textured_rect_separable_white_vertex_calls", "textured_rect_separable_white_vertex_px", "textured_rect_separable_modulated_vertex_calls", "textured_rect_separable_modulated_vertex_px", "solid_triangle_calls", "solid_triangle_candidate_px", "solid_fan_calls", "solid_fan_px", "solid_fan_span_cache_hits", "solid_fan_span_cache_misses", "textured_triangle_calls", "textured_triangle_candidate_px", "sampled_textured_triangle_us", "opaque_px", "translucent_px", "transparent_px"),
    "raster_timings": ("quad_window_probe_us", "solid_quad_us", "textured_quad_us", "solid_fan_probe_us", "solid_fan_accepted_probe_us", "solid_fan_rejected_probe_us", "solid_fan_raster_us", "generic_solid_triangle_us", "generic_textured_triangle_us"),
    "renderer_timings": ("resize_clear_us", "egui_run_us", "texture_apply_us", "tessellate_us", "rasterize_us", "texture_free_us", "total_us"),
    "framebuffer_draw_timings": ("var_refresh_us", "validate_viewport_us", "render_us", "snapshot_us", "blit_us", "total_us"),
}

KEY_VALUE_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)=([^\s]+)")
NUMERIC_RE = re.compile(r"^-?\d+(?:\.\d+)?$")
INTEGER_LIST_RE = re.compile(r"^-?\d+(?:,-?\d+)+$")
DELAY_RE = re.compile(r"^(\d+)(ns|us|ms|s)$")


@dataclass
class SectionStats:
    name: str
    count: int = 0
    frames: list[int] = field(default_factory=list)
    timestamps: list[int] = field(default_factory=list)
    numeric_values: dict[str, list[float]] = field(default_factory=lambda: defaultdict(list))
    list_sums: dict[str, list[list[int]]] = field(default_factory=lambda: defaultdict(list))
    text_values: dict[str, Counter[str]] = field(default_factory=lambda: defaultdict(Counter))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Summarize Dream INI PortMaster renderer stats from a log file.")
    parser.add_argument("log", nargs="?", default="dream-ini-portmaster.log", help="Path to the PortMaster log, default: dream-ini-portmaster.log")
    parser.add_argument("--all-counters", action="store_true", help="Print distributions for every numeric counter, not just selected counters.")
    parser.add_argument("--zero-limit", type=int, default=80, help="Maximum all-zero counter names to print per section, default: 80.")
    return parser.parse_args()


def identify_section(line: str) -> tuple[str, int] | None:
    for marker, name in SECTION_MARKERS:
        index = line.find(marker)
        if index >= 0:
            return name, index + len(marker)
    return None


def parse_number(value: str) -> float | None:
    if NUMERIC_RE.match(value):
        return float(value) if "." in value else float(int(value))
    delay_match = DELAY_RE.match(value)
    if not delay_match:
        return None
    amount = int(delay_match.group(1))
    unit = delay_match.group(2)
    if unit == "ns":
        return amount / 1_000.0
    if unit == "us":
        return float(amount)
    if unit == "ms":
        return amount * 1_000.0
    return amount * 1_000_000.0


def parse_records(path: Path) -> dict[str, SectionStats]:
    sections = {name: SectionStats(name) for _, name in SECTION_MARKERS}
    current_frame: int | None = None
    with path.open("r", encoding="utf-8", errors="replace") as log_file:
        for line in log_file:
            line = line.rstrip("\n")
            timestamp = parse_timestamp(line)
            frame = parse_frame(line)
            if frame is not None:
                current_frame = frame
            section_match = identify_section(line)
            if section_match is None:
                continue
            section_name, start = section_match
            section = sections[section_name]
            section.count += 1
            if timestamp is not None:
                section.timestamps.append(timestamp)
            sample_frame = frame if frame is not None else current_frame
            if sample_frame is not None:
                section.frames.append(sample_frame)
            for key, raw_value in KEY_VALUE_RE.findall(line[start:]):
                if key == "frame":
                    continue
                if INTEGER_LIST_RE.match(raw_value):
                    section.list_sums[key].append([int(part) for part in raw_value.split(",")])
                    continue
                numeric_value = parse_number(raw_value)
                if numeric_value is None:
                    section.text_values[key][raw_value] += 1
                else:
                    section.numeric_values[key].append(numeric_value)
    return sections


def parse_timestamp(line: str) -> int | None:
    first = line.split(maxsplit=1)[0] if line else ""
    return int(first) if first.isdigit() else None


def parse_frame(line: str) -> int | None:
    match = re.search(r"(?:^|\s)frame=(\d+)(?:\s|$)", line)
    return int(match.group(1)) if match else None


def percentile(sorted_values: list[float], percent: float) -> float:
    index = max(0, math.ceil((percent / 100.0) * len(sorted_values)) - 1)
    return sorted_values[min(index, len(sorted_values) - 1)]


def format_number(value: float) -> str:
    return str(int(value)) if value == int(value) else f"{value:.2f}"


def distribution(values: list[float]) -> tuple[str, str, str, str, str, str, str]:
    ordered = sorted(values)
    total = sum(values)
    mean = statistics.fmean(values) if values else 0.0
    return (format_number(total), format_number(ordered[0]), format_number(percentile(ordered, 50)), format_number(percentile(ordered, 90)), format_number(percentile(ordered, 95)), format_number(ordered[-1]), format_number(mean))


def range_text(values: list[int]) -> str:
    if not values:
        return "n/a"
    return f"{min(values)}..{max(values)} ({len(set(values))} unique)"


def print_table(rows: list[tuple[str, ...]], headers: tuple[str, ...]) -> None:
    print("| " + " | ".join(headers) + " |")
    print("| " + " | ".join("---" for _ in headers) + " |")
    for row in rows:
        print("| " + " | ".join(row) + " |")


def selected_rows(section: SectionStats, include_all: bool) -> list[tuple[str, ...]]:
    keys = sorted(section.numeric_values) if include_all else [key for key in SELECTED_COUNTERS.get(section.name, ()) if key in section.numeric_values]
    return [(key, str(len(section.numeric_values[key])), *distribution(section.numeric_values[key])) for key in keys]


def bucket_rows(section: SectionStats) -> list[tuple[str, str, str]]:
    rows = []
    for key in sorted(section.list_sums):
        samples = section.list_sums[key]
        totals = [0] * max(len(sample) for sample in samples)
        for sample in samples:
            for index, value in enumerate(sample):
                totals[index] += value
        rows.append((key, str(len(samples)), ",".join(str(value) for value in totals)))
    return rows


def zero_counter_names(section: SectionStats) -> list[str]:
    return sorted(key for key, values in section.numeric_values.items() if values and all(value == 0 for value in values))


def text_rows(section: SectionStats) -> list[tuple[str, str]]:
    rows = []
    for key in sorted(section.text_values):
        counts = section.text_values[key]
        rows.append((key, ", ".join(f"{value}:{count}" for value, count in sorted(counts.items()))))
    return rows


def print_report(path: Path, sections: dict[str, SectionStats], include_all: bool, zero_limit: int) -> None:
    print("# PortMaster Renderer Log Summary")
    print()
    print(f"Log: `{path}`")
    print()
    print("## Overview")
    overview = [(name, str(sections[name].count), range_text(sections[name].frames), range_text(sections[name].timestamps)) for _, name in SECTION_MARKERS]
    print_table(overview, ("section", "samples", "frame range", "timestamp range"))
    print()
    for _, name in SECTION_MARKERS:
        section = sections[name]
        print(f"## {name}")
        if section.count == 0:
            print("No samples found.")
            print()
            continue
        print(f"Samples: {section.count}; frames: {range_text(section.frames)}")
        print()
        rows = selected_rows(section, include_all)
        if rows:
            label = "All Numeric Counter Distributions" if include_all else "Selected Counter Sums And Distributions"
            print(f"### {label}")
            print_table(rows, ("counter", "n", "sum", "min", "p50", "p90", "p95", "max", "mean"))
            print()
        buckets = bucket_rows(section)
        if buckets:
            print("### Bucket Totals")
            print_table(buckets, ("counter", "n", "bucket sums"))
            print()
        text = text_rows(section)
        if text:
            print("### Text Values")
            print_table(text, ("key", "counts"))
            print()
        zero_names = zero_counter_names(section)
        print("### Zero-Hit Counters")
        if zero_names:
            shown = zero_names[:zero_limit]
            suffix = "" if len(shown) == len(zero_names) else f" ... ({len(zero_names) - len(shown)} more)"
            print(", ".join(shown) + suffix)
        else:
            print("None")
        print()


def main() -> int:
    args = parse_args()
    path = Path(args.log)
    if not path.is_file():
        print(f"error: log file not found: {path}", file=sys.stderr)
        return 2
    sections = parse_records(path)
    if not any(section.count for section in sections.values()):
        print(f"error: no renderer stats sections found in {path}", file=sys.stderr)
        return 1
    print_report(path, sections, args.all_counters, args.zero_limit)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
