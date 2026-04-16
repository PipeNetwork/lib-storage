"""Comprehensive unit tests for PipeStorage client.

Uses unittest only.  HTTP is mocked by replacing ``_request_absolute``
on each client instance -- no ``unittest.mock`` required.
"""

from __future__ import annotations

import base64
import inspect
import json
import threading
import time
import unittest

# Adjust the import path so the test works both when run from the repo root
# (``python -m unittest python.tests.test_client``) **and** when the package
# is installed (``python -m pytest python/tests/test_client.py``).
import sys, os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from pipe_storage.client import (
    CreditsIntentStatus,
    CreditsStatus,
    DEFAULT_BASE_URL,
    MAX_RESPONSE_BYTES,
    PipeError,
    PipeStorage,
    UploadStatus,
    ChallengeResponse,
    AuthSession,
    X402ConflictError,
    X402PendingIntentError,
    X402ProtocolError,
    _is_hex_hash,
    _is_uuid,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

VALID_HEX_HASH = "a" * 64
VALID_UUID = "12345678-1234-1234-a234-123456789abc"
TEST_API_KEY = "test-api-key-000"
TEST_ACCOUNT = "test-account"
TEST_BASE = "https://test.pipe.example.com"


def _make_response(
    status: int = 200,
    headers: dict | None = None,
    body: bytes | str | dict | None = b"",
) -> dict:
    """Build a dict in the same shape ``_request_absolute`` returns."""
    if isinstance(body, dict):
        body = json.dumps(body).encode("utf-8")
    elif isinstance(body, str):
        body = body.encode("utf-8")
    return {
        "status": status,
        "headers": {k.lower(): v for k, v in (headers or {}).items()},
        "body": body or b"",
    }


def _b64_json(value: dict) -> str:
    return base64.b64encode(json.dumps(value).encode("utf-8")).decode("utf-8")


def _patch_request(client: PipeStorage, handler):
    """Replace ``_request_absolute`` with *handler(method, url, *, headers, body)*."""
    original = client._request_absolute
    signature = inspect.signature(handler)
    supports_allowed_statuses = (
        "allowed_statuses" in signature.parameters
        or any(
            parameter.kind == inspect.Parameter.VAR_KEYWORD
            for parameter in signature.parameters.values()
        )
    )

    def wrapper(method, url, *, headers=None, body=None, allowed_statuses=None):
        kwargs = {"headers": headers, "body": body}
        if supports_allowed_statuses:
            kwargs["allowed_statuses"] = allowed_statuses
        return handler(method, url, **kwargs)

    client._request_absolute = wrapper
    return original


def _make_client(**kwargs) -> PipeStorage:
    """Return a PipeStorage with sensible test defaults and env vars cleared."""
    defaults = dict(
        api_key=TEST_API_KEY,
        base_url=TEST_BASE,
        account=TEST_ACCOUNT,
        timeout_sec=5,
        poll_interval_sec=0.01,
    )
    defaults.update(kwargs)
    return PipeStorage(**defaults)


def _completed_status_payload(**overrides) -> dict:
    base = dict(
        operation_id="op-1",
        file_name="agent/test.bin",
        status="completed",
        finished=True,
        parts_completed=1,
        total_parts=1,
        content_hash=VALID_HEX_HASH,
        deterministic_url=f"{TEST_BASE}/{TEST_ACCOUNT}/{VALID_HEX_HASH}",
        bytes_total=100,
        bytes_uploaded=100,
        created_at="2025-01-01T00:00:00Z",
        updated_at="2025-01-01T00:00:01Z",
    )
    base.update(overrides)
    return base


def _credits_intent_status_payload(**overrides) -> dict:
    base = dict(
        intent_id="intent-1",
        status="pending",
        requested_usdc_raw=1_000_000,
        detected_usdc_raw=0,
        credited_usdc_raw=0,
        usdc_mint="EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        treasury_owner_pubkey="TreasuryOwner111111111111111111111111111111",
        treasury_usdc_ata="TreasuryAta11111111111111111111111111111111",
        reference_pubkey="Reference11111111111111111111111111111111",
        payment_tx_sig=None,
        last_checked_at="2026-01-01T00:00:00Z",
        credited_at=None,
        error_message=None,
    )
    base.update(overrides)
    return base


def _credits_status_payload(**overrides) -> dict:
    base = dict(
        balance_usdc_raw=5_000_000,
        balance_usdc=5,
        total_deposited_usdc_raw=5_000_000,
        total_spent_usdc_raw=0,
    )
    base.update(overrides)
    return base


def _payment_required_payload(**overrides) -> dict:
    base = dict(
        x402Version=1,
        resource="/api/credits/x402",
        accepts=[
            {
                "scheme": "exact",
                "network": "solana:mainnet",
                "amount": "1000000",
                "asset": "usdc",
                "payTo": "TreasuryAta11111111111111111111111111111111",
                "maxTimeoutSeconds": 60,
                "extra": {
                    "intent_id": "intent-1",
                    "reference_pubkey": "Reference11111111111111111111111111111111",
                },
            }
        ],
    )
    base.update(overrides)
    return base


# ===================================================================
# 1. Input validation / URL construction
# ===================================================================


class TestIsHexHash(unittest.TestCase):
    def test_valid_64_char_hex(self):
        self.assertTrue(_is_hex_hash("a" * 64))
        self.assertTrue(_is_hex_hash("A" * 64))
        self.assertTrue(_is_hex_hash("0123456789abcdef" * 4))

    def test_too_short(self):
        self.assertFalse(_is_hex_hash("a" * 63))

    def test_too_long(self):
        self.assertFalse(_is_hex_hash("a" * 65))

    def test_non_hex_chars(self):
        self.assertFalse(_is_hex_hash("g" * 64))
        self.assertFalse(_is_hex_hash("z" * 64))

    def test_empty(self):
        self.assertFalse(_is_hex_hash(""))


class TestIsUuid(unittest.TestCase):
    def test_valid(self):
        self.assertTrue(_is_uuid(VALID_UUID))

    def test_uppercase(self):
        self.assertTrue(_is_uuid(VALID_UUID.upper()))

    def test_missing_dashes(self):
        self.assertFalse(_is_uuid(VALID_UUID.replace("-", "")))

    def test_wrong_version(self):
        # version digit position (char 14) must be 1-5
        bad = "12345678-1234-6234-a234-123456789abc"
        self.assertFalse(_is_uuid(bad))

    def test_wrong_variant(self):
        # variant digit (char 19) must be 8,9,a,b
        bad = "12345678-1234-1234-0234-123456789abc"
        self.assertFalse(_is_uuid(bad))

    def test_empty(self):
        self.assertFalse(_is_uuid(""))


class TestDeterministicUrl(unittest.TestCase):
    def test_valid_hash_and_account(self):
        c = _make_client()
        url = c.deterministic_url(VALID_HEX_HASH)
        self.assertEqual(url, f"{TEST_BASE}/{TEST_ACCOUNT}/{VALID_HEX_HASH}")

    def test_override_account(self):
        c = _make_client()
        url = c.deterministic_url(VALID_HEX_HASH, account="other")
        self.assertIn("/other/", url)

    def test_missing_account(self):
        c = _make_client(account=None)
        # also clear env var
        old = os.environ.pop("PIPE_ACCOUNT", None)
        try:
            with self.assertRaises(ValueError):
                c.deterministic_url(VALID_HEX_HASH)
        finally:
            if old is not None:
                os.environ["PIPE_ACCOUNT"] = old

    def test_invalid_hash(self):
        c = _make_client()
        with self.assertRaises(ValueError):
            c.deterministic_url("not-a-hash")

    def test_hash_lowered(self):
        c = _make_client()
        url = c.deterministic_url("A" * 64)
        self.assertTrue(url.endswith("a" * 64))


class TestResolveFetchUrl(unittest.TestCase):
    def test_url_passthrough(self):
        c = _make_client()
        self.assertEqual(c._resolve_fetch_url("https://example.com/f"), "https://example.com/f")

    def test_hex_hash(self):
        c = _make_client()
        url = c._resolve_fetch_url(VALID_HEX_HASH)
        self.assertEqual(url, c.deterministic_url(VALID_HEX_HASH))

    def test_file_name(self):
        c = _make_client()
        url = c._resolve_fetch_url("agent/test.bin")
        self.assertIn("/download-stream?file_name=", url)
        self.assertIn("agent", url)

    def test_dict_with_url(self):
        c = _make_client()
        self.assertEqual(c._resolve_fetch_url({"url": "https://x.com/z"}), "https://x.com/z")

    def test_dict_with_file_name(self):
        c = _make_client()
        url = c._resolve_fetch_url({"file_name": "myfile"})
        self.assertIn("download-stream", url)

    def test_dict_with_content_hash(self):
        c = _make_client()
        url = c._resolve_fetch_url({"content_hash": VALID_HEX_HASH})
        self.assertEqual(url, c.deterministic_url(VALID_HEX_HASH))

    def test_dict_empty_raises(self):
        c = _make_client()
        with self.assertRaises(ValueError):
            c._resolve_fetch_url({})


class TestIsPublicDeterministicUrl(unittest.TestCase):
    def test_matching(self):
        c = _make_client()
        url = f"{TEST_BASE}/acct/{VALID_HEX_HASH}"
        self.assertTrue(c._is_public_deterministic_url(url))

    def test_non_matching_wrong_host(self):
        c = _make_client()
        self.assertFalse(c._is_public_deterministic_url(f"https://other.com/acct/{VALID_HEX_HASH}"))

    def test_non_matching_too_few_segments(self):
        c = _make_client()
        self.assertFalse(c._is_public_deterministic_url(f"{TEST_BASE}/{VALID_HEX_HASH}"))

    def test_non_matching_not_hex(self):
        c = _make_client()
        self.assertFalse(c._is_public_deterministic_url(f"{TEST_BASE}/acct/nothex"))


# ===================================================================
# 2. store()
# ===================================================================


class TestStore(unittest.TestCase):
    def test_store_wait_true(self):
        """Mock returns operation_id header, then completed status on poll."""
        c = _make_client()
        call_count = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            call_count["n"] += 1
            if "/upload" in url and "checkUploadStatus" not in url:
                return _make_response(
                    202,
                    headers={"x-operation-id": "op-1", "Location": f"{TEST_BASE}/dl"},
                    body=b"",
                )
            # checkUploadStatus
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        result = c.store(b"hello", file_name="agent/t.bin", wait=True)
        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["operation_id"], "op-1")
        self.assertIsNotNone(result.get("content_hash"))

    def test_store_wait_false(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                202,
                headers={"x-operation-id": "op-2", "Location": f"{TEST_BASE}/dl"},
            )

        _patch_request(c, handler)
        result = c.store(b"x", file_name="agent/x.bin", wait=False)
        self.assertEqual(result["status"], "queued")
        self.assertEqual(result["operation_id"], "op-2")

    def test_store_missing_api_key(self):
        c = _make_client(api_key=None)
        with self.assertRaises(ValueError):
            c.store(b"data")

    def test_store_default_file_name(self):
        c = _make_client()
        generated_name = None

        def handler(method, url, *, headers=None, body=None):
            nonlocal generated_name
            if "/upload" in url and "checkUploadStatus" not in url:
                # Extract file_name from query string
                from urllib.parse import urlparse, parse_qs

                qs = parse_qs(urlparse(url).query)
                generated_name = qs.get("file_name", [None])[0]
                return _make_response(200, body=b"")
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        c.store(b"data", wait=False)
        self.assertIsNotNone(generated_name)
        self.assertTrue(generated_name.startswith("agent/"))

    def test_store_priority_tier_endpoint(self):
        c = _make_client()
        captured_url = {}

        def handler(method, url, *, headers=None, body=None):
            captured_url["url"] = url
            return _make_response(200, body=b"")

        _patch_request(c, handler)
        c.store(b"data", file_name="f.bin", tier="priority", wait=False)
        self.assertIn("/priorityUpload", captured_url["url"])
        self.assertIn("tier=priority", captured_url["url"])

    def test_store_normal_tier_no_tier_param(self):
        c = _make_client()
        captured_url = {}

        def handler(method, url, *, headers=None, body=None):
            captured_url["url"] = url
            return _make_response(200, body=b"")

        _patch_request(c, handler)
        c.store(b"data", file_name="f.bin", tier="normal", wait=False)
        self.assertIn("/upload", captured_url["url"])
        self.assertNotIn("tier=", captured_url["url"])

    def test_store_uses_apikey_authorization_in_api_key_mode(self):
        c = _make_client(auth_scheme="api_key")
        captured_headers = {}

        def handler(method, url, *, headers=None, body=None):
            captured_headers.update(headers or {})
            return _make_response(202, headers={"x-operation-id": "op-auth"})

        _patch_request(c, handler)
        c.store(b"data", file_name="f.bin", wait=False)
        auth = captured_headers.get("Authorization") or captured_headers.get("authorization")
        self.assertEqual(auth, f"ApiKey {TEST_API_KEY}")

    def test_store_split_mode_falls_back_to_upload_when_v1_missing(self):
        c = _make_client(control_base_url=TEST_BASE, data_base_url=TEST_BASE)
        calls: list[str] = []

        def handler(method, url, *, headers=None, body=None):
            calls.append(url)
            if "/v1/upload" in url:
                raise PipeError("Pipe API request failed (404 Not Found)", 404, "missing /v1/upload")
            return _make_response(202, headers={"x-operation-id": "op-fallback"})

        _patch_request(c, handler)
        result = c.store(b"data", file_name="f.bin", tier="normal", wait=False)
        self.assertEqual(result["operation_id"], "op-fallback")
        self.assertEqual(len(calls), 2)
        self.assertIn("/v1/upload", calls[0])
        self.assertIn("/upload", calls[1])


# ===================================================================
# 3. check_status()
# ===================================================================


class TestCheckStatus(unittest.TestCase):
    def test_successful_response(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        s = c.check_status(operation_id="op-1")
        self.assertIsInstance(s, UploadStatus)
        self.assertEqual(s.status, "completed")
        self.assertTrue(s.finished)

    def test_extra_fields_filtered(self):
        c = _make_client()
        payload = _completed_status_payload()
        payload["unknown_field_xyz"] = "should-not-crash"

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=payload)

        _patch_request(c, handler)
        s = c.check_status(operation_id="op-1")
        self.assertFalse(hasattr(s, "unknown_field_xyz"))

    def test_missing_args(self):
        c = _make_client()
        with self.assertRaises(ValueError):
            c.check_status()

    def test_missing_api_key(self):
        c = _make_client(api_key=None)
        with self.assertRaises(ValueError):
            c.check_status(operation_id="op-1")


# ===================================================================
# 4. wait_for_operation()
# ===================================================================


class TestWaitForOperation(unittest.TestCase):
    def test_completes_first_poll(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        s = c.wait_for_operation("op-1")
        self.assertEqual(s.status, "completed")

    def test_completes_after_multiple_polls(self):
        c = _make_client()
        calls = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            if calls["n"] < 3:
                return _make_response(
                    200,
                    body=_completed_status_payload(status="in_progress", finished=False),
                )
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        s = c.wait_for_operation("op-1")
        self.assertEqual(s.status, "completed")
        self.assertGreaterEqual(calls["n"], 3)

    def test_failed_status_raises_pipe_error(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body=_completed_status_payload(
                    status="failed",
                    finished=True,
                    error="disk full",
                ),
            )

        _patch_request(c, handler)
        with self.assertRaises(PipeError) as ctx:
            c.wait_for_operation("op-1")
        self.assertEqual(ctx.exception.status, 409)
        self.assertIn("disk full", str(ctx.exception))

    def test_timeout_raises(self):
        c = _make_client(timeout_sec=0.05, poll_interval_sec=0.01)

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body=_completed_status_payload(status="in_progress", finished=False),
            )

        _patch_request(c, handler)
        with self.assertRaises(TimeoutError):
            c.wait_for_operation("op-1", timeout_sec=0.05)

    def test_transient_5xx_retried(self):
        c = _make_client()
        calls = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            if calls["n"] <= 2:
                raise PipeError("server error", 503, "service unavailable")
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        s = c.wait_for_operation("op-1")
        self.assertEqual(s.status, "completed")
        self.assertEqual(calls["n"], 3)

    def test_4xx_not_retried(self):
        c = _make_client()
        calls = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            raise PipeError("not found", 404, "not found")

        _patch_request(c, handler)
        with self.assertRaises(PipeError) as ctx:
            c.wait_for_operation("op-1")
        self.assertEqual(ctx.exception.status, 404)
        # Should fail immediately, not retry
        self.assertEqual(calls["n"], 1)

    def test_5xx_exhausts_retries(self):
        c = _make_client()
        calls = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            raise PipeError("server error", 500, "internal")

        _patch_request(c, handler)
        with self.assertRaises(PipeError):
            c.wait_for_operation("op-1")
        self.assertEqual(calls["n"], 3)


# ===================================================================
# 5. pin()
# ===================================================================


class TestPin(unittest.TestCase):
    def test_url_passthrough(self):
        c = _make_client()
        result = c.pin("https://example.com/file")
        self.assertEqual(result["url"], "https://example.com/file")

    def test_hex_hash(self):
        c = _make_client()
        result = c.pin(VALID_HEX_HASH)
        self.assertEqual(result["content_hash"], VALID_HEX_HASH.lower())
        self.assertEqual(result["status"], "completed")
        self.assertIn(VALID_HEX_HASH, result["url"])

    def test_uuid_delegates_to_check_status(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        result = c.pin(VALID_UUID)
        self.assertEqual(result["status"], "completed")
        self.assertIn("operation_id", result)

    def test_file_name_delegates(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        result = c.pin("agent/test.bin")
        self.assertEqual(result["status"], "completed")

    def test_dict_with_content_hash(self):
        c = _make_client()
        result = c.pin({"content_hash": VALID_HEX_HASH})
        self.assertEqual(result["content_hash"], VALID_HEX_HASH.lower())
        self.assertIn(VALID_HEX_HASH, result["url"])

    def test_dict_with_operation_id_completed(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        result = c.pin({"operation_id": "op-1"})
        self.assertEqual(result["status"], "completed")

    def test_not_completed_raises(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body=_completed_status_payload(status="in_progress", finished=False),
            )

        _patch_request(c, handler)
        with self.assertRaises(RuntimeError):
            c.pin({"operation_id": "op-1"})

    def test_completed_no_url_no_hash_raises(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body=_completed_status_payload(
                    content_hash=None,
                    deterministic_url=None,
                ),
            )

        _patch_request(c, handler)
        with self.assertRaises(RuntimeError):
            c.pin({"operation_id": "op-1"})


# ===================================================================
# 6. fetch()
# ===================================================================


class TestFetch(unittest.TestCase):
    def test_fetch_bytes(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=b"\x00\x01\x02")

        _patch_request(c, handler)
        result = c.fetch("https://example.com/f")
        self.assertEqual(result, b"\x00\x01\x02")

    def test_fetch_text(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body="hello world")

        _patch_request(c, handler)
        result = c.fetch("https://example.com/f", as_text=True)
        self.assertEqual(result, "hello world")
        self.assertIsInstance(result, str)

    def test_fetch_json(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body={"key": "value"})

        _patch_request(c, handler)
        result = c.fetch("https://example.com/f", as_json=True)
        self.assertEqual(result, {"key": "value"})

    def test_fetch_pipe_url_without_key_raises(self):
        c = _make_client(api_key=None)
        # A pipe URL that is NOT a deterministic URL
        url = f"{TEST_BASE}/download-stream?file_name=x"

        def handler(method, url_arg, *, headers=None, body=None):
            return _make_response(200, body=b"data")

        _patch_request(c, handler)
        with self.assertRaises(ValueError):
            c.fetch(url)

    def test_fetch_public_deterministic_without_key(self):
        c = _make_client(api_key=None)
        det_url = f"{TEST_BASE}/{TEST_ACCOUNT}/{VALID_HEX_HASH}"

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=b"public data")

        _patch_request(c, handler)
        # Should NOT raise -- deterministic URLs are public
        result = c.fetch(det_url)
        self.assertEqual(result, b"public data")


# ===================================================================
# 7. delete()
# ===================================================================


class TestDelete(unittest.TestCase):
    def test_delete_by_file_name(self):
        c = _make_client()
        captured = {}

        def handler(method, url, *, headers=None, body=None):
            if "/deleteFile" in url:
                captured["body"] = json.loads(body) if body else None
                return _make_response(200, body={"deleted": True})
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        result = c.delete("agent/test.bin")
        self.assertEqual(captured["body"]["file_name"], "agent/test.bin")
        self.assertTrue(result["deleted"])

    def test_delete_by_uuid(self):
        c = _make_client()
        captured = {}

        def handler(method, url, *, headers=None, body=None):
            if "checkUploadStatus" in url:
                return _make_response(200, body=_completed_status_payload(file_name="agent/uuid-file.bin"))
            if "/deleteFile" in url:
                captured["body"] = json.loads(body) if body else None
                return _make_response(200, body={"deleted": True})
            return _make_response(200, body=b"")

        _patch_request(c, handler)
        result = c.delete(VALID_UUID)
        self.assertEqual(captured["body"]["file_name"], "agent/uuid-file.bin")

    def test_delete_by_dict_with_operation_id(self):
        c = _make_client()
        captured = {}

        def handler(method, url, *, headers=None, body=None):
            if "checkUploadStatus" in url:
                return _make_response(200, body=_completed_status_payload(file_name="agent/dict-file.bin"))
            if "/deleteFile" in url:
                captured["body"] = json.loads(body) if body else None
                return _make_response(200, body={"deleted": True})
            return _make_response(200, body=b"")

        _patch_request(c, handler)
        result = c.delete({"operation_id": "op-1"})
        self.assertEqual(captured["body"]["file_name"], "agent/dict-file.bin")

    def test_delete_missing_api_key(self):
        c = _make_client(api_key=None)
        with self.assertRaises(ValueError):
            c.delete("agent/test.bin")


# ===================================================================
# 8. Auth methods
# ===================================================================


class TestAuth(unittest.TestCase):
    def test_auth_challenge(self):
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body={"nonce": "n123", "message": "sign this"})

        _patch_request(c, handler)
        resp = c.auth_challenge("wallet-pubkey")
        self.assertIsInstance(resp, ChallengeResponse)
        self.assertEqual(resp.nonce, "n123")
        self.assertEqual(resp.message, "sign this")

    def test_auth_verify_sets_tokens(self):
        c = _make_client(api_key=None)

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body={
                    "access_token": "new-access",
                    "refresh_token": "new-refresh",
                    "csrf_token": "csrf-1",
                },
            )

        _patch_request(c, handler)
        session = c.auth_verify("wallet", "nonce", "msg", "sig")
        self.assertIsInstance(session, AuthSession)
        self.assertEqual(c.api_key, "new-access")
        self.assertEqual(c.refresh_token, "new-refresh")
        self.assertEqual(session.csrf_token, "csrf-1")

    def test_auth_refresh_updates_tokens(self):
        c = _make_client()
        c.refresh_token = "old-refresh"

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body={
                    "access_token": "refreshed-access",
                    "refresh_token": "refreshed-refresh",
                },
            )

        _patch_request(c, handler)
        session = c.auth_refresh()
        self.assertEqual(c.api_key, "refreshed-access")
        self.assertEqual(c.refresh_token, "refreshed-refresh")

    def test_auth_refresh_keeps_existing_refresh_token_when_missing_in_response(self):
        c = _make_client()
        c.refresh_token = "old-refresh"

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body={
                    "access_token": "refreshed-access",
                },
            )

        _patch_request(c, handler)
        session = c.auth_refresh()
        self.assertEqual(c.api_key, "refreshed-access")
        self.assertEqual(c.refresh_token, "old-refresh")
        self.assertEqual(session.refresh_token, "old-refresh")

    def test_auth_refresh_without_token_raises(self):
        c = _make_client()
        c.refresh_token = None
        with self.assertRaises(ValueError):
            c.auth_refresh()

    def test_auth_logout_clears_tokens(self):
        c = _make_client()
        c.refresh_token = "tok"

        def handler(method, url, *, headers=None, body=None):
            return _make_response(200, body=b"")

        _patch_request(c, handler)
        c.auth_logout()
        self.assertIsNone(c.api_key)
        self.assertIsNone(c.refresh_token)

    def test_auto_refresh_on_401(self):
        """First authenticated call returns 401, refresh succeeds, retry succeeds."""
        c = _make_client()
        c.refresh_token = "r-tok"
        calls = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            # Call 1: the original request -> 401
            if calls["n"] == 1:
                raise PipeError("Unauthorized", 401, "unauthorized")
            # Call 2: the refresh endpoint
            if "/auth/refresh" in url:
                return _make_response(
                    200,
                    body={
                        "access_token": "new-access",
                        "refresh_token": "new-refresh",
                    },
                )
            # Call 3+: the retried original request
            return _make_response(200, body=_completed_status_payload())

        _patch_request(c, handler)
        s = c.check_status(operation_id="op-1")
        self.assertEqual(s.status, "completed")
        self.assertEqual(c.api_key, "new-access")

    def test_api_key_mode_does_not_auto_refresh_on_401(self):
        c = _make_client(auth_scheme="api_key")
        c.refresh_token = "r-tok"
        calls = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            raise PipeError("Unauthorized", 401, "unauthorized")

        _patch_request(c, handler)
        with self.assertRaises(PipeError) as ctx:
            c.check_status(operation_id="op-1")
        self.assertEqual(ctx.exception.status, 401)
        self.assertEqual(calls["n"], 1)

    def test_try_refresh_uses_lock(self):
        """Basic verification that _try_refresh acquires the lock without error."""
        c = _make_client()
        c.refresh_token = "tok"

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                200,
                body={"access_token": "a", "refresh_token": "r"},
            )

        _patch_request(c, handler)
        self.assertTrue(c._try_refresh())
        # Run again to make sure the lock was released
        self.assertTrue(c._try_refresh())


# ===================================================================
# 9. x402 credits top-up
# ===================================================================


class TestCreditsX402(unittest.TestCase):
    def test_request_credits_x402_decodes_payment_required(self):
        c = _make_client(auth_scheme="api_key")

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                402,
                headers={"Payment-Required": _b64_json(_payment_required_payload())},
                body={"error": "payment required"},
            )

        _patch_request(c, handler)
        required = c.request_credits_x402(1_000_000)
        self.assertEqual(required.accepts[0].network, "solana:mainnet")
        self.assertEqual(required.accepts[0].extra.intent_id, "intent-1")

    def test_request_credits_x402_conflict_raises(self):
        c = _make_client(auth_scheme="api_key")

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                409,
                body={
                    "error": "A credits payment is already processing for this intent.",
                    "intent": _credits_intent_status_payload(status="processing"),
                },
            )

        _patch_request(c, handler)
        with self.assertRaises(X402ConflictError) as ctx:
            c.request_credits_x402(1_000_000)
        self.assertEqual(ctx.exception.intent.status, "processing")
        self.assertEqual(ctx.exception.intent.intent_id, "intent-1")

    def test_request_credits_x402_missing_header_raises_protocol_error(self):
        c = _make_client(auth_scheme="api_key")

        def handler(method, url, *, headers=None, body=None):
            return _make_response(402, body={"error": "payment required"})

        _patch_request(c, handler)
        with self.assertRaises(X402ProtocolError):
            c.request_credits_x402(1_000_000)

    def test_confirm_credits_x402_pending_error_raises(self):
        c = _make_client(auth_scheme="api_key")

        def handler(method, url, *, headers=None, body=None):
            return _make_response(
                400,
                body=_credits_intent_status_payload(
                    status="pending",
                    error_message="payment still pending",
                ),
            )

        _patch_request(c, handler)
        with self.assertRaises(X402PendingIntentError) as ctx:
            c.confirm_credits_x402(
                1_000_000,
                {"intent_id": "intent-1", "tx_sig": "tx-sig-1"},
            )
        self.assertEqual(ctx.exception.intent.status, "pending")
        self.assertEqual(ctx.exception.intent.error_message, "payment still pending")

    def test_top_up_credits_x402_402_pay_202_polls_to_credited(self):
        c = _make_client(auth_scheme="api_key", poll_interval_sec=0.001, timeout_sec=0.1)
        calls = {"n": 0}
        seen_payment = {}

        def handler(method, url, *, headers=None, body=None):
            calls["n"] += 1
            if calls["n"] == 1:
                return _make_response(
                    402,
                    headers={"Payment-Required": _b64_json(_payment_required_payload())},
                    body={"error": "payment required"},
                )
            if calls["n"] == 2:
                return _make_response(
                    202,
                    headers={"Retry-After": "0"},
                    body={
                        "intent_id": "intent-1",
                        "status": "processing",
                        "requested_usdc_raw": 1_000_000,
                        "detected_usdc_raw": 0,
                        "credited_usdc_raw": 0,
                        "balance_usdc_raw": 0,
                        "payment_tx_sig": "tx-sig-1",
                        "last_checked_at": "2026-01-01T00:00:00Z",
                        "error_message": None,
                    },
                )
            if calls["n"] == 3:
                return _make_response(
                    200,
                    body=_credits_intent_status_payload(
                        status="credited",
                        detected_usdc_raw=1_000_000,
                        credited_usdc_raw=1_000_000,
                        payment_tx_sig="tx-sig-1",
                        credited_at="2026-01-01T00:00:05Z",
                    ),
                )
            return _make_response(200, body=_credits_status_payload())

        _patch_request(c, handler)

        def pay(payment):
            seen_payment["intent_id"] = payment.intent_id
            seen_payment["network"] = payment.network
            return "tx-sig-1"

        result = c.top_up_credits_x402(1_000_000, pay=pay, poll_interval_sec=0.001, timeout_sec=0.1)
        self.assertEqual(seen_payment["intent_id"], "intent-1")
        self.assertEqual(seen_payment["network"], "solana:mainnet")
        self.assertEqual(result.intent.status, "credited")
        self.assertEqual(result.credits.balance_usdc_raw, 5_000_000)
        self.assertEqual(calls["n"], 4)


# ===================================================================
# 10. Response size
# ===================================================================


class TestResponseSize(unittest.TestCase):
    def test_oversized_response_raises(self):
        """We need to test _request_absolute directly since our mock replaces it.
        Instead, build a client and test the real _request_absolute with a
        monkey-patched _http_pool.open."""
        c = _make_client()

        class FakeHeaders:
            def items(self):
                return []

        class FakeResponse:
            status = 200
            headers = FakeHeaders()

            def read(self, n):
                return b"x" * (MAX_RESPONSE_BYTES + 1)

            def close(self):
                pass

        class FakeOpener:
            def open(self, req, timeout=None):
                return FakeResponse()

        c._http_pool = FakeOpener()
        with self.assertRaises(PipeError) as ctx:
            c._request_absolute("GET", "https://example.com")
        self.assertIn("maximum size", str(ctx.exception))
        self.assertEqual(ctx.exception.status, 502)


# ===================================================================
# 11. PipeError
# ===================================================================


class TestPipeError(unittest.TestCase):
    def test_constructor(self):
        e = PipeError("msg", 404, "not found body")
        self.assertEqual(e.status, 404)
        self.assertEqual(e.body, "not found body")
        self.assertEqual(str(e), "msg")

    def test_is_runtime_error(self):
        e = PipeError("x", 500, "")
        self.assertIsInstance(e, RuntimeError)


# ===================================================================
# Runner
# ===================================================================

if __name__ == "__main__":
    unittest.main()
