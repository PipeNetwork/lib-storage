import { PipeStorageClient } from "../dist/index.js";

function requiredEnv(name) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required env var: ${name}`);
  }
  return value;
}

async function main() {
  requiredEnv("PIPE_API_KEY");

  const fileName = `agent/integration-${Date.now()}.json`;
  const marker = `marker-${Date.now()}`;

  const client = new PipeStorageClient({
    apiKey: process.env.PIPE_API_KEY,
    account: process.env.PIPE_ACCOUNT,
    baseUrl:
      process.env.PIPE_BASE_URL ??
      process.env.PIPE_API_BASE_URL ??
      "https://us-west-01-firestarter.pipenetwork.com",
    timeoutMs: Number(process.env.PIPE_TEST_TIMEOUT_MS ?? 180000),
    pollIntervalMs: Number(process.env.PIPE_TEST_POLL_MS ?? 1000),
  });

  console.log("1) store(wait=false)", { fileName });
  const stored = await client.store({ marker, kind: "integration" }, { fileName, wait: false });
  if (!stored.operationId) {
    throw new Error("store did not return operationId");
  }

  console.log("2) checkStatus", { operationId: stored.operationId });
  const status1 = await client.checkStatus({ operationId: stored.operationId });
  console.log("   status=", status1.status);

  console.log("3) waitForOperation");
  const completed = await client.waitForOperation(stored.operationId, {
    timeoutMs: Number(process.env.PIPE_TEST_TIMEOUT_MS ?? 180000),
  });
  if (completed.status !== "completed") {
    throw new Error(`unexpected final status: ${completed.status}`);
  }

  console.log("4) pin");
  const pinned = await client.pin({ operationId: stored.operationId });
  if (!pinned.url) {
    throw new Error("pin did not return deterministic URL");
  }

  console.log("5) fetch(asJson)");
  const fetched = await client.fetch(pinned.url, { asJson: true });
  if (!fetched || typeof fetched !== "object" || fetched.marker !== marker) {
    throw new Error("fetched payload does not match marker");
  }

  console.log("6) delete");
  await client.delete(fileName);

  console.log("Integration flow passed", {
    operationId: stored.operationId,
    deterministicUrl: pinned.url,
    contentHash: pinned.contentHash,
  });
}

main().catch((err) => {
  console.error("Integration flow failed:", err);
  process.exit(1);
});
