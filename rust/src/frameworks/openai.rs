use crate::client::{PinParams, PipeError, PipeStorage, Result, StoreOptions, UploadTier};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionTool {
    #[serde(rename = "type")]
    pub type_field: String,
    pub function: OpenAIFunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub fn create_openai_pipe_tools(enable_delete: bool) -> Vec<OpenAIFunctionTool> {
    let mut tools = vec![
        OpenAIFunctionTool {
            type_field: "function".to_string(),
            function: OpenAIFunctionDefinition {
                name: "pipe_store".to_string(),
                description: "Store bytes/JSON in Pipe and return operation + deterministic URL"
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "file_name": {"type": "string"},
                        "data": {"description": "JSON payload or text"},
                        "tier": {"type": "string", "enum": ["normal", "priority", "premium", "ultra", "enterprise"]}
                    },
                    "required": ["file_name", "data"]
                }),
            },
        },
        OpenAIFunctionTool {
            type_field: "function".to_string(),
            function: OpenAIFunctionDefinition {
                name: "pipe_pin".to_string(),
                description:
                    "Resolve deterministic URL from operation_id, file_name, or content_hash"
                        .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "operation_id": {"type": "string"},
                        "file_name": {"type": "string"},
                        "content_hash": {"type": "string"},
                        "account": {"type": "string"}
                    }
                }),
            },
        },
        OpenAIFunctionTool {
            type_field: "function".to_string(),
            function: OpenAIFunctionDefinition {
                name: "pipe_fetch".to_string(),
                description: "Fetch object bytes/text/json via deterministic URL/hash or file_name"
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {"type": "string"},
                        "as_text": {"type": "boolean"},
                        "as_json": {"type": "boolean"}
                    },
                    "required": ["key"]
                }),
            },
        },
    ];

    if enable_delete {
        tools.push(OpenAIFunctionTool {
            type_field: "function".to_string(),
            function: OpenAIFunctionDefinition {
                name: "pipe_delete".to_string(),
                description: "Delete object by file_name or operation_id".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "file_name": {"type": "string"},
                        "operation_id": {"type": "string"}
                    }
                }),
            },
        });
    }

    tools
}

pub async fn run_openai_pipe_tool(client: &PipeStorage, name: &str, args: Value) -> Result<Value> {
    match name {
        "pipe_store" => {
            let file_name = args
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("agent/object.json")
                .to_string();

            let tier = args
                .get("tier")
                .and_then(Value::as_str)
                .and_then(parse_tier);

            let store_data = args
                .get("data")
                .map(value_to_store_bytes)
                .unwrap_or_default();

            let stored = client
                .store(
                    store_data,
                    StoreOptions {
                        file_name: Some(file_name),
                        tier,
                        ..Default::default()
                    },
                )
                .await?;

            let pinned = client
                .pin_with(PinParams {
                    operation_id: stored.operation_id.clone(),
                    ..Default::default()
                })
                .await?;

            Ok(json!({
                "operation_id": stored.operation_id,
                "file_name": stored.file_name,
                "content_hash": pinned.content_hash,
                "deterministic_url": pinned.url,
            }))
        }
        "pipe_pin" => {
            let pinned = client
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
            Ok(serde_json::to_value(pinned)?)
        }
        "pipe_fetch" => {
            let key = args
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(|| PipeError::InvalidInput("pipe_fetch requires key".to_string()))?;

            let as_text = args
                .get("as_text")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let as_json = args
                .get("as_json")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if as_json {
                let value: Value = client.fetch_json(key).await?;
                return Ok(value);
            }

            if as_text {
                let value = client.fetch_text(key).await?;
                return Ok(json!(value));
            }

            let bytes = client.fetch(key).await?;
            Ok(json!({
                "bytes_base64": STANDARD.encode(&bytes),
                "bytes_len": bytes.len(),
            }))
        }
        "pipe_delete" => {
            let file_name = args.get("file_name").and_then(Value::as_str);
            let operation_id = args.get("operation_id").and_then(Value::as_str);

            let deleted = if let Some(file_name) = file_name {
                client.delete_file_name(file_name).await?
            } else if let Some(operation_id) = operation_id {
                client.delete(operation_id).await?
            } else {
                return Err(PipeError::InvalidInput(
                    "pipe_delete requires file_name or operation_id".to_string(),
                ));
            };

            Ok(serde_json::to_value(deleted)?)
        }
        _ => Ok(json!({ "error": format!("unknown tool: {}", name) })),
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
