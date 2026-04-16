# Pipe Agent Storage SDK (Python)

Zero-dependency Python SDK for Pipe Storage with Solana wallet authentication.

## Install

```bash
pip install ./python
```

## Auth (Sign In With Solana)

```python
import base64
import json
import os

from solders.keypair import Keypair

from pipe_storage import PipeStorage

base_url = os.getenv(
    "PIPE_BASE_URL",
    "https://us-west-01-firestarter.pipenetwork.com",
)
pipe = PipeStorage(base_url=base_url)

# Example: load an existing Solana CLI-style keypair file.
keypair = Keypair.from_bytes(bytes(json.load(open("keypair.json"))))
wallet_public_key = str(keypair.pubkey())

# 1. Get challenge
challenge = pipe.auth_challenge(wallet_public_key)

# 2. Sign challenge.message with the wallet
signature = keypair.sign_message(challenge.message.encode("utf-8"))
signature_b64 = base64.b64encode(bytes(signature)).decode("utf-8")

# 3. Verify — auto-sets credentials for all subsequent calls
session = pipe.auth_verify(
    wallet_public_key,
    challenge.nonce,
    challenge.message,
    signature_b64,
)

# 4. Refresh when token expires (also auto-refreshes on 401)
pipe.auth_refresh()

# 5. Logout
pipe.auth_logout()
```

This is the **recommended auth path**.

After `auth_verify(...)`, the same `PipeStorage` instance can immediately make
authenticated bearer-session calls.

Default behavior: `PipeStorage()` uses
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

```python
pipe = PipeStorage(
    api_key=os.environ["PIPE_API_KEY"],
    account=os.environ["PIPE_ACCOUNT"],
    auth_scheme="api_key",
)
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

```python
import base64
import json
import os

import requests
from solders.keypair import Keypair
from pipe_storage import PipeStorage

base_url = os.getenv(
    "PIPE_BASE_URL",
    "https://us-west-01-firestarter.pipenetwork.com",
)
pipe = PipeStorage(base_url=base_url)

keypair = Keypair.from_bytes(bytes(json.load(open("keypair.json"))))
wallet_public_key = str(keypair.pubkey())
challenge = pipe.auth_challenge(wallet_public_key)
signature = keypair.sign_message(challenge.message.encode("utf-8"))
signature_b64 = base64.b64encode(bytes(signature)).decode("utf-8")
session = pipe.auth_verify(
    wallet_public_key,
    challenge.nonce,
    challenge.message,
    signature_b64,
)

# Optional: fetch the long-lived user_app_key only if you want API-key mode later.
me = requests.get(
    f"{base_url}/user/me",
    headers={"Authorization": f"Bearer {session.access_token}"},
).json()

print("user_app_key =", me["user_app_key"])
```

## Agent x402 top-up

The SDK can orchestrate the x402 credits top-up flow for agent workflows:

1. request `POST /api/credits/x402`
2. decode the `Payment-Required` payload
3. call your signer/wallet callback
4. retry with `Payment-Signature`
5. poll the credits intent until it is credited

The SDK does not broadcast the Solana transaction itself. You supply a `pay`
callback that returns the Solana transaction signature.

```python
# Reuse the authenticated `pipe` client from the SIWS example above.
result = pipe.top_up_credits_x402(
    1_000_000,
    pay=lambda payment: send_usdc_transfer(
        network=payment.network,
        amount_raw=payment.amount,
        mint=payment.asset,
        destination=payment.pay_to,
        reference_pubkey=payment.reference_pubkey,
        intent_id=payment.intent_id,
    ),
)

print(result.intent.status, result.credits.balance_usdc_raw)
```

Low-level methods are also available if you want to control the loop yourself:

- `credits_status()`
- `credits_intent(intent_id)`
- `request_credits_x402(amount_usdc_raw)`
- `confirm_credits_x402(amount_usdc_raw, payment_signature)`

## Storage

```python
# Reuse the authenticated `pipe` client from the SIWS example above.
stored = pipe.store({"hello": "world"}, file_name="agent/state.json")
pinned = pipe.pin({"operation_id": stored["operation_id"]})
content = pipe.fetch(pinned["url"], as_json=True)
pipe.delete("agent/state.json")
```

`fetch()` accepts deterministic URLs, 64-char hex hashes, file names, or dicts with `url`/`file_name`/`content_hash`.

## Framework adapters

- OpenAI: `openai_pipe_tools`, `run_openai_pipe_tool`
- Anthropic: `anthropic_pipe_tools`, `run_anthropic_pipe_tool`
- AutoGen: `autogen_pipe_tool_schemas`, `autogen_pipe_function_map`
- CrewAI: `CrewAIPipeTool`, `crewai_pipe_tools`
- LangChain: `PipeStorageLangChainTool`
- LlamaIndex: `llamaindex_pipe_tools`

Examples: [`examples/`](./examples/)

- x402 top-up: [`examples/x402_topup.py`](./examples/x402_topup.py)

## Tests

```bash
# Unit tests (offline)
python3 -m unittest python.tests.test_client -v

# Integration-flow test (mocked, offline)
python3 ./python/tests/integration_test.py
```
