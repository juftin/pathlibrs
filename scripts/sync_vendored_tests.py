#!/usr/bin/env python3
"""Sync vendored CPython test files from the upstream repository.

Fetches test_pathlib.py and companion files from a specific CPython git
ref and compares them against the local vendored copies. In ``--write``
mode it replaces local files with the upstream versions.

Usage:

    uv run python scripts/sync_vendored_tests.py             # diff vs latest release
    uv run python scripts/sync_vendored_tests.py --ref main  # diff vs main branch
    uv run python scripts/sync_vendored_tests.py --write     # download + write
    uv run python scripts/sync_vendored_tests.py --discover-latest-tag  # print tag
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import urllib.request
from pathlib import Path

FILES = [
    "__init__.py",
    "test_pathlib.py",
    "test_join.py",
    "test_join_posix.py",
    "test_join_windows.py",
    "test_copy.py",
    "test_read.py",
    "test_write.py",
    "support/lexical_path.py",
    "support/local_path.py",
    "support/zip_path.py",
]

LOCALLY_MODIFIED = {
    "support/__init__.py",
}



def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _fetch(url: str) -> bytes | None:
    try:
        with urllib.request.urlopen(url, timeout=30) as resp:
            return resp.read()
    except urllib.error.HTTPError as exc:
        print(f"  HTTP {exc.code} — skipping", file=sys.stderr)
        return None
    except OSError as exc:
        print(f"  Network error: {exc} — skipping", file=sys.stderr)
        return None


def discover_latest_stable_tag() -> str:
    """Return the latest CPython stable release tag (e.g. ``v3.14.6``).

    Queries the GitHub tags API, filtering for ``v3.N.M`` patterns
    (excluding ``a``, ``b``, ``rc`` pre-releases).
    """
    url = "https://api.github.com/repos/python/cpython/git/matching-refs/tags/v3."
    try:
        with urllib.request.urlopen(url, timeout=30) as resp:
            data = json.loads(resp.read())
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: could not fetch tags: {exc}", file=sys.stderr)
        sys.exit(1)

    stable = re.compile(r"^refs/tags/v(\d+)\.(\d+)\.(\d+)$")
    tags: list[tuple[int, int, int]] = []
    for entry in data:
        ref = entry["ref"]
        m = stable.match(ref)
        if m:
            tags.append((int(m.group(1)), int(m.group(2)), int(m.group(3))))

    if not tags:
        print("error: no stable release tags found", file=sys.stderr)
        sys.exit(1)

    tags.sort(reverse=True)
    major, minor, patch = tags[0]
    return f"v{major}.{minor}.{patch}"


def _build_base_url(ref: str) -> str:
    return (
        f"https://raw.githubusercontent.com/python/cpython"
        f"/{ref}/Lib/test/test_pathlib"
    )


def sync(vendor_dir: Path, base_url: str, *, write: bool) -> int:
    """Download upstream files and compare with local copies.

    Returns:
        Number of files that differ (0 = no changes needed).
    """
    changed = 0

    for filename in FILES:
        url = f"{base_url}/{filename}"
        dest = vendor_dir / filename

        print(f"  {filename:40s}  ", end="", flush=True)

        if filename in LOCALLY_MODIFIED:
            print("skipped (locally modified)")
            continue

        upstream = _fetch(url)
        if upstream is None:
            continue

        local = dest.read_bytes() if dest.exists() else b""

        if local == upstream:
            print("unchanged")
            continue

        changed += 1
        print(
            f"CHANGED  "
            f"(local: {_sha256(local)[:8] if local else 'NEW'}  "
            f"upstream: {_sha256(upstream)[:8]})"
        )

        if write:
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(upstream)

    return changed


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Sync vendored CPython test files from upstream."
    )
    parser.add_argument(
        "--ref",
        help="CPython git ref to sync from (default: latest stable release tag).",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="Write upstream files to the local vendored directory.",
    )
    parser.add_argument(
        "--discover-latest-tag",
        action="store_true",
        help="Print the latest stable CPython release tag and exit.",
    )
    args = parser.parse_args()

    if args.discover_latest_tag:
        print(discover_latest_stable_tag())
        return

    ref = args.ref if args.ref else discover_latest_stable_tag()
    base_url = _build_base_url(ref)

    repo_root = Path(__file__).resolve().parent.parent
    vendor_dir = repo_root / "tests" / "vendored"

    if not vendor_dir.is_dir():
        print(f"error: vendored directory not found: {vendor_dir}", file=sys.stderr)
        sys.exit(1)

    print(f"Syncing from CPython {ref}")
    print(f"  → {vendor_dir}/")
    if args.write:
        print("  (write mode — local files will be overwritten)")
    print()

    changed = sync(vendor_dir, base_url, write=args.write)

    print()
    if changed == 0:
        print(f"All vendored files up to date with CPython {ref}.")
    elif args.write:
        print(f"{changed} file(s) synced from CPython {ref}.")
    else:
        print(
            f"{changed} file(s) have upstream changes. "
            f"Run with --write to sync from {ref}."
        )

    sys.exit(0 if changed == 0 else 1)


if __name__ == "__main__":
    main()
