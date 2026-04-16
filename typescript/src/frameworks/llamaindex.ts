import { PipeStorageClient } from "../index.js";

export interface LlamaIndexTool {
  metadata: {
    name: string;
    description: string;
  };
  call: (args: Record<string, unknown>) => Promise<string>;
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

// Minimal adapters compatible with common LlamaIndex tool expectations.
export function createLlamaIndexPipeTools(client: PipeStorageClient): LlamaIndexTool[] {
  return [
    {
      metadata: {
        name: "pipe_store",
        description: "Store JSON/text in Pipe and return deterministic URL",
      },
      call: async (args) => {
        const fileName = String(args.file_name ?? "agent/object.json");
        const stored = await client.store(args.data, {
          fileName,
          tier: args.tier as
            | "normal"
            | "priority"
            | "premium"
            | "ultra"
            | "enterprise"
            | undefined,
        });
        const pinned = await client.pin({ operationId: stored.operationId });
        return JSON.stringify({
          operation_id: stored.operationId,
          file_name: stored.fileName,
          content_hash: pinned.contentHash,
          deterministic_url: pinned.url,
        });
      },
    },
    {
      metadata: {
        name: "pipe_pin",
        description: "Resolve deterministic URL from operation_id, file_name, or content_hash",
      },
      call: async (args) => {
        const result = await client.pin({
          operationId: args.operation_id as string | undefined,
          fileName: args.file_name as string | undefined,
          contentHash: args.content_hash as string | undefined,
          account: args.account as string | undefined,
        });
        return JSON.stringify(result);
      },
    },
    {
      metadata: {
        name: "pipe_fetch",
        description: "Fetch bytes/text/json from Pipe by key/hash/url",
      },
      call: async (args) => {
        const key = String(args.key ?? args.file_name ?? args.content_hash ?? "");
        const fetched = await client.fetch(key, {
          asText: Boolean(args.as_text),
          asJson: Boolean(args.as_json),
        });
        if (fetched instanceof Uint8Array) {
          return JSON.stringify({
            bytes_base64: bytesToBase64(fetched),
            bytes_len: fetched.byteLength,
          });
        }
        return JSON.stringify(fetched);
      },
    },
    {
      metadata: {
        name: "pipe_delete",
        description: "Delete object in Pipe by file_name or operation_id",
      },
      call: async (args) => {
        const deleted = await client.delete({
          fileName: args.file_name as string | undefined,
          operationId: args.operation_id as string | undefined,
        });
        return JSON.stringify(deleted);
      },
    },
  ];
}
