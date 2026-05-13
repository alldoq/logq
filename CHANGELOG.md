# Changelog

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
