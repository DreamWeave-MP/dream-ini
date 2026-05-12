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


def extract_balanced_call(source: str, start: int) -> str:
    paren = source.index("(", start)
    depth = 0
    in_string = False
    escaped = False
    for index in range(paren, len(source)):
        char = source[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise ValueError("could not find end of macro call")


def extract_write_bindings(log_line: str) -> list[tuple[str, int]]:
    bindings = []
    for match in re.finditer(r"\bwrite!\s*\(", log_line):
        call = extract_balanced_call(log_line, match.start())
        format_match = re.search(r'"((?:[^"\\]|\\.)*)"', call)
        if format_match is None:
            raise ValueError("could not find write! format string")
        labels = re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)=\{\}", format_match.group(1))
        indexes = [int(index) for index in re.findall(r"\bvalues\[(\d+)\]", call)]
        if len(labels) != len(indexes):
            raise ValueError("write! label count does not match values[N] argument count")
        bindings.extend(zip(labels, indexes, strict=True))
    return bindings


def main() -> int:
    source = STATS_RS.read_text(encoding="utf-8")
    struct_body = extract_raster_stats_struct(source)
    log_line = extract_log_line(source)
    value_source = extract_raster_stats_values(source)

    fields = re.findall(
        r"pub\(in\s+crate::gui::portmaster\)\s+([A-Za-z_][A-Za-z0-9_]*)\s*:",
        struct_body,
    )
    bindings = extract_write_bindings(log_line)
    labels = [label for label, _ in bindings]
    indexes = [index for _, index in bindings]
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
    if not re.search(r"\blet\s+values\s*=\s*raster_stats_values!\s*\(\s*self\s*\)\s*;", log_line):
        errors.append("RasterStats::log_line does not bind values from raster_stats_values!(self)")
    expected_indexes = list(range(len(arguments)))
    if indexes != expected_indexes:
        errors.append("log values[N] arguments are duplicated, skipped, out of order, or out of range")
    resolved = [arguments[index] if 0 <= index < len(arguments) else None for index in indexes]
    if any(field is None for field in resolved):
        errors.append("log values[N] argument references an out-of-range runtime value")
    if labels != resolved:
        errors.append("log label order does not match resolved runtime value order")

    if errors:
        print(f"{STATS_RS.relative_to(REPO_ROOT)} RasterStats schema check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"RasterStats schema OK: {len(fields)} fields represented in log_line")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
