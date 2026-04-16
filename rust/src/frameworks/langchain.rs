use crate::client::{PinParams, PipeError, PipeStorage, Result, StoreOptions, UploadTier};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct PipeStorageLangChainTool {
    pub client: PipeStorage,
    pub name: String,
    pub description: String,
}

impl PipeStorageLangChainTool {
    pub fn new(client: PipeStorage) -> Self {
        Self {
            client,
            name: "pipe_storage".to_string(),
            description:
                "Pipe storage tool: action=store|pin|fetch|delete for deterministic agent storage."
                    .to_string(),
        }
    }

    pub async fn invoke(&self, payload: &str) -> Result<String> {
        let args: Value = serde_json::from_str(payload)?;
        let value = self.invoke_value(args).await?;
        Ok(serde_json::to_string(&value)?)
    }

    pub async fn invoke_value(&self, args: Value) -> Result<Value> {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();

        match action.as_str() {
            "store" => {
                let file_name = args
                    .get("file_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        PipeError::InvalidInput("store action requires file_name".to_string())
                    })?
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
                let pinned = self
                    .client
                    .pin_with(PinParams {
                        operation_id: stored.operation_id.clone(),
                        ..Default::default()
                    })
                    .await?;

                Ok(json!({
                    "action": "store",
                    "operation_id": stored.operation_id,
                    "file_name": stored.file_name,
                    "content_hash": pinned.content_hash,
                    "deterministic_url": pinned.url,
                }))
            }
            "pin" => {
                let pinned = self
                    .client
                    .pin_with(PinParams {
                        operation_id: args
                            .get("operation_id")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        file_name: args
                            .get("file_name")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        content_hash: args
                            .get("content_hash")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        account: args
                            .get("account")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                    })
                    .await?;
                Ok(json!({ "action": "pin", "result": pinned }))
            }
            "fetch" => {
                let key = args
                    .get("key")
                    .or_else(|| args.get("file_name"))
                    .or_else(|| args.get("content_hash"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        PipeError::InvalidInput(
                            "fetch action requires key/file_name/content_hash".to_string(),
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
                    return Ok(json!({ "action": "fetch", "value": value }));
                }

                if as_text {
                    let value = self.client.fetch_text(key).await?;
                    return Ok(json!({ "action": "fetch", "value": value }));
                }

                let bytes = self.client.fetch(key).await?;
                Ok(json!({
                    "action": "fetch",
                    "bytes_base64": STANDARD.encode(&bytes),
                    "bytes_len": bytes.len(),
                }))
            }
            "delete" => {
                let file_name = args.get("file_name").and_then(Value::as_str);
                let operation_id = args.get("operation_id").and_then(Value::as_str);

                let deleted = if let Some(file_name) = file_name {
                    self.client.delete_file_name(file_name).await?
                } else if let Some(operation_id) = operation_id {
                    self.client.delete(operation_id).await?
                } else {
                    return Err(PipeError::InvalidInput(
                        "delete action requires file_name or operation_id".to_string(),
                    ));
                };

                Ok(json!({ "action": "delete", "message": deleted.message }))
            }
            _ => Err(PipeError::InvalidInput(format!(
                "unsupported action: {}",
                action
            ))),
        }
    }
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
