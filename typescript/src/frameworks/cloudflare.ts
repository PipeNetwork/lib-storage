import { createOpenAIPipeTools, runOpenAIPipeTool } from "./openai.js";
import { PipeStorageClient } from "../index.js";

export interface CloudflarePipeOptions {
  enableDelete?: boolean;
}

// Workers AI and AI Workflows commonly use OpenAI-compatible tool schemas.
export function createCloudflarePipeTools(options: CloudflarePipeOptions = {}) {
  return createOpenAIPipeTools({
    enableDelete: options.enableDelete,
  });
}

export async function runCloudflarePipeTool(
  client: PipeStorageClient,
  name: string,
  args: string | Record<string, unknown>,
): Promise<unknown> {
  return runOpenAIPipeTool(client, name, args);
}
