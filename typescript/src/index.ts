export type UploadTier = "normal" | "priority" | "premium" | "ultra" | "enterprise";
export type AuthScheme = "bearer" | "apiKey";

export interface PipeStorageClientOptions {
  apiKey?: string;
  authScheme?: AuthScheme;
  baseUrl?: string;
  controlBaseUrl?: string;
  dataBaseUrl?: string;
  account?: string;
  pollIntervalMs?: number;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
}

export interface StoreOptions {
  fileName?: string;
  tier?: UploadTier;
  wait?: boolean;
  timeoutMs?: number;
}

export interface UploadStatus {
  operation_id: string;
  file_name: string;
  status: "queued" | "running" | "durable" | "finalizing" | "completed" | "failed";
  finished: boolean;
  parts_completed: number;
  total_parts: number;
  error?: string | null;
  content_hash?: string | null;
  deterministic_url?: string | null;
  bytes_total: number;
  bytes_uploaded: number;
  created_at: string;
  updated_at: string;
}

export interface StoreResult {
  operationId?: string;
  location?: string;
  fileName: string;
  status: UploadStatus["status"];
  contentHash?: string;
  deterministicUrl?: string;
}

export interface PinInput {
  operationId?: string;
  fileName?: string;
  contentHash?: string;
  account?: string;
}

export interface PinResult {
  url: string;
  contentHash?: string;
  operationId?: string;
  fileName?: string;
  status?: UploadStatus["status"];
}

export interface FetchOptions {
  asText?: boolean;
  asJson?: boolean;
}

export interface FetchByUrl {
  url: string;
}

export interface FetchByFileName {
  fileName: string;
}

export interface FetchByHash {
  contentHash: string;
  account?: string;
}

export type FetchInput = string | FetchByUrl | FetchByFileName | FetchByHash;

export interface DeleteInput {
  fileName?: string;
  operationId?: string;
}

export interface DeleteResponse {
  message: string;
}

export interface ChallengeResponse {
  nonce: string;
  message: string;
}

/** Wire format uses snake_case to match the server JSON response. */
export interface AuthSession {
  access_token: string;
  refresh_token?: string;
  csrf_token?: string;
}

export interface CreditsIntentStatus {
  intent_id: string;
  status: string;
  requested_usdc_raw: number;
  detected_usdc_raw: number;
  credited_usdc_raw: number;
  usdc_mint: string;
  treasury_owner_pubkey?: string;
  treasury_usdc_ata: string;
  reference_pubkey: string;
  payment_tx_sig?: string | null;
  last_checked_at?: string | null;
  credited_at?: string | null;
  error_message?: string | null;
}

export interface CreditsStatus {
  balance_usdc_raw: number;
  balance_usdc: number;
  total_deposited_usdc_raw: number;
  total_spent_usdc_raw: number;
  usdc_mint?: string;
  last_topup_at?: string | null;
  product_mode?: "standard" | "wordpress";
  eligible_for_activation?: boolean;
  eligibility_error?: string | null;
  bundled_public_delivery?: boolean;
  portal_url?: string;
  storage_usdc_raw_per_gb_month?: number;
  storage_usdc_per_gb_month?: number;
  bandwidth_usdc_raw_per_gb?: number;
  bandwidth_usdc_per_gb?: number;
  wordpress_site_count?: number;
  wordpress_plan?: "legacy_credits" | "free" | "annual_10tb";
  wordpress_plan_started_at?: string | null;
  wordpress_plan_expires_at?: string | null;
  wordpress_storage_cap_bytes?: number | null;
  wordpress_current_storage_bytes?: number;
  wordpress_remaining_storage_bytes?: number | null;
  wordpress_renewal_required?: boolean;
  wordpress_legacy_billing?: boolean;
  wordpress_annual_price_usdc_raw?: number;
  wordpress_annual_price_usdc?: number;
  wordpress_free_storage_bytes?: number;
  wordpress_annual_storage_cap_bytes?: number;
  wordpress_plan_term_days?: number;
  intent?: CreditsIntentStatus | null;
}

export interface CreditsIntentConflictResponse {
  error: string;
  intent: CreditsIntentStatus;
}

export interface SubmitCreditsPaymentResponse {
  intent_id: string;
  status: string;
  requested_usdc_raw: number;
  detected_usdc_raw: number;
  credited_usdc_raw: number;
  balance_usdc_raw: number;
  payment_tx_sig?: string | null;
  last_checked_at?: string | null;
  error_message?: string | null;
}

export interface X402PaymentRequired {
  x402Version: number;
  resource: string;
  accepts: Array<{
    scheme: string;
    network: string;
    amount: string;
    asset: string;
    payTo: string;
    maxTimeoutSeconds?: number;
    extra?: {
      intent_id?: string;
      reference_pubkey?: string;
    };
  }>;
}

export interface X402PaymentSignaturePayload {
  intent_id: string;
  tx_sig: string;
}

export interface X402ConfirmResponse extends SubmitCreditsPaymentResponse {
  httpStatus: 200 | 202;
  retryAfterSeconds?: number;
}

export interface X402PaymentContext {
  required: X402PaymentRequired;
  accept: X402PaymentRequired["accepts"][number];
  intentId: string;
  referencePubkey?: string;
  amount: string;
  asset: string;
  payTo: string;
  network: string;
}

export interface TopUpCreditsX402Options {
  pay: (
    payment: X402PaymentContext,
  ) => Promise<string | { txSig: string }> | string | { txSig: string };
  timeoutMs?: number;
  pollIntervalMs?: number;
}

export interface X402TopUpResult {
  intent: CreditsIntentStatus;
  credits: CreditsStatus;
}

export class PipeError extends Error {
  readonly status: number;
  readonly body: string;

  constructor(message: string, status: number, body: string) {
    super(message);
    this.name = "PipeError";
    this.status = status;
    this.body = body;
  }

  static async fromResponse(response: Response): Promise<PipeError> {
    let body = "";
    try {
      body = await response.text();
    } catch {
      body = response.statusText;
    }
    return new PipeError(
      `Pipe API request failed (${response.status} ${response.statusText})`,
      response.status,
      body,
    );
  }
}

export class X402ProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "X402ProtocolError";
  }
}

export class X402ConflictError extends PipeError {
  readonly intent?: CreditsIntentStatus;

  constructor(message: string, status: number, body: string, intent?: CreditsIntentStatus) {
    super(message, status, body);
    this.name = "X402ConflictError";
    this.intent = intent;
  }
}

export class X402PendingIntentError extends Error {
  readonly intent: CreditsIntentStatus;

  constructor(intent: CreditsIntentStatus) {
    super(intent.error_message || `Credits intent ${intent.intent_id} is pending with an error`);
    this.name = "X402PendingIntentError";
    this.intent = intent;
  }
}

export class X402TimeoutError extends Error {
  readonly intentId: string;

  constructor(intentId: string) {
    super(`Timed out waiting for credits intent ${intentId} to be credited`);
    this.name = "X402TimeoutError";
    this.intentId = intentId;
  }
}

const DEFAULT_BASE_URL = "https://us-west-01-firestarter.pipenetwork.com";
const DEFAULT_TIMEOUT_MS = 120_000;
const DEFAULT_POLL_INTERVAL_MS = 1_000;
const MAX_RESPONSE_BYTES = 256 * 1024 * 1024; // 256 MB
const SDK_USER_AGENT = "pipe-agent-storage-ts/0.1.0";

function getEnv(name: string): string | undefined {
  const g = globalThis as { process?: { env?: Record<string, string | undefined> } };
  return g.process?.env?.[name];
}

function normalizeBaseUrl(url: string): string {
  return url.trim().replace(/\/+$/, "");
}

function isHexHash(value: string): boolean {
  return /^[A-Fa-f0-9]{64}$/.test(value);
}

function isUuid(value: string): boolean {
  return /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$/.test(
    value,
  );
}

function randomSuffix(length = 12): string {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let out = "";
  for (let i = 0; i < length; i += 1) {
    out += chars[Math.floor(Math.random() * chars.length)];
  }
  return out;
}

function toBytes(data: unknown): Uint8Array {
  if (data instanceof Uint8Array) {
    return data;
  }
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }
  if (typeof data === "string") {
    return new TextEncoder().encode(data);
  }
  const json = JSON.stringify(data);
  return new TextEncoder().encode(json);
}

function toRequestBody(data: unknown): Blob {
  const bytes = toBytes(data);
  const buf = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buf).set(bytes);
  return new Blob([buf]);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function encodeBase64(input: string): string {
  const g = globalThis as {
    btoa?: (value: string) => string;
    Buffer?: { from: (value: string, encoding?: string) => { toString: (encoding: string) => string } };
  };
  if (g.btoa) {
    return g.btoa(input);
  }
  if (g.Buffer) {
    return g.Buffer.from(input, "utf-8").toString("base64");
  }
  throw new Error("Base64 encoding is not available in this runtime");
}

function decodeBase64(input: string): string {
  const g = globalThis as {
    atob?: (value: string) => string;
    Buffer?: { from: (value: string, encoding?: string) => { toString: (encoding: string) => string } };
  };
  if (g.atob) {
    return g.atob(input);
  }
  if (g.Buffer) {
    return g.Buffer.from(input, "base64").toString("utf-8");
  }
  throw new Error("Base64 decoding is not available in this runtime");
}

export function encodeJsonToBase64(value: unknown): string {
  return encodeBase64(JSON.stringify(value));
}

export function decodeJsonFromBase64<T>(value: string): T {
  return JSON.parse(decodeBase64(value)) as T;
}

export function decodePaymentRequired(headerValue: string): X402PaymentRequired {
  return decodeJsonFromBase64<X402PaymentRequired>(headerValue);
}

export function encodePaymentSignature(payload: X402PaymentSignaturePayload): string {
  return encodeJsonToBase64(payload);
}

function parseRetryAfterSeconds(value: string | null): number | undefined {
  if (!value) {
    return undefined;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : undefined;
}

function parseJsonText<T>(value: string): T | undefined {
  if (!value) {
    return undefined;
  }
  try {
    return JSON.parse(value) as T;
  } catch {
    return undefined;
  }
}

function isCreditsIntentStatus(value: unknown): value is CreditsIntentStatus {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Partial<CreditsIntentStatus>;
  return typeof candidate.intent_id === "string" && typeof candidate.status === "string";
}

function normalizeTxSignature(
  value: string | { txSig: string },
): string {
  const txSig = typeof value === "string" ? value : value.txSig;
  if (!txSig || typeof txSig !== "string" || !txSig.trim()) {
    throw new X402ProtocolError("Payment callback must return a non-empty tx signature");
  }
  return txSig.trim();
}

export class PipeStorageClient {
  private apiKey?: string;
  private authScheme: AuthScheme;
  private readonly controlBaseUrl: string;
  private readonly dataBaseUrl: string;
  private readonly account?: string;
  private readonly timeoutMs: number;
  private readonly pollIntervalMs: number;
  private readonly fetchImpl: typeof fetch;
  private refreshToken?: string;
  private refreshPromise?: Promise<AuthSession>;
  private readonly usePopGateway: boolean;

  constructor(options: PipeStorageClientOptions = {}) {
    this.apiKey = options.apiKey ?? getEnv("PIPE_API_KEY");
    this.authScheme = options.authScheme ?? "bearer";
    const explicitControlBaseUrl = options.controlBaseUrl ?? getEnv("PIPE_CONTROL_BASE_URL");
    const explicitDataBaseUrl = options.dataBaseUrl ?? getEnv("PIPE_DATA_BASE_URL");
    const fallbackBaseUrl =
      options.baseUrl ??
      getEnv("PIPE_BASE_URL") ??
      getEnv("PIPE_API_BASE_URL") ??
      DEFAULT_BASE_URL;
    this.controlBaseUrl = normalizeBaseUrl(explicitControlBaseUrl ?? fallbackBaseUrl);
    this.dataBaseUrl = normalizeBaseUrl(explicitDataBaseUrl ?? fallbackBaseUrl);
    this.account = options.account ?? getEnv("PIPE_ACCOUNT");
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.pollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
    this.fetchImpl = options.fetchImpl ?? fetch;
    this.usePopGateway = explicitControlBaseUrl !== undefined || explicitDataBaseUrl !== undefined;
  }

  async authChallenge(walletPublicKey: string): Promise<ChallengeResponse> {
    const response = await this.request(
      "/auth/siws/challenge",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ wallet_public_key: walletPublicKey }),
      },
      false,
    );
    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }
    return (await response.json()) as ChallengeResponse;
  }

  async authVerify(
    walletPublicKey: string,
    nonce: string,
    message: string,
    signatureB64: string,
  ): Promise<AuthSession> {
    const response = await this.request(
      "/auth/siws/verify",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          wallet_public_key: walletPublicKey,
          nonce,
          message,
          signature_b64: signatureB64,
        }),
      },
      false,
    );
    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }
    const payload = (await response.json()) as Partial<AuthSession>;
    if (!payload.access_token || !payload.refresh_token) {
      throw new Error("authVerify response is missing access_token or refresh_token");
    }
    const session: AuthSession = {
      access_token: payload.access_token,
      refresh_token: payload.refresh_token,
      csrf_token: payload.csrf_token,
    };
    this.apiKey = session.access_token;
    this.authScheme = "bearer";
    this.refreshToken = session.refresh_token;
    return session;
  }

  async authRefresh(): Promise<AuthSession> {
    if (!this.refreshToken) {
      throw new Error("No refresh token available. Call authVerify first.");
    }
    const response = await this.request(
      "/auth/refresh",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ refresh_token: this.refreshToken }),
      },
      false,
    );
    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }
    const payload = (await response.json()) as Partial<AuthSession>;
    if (!payload.access_token) {
      throw new Error("authRefresh response is missing access_token");
    }
    const nextRefreshToken = payload.refresh_token ?? this.refreshToken;
    if (!nextRefreshToken) {
      throw new Error("authRefresh response is missing refresh_token");
    }
    const session: AuthSession = {
      access_token: payload.access_token,
      refresh_token: nextRefreshToken,
      csrf_token: payload.csrf_token,
    };
    this.apiKey = session.access_token;
    this.authScheme = "bearer";
    this.refreshToken = nextRefreshToken;
    return session;
  }

  async authLogout(): Promise<void> {
    this.requireApiKey("authLogout");
    const response = await this.request(
      "/auth/logout",
      { method: "POST" },
      true,
    );
    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }
    this.apiKey = undefined;
    this.refreshToken = undefined;
  }

  async creditsStatus(): Promise<CreditsStatus> {
    const response = await this.request(
      "/api/credits/status",
      { method: "GET" },
      true,
    );
    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }
    return (await response.json()) as CreditsStatus;
  }

  async creditsIntent(intentId: string): Promise<CreditsIntentStatus> {
    if (!intentId.trim()) {
      throw new Error("creditsIntent requires a non-empty intentId");
    }
    const response = await this.request(
      `/api/credits/intent/${encodeURIComponent(intentId)}`,
      { method: "GET" },
      true,
    );
    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }
    return (await response.json()) as CreditsIntentStatus;
  }

  async requestCreditsX402(amountUsdcRaw: number): Promise<X402PaymentRequired> {
    this.validateAmountUsdcRaw(amountUsdcRaw);
    const response = await this.request(
      "/api/credits/x402",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ amount_usdc_raw: amountUsdcRaw }),
      },
      true,
    );

    if (response.status === 402) {
      const headerValue = response.headers.get("payment-required");
      if (!headerValue) {
        throw new X402ProtocolError("Missing Payment-Required header on x402 response");
      }
      return decodePaymentRequired(headerValue);
    }

    if (response.status === 409) {
      throw await this.x402ConflictFromResponse(response);
    }

    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }

    throw new X402ProtocolError(
      `Expected 402 Payment Required from /api/credits/x402, received ${response.status}`,
    );
  }

  async confirmCreditsX402(
    amountUsdcRaw: number,
    paymentSignature: X402PaymentSignaturePayload,
  ): Promise<X402ConfirmResponse> {
    this.validateAmountUsdcRaw(amountUsdcRaw);
    if (!paymentSignature.intent_id.trim() || !paymentSignature.tx_sig.trim()) {
      throw new X402ProtocolError("Payment signature requires non-empty intent_id and tx_sig");
    }

    const response = await this.request(
      "/api/credits/x402",
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "payment-signature": encodePaymentSignature(paymentSignature),
        },
        body: JSON.stringify({ amount_usdc_raw: amountUsdcRaw }),
      },
      true,
    );

    if (response.status === 409) {
      throw await this.x402ConflictFromResponse(response);
    }

    if (response.status === 400) {
      const body = await response.text();
      const payload = parseJsonText<CreditsIntentStatus>(body);
      if (payload && payload.status === "pending" && payload.error_message) {
        throw new X402PendingIntentError(payload);
      }
      throw new PipeError(
        `Pipe API request failed (${response.status} ${response.statusText})`,
        response.status,
        body,
      );
    }

    if (response.status !== 200 && response.status !== 202) {
      throw await PipeError.fromResponse(response);
    }

    const payload = (await response.json()) as SubmitCreditsPaymentResponse;
    return {
      ...payload,
      httpStatus: response.status as 200 | 202,
      retryAfterSeconds: parseRetryAfterSeconds(response.headers.get("retry-after")),
    };
  }

  async topUpCreditsX402(
    amountUsdcRaw: number,
    options: TopUpCreditsX402Options,
  ): Promise<X402TopUpResult> {
    const required = await this.requestCreditsX402(amountUsdcRaw);
    const accept = required.accepts[0];
    if (!accept) {
      throw new X402ProtocolError("Payment-Required header did not include accepts[0]");
    }

    const intentId = accept.extra?.intent_id;
    if (!intentId) {
      throw new X402ProtocolError("Payment-Required header is missing extra.intent_id");
    }

    const txSig = normalizeTxSignature(
      await options.pay({
        required,
        accept,
        intentId,
        referencePubkey: accept.extra?.reference_pubkey,
        amount: accept.amount,
        asset: accept.asset,
        payTo: accept.payTo,
        network: accept.network,
      }),
    );

    const confirm = await this.confirmCreditsX402(amountUsdcRaw, {
      intent_id: intentId,
      tx_sig: txSig,
    });

    const finalIntent =
      confirm.httpStatus === 202 || confirm.status === "processing"
        ? await this.pollCreditsIntentUntilSettled(intentId, confirm.retryAfterSeconds, options)
        : await this.creditsIntent(intentId);

    const credits = await this.creditsStatus();
    return { intent: finalIntent, credits };
  }

  deterministicUrl(contentHash: string, account?: string): string {
    const effectiveAccount = account ?? this.account;
    if (!effectiveAccount) {
      throw new Error(
        "Missing account for deterministic URL. Set client account or pass account explicitly.",
      );
    }
    if (!isHexHash(contentHash)) {
      throw new Error("contentHash must be a 64-character hex string");
    }
    return `${this.dataBaseUrl}/${encodeURIComponent(effectiveAccount)}/${contentHash.toLowerCase()}`;
  }

  async store(data: unknown, options: StoreOptions = {}): Promise<StoreResult> {
    this.requireApiKey("store");

    const tier = options.tier ?? "normal";
    const endpoint = this.usePopGateway
      ? `${this.dataBaseUrl}/v1/upload`
      : `${this.controlBaseUrl}/${tier === "priority" ? "priorityUpload" : "upload"}`;
    const fileName = options.fileName ?? `agent/${Date.now()}-${randomSuffix()}.bin`;
    const query = new URLSearchParams({ file_name: fileName });
    if (options.tier) {
      query.set("tier", options.tier);
    }

    const bodyInit: RequestInit = {
      method: "POST",
      headers: {
        "content-type": "application/octet-stream",
      },
      body: toRequestBody(data),
    };
    const endpointWithQuery = `${endpoint}?${query.toString()}`;
    let response = await this.requestAbsolute(endpointWithQuery, bodyInit, true);
    if (
      this.usePopGateway &&
      (response.status === 404 || response.status === 405)
    ) {
      const fallback = `${this.controlBaseUrl}/${tier === "priority" ? "priorityUpload" : "upload"}?${query.toString()}`;
      response = await this.requestAbsolute(fallback, bodyInit, true);
    }

    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }

    const operationId = response.headers.get("x-operation-id") ?? undefined;
    const location = response.headers.get("location") ?? undefined;

    if (options.wait === false || !operationId) {
      return {
        operationId,
        location,
        fileName,
        status: operationId ? "queued" : "completed",
      };
    }

    const completed = await this.waitForOperation(operationId, {
      timeoutMs: options.timeoutMs,
    });

    return {
      operationId: completed.operation_id,
      location,
      fileName: completed.file_name,
      status: completed.status,
      contentHash: completed.content_hash ?? undefined,
      deterministicUrl: completed.deterministic_url ?? undefined,
    };
  }

  async checkStatus(params: {
    operationId?: string;
    fileName?: string;
  }): Promise<UploadStatus> {
    this.requireApiKey("checkStatus");

    const query = new URLSearchParams();
    if (params.operationId) {
      query.set("operation_id", params.operationId);
    }
    if (params.fileName) {
      query.set("file_name", params.fileName);
    }
    if (!query.toString()) {
      throw new Error("checkStatus requires operationId or fileName");
    }

    const response = await this.request(
      `${this.usePopGateway ? "/pop/v1/checkUploadStatus" : "/checkUploadStatus"}?${query.toString()}`,
      { method: "GET" },
      true,
    );

    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }

    return (await response.json()) as UploadStatus;
  }

  async waitForOperation(
    operationId: string,
    options: { timeoutMs?: number } = {},
  ): Promise<UploadStatus> {
    this.requireApiKey("waitForOperation");

    const timeoutMs = options.timeoutMs ?? this.timeoutMs;
    const started = Date.now();
    let consecutiveErrors = 0;
    const maxTransientErrors = 3;

    while (Date.now() - started < timeoutMs) {
      let status: UploadStatus;
      try {
        status = await this.checkStatus({ operationId });
        consecutiveErrors = 0;
      } catch (err) {
        consecutiveErrors += 1;
        if (consecutiveErrors >= maxTransientErrors || (err instanceof PipeError && err.status < 500)) {
          throw err;
        }
        await sleep(this.pollIntervalMs);
        continue;
      }
      if (status.status === "completed") {
        return status;
      }
      if (status.status === "failed") {
        throw new PipeError(
          `Upload failed for operation ${operationId}: ${status.error ?? "unknown error"}`,
          409,
          status.error ?? "upload failed",
        );
      }
      await sleep(this.pollIntervalMs);
    }

    throw new Error(`Timed out waiting for operation ${operationId}`);
  }

  async pin(input: string | PinInput): Promise<PinResult> {
    if (typeof input === "string") {
      if (/^https?:\/\//i.test(input)) {
        return { url: input };
      }
      if (isHexHash(input)) {
        return {
          url: this.deterministicUrl(input),
          contentHash: input.toLowerCase(),
          status: "completed",
        };
      }
      if (isUuid(input)) {
        return this.pin({ operationId: input });
      }
      return this.pin({ fileName: input });
    }

    if (input.contentHash) {
      return {
        url: this.deterministicUrl(input.contentHash, input.account),
        contentHash: input.contentHash.toLowerCase(),
        status: "completed",
      };
    }

    if (!input.operationId && !input.fileName) {
      throw new Error("pin requires operationId, fileName, contentHash, or deterministic URL");
    }

    const status = await this.checkStatus({
      operationId: input.operationId,
      fileName: input.fileName,
    });

    if (status.status !== "completed") {
      throw new Error(
        `Cannot pin object while status is ${status.status}. operation_id=${status.operation_id}`,
      );
    }

    const hash = status.content_hash ?? undefined;
    const url =
      status.deterministic_url ??
      (hash ? this.deterministicUrl(hash, input.account) : undefined);

    if (!url) {
      throw new Error(
        "Upload completed but deterministic URL is not available yet (missing content_hash)",
      );
    }

    return {
      url,
      contentHash: hash,
      operationId: status.operation_id,
      fileName: status.file_name,
      status: status.status,
    };
  }

  async fetch(
    input: FetchInput,
    options: FetchOptions = {},
  ): Promise<Uint8Array | string | unknown> {
    const url = this.resolveFetchUrl(input);
    const isPipeUrl =
      url.startsWith(`${this.controlBaseUrl}/`) || url.startsWith(`${this.dataBaseUrl}/`);
    const isDeterministic = this.isPublicDeterministicUrl(url);
    const requiresAuth = isPipeUrl && !isDeterministic;

    if (requiresAuth) {
      this.requireApiKey("fetch");
    }

    const useAuth = requiresAuth || (isPipeUrl && !!this.apiKey);

    const response = await this.requestAbsolute(
      url,
      { method: "GET" },
      useAuth,
    );

    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }

    const buf = await response.arrayBuffer();
    if (buf.byteLength > MAX_RESPONSE_BYTES) {
      throw new PipeError("Response body exceeds maximum size", 502, "response too large");
    }
    const bytes = new Uint8Array(buf);
    if (options.asJson) {
      return JSON.parse(new TextDecoder().decode(bytes));
    }
    if (options.asText) {
      return new TextDecoder().decode(bytes);
    }
    return bytes;
  }

  async delete(input: string | DeleteInput): Promise<DeleteResponse> {
    this.requireApiKey("delete");

    let fileName: string | undefined;

    if (typeof input === "string") {
      if (isUuid(input)) {
        const status = await this.checkStatus({ operationId: input });
        fileName = status.file_name;
      } else {
        fileName = input;
      }
    } else {
      if (input.fileName) {
        fileName = input.fileName;
      } else if (input.operationId) {
        const status = await this.checkStatus({ operationId: input.operationId });
        fileName = status.file_name;
      }
    }

    if (!fileName) {
      throw new Error("delete requires fileName or operationId");
    }

    const response = await this.request(
      this.usePopGateway ? "/pop/v1/deleteFile" : "/deleteFile",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ file_name: fileName }),
      },
      true,
    );

    if (!response.ok) {
      throw await PipeError.fromResponse(response);
    }

    return (await response.json()) as DeleteResponse;
  }

  private resolveFetchUrl(input: FetchInput): string {
    if (typeof input === "string") {
      if (/^https?:\/\//i.test(input)) {
        return input;
      }
      if (isHexHash(input)) {
        return this.deterministicUrl(input);
      }
      return `${this.dataBaseUrl}/download-stream?file_name=${encodeURIComponent(input)}`;
    }

    if ("url" in input) {
      return input.url;
    }

    if ("fileName" in input) {
      return `${this.dataBaseUrl}/download-stream?file_name=${encodeURIComponent(input.fileName)}`;
    }

    return this.deterministicUrl(input.contentHash, input.account);
  }

  private isPublicDeterministicUrl(url: string): boolean {
    const prefix = `${this.dataBaseUrl}/`;
    if (!url.startsWith(prefix)) {
      return false;
    }
    const tail = url.slice(prefix.length);
    const parts = tail.split("/");
    return parts.length === 2 && isHexHash(parts[1]);
  }

  private async request(
    path: string,
    init: RequestInit,
    authenticated: boolean,
  ): Promise<Response> {
    return this.requestAbsolute(`${this.controlBaseUrl}${path}`, init, authenticated);
  }

  private async requestAbsolute(
    url: string,
    init: RequestInit,
    authenticated: boolean,
  ): Promise<Response> {
    const headers = new Headers(init.headers ?? {});
    headers.set("user-agent", SDK_USER_AGENT);
    if (authenticated) {
      this.requireApiKey("authenticated request");
      headers.set("authorization", this.authorizationHeader());
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const response = await this.fetchImpl(url, {
        ...init,
        headers,
        signal: controller.signal,
      });

      if (
        response.status === 401 &&
        authenticated &&
        this.authScheme === "bearer" &&
        this.refreshToken
      ) {
        try {
          await this.deduplicatedRefresh();
        } catch {
          return response;
        }
        headers.set("authorization", this.authorizationHeader());
        const controller2 = new AbortController();
        const timer2 = setTimeout(() => controller2.abort(), this.timeoutMs);
        try {
          return await this.fetchImpl(url, {
            ...init,
            headers,
            signal: controller2.signal,
          });
        } finally {
          clearTimeout(timer2);
        }
      }

      return response;
    } finally {
      clearTimeout(timer);
    }
  }

  private async deduplicatedRefresh(): Promise<AuthSession> {
    if (this.refreshPromise) {
      return this.refreshPromise;
    }
    this.refreshPromise = this.authRefresh().finally(() => {
      this.refreshPromise = undefined;
    });
    return this.refreshPromise;
  }

  private requireApiKey(action: string): void {
    if (!this.apiKey) {
      throw new Error(
        `Missing API key for ${action}. Set PIPE_API_KEY or pass { apiKey } to the client.`,
      );
    }
  }

  private validateAmountUsdcRaw(amountUsdcRaw: number): void {
    if (!Number.isInteger(amountUsdcRaw) || amountUsdcRaw <= 0) {
      throw new Error("amountUsdcRaw must be a positive integer");
    }
  }

  private authorizationHeader(): string {
    return this.authScheme === "apiKey"
      ? `ApiKey ${this.apiKey}`
      : `Bearer ${this.apiKey}`;
  }

  private async x402ConflictFromResponse(response: Response): Promise<X402ConflictError> {
    const body = await response.text();
    const payload = parseJsonText<CreditsIntentConflictResponse>(body);
    return new X402ConflictError(
      payload?.error || `x402 request failed (${response.status} ${response.statusText})`,
      response.status,
      body,
      payload?.intent,
    );
  }

  private async pollCreditsIntentUntilSettled(
    intentId: string,
    retryAfterSeconds: number | undefined,
    options: TopUpCreditsX402Options,
  ): Promise<CreditsIntentStatus> {
    const timeoutMs = options.timeoutMs ?? this.timeoutMs;
    const pollIntervalMs = options.pollIntervalMs ?? this.pollIntervalMs;
    const deadline = Date.now() + timeoutMs;
    let delayMs = (retryAfterSeconds ?? 0) * 1000 || pollIntervalMs;

    while (Date.now() < deadline) {
      await sleep(delayMs);
      const intent = await this.creditsIntent(intentId);
      if (intent.status === "credited") {
        return intent;
      }
      if (intent.status === "pending" && intent.error_message) {
        throw new X402PendingIntentError(intent);
      }
      delayMs = pollIntervalMs;
    }

    throw new X402TimeoutError(intentId);
  }
}

export function createPipeStorageClient(
  options: PipeStorageClientOptions = {},
): PipeStorageClient {
  return new PipeStorageClient(options);
}

export * from "./frameworks/openai.js";
export * from "./frameworks/anthropic.js";
export * from "./frameworks/vercel.js";
export * from "./frameworks/cloudflare.js";
export * from "./frameworks/langchain.js";
export * from "./frameworks/llamaindex.js";
