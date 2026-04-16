#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Version alignment"
python3 ./scripts/check_versions.py

echo "==> TypeScript"
(
  cd typescript
  npm ci
  npm run build
  npx tsc -p tsconfig.json --noEmit
  node --test tests/test_client.mjs
)

echo "==> Python"
python3 -m py_compile \
  python/src/pipe_storage/client.py \
  python/src/pipe_storage/frameworks.py \
  python/src/pipe_storage/__init__.py \
  python/tests/integration_test.py \
  python/tests/test_client.py \
  python/examples/openai_tool.py \
  python/examples/autogen_tools.py \
  python/examples/crewai_tools.py \
  quickstart/python_agent.py
python3 -m unittest python.tests.test_client -v
python3 ./python/tests/integration_test.py

echo "==> Rust"
cargo fmt --manifest-path rust/Cargo.toml --check
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml --test test_client -- --nocapture

if [[ "${PIPE_RUN_INTEGRATION_TESTS:-0}" == "1" ]]; then
  echo "==> Live integration tests (opt-in)"
  (
    cd typescript
    npm run test:integration
  )
  PIPE_RUN_INTEGRATION_TESTS=1 cargo test --manifest-path rust/Cargo.toml --test integration -- --nocapture
fi

if [[ "${PIPE_RUN_STRESS_TESTS:-0}" == "1" ]]; then
  echo "==> Stress tests (opt-in)"
  PIPE_STRESS_TEST=1 cargo test --manifest-path rust/Cargo.toml --test integration_stress -- --nocapture
fi

echo "All checks passed"
