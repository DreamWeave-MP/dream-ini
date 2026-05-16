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
    "primitive_stats": ("mesh_primitives", "mesh_indices", "mesh_triangles", "quad_windows", "axis_aligned_quad_windows", "solid_quad_fast_path_hits", "textured_quad_fast_path_hits", "textured_quad_reject_not_axis_aligned_rectangle", "textured_quad_reject_non_uniform_color", "solid_fan_preflight_reject_too_few_triangles", "solid_fan_preflight_second_triangle_checks", "solid_fan_preflight_reject_no_second_triangle_continuation", "solid_fan_preflight_center_slots_allowed", "solid_fan_preflight_center_slots_rejected", "solid_fan_rejected_probe_calls", "solid_fan_accepted_candidate_triangles_scanned", "solid_fan_rejected_candidate_triangles_scanned", "solid_fan_accepted_runs", "solid_fan_accepted_triangles", "generic_triangles_rasterized", "generic_solid_triangles", "generic_textured_triangles", "generic_textured_constant_texel_triangles", "generic_textured_sampled_triangles", "degenerate_triangles"),
    "raster_stats": ("solid_rect_calls", "solid_rect_px", "textured_rect_calls", "textured_rect_px", "textured_rect_sampled_us", "textured_rect_separable_run_calls", "textured_rect_separable_run_px", "textured_rect_separable_opaque_run_calls", "textured_rect_separable_opaque_run_px", "textured_rect_separable_translucent_run_calls", "textured_rect_separable_translucent_run_px", "textured_rect_separable_transparent_run_calls", "textured_rect_separable_transparent_run_px", "textured_rect_separable_white_vertex_calls", "textured_rect_separable_white_vertex_px", "textured_rect_separable_modulated_vertex_calls", "textured_rect_separable_modulated_vertex_px", "textured_rect_sampled_vector_candidate_px", "textured_rect_sampled_white_vertex_contiguous_px", "textured_rect_sampled_modulated_vertex_contiguous_px", "textured_rect_sampled_modulated_contiguous_runs", "textured_rect_sampled_modulated_contiguous_px_lt4", "textured_rect_sampled_modulated_contiguous_px_4_7", "textured_rect_sampled_modulated_contiguous_px_8_15", "textured_rect_sampled_modulated_contiguous_px_16_31", "textured_rect_sampled_modulated_contiguous_px_32_63", "textured_rect_sampled_modulated_contiguous_px_64_plus", "textured_rect_sampled_modulated_opportunity_blocks_16", "textured_rect_sampled_modulated_opportunity_px_16", "textured_rect_sampled_modulated_true_tail_px_16", "textured_rect_sampled_modulated_opportunity_blocks_4", "textured_rect_sampled_modulated_opportunity_px_4", "textured_rect_sampled_modulated_true_tail_px_4", "textured_rect_sampled_vector_backend_available", "textured_rect_sampled_modulated_vector_attempt_blocks_16", "textured_rect_sampled_modulated_vector_success_blocks_16", "textured_rect_sampled_modulated_vector_fallback_blocks_16", "textured_rect_sampled_modulated_vector_attempt_blocks_4", "textured_rect_sampled_modulated_vector_success_blocks_4", "textured_rect_sampled_modulated_vector_fallback_blocks_4", "textured_rect_sampled_vector_blocks", "textured_rect_sampled_vector_opaque_blocks", "textured_rect_sampled_vector_transparent_blocks", "textured_rect_sampled_vector_mixed_blocks", "textured_rect_sampled_vector_tail_px", "solid_triangle_calls", "solid_triangle_candidate_px", "solid_fan_calls", "solid_fan_px", "solid_fan_span_cache_hits", "solid_fan_span_cache_misses", "textured_triangle_calls", "textured_triangle_candidate_px", "sampled_textured_triangle_us", "opaque_px", "translucent_px", "transparent_px"),
    "raster_timings": ("quad_window_probe_us", "solid_quad_us", "textured_quad_us", "solid_fan_probe_us", "solid_fan_accepted_probe_us", "solid_fan_rejected_probe_us", "solid_fan_raster_us", "generic_solid_triangle_us", "generic_textured_triangle_us"),
    "renderer_timings": ("resize_clear_us", "egui_run_us", "texture_apply_us", "tessellate_us", "rasterize_us", "texture_free_us", "total_us"),
    "framebuffer_draw_timings": ("var_refresh_us", "validate_viewport_us", "render_us", "snapshot_us", "blit_us", "total_us"),
}

CONSTANT_TEXEL_RASTER_COUNTERS = (
    "constant_texel_textured_triangle_opaque_px",
    "constant_texel_textured_triangle_translucent_px",
    "constant_texel_textured_triangle_transparent_px",
    "constant_texel_textured_triangle_span_runs",
    "constant_texel_textured_triangle_span_px_lt4",
    "constant_texel_textured_triangle_span_px_4_7",
    "constant_texel_textured_triangle_span_px_8_15",
    "constant_texel_textured_triangle_span_px_16_31",
    "constant_texel_textured_triangle_span_px_32_plus",
    "constant_texel_textured_triangle_repeated_color_opportunity_blocks_16",
    "constant_texel_textured_triangle_repeated_color_opportunity_px_16",
    "constant_texel_textured_triangle_repeated_color_true_tail_px_16",
    "constant_texel_textured_triangle_repeated_color_span_helper_calls",
    "constant_texel_textured_triangle_repeated_color_span_helper_px",
    "constant_texel_textured_triangle_repeated_color_opaque_write_helper_calls",
    "constant_texel_textured_triangle_repeated_color_opaque_write_helper_px",
    "constant_texel_textured_triangle_repeated_color_translucent_blend_helper_calls",
    "constant_texel_textured_triangle_repeated_color_translucent_blend_helper_px",
)

SELECTED_COUNTERS["raster_stats"] += tuple(
    key for key in CONSTANT_TEXEL_RASTER_COUNTERS if key not in SELECTED_COUNTERS["raster_stats"]
)

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
    numeric_records: list[dict[str, float]] = field(default_factory=list)
    numeric_values: dict[str, list[float]] = field(default_factory=lambda: defaultdict(list))
    list_sums: dict[str, list[list[int]]] = field(default_factory=lambda: defaultdict(list))
    text_values: dict[str, Counter[str]] = field(default_factory=lambda: defaultdict(Counter))


@dataclass
class BenchmarkSummary:
    name: str | None = None
    configured_frames: int | None = None
    completed_frames: int | None = None
    completion_reason: str | None = None
    workload: str | None = None
    config: dict[str, str] = field(default_factory=dict)
    disabled_reasons: Counter[str] = field(default_factory=Counter)


@dataclass(frozen=True)
class NormalizedMetric:
    name: str
    denominator: str
    unit: str
    denominator_scale: float = 1.0


NORMALIZED_SAMPLED_RECT_METRICS = (
    NormalizedMetric("textured_rect_sampled_us_per_sampled_rect_px", "textured_rect_sampled_px", "us/px"),
    NormalizedMetric("textured_rect_sampled_us_per_modulated_contiguous_px", "textured_rect_sampled_modulated_vertex_contiguous_px", "us/px"),
    NormalizedMetric("textured_rect_sampled_us_per_vectorized_px_4", "textured_rect_sampled_modulated_vector_success_blocks_4", "us/px", 4.0),
    NormalizedMetric("textured_rect_sampled_us_per_vector_success_block_4", "textured_rect_sampled_modulated_vector_success_blocks_4", "us/block"),
)


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


def parse_key_values(text: str) -> dict[str, str]:
    values = {}
    index = 0
    while index < len(text):
        match = re.search(r"([A-Za-z_][A-Za-z0-9_]*)=", text[index:])
        if match is None:
            break
        key = match.group(1)
        value_start = index + match.end()
        value_end = value_start
        quote: str | None = None
        bracket_depth = 0
        while value_end < len(text):
            char = text[value_end]
            if quote is not None:
                if char == quote and text[value_end - 1] != "\\":
                    quote = None
            elif char in {'"', "'"}:
                quote = char
            elif char in "[({":
                bracket_depth += 1
            elif char in "])}" and bracket_depth > 0:
                bracket_depth -= 1
            elif char.isspace() and bracket_depth == 0:
                break
            value_end += 1
        values[key] = clean_value(text[value_start:value_end])
        index = value_end + 1
    return values


def clean_value(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        return value[1:-1].replace('\\"', '"')
    return value


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


def parse_records(path: Path) -> tuple[dict[str, SectionStats], BenchmarkSummary]:
    sections = {name: SectionStats(name) for _, name in SECTION_MARKERS}
    benchmark = BenchmarkSummary()
    current_frame: int | None = None
    with path.open("r", encoding="utf-8", errors="replace") as log_file:
        for line in log_file:
            line = line.rstrip("\n")
            timestamp = parse_timestamp(line)
            frame = parse_frame(line)
            if frame is not None:
                current_frame = frame
            parse_benchmark_line(line, benchmark)
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
            numeric_record = {}
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
                    numeric_record[key] = numeric_value
            section.numeric_records.append(numeric_record)
    return sections, benchmark


def parse_benchmark_line(line: str, benchmark: BenchmarkSummary) -> None:
    marker = "portmaster render benchmark "
    index = line.find(marker)
    if index < 0:
        return
    details = line[index + len(marker) :]
    action, _, payload = details.partition(" ")
    values = parse_key_values(payload)
    if action == "enabled":
        record_benchmark_values(benchmark, values)
        return
    if action == "config":
        record_benchmark_values(benchmark, values)
        for key, value in values.items():
            if key not in {"name", "frames", "workload"}:
                benchmark.config[key] = value
        return
    if action == "complete":
        record_benchmark_values(benchmark, values)
        completed = values.get("rendered_frames") or values.get("completed_frames") or values.get("frames")
        if completed is not None:
            benchmark.completed_frames = int(completed)
        configured = values.get("frame_limit") or values.get("configured_frames")
        if configured is not None:
            benchmark.configured_frames = int(configured)
        reason = values.get("reason")
        if reason is not None:
            benchmark.completion_reason = reason
        return
    if action == "disabled":
        reason = values.get("reason")
        if reason is not None:
            benchmark.disabled_reasons[reason] += 1


def record_benchmark_values(benchmark: BenchmarkSummary, values: dict[str, str]) -> None:
    if "name" in values:
        benchmark.name = values["name"]
    if "frames" in values:
        benchmark.configured_frames = int(values["frames"])
    if "workload" in values:
        benchmark.workload = values["workload"]


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


def distribution_without_sum(values: list[float]) -> tuple[str, str, str, str, str, str]:
    ordered = sorted(values)
    mean = statistics.fmean(values) if values else 0.0
    return (format_number(ordered[0]), format_number(percentile(ordered, 50)), format_number(percentile(ordered, 90)), format_number(percentile(ordered, 95)), format_number(ordered[-1]), format_number(mean))


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


def normalized_sampled_rect_rows(section: SectionStats) -> tuple[list[tuple[str, ...]], list[str]]:
    rows = []
    unavailable = []
    for metric in NORMALIZED_SAMPLED_RECT_METRICS:
        ratios = []
        skipped_zero = 0
        skipped_missing = 0
        for record in section.numeric_records:
            numerator = record.get("textured_rect_sampled_us")
            denominator = record.get(metric.denominator)
            if numerator is None or denominator is None:
                skipped_missing += 1
                continue
            denominator *= metric.denominator_scale
            if denominator == 0:
                skipped_zero += 1
                continue
            ratios.append(numerator / denominator)
        if ratios:
            skipped = skipped_missing + skipped_zero
            rows.append((metric.name, metric.unit, str(len(ratios)), str(skipped), *distribution_without_sum(ratios)))
        else:
            reason = "missing counter" if skipped_missing else "zero denominator"
            unavailable.append(f"{metric.name} ({reason})")
    return rows, unavailable


def counter_total(section: SectionStats, key: str) -> float | None:
    values = section.numeric_values.get(key)
    return sum(values) if values is not None else None


def format_percent(numerator: float, denominator: float) -> str:
    return f"{(numerator / denominator) * 100.0:.2f}%"


def append_share_row(rows: list[tuple[str, str, str]], label: str, numerator: float | None, denominator: float | None) -> None:
    if numerator is None or denominator is None or denominator <= 0:
        return
    rows.append((label, format_number(numerator), format_percent(numerator, denominator)))


def constant_texel_summary_rows(section: SectionStats) -> list[tuple[str, str, str]]:
    rows = []
    span_buckets = (
        ("span width px <4", "constant_texel_textured_triangle_span_px_lt4"),
        ("span width px 4-7", "constant_texel_textured_triangle_span_px_4_7"),
        ("span width px 8-15", "constant_texel_textured_triangle_span_px_8_15"),
        ("span width px 16-31", "constant_texel_textured_triangle_span_px_16_31"),
        ("span width px 32+", "constant_texel_textured_triangle_span_px_32_plus"),
    )
    span_totals = [(label, counter_total(section, key)) for label, key in span_buckets]
    if all(total is not None for _, total in span_totals):
        span_px = sum(total for _, total in span_totals if total is not None)
        for label, total in span_totals:
            append_share_row(rows, label, total, span_px)

    opportunity_px = counter_total(
        section, "constant_texel_textured_triangle_repeated_color_opportunity_px_16"
    )
    true_tail_px = counter_total(
        section, "constant_texel_textured_triangle_repeated_color_true_tail_px_16"
    )
    repeated_color_px = None
    if opportunity_px is not None and true_tail_px is not None:
        repeated_color_px = opportunity_px + true_tail_px
        append_share_row(rows, "repeated-color 16-wide opportunity px", opportunity_px, repeated_color_px)
        append_share_row(rows, "repeated-color true-tail px", true_tail_px, repeated_color_px)

    helper_px = counter_total(
        section, "constant_texel_textured_triangle_repeated_color_span_helper_px"
    )
    append_share_row(rows, "helper-use px share of repeated-color span px", helper_px, repeated_color_px)

    opaque_px = counter_total(section, "constant_texel_textured_triangle_opaque_px")
    translucent_px = counter_total(section, "constant_texel_textured_triangle_translucent_px")
    transparent_px = counter_total(section, "constant_texel_textured_triangle_transparent_px")
    if opaque_px is not None and translucent_px is not None and transparent_px is not None:
        alpha_px = opaque_px + translucent_px + transparent_px
        append_share_row(rows, "alpha mix opaque px", opaque_px, alpha_px)
        append_share_row(rows, "alpha mix translucent px", translucent_px, alpha_px)
        append_share_row(rows, "alpha mix transparent px", transparent_px, alpha_px)

    return rows


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


def benchmark_rows(benchmark: BenchmarkSummary) -> list[tuple[str, str]]:
    if not benchmark_has_data(benchmark):
        return []
    rows = []
    if benchmark.name is not None:
        rows.append(("name", benchmark.name))
        kind = "synthetic sampled-rect workload" if benchmark.name == "sampled-rect-modulated" else "normal GUI render-loop run"
        rows.append(("run type", kind))
    if benchmark.configured_frames is not None:
        rows.append(("configured frames", str(benchmark.configured_frames)))
    if benchmark.completed_frames is not None:
        rows.append(("completed frames", str(benchmark.completed_frames)))
    if benchmark.completion_reason is not None:
        rows.append(("completion reason", benchmark.completion_reason))
    if benchmark.workload is not None:
        rows.append(("workload", benchmark.workload))
    for key in (
        "viewport",
        "rects",
        "pixels",
        "span_px_lt4",
        "span_px_4_7",
        "span_px_8_15",
        "target_distribution",
        "vertex_color_rgba",
        "texture",
        "texture_pattern",
        "transparent",
        "translucent",
        "opaque",
        "fixed_workload_per_frame",
    ):
        if key in benchmark.config:
            rows.append((key, benchmark.config[key]))
    if benchmark.disabled_reasons:
        rows.append(("disabled reasons", ", ".join(f"{reason}:{count}" for reason, count in sorted(benchmark.disabled_reasons.items()))))
    return rows


def benchmark_has_data(benchmark: BenchmarkSummary) -> bool:
    return any((benchmark.name, benchmark.configured_frames, benchmark.completed_frames, benchmark.workload, benchmark.config, benchmark.disabled_reasons))


def print_report(path: Path, sections: dict[str, SectionStats], benchmark: BenchmarkSummary, include_all: bool, zero_limit: int) -> None:
    print("# PortMaster Renderer Log Summary")
    print()
    print(f"Log: `{path}`")
    print()
    print("## Overview")
    overview = [(name, str(sections[name].count), range_text(sections[name].frames), range_text(sections[name].timestamps)) for _, name in SECTION_MARKERS]
    print_table(overview, ("section", "samples", "frame range", "timestamp range"))
    print()
    rows = benchmark_rows(benchmark)
    if rows:
        print("## Render Benchmark")
        print_table(rows, ("field", "value"))
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
        if section.name == "raster_stats":
            normalized_rows, unavailable = normalized_sampled_rect_rows(section)
            print("### Normalized Sampled-Rect Timing Distributions")
            print("Ratios are computed from matching values in the same raster_stats sample; samples with missing or zero denominators are skipped.")
            if normalized_rows:
                print_table(normalized_rows, ("metric", "unit", "n", "skipped", "min", "p50", "p90", "p95", "max", "mean"))
            else:
                print("Insufficient data for normalized sampled-rect timing metrics.")
            if unavailable:
                print("Unavailable: " + "; ".join(unavailable))
            print()
            constant_texel_rows = constant_texel_summary_rows(section)
            if constant_texel_rows:
                print("### Constant-Texel Span Summary")
                print(
                    "Opportunity/helper-use counters describe repeated-color spans and helper paths; they do not prove SIMD execution."
                )
                print_table(constant_texel_rows, ("metric", "px", "share"))
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
    sections, benchmark = parse_records(path)
    if not any(section.count for section in sections.values()):
        print(f"error: no renderer stats sections found in {path}", file=sys.stderr)
        return 1
    print_report(path, sections, benchmark, args.all_counters, args.zero_limit)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
