use pipe_agent_storage::{create_openai_pipe_tools, run_openai_pipe_tool, PipeStorage};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PipeStorage::from_env();

    let tools = create_openai_pipe_tools(true);
    println!(
        "openai_tools={:?}",
        tools
            .iter()
            .map(|t| t.function.name.as_str())
            .collect::<Vec<_>>()
    );

    let file_name = format!("agent/rust-openai-example-{}.json", now_ms());
    let stored = run_openai_pipe_tool(
        &client,
        "pipe_store",
        json!({
            "file_name": file_name,
            "data": {"source": "rust-example", "ts": now_ms()}
        }),
    )
    .await?;

    println!("pipe_store={}", stored);

    let deterministic_url = stored
        .get("deterministic_url")
        .and_then(|v| v.as_str())
        .ok_or("missing deterministic_url in pipe_store response")?;

    let fetched = run_openai_pipe_tool(
        &client,
        "pipe_fetch",
        json!({
            "key": deterministic_url,
            "as_json": true
        }),
    )
    .await?;

    println!("pipe_fetch={}", fetched);

    run_openai_pipe_tool(
        &client,
        "pipe_delete",
        json!({ "file_name": stored.get("file_name") }),
    )
    .await?;

    Ok(())
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
