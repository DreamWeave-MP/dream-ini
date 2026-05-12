#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

"""Verify RasterStats fields are emitted by RasterStats::log_line."""

from __future__ import annotations

import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
STATS_RS = REPO_ROOT / "src" / "gui" / "portmaster" / "raster" / "stats.rs"


def extract_raster_stats_struct(source: str) -> str:
    start_match = re.search(r"\bstruct\s+RasterStats\s*\{", source)
    if start_match is None:
        raise ValueError("could not find RasterStats struct")

    start = start_match.end()
    end_match = re.search(r"^}\s*$", source[start:], re.MULTILINE)
    if end_match is None:
        raise ValueError("could not find end of RasterStats struct")

    return source[start : start + end_match.start()]


def extract_log_line(source: str) -> str:
    start_match = re.search(r"\bfn\s+log_line\s*\(", source)
    if start_match is None:
        raise ValueError("could not find RasterStats::log_line")

    end_match = re.search(r"^#\[cfg\(test\)\]", source[start_match.start() :], re.MULTILINE)
    if end_match is None:
        raise ValueError("could not find end marker after RasterStats::log_line")

    return source[start_match.start() : start_match.start() + end_match.start()]


def extract_raster_stats_values(source: str) -> str:
    start_match = re.search(r"macro_rules!\s+raster_stats_values\s*\{", source)
    if start_match is None:
        raise ValueError("could not find raster_stats_values macro")

    start = start_match.end()
    end_match = re.search(r"^}\s*$", source[start:], re.MULTILINE)
    if end_match is None:
        raise ValueError("could not find end of raster_stats_values macro")

    return source[start : start + end_match.start()]


def difference(left: list[str], right: list[str]) -> list[str]:
    right_values = set(right)
    return [value for value in left if value not in right_values]


def main() -> int:
    source = STATS_RS.read_text(encoding="utf-8")
    struct_body = extract_raster_stats_struct(source)
    log_line = extract_log_line(source)
    value_source = extract_raster_stats_values(source)

    fields = re.findall(
        r"pub\(in\s+crate::gui::portmaster\)\s+([A-Za-z_][A-Za-z0-9_]*)\s*:",
        struct_body,
    )
    labels = re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)=\{\}", log_line)
    arguments = re.findall(r"\$stats\.([A-Za-z_][A-Za-z0-9_]*)\b", value_source)

    errors = []
    if missing := difference(fields, labels):
        errors.append("fields missing from log labels: " + ", ".join(missing))
    if extra := difference(labels, fields):
        errors.append("log labels without struct fields: " + ", ".join(extra))
    if missing := difference(fields, arguments):
        errors.append("fields missing from log arguments: " + ", ".join(missing))
    if extra := difference(arguments, fields):
        errors.append("runtime value sources without struct fields: " + ", ".join(extra))
    if "raster_stats_values!(self)" not in log_line:
        errors.append("RasterStats::log_line does not use raster_stats_values!(self)")
    if labels != arguments:
        errors.append("log label order does not match runtime value order")

    if errors:
        print(f"{STATS_RS.relative_to(REPO_ROOT)} RasterStats schema check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"RasterStats schema OK: {len(fields)} fields represented in log_line")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
