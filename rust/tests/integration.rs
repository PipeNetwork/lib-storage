mod common;

use pipe_agent_storage::{OperationState, StoreOptions};
use serde_json::json;
use std::env;

#[tokio::test]
async fn integration_flow_prod_host() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("PIPE_RUN_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("Skipping live integration test. Set PIPE_RUN_INTEGRATION_TESTS=1 to enable.");
        return Ok(());
    }

    let (mut client, needs_logout) = common::authenticated_client().await?;

    let file_name = format!("agent/integration-rust-{}.json", common::now_ms());
    let marker = format!("marker-{}", common::now_ms());

    println!("1) store(wait=false) file_name={}", file_name);
    let stored = client
        .store_json(
            &json!({ "marker": marker, "kind": "integration" }),
            StoreOptions {
                file_name: Some(file_name.clone()),
                wait: false,
                ..Default::default()
            },
        )
        .await?;

    let operation_id = stored
        .operation_id
        .ok_or("store did not return operation_id")?;

    println!("2) check_status operation_id={}", operation_id);
    let status = client.check_status(Some(&operation_id), None).await?;
    println!("   status={:?}", status.status);

    println!("3) wait_for_operation");
    let completed = client.wait_for_operation(&operation_id, None).await?;
    assert_eq!(completed.status, OperationState::Completed);
    println!(
        "   completed: content_hash={:?}, deterministic_url={:?}",
        completed.content_hash, completed.deterministic_url
    );

    println!("4) pin");
    // Use the deterministic_url from the completed status directly,
    // rather than re-querying by operation_id (avoids server-side race).
    let deterministic_url = completed
        .deterministic_url
        .ok_or("completed status missing deterministic_url")?;
    let pinned = client.pin(&deterministic_url).await?;
    assert!(!pinned.url.is_empty());

    println!("5) fetch_json");
    let fetched: serde_json::Value = client.fetch_json(&pinned.url).await?;
    assert_eq!(
        fetched.get("marker").and_then(|v| v.as_str()),
        Some(marker.as_str())
    );

    println!("6) delete");
    client.delete_file_name(&file_name).await?;

    if needs_logout {
        println!("7) logout");
        client.auth_logout().await?;
    }

    println!(
        "Integration flow passed: operation_id={}, deterministic_url={}",
        operation_id, pinned.url
    );

    Ok(())
}
