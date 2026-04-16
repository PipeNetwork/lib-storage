import { PipeStorageClient } from "../index.js";
import { runOpenAIPipeTool } from "./openai.js";

export interface VercelToolDefinition {
  description: string;
  parameters: Record<string, unknown>;
  execute: (args: Record<string, unknown>) => Promise<unknown>;
}

export interface VercelPipeToolOptions {
  enableDelete?: boolean;
}

// Minimal AI SDK-compatible shape for tool maps used in Vercel AI SDK pipelines.
export function createVercelPipeTools(
  client: PipeStorageClient,
  options: VercelPipeToolOptions = {},
): Record<string, VercelToolDefinition> {
  const tools: Record<string, VercelToolDefinition> = {
    pipe_store: {
      description: "Store bytes/JSON in Pipe and return operation + deterministic URL",
      parameters: {
        type: "object",
        properties: {
          file_name: { type: "string" },
          data: { description: "JSON-serializable payload or UTF-8 text" },
          tier: {
            type: "string",
            enum: ["normal", "priority", "premium", "ultra", "enterprise"],
          },
        },
        required: ["file_name", "data"],
      },
      execute: async (args) => runOpenAIPipeTool(client, "pipe_store", args),
    },
    pipe_pin: {
      description: "Resolve deterministic URL from operation_id, file_name, or content_hash",
      parameters: {
        type: "object",
        properties: {
          operation_id: { type: "string" },
          file_name: { type: "string" },
          content_hash: { type: "string" },
          account: { type: "string" },
        },
      },
      execute: async (args) => runOpenAIPipeTool(client, "pipe_pin", args),
    },
    pipe_fetch: {
      description: "Fetch object bytes/text/json via deterministic URL/hash or file_name",
      parameters: {
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
      execute: async (args) => runOpenAIPipeTool(client, "pipe_fetch", args),
    },
  };

  if (options.enableDelete !== false) {
    tools.pipe_delete = {
      description: "Delete object by file_name or operation_id",
      parameters: {
        type: "object",
        properties: {
          file_name: { type: "string" },
          operation_id: { type: "string" },
        },
      },
      execute: async (args) => runOpenAIPipeTool(client, "pipe_delete", args),
    };
  }

  return tools;
}
