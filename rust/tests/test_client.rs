use pipe_agent_storage::{
    AuthSession, ChallengeResponse, DeleteResponse, OperationState, PinParams, PipeError,
    PipeStorage, PipeStorageOptions, StoreData, StoreOptions, UploadStatus, UploadTier,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helper: create a PipeStorage client pointed at a custom base URL
// ---------------------------------------------------------------------------

fn client_with_key(base_url: &str) -> PipeStorage {
    PipeStorage::new(PipeStorageOptions {
        api_key: Some("test-api-key".to_string()),
        base_url: Some(base_url.to_string()),
        account: Some("test-account".to_string()),
        timeout: Some(Duration::from_secs(5)),
        poll_interval: Some(Duration::from_millis(50)),
    })
}

fn client_no_key() -> PipeStorage {
    PipeStorage::new(PipeStorageOptions {
        api_key: None,
        base_url: Some("https://example.com".to_string()),
        account: Some("test-account".to_string()),
        ..Default::default()
    })
}

fn client_no_account() -> PipeStorage {
    PipeStorage::new(PipeStorageOptions {
        api_key: Some("key".to_string()),
        base_url: Some("https://example.com".to_string()),
        account: None,
        ..Default::default()
    })
}

/// A valid 64-char hex hash for testing.
const VALID_HASH: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
// ===========================================================================
//  1. Pure function tests (no HTTP needed)
// ===========================================================================

mod deterministic_url {
    use super::*;

    #[tokio::test]
    async fn valid_hash_and_account() {
        let c = client_with_key("https://example.com");
        let url = c.deterministic_url(VALID_HASH, None).unwrap();
        assert_eq!(
            url,
            format!("https://example.com/test-account/{}", VALID_HASH)
        );
    }

    #[tokio::test]
    async fn explicit_account_overrides_default() {
        let c = client_with_key("https://example.com");
        let url = c
            .deterministic_url(VALID_HASH, Some("other-account"))
            .unwrap();
        assert_eq!(
            url,
            format!("https://example.com/other-account/{}", VALID_HASH)
        );
    }

    #[tokio::test]
    async fn missing_account_returns_error() {
        let c = client_no_account();
        let result = c.deterministic_url(VALID_HASH, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("missing account"), "got: {msg}");
    }

    #[tokio::test]
    async fn invalid_hash_too_short() {
        let c = client_with_key("https://example.com");
        let result = c.deterministic_url("abcdef", None);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("64-character hex"), "got: {msg}");
    }

    #[tokio::test]
    async fn invalid_hash_non_hex() {
        let c = client_with_key("https://example.com");
        // 64 chars but includes 'g' and 'z'
        let bad = "gbcdef0123456789abcdef0123456789abcdef0123456789abcdef012345678z";
        assert_eq!(bad.len(), 64);
        let result = c.deterministic_url(bad, None);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn uppercase_hash_is_lowercased() {
        let c = client_with_key("https://example.com");
        let upper = VALID_HASH.to_uppercase();
        let url = c.deterministic_url(&upper, None).unwrap();
        assert!(url.ends_with(VALID_HASH)); // lowercased
    }

    #[tokio::test]
    async fn base_url_trailing_slash_normalized() {
        let c = client_with_key("https://example.com/");
        let url = c.deterministic_url(VALID_HASH, None).unwrap();
        // Should not have double slash
        assert!(
            !url.contains("//test-account"),
            "double slash detected: {url}"
        );
    }

    #[tokio::test]
    async fn account_with_special_chars_is_encoded() {
        let c = PipeStorage::new(PipeStorageOptions {
            api_key: Some("key".to_string()),
            base_url: Some("https://example.com".to_string()),
            account: Some("user@domain".to_string()),
            ..Default::default()
        });
        let url = c.deterministic_url(VALID_HASH, None).unwrap();
        assert!(url.contains("user%40domain"), "got: {url}");
    }
}

mod store_options_default {
    use super::*;

    #[test]
    fn defaults_are_correct() {
        let opts = StoreOptions::default();
        assert!(opts.file_name.is_none());
        assert!(opts.tier.is_none());
        assert!(opts.wait); // wait defaults to true
        assert!(opts.timeout.is_none());
    }
}

mod pin_params_default {
    use super::*;

    #[test]
    fn defaults_are_none() {
        let p = PinParams::default();
        assert!(p.operation_id.is_none());
        assert!(p.file_name.is_none());
        assert!(p.content_hash.is_none());
        assert!(p.account.is_none());
    }
}

mod upload_status_serde {
    use super::*;

    #[test]
    fn deserialize_all_fields() {
        let json = serde_json::json!({
            "operation_id": "op-123",
            "file_name": "test.bin",
            "status": "completed",
            "finished": true,
            "parts_completed": 5,
            "total_parts": 5,
            "error": null,
            "content_hash": VALID_HASH,
            "deterministic_url": "https://example.com/acct/hash",
            "bytes_total": 1024,
            "bytes_uploaded": 1024,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:01:00Z",
        });
        let status: UploadStatus = serde_json::from_value(json).unwrap();
        assert_eq!(status.operation_id, "op-123");
        assert_eq!(status.file_name, "test.bin");
        assert_eq!(status.status, OperationState::Completed);
        assert!(status.finished);
        assert_eq!(status.parts_completed, 5);
        assert_eq!(status.total_parts, 5);
        assert!(status.error.is_none());
        assert_eq!(status.content_hash.as_deref(), Some(VALID_HASH));
        assert_eq!(status.bytes_total, 1024);
        assert_eq!(status.bytes_uploaded, 1024);
    }

    #[test]
    fn deserialize_minimal_fields_defaults() {
        // Only the required fields present; optional ones use #[serde(default)]
        let json = serde_json::json!({
            "operation_id": "op-456",
            "file_name": "minimal.bin",
            "status": "queued",
        });
        let status: UploadStatus = serde_json::from_value(json).unwrap();
        assert_eq!(status.operation_id, "op-456");
        assert_eq!(status.status, OperationState::Queued);
        assert!(!status.finished);
        assert_eq!(status.parts_completed, 0);
        assert_eq!(status.total_parts, 0);
        assert!(status.error.is_none());
        assert!(status.content_hash.is_none());
        assert!(status.deterministic_url.is_none());
        assert_eq!(status.bytes_total, 0);
        assert_eq!(status.bytes_uploaded, 0);
        assert_eq!(status.created_at, "");
        assert_eq!(status.updated_at, "");
    }

    #[test]
    fn missing_required_field_fails() {
        // Missing operation_id
        let json = serde_json::json!({
            "file_name": "test.bin",
            "status": "completed",
        });
        let result = serde_json::from_value::<UploadStatus>(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_file_name_fails() {
        let json = serde_json::json!({
            "operation_id": "op-1",
            "status": "running",
        });
        let result = serde_json::from_value::<UploadStatus>(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_status_fails() {
        let json = serde_json::json!({
            "operation_id": "op-1",
            "file_name": "test.bin",
        });
        let result = serde_json::from_value::<UploadStatus>(json);
        assert!(result.is_err());
    }
}

mod upload_tier_as_str {
    use super::*;

    #[test]
    fn all_variants() {
        assert_eq!(UploadTier::Normal.as_str(), "normal");
        assert_eq!(UploadTier::Priority.as_str(), "priority");
        assert_eq!(UploadTier::Premium.as_str(), "premium");
        assert_eq!(UploadTier::Ultra.as_str(), "ultra");
        assert_eq!(UploadTier::Enterprise.as_str(), "enterprise");
    }

    #[test]
    fn serde_round_trip() {
        for tier in [
            UploadTier::Normal,
            UploadTier::Priority,
            UploadTier::Premium,
            UploadTier::Ultra,
            UploadTier::Enterprise,
        ] {
            let json = serde_json::to_string(&tier).unwrap();
            let back: UploadTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, back);
        }
    }
}

mod operation_state_serde {
    use super::*;

    #[test]
    fn round_trip_all_variants() {
        for state in [
            OperationState::Queued,
            OperationState::Running,
            OperationState::Durable,
            OperationState::Finalizing,
            OperationState::Completed,
            OperationState::Failed,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let back: OperationState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, back);
        }
    }

    #[test]
    fn snake_case_serialization() {
        assert_eq!(
            serde_json::to_string(&OperationState::Queued).unwrap(),
            "\"queued\""
        );
        assert_eq!(
            serde_json::to_string(&OperationState::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&OperationState::Durable).unwrap(),
            "\"durable\""
        );
        assert_eq!(
            serde_json::to_string(&OperationState::Finalizing).unwrap(),
            "\"finalizing\""
        );
        assert_eq!(
            serde_json::to_string(&OperationState::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&OperationState::Failed).unwrap(),
            "\"failed\""
        );
    }
}

mod store_data_from {
    use super::*;

    #[test]
    fn from_vec_u8() {
        let data: StoreData = vec![1u8, 2, 3].into();
        // StoreData is Debug, just ensure it converts without panic
        let _ = format!("{:?}", data);
    }

    #[test]
    fn from_slice() {
        let bytes: &[u8] = &[4, 5, 6];
        let data: StoreData = bytes.into();
        let _ = format!("{:?}", data);
    }

    #[test]
    fn from_string() {
        let data: StoreData = String::from("hello world").into();
        let _ = format!("{:?}", data);
    }

    #[test]
    fn from_str_ref() {
        let data: StoreData = "hello".into();
        let _ = format!("{:?}", data);
    }
}

mod pipe_error_display {
    use super::*;

    #[test]
    fn missing_api_key_message() {
        let err = PipeError::MissingApiKey("store");
        let msg = format!("{}", err);
        assert!(msg.contains("missing API key"));
        assert!(msg.contains("store"));
        assert!(msg.contains("PIPE_API_KEY"));
    }

    #[test]
    fn invalid_input_message() {
        let err = PipeError::InvalidInput("bad data".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("invalid input"));
        assert!(msg.contains("bad data"));
    }

    #[test]
    fn timeout_message() {
        let err = PipeError::Timeout("operation xyz".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("timeout"));
        assert!(msg.contains("operation xyz"));
    }

    #[test]
    fn http_error_message() {
        let err = PipeError::Http {
            status: 404,
            body: "not found".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("404"));
        assert!(msg.contains("not found"));
    }
}

mod should_refresh {
    use super::*;

    #[test]
    fn returns_false_for_401_without_refresh_token() {
        let c = PipeStorage::new(PipeStorageOptions {
            api_key: Some("key".to_string()),
            base_url: Some("https://example.com".to_string()),
            ..Default::default()
        });
        // The client as constructed has refresh_token = None, so should_refresh
        // returns false even for 401. The positive case (returns true with refresh
        // token) is tested in mock_auth_verify::sets_tokens_on_client.
        let err_401 = PipeError::Http {
            status: 401,
            body: "unauthorized".to_string(),
        };
        assert!(!c.should_refresh(&err_401));
    }

    #[test]
    fn returns_false_for_non_401() {
        let c = client_with_key("https://example.com");
        let err_403 = PipeError::Http {
            status: 403,
            body: "forbidden".to_string(),
        };
        assert!(!c.should_refresh(&err_403));

        let err_500 = PipeError::Http {
            status: 500,
            body: "server error".to_string(),
        };
        assert!(!c.should_refresh(&err_500));
    }

    #[test]
    fn returns_false_for_non_http_errors() {
        let c = client_with_key("https://example.com");
        let err = PipeError::Timeout("timed out".to_string());
        assert!(!c.should_refresh(&err));

        let err2 = PipeError::InvalidInput("bad".to_string());
        assert!(!c.should_refresh(&err2));

        let err3 = PipeError::MissingApiKey("test");
        assert!(!c.should_refresh(&err3));
    }
}

mod auth_session_serde {
    use super::*;

    #[test]
    fn deserialize_with_csrf() {
        let json = serde_json::json!({
            "access_token": "at-123",
            "refresh_token": "rt-456",
            "csrf_token": "csrf-789",
        });
        let session: AuthSession = serde_json::from_value(json).unwrap();
        assert_eq!(session.access_token, "at-123");
        assert_eq!(session.refresh_token, "rt-456");
        assert_eq!(session.csrf_token.as_deref(), Some("csrf-789"));
    }

    #[test]
    fn deserialize_without_csrf() {
        let json = serde_json::json!({
            "access_token": "at-123",
            "refresh_token": "rt-456",
        });
        let session: AuthSession = serde_json::from_value(json).unwrap();
        assert!(session.csrf_token.is_none());
    }

    #[test]
    fn missing_access_token_fails() {
        let json = serde_json::json!({
            "refresh_token": "rt-456",
        });
        let result = serde_json::from_value::<AuthSession>(json);
        assert!(result.is_err());
    }
}

mod challenge_response_serde {
    use super::*;

    #[test]
    fn deserialize_valid() {
        let json = serde_json::json!({
            "nonce": "abc123",
            "message": "Sign this message",
        });
        let cr: ChallengeResponse = serde_json::from_value(json).unwrap();
        assert_eq!(cr.nonce, "abc123");
        assert_eq!(cr.message, "Sign this message");
    }

    #[test]
    fn missing_nonce_fails() {
        let json = serde_json::json!({
            "message": "Sign this",
        });
        assert!(serde_json::from_value::<ChallengeResponse>(json).is_err());
    }

    #[test]
    fn missing_message_fails() {
        let json = serde_json::json!({
            "nonce": "abc",
        });
        assert!(serde_json::from_value::<ChallengeResponse>(json).is_err());
    }
}

mod delete_response_serde {
    use super::*;

    #[test]
    fn deserialize_valid() {
        let json = serde_json::json!({
            "message": "File deleted successfully",
        });
        let dr: DeleteResponse = serde_json::from_value(json).unwrap();
        assert_eq!(dr.message, "File deleted successfully");
    }

    #[test]
    fn missing_message_fails() {
        let json = serde_json::json!({});
        assert!(serde_json::from_value::<DeleteResponse>(json).is_err());
    }
}

// ===========================================================================
//  2. Auth state tests (no HTTP)
// ===========================================================================

mod auth_state {
    use super::*;

    #[tokio::test]
    async fn store_without_api_key_returns_missing_api_key() {
        let c = client_no_key();
        let result = c.store("data", StoreOptions::default()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::MissingApiKey(action) => assert_eq!(action, "store"),
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn check_status_without_api_key_returns_missing_api_key() {
        let c = client_no_key();
        let result = c.check_status(Some("op-1"), None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::MissingApiKey(action) => assert_eq!(action, "check_status"),
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn delete_without_api_key_returns_missing_api_key() {
        let c = client_no_key();
        let result = c.delete("some-file.bin").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::MissingApiKey(action) => assert_eq!(action, "delete"),
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn delete_file_name_without_api_key_returns_missing_api_key() {
        let c = client_no_key();
        let result = c.delete_file_name("file.bin").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::MissingApiKey(action) => assert_eq!(action, "delete"),
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }
    }

    #[test]
    fn new_client_has_no_refresh_token() {
        let c = client_with_key("https://example.com");
        // refresh_token is private, so test via should_refresh: a 401 error should
        // return false because there is no refresh token.
        let err = PipeError::Http {
            status: 401,
            body: "unauthorized".to_string(),
        };
        assert!(!c.should_refresh(&err));
    }

    #[test]
    fn from_env_returns_a_client() {
        // We cannot safely call set_var in tests (not thread-safe and deprecated).
        // Instead, verify from_env() produces a valid client that behaves the same
        // as PipeStorage::new() — the env-reading logic is exercised via
        // the constructor. Without PIPE_API_KEY set, the client should lack an API key.
        let c = PipeStorage::from_env();
        // Without PIPE_API_KEY set, deterministic_url still works for the URL
        // construction part (it doesn't need an API key).
        // Without PIPE_ACCOUNT, deterministic_url should fail.
        let result = c.deterministic_url(VALID_HASH, Some("explicit-account"));
        // Should succeed when account is passed explicitly, regardless of env.
        assert!(result.is_ok());
    }
}

mod check_status_validation {
    use super::*;

    #[tokio::test]
    async fn requires_operation_id_or_file_name() {
        let c = client_with_key("https://example.com");
        let result = c.check_status(None, None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::InvalidInput(msg) => {
                assert!(
                    msg.contains("operation_id") || msg.contains("file_name"),
                    "got: {msg}"
                );
            }
            other => panic!("expected InvalidInput, got: {:?}", other),
        }
    }
}

mod pin_validation {
    use super::*;

    #[tokio::test]
    async fn pin_http_url_returns_immediately() {
        let c = client_no_key(); // No key needed for URL pin
        let result = c.pin("https://example.com/file.bin").await.unwrap();
        assert_eq!(result.url, "https://example.com/file.bin");
        assert_eq!(result.status, Some(OperationState::Completed));
        assert!(result.content_hash.is_none());
    }

    #[tokio::test]
    async fn pin_hex_hash_builds_deterministic_url() {
        let c = client_with_key("https://example.com");
        let result = c.pin(VALID_HASH).await.unwrap();
        assert!(result.url.contains(VALID_HASH));
        assert_eq!(result.content_hash.as_deref(), Some(VALID_HASH));
        assert_eq!(result.status, Some(OperationState::Completed));
    }

    #[tokio::test]
    async fn pin_hex_hash_without_account_fails() {
        let c = client_no_account();
        let result = c.pin(VALID_HASH).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pin_with_content_hash_returns_immediately() {
        let c = client_with_key("https://example.com");
        let result = c
            .pin_with(PinParams {
                content_hash: Some(VALID_HASH.to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(result.url.contains(VALID_HASH));
        assert_eq!(result.content_hash.as_deref(), Some(VALID_HASH));
    }

    #[tokio::test]
    async fn pin_with_no_params_fails() {
        let c = client_with_key("https://example.com");
        let result = c.pin_with(PinParams::default()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::InvalidInput(msg) => {
                assert!(msg.contains("pin requires"), "got: {msg}");
            }
            other => panic!("expected InvalidInput, got: {:?}", other),
        }
    }
}

mod fetch_validation {
    use super::*;

    #[tokio::test]
    async fn fetch_pipe_url_without_key_returns_missing_api_key() {
        // A URL that starts with the client's base_url but is NOT a public deterministic URL
        let c = PipeStorage::new(PipeStorageOptions {
            api_key: None,
            base_url: Some("https://api.example.com".to_string()),
            account: None,
            ..Default::default()
        });
        let result = c
            .fetch("https://api.example.com/download-stream?file_name=test.bin")
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::MissingApiKey(action) => assert_eq!(action, "fetch"),
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }
    }
}

mod resolve_fetch_url_behavior {
    use super::*;

    // We can't call resolve_fetch_url directly (private), but we can test its
    // behavior through fetch's error messages or by observing what URL gets hit
    // via the mock server. Here we test the indirectly observable behavior.

    #[tokio::test]
    async fn fetch_with_full_url_uses_url_directly() {
        // When given an HTTP URL, fetch should use it as-is. We verify by
        // checking that a non-existent external URL gives a reqwest error
        // (not MissingApiKey or InvalidInput).
        let c = client_with_key("https://example.com");
        let result = c.fetch("http://127.0.0.1:1/nonexistent").await;
        assert!(result.is_err());
        // Should be a reqwest connection error, not a PipeError::InvalidInput
        match result.unwrap_err() {
            PipeError::Reqwest(_) => {} // expected
            other => panic!("expected Reqwest error, got: {:?}", other),
        }
    }
}

// ===========================================================================
//  3. Mock HTTP server tests
// ===========================================================================

/// Start a mock HTTP server that handles one connection and returns a canned
/// response based on the request path. Returns the base URL.
///
/// The handler closure receives (method, path, body) and must return
/// (status_code, headers_vec, response_body).
async fn start_mock_server<F>(handler: F) -> String
where
    F: Fn(&str, &str, &str) -> (u16, Vec<(&'static str, String)>, String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{}", port);

    tokio::spawn(async move {
        // Accept connections in a loop so the server can handle multiple requests
        // from the same test (e.g., auth_verify which only needs one).
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };

            let mut buf = vec![0u8; 16384];
            let n = match stream.read(&mut buf).await {
                Ok(n) => n,
                Err(_) => break,
            };
            let request = String::from_utf8_lossy(&buf[..n]).to_string();

            // Parse method and path from first line
            let first_line = request.lines().next().unwrap_or("");
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            let method = parts.first().copied().unwrap_or("GET");
            let path = parts.get(1).copied().unwrap_or("/");

            // Extract body (after blank line)
            let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_string();

            let (status, extra_headers, response_body) = handler(method, path, &body);

            let status_text = match status {
                200 => "OK",
                201 => "Created",
                400 => "Bad Request",
                401 => "Unauthorized",
                403 => "Forbidden",
                404 => "Not Found",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                _ => "Unknown",
            };

            let mut headers = format!(
                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n",
                status,
                status_text,
                response_body.len()
            );

            for (name, value) in &extra_headers {
                headers.push_str(&format!("{}: {}\r\n", name, value));
            }
            headers.push_str("\r\n");

            let _ = stream.write_all(headers.as_bytes()).await;
            let _ = stream.write_all(response_body.as_bytes()).await;
            let _ = stream.flush().await;
        }
    });

    base_url
}

mod mock_store {
    use super::*;

    #[tokio::test]
    async fn store_wait_false_returns_queued() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/upload") {
                (
                    200,
                    vec![("x-operation-id", "op-test-123".to_string())],
                    "{}".to_string(),
                )
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let result = c
            .store(
                "hello world",
                StoreOptions {
                    wait: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(result.operation_id.as_deref(), Some("op-test-123"));
        assert_eq!(result.status, OperationState::Queued);
    }

    #[tokio::test]
    async fn store_wait_false_without_operation_id_returns_completed() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/upload") {
                (200, vec![], "{}".to_string())
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let result = c
            .store(
                "hello world",
                StoreOptions {
                    wait: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert!(result.operation_id.is_none());
        assert_eq!(result.status, OperationState::Completed);
    }

    #[tokio::test]
    async fn store_with_priority_tier_uses_priority_upload() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/priorityUpload") {
                (
                    200,
                    vec![("x-operation-id", "op-priority".to_string())],
                    "{}".to_string(),
                )
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let result = c
            .store(
                "data",
                StoreOptions {
                    wait: false,
                    tier: Some(UploadTier::Priority),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(result.operation_id.as_deref(), Some("op-priority"));
    }

    #[tokio::test]
    async fn split_mode_falls_back_from_v1_upload_to_upload() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/v1/upload") {
                (404, vec![], "missing /v1/upload".to_string())
            } else if path.starts_with("/upload") {
                (
                    200,
                    vec![("x-operation-id", "op-fallback".to_string())],
                    "{}".to_string(),
                )
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url)
            .with_split_base_urls(Some(base_url.clone()), Some(base_url.clone()));
        let result = c
            .store(
                "data",
                StoreOptions {
                    wait: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(result.operation_id.as_deref(), Some("op-fallback"));
    }
}

mod mock_check_status {
    use super::*;

    #[tokio::test]
    async fn returns_parsed_upload_status() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/checkUploadStatus") {
                let json = serde_json::json!({
                    "operation_id": "op-42",
                    "file_name": "test.bin",
                    "status": "completed",
                    "finished": true,
                    "content_hash": VALID_HASH,
                    "deterministic_url": format!("https://example.com/acct/{}", VALID_HASH),
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let status = c.check_status(Some("op-42"), None).await.unwrap();
        assert_eq!(status.operation_id, "op-42");
        assert_eq!(status.file_name, "test.bin");
        assert_eq!(status.status, OperationState::Completed);
        assert!(status.finished);
        assert_eq!(status.content_hash.as_deref(), Some(VALID_HASH));
    }

    #[tokio::test]
    async fn accepts_durable_and_finalizing_status_values() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/checkUploadStatus") {
                let json = serde_json::json!({
                    "operation_id": "op-42",
                    "file_name": "test.bin",
                    "status": "durable",
                    "finished": false,
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let status = c.check_status(Some("op-42"), None).await.unwrap();
        assert_eq!(status.status, OperationState::Durable);
    }
}

mod mock_auth_challenge {
    use super::*;

    #[tokio::test]
    async fn returns_nonce_and_message() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/auth/siws/challenge") {
                let json = serde_json::json!({
                    "nonce": "random-nonce-123",
                    "message": "Sign this to authenticate",
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let challenge = c.auth_challenge("wallet-pubkey-abc").await.unwrap();
        assert_eq!(challenge.nonce, "random-nonce-123");
        assert_eq!(challenge.message, "Sign this to authenticate");
    }
}

mod mock_auth_verify {
    use super::*;

    #[tokio::test]
    async fn sets_tokens_on_client() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/auth/siws/verify") {
                let json = serde_json::json!({
                    "access_token": "new-access-token",
                    "refresh_token": "new-refresh-token",
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let mut c = client_with_key(&base_url);

        // Before verify: no refresh token
        let err_401 = PipeError::Http {
            status: 401,
            body: "unauthorized".to_string(),
        };
        assert!(!c.should_refresh(&err_401));

        let session = c
            .auth_verify("wallet", "nonce", "message", "sig")
            .await
            .unwrap();
        assert_eq!(session.access_token, "new-access-token");
        assert_eq!(session.refresh_token, "new-refresh-token");

        // After verify: refresh token is set, so should_refresh returns true for 401
        assert!(c.should_refresh(&err_401));
    }
}

mod mock_fetch {
    use super::*;

    #[tokio::test]
    async fn returns_response_bytes() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/download-stream") {
                (200, vec![], "file-content-bytes-here".to_string())
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let bytes = c.fetch("my-file.bin").await.unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "file-content-bytes-here");
    }

    #[tokio::test]
    async fn fetch_text_returns_string() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/download-stream") {
                (200, vec![], "hello text".to_string())
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let text = c.fetch_text("my-file.txt").await.unwrap();
        assert_eq!(text, "hello text");
    }
}

mod mock_delete {
    use super::*;

    #[tokio::test]
    async fn delete_file_name_returns_message() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/deleteFile") {
                let json = serde_json::json!({
                    "message": "File deleted successfully",
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let c = client_with_key(&base_url);
        let resp = c.delete_file_name("test-file.bin").await.unwrap();
        assert_eq!(resp.message, "File deleted successfully");
    }
}

mod mock_http_errors {
    use super::*;

    #[tokio::test]
    async fn non_2xx_returns_pipe_error_http() {
        let base_url =
            start_mock_server(|_method, _path, _body| (403, vec![], "access denied".to_string()))
                .await;

        let c = client_with_key(&base_url);
        let result = c.check_status(Some("op-1"), None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::Http { status, body } => {
                assert_eq!(status, 403);
                assert_eq!(body, "access denied");
            }
            other => panic!("expected Http error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn server_401_error() {
        let base_url =
            start_mock_server(|_method, _path, _body| (401, vec![], "token expired".to_string()))
                .await;

        let c = client_with_key(&base_url);
        let result = c.check_status(Some("op-1"), None).await;
        match result.unwrap_err() {
            PipeError::Http { status, body } => {
                assert_eq!(status, 401);
                assert_eq!(body, "token expired");
            }
            other => panic!("expected Http 401 error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn server_500_error() {
        let base_url =
            start_mock_server(|_method, _path, _body| (500, vec![], "internal error".to_string()))
                .await;

        let c = client_with_key(&base_url);
        let result = c.delete_file_name("file.bin").await;
        match result.unwrap_err() {
            PipeError::Http { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "internal error");
            }
            other => panic!("expected Http 500 error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn store_non_2xx_returns_error() {
        let base_url =
            start_mock_server(|_method, _path, _body| (400, vec![], "bad request".to_string()))
                .await;

        let c = client_with_key(&base_url);
        let result = c
            .store(
                "data",
                StoreOptions {
                    wait: false,
                    ..Default::default()
                },
            )
            .await;
        match result.unwrap_err() {
            PipeError::Http { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body, "bad request");
            }
            other => panic!("expected Http error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn auth_challenge_non_2xx() {
        let base_url =
            start_mock_server(|_method, _path, _body| (429, vec![], "rate limited".to_string()))
                .await;

        let c = client_with_key(&base_url);
        let result = c.auth_challenge("wallet").await;
        match result.unwrap_err() {
            PipeError::Http { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(body, "rate limited");
            }
            other => panic!("expected Http error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn fetch_non_2xx() {
        let base_url =
            start_mock_server(|_method, _path, _body| (404, vec![], "not found".to_string())).await;

        let c = client_with_key(&base_url);
        let result = c.fetch("missing-file.bin").await;
        match result.unwrap_err() {
            PipeError::Http { status, body } => {
                assert_eq!(status, 404);
                assert_eq!(body, "not found");
            }
            other => panic!("expected Http error, got: {:?}", other),
        }
    }
}

mod mock_auth_refresh {
    use super::*;

    #[tokio::test]
    async fn refresh_without_token_fails() {
        let mut c = client_with_key("https://example.com");
        let result = c.auth_refresh().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipeError::InvalidInput(msg) => {
                assert!(msg.contains("refresh token"), "got: {msg}");
            }
            other => panic!("expected InvalidInput, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn refresh_updates_tokens() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/auth/siws/verify") {
                let json = serde_json::json!({
                    "access_token": "access-1",
                    "refresh_token": "refresh-1",
                })
                .to_string();
                (200, vec![], json)
            } else if path.starts_with("/auth/refresh") {
                let json = serde_json::json!({
                    "access_token": "access-2",
                    "refresh_token": "refresh-2",
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let mut c = client_with_key(&base_url);

        // First, establish a refresh token via auth_verify
        c.auth_verify("wallet", "nonce", "msg", "sig")
            .await
            .unwrap();

        // Now refresh
        let session = c.auth_refresh().await.unwrap();
        assert_eq!(session.access_token, "access-2");
        assert_eq!(session.refresh_token, "refresh-2");

        // should_refresh should still work (new refresh token is set)
        let err_401 = PipeError::Http {
            status: 401,
            body: "expired".to_string(),
        };
        assert!(c.should_refresh(&err_401));
    }

    #[tokio::test]
    async fn refresh_without_refresh_token_in_response_keeps_previous_token() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/auth/siws/verify") {
                let json = serde_json::json!({
                    "access_token": "access-1",
                    "refresh_token": "refresh-1",
                })
                .to_string();
                (200, vec![], json)
            } else if path.starts_with("/auth/refresh") {
                let json = serde_json::json!({
                    "access_token": "access-2",
                })
                .to_string();
                (200, vec![], json)
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let mut c = client_with_key(&base_url);
        c.auth_verify("wallet", "nonce", "msg", "sig")
            .await
            .unwrap();

        let session = c.auth_refresh().await.unwrap();
        assert_eq!(session.access_token, "access-2");
        assert_eq!(session.refresh_token, "refresh-1");
        assert!(c.should_refresh(&PipeError::Http {
            status: 401,
            body: "expired".to_string(),
        }));
    }
}

mod mock_auth_logout {
    use super::*;

    #[tokio::test]
    async fn logout_clears_tokens() {
        let base_url = start_mock_server(|_method, path, _body| {
            if path.starts_with("/auth/siws/verify") {
                let json = serde_json::json!({
                    "access_token": "access-1",
                    "refresh_token": "refresh-1",
                })
                .to_string();
                (200, vec![], json)
            } else if path.starts_with("/auth/logout") {
                (200, vec![], "{}".to_string())
            } else {
                (404, vec![], "not found".to_string())
            }
        })
        .await;

        let mut c = client_with_key(&base_url);

        // Establish tokens
        c.auth_verify("wallet", "nonce", "msg", "sig")
            .await
            .unwrap();
        let err_401 = PipeError::Http {
            status: 401,
            body: "expired".to_string(),
        };
        assert!(c.should_refresh(&err_401));

        // Logout
        c.auth_logout().await.unwrap();

        // After logout: no refresh token, and store should fail (no api key)
        assert!(!c.should_refresh(&err_401));
        let result = c
            .store(
                "data",
                StoreOptions {
                    wait: false,
                    ..Default::default()
                },
            )
            .await;
        assert!(matches!(result.unwrap_err(), PipeError::MissingApiKey(_)));
    }
}

mod pipe_storage_options_default {
    use super::*;

    #[test]
    fn defaults() {
        let opts = PipeStorageOptions::default();
        assert!(opts.api_key.is_none());
        assert!(opts.base_url.is_none());
        assert!(opts.account.is_none());
        assert_eq!(opts.timeout, Some(Duration::from_secs(120)));
        assert_eq!(opts.poll_interval, Some(Duration::from_secs(1)));
    }
}
