//! Integration stress test: SIWS signup -> upload files -> download & verify blake3 -> delete all -> logout.
//!
//! Disabled by default to avoid accidental writes against shared infrastructure.
//! Enable with:
//!
//!   PIPE_STRESS_TEST=1 cargo test --test integration_stress -- --nocapture
//!
//! Scale up via env vars for deliberate stress testing:
//!
//!   PIPE_STRESS_TEST=1 PIPE_STRESS_COUNT=500 PIPE_STRESS_MAX_SIZE=1048576 cargo test --test integration_stress -- --nocapture
//!
//! Optional env vars:
//!   PIPE_BASE_URL        - server URL (default: production)
//!   PIPE_STRESS_COUNT    - number of files (default: 5)
//!   PIPE_STRESS_CONC     - upload/download concurrency (default: 5)
//!   PIPE_STRESS_MIN_SIZE - min file size in bytes (default: 1024)
//!   PIPE_STRESS_MAX_SIZE - max file size in bytes (default: 4096)

mod common;

use futures::stream::{self, StreamExt};
use pipe_agent_storage::{PipeStorage, PipeStorageOptions, StoreOptions};
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Record for a single uploaded file.
struct UploadRecord {
    file_name: String,
    original_hash: blake3::Hash,
    original_size: usize,
    deterministic_url: Option<String>,
}

#[tokio::test]
async fn stress_test_full_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("PIPE_STRESS_TEST").ok().as_deref() != Some("1") {
        eprintln!("Skipping stress test. Set PIPE_STRESS_TEST=1 to enable.");
        return Ok(());
    }

    let base_url = common::env_or("PIPE_BASE_URL", common::DEFAULT_BASE_URL);
    let file_count: usize = common::env_or("PIPE_STRESS_COUNT", "5").parse()?;
    let concurrency: usize = common::env_or("PIPE_STRESS_CONC", "5").parse()?;
    let min_size: usize = common::env_or("PIPE_STRESS_MIN_SIZE", "1024").parse()?;
    let max_size: usize = common::env_or("PIPE_STRESS_MAX_SIZE", "4096").parse()?;

    let total_start = Instant::now();

    // -- Step 1: Generate Solana keypair --
    println!(
        "\n=== STRESS TEST: {} files, concurrency={} ===\n",
        file_count, concurrency
    );
    let (signing_key, wallet_pubkey) = common::load_or_create_keypair();
    println!("[1/7] Loaded keypair: {}", &wallet_pubkey[..12]);

    // -- Step 2: SIWS auth --
    let mut client = PipeStorage::new(PipeStorageOptions {
        base_url: Some(base_url.clone()),
        timeout: Some(Duration::from_secs(300)),
        poll_interval: Some(Duration::from_secs(1)),
        ..Default::default()
    });

    let challenge = client.auth_challenge(&wallet_pubkey).await?;
    let signature_b64 = common::sign_message(&signing_key, &challenge.message);
    let session = client
        .auth_verify(
            &wallet_pubkey,
            &challenge.nonce,
            &challenge.message,
            &signature_b64,
        )
        .await?;
    println!(
        "[2/7] Authenticated via SIWS (token: {}...)",
        &session.access_token[..16.min(session.access_token.len())]
    );

    // -- Step 3: Generate payloads --
    let mut payloads: Vec<(String, Vec<u8>, blake3::Hash)> = Vec::with_capacity(file_count);
    let mut total_bytes: u64 = 0;
    for i in 0..file_count {
        let size = if min_size == max_size {
            min_size
        } else {
            min_size + (rand::random::<usize>() % (max_size - min_size))
        };
        let data = common::random_payload(size);
        let hash = blake3::hash(&data);
        let file_name = format!("stress-test/{}-{}-{}.bin", common::now_ms(), i, size);
        total_bytes += size as u64;
        payloads.push((file_name, data, hash));
    }
    println!(
        "[3/7] Generated {} payloads ({:.2} MB total)",
        file_count,
        total_bytes as f64 / 1_048_576.0
    );

    // -- Step 4: Upload concurrently --
    let upload_start = Instant::now();
    let sem = Arc::new(Semaphore::new(concurrency));
    let client_arc = Arc::new(client.clone());

    let upload_results: Vec<Result<UploadRecord, String>> = stream::iter(payloads.into_iter())
        .map(|(file_name, data, hash)| {
            let sem = sem.clone();
            let client = client_arc.clone();
            async move {
                let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
                let size = data.len();
                let stored = client
                    .store(
                        data,
                        StoreOptions {
                            file_name: Some(file_name.clone()),
                            wait: true,
                            timeout: Some(Duration::from_secs(300)),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| format!("store {} failed: {}", file_name, e))?;

                Ok(UploadRecord {
                    file_name,
                    original_hash: hash,
                    original_size: size,
                    deterministic_url: stored.deterministic_url,
                })
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut uploads: Vec<UploadRecord> = Vec::with_capacity(file_count);
    let mut upload_failures = 0;
    for result in upload_results {
        match result {
            Ok(record) => uploads.push(record),
            Err(e) => {
                eprintln!("  UPLOAD FAIL: {}", e);
                upload_failures += 1;
            }
        }
    }

    let upload_elapsed = upload_start.elapsed();
    println!(
        "[4/7] Uploaded {} files in {:.1}s ({} failed, {:.1} files/s)",
        uploads.len(),
        upload_elapsed.as_secs_f64(),
        upload_failures,
        uploads.len() as f64 / upload_elapsed.as_secs_f64()
    );

    assert!(
        upload_failures == 0,
        "{} uploads failed out of {}",
        upload_failures,
        file_count
    );

    // -- Step 5: Download and verify blake3 --
    let download_start = Instant::now();
    let sem = Arc::new(Semaphore::new(concurrency));

    let verify_results: Vec<Result<(), String>> = stream::iter(uploads.iter())
        .map(|record| {
            let sem = sem.clone();
            let client = client_arc.clone();
            let file_name = record.file_name.clone();
            let expected_hash = record.original_hash;
            let expected_size = record.original_size;
            let fetch_key = record
                .deterministic_url
                .clone()
                .unwrap_or_else(|| file_name.clone());
            async move {
                let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
                let data = client
                    .fetch(&fetch_key)
                    .await
                    .map_err(|e| format!("fetch {} failed: {}", file_name, e))?;

                if data.len() != expected_size {
                    return Err(format!(
                        "{}: size mismatch (expected {}, got {})",
                        file_name,
                        expected_size,
                        data.len()
                    ));
                }

                let actual_hash = blake3::hash(&data);
                if actual_hash != expected_hash {
                    return Err(format!(
                        "{}: blake3 mismatch (expected {}, got {})",
                        file_name, expected_hash, actual_hash
                    ));
                }

                Ok(())
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut verify_failures = 0;
    for result in &verify_results {
        if let Err(e) = result {
            eprintln!("  VERIFY FAIL: {}", e);
            verify_failures += 1;
        }
    }

    let download_elapsed = download_start.elapsed();
    println!(
        "[5/7] Downloaded & verified {} files in {:.1}s ({} failed, {:.1} files/s)",
        uploads.len(),
        download_elapsed.as_secs_f64(),
        verify_failures,
        uploads.len() as f64 / download_elapsed.as_secs_f64()
    );

    assert!(
        verify_failures == 0,
        "{} verifications failed out of {}",
        verify_failures,
        uploads.len()
    );

    // -- Step 6: Delete all files --
    let delete_start = Instant::now();
    let sem = Arc::new(Semaphore::new(concurrency));

    let delete_results: Vec<Result<(), String>> = stream::iter(uploads.iter())
        .map(|record| {
            let sem = sem.clone();
            let client = client_arc.clone();
            let file_name = record.file_name.clone();
            async move {
                let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
                client
                    .delete_file_name(&file_name)
                    .await
                    .map_err(|e| format!("delete {} failed: {}", file_name, e))?;
                Ok(())
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut delete_failures = 0;
    for result in &delete_results {
        if let Err(e) = result {
            eprintln!("  DELETE FAIL: {}", e);
            delete_failures += 1;
        }
    }

    let delete_elapsed = delete_start.elapsed();
    println!(
        "[6/7] Deleted {} files in {:.1}s ({} failed)",
        uploads.len(),
        delete_elapsed.as_secs_f64(),
        delete_failures,
    );

    // -- Step 7: Logout --
    drop(client_arc);
    client.auth_logout().await?;
    println!("[7/7] Logged out");

    // -- Summary --
    let total_elapsed = total_start.elapsed();
    println!("\n=== STRESS TEST COMPLETE ===");
    println!("  Files:          {}", file_count);
    println!(
        "  Total data:     {:.2} MB",
        total_bytes as f64 / 1_048_576.0
    );
    println!("  Upload time:    {:.1}s", upload_elapsed.as_secs_f64());
    println!("  Download time:  {:.1}s", download_elapsed.as_secs_f64());
    println!("  Delete time:    {:.1}s", delete_elapsed.as_secs_f64());
    println!("  Total time:     {:.1}s", total_elapsed.as_secs_f64());
    println!(
        "  Upload fails:   {} | Verify fails: {} | Delete fails: {}",
        upload_failures, verify_failures, delete_failures
    );

    assert_eq!(upload_failures, 0, "upload failures");
    assert_eq!(verify_failures, 0, "verify failures");
    assert_eq!(delete_failures, 0, "delete failures");

    Ok(())
}
