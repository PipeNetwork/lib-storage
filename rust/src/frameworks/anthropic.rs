use crate::client::{PipeStorage, Result};
use crate::frameworks::openai::run_openai_pipe_tool;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub fn create_anthropic_pipe_tools(enable_delete: bool) -> Vec<AnthropicToolDefinition> {
    let mut tools = vec![
        AnthropicToolDefinition {
            name: "pipe_store".to_string(),
            description: "Store bytes/JSON in Pipe and return operation + deterministic URL"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_name": {"type": "string"},
                    "data": {"description": "JSON payload or text"},
                    "tier": {"type": "string", "enum": ["normal", "priority", "premium", "ultra", "enterprise"]}
                },
                "required": ["file_name", "data"]
            }),
        },
        AnthropicToolDefinition {
            name: "pipe_pin".to_string(),
            description: "Resolve deterministic URL from operation_id, file_name, or content_hash"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "operation_id": {"type": "string"},
                    "file_name": {"type": "string"},
                    "content_hash": {"type": "string"},
                    "account": {"type": "string"}
                }
            }),
        },
        AnthropicToolDefinition {
            name: "pipe_fetch".to_string(),
            description: "Fetch object bytes/text/json via deterministic URL/hash or file_name"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string"},
                    "as_text": {"type": "boolean"},
                    "as_json": {"type": "boolean"}
                },
                "required": ["key"]
            }),
        },
    ];

    if enable_delete {
        tools.push(AnthropicToolDefinition {
            name: "pipe_delete".to_string(),
            description: "Delete object by file_name or operation_id".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_name": {"type": "string"},
                    "operation_id": {"type": "string"}
                }
            }),
        });
    }

    tools
}

pub async fn run_anthropic_pipe_tool(
    client: &PipeStorage,
    name: &str,
    input: Value,
) -> Result<Value> {
    run_openai_pipe_tool(client, name, input).await
}
