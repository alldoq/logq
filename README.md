# logq

Local-first JSONL log explorer. Point at a folder, get a web UI with SQL, histograms, facets. Powered by DuckDB.

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

## What it does

- Scans dir for `.jsonl`, `.ndjson`, `.jsonl.gz`, `.ndjson.gz`, `.json.gz`.
- Registers a `logs` view via DuckDB `read_json_auto` (union schema, ignore errors).
- Infers timestamp column (`ts`, `timestamp`, `time`, `@timestamp`, or any TIMESTAMP-typed col).
- Infers level column (`level`, `severity`, …).
- Detects low-cardinality facets (`level`, `service`, `host`, `env`, `status`, …).
- Serves an embedded SPA: SQL editor, results table, time histogram, facet panel, saved queries.
- Saves named queries to `.logq/queries.yml` in the target directory.

## Status

v0.1 MVP per `logq-spec.md` §12. Not yet implemented: live tail, gzip beyond DuckDB's native support, remote tail over SSH, column-type overrides, schema overrides via `.logq/schema.yml`.

## License

MIT OR Apache-2.0
