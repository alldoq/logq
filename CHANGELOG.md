# Changelog

## v0.1.3 (2026-05-14)

- Plain-text log support (`.log`, `.txt`, `.out`, `.log.gz`). Each line
  becomes one row with `msg`, plus a best-effort regex extraction of a
  leading ISO8601 `ts` and a `level` token.
- HTTP API now also mounted under `/api/v1/*`, with `/api/health`.
- Optional bearer-token auth via `--token` or `LOGQ_TOKEN`. Accepted as
  `Authorization: Bearer`, `X-Logq-Token`, or `?token=` on the WebSocket.
- `logq query "<sql>"` and `logq schema` subcommands for one-shot CLI use
  without the web server. `query` supports `json|ndjson|csv|tsv` output.

## v0.1.2 (2026-05-13)

- `.jsonl.zst`, `.ndjson.zst`, `.json.zst` support, decompressed to a
  tempdir at startup and registered with DuckDB.
- `--remote user@host:/path/*.jsonl` flag streams a remote machine's
  logs through `ssh tail -F` into the live-tail panel. Multiple allowed.
- **CSV / TSV / Parquet** support. `.csv`, `.tsv`, `.csv.gz`, `.parquet`
  are scanned and UNION'd by name into the `logs` view alongside JSONL.
- **Column-header stats**: click any header for top-25 values plus
  distinct count via a new `/api/column-stats` endpoint (scoped to the
  current query).
- **JSON cell expand**: click a nested cell to open a pretty-printed
  modal with the full structure.
- **Export**: CSV and Parquet download buttons (`/api/export`) run a
  `COPY (...) TO` on the current SQL.
- **Dockerfile** and a GHCR multi-arch publish workflow on tag push.
- **Homebrew tap template** plus `scripts/update-homebrew.sh` to render
  `Formula/logq.rb` from a published release's SHA256 sums.

## v0.1.1 (2026-05-13)

- Live tail: WebSocket stream of newly appended lines (`--tail` flag or
  UI toggle).
- Chart toggle: bar, line, or table view for any GROUP BY result.
- Copy-link button encodes SQL and view in the URL hash for shareable
  views.
- Schema overrides via `.logq/schema.yml` coerce columns to TIMESTAMP,
  BIGINT, DOUBLE, etc.
- Fix: Windows MSVC link error (`unresolved external symbol
  RmStartSession`); link `rstrtmgr.lib` for the DuckDB bundled C++ build.

## v0.1.0 (2026-05-13)

First release. Weekend MVP per `logq-spec.md` §12.

### Features
- Single Rust binary with bundled DuckDB engine.
- Auto-discovers `.jsonl`, `.ndjson`, `.jsonl.gz`, `.ndjson.gz`, and
  `.json.gz` files under a directory.
- Registers a `logs` view via DuckDB `read_json_auto` (union schema,
  ignore errors).
- Inferred timestamp column (`ts`, `timestamp`, `time`, `@timestamp`, or
  any TIMESTAMP-typed column).
- Inferred level column (`level`, `severity`, ...).
- Low-cardinality facet detection (`level`, `service`, `host`, `env`,
  `status`, ...).
- Embedded SPA: SQL editor (Cmd+Enter), results table, time histogram
  (SVG), facet panel, saved queries.
- Saved queries persist to `.logq/queries.yml` in the target directory.
- Auto-opens the browser.
- Bundled sample dataset (about 470k rows across 12 gzipped JSONL files,
  about 150 MB uncompressed).

### Not yet
- Live tail or WebSocket push.
- Remote tail over SSH.
- Column-type overrides (`.logq/schema.yml`).
- Cross-file joins with friendly file aliases.
- Pre-built binaries for Linux or Windows.
- Homebrew formula.
