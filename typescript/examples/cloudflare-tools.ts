import {
  PipeStorageClient,
  createCloudflarePipeTools,
  runCloudflarePipeTool,
} from "../src/index.js";

async function main() {
  const client = new PipeStorageClient();
  const tools = createCloudflarePipeTools();
  console.log("cloudflare_tools", tools.map((t) => t.function.name));

  const fileName = `agent/cloudflare-${Date.now()}.json`;
  const stored = await runCloudflarePipeTool(client, "pipe_store", {
    file_name: fileName,
    data: { from: "cloudflare-workflows", ts: new Date().toISOString() },
  });
  console.log("pipe_store", stored);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
