# Changelog

## v0.1.2 — 2026-05-13

- `.jsonl.zst` / `.ndjson.zst` / `.json.zst` support — decompressed to a
  tempdir at startup and registered with DuckDB.
- `--remote user@host:/path/*.jsonl` flag — streams a remote machine's
  logs through `ssh tail -F` into the live-tail panel. Multiple allowed.
- **CSV / TSV / Parquet** support — `.csv`, `.tsv`, `.csv.gz`, `.parquet`
  scanned and UNION'd by name into the `logs` view alongside JSONL.
- **Column-header stats** — click any header for top-25 values + distinct
  count via a new `/api/column-stats` endpoint (scoped to current query).
- **JSON cell expand** — click a nested cell to open a pretty-printed
  modal with the full structure.
- **Export** — CSV and Parquet download buttons (`/api/export`) run a
  `COPY (...) TO` on the current SQL.
- **Dockerfile** + GHCR multi-arch publish workflow on tag push.
- **Homebrew tap template** + `scripts/update-homebrew.sh` to render
  `Formula/logq.rb` from a published release's SHA256 sums.

## v0.1.1 — 2026-05-13

- Live tail: WebSocket stream of newly appended lines (`--tail` flag or UI toggle).
- Chart toggle: bar / line / table view for any GROUP BY result.
- Copy-link button encodes SQL + view in URL hash for shareable views.
- Schema overrides via `.logq/schema.yml` — coerce columns to TIMESTAMP / BIGINT / DOUBLE etc.
- Fix: Windows MSVC link error (`unresolved external symbol RmStartSession`) — link `rstrtmgr.lib` for the DuckDB bundled C++ build.

## v0.1.0 — 2026-05-13

First release. Weekend MVP per `logq-spec.md` §12.

### Features
- Single Rust binary with bundled DuckDB engine
- Auto-discovers `.jsonl`, `.ndjson`, `.jsonl.gz`, `.ndjson.gz`, `.json.gz` files under a directory
- Registers a `logs` view via DuckDB `read_json_auto` (union schema, ignore errors)
- Inferred timestamp column (`ts`, `timestamp`, `time`, `@timestamp`, …, or any TIMESTAMP-typed col)
- Inferred level column (`level`, `severity`, …)
- Low-cardinality facet detection (`level`, `service`, `host`, `env`, `status`, …)
- Embedded SPA: SQL editor (⌘↵), results table, time histogram (SVG), facet panel, saved queries
- Saved queries persist to `.logq/queries.yml` in target directory
- Auto-opens browser
- Bundled sample dataset (~470k rows across 12 gzipped JSONL files, ~150 MB uncompressed)

### Not yet
- Live tail / WebSocket push
- Remote tail over SSH
- Column-type overrides (`.logq/schema.yml`)
- Cross-file joins with friendly file aliases
- Pre-built binaries for Linux / Windows
- Homebrew formula
