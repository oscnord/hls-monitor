# HLS Monitor

[![CI](https://github.com/oscnord/hls-monitor/actions/workflows/ci.yml/badge.svg)](https://github.com/oscnord/hls-monitor/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

Real-time HLS stream monitor that detects playlist anomalies across one or more live streams. Available as a CLI tool for quick debugging or as an HTTP API server for integration into monitoring infrastructure.

## What it detects

**Playlist structure**
- **Target duration exceeded** — segments longer than `EXT-X-TARGETDURATION` plus tolerance
- **Segment duration anomaly** — abnormally short segments relative to target duration
- **Gap detection** — `EXT-X-GAP` tags in the playlist
- **Playlist type violation** — VOD/EVENT playlists that mutate unexpectedly
- **Version violation** — `EXT-X-VERSION` changes mid-stream

**Sequence tracking**
- **Media sequence regression** — `EXT-X-MEDIA-SEQUENCE` going backwards
- **Media sequence gap** — large jumps in media sequence between polls
- **Discontinuity sequence issues** — `EXT-X-DISCONTINUITY-SEQUENCE` increments that don't match the playlist
- **Segment continuity breaks** — unexpected first segment after a sliding window advance
- **Playlist size shrinkage** — segment count decreasing on the same media sequence
- **Playlist content changes** — segments changing on the same media sequence

**Temporal metadata**
- **Program date-time jumps** — `EXT-X-PROGRAM-DATE-TIME` discontinuities between segments
- **DateRange violations** — invalid or inconsistent `EXT-X-DATERANGE` tags

**Cross-variant**
- **Variant sync drift** — media sequence divergence between variants of the same stream
- **Variant unavailability** — variants that fail to fetch repeatedly

**Operational**
- **Stale manifests** — playlists that stop updating beyond a configurable threshold
- **SCTE-35 / CUE marker issues** — orphaned CUE-IN/CUE-OUT tags, missing continuations (opt-in)

## Install

```
cargo install --path crates/hls-cli
```

This installs the `hls-monitor` binary to `~/.cargo/bin/`.

## Quick start

Watch a single stream from the command line:

```
hls-monitor watch https://example.com/master.m3u8
```

With options:

```
hls-monitor watch https://example.com/master.m3u8 \
  --stale-limit 8000 \
  --scte35 \
  --target-duration-tolerance 1.0 \
  --max-concurrent-fetches 8 \
  --webhook-url https://hooks.example.com/alerts
```

One-shot validation (fetch once, report, exit):

```
hls-monitor validate https://example.com/master.m3u8
hls-monitor validate https://example.com/master.m3u8 --json
```

Exit code `0` means no violations, `1` means violations found. Use `--json` for machine-readable output.

Run the API server with a config file:

```
hls-monitor serve --config config.toml
```

Or start an empty server and create monitors via the API:

```
hls-monitor serve
hls-monitor serve --listen 127.0.0.1:9090
```

## Check thresholds

All check thresholds are configurable via CLI flags, TOML config, or the API:

| Flag | Description | Default |
| ---- | ----------- | ------- |
| `--scte35` | Enable SCTE-35 CUE-OUT/CUE-IN validation | `false` |
| `--request-timeout` | HTTP request timeout (ms) | `10000` |
| `--target-duration-tolerance` | Max seconds a segment may exceed EXT-X-TARGETDURATION | `0.5` |
| `--mseq-gap-threshold` | Max media sequence jump between polls | `5` |
| `--variant-sync-drift-threshold` | Max media sequence difference between variants | `3` |
| `--variant-failure-threshold` | Consecutive failures before reporting unavailable | `3` |
| `--segment-duration-anomaly-ratio` | Min ratio of segment duration to target duration | `0.5` |
| `--max-concurrent-fetches` | Max concurrent variant playlist fetches | `4` |

## Configuration

See [`config.example.toml`](config.example.toml) for all available options. Copy it and adjust to your needs:

```toml
[server]
# listen = "0.0.0.0:8080"
# log_format = "pretty"             # "pretty" or "json"

[defaults]
# stale_limit_ms = 6000             # max age before a manifest is considered stale
# poll_interval_ms = 4000           # omit to auto-derive from stale_limit
# scte35 = false                    # enable SCTE-35 / CUE marker validation
# error_limit = 100                 # max errors kept per monitor
# event_limit = 200                 # max events kept per monitor
# target_duration_tolerance = 0.5
# mseq_gap_threshold = 5
# variant_sync_drift_threshold = 3
# variant_failure_threshold = 3
# segment_duration_anomaly_ratio = 0.5
# max_concurrent_fetches = 4

[[webhook]]
url = "https://hooks.example.com/hls-alerts"
# events = []                       # empty = deliver all notification types
# secret = "hmac-sha256-key"        # signs payload with X-HLS-Signature-256

[[monitor]]
id = "live-channel-1"
stale_limit_ms = 8000
scte35 = true
streams = [
  { id = "cdn-primary", url = "https://cdn1.example.com/live/master.m3u8" },
  { url = "https://cdn2.example.com/live/master.m3u8" },
]
```

Monitors defined in the config file are auto-started when the server launches.

## API

All monitor endpoints are under `/api/v1`.

| Method   | Path                                    | Description                  |
| -------- | --------------------------------------- | ---------------------------- |
| `GET`    | `/health`                               | Health check                 |
| `GET`    | `/metrics`                              | OpenMetrics / Prometheus     |
| `POST`   | `/api/v1/monitors`                      | Create a monitor             |
| `GET`    | `/api/v1/monitors`                      | List all monitors            |
| `DELETE` | `/api/v1/monitors`                      | Delete all monitors          |
| `GET`    | `/api/v1/monitors/:id`                  | Get monitor details          |
| `DELETE` | `/api/v1/monitors/:id`                  | Stop and delete a monitor    |
| `POST`   | `/api/v1/monitors/:id/start`            | Start a monitor              |
| `POST`   | `/api/v1/monitors/:id/stop`             | Stop a monitor               |
| `GET`    | `/api/v1/monitors/:id/streams`          | List streams                 |
| `PUT`    | `/api/v1/monitors/:id/streams`          | Add streams                  |
| `DELETE` | `/api/v1/monitors/:id/streams/:sid`     | Remove a stream              |
| `GET`    | `/api/v1/monitors/:id/errors`           | Get errors                   |
| `DELETE` | `/api/v1/monitors/:id/errors`           | Clear errors                 |
| `GET`    | `/api/v1/monitors/:id/status`           | Live per-variant status      |
| `GET`    | `/api/v1/monitors/:id/events`           | Informational events         |

Create a monitor:

```
curl -X POST http://localhost:8080/api/v1/monitors \
  -H 'Content-Type: application/json' \
  -d '{
    "streams": ["https://example.com/master.m3u8"],
    "stale_limit": 8000,
    "scte35": true,
    "max_concurrent_fetches": 4
  }'
```

## Metrics

The `/metrics` endpoint serves OpenMetrics-compatible output (Prometheus-scrapable). Includes monitor state, error counts by type, stream-level totals, manifest fetch errors, and uptime.

## Webhooks

Webhooks deliver JSON payloads via POST on errors and events. Each `[[webhook]]` entry in the config can filter which notification types to receive via the `events` list (empty means all). Payloads can be signed with HMAC-SHA256 by setting `secret` — the signature is sent in the `X-HLS-Signature-256` header.

## Project structure

| Crate      | Description                                        |
| ---------- | -------------------------------------------------- |
| `hls-core` | Library — monitors, checks, webhooks               |
| `hls-api`  | HTTP server — Axum routes, metrics                 |
| `hls-cli`  | CLI binary — `serve`, `watch`, and `validate` commands |

## Building

```
cargo build --release
```

The binary is at `target/release/hls-monitor`. Requires Rust 1.85+.

Run tests:

```
cargo test
```

## License

MIT
