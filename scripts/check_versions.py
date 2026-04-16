#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
TS_PACKAGE = ROOT / "typescript" / "package.json"
TS_CLIENT = ROOT / "typescript" / "src" / "index.ts"
PYPROJECT = ROOT / "python" / "pyproject.toml"
PY_CLIENT = ROOT / "python" / "src" / "pipe_storage" / "client.py"
RUST_CARGO = ROOT / "rust" / "Cargo.toml"
RUST_CLIENT = ROOT / "rust" / "src" / "client.rs"


def read_ts_version() -> str:
    data = json.loads(TS_PACKAGE.read_text(encoding="utf-8"))
    version = str(data.get("version", "")).strip()
    if not version:
        raise RuntimeError(f"missing version in {TS_PACKAGE}")
    return version


def read_version_line(path: pathlib.Path) -> str:
    pattern = re.compile(r'^version\s*=\s*"([^"]+)"\s*$', re.MULTILINE)
    match = pattern.search(path.read_text(encoding="utf-8"))
    if not match:
        raise RuntimeError(f"missing version in {path}")
    return match.group(1).strip()


def read_user_agent_version(path: pathlib.Path, pattern: str) -> str:
    compiled = re.compile(pattern, re.MULTILINE)
    match = compiled.search(path.read_text(encoding="utf-8"))
    if not match:
        raise RuntimeError(f"missing SDK_USER_AGENT in {path}")
    return match.group(1).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description="Check SDK version alignment")
    parser.add_argument("--expected", help="Expected x.y.z version")
    args = parser.parse_args()

    ts = read_ts_version()
    py = read_version_line(PYPROJECT)
    rs = read_version_line(RUST_CARGO)

    versions = {
        "typescript": ts,
        "python": py,
        "rust": rs,
    }

    unique = set(versions.values())
    if len(unique) != 1:
        print("Version mismatch detected:")
        for name, value in versions.items():
            print(f"  {name}: {value}")
        return 1

    version = next(iter(unique))
    if args.expected and version != args.expected:
        print(f"Version mismatch: expected {args.expected}, found {version}")
        return 1

    ua_versions = {
        "typescript": read_user_agent_version(
            TS_CLIENT,
            r'^const SDK_USER_AGENT = "pipe-agent-storage-ts/([^"]+)";$',
        ),
        "python": read_user_agent_version(
            PY_CLIENT,
            r'^SDK_USER_AGENT = "pipe-agent-storage-python/([^"]+)"$',
        ),
        "rust": read_user_agent_version(
            RUST_CLIENT,
            r'^const SDK_USER_AGENT: &str = "pipe-agent-storage-rust/([^"]+)";$',
        ),
    }

    for name, ua_version in ua_versions.items():
        if ua_version != version:
            print(
                f"SDK user-agent mismatch for {name}: expected {version}, found {ua_version}"
            )
            return 1

    print(f"SDK versions aligned: {version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
