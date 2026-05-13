use anyhow::Result;
use notify::{event::ModifyKind, Event, EventKind, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Blocking watcher: monitors all files; emits each newly-appended line on `tx`.
/// Skips gzipped files (no tail semantics).
pub fn watch(files: Vec<PathBuf>, tx: broadcast::Sender<String>) -> Result<()> {
    // Initialise per-file read offset at current EOF — only new content streams.
    let mut offsets: HashMap<PathBuf, u64> = HashMap::new();
    for p in &files {
        if is_gz(p) { continue; }
        if let Ok(meta) = std::fs::metadata(p) {
            offsets.insert(p.clone(), meta.len());
        }
    }

    let (event_tx, event_rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = event_tx.send(res);
    })?;

    // Watch each file's parent (notify on most platforms wants dirs).
    let mut watched_dirs = std::collections::HashSet::new();
    for p in &files {
        if let Some(parent) = p.parent() {
            if watched_dirs.insert(parent.to_path_buf()) {
                let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
            }
        }
    }

    loop {
        let evt = match event_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(e)) => Some(e),
            Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Determine which files to poll
        let to_check: Vec<PathBuf> = match evt {
            Some(e) => match e.kind {
                EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any => {
                    e.paths.into_iter().filter(|p| !is_gz(p) && files.iter().any(|f| f == p)).collect()
                }
                _ => continue,
            },
            None => offsets.keys().cloned().collect(),
        };

        for path in to_check {
            let cur_off = *offsets.get(&path).unwrap_or(&0);
            let new_off = match read_new_lines(&path, cur_off, &tx) {
                Ok(n) => n,
                Err(_) => cur_off,
            };
            offsets.insert(path, new_off);
        }
    }
    Ok(())
}

fn read_new_lines(path: &PathBuf, from: u64, tx: &broadcast::Sender<String>) -> Result<u64> {
    let mut f = File::open(path)?;
    let meta = f.metadata()?;
    let len = meta.len();
    let start = if len < from { 0 } else { from };
    f.seek(SeekFrom::Start(start))?;
    let mut reader = BufReader::new(&f);
    let mut line = String::new();
    let mut last = start;
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 { break; }
        last += n as u64;
        // Only emit complete lines
        if line.ends_with('\n') {
            let trimmed = line.trim_end_matches(&['\n', '\r'][..]).to_string();
            if !trimmed.is_empty() {
                let payload = serde_json::json!({
                    "file": path.to_string_lossy(),
                    "line": trimmed,
                }).to_string();
                let _ = tx.send(payload);
            }
        } else {
            // partial trailing line — rewind to keep it for next read
            last -= n as u64;
            break;
        }
    }
    Ok(last)
}

fn is_gz(p: &PathBuf) -> bool {
    p.extension().map(|e| e == "gz").unwrap_or(false)
}
