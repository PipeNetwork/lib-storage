import { PipeStorageClient, createOpenAIPipeTools } from "../typescript/dist/index.js";

async function main() {
  const pipe = new PipeStorageClient({
    apiKey: process.env.PIPE_API_KEY,
    account: process.env.PIPE_ACCOUNT,
  });

  const fileName = `agent/session-${Date.now()}.json`;

  const stored = await pipe.store(
    {
      task: "agent-memory",
      created_at: new Date().toISOString(),
      notes: ["first checkpoint", "second checkpoint"],
    },
    { fileName },
  );

  const pinned = await pipe.pin({ operationId: stored.operationId });
  console.log("deterministic_url", pinned.url);

  const data = await pipe.fetch(pinned.url, { asJson: true });
  console.log("fetched", data);

  const tools = createOpenAIPipeTools();
  console.log("openai_tools", tools.map((t) => t.function.name));

  await pipe.delete(fileName);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
