# logq

Local-first JSONL / CSV / Parquet log explorer. Point at a folder and get a web UI with SQL, histograms, facets, charts, live tail, and exports. Powered by DuckDB.

**Site:** https://alldoq.github.io/logq/ · **Releases:** https://github.com/alldoq/logq/releases · **Container:** `ghcr.io/alldoq/logq`

## Install

### Docker / Podman

```bash
docker run --rm -p 7777:7777 -v "$PWD/logs:/data" ghcr.io/alldoq/logq:latest
```

### Homebrew

```bash
brew install alldoq/tap/logq
```

### From source

```bash
cargo build --release
```

The DuckDB engine is bundled (no system DuckDB needed); first build is slow.

## Use

```bash
./target/release/logq ./var/log
# http://localhost:7777
```

Sources:
- A directory path: walks recursively.
- `-`: read JSONL or plain text from stdin (`kubectl logs ... | logq -`).
- `http(s)://...` or `s3://bucket/key`: single-file remote source via DuckDB's `httpfs` extension (auto-installed on first use).

Flags:
- `--port <N>`: port (default `7777`)
- `--host <H>`: bind address (default `127.0.0.1`)
- `--no-open`: don't auto-launch the browser
- `--tail`: start the live-tail watcher at startup
- `--remote user@host:/path/*.jsonl`: stream a remote machine's logs into the live-tail panel over SSH (key-based auth required). May be passed multiple times.
- `--token <T>`: require a bearer token on every `/api/*` request. Also honours `LOGQ_TOKEN`.

UI features:
- **Live tail**: toggle button streams newly appended lines via WebSocket.
- **Chart toggle**: switch any GROUP BY result between table, bar, and line.
- **Copy link**: encodes the current SQL + view into the URL hash for sharing.
- **Column-header stats**: click any header for top-25 value counts and distinct count, scoped to the current query.
- **JSON cell expand**: click a nested cell to open a pretty-printed JSON modal.
- **Export**: CSV and Parquet download buttons run a `COPY (...) TO` on the current query.
- **Schema overrides**: drop a `.logq/schema.yml` in the target dir to coerce columns:

  ```yaml
  columns:
    ts:
      type: TIMESTAMP
      format: "%Y-%m-%dT%H:%M:%S%z"   # optional strptime fmt
    dur_ms:
      type: DOUBLE
  ```

## What it does

- Scans the directory for JSONL (`.jsonl`, `.ndjson`, `.json.gz`, `.jsonl.zst`, etc.), CSV/TSV (including `.csv.gz`), Parquet, and plain text (`.log`, `.txt`, `.out`, `.log.gz`). All inputs are UNION'd by name into one `logs` view.
- For plain-text files, each line is exposed as `msg` (VARCHAR), with a best-effort regex extraction of a leading ISO8601 `ts` and a `level` token (`INFO`, `WARN`, `ERROR`, `DEBUG`, `TRACE`, `FATAL`, etc.). Unparseable lines keep `ts`/`level` NULL and the full line in `msg`.
- Registers the view via DuckDB `read_json_auto` / `read_csv_auto` / `read_parquet` (union schema, ignore errors).
- Infers the timestamp column (`ts`, `timestamp`, `time`, `@timestamp`, or any TIMESTAMP-typed column).
- Infers the level column (`level`, `severity`, etc.).
- Detects low-cardinality facets (`level`, `service`, `host`, `env`, `status`, ...).
- Serves an embedded SPA: SQL editor, results table, time histogram, facet panel, saved queries.
- Saves named queries to `.logq/queries.yml` in the target directory.

## CLI subcommands

```bash
logq /var/log query "SELECT level, COUNT(*) FROM logs GROUP BY 1" --format ndjson
logq /var/log schema
```

`query` accepts `--format json|ndjson|csv|tsv`. `schema` prints inferred column types as JSON.

## HTTP API

All endpoints are also mounted under `/api/v1/*`. When `--token` is set, requests must carry `Authorization: Bearer <token>`, `X-Logq-Token: <token>`, or `?token=<token>` (WebSocket).

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/health` | – | `{status, version, dir, files, columns}` |
| GET | `/api/meta` | – | dir, files, columns, inferred ts/level/facets |
| POST | `/api/query` | `{sql}` | `{columns, rows, elapsed_ms, error?}` |
| POST | `/api/histogram` | `{column?, buckets?, where_clause?}` | `{column, buckets:[{bucket, count}]}` |
| POST | `/api/column-stats` | `{column, sql?}` | `{column, distinct, top_values:[[label, n]]}` |
| POST | `/api/export` | `{sql, format: csv\|json\|parquet}` | binary download |
| GET  | `/api/saved` | – | `{queries:[{name, sql, description}]}` |
| POST | `/api/saved` | `{name, sql, description?}` | `{ok}` |
| POST | `/api/saved/delete` | `{name}` | `{ok}` |
| POST | `/api/tail/start` | – | `{started}` |
| GET (ws) | `/api/tail` | – | newline-framed JSON `{file, line, remote?}` |

See [API.md](API.md) for examples.

## Status

Covers the `logq-spec.md` v0.1 MVP and most v0.5/v1.0 items: live tail, chart toggle, URL state, schema overrides, `.zst` support, remote tail over SSH, CSV/Parquet ingest, exports, Docker image, Homebrew tap. Not yet: performance hardening for 100 GB+ datasets, custom-format inference plugins, daemon/alert mode.

## License

MIT OR Apache-2.0
