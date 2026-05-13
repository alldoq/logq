use anyhow::{anyhow, Result};
use duckdb::types::{TimeUnit, ValueRef};
use duckdb::{params, Connection};
use serde_json::{json, Value};

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    /// Register a `logs` view over the given files.
    pub fn register_logs_view(&self, files: &[std::path::PathBuf]) -> Result<()> {
        if files.is_empty() {
            // Empty placeholder view so DESCRIBE doesn't crash.
            self.conn.execute(
                "CREATE OR REPLACE VIEW logs AS SELECT NULL::VARCHAR AS msg WHERE FALSE",
                params![],
            )?;
            return Ok(());
        }
        let list = files
            .iter()
            .map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "CREATE OR REPLACE VIEW logs AS \
             SELECT * FROM read_json_auto([{}], \
                 union_by_name=true, \
                 ignore_errors=true, \
                 maximum_object_size=33554432, \
                 filename=true)",
            list
        );
        self.conn.execute(&sql, params![])?;
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
