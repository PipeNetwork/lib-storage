# Releasing SDKs

This repo contains three SDK packages that should stay version-aligned:

- TypeScript: `typescript/package.json`
- Python: `python/pyproject.toml`
- Rust: `rust/Cargo.toml`

## 1) Run full checks

```bash
./scripts/check_all.sh
```

This includes version-alignment validation across:

- `typescript/package.json`
- `python/pyproject.toml`
- `rust/Cargo.toml`

By default, `check_all.sh` runs only offline checks. Live integration and stress tests are opt-in:

```bash
PIPE_RUN_INTEGRATION_TESTS=1 ./scripts/check_all.sh
PIPE_RUN_STRESS_TESTS=1 ./scripts/check_all.sh
```

## 2) Bump versions together

```bash
./scripts/bump_version.py 0.1.1
```

## 3) Re-run checks

```bash
./scripts/check_all.sh
```

## 4) Publish

### TypeScript (npm)

```bash
cd typescript
npm publish --access public
```

### Python (PyPI)

```bash
cd python
python -m pip install --upgrade build twine
python -m build
python -m twine upload dist/*
```

### Rust (crates.io)

```bash
cd rust
cargo publish
```

## GitHub Actions Publish Workflow

Use `.github/workflows/publish.yml` for automated publishing.

- Trigger manually via `workflow_dispatch` with a `version` input.
- Or push a tag like `v0.1.1` to publish all SDKs.

Required repository secrets:

- `NPM_TOKEN`
- `PYPI_API_TOKEN`
- `CARGO_REGISTRY_TOKEN`

## Notes

- Keep package names stable unless explicitly planning a breaking rename.
- Use semantic versioning across all SDKs.
- Prefer publishing in this order: TypeScript -> Python -> Rust.
