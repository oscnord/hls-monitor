# One-Shot Validation Mode — Design

## Goal

Add a `validate` CLI command that fetches an HLS playlist tree once, runs all applicable checks, and exits with a structured error report. No polling, no stale check, no webhooks.

## Usage

```
hls-monitor validate <URL> [--json]
```

## Behavior

1. Fetch master playlist, discover all variants/renditions
2. Fetch each variant playlist
3. Run all per-variant checks (target duration, gap, duration anomaly, PDT, daterange, playlist type)
4. Run stream-level checks (sync drift, availability)
5. Skip stale manifest check
6. Print results and exit

Checks requiring previous state (mseq regression, version change, playlist size/content, segment continuity, discontinuity) naturally produce no errors on first poll — no special handling needed.

## Output

- Default: styled error list to stderr, errors to stdout
- `--json`: JSON array of MonitorError to stdout
- Exit code: 0 = clean, 1 = violations found

## Implementation

Reuse existing `Monitor::poll_once()` — it already fetches master + variants and runs all checks in a single pass. The `validate` command creates a Monitor, calls `poll_once()`, collects errors, and exits.
