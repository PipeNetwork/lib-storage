"""Integration test for the full PipeStorage flow.

Exercises: store → check_status → wait_for_operation → pin → fetch → delete
using a mock HTTP layer so no real PIPE_API_KEY or network access is required.
Mirrors the mock-server approach used by the Rust and TypeScript test suites.
"""

from __future__ import annotations

import json
import sys
import os
import inspect
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from pipe_storage.client import PipeStorage, PipeError, UploadStatus

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

VALID_HASH = "ab" * 32  # 64-char hex
TEST_API_KEY = "test-api-key-integration"
TEST_ACCOUNT = "integration-account"
TEST_BASE = "https://test.pipe.example.com"
FILE_NAME = "agent/integration-test.json"
MARKER = "marker-integration-1234"
OP_ID = "op-integ-001"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_response(
    status: int = 200,
    headers: dict | None = None,
    body: bytes | str | dict | None = b"",
) -> dict:
    if isinstance(body, dict):
        body = json.dumps(body).encode("utf-8")
    elif isinstance(body, str):
        body = body.encode("utf-8")
    return {
        "status": status,
        "headers": {k.lower(): v for k, v in (headers or {}).items()},
        "body": body or b"",
    }


def _completed_status(**overrides) -> dict:
    base = dict(
        operation_id=OP_ID,
        file_name=FILE_NAME,
        status="completed",
        finished=True,
        parts_completed=1,
        total_parts=1,
        content_hash=VALID_HASH,
        deterministic_url=f"{TEST_BASE}/{TEST_ACCOUNT}/{VALID_HASH}",
        bytes_total=100,
        bytes_uploaded=100,
        created_at="2025-01-01T00:00:00Z",
        updated_at="2025-01-01T00:00:01Z",
    )
    base.update(overrides)
    return base


def _queued_status(**overrides) -> dict:
    return _completed_status(
        status="queued",
        finished=False,
        content_hash=None,
        deterministic_url=None,
        bytes_uploaded=0,
        **overrides,
    )


def _make_client() -> PipeStorage:
    return PipeStorage(
        api_key=TEST_API_KEY,
        base_url=TEST_BASE,
        account=TEST_ACCOUNT,
        timeout_sec=5,
        poll_interval_sec=0.01,
    )


def _patch(client: PipeStorage, handler):
    """Replace ``_request_absolute`` with *handler(method, url, *, headers, body)*."""

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


# ---------------------------------------------------------------------------
# Integration test
# ---------------------------------------------------------------------------


class TestIntegrationFlow(unittest.TestCase):
    """Runs the full store → check → wait → pin → fetch → delete flow."""

    def test_full_lifecycle(self):
        c = _make_client()
        calls = []

        def handler(method, url, *, headers=None, body=None):
            calls.append({"method": method, "url": url, "body": body})

            # 1) store → upload endpoint
            if "/upload" in url and "checkUploadStatus" not in url:
                return _make_response(
                    202,
                    headers={
                        "x-operation-id": OP_ID,
                        "Location": f"{TEST_BASE}/dl",
                    },
                )

            # 2-3) check_status / wait_for_operation → first queued, then completed
            if "checkUploadStatus" in url:
                # Count how many status checks we've done
                status_checks = sum(
                    1 for c in calls if "checkUploadStatus" in c["url"]
                )
                if status_checks <= 1:
                    return _make_response(200, body=_queued_status())
                return _make_response(200, body=_completed_status())

            # 5) fetch → return the stored payload
            if "/download-stream" in url or f"/{TEST_ACCOUNT}/{VALID_HASH}" in url:
                return _make_response(
                    200,
                    body={"marker": MARKER, "kind": "integration"},
                )

            # 6) delete
            if "/deleteFile" in url:
                return _make_response(200, body={"deleted": True})

            return _make_response(404, body="not found")

        _patch(c, handler)

        # --- 1) store(wait=False) ---
        stored = c.store(
            {"marker": MARKER, "kind": "integration"},
            file_name=FILE_NAME,
            wait=False,
        )
        self.assertEqual(stored["operation_id"], OP_ID)
        self.assertEqual(stored["status"], "queued")

        # --- 2) check_status ---
        status = c.check_status(operation_id=OP_ID)
        self.assertIsInstance(status, UploadStatus)
        # First poll returns queued
        self.assertEqual(status.status, "queued")

        # --- 3) wait_for_operation ---
        completed = c.wait_for_operation(OP_ID)
        self.assertEqual(completed.status, "completed")
        self.assertEqual(completed.content_hash, VALID_HASH)

        # --- 4) pin ---
        pinned = c.pin({"operation_id": OP_ID})
        self.assertEqual(pinned["status"], "completed")
        self.assertIn(VALID_HASH, pinned["url"])
        self.assertEqual(pinned["content_hash"], VALID_HASH)

        # --- 5) fetch ---
        fetched = c.fetch(pinned["url"], as_json=True)
        self.assertIsInstance(fetched, dict)
        self.assertEqual(fetched["marker"], MARKER)

        # --- 6) delete ---
        deleted = c.delete(FILE_NAME)
        self.assertTrue(deleted.get("deleted"))

        # Verify the call sequence hit all expected endpoints
        urls = [c["url"] for c in calls]
        self.assertTrue(any("/upload" in u and "check" not in u for u in urls))
        self.assertTrue(any("checkUploadStatus" in u for u in urls))
        self.assertTrue(any("/deleteFile" in u for u in urls))

    def test_store_wait_true_polls_to_completion(self):
        """store(wait=True) should poll until completed."""
        c = _make_client()
        poll_count = {"n": 0}

        def handler(method, url, *, headers=None, body=None):
            if "/upload" in url and "checkUploadStatus" not in url:
                return _make_response(
                    202,
                    headers={"x-operation-id": OP_ID},
                )
            if "checkUploadStatus" in url:
                poll_count["n"] += 1
                if poll_count["n"] < 3:
                    return _make_response(200, body=_queued_status())
                return _make_response(200, body=_completed_status())
            return _make_response(404, body="not found")

        _patch(c, handler)
        result = c.store(b"data", file_name=FILE_NAME, wait=True)
        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["content_hash"], VALID_HASH)
        self.assertGreaterEqual(poll_count["n"], 3)

    def test_failed_upload_raises(self):
        """wait_for_operation raises PipeError when upload fails."""
        c = _make_client()

        def handler(method, url, *, headers=None, body=None):
            if "checkUploadStatus" in url:
                return _make_response(
                    200,
                    body=_completed_status(
                        status="failed",
                        finished=True,
                        error="checksum mismatch",
                        content_hash=None,
                        deterministic_url=None,
                    ),
                )
            return _make_response(404, body="not found")

        _patch(c, handler)
        with self.assertRaises(PipeError) as ctx:
            c.wait_for_operation(OP_ID)
        self.assertEqual(ctx.exception.status, 409)
        self.assertIn("checksum mismatch", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
