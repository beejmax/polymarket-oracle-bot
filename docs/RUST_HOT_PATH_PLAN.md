# Rust Hot-Path Plan

## Target Split

- Rust owns the latency-sensitive path: price/orderbook ingest, signal eval, risk gates,
  order build/sign/submit, and order reconciliation.
- Python owns research, calibration, reports, and backfills.
- Storage is off the hot path. Runtime writes to an async journal queue after decisions.

## Phase 1: Feed And Signal Parity

Status: started.

- Run Rust and Python in paper mode against the same live markets.
- Compare market discovery, Chainlink ticks, CLOB quotes, and emitted signal fields.
- Add structured latency spans:
  - tick receive
  - quote receive
  - signal evaluate
  - risk approve/reject
  - journal enqueue

Exit criteria:

- Rust discovers the same active markets as Python.
- Rust receives Chainlink ticks and CLOB quote snapshots for all enabled assets.
- Rust emits the same accepted/rejected signal decisions for a sampled run.

## Phase 2: Recorder And Replay

- Record raw Chainlink RTDS messages and CLOB market websocket messages.
- Record normalized ticks, quotes, markets, and signals.
- Build a deterministic replay runner over recorded frames.
- Use replay to calibrate threshold parameters before any live order work.

Exit criteria:

- A replay over captured data reproduces live paper signals exactly.
- Signal thresholds can be evaluated against actual spreads and feed timing.

## Phase 3: Live Execution Client

- Implement direct CLOB REST order client in Rust.
- Add FOK and FAK order modes.
- Implement auth/signing without blocking the runtime.
- Enforce position creation only after immediate matched response.
- Keep three explicit live gates.

Exit criteria:

- Preflight validates credentials, balance/allowance, market readiness, and order formatting.
- Live execution can be tested with a deliberately tiny FOK order and reconciled state.

## Phase 4: Reconciliation And Recovery

- Add user websocket or REST polling reconciliation.
- Track matched, delayed, rejected, failed, and cancelled states.
- Add restart recovery from durable journal.
- Add heartbeat/cancel safety.

Exit criteria:

- Process restart recovers open exposure and continues settlement/reconciliation.
- No duplicate position can be opened for an asset/window after restart.

## Phase 5: Production Runtime

- Add systemd service files.
- Add health check and alerting.
- Add structured JSON logs.
- Add metrics export or latency summary endpoint.
- Add deployment preflight.

Exit criteria:

- Runtime can operate unattended in paper mode for 24h with bounded memory growth.
- Live mode remains impossible without passing all preflight and explicit gates.

