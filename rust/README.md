# Pipe Agent Storage SDK (Rust)

Async Rust SDK for Pipe Storage with Solana wallet authentication. Uses `reqwest` + `tokio`.

Default behavior: `PipeStorage::new(PipeStorageOptions::default())` and
`PipeStorage::from_env()` use `https://us-west-01-firestarter.pipenetwork.com`
(production) unless you override `PIPE_BASE_URL` (or `PIPE_API_BASE_URL`).
Requests are real and may incur usage cost.

## Add to project

```toml
[dependencies]
pipe-agent-storage = { path = "./rust" }
```

## Auth (Sign In With Solana)

```rust
use pipe_agent_storage::{PipeStorage, PipeStorageOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipe = PipeStorage::new(PipeStorageOptions::default());

    // 1. Get challenge
    let challenge = pipe.auth_challenge("Base58WalletPubkey...").await?;

    // 2. Sign challenge.message with your ed25519 key (external)
    let signature_b64 = sign_with_wallet(&challenge.message);

    // 3. Verify — auto-sets credentials for all subsequent calls
    let session = pipe.auth_verify(
        "Base58WalletPubkey...",
        &challenge.nonce,
        &challenge.message,
        &signature_b64,
    ).await?;

    // 4. Refresh when token expires
    pipe.auth_refresh().await?;

    // 5. Auto-refresh helper for &self methods (Rust requires &mut self for refresh)
    match pipe.store("data", Default::default()).await {
        Err(ref e) if pipe.should_refresh(e) => {
            pipe.auth_refresh().await?;
            pipe.store("data", Default::default()).await?;
        }
        other => { other?; }
    }

    // 6. Logout
    pipe.auth_logout().await?;
    Ok(())
}
```

Or use a static API key:

```bash
export PIPE_API_KEY="<your_jwt_or_api_token>"
export PIPE_ACCOUNT="<user_id_or_public_slug>"
```

## Storage

```rust
use pipe_agent_storage::{PipeStorage, StoreOptions};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pipe = PipeStorage::from_env();

    let stored = pipe.store_json(
        &json!({"hello": "world"}),
        StoreOptions {
            file_name: Some("agent/state.json".to_string()),
            ..Default::default()
        },
    ).await?;

    let pinned = pipe.pin("agent/state.json").await?;
    let value: serde_json::Value = pipe.fetch_json(&pinned.url).await?;
    pipe.delete("agent/state.json").await?;
    Ok(())
}
```

## Framework adapters

- OpenAI: `create_openai_pipe_tools`, `run_openai_pipe_tool`
- Anthropic: `create_anthropic_pipe_tools`, `run_anthropic_pipe_tool`
- LangChain: `PipeStorageLangChainTool`
- LlamaIndex: `create_llamaindex_pipe_tools`

## Tests

```bash
# Unit tests (74 tests, offline)
cargo test --test test_client

# Live integration test (opt-in, requires PIPE_API_KEY)
PIPE_RUN_INTEGRATION_TESTS=1 cargo test --test integration -- --nocapture

# Stress test (opt-in; creates SIWS account, uploads/verifies/deletes files)
PIPE_STRESS_TEST=1 cargo test --test integration_stress -- --nocapture
```

Override host with `PIPE_BASE_URL`. Stress test config defaults: `PIPE_STRESS_COUNT=5`, `PIPE_STRESS_CONC=5`, `PIPE_STRESS_MIN_SIZE=1024`, `PIPE_STRESS_MAX_SIZE=4096`.
