# Agent Workload Bench

This benchmark targets agent-like write patterns:

- Object sizes: 1KB to 200KB (configurable)
- High write frequency
- Parallel uploads

Default host:
`https://us-west-01-firestarter.pipenetwork.com`

This default host is production; benchmark traffic is real and may incur usage cost.
Set `PIPE_BASE_URL` to target staging or local environments.

## Run

```bash
cd typescript
npm install
npm run build
cd ..

export PIPE_API_KEY="<token>"
export PIPE_ACCOUNT="<account_or_slug>"   # optional, but useful for deterministic URL checks

node ./bench/agent_workload.mjs
```

## Configuration

- `PIPE_BENCH_SIZES` comma-separated bytes (default: `1024,10240,51200,102400,204800`)
- `PIPE_BENCH_WRITES_PER_SIZE` (default: `30`)
- `PIPE_BENCH_CONCURRENCY` (default: `16`)
- `PIPE_BENCH_WAIT` wait for each upload completion (default: `true`)
- `PIPE_BENCH_DELETE_AFTER` cleanup uploaded test files (default: `true`)
- `PIPE_BENCH_TIMEOUT_MS` request timeout (default: `240000`)
- `PIPE_BENCH_POLL_MS` upload poll interval (default: `1000`)
- `PIPE_BENCH_OUT` output JSON path (default: `bench/results/bench-<timestamp>.json`)
- `PIPE_BASE_URL` or `PIPE_API_BASE_URL` override host

## Output

Per-size metrics include:

- success/failure counts
- success rate
- latency `avg`, `p50`, `p95`, `p99`
- sample error messages
