# logq

Local-first JSONL log explorer. Point at a folder, get a web UI with SQL, histograms, facets. Powered by DuckDB.

**Site:** https://alldoq.github.io/logq/ · **Releases:** https://github.com/alldoq/logq/releases

## Build

```bash
cargo build --release
```

The DuckDB engine is bundled (no system DuckDB needed) — first build is slow.

## Use

```bash
./target/release/logq ./var/log
# → http://localhost:7777
```

Flags:
- `--port <N>` — port (default `7777`)
- `--host <H>` — bind (default `127.0.0.1`)
- `--no-open` — don't auto-launch browser
- `--tail` — start the live-tail watcher at startup
- `--remote user@host:/path/*.jsonl` — stream a remote machine's logs into the live-tail panel over SSH (key-based auth required). May be passed multiple times

UI features:
- **Live tail** — toggle button streams newly appended lines via WebSocket.
- **Chart toggle** — switch any GROUP BY result between table/bar/line.
- **Copy link** — encodes the current SQL + view into the URL hash for sharing.
- **Schema overrides** — drop a `.logq/schema.yml` in the target dir to coerce columns:

  ```yaml
  columns:
    ts:
      type: TIMESTAMP
      format: "%Y-%m-%dT%H:%M:%S%z"   # optional strptime fmt
    dur_ms:
      type: DOUBLE
  ```

## What it does

- Scans dir for `.jsonl`, `.ndjson`, `.jsonl.gz`, `.ndjson.gz`, `.json.gz`, `.jsonl.zst`, `.ndjson.zst`, `.json.zst`.
- Registers a `logs` view via DuckDB `read_json_auto` (union schema, ignore errors).
- Infers timestamp column (`ts`, `timestamp`, `time`, `@timestamp`, or any TIMESTAMP-typed col).
- Infers level column (`level`, `severity`, …).
- Detects low-cardinality facets (`level`, `service`, `host`, `env`, `status`, …).
- Serves an embedded SPA: SQL editor, results table, time histogram, facet panel, saved queries.
- Saves named queries to `.logq/queries.yml` in the target directory.

## Status

Covers `logq-spec.md` §12 v0.1 MVP plus most v0.5/v1.0 items: live tail, chart toggle, URL state, schema overrides, `.zst` support, remote tail over SSH. Not yet: Homebrew formula, performance hardening for 100 GB+ datasets, custom-format inference plugins.

## License

MIT OR Apache-2.0
