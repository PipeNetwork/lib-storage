from __future__ import annotations

import base64
import json
import os
import random
import re
import string
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, fields
from http.client import HTTPResponse
from typing import Any, Callable

DEFAULT_BASE_URL = "https://us-west-01-firestarter.pipenetwork.com"
DEFAULT_TIMEOUT_SEC = 120.0
DEFAULT_POLL_INTERVAL_SEC = 1.0
MAX_RESPONSE_BYTES = 256 * 1024 * 1024  # 256 MB
SDK_USER_AGENT = "pipe-agent-storage-python/0.1.0"


class PipeError(RuntimeError):
    def __init__(self, message: str, status: int, body: str):
        super().__init__(message)
        self.status = status
        self.body = body


class X402ProtocolError(RuntimeError):
    pass


class X402ConflictError(PipeError):
    def __init__(
        self,
        message: str,
        status: int,
        body: str,
        intent: CreditsIntentStatus | None = None,
    ):
        super().__init__(message, status, body)
        self.intent = intent


class X402PendingIntentError(RuntimeError):
    def __init__(self, intent: CreditsIntentStatus):
        super().__init__(intent.error_message or f"Credits intent {intent.intent_id} is pending with an error")
        self.intent = intent


class X402TimeoutError(TimeoutError):
    def __init__(self, intent_id: str):
        super().__init__(f"Timed out waiting for credits intent {intent_id} to be credited")
        self.intent_id = intent_id


@dataclass
class ChallengeResponse:
    nonce: str
    message: str


@dataclass
class AuthSession:
    access_token: str
    refresh_token: str
    csrf_token: str | None = None


@dataclass
class CreditsIntentStatus:
    intent_id: str
    status: str
    requested_usdc_raw: int
    detected_usdc_raw: int
    credited_usdc_raw: int
    usdc_mint: str
    treasury_owner_pubkey: str | None = None
    treasury_usdc_ata: str = ""
    reference_pubkey: str = ""
    payment_tx_sig: str | None = None
    last_checked_at: str | None = None
    credited_at: str | None = None
    error_message: str | None = None


@dataclass
class CreditsStatus:
    balance_usdc_raw: int
    balance_usdc: float
    total_deposited_usdc_raw: int
    total_spent_usdc_raw: int
    usdc_mint: str | None = None
    last_topup_at: str | None = None
    product_mode: str | None = None
    eligible_for_activation: bool | None = None
    eligibility_error: str | None = None
    bundled_public_delivery: bool | None = None
    portal_url: str | None = None
    storage_usdc_raw_per_gb_month: int | None = None
    storage_usdc_per_gb_month: float | None = None
    bandwidth_usdc_raw_per_gb: int | None = None
    bandwidth_usdc_per_gb: float | None = None
    wordpress_site_count: int | None = None
    wordpress_plan: str | None = None
    wordpress_plan_started_at: str | None = None
    wordpress_plan_expires_at: str | None = None
    wordpress_storage_cap_bytes: int | None = None
    wordpress_current_storage_bytes: int | None = None
    wordpress_remaining_storage_bytes: int | None = None
    wordpress_renewal_required: bool | None = None
    wordpress_legacy_billing: bool | None = None
    wordpress_annual_price_usdc_raw: int | None = None
    wordpress_annual_price_usdc: float | None = None
    wordpress_free_storage_bytes: int | None = None
    wordpress_annual_storage_cap_bytes: int | None = None
    wordpress_plan_term_days: int | None = None
    intent: CreditsIntentStatus | None = None


@dataclass
class X402PaymentAcceptExtra:
    intent_id: str | None = None
    reference_pubkey: str | None = None


@dataclass
class X402PaymentAccept:
    scheme: str
    network: str
    amount: str
    asset: str
    payTo: str
    maxTimeoutSeconds: int | None = None
    extra: X402PaymentAcceptExtra | None = None


@dataclass
class X402PaymentRequired:
    x402Version: int
    resource: str
    accepts: list[X402PaymentAccept]


@dataclass
class X402PaymentSignaturePayload:
    intent_id: str
    tx_sig: str


@dataclass
class X402ConfirmResponse:
    intent_id: str
    status: str
    requested_usdc_raw: int
    detected_usdc_raw: int
    credited_usdc_raw: int
    balance_usdc_raw: int
    http_status: int
    retry_after_seconds: int | None = None
    payment_tx_sig: str | None = None
    last_checked_at: str | None = None
    error_message: str | None = None


@dataclass
class X402PaymentContext:
    required: X402PaymentRequired
    accept: X402PaymentAccept
    intent_id: str
    reference_pubkey: str | None
    amount: str
    asset: str
    pay_to: str
    network: str


@dataclass
class X402TopUpResult:
    intent: CreditsIntentStatus
    credits: CreditsStatus


@dataclass
class UploadStatus:
    operation_id: str
    file_name: str
    status: str
    finished: bool
    parts_completed: int
    total_parts: int
    error: str | None = None
    content_hash: str | None = None
    deterministic_url: str | None = None
    bytes_total: int = 0
    bytes_uploaded: int = 0
    created_at: str = ""
    updated_at: str = ""


class PipeStorage:
    def __init__(
        self,
        api_key: str | None = None,
        auth_scheme: str = "bearer",
        base_url: str | None = None,
        control_base_url: str | None = None,
        data_base_url: str | None = None,
        account: str | None = None,
        timeout_sec: float = DEFAULT_TIMEOUT_SEC,
        poll_interval_sec: float = DEFAULT_POLL_INTERVAL_SEC,
    ) -> None:
        self.api_key = api_key or os.getenv("PIPE_API_KEY")
        self.auth_scheme = _normalize_auth_scheme(auth_scheme)
        fallback_base_url = base_url or os.getenv("PIPE_BASE_URL") or os.getenv("PIPE_API_BASE_URL") or DEFAULT_BASE_URL
        explicit_control_base_url = control_base_url or os.getenv("PIPE_CONTROL_BASE_URL")
        explicit_data_base_url = data_base_url or os.getenv("PIPE_DATA_BASE_URL")
        self.control_base_url = (explicit_control_base_url or fallback_base_url).rstrip("/")
        self.data_base_url = (explicit_data_base_url or fallback_base_url).rstrip("/")
        self.account = account or os.getenv("PIPE_ACCOUNT")
        self.timeout_sec = timeout_sec
        self.poll_interval_sec = poll_interval_sec
        self.refresh_token: str | None = None
        self._use_pop_gateway = explicit_control_base_url is not None or explicit_data_base_url is not None
        self._refresh_lock = threading.Lock()
        self._http_pool = urllib.request.build_opener(
            urllib.request.HTTPSHandler(),
            urllib.request.HTTPHandler(),
        )

    def auth_challenge(self, wallet_public_key: str) -> ChallengeResponse:
        response = self._request(
            "POST",
            "/auth/siws/challenge",
            json_body={"wallet_public_key": wallet_public_key},
        )
        payload = json.loads(response["body"].decode("utf-8"))
        return ChallengeResponse(nonce=payload["nonce"], message=payload["message"])

    def auth_verify(
        self,
        wallet_public_key: str,
        nonce: str,
        message: str,
        signature_b64: str,
    ) -> AuthSession:
        response = self._request(
            "POST",
            "/auth/siws/verify",
            json_body={
                "wallet_public_key": wallet_public_key,
                "nonce": nonce,
                "message": message,
                "signature_b64": signature_b64,
            },
        )
        payload = json.loads(response["body"].decode("utf-8"))
        session = AuthSession(
            access_token=payload["access_token"],
            refresh_token=payload["refresh_token"],
            csrf_token=payload.get("csrf_token"),
        )
        self.api_key = session.access_token
        self.auth_scheme = "bearer"
        self.refresh_token = session.refresh_token
        return session

    def auth_refresh(self) -> AuthSession:
        if not self.refresh_token:
            raise ValueError("No refresh token available. Call auth_verify first.")
        response = self._request(
            "POST",
            "/auth/refresh",
            json_body={"refresh_token": self.refresh_token},
        )
        payload = json.loads(response["body"].decode("utf-8"))
        next_refresh_token = payload.get("refresh_token") or self.refresh_token
        if not next_refresh_token:
            raise ValueError("auth_refresh response missing refresh_token")
        session = AuthSession(
            access_token=payload["access_token"],
            refresh_token=next_refresh_token,
            csrf_token=payload.get("csrf_token"),
        )
        self.api_key = session.access_token
        self.auth_scheme = "bearer"
        self.refresh_token = session.refresh_token
        return session

    def auth_logout(self) -> None:
        self._require_api_key("auth_logout")
        self._request("POST", "/auth/logout", authenticated=True)
        self.api_key = None
        self.refresh_token = None

    def credits_status(self) -> CreditsStatus:
        response = self._request("GET", "/api/credits/status", authenticated=True)
        payload = json.loads(response["body"].decode("utf-8"))
        return _credits_status_from_payload(payload)

    def credits_intent(self, intent_id: str) -> CreditsIntentStatus:
        if not intent_id.strip():
            raise ValueError("credits_intent requires a non-empty intent_id")
        response = self._request(
            "GET",
            f"/api/credits/intent/{urllib.parse.quote(intent_id, safe='')}",
            authenticated=True,
        )
        payload = json.loads(response["body"].decode("utf-8"))
        return _credits_intent_status_from_payload(payload)

    def request_credits_x402(self, amount_usdc_raw: int) -> X402PaymentRequired:
        self._validate_amount_usdc_raw(amount_usdc_raw)
        response = self._request(
            "POST",
            "/api/credits/x402",
            json_body={"amount_usdc_raw": amount_usdc_raw},
            authenticated=True,
            allowed_statuses={402, 409},
        )
        if response["status"] == 402:
            header_value = response["headers"].get("payment-required")
            if not header_value:
                raise X402ProtocolError("Missing Payment-Required header on x402 response")
            return decode_payment_required(header_value)
        if response["status"] == 409:
            raise _x402_conflict_error(response)
        raise X402ProtocolError(
            f"Expected 402 Payment Required from /api/credits/x402, received {response['status']}"
        )

    def confirm_credits_x402(
        self,
        amount_usdc_raw: int,
        payment_signature: X402PaymentSignaturePayload | dict[str, str],
    ) -> X402ConfirmResponse:
        self._validate_amount_usdc_raw(amount_usdc_raw)
        signature_payload = (
            payment_signature
            if isinstance(payment_signature, X402PaymentSignaturePayload)
            else X402PaymentSignaturePayload(
                intent_id=str(payment_signature.get("intent_id", "")).strip(),
                tx_sig=str(payment_signature.get("tx_sig", "")).strip(),
            )
        )
        if not signature_payload.intent_id or not signature_payload.tx_sig:
            raise X402ProtocolError("Payment signature requires non-empty intent_id and tx_sig")

        response = self._request(
            "POST",
            "/api/credits/x402",
            json_body={"amount_usdc_raw": amount_usdc_raw},
            headers={"Payment-Signature": encode_payment_signature(signature_payload)},
            authenticated=True,
            allowed_statuses={400, 409},
        )
        if response["status"] == 409:
            raise _x402_conflict_error(response)
        if response["status"] == 400:
            payload = json.loads(response["body"].decode("utf-8", errors="replace"))
            if _is_intent_status_payload(payload) and payload.get("status") == "pending" and payload.get("error_message"):
                raise X402PendingIntentError(_credits_intent_status_from_payload(payload))
            raise PipeError(
                "Pipe API request failed (400 Bad Request)",
                400,
                response["body"].decode("utf-8", errors="replace"),
            )

        payload = json.loads(response["body"].decode("utf-8"))
        return X402ConfirmResponse(
            intent_id=payload["intent_id"],
            status=payload["status"],
            requested_usdc_raw=payload["requested_usdc_raw"],
            detected_usdc_raw=payload["detected_usdc_raw"],
            credited_usdc_raw=payload["credited_usdc_raw"],
            balance_usdc_raw=payload["balance_usdc_raw"],
            payment_tx_sig=payload.get("payment_tx_sig"),
            last_checked_at=payload.get("last_checked_at"),
            error_message=payload.get("error_message"),
            http_status=response["status"],
            retry_after_seconds=_parse_retry_after_seconds(response["headers"].get("retry-after")),
        )

    def top_up_credits_x402(
        self,
        amount_usdc_raw: int,
        *,
        pay: Callable[[X402PaymentContext], str | dict[str, str]],
        timeout_sec: float | None = None,
        poll_interval_sec: float | None = None,
    ) -> X402TopUpResult:
        required = self.request_credits_x402(amount_usdc_raw)
        if not required.accepts:
            raise X402ProtocolError("Payment-Required payload did not include accepts[0]")
        accept = required.accepts[0]
        intent_id = accept.extra.intent_id if accept.extra else None
        if not intent_id:
            raise X402ProtocolError("Payment-Required payload is missing extra.intent_id")

        tx_sig = _normalize_payment_callback_result(
            pay(
                X402PaymentContext(
                    required=required,
                    accept=accept,
                    intent_id=intent_id,
                    reference_pubkey=accept.extra.reference_pubkey if accept.extra else None,
                    amount=accept.amount,
                    asset=accept.asset,
                    pay_to=accept.payTo,
                    network=accept.network,
                )
            )
        )
        confirm = self.confirm_credits_x402(
            amount_usdc_raw,
            X402PaymentSignaturePayload(intent_id=intent_id, tx_sig=tx_sig),
        )

        final_intent = (
            self._poll_credits_intent_until_settled(
                intent_id,
                retry_after_seconds=confirm.retry_after_seconds,
                timeout_sec=timeout_sec,
                poll_interval_sec=poll_interval_sec,
            )
            if confirm.http_status == 202 or confirm.status == "processing"
            else self.credits_intent(intent_id)
        )
        return X402TopUpResult(intent=final_intent, credits=self.credits_status())

    def deterministic_url(self, content_hash: str, account: str | None = None) -> str:
        effective_account = account or self.account
        if not effective_account:
            raise ValueError("Missing account for deterministic URL. Set PIPE_ACCOUNT or pass account.")
        if not _is_hex_hash(content_hash):
            raise ValueError("content_hash must be a 64-character hex string")
        return f"{self.data_base_url}/{urllib.parse.quote(effective_account, safe='')}/{content_hash.lower()}"

    def store(
        self,
        data: Any,
        file_name: str | None = None,
        tier: str = "normal",
        wait: bool = True,
        timeout_sec: float | None = None,
    ) -> dict[str, Any]:
        self._require_api_key("store")

        if not file_name:
            file_name = f"agent/{int(time.time() * 1000)}-{_random_suffix()}.bin"

        endpoint = (
            f"{self.data_base_url}/v1/upload"
            if self._use_pop_gateway
            else f"{self.control_base_url}/{'priorityUpload' if tier == 'priority' else 'upload'}"
        )
        query: dict[str, str] = {"file_name": file_name}
        if tier and tier != "normal":
            query["tier"] = tier

        body = _to_bytes(data)
        upload_url = f"{endpoint}?{urllib.parse.urlencode(query)}"
        request_headers = {"Content-Type": "application/octet-stream"}
        try:
            response = self._request_url(
                "POST",
                upload_url,
                headers=request_headers,
                body=body,
                authenticated=True,
            )
        except PipeError as exc:
            if not self._use_pop_gateway or exc.status not in (404, 405):
                raise
            fallback_endpoint = f"{self.control_base_url}/{'priorityUpload' if tier == 'priority' else 'upload'}"
            response = self._request_url(
                "POST",
                f"{fallback_endpoint}?{urllib.parse.urlencode(query)}",
                headers=request_headers,
                body=body,
                authenticated=True,
            )

        operation_id = response["headers"].get("x-operation-id")
        location = response["headers"].get("location")

        if not wait or not operation_id:
            return {
                "operation_id": operation_id,
                "location": location,
                "file_name": file_name,
                "status": "queued" if operation_id else "completed",
            }

        status = self.wait_for_operation(operation_id, timeout_sec=timeout_sec)
        return {
            "operation_id": status.operation_id,
            "location": location,
            "file_name": status.file_name,
            "status": status.status,
            "content_hash": status.content_hash,
            "deterministic_url": status.deterministic_url,
        }

    def check_status(
        self,
        *,
        operation_id: str | None = None,
        file_name: str | None = None,
    ) -> UploadStatus:
        self._require_api_key("check_status")

        if not operation_id and not file_name:
            raise ValueError("check_status requires operation_id or file_name")

        query: dict[str, str] = {}
        if operation_id:
            query["operation_id"] = operation_id
        if file_name:
            query["file_name"] = file_name

        response = self._request(
            "GET",
            "/pop/v1/checkUploadStatus" if self._use_pop_gateway else "/checkUploadStatus",
            query=query,
            authenticated=True,
        )
        payload = json.loads(response["body"].decode("utf-8"))
        known_fields = {f.name for f in fields(UploadStatus)}
        return UploadStatus(**{k: v for k, v in payload.items() if k in known_fields})

    def wait_for_operation(self, operation_id: str, timeout_sec: float | None = None) -> UploadStatus:
        self._require_api_key("wait_for_operation")

        deadline = time.monotonic() + (timeout_sec or self.timeout_sec)
        consecutive_errors = 0
        max_transient_errors = 3
        while time.monotonic() < deadline:
            try:
                status = self.check_status(operation_id=operation_id)
                consecutive_errors = 0
            except PipeError as exc:
                consecutive_errors += 1
                if consecutive_errors >= max_transient_errors or exc.status < 500:
                    raise
                time.sleep(self.poll_interval_sec)
                continue

            if status.status == "completed":
                return status
            if status.status == "failed":
                raise PipeError(
                    f"Upload failed for operation {operation_id}: {status.error or 'unknown error'}",
                    409,
                    status.error or "upload failed",
                )
            time.sleep(self.poll_interval_sec)

        raise TimeoutError(f"Timed out waiting for operation {operation_id}")

    def pin(self, key: str | dict[str, str]) -> dict[str, Any]:
        if isinstance(key, str):
            if key.startswith("http://") or key.startswith("https://"):
                return {"url": key}
            if _is_hex_hash(key):
                return {
                    "url": self.deterministic_url(key),
                    "content_hash": key.lower(),
                    "status": "completed",
                }
            if _is_uuid(key):
                return self.pin({"operation_id": key})
            return self.pin({"file_name": key})

        content_hash = key.get("content_hash")
        if content_hash:
            return {
                "url": self.deterministic_url(content_hash, key.get("account")),
                "content_hash": content_hash.lower(),
                "status": "completed",
            }

        operation_id = key.get("operation_id")
        file_name = key.get("file_name")
        if not operation_id and not file_name:
            raise ValueError("pin requires operation_id, file_name, content_hash, or deterministic URL")

        status = self.check_status(operation_id=operation_id, file_name=file_name)
        if status.status != "completed":
            raise RuntimeError(f"Cannot pin object while status is {status.status}. operation_id={status.operation_id}")

        url = status.deterministic_url
        if not url and status.content_hash:
            url = self.deterministic_url(status.content_hash, key.get("account"))

        if not url:
            raise RuntimeError("Upload completed but deterministic URL is unavailable (missing content_hash)")

        return {
            "url": url,
            "content_hash": status.content_hash,
            "operation_id": status.operation_id,
            "file_name": status.file_name,
            "status": status.status,
        }

    def fetch(
        self,
        key: str | dict[str, str],
        *,
        as_text: bool = False,
        as_json: bool = False,
    ) -> bytes | str | Any:
        url = self._resolve_fetch_url(key)
        is_pipe_url = url.startswith(f"{self.control_base_url}/") or url.startswith(f"{self.data_base_url}/")
        is_deterministic = self._is_public_deterministic_url(url)
        requires_auth = is_pipe_url and not is_deterministic

        headers: dict[str, str] = {}
        if self.api_key:
            headers["Authorization"] = self._authorization_header()
        elif is_pipe_url and not is_deterministic:
            raise ValueError("Missing API key for authenticated fetch. Set PIPE_API_KEY.")

        response = (
            self._request_url("GET", url, headers=headers, authenticated=requires_auth)
            if is_pipe_url
            else self._request_absolute("GET", url, headers=headers)
        )
        body = response["body"]

        if as_json:
            return json.loads(body.decode("utf-8"))
        if as_text:
            return body.decode("utf-8")
        return body

    def delete(self, key: str | dict[str, str]) -> dict[str, Any]:
        self._require_api_key("delete")

        file_name: str | None = None
        if isinstance(key, str):
            if _is_uuid(key):
                file_name = self.check_status(operation_id=key).file_name
            else:
                file_name = key
        else:
            file_name = key.get("file_name")
            if not file_name and key.get("operation_id"):
                file_name = self.check_status(operation_id=key["operation_id"]).file_name

        if not file_name:
            raise ValueError("delete requires file_name or operation_id")

        response = self._request(
            "POST",
            "/pop/v1/deleteFile" if self._use_pop_gateway else "/deleteFile",
            json_body={"file_name": file_name},
            authenticated=True,
        )
        return json.loads(response["body"].decode("utf-8"))

    def _resolve_fetch_url(self, key: str | dict[str, str]) -> str:
        if isinstance(key, str):
            if key.startswith("http://") or key.startswith("https://"):
                return key
            if _is_hex_hash(key):
                return self.deterministic_url(key)
            return f"{self.data_base_url}/download-stream?file_name={urllib.parse.quote(key, safe='')}"

        if "url" in key:
            return key["url"]
        if "file_name" in key:
            return f"{self.data_base_url}/download-stream?file_name={urllib.parse.quote(key['file_name'], safe='')}"
        if "content_hash" in key:
            return self.deterministic_url(key["content_hash"], key.get("account"))

        raise ValueError("fetch requires url, file_name, content_hash, or a supported string key")

    def _is_public_deterministic_url(self, url: str) -> bool:
        prefix = f"{self.data_base_url}/"
        if not url.startswith(prefix):
            return False
        tail = url[len(prefix) :]
        parts = tail.split("/")
        return len(parts) == 2 and _is_hex_hash(parts[1])

    def _request(
        self,
        method: str,
        path: str,
        *,
        query: dict[str, str] | None = None,
        body: bytes | None = None,
        json_body: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        authenticated: bool = False,
        allowed_statuses: set[int] | None = None,
    ) -> dict[str, Any]:
        url = f"{self.control_base_url}{path}"
        if query:
            url = f"{url}?{urllib.parse.urlencode(query)}"

        request_headers: dict[str, str] = dict(headers or {})
        if authenticated:
            self._require_api_key(f"{method} {path}")
            request_headers["Authorization"] = self._authorization_header()

        if json_body is not None:
            body = json.dumps(json_body).encode("utf-8")
            request_headers.setdefault("Content-Type", "application/json")

        try:
            return self._request_absolute(
                method,
                url,
                headers=request_headers,
                body=body,
                allowed_statuses=allowed_statuses,
            )
        except PipeError as exc:
            if exc.status != 401 or not authenticated or self.auth_scheme != "bearer" or not self.refresh_token:
                raise
            if not self._try_refresh():
                raise exc from None
            request_headers["Authorization"] = self._authorization_header()
            return self._request_absolute(
                method,
                url,
                headers=request_headers,
                body=body,
                allowed_statuses=allowed_statuses,
            )

    def _request_url(
        self,
        method: str,
        url: str,
        *,
        body: bytes | None = None,
        headers: dict[str, str] | None = None,
        authenticated: bool = False,
        allowed_statuses: set[int] | None = None,
    ) -> dict[str, Any]:
        request_headers: dict[str, str] = dict(headers or {})
        if authenticated:
            self._require_api_key(url)
            request_headers["Authorization"] = self._authorization_header()

        try:
            return self._request_absolute(
                method,
                url,
                headers=request_headers,
                body=body,
                allowed_statuses=allowed_statuses,
            )
        except PipeError as exc:
            if exc.status != 401 or not authenticated or self.auth_scheme != "bearer" or not self.refresh_token:
                raise
            if not self._try_refresh():
                raise exc from None
            request_headers["Authorization"] = self._authorization_header()
            return self._request_absolute(
                method,
                url,
                headers=request_headers,
                body=body,
                allowed_statuses=allowed_statuses,
            )

    def _request_absolute(
        self,
        method: str,
        url: str,
        *,
        headers: dict[str, str] | None = None,
        body: bytes | None = None,
        allowed_statuses: set[int] | None = None,
    ) -> dict[str, Any]:
        req = urllib.request.Request(url=url, data=body, method=method)
        req.add_header("User-Agent", SDK_USER_AGENT)
        for key, value in (headers or {}).items():
            req.add_header(key, value)

        try:
            res: HTTPResponse = self._http_pool.open(req, timeout=self.timeout_sec)
            try:
                response_body = res.read(MAX_RESPONSE_BYTES + 1)
                if len(response_body) > MAX_RESPONSE_BYTES:
                    raise PipeError("Response body exceeds maximum size", 502, "response too large")
                return {
                    "status": res.status,
                    "headers": {k.lower(): v for k, v in res.headers.items()},
                    "body": response_body,
                }
            finally:
                res.close()
        except urllib.error.HTTPError as exc:
            response_body = exc.read(MAX_RESPONSE_BYTES)
            if allowed_statuses and exc.code in allowed_statuses:
                return {
                    "status": exc.code,
                    "headers": {k.lower(): v for k, v in exc.headers.items()},
                    "body": response_body,
                }
            body_text = response_body.decode("utf-8", errors="replace")
            raise PipeError(
                f"Pipe API request failed ({exc.code} {exc.reason})",
                exc.code,
                body_text,
            ) from exc

    def _try_refresh(self) -> bool:
        with self._refresh_lock:
            if not self.refresh_token:
                return False
            try:
                self.auth_refresh()
                return True
            except Exception:
                return False

    def _require_api_key(self, action: str) -> None:
        if not self.api_key:
            raise ValueError(f"Missing API key for {action}. Set PIPE_API_KEY or pass api_key.")

    def _authorization_header(self) -> str:
        return f"ApiKey {self.api_key}" if self.auth_scheme == "api_key" else f"Bearer {self.api_key}"

    def _validate_amount_usdc_raw(self, amount_usdc_raw: int) -> None:
        if not isinstance(amount_usdc_raw, int) or amount_usdc_raw <= 0:
            raise ValueError("amount_usdc_raw must be a positive integer")

    def _poll_credits_intent_until_settled(
        self,
        intent_id: str,
        *,
        retry_after_seconds: int | None = None,
        timeout_sec: float | None = None,
        poll_interval_sec: float | None = None,
    ) -> CreditsIntentStatus:
        timeout = timeout_sec or self.timeout_sec
        interval = poll_interval_sec or self.poll_interval_sec
        deadline = time.monotonic() + timeout
        delay = retry_after_seconds if retry_after_seconds is not None else interval

        while time.monotonic() < deadline:
            time.sleep(delay)
            intent = self.credits_intent(intent_id)
            if intent.status == "credited":
                return intent
            if intent.status == "pending" and intent.error_message:
                raise X402PendingIntentError(intent)
            delay = interval

        raise X402TimeoutError(intent_id)


def _is_hex_hash(value: str) -> bool:
    return bool(re.fullmatch(r"[A-Fa-f0-9]{64}", value))


def _is_uuid(value: str) -> bool:
    return bool(
        re.fullmatch(
            r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}",
            value,
        )
    )


def _random_suffix(length: int = 12) -> str:
    alphabet = string.ascii_lowercase + string.digits
    return "".join(random.choice(alphabet) for _ in range(length))


def _to_bytes(data: Any) -> bytes:
    if isinstance(data, bytes):
        return data
    if isinstance(data, bytearray):
        return bytes(data)
    if isinstance(data, str):
        return data.encode("utf-8")
    return json.dumps(data).encode("utf-8")


def encode_json_to_base64(value: Any) -> str:
    return base64.b64encode(json.dumps(value).encode("utf-8")).decode("utf-8")


def decode_json_from_base64(value: str) -> Any:
    return json.loads(base64.b64decode(value).decode("utf-8"))


def decode_payment_required(header_value: str) -> X402PaymentRequired:
    payload = decode_json_from_base64(header_value)
    if not isinstance(payload, dict):
        raise X402ProtocolError("Payment-Required header did not decode to an object")
    return _x402_payment_required_from_payload(payload)


def encode_payment_signature(payload: X402PaymentSignaturePayload | dict[str, str]) -> str:
    wire_payload = (
        {
            "intent_id": payload.intent_id,
            "tx_sig": payload.tx_sig,
        }
        if isinstance(payload, X402PaymentSignaturePayload)
        else payload
    )
    return encode_json_to_base64(wire_payload)


def _normalize_auth_scheme(value: str) -> str:
    normalized = value.strip()
    if normalized in {"apiKey", "api_key", "api-key"}:
        return "api_key"
    if normalized == "bearer":
        return "bearer"
    raise ValueError("auth_scheme must be 'bearer' or 'api_key'")


def _normalize_payment_callback_result(value: str | dict[str, str]) -> str:
    if isinstance(value, str):
        tx_sig = value.strip()
    elif isinstance(value, dict):
        tx_sig = str(value.get("tx_sig") or value.get("txSig") or "").strip()
    else:
        tx_sig = ""
    if not tx_sig:
        raise X402ProtocolError("Payment callback must return a non-empty tx signature")
    return tx_sig


def _parse_retry_after_seconds(value: str | None) -> int | None:
    if not value:
        return None
    try:
        parsed = int(value)
    except (TypeError, ValueError):
        return None
    return parsed if parsed >= 0 else None


def _is_intent_status_payload(value: Any) -> bool:
    return isinstance(value, dict) and isinstance(value.get("intent_id"), str) and isinstance(value.get("status"), str)


def _credits_intent_status_from_payload(payload: dict[str, Any]) -> CreditsIntentStatus:
    known_fields = {f.name for f in fields(CreditsIntentStatus)}
    filtered = {k: v for k, v in payload.items() if k in known_fields}
    return CreditsIntentStatus(**filtered)


def _credits_status_from_payload(payload: dict[str, Any]) -> CreditsStatus:
    known_fields = {f.name for f in fields(CreditsStatus)}
    filtered = {k: v for k, v in payload.items() if k in known_fields and k != "intent"}
    intent_payload = payload.get("intent")
    filtered["intent"] = (
        _credits_intent_status_from_payload(intent_payload)
        if isinstance(intent_payload, dict)
        else None
    )
    return CreditsStatus(**filtered)


def _x402_payment_required_from_payload(payload: dict[str, Any]) -> X402PaymentRequired:
    accepts_payload = payload.get("accepts")
    if not isinstance(accepts_payload, list):
        raise X402ProtocolError("Payment-Required payload is missing accepts")
    accepts: list[X402PaymentAccept] = []
    for item in accepts_payload:
        if not isinstance(item, dict):
            raise X402ProtocolError("Payment-Required accept entry must be an object")
        extra_payload = item.get("extra")
        extra = (
            X402PaymentAcceptExtra(
                intent_id=extra_payload.get("intent_id"),
                reference_pubkey=extra_payload.get("reference_pubkey"),
            )
            if isinstance(extra_payload, dict)
            else None
        )
        accepts.append(
            X402PaymentAccept(
                scheme=str(item["scheme"]),
                network=str(item["network"]),
                amount=str(item["amount"]),
                asset=str(item["asset"]),
                payTo=str(item["payTo"]),
                maxTimeoutSeconds=item.get("maxTimeoutSeconds"),
                extra=extra,
            )
        )
    return X402PaymentRequired(
        x402Version=int(payload["x402Version"]),
        resource=str(payload["resource"]),
        accepts=accepts,
    )


def _x402_conflict_error(response: dict[str, Any]) -> X402ConflictError:
    body = response["body"].decode("utf-8", errors="replace")
    try:
        payload = json.loads(body)
    except ValueError:
        payload = None
    intent = None
    if isinstance(payload, dict) and isinstance(payload.get("intent"), dict):
        intent = _credits_intent_status_from_payload(payload["intent"])
    message = payload.get("error") if isinstance(payload, dict) else None
    return X402ConflictError(
        message or f"x402 request failed ({response['status']})",
        response["status"],
        body,
        intent,
    )
