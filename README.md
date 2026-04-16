# Pipe Agent SDKs (Preview)

Multi-language SDKs for Pipe Storage with Solana wallet authentication.

- `typescript/` — TypeScript SDK (`PipeStorageClient`, package name `@pipe-network/agent-storage`)
- `python/` — Python SDK (`PipeStorage`)
- `rust/` — Rust SDK (`PipeStorage`)

## Core API

All SDKs expose the same surface:

### Auth (Sign In With Solana)

- `auth_challenge(wallet_public_key)` — get nonce + message to sign
- `auth_verify(wallet_public_key, nonce, message, signature_b64)` — verify signature, get JWT tokens
- `auth_refresh()` — refresh expired access token
- `auth_logout()` — invalidate session

After `auth_verify`, the access token is automatically used for all subsequent calls. If a request gets a 401 and a refresh token is available, the SDK auto-refreshes and retries (Python/TypeScript transparent, Rust via `should_refresh()` helper).

### Storage

- `store(data)` — upload, poll for completion, return content hash + deterministic URL
- `fetch(key)` — download by file name, hash, operation ID, or URL
- `pin(key)` — resolve any key type to a public deterministic URL (`/{account}/{hash}`)
- `delete(key)` — remove by file name or operation ID

### Framework adapters

- OpenAI tool definitions + dispatcher
- Anthropic tool definitions + dispatcher
- Vercel AI SDK tool map
- Cloudflare AI Workflows tool definitions
- AutoGen tool schemas + function map
- CrewAI tool wrappers
- LangChain-style tool adapter
- LlamaIndex-style tool adapter

## Environment variables

| Variable | Purpose |
|---|---|
| `PIPE_API_KEY` | `user_app_key` or JWT access token |
| `PIPE_ACCOUNT` | Account ID for deterministic URL generation |
| `PIPE_BASE_URL` | Override base URL (default: `https://us-west-01-firestarter.pipenetwork.com`) |

## Two auth modes

There are two valid ways to use these SDKs:

1. SIWS mode (preferred / expected)
   - use SIWS (`auth_challenge` -> sign -> `auth_verify`)
   - use the authenticated bearer session immediately
2. Provisioned API-key mode (optional compatibility mode)
   - you already have a Pipe `user_app_key`
   - you already know the `PIPE_ACCOUNT` value to use
   - examples and quickstarts mostly assume this mode for simplicity

## Credential bootstrap

The SDKs support SIWS auth methods, but the shipped quickstarts and example
programs are **runtime usage examples**, not account-provisioning flows.

In practice, that means:

- the current examples assume you already have a Pipe account
- they assume you already have `PIPE_API_KEY`
- they assume you already know `PIPE_ACCOUNT`
- they do **not** create a user or fetch `user_app_key` for you

The recommended setup is:

1. start with SIWS
2. use the authenticated bearer session directly
3. only switch to `PIPE_API_KEY` mode if your environment explicitly wants that operational model

If you want to bootstrap credentials programmatically:

1. call `auth_challenge`
2. sign the returned message with the wallet
3. call `auth_verify`
4. use the returned bearer session immediately
5. optionally call `/user/me` and persist `user_app_key` for future API-key-mode runs

Do not assume the example programs will do this for you.

## Default Host Behavior

All SDKs default to `https://us-west-01-firestarter.pipenetwork.com` with zero configuration.
That endpoint is production, so reads/writes are real and may incur usage cost.
For staging/local environments, set `PIPE_BASE_URL` (or pass SDK-specific base URL options).

## Quickstarts

These quickstarts currently require a pre-existing `PIPE_API_KEY` and `PIPE_ACCOUNT`.
That is for convenience only; it is not the primary recommended auth path.

```bash
export PIPE_API_KEY="<user_app_key_or_token>"
export PIPE_ACCOUNT="<account>"

# TypeScript
cd typescript && npm install && npm run build && cd ..
node ./quickstart/typescript_agent.mjs

# Python
PYTHONPATH=./python/src python3 ./quickstart/python_agent.py

# Rust
cargo run --manifest-path rust/Cargo.toml --example openai_tool
```

## Tests

### Unit tests (offline, no API key needed)

```bash
# Python (73 tests)
python3 -m unittest python.tests.test_client -v

# TypeScript (56 tests)
cd typescript && npm run build && node --test tests/test_client.mjs

# Rust (74 tests)
cd rust && cargo test --test test_client
```

### Integration-flow test (offline, mocked)

```bash
# Python integration-style lifecycle test (no network required)
python3 ./python/tests/integration_test.py
```

### Live integration tests (opt-in, requires live server + API key)

```bash
export PIPE_API_KEY="<token>"
export PIPE_ACCOUNT="<account>"

# TypeScript
cd typescript && npm run test:integration

# Rust
cd rust && PIPE_RUN_INTEGRATION_TESTS=1 cargo test --test integration -- --nocapture
```

### Stress test (opt-in; creates account via SIWS, uploads/verifies/deletes files)

```bash
PIPE_STRESS_TEST=1 cargo test --manifest-path rust/Cargo.toml --test integration_stress -- --nocapture
```

Optional config: `PIPE_STRESS_COUNT` (default 5), `PIPE_STRESS_CONC` (default 5), `PIPE_STRESS_MIN_SIZE` (default 1024), `PIPE_STRESS_MAX_SIZE` (default 4096).

## Other docs

- `INTEGRATIONS.md` — framework adapter matrix
- `bench/README.md` — benchmark guide
- `RELEASING.md` — release guide
- `.github/workflows/ci.yml` — CI workflow
- `.github/workflows/publish.yml` — publish workflow
