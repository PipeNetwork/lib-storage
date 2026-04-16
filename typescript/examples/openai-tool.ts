import {
  PipeStorageClient,
  createOpenAIPipeTools,
  runOpenAIPipeTool,
} from "../src/index.js";

const pipe = new PipeStorageClient();
export const tools = createOpenAIPipeTools();

export async function runTool(name: string, args: string) {
  return runOpenAIPipeTool(pipe, name, args);
}
