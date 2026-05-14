# logq HTTP API

All endpoints are mounted at both `/api/*` and `/api/v1/*`. The v1 prefix is the stable contract; the unversioned paths are kept for the browser UI.

## Auth

When started with `--token <T>` or with `LOGQ_TOKEN` in the environment, every `/api/*` request must carry the token via one of:

- `Authorization: Bearer <T>`
- `X-Logq-Token: <T>`
- `?token=<T>` (query string, used by the WebSocket upgrade)

Unauthenticated requests return `401 Unauthorized`. Without a token, the API is open (defaulting to a `127.0.0.1` bind).

## Endpoints

### GET `/api/v1/health`

```json
{
  "status": "ok",
  "version": "0.1.3",
  "dir": "/var/log",
  "files": 14,
  "columns": 22
}
```

### GET `/api/v1/meta`

Returns the scanned directory, discovered files, inferred timestamp / level columns, and detected facets.

```json
{
  "dir": "/var/log",
  "files": [{"path": "...", "rel_path": "app.jsonl", "size": 1234, "kind": "Jsonl"}],
  "columns": [{"name": "ts", "type": "TIMESTAMP"}, ...],
  "timestamp_col": "ts",
  "level_col": "level",
  "facets": ["level", "service"]
}
```

### POST `/api/v1/query`

```bash
curl -s -X POST localhost:7777/api/v1/query \
  -H 'Content-Type: application/json' \
  -d '{"sql": "SELECT level, COUNT(*) FROM logs GROUP BY 1"}'
```

```json
{"columns": ["level", "count_star()"], "rows": [["INFO", 4012]], "elapsed_ms": 7, "error": null}
```

Rows are capped at 10,000 server-side. Use `LIMIT` for shape control.

### POST `/api/v1/histogram`

```json
{"column": "ts", "buckets": 60, "where_clause": "level='ERROR'"}
```

```json
{"column": "ts", "buckets": [{"bucket": "2026-03-01T00:00:00Z", "count": 124}]}
```

`column` defaults to the inferred timestamp column. Strings are auto-cast to TIMESTAMP where possible.

### POST `/api/v1/column-stats`

```json
{"column": "service", "sql": "SELECT * FROM logs WHERE level='ERROR'"}
```

```json
{"column": "service", "distinct": 6, "top_values": [["api", 1500], ["worker", 600]]}
```

`sql` is optional; without it stats are over the full `logs` view.

### POST `/api/v1/export`

```json
{"sql": "SELECT * FROM logs WHERE level='ERROR'", "format": "csv"}
```

Returns a streamed download. `format` is `csv`, `json`, or `parquet`. CSV and Parquet exclude DuckDB's auto-injected `filename` column so files round-trip cleanly back into logq.

### Saved queries

- `GET /api/v1/saved`
- `POST /api/v1/saved` with `{name, sql, description?}`
- `POST /api/v1/saved/delete` with `{name}`

Persisted to `.logq/queries.yml` in the target directory.

### Live tail

- `POST /api/v1/tail/start` arms the watcher (idempotent).
- `GET /api/v1/tail` upgrades to a WebSocket. Each frame is a JSON line:
  ```json
  {"file": "/var/log/app.jsonl", "line": "{\"ts\":..., \"level\":\"INFO\"}", "remote": false}
  ```
  Local files use `notify` with byte-offset tracking. Remote sources (from `--remote user@host:/path`) are shelled out to `ssh tail -F` and tagged with `"remote": true`.

## CLI mode

For one-shot queries without the web server:

```bash
logq /var/log schema
logq /var/log query "SELECT level, COUNT(*) FROM logs GROUP BY 1" --format ndjson
```

Formats: `json` (default), `ndjson`, `csv`, `tsv`.

## Error shape

Query and column-stats endpoints return `{"error": "<message>"}` with HTTP 200 so the UI can render diagnostics inline. Export returns HTTP 400 with `{"error": ...}` on bad SQL.
