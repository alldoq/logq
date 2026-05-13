use crate::db::{detect_facets, infer_level_col, infer_timestamp_col, Db, QueryResult};
use crate::saved::{self, SavedQueries, SavedQuery};
use crate::scan::FileEntry;
use anyhow::Result;
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

pub struct AppState {
    pub dir: PathBuf,
    pub files: Vec<FileEntry>,
    pub db: Mutex<Db>,
    pub inferred_ts_col: Option<String>,
    pub inferred_level_col: Option<String>,
    pub facets: Vec<String>,
    pub columns: Vec<(String, String)>,
}

impl AppState {
    pub fn new(dir: PathBuf, files: Vec<FileEntry>) -> Result<Arc<Self>> {
        let db = Db::open()?;
        let paths: Vec<std::path::PathBuf> = files.iter().map(|f| f.path.clone()).collect();
        db.register_logs_view(&paths)?;
        let columns = db.columns().unwrap_or_default();
        let ts = infer_timestamp_col(&columns);
        let lvl = infer_level_col(&columns);
        let facets = detect_facets(&db, &columns);
        Ok(Arc::new(Self {
            dir,
            files,
            db: Mutex::new(db),
            inferred_ts_col: ts,
            inferred_level_col: lvl,
            facets,
            columns,
        }))
    }
}

pub async fn serve(state: Arc<AppState>, host: &str, port: u16) -> Result<()> {
    let app = Router::new()
        .route("/api/meta", get(meta))
        .route("/api/query", post(query))
        .route("/api/histogram", post(histogram))
        .route("/api/saved", get(get_saved).post(post_saved))
        .route("/api/saved/delete", post(delete_saved))
        .fallback(static_handler)
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Serialize)]
struct Meta {
    dir: String,
    files: Vec<FileEntry>,
    columns: Vec<ColInfo>,
    timestamp_col: Option<String>,
    level_col: Option<String>,
    facets: Vec<String>,
}

#[derive(Serialize)]
struct ColInfo {
    name: String,
    r#type: String,
}

async fn meta(State(s): State<Arc<AppState>>) -> Json<Meta> {
    Json(Meta {
        dir: s.dir.to_string_lossy().to_string(),
        files: s.files.clone(),
        columns: s.columns.iter().map(|(n, t)| ColInfo { name: n.clone(), r#type: t.clone() }).collect(),
        timestamp_col: s.inferred_ts_col.clone(),
        level_col: s.inferred_level_col.clone(),
        facets: s.facets.clone(),
    })
}

#[derive(Deserialize)]
struct QueryReq {
    sql: String,
}

#[derive(Serialize)]
struct QueryResp {
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
    error: Option<String>,
    elapsed_ms: u128,
}

async fn query(State(s): State<Arc<AppState>>, Json(req): Json<QueryReq>) -> Json<QueryResp> {
    let start = std::time::Instant::now();
    let res: Result<QueryResult> = {
        let db = s.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query(&req.sql)
    };
    let elapsed = start.elapsed().as_millis();
    match res {
        Ok(r) => Json(QueryResp { columns: r.columns, rows: r.rows, error: None, elapsed_ms: elapsed }),
        Err(e) => Json(QueryResp { columns: vec![], rows: vec![], error: Some(e.to_string()), elapsed_ms: elapsed }),
    }
}

#[derive(Deserialize)]
struct HistReq {
    #[serde(default)]
    column: Option<String>,
    #[serde(default = "default_buckets")]
    buckets: u32,
    #[serde(default)]
    where_clause: Option<String>,
}

fn default_buckets() -> u32 { 50 }

#[derive(Serialize)]
struct HistBucket {
    bucket: String,
    count: i64,
}

#[derive(Serialize)]
struct HistResp {
    column: Option<String>,
    buckets: Vec<HistBucket>,
    error: Option<String>,
}

async fn histogram(State(s): State<Arc<AppState>>, Json(req): Json<HistReq>) -> Json<HistResp> {
    let col = req.column.or_else(|| s.inferred_ts_col.clone());
    let col = match col {
        Some(c) => c,
        None => return Json(HistResp { column: None, buckets: vec![], error: Some("no timestamp column".into()) }),
    };
    let safe_col = col.replace('"', "\"\"");
    let where_part = match req.where_clause.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(w) => format!("WHERE {}", w),
        None => String::new(),
    };
    // Coerce to TIMESTAMP (handles VARCHAR ISO8601 + epoch ms/s + native TIMESTAMP).
    let ts_expr = format!(
        "COALESCE(TRY_CAST(\"{c}\" AS TIMESTAMP), \
                  TRY_CAST(TRY_CAST(\"{c}\" AS VARCHAR) AS TIMESTAMP))",
        c = safe_col
    );
    let sql = format!(
        "WITH t AS (SELECT {ts} AS ts FROM logs {w}), \
         r AS (SELECT MIN(ts) AS lo, MAX(ts) AS hi FROM t WHERE ts IS NOT NULL) \
         SELECT time_bucket(\
             (GREATEST(EXTRACT(EPOCH FROM (SELECT hi FROM r)) - EXTRACT(EPOCH FROM (SELECT lo FROM r)), 1.0) / {b} * INTERVAL 1 SECOND), \
             ts) AS b, COUNT(*) AS c \
         FROM t WHERE ts IS NOT NULL GROUP BY b ORDER BY b",
        ts = ts_expr, w = where_part, b = req.buckets
    );
    let db = s.db.lock().unwrap_or_else(|e| e.into_inner());
    match db.query(&sql) {
        Ok(r) => {
            let buckets: Vec<HistBucket> = r.rows.into_iter().filter_map(|row| {
                let bucket = match row.get(0)? {
                    serde_json::Value::String(s) => s.clone(),
                    v => v.to_string(),
                };
                let count = match row.get(1)? {
                    serde_json::Value::Number(n) => n.as_i64().unwrap_or(0),
                    _ => 0,
                };
                Some(HistBucket { bucket, count })
            }).collect();
            Json(HistResp { column: Some(col), buckets, error: None })
        }
        Err(e) => Json(HistResp { column: Some(col), buckets: vec![], error: Some(e.to_string()) }),
    }
}

async fn get_saved(State(s): State<Arc<AppState>>) -> Json<SavedQueries> {
    Json(saved::load(&s.dir).unwrap_or_default())
}

#[derive(Deserialize)]
struct SaveReq {
    name: String,
    sql: String,
    #[serde(default)]
    description: String,
}

async fn post_saved(State(s): State<Arc<AppState>>, Json(req): Json<SaveReq>) -> (StatusCode, Json<serde_json::Value>) {
    let mut qs = saved::load(&s.dir).unwrap_or_default();
    qs.queries.retain(|q| q.name != req.name);
    qs.queries.push(SavedQuery { name: req.name, sql: req.sql, description: req.description });
    match saved::save(&s.dir, &qs) {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

#[derive(Deserialize)]
struct DelReq { name: String }

async fn delete_saved(State(s): State<Arc<AppState>>, Json(req): Json<DelReq>) -> (StatusCode, Json<serde_json::Value>) {
    let mut qs = saved::load(&s.dir).unwrap_or_default();
    qs.queries.retain(|q| q.name != req.name);
    match saved::save(&s.dir, &qs) {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], file.data.into_owned()).into_response()
        }
        None => {
            // SPA fallback
            if let Some(idx) = Assets::get("index.html") {
                ([(header::CONTENT_TYPE, "text/html")], idx.data.into_owned()).into_response()
            } else {
                (StatusCode::NOT_FOUND, "not found").into_response()
            }
        }
    }
}
