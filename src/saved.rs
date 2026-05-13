use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQuery {
    pub name: String,
    pub sql: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SavedQueries {
    #[serde(default)]
    pub queries: Vec<SavedQuery>,
}

pub fn queries_path(dir: &Path) -> PathBuf {
    dir.join(".logq").join("queries.yml")
}

pub fn load(dir: &Path) -> Result<SavedQueries> {
    let p = queries_path(dir);
    if !p.exists() {
        return Ok(SavedQueries::default());
    }
    let s = std::fs::read_to_string(&p)?;
    let q: SavedQueries = serde_yaml::from_str(&s).unwrap_or_default();
    Ok(q)
}

pub fn save(dir: &Path, q: &SavedQueries) -> Result<()> {
    let p = queries_path(dir);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = serde_yaml::to_string(q)?;
    std::fs::write(&p, s)?;
    Ok(())
}
