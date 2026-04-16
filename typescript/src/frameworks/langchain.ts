import { PipeStorageClient } from "../index.js";

export type LangChainPipeAction = "store" | "pin" | "fetch" | "delete";

export interface LangChainPipeInput {
  action: LangChainPipeAction;
  file_name?: string;
  operation_id?: string;
  content_hash?: string;
  key?: string;
  data?: unknown;
  tier?: "normal" | "priority" | "premium" | "ultra" | "enterprise";
  as_text?: boolean;
  as_json?: boolean;
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

// Small dependency-free adapter that matches common Tool shape (name/description/invoke).
export class PipeStorageLangChainTool {
  readonly name = "pipe_storage";
  readonly description =
    "Pipe storage tool: action=store|pin|fetch|delete for deterministic agent storage.";

  private readonly client: PipeStorageClient;

  constructor(client: PipeStorageClient) {
    this.client = client;
  }

  async invoke(input: string | LangChainPipeInput): Promise<string> {
    const payload: LangChainPipeInput =
      typeof input === "string"
        ? (JSON.parse(input) as LangChainPipeInput)
        : input;

    switch (payload.action) {
      case "store": {
        if (!payload.file_name) {
          throw new Error("store action requires file_name");
        }
        const stored = await this.client.store(payload.data, {
          fileName: payload.file_name,
          tier: payload.tier,
        });
        const pinned = await this.client.pin({ operationId: stored.operationId });
        return JSON.stringify({
          action: "store",
          operation_id: stored.operationId,
          file_name: stored.fileName,
          content_hash: pinned.contentHash,
          deterministic_url: pinned.url,
        });
      }

      case "pin": {
        const pinned = await this.client.pin({
          operationId: payload.operation_id,
          fileName: payload.file_name,
          contentHash: payload.content_hash,
        });
        return JSON.stringify({ action: "pin", ...pinned });
      }

      case "fetch": {
        const key = payload.key ?? payload.file_name ?? payload.content_hash;
        if (!key) {
          throw new Error("fetch action requires key/file_name/content_hash");
        }
        const fetched = await this.client.fetch(key, {
          asText: Boolean(payload.as_text),
          asJson: Boolean(payload.as_json),
        });
        if (fetched instanceof Uint8Array) {
          return JSON.stringify({
            action: "fetch",
            bytes_base64: bytesToBase64(fetched),
            bytes_len: fetched.byteLength,
          });
        }
        return JSON.stringify({ action: "fetch", value: fetched });
      }

      case "delete": {
        const deleted = await this.client.delete({
          fileName: payload.file_name,
          operationId: payload.operation_id,
        });
        return JSON.stringify({ action: "delete", ...deleted });
      }

      default:
        throw new Error(`Unsupported action: ${(payload as LangChainPipeInput).action}`);
    }
  }
}
