import { PipeStorageClient, createVercelPipeTools } from "../src/index.js";

async function main() {
  const client = new PipeStorageClient();
  const tools = createVercelPipeTools(client);

  const fileName = `agent/vercel-${Date.now()}.json`;
  const stored = await tools.pipe_store.execute({
    file_name: fileName,
    data: { from: "vercel-ai-sdk", ts: new Date().toISOString() },
  });

  console.log("pipe_store", stored);

  if (typeof stored === "object" && stored && "deterministic_url" in stored) {
    const fetched = await tools.pipe_fetch.execute({
      key: (stored as { deterministic_url: string }).deterministic_url,
      as_json: true,
    });
    console.log("pipe_fetch", fetched);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
