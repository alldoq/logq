mod scan;
mod db;
mod server;
mod saved;
mod tail;
mod zst;
mod remote;
mod source;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "logq", version, about = "Local-first JSONL log explorer powered by DuckDB")]
struct Cli {
    /// Source: directory path, `-` for stdin, or an http(s)/s3 URL
    #[arg(default_value = ".")]
    dir: String,

    /// Port to listen on
    #[arg(long, default_value_t = 7777)]
    port: u16,

    /// Host/bind address
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Open browser automatically
    #[arg(long, default_value_t = false)]
    no_open: bool,

    /// Auto-start live tail watcher
    #[arg(long, default_value_t = false)]
    tail: bool,

    /// Stream remote logs over SSH: --remote user@host:/var/log/*.jsonl
    /// Multiple allowed. Requires `ssh` on PATH and key-based auth.
    #[arg(long = "remote")]
    remote: Vec<String>,

    /// Require this bearer token on every /api/* request (also honours
    /// LOGQ_TOKEN env var). Useful when binding to 0.0.0.0.
    #[arg(long)]
    token: Option<String>,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run a one-shot SQL query against the directory and print JSON/CSV/TSV to stdout.
    Query {
        /// SQL to run against the `logs` view
        #[arg(value_name = "SQL")]
        sql: String,
        /// Output format: json (default), ndjson, csv, tsv
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Print the inferred schema (columns + types) as JSON.
    Schema {},
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "logq=info,tower_http=warn".into()))
        .init();

    let cli = Cli::parse();
    let resolved = source::resolve(&cli.dir)?;

    // Subcommands run without the HTTP server.
    if let Some(cmd) = &cli.cmd {
        return run_subcommand(cmd, &resolved).await;
    }

    println!("→ Source: {}", resolved.label);
    let files = resolved.files.clone();
    let dir = resolved.dir.clone();
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    println!("→ Found {} files ({:.2} MB)", files.len(), total_bytes as f64 / 1_048_576.0);

    let counts = scan::count_by_kind(&files);
    let mut parts: Vec<String> = counts.iter().map(|(k, v)| format!("{} ({} files)", k, v)).collect();
    parts.sort();
    if !parts.is_empty() {
        println!("→ Detected: {}", parts.join(", "));
    }

    let token = cli.token.clone().or_else(|| std::env::var("LOGQ_TOKEN").ok().filter(|s| !s.is_empty()));
    let state = server::AppState::new(dir.clone(), files, token.clone())?;
    if token.is_some() {
        println!("→ Auth: bearer token required for /api/*");
    }

    if let Some(ts) = &state.inferred_ts_col {
        println!("→ Inferred timestamp column: \"{}\"", ts);
    }
    if let Some(lvl) = &state.inferred_level_col {
        println!("→ Inferred level column: \"{}\"", lvl);
    }

    if cli.tail {
        use std::sync::atomic::Ordering;
        state.tail_enabled.store(true, Ordering::SeqCst);
        let paths: Vec<std::path::PathBuf> = state.files.iter().map(|f| f.path.clone()).collect();
        let tx = state.tail_tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = tail::watch(paths, tx) {
                tracing::error!("tail watcher error: {}", e);
            }
        });
        println!("→ Live tail watcher started");
    }

    for spec in &cli.remote {
        let spec = spec.clone();
        let tx = state.tail_tx.clone();
        println!("→ Streaming remote: {}", spec);
        std::thread::spawn(move || {
            if let Err(e) = remote::stream(&spec, tx) {
                tracing::error!("remote tail {} error: {}", spec, e);
            }
        });
    }

    let url = format!("http://{}:{}", cli.host, cli.port);
    println!("→ {}", url);

    if !cli.no_open {
        let _ = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
            .arg(&url)
            .spawn();
    }

    server::serve(state, &cli.host, cli.port).await?;
    Ok(())
}

async fn run_subcommand(cmd: &Cmd, resolved: &source::Resolved) -> Result<()> {
    let state = server::AppState::new(resolved.dir.clone(), resolved.files.clone(), None)?;
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    match cmd {
        Cmd::Query { sql, format } => {
            let r = db.query(sql)?;
            print_result(&r, format)?;
        }
        Cmd::Schema {} => {
            let cols: Vec<serde_json::Value> = state.columns.iter()
                .map(|(n, t)| serde_json::json!({"name": n, "type": t}))
                .collect();
            println!("{}", serde_json::to_string_pretty(&cols)?);
        }
    }
    Ok(())
}

fn print_result(r: &db::QueryResult, format: &str) -> Result<()> {
    match format {
        "json" => {
            let v = serde_json::json!({ "columns": r.columns, "rows": r.rows });
            println!("{}", serde_json::to_string(&v)?);
        }
        "ndjson" => {
            for row in &r.rows {
                let obj: serde_json::Map<String, serde_json::Value> =
                    r.columns.iter().cloned().zip(row.iter().cloned()).collect();
                println!("{}", serde_json::Value::Object(obj));
            }
        }
        "csv" | "tsv" => {
            let sep = if format == "tsv" { '\t' } else { ',' };
            println!("{}", r.columns.join(&sep.to_string()));
            for row in &r.rows {
                let cells: Vec<String> = row.iter().map(|v| match v {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::String(s) => csv_escape(s, sep),
                    other => csv_escape(&other.to_string(), sep),
                }).collect();
                println!("{}", cells.join(&sep.to_string()));
            }
        }
        other => anyhow::bail!("unknown format: {other} (use json, ndjson, csv, tsv)"),
    }
    Ok(())
}

fn csv_escape(s: &str, sep: char) -> String {
    if s.contains(sep) || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
