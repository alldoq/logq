use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub rel_path: String,
    pub size: u64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum FileKind {
    Jsonl,
    JsonlGz,
    JsonlZst,
    Json,
    Log,
    Csv,
    CsvGz,
    Parquet,
}

impl FileKind {
    pub fn label(self) -> &'static str {
        match self {
            FileKind::Jsonl => "jsonl",
            FileKind::JsonlGz => "gz-jsonl",
            FileKind::JsonlZst => "zst-jsonl",
            FileKind::Json => "json",
            FileKind::Log => "log",
            FileKind::Csv => "csv",
            FileKind::CsvGz => "gz-csv",
            FileKind::Parquet => "parquet",
        }
    }
}

pub fn classify(path: &Path) -> Option<FileKind> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.ends_with(".jsonl") || name.ends_with(".ndjson") {
        Some(FileKind::Jsonl)
    } else if name.ends_with(".jsonl.gz") || name.ends_with(".ndjson.gz") || name.ends_with(".json.gz") {
        Some(FileKind::JsonlGz)
    } else if name.ends_with(".jsonl.zst") || name.ends_with(".ndjson.zst") || name.ends_with(".json.zst") {
        Some(FileKind::JsonlZst)
    } else if name.ends_with(".json") {
        Some(FileKind::Json)
    } else if name.ends_with(".log") {
        Some(FileKind::Log)
    } else if name.ends_with(".csv") || name.ends_with(".tsv") {
        Some(FileKind::Csv)
    } else if name.ends_with(".csv.gz") || name.ends_with(".tsv.gz") {
        Some(FileKind::CsvGz)
    } else if name.ends_with(".parquet") || name.ends_with(".pq") {
        Some(FileKind::Parquet)
    } else {
        None
    }
}

pub fn scan_dir(root: &Path) -> Result<Vec<FileEntry>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if let Some(kind) = classify(path) {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string();
            out.push(FileEntry {
                path: path.to_path_buf(),
                rel_path: rel,
                size: meta.len(),
                kind,
            });
        }
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

pub fn count_by_kind(files: &[FileEntry]) -> HashMap<&'static str, usize> {
    let mut m = HashMap::new();
    for f in files {
        *m.entry(f.kind.label()).or_insert(0) += 1;
    }
    m
}
