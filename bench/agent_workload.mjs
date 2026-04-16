import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PipeStorageClient } from "../typescript/dist/index.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function envNum(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function envBool(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const norm = raw.toLowerCase();
  return norm === "1" || norm === "true" || norm === "yes";
}

function parseSizes() {
  const raw = process.env.PIPE_BENCH_SIZES;
  if (!raw) return [1024, 10 * 1024, 50 * 1024, 100 * 1024, 200 * 1024];
  const values = raw
    .split(",")
    .map((v) => Number(v.trim()))
    .filter((n) => Number.isFinite(n) && n > 0)
    .map((n) => Math.floor(n));
  return values.length > 0 ? values : [1024, 10 * 1024, 50 * 1024, 100 * 1024, 200 * 1024];
}

function percentile(values, p) {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[idx];
}

async function runWithConcurrency(tasks, concurrency) {
  const results = [];
  let cursor = 0;

  async function worker() {
    while (true) {
      const idx = cursor;
      cursor += 1;
      if (idx >= tasks.length) return;
      results[idx] = await tasks[idx]();
    }
  }

  const workers = [];
  const count = Math.max(1, Math.min(concurrency, tasks.length));
  for (let i = 0; i < count; i += 1) {
    workers.push(worker());
  }

  await Promise.all(workers);
  return results;
}

function nowMs() {
  return Date.now();
}

function bytes(size, seed) {
  const out = new Uint8Array(size);
  for (let i = 0; i < size; i += 1) {
    out[i] = (seed + i) % 251;
  }
  return out;
}

async function main() {
  const apiKey = process.env.PIPE_API_KEY;
  if (!apiKey) {
    throw new Error("Missing PIPE_API_KEY");
  }

  const baseUrl =
    process.env.PIPE_BASE_URL ||
    process.env.PIPE_API_BASE_URL ||
    "https://us-west-01-firestarter.pipenetwork.com";

  const account = process.env.PIPE_ACCOUNT;
  const sizes = parseSizes();
  const writesPerSize = envNum("PIPE_BENCH_WRITES_PER_SIZE", 30);
  const concurrency = envNum("PIPE_BENCH_CONCURRENCY", 16);
  const deleteAfter = envBool("PIPE_BENCH_DELETE_AFTER", true);
  const waitForUpload = envBool("PIPE_BENCH_WAIT", true);

  const client = new PipeStorageClient({
    apiKey,
    account,
    baseUrl,
    timeoutMs: envNum("PIPE_BENCH_TIMEOUT_MS", 240_000),
    pollIntervalMs: envNum("PIPE_BENCH_POLL_MS", 1_000),
  });

  const runStarted = new Date().toISOString();
  const summary = {
    started_at: runStarted,
    base_url: baseUrl,
    account: account || null,
    writes_per_size: writesPerSize,
    concurrency,
    wait: waitForUpload,
    delete_after: deleteAfter,
    sizes,
    results: [],
  };

  for (const size of sizes) {
    const namespace = `agent/bench/${Date.now()}-${size}`;
    const taskDefs = [];

    for (let i = 0; i < writesPerSize; i += 1) {
      taskDefs.push(async () => {
        const fileName = `${namespace}-${i}.bin`;
        const payload = bytes(size, i + size);
        const started = nowMs();
        try {
          const stored = await client.store(payload, {
            fileName,
            wait: waitForUpload,
          });
          const ended = nowMs();
          return {
            ok: true,
            size,
            file_name: fileName,
            operation_id: stored.operationId || null,
            deterministic_url: stored.deterministicUrl || null,
            latency_ms: ended - started,
          };
        } catch (err) {
          const ended = nowMs();
          return {
            ok: false,
            size,
            file_name: fileName,
            error: err instanceof Error ? err.message : String(err),
            latency_ms: ended - started,
          };
        }
      });
    }

    console.log(`Running size=${size} bytes, writes=${writesPerSize}, concurrency=${concurrency}`);
    const outcomes = await runWithConcurrency(taskDefs, concurrency);

    const success = outcomes.filter((r) => r.ok);
    const failed = outcomes.filter((r) => !r.ok);
    const latencies = success.map((r) => r.latency_ms);

    const row = {
      size_bytes: size,
      total: outcomes.length,
      success: success.length,
      failed: failed.length,
      success_rate: outcomes.length ? success.length / outcomes.length : 0,
      latency_ms: {
        avg: latencies.length ? Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length) : 0,
        p50: Math.round(percentile(latencies, 50)),
        p95: Math.round(percentile(latencies, 95)),
        p99: Math.round(percentile(latencies, 99)),
      },
      sample_errors: failed.slice(0, 3).map((f) => f.error),
    };

    summary.results.push(row);
    console.log("  ->", row);

    if (deleteAfter) {
      const deletions = success.map((r) => async () => {
        try {
          await client.delete(r.file_name);
          return { ok: true };
        } catch (err) {
          return { ok: false, error: err instanceof Error ? err.message : String(err) };
        }
      });
      const deleted = await runWithConcurrency(deletions, concurrency);
      const failedDelete = deleted.filter((d) => !d.ok).length;
      if (failedDelete > 0) {
        console.warn(`  delete failures for size=${size}: ${failedDelete}`);
      }
    }
  }

  const outFile =
    process.env.PIPE_BENCH_OUT ||
    path.join(__dirname, "results", `bench-${new Date().toISOString().replace(/[:.]/g, "-")}.json`);

  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  fs.writeFileSync(outFile, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  console.log(`Benchmark results saved to ${outFile}`);
}

main().catch((err) => {
  console.error("Benchmark failed:", err);
  process.exit(1);
});
