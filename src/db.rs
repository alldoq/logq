use anyhow::{anyhow, Result};
use duckdb::types::{TimeUnit, ValueRef};
use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct Db {
    pub conn: Connection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaOverride {
    /// e.g. "TIMESTAMP", "BIGINT", "VARCHAR", "DOUBLE"
    pub r#type: String,
    /// optional strftime format for TIMESTAMP parse (passed to strptime)
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SchemaConfig {
    /// map column name -> override
    #[serde(default)]
    pub columns: HashMap<String, SchemaOverride>,
}

pub fn load_schema_config(dir: &std::path::Path) -> SchemaConfig {
    let p = dir.join(".logq").join("schema.yml");
    if !p.exists() { return SchemaConfig::default(); }
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_yaml::from_str(&s).unwrap_or_default(),
        Err(_) => SchemaConfig::default(),
    }
}

impl Db {
    pub fn open() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    /// Register a `logs` view over the given files, applying optional schema overrides.
    /// Files are dispatched per-kind to the right DuckDB reader, then UNION'd via union_by_name.
    pub fn register_logs_view(
        &self,
        json_files: &[std::path::PathBuf],
        csv_files: &[std::path::PathBuf],
        parquet_files: &[std::path::PathBuf],
        text_files: &[std::path::PathBuf],
        schema: &SchemaConfig,
    ) -> Result<()> {
        if json_files.is_empty() && csv_files.is_empty() && parquet_files.is_empty() && text_files.is_empty() {
            self.conn.execute(
                "CREATE OR REPLACE VIEW logs_raw AS SELECT NULL::VARCHAR AS msg WHERE FALSE",
                params![],
            )?;
            self.conn.execute(
                "CREATE OR REPLACE VIEW logs AS SELECT * FROM logs_raw",
                params![],
            )?;
            return Ok(());
        }

        let fmt_list = |files: &[std::path::PathBuf]| -> String {
            files
                .iter()
                .map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut parts: Vec<String> = Vec::new();
        if !json_files.is_empty() {
            parts.push(format!(
                "SELECT * FROM read_json_auto([{}], \
                     union_by_name=true, ignore_errors=true, \
                     maximum_object_size=33554432, filename=true)",
                fmt_list(json_files)
            ));
        }
        if !csv_files.is_empty() {
            parts.push(format!(
                "SELECT * FROM read_csv_auto([{}], union_by_name=true, filename=true)",
                fmt_list(csv_files)
            ));
        }
        if !parquet_files.is_empty() {
            parts.push(format!(
                "SELECT * FROM read_parquet([{}], union_by_name=true, filename=true)",
                fmt_list(parquet_files)
            ));
        }
        if !text_files.is_empty() {
            // Read each file as a single VARCHAR column named `raw` (one row per line).
            // A delimiter that never appears + skip header off ensures the line is the row.
            // Then try to peel off a leading ISO8601 timestamp and a level token so users
            // can filter by `ts` / `level` without hand-writing regex every time.
            parts.push(format!(
                "WITH t AS (\
                     SELECT column0 AS raw, filename FROM read_csv([{}], \
                         columns={{'column0':'VARCHAR'}}, \
                         delim='\\x1F', quote='', escape='', header=false, \
                         filename=true, ignore_errors=true) \
                 ) \
                 SELECT \
                   TRY_CAST(regexp_extract(raw, '([0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}[T ][0-9]{{2}}:[0-9]{{2}}:[0-9]{{2}}(?:\\.[0-9]+)?(?:Z|[+-][0-9:]+)?)', 1) AS TIMESTAMP) AS ts, \
                   NULLIF(regexp_extract(raw, '\\b(TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|FATAL|CRIT(?:ICAL)?)\\b', 1), '') AS level, \
                   raw AS msg, \
                   filename \
                 FROM t",
                fmt_list(text_files)
            ));
        }
        let body = if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            parts.into_iter().map(|p| format!("({})", p)).collect::<Vec<_>>().join(" UNION ALL BY NAME ")
        };
        let raw_sql = format!("CREATE OR REPLACE VIEW logs_raw AS {}", body);
        self.conn.execute(&raw_sql, params![])?;

        // Get raw columns to build override projection
        let raw_cols = {
            let mut stmt = self.conn.prepare("DESCRIBE logs_raw")?;
            let rows = stmt.query_map([], |row| {
                let n: String = row.get(0)?;
                let t: String = row.get(1)?;
                Ok((n, t))
            })?;
            let mut out = Vec::new();
            for r in rows { out.push(r?); }
            out
        };

        let projection: Vec<String> = raw_cols.iter().map(|(name, _orig_type)| {
            let q = name.replace('"', "\"\"");
            match schema.columns.get(name) {
                Some(ov) => {
                    let tgt = ov.r#type.to_uppercase();
                    if tgt == "TIMESTAMP" {
                        match &ov.format {
                            Some(fmt) => {
                                let fmt_esc = fmt.replace('\'', "''");
                                format!("strptime(CAST(\"{q}\" AS VARCHAR), '{fmt_esc}') AS \"{q}\"")
                            }
                            None => format!("TRY_CAST(\"{q}\" AS TIMESTAMP) AS \"{q}\""),
                        }
                    } else {
                        format!("TRY_CAST(\"{q}\" AS {tgt}) AS \"{q}\"")
                    }
                }
                None => format!("\"{q}\""),
            }
        }).collect();

        let view_sql = format!(
            "CREATE OR REPLACE VIEW logs AS SELECT {} FROM logs_raw",
            if projection.is_empty() { "*".to_string() } else { projection.join(", ") }
        );
        self.conn.execute(&view_sql, params![])?;
        Ok(())
    }

    /// Return (column_name, column_type) for the `logs` view.
    pub fn columns(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("DESCRIBE logs")?;
        let rows = stmt.query_map([], |row| {
            let n: String = row.get(0)?;
            let t: String = row.get(1)?;
            Ok((n, t))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Execute a SQL query, returning columns + JSON rows.
    pub fn query(&self, sql: &str) -> Result<QueryResult> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query([])?;
        let col_count = rows.as_ref().map(|s| s.column_count()).unwrap_or(0);
        let col_names: Vec<String> = match rows.as_ref() {
            Some(s) => (0..col_count)
                .map(|i| s.column_name(i).map(|c| c.to_string()).unwrap_or_default())
                .collect(),
            None => Vec::new(),
        };
        let mut data: Vec<Vec<Value>> = Vec::new();
        let limit = 10_000usize;
        while let Some(row) = rows.next()? {
            let mut out_row = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let v: ValueRef = row.get_ref(i)?;
                out_row.push(value_ref_to_json(v));
            }
            data.push(out_row);
            if data.len() >= limit {
                break;
            }
        }
        Ok(QueryResult { columns: col_names, rows: data })
    }
}

#[derive(serde::Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

fn value_ref_to_json(v: ValueRef) -> Value {
    use ValueRef::*;
    match v {
        Null => Value::Null,
        Boolean(b) => json!(b),
        TinyInt(n) => json!(n),
        SmallInt(n) => json!(n),
        Int(n) => json!(n),
        BigInt(n) => json!(n),
        HugeInt(n) => json!(n.to_string()),
        UTinyInt(n) => json!(n),
        USmallInt(n) => json!(n),
        UInt(n) => json!(n),
        UBigInt(n) => json!(n),
        Float(f) => json!(f),
        Double(f) => json!(f),
        Decimal(d) => json!(d.to_string()),
        Timestamp(unit, v) => json!(format_timestamp(unit, v)),
        Text(t) => match std::str::from_utf8(t) {
            Ok(s) => json!(s),
            Err(_) => json!(String::from_utf8_lossy(t).to_string()),
        },
        Blob(b) => json!(format!("<{} bytes>", b.len())),
        Date32(d) => json!(d),
        Time64(_, t) => json!(t),
        Interval { months, days, nanos } => json!(format!("{}mo {}d {}ns", months, days, nanos)),
        Enum(_, _) => json!(format!("{:?}", v)),
        List(..) | Array(..) | Struct(_, _) | Map(..) | Union(_, _) => {
            json!(format!("{:?}", v))
        }
    }
}

fn format_timestamp(unit: TimeUnit, v: i64) -> String {
    let (secs, nanos) = match unit {
        TimeUnit::Second => (v, 0i64),
        TimeUnit::Millisecond => (v / 1000, (v % 1000) * 1_000_000),
        TimeUnit::Microsecond => (v / 1_000_000, (v % 1_000_000) * 1000),
        TimeUnit::Nanosecond => (v / 1_000_000_000, v % 1_000_000_000),
    };
    match chrono::DateTime::from_timestamp(secs, nanos as u32) {
        Some(dt) => dt.to_rfc3339(),
        None => v.to_string(),
    }
}

/// Heuristic: pick a timestamp column from a column list.
pub fn infer_timestamp_col(cols: &[(String, String)]) -> Option<String> {
    let preferred = ["ts", "timestamp", "time", "@timestamp", "datetime", "event_time", "created_at"];
    for p in preferred {
        if let Some((n, _)) = cols.iter().find(|(n, _)| n.eq_ignore_ascii_case(p)) {
            return Some(n.clone());
        }
    }
    // Any TIMESTAMP-typed column
    cols.iter()
        .find(|(_, t)| t.to_uppercase().contains("TIMESTAMP") || t.to_uppercase().contains("DATE"))
        .map(|(n, _)| n.clone())
}

pub fn infer_level_col(cols: &[(String, String)]) -> Option<String> {
    let candidates = ["level", "severity", "lvl", "log_level", "loglevel"];
    for c in candidates {
        if let Some((n, _)) = cols.iter().find(|(n, _)| n.eq_ignore_ascii_case(c)) {
            return Some(n.clone());
        }
    }
    None
}

/// Best-effort low-cardinality facet detection (≤ 50 distinct values, type VARCHAR-ish).
pub fn detect_facets(db: &Db, cols: &[(String, String)]) -> Vec<String> {
    let mut facets = Vec::new();
    let preferred = ["level", "service", "host", "env", "status", "kind", "type", "logger"];
    for p in preferred {
        if let Some((n, t)) = cols.iter().find(|(n, _)| n.eq_ignore_ascii_case(p)) {
            if t.to_uppercase().contains("VARCHAR") || t.to_uppercase().contains("TEXT") {
                // Verify low cardinality
                let q = format!("SELECT COUNT(DISTINCT \"{}\") FROM logs", n.replace('"', "\"\""));
                if let Ok(mut stmt) = db.conn.prepare(&q) {
                    if let Ok(c) = stmt.query_row::<i64, _, _>([], |r| r.get(0)) {
                        if c > 0 && c <= 50 {
                            facets.push(n.clone());
                        }
                    }
                }
            }
        }
    }
    facets
}

#[allow(dead_code)]
pub fn ensure_safe(sql: &str) -> Result<()> {
    let lower = sql.to_lowercase();
    for bad in ["attach ", "copy ", "install ", "load ", "pragma ", "create table", "drop "] {
        if lower.contains(bad) {
            return Err(anyhow!("statement not allowed: {}", bad.trim()));
        }
    }
    Ok(())
}
