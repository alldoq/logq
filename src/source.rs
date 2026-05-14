use crate::scan::{self, FileEntry, FileKind};
use anyhow::{anyhow, Result};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Resolved log source. May be:
/// - a real directory (`dir` is canonicalised, `files` from walking it),
/// - stdin (`dir` points to a held tempdir containing a single jsonl file),
/// - one or more http(s)/s3 URLs (`files` is a synthetic entry per URL,
///   `kind` inferred from extension).
///
/// `_tempdir` (when present) keeps the temp data alive for the process lifetime.
pub struct Resolved {
    pub dir: PathBuf,
    pub files: Vec<FileEntry>,
    pub label: String,
    #[allow(dead_code)]
    pub _tempdir: Option<tempfile::TempDir>,
}

pub fn resolve(arg: &str) -> Result<Resolved> {
    // Stdin: `-` or `--`
    if arg == "-" {
        return resolve_stdin();
    }
    // Remote URLs handed straight to DuckDB via httpfs.
    if arg.starts_with("http://") || arg.starts_with("https://") || arg.starts_with("s3://") {
        return resolve_url(arg);
    }
    // Local directory.
    let p = PathBuf::from(arg);
    let dir = p.canonicalize().unwrap_or(p);
    let files = scan::scan_dir(&dir)?;
    let label = format!("{} ({} files)", dir.display(), files.len());
    Ok(Resolved { dir, files, label, _tempdir: None })
}

fn resolve_stdin() -> Result<Resolved> {
    let td = tempfile::Builder::new().prefix("logq-stdin-").tempdir()?;
    let path = td.path().join("stdin.jsonl");

    // Sniff first line to decide whether stdin is JSONL or plain text.
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let mut buf = Vec::with_capacity(4096);
    use std::io::Read;
    let mut sniff = [0u8; 1024];
    let n = lock.read(&mut sniff)?;
    buf.extend_from_slice(&sniff[..n]);
    let is_jsonl = sniff[..n].iter().position(|&c| c == b'\n')
        .and_then(|nl| std::str::from_utf8(&sniff[..nl]).ok())
        .map(|first| {
            let t = first.trim_start();
            t.starts_with('{') || t.starts_with('[')
        })
        .unwrap_or(false);

    let final_path = if is_jsonl { path } else { td.path().join("stdin.log") };

    let f = std::fs::File::create(&final_path)?;
    let mut w = BufWriter::new(f);
    w.write_all(&buf)?;
    std::io::copy(&mut lock, &mut w)?;
    w.flush()?;

    let dir = td.path().to_path_buf();
    let files = scan::scan_dir(&dir)?;
    let kind_label = if is_jsonl { "jsonl" } else { "text" };
    let label = format!("stdin ({kind_label})");
    Ok(Resolved { dir, files, label, _tempdir: Some(td) })
}

fn resolve_url(url: &str) -> Result<Resolved> {
    let kind = classify_url(url).ok_or_else(|| anyhow!(
        "URL extension not recognised (need .jsonl/.json.gz/.csv/.parquet etc): {url}"
    ))?;
    // Use a tempdir as the "directory" so saved queries land somewhere writable.
    let td = tempfile::Builder::new().prefix("logq-url-").tempdir()?;
    let rel = url.rsplit('/').next().unwrap_or("remote").to_string();
    let files = vec![FileEntry {
        path: PathBuf::from(url), // DuckDB reads the URL directly
        rel_path: rel.clone(),
        size: 0,
        kind,
    }];
    let label = format!("{url}");
    Ok(Resolved { dir: td.path().to_path_buf(), files, label, _tempdir: Some(td) })
}

fn classify_url(url: &str) -> Option<FileKind> {
    // Strip query string before extension lookup.
    let path = url.split('?').next().unwrap_or(url);
    let lc = path.to_lowercase();
    if lc.ends_with(".jsonl") || lc.ends_with(".ndjson") { return Some(FileKind::Jsonl); }
    if lc.ends_with(".jsonl.gz") || lc.ends_with(".ndjson.gz") || lc.ends_with(".json.gz") {
        return Some(FileKind::JsonlGz);
    }
    if lc.ends_with(".jsonl.zst") || lc.ends_with(".ndjson.zst") || lc.ends_with(".json.zst") {
        return Some(FileKind::JsonlZst);
    }
    if lc.ends_with(".json") { return Some(FileKind::Json); }
    if lc.ends_with(".csv") || lc.ends_with(".tsv") { return Some(FileKind::Csv); }
    if lc.ends_with(".csv.gz") || lc.ends_with(".tsv.gz") { return Some(FileKind::CsvGz); }
    if lc.ends_with(".parquet") || lc.ends_with(".pq") { return Some(FileKind::Parquet); }
    if lc.ends_with(".log") || lc.ends_with(".txt") { return Some(FileKind::Log); }
    None
}

/// Whether any file entry is a remote (URL) source. Used by db.rs to load httpfs.
pub fn has_remote(files: &[FileEntry]) -> bool {
    files.iter().any(|f| {
        let s = f.path.to_string_lossy();
        s.starts_with("http://") || s.starts_with("https://") || s.starts_with("s3://")
    })
}

#[allow(dead_code)]
pub fn _unused(_p: &Path) {}
