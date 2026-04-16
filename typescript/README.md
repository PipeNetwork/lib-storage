# Pipe Agent Storage SDK (TypeScript)

TypeScript SDK for Pipe Storage with Solana wallet authentication. Works in Node.js, Deno, Cloudflare Workers, and browsers.

## Install

```bash
npm install ./typescript
```

## Auth (Sign In With Solana)

```ts
import fs from "node:fs";

import { Keypair } from "@solana/web3.js";
import { PipeStorageClient } from "@pipe-network/agent-storage";
import nacl from "tweetnacl";

const baseUrl = process.env.PIPE_BASE_URL ?? "https://us-west-01-firestarter.pipenetwork.com";
const pipe = new PipeStorageClient({ baseUrl });

// Example: load an existing Solana CLI-style keypair file.
const keypair = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync("keypair.json", "utf8"))),
);
const walletPublicKey = keypair.publicKey.toBase58();

// 1. Get challenge
const challenge = await pipe.authChallenge(walletPublicKey);

// 2. Sign challenge.message with the wallet
const messageBytes = new TextEncoder().encode(challenge.message);
const signature = nacl.sign.detached(messageBytes, keypair.secretKey);
const signatureB64 = Buffer.from(signature).toString("base64");

// 3. Verify — auto-sets credentials for all subsequent calls
const session = await pipe.authVerify(
  walletPublicKey,
  challenge.nonce,
  challenge.message,
  signatureB64,
);

// 4. Refresh when token expires (also auto-refreshes on 401)
await pipe.authRefresh();

// 5. Logout
await pipe.authLogout();
```

This is the **recommended auth path**.

After `authVerify(...)`, the same `PipeStorageClient` instance can immediately
make authenticated bearer-session calls.

Default behavior: `PipeStorageClient()` uses
`https://us-west-01-firestarter.pipenetwork.com` (production). Requests are real
and may incur usage cost.

Use a non-production host when needed:

```bash
export PIPE_BASE_URL="http://localhost:8080"
```

Optional: use a static API key:

```bash
export PIPE_API_KEY="<user_app_key_or_bearer_token>"
export PIPE_ACCOUNT="<user_id_or_public_slug>"
```

If your environment intentionally provisions a long-lived `user_app_key`, construct the
client in API-key mode so it sends `Authorization: ApiKey ...` instead of the
default bearer header:

```ts
const pipe = new PipeStorageClient({
  apiKey: process.env.PIPE_API_KEY,
  account: process.env.PIPE_ACCOUNT,
  authScheme: "apiKey",
});
```

Use this only when you explicitly want provisioned API-key mode.

## What the current examples assume

The shipped quickstarts and examples are runtime examples, not provisioning
examples.

They assume:

- you already have a Pipe account
- you already have `PIPE_API_KEY`
- you already know `PIPE_ACCOUNT`

They do **not**:

- create an account
- run SIWS and then export `user_app_key`
- write a `.env` file for you

If you need to bootstrap credentials first, use the SIWS methods above.

## Bootstrap to API-key mode with SIWS

If you want to start from a Solana wallet and then optionally switch to
API-key mode later, the pattern is:

```ts
import fs from "node:fs";

import { Keypair } from "@solana/web3.js";
import { PipeStorageClient } from "@pipe-network/agent-storage";
import nacl from "tweetnacl";

const baseUrl = process.env.PIPE_BASE_URL ?? "https://us-west-01-firestarter.pipenetwork.com";
const pipe = new PipeStorageClient({ baseUrl });

const keypair = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync("keypair.json", "utf8"))),
);
const walletPublicKey = keypair.publicKey.toBase58();
const challenge = await pipe.authChallenge(walletPublicKey);
const messageBytes = new TextEncoder().encode(challenge.message);
const signature = nacl.sign.detached(messageBytes, keypair.secretKey);
const signatureB64 = Buffer.from(signature).toString("base64");
const session = await pipe.authVerify(
  walletPublicKey,
  challenge.nonce,
  challenge.message,
  signatureB64,
);

// Optional: fetch the long-lived user_app_key only if you want API-key mode later.
const me = await fetch(`${baseUrl}/user/me`, {
  headers: { Authorization: `Bearer ${session.access_token}` },
}).then((r) => r.json());

console.log("user_app_key =", me.user_app_key);
```

## Agent x402 top-up

The SDK can orchestrate the x402 credits top-up loop for agentic workflows:

1. request `POST /api/credits/x402`
2. decode the `Payment-Required` header
3. hand the payment requirement to your wallet/signer callback
4. retry with `Payment-Signature`
5. poll the credits intent until it is credited

The SDK does not send the on-chain payment itself. You provide a `pay`
callback that returns the Solana transaction signature.

```ts
// Reuse the authenticated `pipe` client from the SIWS example above.
const result = await pipe.topUpCreditsX402(1_000_000, {
  async pay(payment) {
    const txSig = await sendUsdcTransfer({
      network: payment.network,
      amountRaw: payment.amount,
      mint: payment.asset,
      destination: payment.payTo,
      referencePubkey: payment.referencePubkey,
      intentId: payment.intentId,
    });
    return txSig;
  },
});

console.log(result.intent.status, result.credits.balance_usdc_raw);
```

Low-level methods are also available if you want to manage the loop yourself:

- `creditsStatus()`
- `creditsIntent(intentId)`
- `requestCreditsX402(amountUsdcRaw)`
- `confirmCreditsX402(amountUsdcRaw, paymentSignature)`

## Storage

```ts
// Reuse the authenticated `pipe` client from the SIWS example above.
const stored = await pipe.store({ hello: "world" }, { fileName: "agent/state.json" });
const pinned = await pipe.pin({ operationId: stored.operationId });
const data = await pipe.fetch(pinned.url, { asJson: true });
await pipe.delete("agent/state.json");
```

`fetch()` accepts deterministic URLs, 64-char hex hashes, file names, or objects with `url`/`fileName`/`contentHash`.

## Framework adapters

- OpenAI: `createOpenAIPipeTools`, `runOpenAIPipeTool`
- Anthropic: `createAnthropicPipeTools`, `runAnthropicPipeTool`
- Vercel AI SDK: `createVercelPipeTools`
- Cloudflare AI Workflows: `createCloudflarePipeTools`, `runCloudflarePipeTool`
- LangChain: `PipeStorageLangChainTool`
- LlamaIndex: `createLlamaIndexPipeTools`

Examples: [`examples/`](./examples/)

- x402 top-up: [`examples/x402-topup.ts`](./examples/x402-topup.ts)

## Tests

```bash
# Unit tests (offline)
npm run build && node --test tests/test_client.mjs

# Integration test (requires PIPE_API_KEY)
npm run test:integration
```

## Benchmark

```bash
npm run bench:agent
```
