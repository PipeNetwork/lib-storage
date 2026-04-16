import { PipeStorageClient } from "../src/index.js";

async function main() {
  const pipe = new PipeStorageClient({
    apiKey: process.env.PIPE_API_KEY,
    account: process.env.PIPE_ACCOUNT,
    authScheme: "apiKey",
  });

  const result = await pipe.topUpCreditsX402(1_000_000, {
    async pay(payment) {
      // Replace this with your own signer/wallet implementation.
      // The callback must return the Solana tx signature string.
      console.log("Paying x402 intent", {
        intentId: payment.intentId,
        network: payment.network,
        amount: payment.amount,
        asset: payment.asset,
        payTo: payment.payTo,
        referencePubkey: payment.referencePubkey,
      });
      throw new Error("Implement wallet payment and return tx signature");
    },
  });

  console.log("Credits intent:", result.intent.status);
  console.log("Balance (raw):", result.credits.balance_usdc_raw);
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
