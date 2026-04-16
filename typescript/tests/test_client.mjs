import { describe, it, beforeEach } from "node:test";
import assert from "node:assert/strict";
import {
  PipeStorageClient,
  PipeError,
  X402ConflictError,
  X402PendingIntentError,
  X402ProtocolError,
  encodeJsonToBase64,
} from "../dist/index.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const BASE = "https://test.pipe.local";
const API_KEY = "test-api-key-1234";
const ACCOUNT = "TestAccount42";
const VALID_HASH =
  "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

/**
 * Build a mock fetch that returns pre-configured responses in sequence.
 * Each config: { status, headers, body, json, delay }
 *   - headers: plain object or Headers
 *   - json: returned as JSON (sets body to JSON.stringify(json))
 *   - body: raw string body (used when json is absent)
 *   - delay: optional ms to wait before resolving
 * Also records every call in .calls for inspection.
 */
function mockFetch(configs) {
  let idx = 0;
  const calls = [];

  async function fakeFetch(url, init) {
    const callIndex = idx;
    idx += 1;
    calls.push({ url, init });
    const cfg = configs[Math.min(callIndex, configs.length - 1)];

    if (cfg.delay) {
      await new Promise((r) => setTimeout(r, cfg.delay));
    }

    // Respect abort signal
    if (init?.signal?.aborted) {
      throw new DOMException("The operation was aborted.", "AbortError");
    }

    const responseBody =
      cfg.json !== undefined ? JSON.stringify(cfg.json) : (cfg.body ?? "");
    const headers = new Headers(cfg.headers ?? {});
    if (cfg.json !== undefined && !headers.has("content-type")) {
      headers.set("content-type", "application/json");
    }
    return new Response(responseBody, {
      status: cfg.status ?? 200,
      statusText: cfg.statusText ?? "OK",
      headers,
    });
  }

  fakeFetch.calls = calls;
  return fakeFetch;
}

/** Shorthand for a completed UploadStatus JSON payload. */
function completedStatus(overrides = {}) {
  return {
    operation_id: "op-1",
    file_name: "agent/test.bin",
    status: "completed",
    finished: true,
    parts_completed: 1,
    total_parts: 1,
    error: null,
    content_hash: VALID_HASH,
    deterministic_url: `${BASE}/${ACCOUNT}/${VALID_HASH}`,
    bytes_total: 100,
    bytes_uploaded: 100,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:01Z",
    ...overrides,
  };
}

function queuedStatus(overrides = {}) {
  return completedStatus({
    status: "queued",
    finished: false,
    content_hash: null,
    deterministic_url: null,
    bytes_uploaded: 0,
    ...overrides,
  });
}

function failedStatus(overrides = {}) {
  return completedStatus({
    status: "failed",
    finished: true,
    error: "checksum mismatch",
    content_hash: null,
    deterministic_url: null,
    ...overrides,
  });
}

function creditsIntentStatus(overrides = {}) {
  return {
    intent_id: "intent-1",
    status: "pending",
    requested_usdc_raw: 1_000_000,
    detected_usdc_raw: 0,
    credited_usdc_raw: 0,
    usdc_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    treasury_owner_pubkey: "TreasuryOwner111111111111111111111111111111",
    treasury_usdc_ata: "TreasuryAta11111111111111111111111111111111",
    reference_pubkey: "Reference11111111111111111111111111111111",
    payment_tx_sig: null,
    last_checked_at: "2026-01-01T00:00:00Z",
    credited_at: null,
    error_message: null,
    ...overrides,
  };
}

function creditsStatusPayload(overrides = {}) {
  return {
    balance_usdc_raw: 5_000_000,
    balance_usdc: 5,
    total_deposited_usdc_raw: 5_000_000,
    total_spent_usdc_raw: 0,
    ...overrides,
  };
}

function paymentRequiredPayload(overrides = {}) {
  return {
    x402Version: 1,
    resource: "/api/credits/x402",
    accepts: [
      {
        scheme: "exact",
        network: "solana:mainnet",
        amount: "1000000",
        asset: "usdc",
        payTo: "TreasuryAta11111111111111111111111111111111",
        maxTimeoutSeconds: 60,
        extra: {
          intent_id: "intent-1",
          reference_pubkey: "Reference11111111111111111111111111111111",
        },
      },
    ],
    ...overrides,
  };
}

function makeClient(fetchImpl, overrides = {}) {
  return new PipeStorageClient({
    apiKey: API_KEY,
    baseUrl: BASE,
    account: ACCOUNT,
    pollIntervalMs: 5, // fast polls for tests
    timeoutMs: 2000,
    fetchImpl,
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("deterministicUrl", () => {
  it("returns correct URL with valid hash and account", () => {
    const client = makeClient(mockFetch([]));
    const url = client.deterministicUrl(VALID_HASH);
    assert.equal(url, `${BASE}/${ACCOUNT}/${VALID_HASH}`);
  });

  it("accepts explicit account override", () => {
    const client = makeClient(mockFetch([]));
    const url = client.deterministicUrl(VALID_HASH, "OtherAcct");
    assert.equal(url, `${BASE}/OtherAcct/${VALID_HASH}`);
  });

  it("lowercases the hash", () => {
    const upper = VALID_HASH.toUpperCase();
    const client = makeClient(mockFetch([]));
    const url = client.deterministicUrl(upper);
    assert.equal(url, `${BASE}/${ACCOUNT}/${VALID_HASH}`);
  });

  it("throws when account is missing", () => {
    const client = makeClient(mockFetch([]), { account: undefined });
    assert.throws(
      () => client.deterministicUrl(VALID_HASH),
      /Missing account/,
    );
  });

  it("throws for invalid (short) hash", () => {
    const client = makeClient(mockFetch([]));
    assert.throws(() => client.deterministicUrl("abcdef"), /64-character hex/);
  });

  it("throws for non-hex 64 char string", () => {
    const client = makeClient(mockFetch([]));
    const nonHex = "z".repeat(64);
    assert.throws(() => client.deterministicUrl(nonHex), /64-character hex/);
  });
});

// ---------------------------------------------------------------------------
describe("store", () => {
  it("successful store with wait (poll to completed)", async () => {
    const fake = mockFetch([
      // 1) upload response
      {
        status: 200,
        headers: { "x-operation-id": "op-1", location: "/loc" },
        body: "ok",
      },
      // 2) first poll → queued
      { status: 200, json: queuedStatus() },
      // 3) second poll → completed
      { status: 200, json: completedStatus() },
    ]);
    const client = makeClient(fake);
    const result = await client.store("hello", {
      fileName: "agent/test.bin",
    });
    assert.equal(result.status, "completed");
    assert.equal(result.operationId, "op-1");
    assert.equal(result.contentHash, VALID_HASH);
    assert.equal(result.fileName, "agent/test.bin");
    // upload call + 2 poll calls = 3
    assert.equal(fake.calls.length, 3);
    // First call is POST to /upload
    assert.ok(fake.calls[0].url.includes("/upload?"));
  });

  it("store with wait=false returns immediately", async () => {
    const fake = mockFetch([
      {
        status: 200,
        headers: { "x-operation-id": "op-2" },
        body: "ok",
      },
    ]);
    const client = makeClient(fake);
    const result = await client.store("data", {
      fileName: "f.bin",
      wait: false,
    });
    assert.equal(result.status, "queued");
    assert.equal(result.operationId, "op-2");
    assert.equal(fake.calls.length, 1);
  });

  it("throws when API key is missing", async () => {
    const client = makeClient(mockFetch([]), { apiKey: undefined });
    await assert.rejects(() => client.store("data"), /Missing API key/);
  });

  it("tier=priority routes to /priorityUpload", async () => {
    const fake = mockFetch([
      { status: 200, headers: { "x-operation-id": "op-3" }, body: "ok" },
      { status: 200, json: completedStatus() },
    ]);
    const client = makeClient(fake);
    await client.store("data", {
      tier: "priority",
      fileName: "agent/test.bin",
    });
    assert.ok(
      fake.calls[0].url.includes("/priorityUpload"),
      `expected /priorityUpload in URL, got ${fake.calls[0].url}`,
    );
    assert.ok(fake.calls[0].url.includes("tier=priority"));
  });

  it("split-base mode falls back from /v1/upload to /upload when /v1 is unavailable", async () => {
    const fake = mockFetch([
      { status: 404, statusText: "Not Found", body: "missing /v1/upload" },
      { status: 200, headers: { "x-operation-id": "op-fallback" }, body: "ok" },
    ]);
    const client = makeClient(fake, {
      controlBaseUrl: BASE,
      dataBaseUrl: BASE,
    });
    const result = await client.store("data", {
      fileName: "agent/test.bin",
      wait: false,
    });
    assert.equal(result.operationId, "op-fallback");
    assert.equal(fake.calls.length, 2);
    assert.ok(fake.calls[0].url.includes("/v1/upload"));
    assert.ok(fake.calls[1].url.includes("/upload"));
  });

  it("default fileName starts with agent/", async () => {
    const fake = mockFetch([{ status: 200, body: "ok" }]);
    const client = makeClient(fake);
    const result = await client.store("data", { wait: false });
    assert.ok(
      result.fileName.startsWith("agent/"),
      `expected fileName to start with agent/, got: ${result.fileName}`,
    );
  });

  it("sends Authorization header", async () => {
    const fake = mockFetch([
      { status: 200, body: "ok" },
    ]);
    const client = makeClient(fake);
    await client.store("x", { wait: false });
    const authHeader = fake.calls[0].init.headers.get("authorization");
    assert.equal(authHeader, `Bearer ${API_KEY}`);
  });

  it("sends ApiKey authorization header in apiKey mode", async () => {
    const fake = mockFetch([{ status: 200, body: "ok" }]);
    const client = makeClient(fake, { authScheme: "apiKey" });
    await client.store("x", { wait: false });
    const authHeader = fake.calls[0].init.headers.get("authorization");
    assert.equal(authHeader, `ApiKey ${API_KEY}`);
  });
});

// ---------------------------------------------------------------------------
describe("checkStatus", () => {
  it("returns status on success", async () => {
    const fake = mockFetch([{ status: 200, json: completedStatus() }]);
    const client = makeClient(fake);
    const st = await client.checkStatus({ operationId: "op-1" });
    assert.equal(st.status, "completed");
    assert.equal(st.operation_id, "op-1");
    assert.ok(fake.calls[0].url.includes("operation_id=op-1"));
  });

  it("accepts fileName param", async () => {
    const fake = mockFetch([{ status: 200, json: completedStatus() }]);
    const client = makeClient(fake);
    await client.checkStatus({ fileName: "agent/test.bin" });
    assert.ok(fake.calls[0].url.includes("file_name="));
  });

  it("throws when no params given", async () => {
    const client = makeClient(mockFetch([]));
    await assert.rejects(
      () => client.checkStatus({}),
      /requires operationId or fileName/,
    );
  });

  it("throws PipeError on non-ok response", async () => {
    const fake = mockFetch([
      { status: 404, statusText: "Not Found", body: "not found" },
    ]);
    const client = makeClient(fake);
    try {
      await client.checkStatus({ operationId: "op-x" });
      assert.fail("expected throw");
    } catch (err) {
      assert.ok(err instanceof PipeError, `expected PipeError but got ${err?.constructor?.name}`);
      assert.equal(err.status, 404);
    }
  });
});

// ---------------------------------------------------------------------------
describe("waitForOperation", () => {
  it("completes on first poll", async () => {
    const fake = mockFetch([{ status: 200, json: completedStatus() }]);
    const client = makeClient(fake);
    const st = await client.waitForOperation("op-1");
    assert.equal(st.status, "completed");
    assert.equal(fake.calls.length, 1);
  });

  it("completes after 2 polls", async () => {
    const fake = mockFetch([
      { status: 200, json: queuedStatus() },
      { status: 200, json: completedStatus() },
    ]);
    const client = makeClient(fake);
    const st = await client.waitForOperation("op-1");
    assert.equal(st.status, "completed");
    assert.equal(fake.calls.length, 2);
  });

  it("throws PipeError on failed status", async () => {
    const fake = mockFetch([{ status: 200, json: failedStatus() }]);
    const client = makeClient(fake);
    try {
      await client.waitForOperation("op-1");
      assert.fail("expected throw");
    } catch (err) {
      assert.ok(err instanceof PipeError, `expected PipeError but got ${err?.constructor?.name}`);
      assert.equal(err.status, 409);
      assert.ok(err.message.includes("Upload failed"));
    }
  });

  it("times out when operation never completes", async () => {
    // Always return queued
    const fake = mockFetch([{ status: 200, json: queuedStatus() }]);
    const client = makeClient(fake, { timeoutMs: 50 });
    await assert.rejects(
      () => client.waitForOperation("op-1", { timeoutMs: 50 }),
      /Timed out/,
    );
  });

  it("retries transient 5xx errors", async () => {
    const fake = mockFetch([
      { status: 500, statusText: "Internal Server Error", body: "err" },
      { status: 200, json: completedStatus() },
    ]);
    const client = makeClient(fake);
    const st = await client.waitForOperation("op-1");
    assert.equal(st.status, "completed");
    assert.equal(fake.calls.length, 2);
  });

  it("does not retry 4xx errors", async () => {
    const fake = mockFetch([
      { status: 400, statusText: "Bad Request", body: "bad" },
    ]);
    const client = makeClient(fake);
    try {
      await client.waitForOperation("op-1");
      assert.fail("expected throw");
    } catch (err) {
      assert.ok(err instanceof PipeError, `expected PipeError but got ${err?.constructor?.name}`);
      assert.equal(err.status, 400);
    }
  });
});

// ---------------------------------------------------------------------------
describe("pin", () => {
  it("URL string passthrough", async () => {
    const client = makeClient(mockFetch([]));
    const result = await client.pin("https://example.com/file.bin");
    assert.equal(result.url, "https://example.com/file.bin");
  });

  it("hex hash becomes deterministic URL", async () => {
    const client = makeClient(mockFetch([]));
    const result = await client.pin(VALID_HASH);
    assert.equal(result.url, `${BASE}/${ACCOUNT}/${VALID_HASH}`);
    assert.equal(result.contentHash, VALID_HASH);
    assert.equal(result.status, "completed");
  });

  it("UUID triggers checkStatus delegation", async () => {
    const uuid = "12345678-1234-1234-8234-123456789abc";
    const fake = mockFetch([{ status: 200, json: completedStatus() }]);
    const client = makeClient(fake);
    const result = await client.pin(uuid);
    assert.ok(fake.calls[0].url.includes("operation_id=" + uuid));
    assert.equal(result.status, "completed");
    assert.ok(result.url);
  });

  it("file name triggers checkStatus delegation", async () => {
    const fake = mockFetch([{ status: 200, json: completedStatus() }]);
    const client = makeClient(fake);
    const result = await client.pin("agent/myfile.bin");
    assert.ok(fake.calls[0].url.includes("file_name="));
    assert.equal(result.status, "completed");
  });

  it("object with contentHash", async () => {
    const client = makeClient(mockFetch([]));
    const result = await client.pin({ contentHash: VALID_HASH });
    assert.equal(result.url, `${BASE}/${ACCOUNT}/${VALID_HASH}`);
    assert.equal(result.contentHash, VALID_HASH);
  });

  it("throws when status is not completed", async () => {
    const fake = mockFetch([{ status: 200, json: queuedStatus() }]);
    const client = makeClient(fake);
    await assert.rejects(
      () => client.pin({ operationId: "op-1" }),
      /Cannot pin object while status is queued/,
    );
  });
});

// ---------------------------------------------------------------------------
describe("fetch", () => {
  it("fetches bytes by default", async () => {
    const fake = mockFetch([
      { status: 200, body: "hello world" },
    ]);
    const client = makeClient(fake);
    const result = await client.fetch({ fileName: "agent/test.bin" });
    assert.ok(result instanceof Uint8Array);
    assert.equal(new TextDecoder().decode(result), "hello world");
  });

  it("fetches text with asText", async () => {
    const fake = mockFetch([
      { status: 200, body: "hello text" },
    ]);
    const client = makeClient(fake);
    const result = await client.fetch(
      { fileName: "agent/test.txt" },
      { asText: true },
    );
    assert.equal(result, "hello text");
  });

  it("fetches JSON with asJson", async () => {
    const obj = { key: "value", num: 42 };
    const fake = mockFetch([{ status: 200, json: obj }]);
    const client = makeClient(fake);
    const result = await client.fetch(
      { fileName: "agent/test.json" },
      { asJson: true },
    );
    assert.deepEqual(result, obj);
  });

  it("public deterministic URL does not require API key", async () => {
    const fake = mockFetch([{ status: 200, body: "pub data" }]);
    const client = makeClient(fake, { apiKey: undefined });
    const url = `${BASE}/${ACCOUNT}/${VALID_HASH}`;
    const result = await client.fetch(url, { asText: true });
    assert.equal(result, "pub data");
    // Should NOT have Authorization header
    const authHeader = fake.calls[0].init.headers.get("authorization");
    assert.equal(authHeader, null);
  });

  it("non-deterministic pipe URL without key throws", async () => {
    const client = makeClient(mockFetch([]), { apiKey: undefined });
    await assert.rejects(
      () => client.fetch({ fileName: "agent/test.bin" }),
      /Missing API key/,
    );
  });

  it("non-ok response throws PipeError", async () => {
    const fake = mockFetch([
      { status: 500, statusText: "Server Error", body: "boom" },
    ]);
    const client = makeClient(fake);
    try {
      await client.fetch({ fileName: "agent/test.bin" });
      assert.fail("expected throw");
    } catch (err) {
      assert.ok(err instanceof PipeError, `expected PipeError but got ${err?.constructor?.name}`);
      assert.equal(err.status, 500);
    }
  });

  it("fetch with full URL string", async () => {
    const fake = mockFetch([{ status: 200, body: "data" }]);
    const client = makeClient(fake, { apiKey: undefined });
    const result = await client.fetch("https://other.host/file", {
      asText: true,
    });
    assert.equal(result, "data");
    assert.equal(fake.calls[0].url, "https://other.host/file");
  });

  it("fetch with hex hash string resolves to deterministic URL", async () => {
    const fake = mockFetch([{ status: 200, body: "hash-data" }]);
    const client = makeClient(fake);
    await client.fetch(VALID_HASH, { asText: true });
    assert.equal(fake.calls[0].url, `${BASE}/${ACCOUNT}/${VALID_HASH}`);
  });

  it("fetch with contentHash object", async () => {
    const fake = mockFetch([{ status: 200, body: "obj-data" }]);
    const client = makeClient(fake);
    await client.fetch({ contentHash: VALID_HASH }, { asText: true });
    assert.equal(fake.calls[0].url, `${BASE}/${ACCOUNT}/${VALID_HASH}`);
  });
});

// ---------------------------------------------------------------------------
describe("delete", () => {
  it("deletes by file name string", async () => {
    const fake = mockFetch([
      { status: 200, json: { message: "deleted" } },
    ]);
    const client = makeClient(fake);
    const result = await client.delete("agent/test.bin");
    assert.equal(result.message, "deleted");
    // Should POST to /deleteFile
    assert.ok(fake.calls[0].url.includes("/deleteFile"));
    const body = JSON.parse(fake.calls[0].init.body);
    assert.equal(body.file_name, "agent/test.bin");
  });

  it("deletes by UUID (looks up via checkStatus first)", async () => {
    const uuid = "12345678-1234-1234-8234-123456789abc";
    const fake = mockFetch([
      // checkStatus response
      { status: 200, json: completedStatus({ file_name: "agent/looked-up.bin" }) },
      // deleteFile response
      { status: 200, json: { message: "deleted" } },
    ]);
    const client = makeClient(fake);
    const result = await client.delete(uuid);
    assert.equal(result.message, "deleted");
    // First call should be checkStatus
    assert.ok(fake.calls[0].url.includes("checkUploadStatus"));
    // Second call should be deleteFile with looked-up file name
    const body = JSON.parse(fake.calls[1].init.body);
    assert.equal(body.file_name, "agent/looked-up.bin");
  });

  it("deletes by object with fileName", async () => {
    const fake = mockFetch([
      { status: 200, json: { message: "deleted" } },
    ]);
    const client = makeClient(fake);
    await client.delete({ fileName: "agent/obj.bin" });
    const body = JSON.parse(fake.calls[0].init.body);
    assert.equal(body.file_name, "agent/obj.bin");
  });

  it("deletes by object with operationId", async () => {
    const fake = mockFetch([
      { status: 200, json: completedStatus({ file_name: "agent/op.bin" }) },
      { status: 200, json: { message: "deleted" } },
    ]);
    const client = makeClient(fake);
    await client.delete({ operationId: "op-1" });
    const body = JSON.parse(fake.calls[1].init.body);
    assert.equal(body.file_name, "agent/op.bin");
  });

  it("throws when no identifier provided", async () => {
    const client = makeClient(mockFetch([]));
    await assert.rejects(() => client.delete({}), /requires fileName or operationId/);
  });
});

// ---------------------------------------------------------------------------
describe("auth methods", () => {
  it("authChallenge returns nonce + message", async () => {
    const fake = mockFetch([
      { status: 200, json: { nonce: "abc123", message: "Sign this" } },
    ]);
    const client = makeClient(fake);
    const resp = await client.authChallenge("walletPubKey");
    assert.equal(resp.nonce, "abc123");
    assert.equal(resp.message, "Sign this");
    const body = JSON.parse(fake.calls[0].init.body);
    assert.equal(body.wallet_public_key, "walletPubKey");
  });

  it("authVerify sets apiKey and refreshToken internally", async () => {
    const verifyFake = mockFetch([
      // authVerify response
      {
        status: 200,
        json: {
          access_token: "new-access-token",
          refresh_token: "new-refresh-token",
        },
      },
      // store call should succeed with new token
      { status: 200, body: "ok" },
    ]);
    const client = makeClient(verifyFake, { apiKey: undefined });
    const session = await client.authVerify("wallet", "nonce1", "msg", "sig64");
    assert.equal(session.access_token, "new-access-token");
    // Now store should work with the new token
    await client.store("data", { wait: false });
    const authHeader = verifyFake.calls[1].init.headers.get("authorization");
    assert.equal(authHeader, "Bearer new-access-token");
  });

  it("authRefresh updates tokens", async () => {
    const fake = mockFetch([
      // authVerify to set initial tokens
      {
        status: 200,
        json: { access_token: "old-access", refresh_token: "old-refresh" },
      },
      // authRefresh response
      {
        status: 200,
        json: { access_token: "fresh-access", refresh_token: "fresh-refresh" },
      },
      // store to verify new token
      { status: 200, body: "ok" },
    ]);
    const client = makeClient(fake, { apiKey: undefined });
    await client.authVerify("w", "n", "m", "s");
    await client.authRefresh();
    await client.store("data", { wait: false });
    const authHeader = fake.calls[2].init.headers.get("authorization");
    assert.equal(authHeader, "Bearer fresh-access");
  });

  it("authRefresh without token throws", async () => {
    const client = makeClient(mockFetch([]));
    await assert.rejects(
      () => client.authRefresh(),
      /No refresh token available/,
    );
  });

  it("authRefresh keeps existing refresh token when response omits refresh_token", async () => {
    const fake = mockFetch([
      // authVerify to set initial tokens
      {
        status: 200,
        json: { access_token: "old-access", refresh_token: "old-refresh" },
      },
      // authRefresh response without refresh_token
      {
        status: 200,
        json: { access_token: "fresh-access" },
      },
      // second refresh call should still use old refresh token
      {
        status: 200,
        json: { access_token: "fresh-access-2" },
      },
    ]);
    const client = makeClient(fake, { apiKey: undefined });
    await client.authVerify("w", "n", "m", "s");
    const first = await client.authRefresh();
    const second = await client.authRefresh();
    assert.equal(first.refresh_token, "old-refresh");
    assert.equal(second.refresh_token, "old-refresh");
    assert.equal(second.access_token, "fresh-access-2");
    assert.ok(String(fake.calls[2].init.body).includes("\"refresh_token\":\"old-refresh\""));
  });

  it("authLogout clears tokens (subsequent store throws)", async () => {
    const fake = mockFetch([
      // authVerify
      {
        status: 200,
        json: { access_token: "tok", refresh_token: "rtok" },
      },
      // authLogout
      { status: 200, body: "" },
    ]);
    const client = makeClient(fake, { apiKey: undefined });
    await client.authVerify("w", "n", "m", "s");
    await client.authLogout();
    // Now store should fail
    await assert.rejects(() => client.store("data"), /Missing API key/);
  });
});

// ---------------------------------------------------------------------------
describe("auto-refresh on 401", () => {
  it("first request 401 -> refresh -> retry succeeds", async () => {
    const fake = mockFetch([
      // 1) initial authVerify to set refresh token
      {
        status: 200,
        json: { access_token: "old-tok", refresh_token: "r-tok" },
      },
      // 2) checkStatus returns 401
      { status: 401, statusText: "Unauthorized", body: "expired" },
      // 3) authRefresh
      {
        status: 200,
        json: { access_token: "new-tok", refresh_token: "r-tok-2" },
      },
      // 4) retry checkStatus succeeds
      { status: 200, json: completedStatus() },
    ]);
    const client = makeClient(fake, { apiKey: undefined });
    await client.authVerify("w", "n", "m", "s");
    const st = await client.checkStatus({ operationId: "op-1" });
    assert.equal(st.status, "completed");
    // Verify the retry used the new token
    const retryAuth = fake.calls[3].init.headers.get("authorization");
    assert.equal(retryAuth, "Bearer new-tok");
  });

  it("refresh deduplication: two concurrent 401s only trigger one refresh", async () => {
    let refreshCallCount = 0;
    const configs = [];
    // We'll use a custom fetch that tracks refresh calls
    const calls = [];
    let callIdx = 0;

    async function customFetch(url, init) {
      const idx = callIdx++;
      calls.push({ url, init });

      // authVerify call
      if (url.includes("/auth/siws/verify")) {
        return new Response(
          JSON.stringify({
            access_token: "old-tok",
            refresh_token: "r-tok",
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }

      // refresh call
      if (url.includes("/auth/refresh")) {
        refreshCallCount++;
        // Small delay to simulate network
        await new Promise((r) => setTimeout(r, 20));
        return new Response(
          JSON.stringify({
            access_token: "new-tok",
            refresh_token: "r-tok-2",
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }

      // checkUploadStatus — return 401 first time for each concurrent call,
      // then 200 on retry
      if (url.includes("checkUploadStatus")) {
        const auth = init.headers.get("authorization");
        if (auth === "Bearer old-tok") {
          return new Response("expired", { status: 401 });
        }
        return new Response(JSON.stringify(completedStatus()), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }

      return new Response("not found", { status: 404 });
    }

    const client = makeClient(customFetch, { apiKey: undefined });
    await client.authVerify("w", "n", "m", "s");

    // Fire two concurrent requests that will both get 401
    const [r1, r2] = await Promise.all([
      client.checkStatus({ operationId: "op-1" }),
      client.checkStatus({ operationId: "op-2" }),
    ]);
    assert.equal(r1.status, "completed");
    assert.equal(r2.status, "completed");
    // Only one refresh call should have been made
    assert.equal(refreshCallCount, 1, "expected exactly 1 refresh call");
  });

  it("does not auto-refresh in apiKey mode", async () => {
    const fake = mockFetch([
      { status: 401, statusText: "Unauthorized", body: "expired" },
    ]);
    const client = makeClient(fake, { authScheme: "apiKey" });
    await assert.rejects(
      () => client.checkStatus({ operationId: "op-1" }),
      (err) => err instanceof PipeError && err.status === 401,
    );
    assert.equal(fake.calls.length, 1);
  });
});

// ---------------------------------------------------------------------------
describe("x402", () => {
  it("requestCreditsX402 decodes Payment-Required header", async () => {
    const fake = mockFetch([
      {
        status: 402,
        headers: {
          "Payment-Required": encodeJsonToBase64(paymentRequiredPayload()),
        },
        json: { error: "payment required" },
      },
    ]);
    const client = makeClient(fake, { authScheme: "apiKey" });
    const required = await client.requestCreditsX402(1_000_000);
    assert.equal(required.accepts[0].extra.intent_id, "intent-1");
    assert.equal(required.accepts[0].network, "solana:mainnet");
  });

  it("requestCreditsX402 throws X402ConflictError on blocking intent", async () => {
    const fake = mockFetch([
      {
        status: 409,
        json: {
          error: "A credits payment is already processing for this intent.",
          intent: creditsIntentStatus({ status: "processing" }),
        },
      },
    ]);
    const client = makeClient(fake, { authScheme: "apiKey" });
    await assert.rejects(
      () => client.requestCreditsX402(1_000_000),
      (err) =>
        err instanceof X402ConflictError &&
        err.intent?.status === "processing" &&
        err.intent?.intent_id === "intent-1",
    );
  });

  it("requestCreditsX402 throws protocol error when header is missing", async () => {
    const fake = mockFetch([
      {
        status: 402,
        json: { error: "payment required" },
      },
    ]);
    const client = makeClient(fake, { authScheme: "apiKey" });
    await assert.rejects(
      () => client.requestCreditsX402(1_000_000),
      (err) =>
        err instanceof X402ProtocolError &&
        err.message.includes("Missing Payment-Required"),
    );
  });

  it("confirmCreditsX402 surfaces pending intent errors", async () => {
    const fake = mockFetch([
      {
        status: 400,
        json: creditsIntentStatus({
          status: "pending",
          error_message: "payment still pending",
        }),
      },
    ]);
    const client = makeClient(fake, { authScheme: "apiKey" });
    await assert.rejects(
      () =>
        client.confirmCreditsX402(1_000_000, {
          intent_id: "intent-1",
          tx_sig: "tx-sig-1",
        }),
      (err) =>
        err instanceof X402PendingIntentError &&
        err.intent.status === "pending" &&
        err.intent.error_message === "payment still pending",
    );
  });

  it("topUpCreditsX402 completes 402 -> pay -> 202 -> poll flow", async () => {
    const fake = mockFetch([
      {
        status: 402,
        headers: {
          "Payment-Required": encodeJsonToBase64(paymentRequiredPayload()),
        },
        json: { error: "payment required" },
      },
      {
        status: 202,
        headers: { "Retry-After": "0" },
        json: {
          intent_id: "intent-1",
          status: "processing",
          requested_usdc_raw: 1_000_000,
          detected_usdc_raw: 0,
          credited_usdc_raw: 0,
          balance_usdc_raw: 0,
          payment_tx_sig: "tx-sig-1",
          last_checked_at: "2026-01-01T00:00:00Z",
          error_message: null,
        },
      },
      {
        status: 200,
        json: creditsIntentStatus({
          status: "credited",
          detected_usdc_raw: 1_000_000,
          credited_usdc_raw: 1_000_000,
          payment_tx_sig: "tx-sig-1",
          credited_at: "2026-01-01T00:00:05Z",
        }),
      },
      {
        status: 200,
        json: creditsStatusPayload(),
      },
    ]);
    const client = makeClient(fake, {
      authScheme: "apiKey",
      pollIntervalMs: 1,
      timeoutMs: 100,
    });

    let seenPaymentContext = null;
    const result = await client.topUpCreditsX402(1_000_000, {
      async pay(payment) {
        seenPaymentContext = payment;
        return "tx-sig-1";
      },
      pollIntervalMs: 1,
      timeoutMs: 100,
    });

    assert.equal(seenPaymentContext.intentId, "intent-1");
    assert.equal(seenPaymentContext.network, "solana:mainnet");
    assert.equal(result.intent.status, "credited");
    assert.equal(result.credits.balance_usdc_raw, 5_000_000);
    assert.equal(fake.calls.length, 4);
    assert.ok(fake.calls[1].init.headers.get("payment-signature"));
  });
});

// ---------------------------------------------------------------------------
describe("request timeout", () => {
  it("AbortController fires on slow fetch", async () => {
    // fetchImpl that never resolves (waits forever)
    function neverResolve(_url, init) {
      return new Promise((_resolve, reject) => {
        if (init?.signal) {
          init.signal.addEventListener("abort", () => {
            reject(new DOMException("The operation was aborted.", "AbortError"));
          });
        }
      });
    }
    const client = makeClient(neverResolve, { timeoutMs: 50 });
    await assert.rejects(
      () => client.checkStatus({ operationId: "op-1" }),
      (err) => err.name === "AbortError",
    );
  });
});

// ---------------------------------------------------------------------------
describe("response size limit", () => {
  it("throws PipeError when response exceeds MAX_RESPONSE_BYTES", async () => {
    // MAX_RESPONSE_BYTES is 256 * 1024 * 1024 (256 MB). We can't actually allocate
    // that much in a test, so we mock the arrayBuffer to return a size-reporting object.
    // Instead, we'll create a custom fetch that returns a response whose arrayBuffer
    // is larger than the limit.
    const MAX = 256 * 1024 * 1024;

    function bigFetch(_url, _init) {
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: new Headers(),
        arrayBuffer() {
          // Return a buffer that reports > MAX size
          // We create a minimal ArrayBuffer but patch byteLength via a wrapper
          const fakeBuf = {
            byteLength: MAX + 1,
          };
          return Promise.resolve(fakeBuf);
        },
      });
    }

    const client = makeClient(bigFetch);
    try {
      await client.fetch({ fileName: "agent/big.bin" });
      assert.fail("expected throw");
    } catch (err) {
      assert.ok(err instanceof PipeError, `expected PipeError but got ${err?.constructor?.name}`);
      assert.ok(err.message.includes("maximum size"));
    }
  });
});

// ---------------------------------------------------------------------------
describe("PipeError", () => {
  it("fromResponse creates correct error", async () => {
    const response = new Response("error body text", {
      status: 503,
      statusText: "Service Unavailable",
    });
    const err = await PipeError.fromResponse(response);
    assert.ok(err instanceof PipeError);
    assert.ok(err instanceof Error);
    assert.equal(err.name, "PipeError");
    assert.equal(err.status, 503);
    assert.equal(err.body, "error body text");
    assert.ok(err.message.includes("503"));
    assert.ok(err.message.includes("Service Unavailable"));
  });

  it("status and body are preserved", () => {
    const err = new PipeError("test msg", 418, "teapot body");
    assert.equal(err.status, 418);
    assert.equal(err.body, "teapot body");
    assert.equal(err.message, "test msg");
  });

  it("fromResponse handles unreadable body", async () => {
    // Create a response whose text() throws
    const response = {
      status: 500,
      statusText: "Internal Server Error",
      text() {
        return Promise.reject(new Error("stream error"));
      },
    };
    const err = await PipeError.fromResponse(response);
    assert.equal(err.status, 500);
    assert.equal(err.body, "Internal Server Error");
  });
});
