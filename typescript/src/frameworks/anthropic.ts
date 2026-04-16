import { PipeStorageClient } from "../index.js";
import { runOpenAIPipeTool } from "./openai.js";

export interface AnthropicTool {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

export interface AnthropicPipeOptions {
  enableDelete?: boolean;
}

export function createAnthropicPipeTools(
  options: AnthropicPipeOptions = {},
): AnthropicTool[] {
  const tools: AnthropicTool[] = [
    {
      name: "pipe_store",
      description: "Store bytes/JSON in Pipe and return operation + deterministic URL",
      input_schema: {
        type: "object",
        properties: {
          file_name: { type: "string" },
          data: {
            description: "JSON-serializable payload or UTF-8 text",
          },
          tier: {
            type: "string",
            enum: ["normal", "priority", "premium", "ultra", "enterprise"],
          },
        },
        required: ["file_name", "data"],
      },
    },
    {
      name: "pipe_pin",
      description: "Resolve deterministic URL from operation_id, file_name, or content_hash",
      input_schema: {
        type: "object",
        properties: {
          operation_id: { type: "string" },
          file_name: { type: "string" },
          content_hash: { type: "string" },
          account: { type: "string" },
        },
      },
    },
    {
      name: "pipe_fetch",
      description: "Fetch object bytes/text/json via deterministic URL/hash or file_name",
      input_schema: {
        type: "object",
        properties: {
          key: {
            type: "string",
            description: "deterministic URL, 64-char hash, or file name",
          },
          as_text: { type: "boolean" },
          as_json: { type: "boolean" },
        },
        required: ["key"],
      },
    },
  ];

  if (options.enableDelete !== false) {
    tools.push({
      name: "pipe_delete",
      description: "Delete object by file_name or operation_id",
      input_schema: {
        type: "object",
        properties: {
          file_name: { type: "string" },
          operation_id: { type: "string" },
        },
      },
    });
  }

  return tools;
}

export async function runAnthropicPipeTool(
  client: PipeStorageClient,
  name: string,
  input: Record<string, unknown>,
): Promise<unknown> {
  return runOpenAIPipeTool(client, name, input);
}
