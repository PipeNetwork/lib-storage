use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;

const DEFAULT_BASE_URL: &str = "https://us-west-01-firestarter.pipenetwork.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_RESPONSE_BYTES: usize = 256 * 1024 * 1024; // 256 MB
const SDK_USER_AGENT: &str = "pipe-agent-storage-rust/0.1.0";

pub type Result<T> = std::result::Result<T, PipeError>;

#[derive(Debug, Error)]
pub enum PipeError {
    #[error("missing API key for {0}. Set PIPE_API_KEY or pass api_key")]
    MissingApiKey(&'static str),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("pipe API request failed ({status}): {body}")]
    Http { status: u16, body: String },

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UploadTier {
    Normal,
    Priority,
    Premium,
    Ultra,
    Enterprise,
}

impl UploadTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Priority => "priority",
            Self::Premium => "premium",
            Self::Ultra => "ultra",
            Self::Enterprise => "enterprise",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Queued,
    Running,
    Durable,
    Finalizing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStatus {
    pub operation_id: String,
    pub file_name: String,
    pub status: OperationState,
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub parts_completed: u64,
    #[serde(default)]
    pub total_parts: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub deterministic_url: Option<String>,
    #[serde(default)]
    pub bytes_total: u64,
    #[serde(default)]
    pub bytes_uploaded: u64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoreOptions {
    pub file_name: Option<String>,
    pub tier: Option<UploadTier>,
    pub wait: bool,
    pub timeout: Option<Duration>,
}

impl Default for StoreOptions {
    fn default() -> Self {
        Self {
            file_name: None,
            tier: None,
            wait: true,
            timeout: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreResult {
    pub operation_id: Option<String>,
    pub location: Option<String>,
    pub file_name: String,
    pub status: OperationState,
    pub content_hash: Option<String>,
    pub deterministic_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PinParams {
    pub operation_id: Option<String>,
    pub file_name: Option<String>,
    pub content_hash: Option<String>,
    pub account: Option<String>,
}

impl Default for PinParams {
    fn default() -> Self {
        Self {
            operation_id: None,
            file_name: None,
            content_hash: None,
            account: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinResult {
    pub url: String,
    pub content_hash: Option<String>,
    pub operation_id: Option<String>,
    pub file_name: Option<String>,
    pub status: Option<OperationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub nonce: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub csrf_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PipeStorageOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub account: Option<String>,
    pub timeout: Option<Duration>,
    pub poll_interval: Option<Duration>,
}

impl Default for PipeStorageOptions {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            account: None,
            timeout: Some(DEFAULT_TIMEOUT),
            poll_interval: Some(DEFAULT_POLL_INTERVAL),
        }
    }
}

#[derive(Clone)]
pub struct PipeStorage {
    api_key: Option<String>,
    refresh_token: Option<String>,
    control_base_url: String,
    data_base_url: String,
    use_pop_gateway: bool,
    account: Option<String>,
    timeout: Duration,
    poll_interval: Duration,
    http: Client,
}

impl PipeStorage {
    pub fn from_env() -> Self {
        let mut client = Self::new(PipeStorageOptions {
            api_key: env_var("PIPE_API_KEY"),
            base_url: env_var("PIPE_BASE_URL").or_else(|| env_var("PIPE_API_BASE_URL")),
            account: env_var("PIPE_ACCOUNT"),
            timeout: Some(DEFAULT_TIMEOUT),
            poll_interval: Some(DEFAULT_POLL_INTERVAL),
        });
        client.apply_split_base_urls(
            env_var("PIPE_CONTROL_BASE_URL"),
            env_var("PIPE_DATA_BASE_URL"),
        );
        client
    }

    pub fn new(options: PipeStorageOptions) -> Self {
        let base_url = normalize_base_url(options.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL));
        let timeout = options.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let poll_interval = options.poll_interval.unwrap_or(DEFAULT_POLL_INTERVAL);

        let http = Client::builder()
            .timeout(timeout)
            .user_agent(SDK_USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            api_key: options.api_key,
            refresh_token: None,
            control_base_url: base_url.clone(),
            data_base_url: base_url,
            use_pop_gateway: false,
            account: options.account,
            timeout,
            poll_interval,
            http,
        }
    }

    pub fn with_split_base_urls(
        mut self,
        control_base_url: Option<String>,
        data_base_url: Option<String>,
    ) -> Self {
        self.apply_split_base_urls(control_base_url, data_base_url);
        self
    }

    pub fn set_split_base_urls(
        &mut self,
        control_base_url: Option<String>,
        data_base_url: Option<String>,
    ) {
        self.apply_split_base_urls(control_base_url, data_base_url);
    }

    fn apply_split_base_urls(
        &mut self,
        control_base_url: Option<String>,
        data_base_url: Option<String>,
    ) {
        let control_base_url = control_base_url
            .as_deref()
            .map(normalize_base_url)
            .filter(|value| !value.is_empty());
        let data_base_url = data_base_url
            .as_deref()
            .map(normalize_base_url)
            .filter(|value| !value.is_empty());

        if control_base_url.is_none() && data_base_url.is_none() {
            return;
        }

        if let Some(url) = control_base_url {
            self.control_base_url = url;
        }
        if let Some(url) = data_base_url {
            self.data_base_url = url;
        }
        self.use_pop_gateway = true;
    }

    pub async fn auth_challenge(&self, wallet_public_key: &str) -> Result<ChallengeResponse> {
        let url = format!("{}/auth/siws/challenge", self.control_base_url);
        let response = self
            .http
            .request(Method::POST, &url)
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({ "wallet_public_key": wallet_public_key }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        parse_json_response(response).await
    }

    pub async fn auth_verify(
        &mut self,
        wallet_public_key: &str,
        nonce: &str,
        message: &str,
        signature_b64: &str,
    ) -> Result<AuthSession> {
        let url = format!("{}/auth/siws/verify", self.control_base_url);
        let response = self
            .http
            .request(Method::POST, &url)
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "wallet_public_key": wallet_public_key,
                "nonce": nonce,
                "message": message,
                "signature_b64": signature_b64,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        let session: AuthSession = parse_json_response(response).await?;
        self.api_key = Some(session.access_token.clone());
        self.refresh_token = Some(session.refresh_token.clone());
        Ok(session)
    }

    pub async fn auth_refresh(&mut self) -> Result<AuthSession> {
        let token = self.refresh_token.clone().ok_or_else(|| {
            PipeError::InvalidInput("no refresh token available — call auth_verify first".into())
        })?;

        let url = format!("{}/auth/refresh", self.control_base_url);
        let response = self
            .http
            .request(Method::POST, &url)
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({ "refresh_token": token }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        let payload: Value = parse_json_response(response).await?;
        let access_token = payload
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PipeError::InvalidInput("auth_refresh response missing access_token".to_string())
            })?
            .to_string();
        let refresh_token = payload
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| self.refresh_token.clone())
            .ok_or_else(|| {
                PipeError::InvalidInput("auth_refresh response missing refresh_token".to_string())
            })?;
        let session = AuthSession {
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            csrf_token: payload
                .get("csrf_token")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        self.api_key = Some(access_token);
        self.refresh_token = Some(refresh_token);
        Ok(session)
    }

    pub async fn auth_logout(&mut self) -> Result<()> {
        self.require_api_key("auth_logout")?;

        let url = format!("{}/auth/logout", self.control_base_url);
        let response = self
            .http
            .request(Method::POST, &url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.api_key.as_deref().unwrap_or_default()),
            )
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        self.api_key = None;
        self.refresh_token = None;
        Ok(())
    }

    /// Returns `true` if a refresh token is available and the error looks like an
    /// expired access token (HTTP 401). Call `auth_refresh()` after this returns
    /// `true`, then retry the failed operation.
    ///
    /// ```no_run
    /// # async fn example(client: &mut pipe_agent_storage::PipeStorage) -> pipe_agent_storage::Result<()> {
    /// match client.store("data", Default::default()).await {
    ///     Err(ref e) if client.should_refresh(e) => {
    ///         client.auth_refresh().await?;
    ///         client.store("data", Default::default()).await?;
    ///     }
    ///     other => { other?; }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn should_refresh(&self, err: &PipeError) -> bool {
        matches!(err, PipeError::Http { status: 401, .. }) && self.refresh_token.is_some()
    }

    pub fn deterministic_url(&self, content_hash: &str, account: Option<&str>) -> Result<String> {
        let effective_account = account
            .map(|s| s.to_string())
            .or_else(|| self.account.clone())
            .ok_or_else(|| {
                PipeError::InvalidInput(
                    "missing account for deterministic URL (set PIPE_ACCOUNT or pass account)"
                        .to_string(),
                )
            })?;

        if !is_hex_hash(content_hash) {
            return Err(PipeError::InvalidInput(
                "content_hash must be a 64-character hex string".to_string(),
            ));
        }

        Ok(format!(
            "{}/{}/{}",
            self.data_base_url,
            urlencoding::encode(&effective_account),
            content_hash.to_lowercase()
        ))
    }

    pub async fn store<D: Into<StoreData>>(
        &self,
        data: D,
        mut options: StoreOptions,
    ) -> Result<StoreResult> {
        self.require_api_key("store")?;

        let data = data.into();
        let file_name = options
            .file_name
            .take()
            .unwrap_or_else(|| format!("agent/{}-{}.bin", now_ms(), Uuid::new_v4()));

        let tier = options.tier.unwrap_or(UploadTier::Normal);
        let endpoint = if self.use_pop_gateway {
            format!("{}/v1/upload", self.data_base_url)
        } else if tier == UploadTier::Priority {
            format!("{}/priorityUpload", self.control_base_url)
        } else {
            format!("{}/upload", self.control_base_url)
        };

        let mut query = format!("file_name={}", urlencoding::encode(&file_name));
        if options.tier.is_some() {
            query.push_str(&format!("&tier={}", tier.as_str()));
        }

        let url = format!("{}?{}", endpoint, query);
        let mut response = self
            .http
            .request(Method::POST, &url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.api_key.as_deref().unwrap_or_default()),
            )
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data.bytes.clone())
            .send()
            .await?;

        if self.use_pop_gateway
            && !response.status().is_success()
            && matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            )
        {
            let fallback_endpoint = if tier == UploadTier::Priority {
                format!("{}/priorityUpload", self.control_base_url)
            } else {
                format!("{}/upload", self.control_base_url)
            };
            let fallback_url = format!("{}?{}", fallback_endpoint, query);
            response = self
                .http
                .request(Method::POST, &fallback_url)
                .header(
                    AUTHORIZATION,
                    format!("Bearer {}", self.api_key.as_deref().unwrap_or_default()),
                )
                .header(CONTENT_TYPE, "application/octet-stream")
                .body(data.bytes)
                .send()
                .await?;
        }

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        let headers = response.headers().clone();
        let operation_id = header_value(&headers, "x-operation-id");
        let location = header_value(&headers, "location");

        if !options.wait || operation_id.is_none() {
            let status = if operation_id.is_some() {
                OperationState::Queued
            } else {
                OperationState::Completed
            };
            return Ok(StoreResult {
                operation_id,
                location,
                file_name,
                status,
                content_hash: None,
                deterministic_url: None,
            });
        }

        let final_status = self
            .wait_for_operation(operation_id.as_deref().unwrap_or_default(), options.timeout)
            .await?;

        Ok(StoreResult {
            operation_id: Some(final_status.operation_id.clone()),
            location,
            file_name: final_status.file_name.clone(),
            status: final_status.status,
            content_hash: final_status.content_hash.clone(),
            deterministic_url: final_status.deterministic_url.clone(),
        })
    }

    pub async fn store_json<T: Serialize>(
        &self,
        value: &T,
        options: StoreOptions,
    ) -> Result<StoreResult> {
        let bytes = serde_json::to_vec(value)?;
        self.store(bytes, options).await
    }

    pub async fn check_status(
        &self,
        operation_id: Option<&str>,
        file_name: Option<&str>,
    ) -> Result<UploadStatus> {
        self.require_api_key("check_status")?;

        if operation_id.is_none() && file_name.is_none() {
            return Err(PipeError::InvalidInput(
                "check_status requires operation_id or file_name".to_string(),
            ));
        }

        let mut query_parts: Vec<String> = Vec::new();
        if let Some(op) = operation_id {
            query_parts.push(format!("operation_id={}", urlencoding::encode(op)));
        }
        if let Some(file) = file_name {
            query_parts.push(format!("file_name={}", urlencoding::encode(file)));
        }

        let url = format!(
            "{}/{}?{}",
            self.control_base_url,
            if self.use_pop_gateway {
                "pop/v1/checkUploadStatus"
            } else {
                "checkUploadStatus"
            },
            query_parts.join("&")
        );

        let response = self
            .http
            .request(Method::GET, &url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.api_key.as_deref().unwrap_or_default()),
            )
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        parse_json_response(response).await
    }

    pub async fn wait_for_operation(
        &self,
        operation_id: &str,
        timeout: Option<Duration>,
    ) -> Result<UploadStatus> {
        self.require_api_key("wait_for_operation")?;

        let timeout = timeout.unwrap_or(self.timeout);
        let started = Instant::now();
        let mut consecutive_errors: u32 = 0;
        const MAX_TRANSIENT_ERRORS: u32 = 3;

        while started.elapsed() < timeout {
            let status = match self.check_status(Some(operation_id), None).await {
                Ok(s) => {
                    consecutive_errors = 0;
                    s
                }
                Err(PipeError::Http { status, .. }) if status < 500 => {
                    return Err(PipeError::Http {
                        status,
                        body: format!("non-retryable error polling operation {}", operation_id),
                    });
                }
                Err(e) => {
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_TRANSIENT_ERRORS {
                        return Err(e);
                    }
                    sleep(self.poll_interval).await;
                    continue;
                }
            };
            match status.status {
                OperationState::Completed => return Ok(status),
                OperationState::Failed => {
                    return Err(PipeError::Http {
                        status: 409,
                        body: status.error.unwrap_or_else(|| "upload failed".to_string()),
                    })
                }
                OperationState::Queued
                | OperationState::Running
                | OperationState::Durable
                | OperationState::Finalizing => {
                    sleep(self.poll_interval).await;
                }
            }
        }

        Err(PipeError::Timeout(format!(
            "timed out waiting for operation {}",
            operation_id
        )))
    }

    pub async fn pin(&self, key: &str) -> Result<PinResult> {
        if is_http_url(key) {
            return Ok(PinResult {
                url: key.to_string(),
                content_hash: None,
                operation_id: None,
                file_name: None,
                status: Some(OperationState::Completed),
            });
        }

        if is_hex_hash(key) {
            return Ok(PinResult {
                url: self.deterministic_url(key, None)?,
                content_hash: Some(key.to_lowercase()),
                operation_id: None,
                file_name: None,
                status: Some(OperationState::Completed),
            });
        }

        if is_uuid(key) {
            return self
                .pin_with(PinParams {
                    operation_id: Some(key.to_string()),
                    ..PinParams::default()
                })
                .await;
        }

        self.pin_with(PinParams {
            file_name: Some(key.to_string()),
            ..PinParams::default()
        })
        .await
    }

    pub async fn pin_with(&self, params: PinParams) -> Result<PinResult> {
        if let Some(hash) = params.content_hash.as_deref() {
            return Ok(PinResult {
                url: self.deterministic_url(hash, params.account.as_deref())?,
                content_hash: Some(hash.to_lowercase()),
                operation_id: params.operation_id,
                file_name: params.file_name,
                status: Some(OperationState::Completed),
            });
        }

        if params.operation_id.is_none() && params.file_name.is_none() {
            return Err(PipeError::InvalidInput(
                "pin requires operation_id, file_name, content_hash, or URL/hash key".to_string(),
            ));
        }

        let status = self
            .check_status(params.operation_id.as_deref(), params.file_name.as_deref())
            .await?;

        if status.status != OperationState::Completed {
            return Err(PipeError::InvalidInput(format!(
                "cannot pin while status is {:?}",
                status.status
            )));
        }

        let url = status
            .deterministic_url
            .clone()
            .or_else(|| {
                status
                    .content_hash
                    .as_deref()
                    .and_then(|h| self.deterministic_url(h, params.account.as_deref()).ok())
            })
            .ok_or_else(|| {
                PipeError::InvalidInput(
                    "upload completed but deterministic_url/content_hash is missing".to_string(),
                )
            })?;

        Ok(PinResult {
            url,
            content_hash: status.content_hash,
            operation_id: Some(status.operation_id),
            file_name: Some(status.file_name),
            status: Some(status.status),
        })
    }

    pub async fn fetch(&self, key: &str) -> Result<Vec<u8>> {
        let url = self.resolve_fetch_url(key)?;
        let is_pipe_url = url.starts_with(&format!("{}/", self.control_base_url))
            || url.starts_with(&format!("{}/", self.data_base_url));
        let is_public_det = self.is_public_deterministic_url(&url);

        if self.api_key.is_none() && is_pipe_url && !is_public_det {
            return Err(PipeError::MissingApiKey("fetch"));
        }

        let mut req = self.http.request(Method::GET, &url);
        if let Some(api_key) = self.api_key.as_deref() {
            req = req.header(AUTHORIZATION, format!("Bearer {}", api_key));
        }

        let mut response = req.send().await?;
        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        let mut out: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            let next_len = out.len().checked_add(chunk.len()).ok_or(PipeError::Http {
                status: 502,
                body: "response too large".to_string(),
            })?;
            if next_len > MAX_RESPONSE_BYTES {
                return Err(PipeError::Http {
                    status: 502,
                    body: "response too large".to_string(),
                });
            }
            out.extend_from_slice(&chunk);
        }

        Ok(out)
    }

    pub async fn fetch_text(&self, key: &str) -> Result<String> {
        let bytes = self.fetch(key).await?;
        String::from_utf8(bytes)
            .map_err(|e| PipeError::InvalidInput(format!("fetched data is not UTF-8: {}", e)))
    }

    pub async fn fetch_json<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        let bytes = self.fetch(key).await?;
        Ok(serde_json::from_slice::<T>(&bytes)?)
    }

    pub async fn delete(&self, key: &str) -> Result<DeleteResponse> {
        self.require_api_key("delete")?;

        let file_name = if is_uuid(key) {
            self.check_status(Some(key), None).await?.file_name
        } else {
            key.to_string()
        };

        self.delete_file_name(&file_name).await
    }

    pub async fn delete_file_name(&self, file_name: &str) -> Result<DeleteResponse> {
        self.require_api_key("delete")?;

        let url = format!(
            "{}/{}",
            self.control_base_url,
            if self.use_pop_gateway {
                "pop/v1/deleteFile"
            } else {
                "deleteFile"
            }
        );
        let response = self
            .http
            .request(Method::POST, &url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.api_key.as_deref().unwrap_or_default()),
            )
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({ "file_name": file_name }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(http_error(response).await);
        }

        parse_json_response(response).await
    }

    fn resolve_fetch_url(&self, key: &str) -> Result<String> {
        if is_http_url(key) {
            return Ok(key.to_string());
        }

        if is_hex_hash(key) {
            return self.deterministic_url(key, None);
        }

        Ok(format!(
            "{}/download-stream?file_name={}",
            self.data_base_url,
            urlencoding::encode(key)
        ))
    }

    fn is_public_deterministic_url(&self, url: &str) -> bool {
        let prefix = format!("{}/", self.data_base_url);
        if !url.starts_with(&prefix) {
            return false;
        }

        let tail = &url[prefix.len()..];
        let mut parts = tail.split('/');
        let _account = parts.next();
        let maybe_hash = parts.next();
        let extra = parts.next();

        extra.is_none() && maybe_hash.map(is_hex_hash).unwrap_or(false)
    }

    fn require_api_key(&self, action: &'static str) -> Result<()> {
        if self.api_key.is_none() {
            return Err(PipeError::MissingApiKey(action));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StoreData {
    bytes: Vec<u8>,
}

impl From<Vec<u8>> for StoreData {
    fn from(value: Vec<u8>) -> Self {
        Self { bytes: value }
    }
}

impl From<&[u8]> for StoreData {
    fn from(value: &[u8]) -> Self {
        Self {
            bytes: value.to_vec(),
        }
    }
}

impl From<String> for StoreData {
    fn from(value: String) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<&str> for StoreData {
    fn from(value: &str) -> Self {
        Self {
            bytes: value.as_bytes().to_vec(),
        }
    }
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn env_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn is_hex_hash(value: &str) -> bool {
    value.len() == 64 && value.as_bytes().iter().all(|b| b.is_ascii_hexdigit())
}

fn is_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

async fn parse_json_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let body = response.text().await?;
    Ok(serde_json::from_str::<T>(&body)?)
}

fn header_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

async fn http_error(response: reqwest::Response) -> PipeError {
    let status = response.status();
    let body = response.text().await.unwrap_or_else(|_| {
        status
            .canonical_reason()
            .unwrap_or("request failed")
            .to_string()
    });

    PipeError::Http {
        status: status.as_u16(),
        body,
    }
}
