use crate::client::{PinParams, PipeError, PipeStorage, Result, StoreOptions, UploadTier};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaIndexToolMetadata {
    pub name: String,
    pub description: String,
}

#[derive(Clone)]
pub struct LlamaIndexPipeTool {
    pub metadata: LlamaIndexToolMetadata,
    client: PipeStorage,
}

impl LlamaIndexPipeTool {
    pub async fn call(&self, args: Value) -> Result<String> {
        let value = match self.metadata.name.as_str() {
            "pipe_store" => self.call_store(args).await?,
            "pipe_pin" => self.call_pin(args).await?,
            "pipe_fetch" => self.call_fetch(args).await?,
            "pipe_delete" => self.call_delete(args).await?,
            _ => {
                return Err(PipeError::InvalidInput(format!(
                    "unknown tool: {}",
                    self.metadata.name
                )))
            }
        };

        Ok(serde_json::to_string(&value)?)
    }

    async fn call_store(&self, args: Value) -> Result<Value> {
        let file_name = args
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or("agent/object.json")
            .to_string();
        let tier = args
            .get("tier")
            .and_then(Value::as_str)
            .and_then(parse_tier);
        let data = args
            .get("data")
            .map(value_to_store_bytes)
            .unwrap_or_default();

        let stored = self
            .client
            .store(
                data,
                StoreOptions {
                    file_name: Some(file_name),
                    tier,
                    ..Default::default()
                },
            )
            .await?;
        let pinned = self.client.pin(stored.file_name.as_str()).await?;

        Ok(json!({
            "operation_id": stored.operation_id,
            "file_name": stored.file_name,
            "content_hash": pinned.content_hash,
            "deterministic_url": pinned.url,
        }))
    }

    async fn call_pin(&self, args: Value) -> Result<Value> {
        let content_hash = args
            .get("content_hash")
            .and_then(Value::as_str)
            .map(String::from);
        let operation_id = args
            .get("operation_id")
            .and_then(Value::as_str)
            .map(String::from);
        let file_name = args
            .get("file_name")
            .and_then(Value::as_str)
            .map(String::from);
        let account = args
            .get("account")
            .and_then(Value::as_str)
            .map(String::from);

        let pinned = self
            .client
            .pin_with(PinParams {
                content_hash,
                operation_id,
                file_name,
                account,
            })
            .await?;

        Ok(json!({
            "url": pinned.url,
            "content_hash": pinned.content_hash,
            "operation_id": pinned.operation_id,
            "file_name": pinned.file_name,
            "status": pinned.status,
        }))
    }

    async fn call_fetch(&self, args: Value) -> Result<Value> {
        let key = args
            .get("key")
            .or_else(|| args.get("file_name"))
            .or_else(|| args.get("content_hash"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PipeError::InvalidInput(
                    "pipe_fetch requires key/file_name/content_hash".to_string(),
                )
            })?;

        let as_text = args
            .get("as_text")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let as_json = args
            .get("as_json")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if as_json {
            let value: Value = self.client.fetch_json(key).await?;
            return Ok(value);
        }

        if as_text {
            let value = self.client.fetch_text(key).await?;
            return Ok(json!(value));
        }

        let bytes = self.client.fetch(key).await?;
        Ok(json!({
            "bytes_base64": STANDARD.encode(&bytes),
            "bytes_len": bytes.len(),
        }))
    }

    async fn call_delete(&self, args: Value) -> Result<Value> {
        let file_name = args.get("file_name").and_then(Value::as_str);
        let operation_id = args.get("operation_id").and_then(Value::as_str);

        let deleted = if let Some(file_name) = file_name {
            self.client.delete_file_name(file_name).await?
        } else if let Some(operation_id) = operation_id {
            self.client.delete(operation_id).await?
        } else {
            return Err(PipeError::InvalidInput(
                "pipe_delete requires file_name or operation_id".to_string(),
            ));
        };

        Ok(serde_json::to_value(deleted)?)
    }
}

pub fn create_llamaindex_pipe_tools(client: PipeStorage) -> Vec<LlamaIndexPipeTool> {
    vec![
        LlamaIndexPipeTool {
            metadata: LlamaIndexToolMetadata {
                name: "pipe_store".to_string(),
                description: "Store JSON/text in Pipe and return deterministic URL".to_string(),
            },
            client: client.clone(),
        },
        LlamaIndexPipeTool {
            metadata: LlamaIndexToolMetadata {
                name: "pipe_pin".to_string(),
                description:
                    "Resolve deterministic URL from operation_id, file_name, or content_hash"
                        .to_string(),
            },
            client: client.clone(),
        },
        LlamaIndexPipeTool {
            metadata: LlamaIndexToolMetadata {
                name: "pipe_fetch".to_string(),
                description: "Fetch bytes/text/json from Pipe by key/hash/url".to_string(),
            },
            client: client.clone(),
        },
        LlamaIndexPipeTool {
            metadata: LlamaIndexToolMetadata {
                name: "pipe_delete".to_string(),
                description: "Delete Pipe object by file_name or operation_id".to_string(),
            },
            client,
        },
    ]
}

fn parse_tier(raw: &str) -> Option<UploadTier> {
    match raw.to_ascii_lowercase().as_str() {
        "normal" => Some(UploadTier::Normal),
        "priority" => Some(UploadTier::Priority),
        "premium" => Some(UploadTier::Premium),
        "ultra" => Some(UploadTier::Ultra),
        "enterprise" => Some(UploadTier::Enterprise),
        _ => None,
    }
}

fn value_to_store_bytes(v: &Value) -> Vec<u8> {
    if let Some(s) = v.as_str() {
        s.as_bytes().to_vec()
    } else {
        serde_json::to_vec(v).unwrap_or_default()
    }
}
