# hls-monitor

Real-time HLS stream monitor that detects playlist anomalies across one or more live streams. Available as a CLI tool for quick debugging or as an HTTP API server for integration into monitoring infrastructure.

## What it detects

- **Media sequence regression** — `EXT-X-MEDIA-SEQUENCE` going backwards
- **Discontinuity sequence issues** — `EXT-X-DISCONTINUITY-SEQUENCE` increments that don't match the playlist
- **Stale manifests** — playlists that stop updating beyond a configurable threshold
- **Playlist size shrinkage** — segment count decreasing on the same media sequence
- **Playlist content changes** — segments changing on the same media sequence
- **Segment continuity breaks** — unexpected first segment after a sliding window advance
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
hls-monitor watch https://example.com/master.m3u8 --stale-limit 8000 --scte35 --webhook-url https://hooks.example.com/alerts
```

Run the API server with a config file:

```
hls-monitor serve --config config.toml
```

Or start an empty server and create monitors via the API:

```
hls-monitor serve
hls-monitor serve --listen 127.0.0.1:9090
```

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
    "scte35": true
  }'
```

## Metrics

The `/metrics` endpoint serves OpenMetrics-compatible output (Prometheus-scrapable). Includes monitor state, error counts by type, stream-level totals, manifest fetch errors, and uptime.

## Webhooks

Webhooks deliver JSON payloads via POST on errors and events. Each `[[webhook]]` entry in the config can filter which notification types to receive via the `events` list (empty means all). Payloads can be signed with HMAC-SHA256 by setting `secret` — the signature is sent in the `X-HLS-Signature-256` header.

## Project structure

| Crate      | Description                              |
| ---------- | ---------------------------------------- |
| `hls-core` | Library — monitors, checks, webhooks     |
| `hls-api`  | HTTP server — Axum routes, metrics       |
| `hls-cli`  | CLI binary — `serve` and `watch` commands|

## Building

```
cargo build --release
```

The binary is at `target/release/hls-monitor`. Requires Rust 1.75+.

Run tests:

```
cargo test
```

## License

MIT
