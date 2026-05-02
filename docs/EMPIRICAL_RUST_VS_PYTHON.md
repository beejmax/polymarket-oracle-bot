# Empirical Rust vs Python Check

Date: 2026-05-02

## Question

Should the bot be implemented in Rust or Python?

## Local Hot-Path Benchmark

Command:

```bash
PYTHONPATH=src python3 scripts/bench_hot_path.py --iterations 200000 --sample-every 1
cargo run --release -q -p oracle-bot-rs --bin bench_hot_path -- --iterations 200000 --sample-every 1
```

Workload:

- update one CLOB book snapshot
- read best ask
- evaluate signal
- size accepted Python signal through the risk manager

Results:

| runtime | p50 | p90 | p99 | max | throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| Python | 7.034 us | 7.174 us | 11.262 us | 54.576 us | 135,062 ops/s |
| Rust | 0.551 us | 0.561 us | 0.711 us | 26.692 us | 1,578,871 ops/s |

Rust is much faster locally, roughly 12.8x at p50 on this synthetic benchmark.
But Python's absolute local signal path is still about 0.007 ms p50 and 0.011 ms
p99, which is tiny relative to exchange/feed latencies measured below.

## Live Public-Feed Capture

Command:

```bash
timeout 30s env PYTHONPATH=src /tmp/poly_clob_inspect/bin/python -m poly_oracle_bot \
  --config config.example.toml \
  --db data/empirical.sqlite3 \
  --no-dashboard
PYTHONPATH=src python3 scripts/analyze_telemetry.py data/events.jsonl
```

Results from the 30 second sample:

- events: 4,100
- CLOB quote lag: p50 22 ms, p90 35 ms, p99 81 ms
- Chainlink RTDS raw lag was dominated by initial snapshot rows
- Chainlink RTDS rows under 5s lag: p50 about 2.661s, p90 about 4.160s

## Decision

Do not move the whole project to Rust right now.

Use Python as the primary runtime while validating:

- price-to-beat capture
- signal thresholds
- orderbook behavior
- live credential/auth flow
- order response and reconciliation behavior
- recorded telemetry and replay tooling

Rust remains the right eventual hot-path option if the measured live order path
proves that local runtime jitter is material. Current evidence says it is not:
Python's local signal loop is microseconds, while current public feed/orderbook
latency is milliseconds to seconds.

## Revisit Criteria

Reconsider Rust for live execution if any of these are measured:

- Python signal-to-submit p99 exceeds 20-30 ms after direct async execution is
  implemented.
- `py-clob-client` signing/submission adds material blocking or jitter.
- Direct Chainlink Data Streams and direct CLOB submission bring external
  latencies low enough that local microseconds matter.
- The system moves from one/few markets to high fan-out market making where
  millions of local operations per second matter.

Near-term priority is not Rust. It is direct measurement of live order
sign/build/submit latency and correctness of reconciliation.

Use the no-submit executor preflight before any live run:

```bash
poly-oracle-bot --config config.toml --db data/bot.sqlite3 --executor-preflight
```
