#!/usr/bin/env python3
from __future__ import annotations

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

SEMVER = re.compile(r"^\d+\.\d+\.\d+$")


def replace_first(pattern: str, repl: str, text: str) -> str:
    out, count = re.subn(pattern, repl, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise RuntimeError(f"expected one match for pattern: {pattern}")
    return out


def bump_typescript(version: str) -> None:
    data = json.loads(TS_PACKAGE.read_text(encoding="utf-8"))
    data["version"] = version
    TS_PACKAGE.write_text(f"{json.dumps(data, indent=2)}\n", encoding="utf-8")


def bump_python(version: str) -> None:
    text = PYPROJECT.read_text(encoding="utf-8")
    text = replace_first(r'^version\s*=\s*"[^"]+"', f'version = "{version}"', text)
    PYPROJECT.write_text(text, encoding="utf-8")


def bump_rust(version: str) -> None:
    text = RUST_CARGO.read_text(encoding="utf-8")
    text = replace_first(r'^version\s*=\s*"[^"]+"', f'version = "{version}"', text)
    RUST_CARGO.write_text(text, encoding="utf-8")


def bump_user_agents(version: str) -> None:
    ts = TS_CLIENT.read_text(encoding="utf-8")
    ts = replace_first(
        r'^const SDK_USER_AGENT = "pipe-agent-storage-ts/[^"]+";$',
        f'const SDK_USER_AGENT = "pipe-agent-storage-ts/{version}";',
        ts,
    )
    TS_CLIENT.write_text(ts, encoding="utf-8")

    py = PY_CLIENT.read_text(encoding="utf-8")
    py = replace_first(
        r'^SDK_USER_AGENT = "pipe-agent-storage-python/[^"]+"$',
        f'SDK_USER_AGENT = "pipe-agent-storage-python/{version}"',
        py,
    )
    PY_CLIENT.write_text(py, encoding="utf-8")

    rs = RUST_CLIENT.read_text(encoding="utf-8")
    rs = replace_first(
        r'^const SDK_USER_AGENT: &str = "pipe-agent-storage-rust/[^"]+";$',
        f'const SDK_USER_AGENT: &str = "pipe-agent-storage-rust/{version}";',
        rs,
    )
    RUST_CLIENT.write_text(rs, encoding="utf-8")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: bump_version.py <x.y.z>")
        return 2

    version = sys.argv[1].strip()
    if not SEMVER.fullmatch(version):
        print(f"invalid semver: {version}")
        return 2

    bump_typescript(version)
    bump_python(version)
    bump_rust(version)
    bump_user_agents(version)

    print(f"Updated TypeScript/Python/Rust SDK versions to {version}")
    print("Updated SDK user-agent constants to", version)
    print("Run: python3 ./scripts/check_versions.py --expected", version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
