# Pipe Agent Storage SDK (TypeScript)

TypeScript SDK for Pipe Storage with Solana wallet authentication. Works in Node.js, Deno, Cloudflare Workers, and browsers.

## Install

```bash
npm install ./typescript
```

## Auth (Sign In With Solana)

```ts
import { PipeStorageClient } from "@pipe-network/agent-storage";

const pipe = new PipeStorageClient();

// 1. Get challenge
const challenge = await pipe.authChallenge("Base58WalletPubkey...");

// 2. Sign challenge.message with your Solana wallet (external)
const signatureB64 = await signWithWallet(challenge.message);

// 3. Verify — auto-sets credentials for all subsequent calls
const session = await pipe.authVerify("Base58WalletPubkey...", challenge.nonce, challenge.message, signatureB64);

// 4. Refresh when token expires (also auto-refreshes on 401)
await pipe.authRefresh();

// 5. Logout
await pipe.authLogout();
```

Default behavior: `PipeStorageClient()` uses
`https://us-west-01-firestarter.pipenetwork.com` (production). Requests are real
and may incur usage cost.

Use a non-production host when needed:

```bash
export PIPE_BASE_URL="http://localhost:8080"
```

Or use a static API key:

```bash
export PIPE_API_KEY="<your_jwt_or_api_token>"
export PIPE_ACCOUNT="<user_id_or_public_slug>"
```

If you are using a long-lived `user_app_key` for headless agents, construct the
client in API-key mode so it sends `Authorization: ApiKey ...` instead of the
default bearer header:

```ts
const pipe = new PipeStorageClient({
  apiKey: process.env.PIPE_API_KEY,
  account: process.env.PIPE_ACCOUNT,
  authScheme: "apiKey",
});
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
import { PipeStorageClient } from "@pipe-network/agent-storage";

const pipe = new PipeStorageClient({
  apiKey: process.env.PIPE_API_KEY,
  account: process.env.PIPE_ACCOUNT,
  authScheme: "apiKey",
});

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
import { PipeStorageClient } from "@pipe-network/agent-storage";

const pipe = new PipeStorageClient();

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
