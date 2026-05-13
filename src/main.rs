mod scan;
mod db;
mod server;
mod saved;
mod tail;
mod zst;
mod remote;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "logq", version, about = "Local-first JSONL log explorer powered by DuckDB")]
struct Cli {
    /// Directory containing JSONL/JSON.gz/log files
    #[arg(default_value = ".")]
    dir: PathBuf,

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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "logq=info,tower_http=warn".into()))
        .init();

    let cli = Cli::parse();
    let dir = cli.dir.canonicalize().unwrap_or(cli.dir.clone());

    println!("→ Scanning {}...", dir.display());
    let files = scan::scan_dir(&dir)?;
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    println!("→ Found {} files ({:.2} MB)", files.len(), total_bytes as f64 / 1_048_576.0);

    let counts = scan::count_by_kind(&files);
    let mut parts: Vec<String> = counts.iter().map(|(k, v)| format!("{} ({} files)", k, v)).collect();
    parts.sort();
    if !parts.is_empty() {
        println!("→ Detected: {}", parts.join(", "));
    }

    let state = server::AppState::new(dir.clone(), files)?;

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
