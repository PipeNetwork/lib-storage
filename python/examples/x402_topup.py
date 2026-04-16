import os

from pipe_storage import PipeStorage


def main() -> None:
    pipe = PipeStorage(
        api_key=os.environ["PIPE_API_KEY"],
        account=os.environ["PIPE_ACCOUNT"],
        auth_scheme="api_key",
    )

    result = pipe.top_up_credits_x402(
        1_000_000,
        pay=lambda payment: _pay_with_wallet(payment),
    )

    print("Credits intent:", result.intent.status)
    print("Balance (raw):", result.credits.balance_usdc_raw)


def _pay_with_wallet(payment) -> str:
    # Replace this with your own signer/wallet implementation.
    # The callback must return the Solana tx signature string.
    print(
        "Paying x402 intent",
        {
            "intent_id": payment.intent_id,
            "network": payment.network,
            "amount": payment.amount,
            "asset": payment.asset,
            "pay_to": payment.pay_to,
            "reference_pubkey": payment.reference_pubkey,
        },
    )
    raise RuntimeError("Implement wallet payment and return tx signature")


if __name__ == "__main__":
    main()
