#!/usr/bin/env python3
"""Generate a Markdown benchmark summary from pytest-benchmark JSON output."""

import json
import sys
from pathlib import Path

FILE_GROUP_MAP = {
    "test_pure_ops": "pure",
    "test_stat_ops": "stat",
    "test_io_ops": "io",
    "test_dir_ops": "dir",
    "test_glob_ops": "glob",
    "test_mutation_ops": "mutation",
    "test_memory": "memory",
}


def _group_name(fullname: str) -> str:
    for key, label in FILE_GROUP_MAP.items():
        if key in fullname:
            return label
    return "other"


def _short_name(fullname: str) -> str:
    name = fullname.split("::")[-1] if "::" in fullname else fullname
    name = name.replace("test_", "")
    name = name.replace("[pathlib]", "")
    name = name.replace("[pathlibrs]", "")
    return name


def _ratio_label(ratio: float) -> str:
    if ratio < 0.97:
        return f"{1.0 / ratio:.2f}x faster :rocket:"
    elif ratio > 1.03:
        return f"{ratio:.2f}x slower :snail:"
    return "~parity"


def _fmt_time(t: float) -> str:
    if t < 0.000001:
        return f"{t * 1_000_000_000:.0f} ns"
    elif t < 0.001:
        return f"{t * 1_000_000:.1f} us"
    elif t < 1.0:
        return f"{t * 1000:.1f} ms"
    return f"{t:.1f} s"


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: benchmark_comment.py <benchmark.json>", file=sys.stderr)
        sys.exit(1)

    json_path = Path(sys.argv[1])
    if not json_path.exists():
        print(f"File not found: {json_path}", file=sys.stderr)
        sys.exit(1)

    with open(json_path) as f:
        data = json.load(f)

    benchmarks = data.get("benchmarks", [])
    if not benchmarks:
        print("No benchmarks found.", file=sys.stderr)
        sys.exit(1)

    medians: dict[str, dict[str, float]] = {}
    for b in benchmarks:
        fullname = b.get("fullname", b["name"])
        impl = b.get("param", "")
        if impl in ("pathlib", "pathlibrs"):
            base = fullname.rsplit("[", 1)[0]
            medians.setdefault(base, {})[impl] = b["stats"]["median"]

    def _sort_key(item: tuple[str, dict[str, float]]) -> tuple[int, str]:
        name = item[0]
        for i, key in enumerate(FILE_GROUP_MAP):
            if key in name:
                return (i, name)
        return (99, name)

    print("## Benchmark Summary — pathlibrs vs pathlib (release build)\n")
    print("| Category | Operation | pathlib | pathlibrs | Ratio |")
    print("|----------|-----------|---------|-----------|-------|")

    current_group = ""
    for base_name, timings in sorted(medians.items(), key=_sort_key):
        if "pathlib" not in timings or "pathlibrs" not in timings:
            continue

        group = _group_name(base_name)
        if group != current_group:
            current_group = group
            print(f"| | **{group}** | | | |")

        py = timings["pathlib"]
        rs = timings["pathlibrs"]
        if py == 0:
            continue

        ratio = rs / py
        label = _ratio_label(ratio)
        name = _short_name(base_name)

        print(f"| | `{name}` | {_fmt_time(py)} | {_fmt_time(rs)} | {label} |")

    # Count faster/slower/parity
    faster = slower = parity = 0
    for _, timings in medians.items():
        if "pathlib" not in timings or "pathlibrs" not in timings:
            continue
        py = timings["pathlib"]
        rs = timings["pathlibrs"]
        if py == 0:
            continue
        ratio = rs / py
        if ratio < 0.97:
            faster += 1
        elif ratio > 1.03:
            slower += 1
        else:
            parity += 1

    total = faster + slower + parity
    print(f"\n**{faster} faster, {slower} slower, {parity} at parity** out of {total} benchmarks.")
    print(f"_Generated from `{json_path.name}` — median of all rounds._")
    print("_pathlibrs built with `maturin develop --release` (LTO enabled)._")


if __name__ == "__main__":
    main()
