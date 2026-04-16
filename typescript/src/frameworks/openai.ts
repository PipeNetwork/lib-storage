import { PipeStorageClient } from "../index.js";

export interface OpenAIFunctionTool {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

export interface PipeOpenAIOptions {
  enableDelete?: boolean;
  enableRawFetch?: boolean;
}

function bytesToBase64(bytes: Uint8Array): string {
  const g = globalThis as { btoa?: (s: string) => string; Buffer?: { from: (b: Uint8Array) => { toString: (enc: string) => string } } };
  if (g.Buffer) {
    return g.Buffer.from(bytes).toString("base64");
  }
  if (!g.btoa) {
    throw new Error("No base64 encoder available in this runtime");
  }
  let binary = "";
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  return g.btoa(binary);
}

export function createOpenAIPipeTools(
  options: PipeOpenAIOptions = {},
): OpenAIFunctionTool[] {
  const tools: OpenAIFunctionTool[] = [
    {
      type: "function",
      function: {
        name: "pipe_store",
        description: "Store bytes/JSON in Pipe and return operation + deterministic URL",
        parameters: {
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
    },
    {
      type: "function",
      function: {
        name: "pipe_pin",
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
      },
    },
    {
      type: "function",
      function: {
        name: "pipe_fetch",
        description: "Fetch object bytes/text via deterministic URL/hash or file_name",
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
      },
    },
  ];

  if (options.enableDelete !== false) {
    tools.push({
      type: "function",
      function: {
        name: "pipe_delete",
        description: "Delete object by file_name or operation_id",
        parameters: {
          type: "object",
          properties: {
            file_name: { type: "string" },
            operation_id: { type: "string" },
          },
        },
      },
    });
  }

  return tools;
}

export async function runOpenAIPipeTool(
  client: PipeStorageClient,
  name: string,
  rawArguments: string | Record<string, unknown>,
): Promise<unknown> {
  const args =
    typeof rawArguments === "string"
      ? (JSON.parse(rawArguments) as Record<string, unknown>)
      : rawArguments;

  switch (name) {
    case "pipe_store": {
      const fileName = String(args.file_name ?? "agent/object.json");
      const data = args.data;
      const tier = args.tier as
        | "normal"
        | "priority"
        | "premium"
        | "ultra"
        | "enterprise"
        | undefined;

      const stored = await client.store(data, {
        fileName,
        tier,
      });

      const pinned = await client.pin({ operationId: stored.operationId });
      return {
        operation_id: stored.operationId,
        file_name: stored.fileName,
        content_hash: pinned.contentHash,
        deterministic_url: pinned.url,
      };
    }

    case "pipe_pin": {
      const pinned = await client.pin({
        operationId: args.operation_id as string | undefined,
        fileName: args.file_name as string | undefined,
        contentHash: args.content_hash as string | undefined,
        account: args.account as string | undefined,
      });
      return pinned;
    }

    case "pipe_fetch": {
      const key = String(args.key ?? "");
      const asText = Boolean(args.as_text ?? false);
      const asJson = Boolean(args.as_json ?? false);
      const fetched = await client.fetch(key, {
        asText,
        asJson,
      });
      if (fetched instanceof Uint8Array) {
        return {
          bytes_base64: bytesToBase64(fetched),
          bytes_len: fetched.byteLength,
        };
      }
      return fetched;
    }

    case "pipe_delete": {
      const deleted = await client.delete({
        fileName: args.file_name as string | undefined,
        operationId: args.operation_id as string | undefined,
      });
      return deleted;
    }

    default:
      throw new Error(`Unknown Pipe tool: ${name}`);
  }
}
