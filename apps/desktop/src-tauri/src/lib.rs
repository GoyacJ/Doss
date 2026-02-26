use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::prelude::*;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use thiserror::Error;

#[derive(Debug, Error)]
enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("crypto error")]
    Crypto,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid stage transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

type AppResult<T> = Result<T, AppError>;

#[derive(Clone)]
struct AppState {
    db_path: Arc<PathBuf>,
    cipher: Arc<FieldCipher>,
}

impl AppState {
    fn new(db_path: PathBuf, seed: &str) -> Self {
        Self {
            db_path: Arc::new(db_path),
            cipher: Arc::new(FieldCipher::from_seed(seed)),
        }
    }
}

#[derive(Debug, Clone)]
struct FieldCipher {
    key: [u8; 32],
}

impl FieldCipher {
    fn from_seed(seed: &str) -> Self {
        let digest = Sha256::digest(seed.as_bytes());
        let mut key = [0_u8; 32];
        key.copy_from_slice(&digest[..32]);
        Self { key }
    }

    fn encrypt(&self, plaintext: &str) -> AppResult<String> {
        if plaintext.is_empty() {
            return Ok(String::new());
        }

        let nonce_bytes: [u8; 12] = rand::random();
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| AppError::Crypto)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let encrypted = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| AppError::Crypto)?;

        let nonce_text = BASE64_STANDARD.encode(nonce_bytes);
        let encrypted_text = BASE64_STANDARD.encode(encrypted);
        Ok(format!("{nonce_text}:{encrypted_text}"))
    }

    fn decrypt(&self, ciphertext: &str) -> AppResult<String> {
        if ciphertext.is_empty() {
            return Ok(String::new());
        }

        let mut parts = ciphertext.split(':');
        let nonce_part = parts.next().ok_or(AppError::Crypto)?;
        let data_part = parts.next().ok_or(AppError::Crypto)?;
        if parts.next().is_some() {
            return Err(AppError::Crypto);
        }

        let nonce_vec = BASE64_STANDARD.decode(nonce_part)?;
        let data = BASE64_STANDARD.decode(data_part)?;
        if nonce_vec.len() != 12 {
            return Err(AppError::Crypto);
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| AppError::Crypto)?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_vec), data.as_ref())
            .map_err(|_| AppError::Crypto)?;
        String::from_utf8(plaintext).map_err(|_| AppError::Crypto)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum PipelineStage {
    New,
    Screening,
    Interview,
    Hold,
    Rejected,
    Offered,
}

impl PipelineStage {
    fn as_db(&self) -> &'static str {
        match self {
            PipelineStage::New => "NEW",
            PipelineStage::Screening => "SCREENING",
            PipelineStage::Interview => "INTERVIEW",
            PipelineStage::Hold => "HOLD",
            PipelineStage::Rejected => "REJECTED",
            PipelineStage::Offered => "OFFERED",
        }
    }

    fn from_db(value: &str) -> AppResult<Self> {
        match value {
            "NEW" => Ok(PipelineStage::New),
            "SCREENING" => Ok(PipelineStage::Screening),
            "INTERVIEW" => Ok(PipelineStage::Interview),
            "HOLD" => Ok(PipelineStage::Hold),
            "REJECTED" => Ok(PipelineStage::Rejected),
            "OFFERED" => Ok(PipelineStage::Offered),
            _ => Err(AppError::NotFound(format!("Unknown stage: {value}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum CrawlTaskStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

impl CrawlTaskStatus {
    fn as_db(&self) -> &'static str {
        match self {
            CrawlTaskStatus::Pending => "PENDING",
            CrawlTaskStatus::Running => "RUNNING",
            CrawlTaskStatus::Succeeded => "SUCCEEDED",
            CrawlTaskStatus::Failed => "FAILED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SourceType {
    Boss,
    Zhilian,
    Wuba,
    Manual,
}

impl SourceType {
    fn as_db(&self) -> &'static str {
        match self {
            SourceType::Boss => "boss",
            SourceType::Zhilian => "zhilian",
            SourceType::Wuba => "wuba",
            SourceType::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CrawlMode {
    Compliant,
    Advanced,
}

impl CrawlMode {
    fn as_db(&self) -> &'static str {
        match self {
            CrawlMode::Compliant => "compliant",
            CrawlMode::Advanced => "advanced",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Job {
    id: i64,
    external_id: Option<String>,
    source: String,
    title: String,
    company: String,
    city: Option<String>,
    salary_k: Option<String>,
    description: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NewJobInput {
    external_id: Option<String>,
    source: Option<SourceType>,
    title: String,
    company: String,
    city: Option<String>,
    salary_k: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Candidate {
    id: i64,
    external_id: Option<String>,
    source: String,
    name: String,
    current_company: Option<String>,
    years_of_experience: f64,
    stage: PipelineStage,
    tags: Vec<String>,
    phone_masked: Option<String>,
    email_masked: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NewCandidateInput {
    external_id: Option<String>,
    source: Option<SourceType>,
    name: String,
    current_company: Option<String>,
    years_of_experience: f64,
    phone: Option<String>,
    email: Option<String>,
    tags: Vec<String>,
    job_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ResumeRecord {
    id: i64,
    candidate_id: i64,
    source: String,
    raw_text: String,
    parsed: Value,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertResumeInput {
    candidate_id: i64,
    source: Option<SourceType>,
    raw_text: String,
    parsed: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DimensionScore {
    key: String,
    score: i32,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceItem {
    dimension: String,
    statement: String,
    source_snippet: String,
}

#[derive(Debug, Clone, Serialize)]
struct AnalysisRecord {
    id: i64,
    candidate_id: i64,
    job_id: Option<i64>,
    overall_score: i32,
    dimension_scores: Vec<DimensionScore>,
    risks: Vec<String>,
    highlights: Vec<String>,
    suggestions: Vec<String>,
    evidence: Vec<EvidenceItem>,
    model_info: Value,
    created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RunAnalysisInput {
    candidate_id: i64,
    job_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct CrawlTask {
    id: i64,
    source: String,
    mode: String,
    task_type: String,
    status: String,
    retry_count: i32,
    error_code: Option<String>,
    payload: Value,
    snapshot: Option<Value>,
    started_at: Option<String>,
    finished_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NewCrawlTaskInput {
    source: SourceType,
    mode: CrawlMode,
    task_type: String,
    payload: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateCrawlTaskInput {
    task_id: i64,
    status: CrawlTaskStatus,
    retry_count: Option<i32>,
    error_code: Option<String>,
    snapshot: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct PipelineEvent {
    id: i64,
    candidate_id: i64,
    job_id: Option<i64>,
    from_stage: PipelineStage,
    to_stage: PipelineStage,
    note: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MoveStageInput {
    candidate_id: i64,
    job_id: Option<i64>,
    to_stage: PipelineStage,
    note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StageStat {
    stage: PipelineStage,
    count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct DashboardMetrics {
    total_jobs: i64,
    total_candidates: i64,
    total_resumes: i64,
    pending_tasks: i64,
    stage_stats: Vec<StageStat>,
}

#[derive(Debug, Clone, Serialize)]
struct SearchHit {
    candidate_id: i64,
    name: String,
    stage: PipelineStage,
    snippet: String,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hash_value(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn normalize_phone(value: &str) -> String {
    value.chars().filter(|char| char.is_ascii_digit()).collect()
}

fn mask_phone(value: &str) -> String {
    let normalized = normalize_phone(value);
    if normalized.len() < 7 {
        return "***".to_string();
    }

    let prefix = &normalized[0..3];
    let suffix = &normalized[normalized.len() - 4..];
    format!("{prefix}****{suffix}")
}

fn mask_email(value: &str) -> String {
    let mut parts = value.split('@');
    let user = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if user.len() <= 2 {
        format!("***@{domain}")
    } else {
        format!("{}***@{domain}", &user[0..2])
    }
}

fn is_valid_transition(from: &str, to: &str) -> bool {
    match from {
        "NEW" => matches!(to, "SCREENING" | "HOLD" | "REJECTED"),
        "SCREENING" => matches!(to, "INTERVIEW" | "HOLD" | "REJECTED"),
        "INTERVIEW" => matches!(to, "HOLD" | "REJECTED" | "OFFERED"),
        "HOLD" => matches!(to, "SCREENING" | "INTERVIEW" | "REJECTED"),
        "REJECTED" => false,
        "OFFERED" => false,
        _ => false,
    }
}

fn open_connection(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    Ok(conn)
}

fn migrate_db(db_path: &Path) -> AppResult<()> {
    let conn = open_connection(db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            external_id TEXT,
            source TEXT NOT NULL,
            title TEXT NOT NULL,
            company TEXT NOT NULL,
            city TEXT,
            salary_k TEXT,
            description TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS candidates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            external_id TEXT,
            source TEXT NOT NULL,
            name TEXT NOT NULL,
            current_company TEXT,
            years_of_experience REAL NOT NULL DEFAULT 0,
            stage TEXT NOT NULL DEFAULT 'NEW',
            phone_enc TEXT,
            phone_hash TEXT,
            email_enc TEXT,
            email_hash TEXT,
            tags_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS applications (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id INTEGER NOT NULL,
            candidate_id INTEGER NOT NULL,
            stage TEXT NOT NULL,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(job_id, candidate_id),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS resumes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL UNIQUE,
            source TEXT NOT NULL,
            raw_text TEXT NOT NULL,
            parsed_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS analysis_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            overall_score INTEGER NOT NULL,
            dimension_scores_json TEXT NOT NULL,
            risks_json TEXT NOT NULL,
            highlights_json TEXT NOT NULL,
            suggestions_json TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            model_info_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS pipeline_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            from_stage TEXT NOT NULL,
            to_stage TEXT NOT NULL,
            note TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS crawl_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            mode TEXT NOT NULL,
            task_type TEXT NOT NULL,
            status TEXT NOT NULL,
            retry_count INTEGER NOT NULL DEFAULT 0,
            error_code TEXT,
            payload_json TEXT NOT NULL,
            snapshot_json TEXT,
            started_at TEXT,
            finished_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS audit_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            action TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_updated_at ON jobs(updated_at);
        CREATE INDEX IF NOT EXISTS idx_candidates_updated_at ON candidates(updated_at);
        CREATE INDEX IF NOT EXISTS idx_candidates_stage ON candidates(stage);
        CREATE INDEX IF NOT EXISTS idx_applications_job_stage ON applications(job_id, stage);
        CREATE INDEX IF NOT EXISTS idx_crawl_tasks_status ON crawl_tasks(status);

        CREATE VIRTUAL TABLE IF NOT EXISTS candidate_search USING fts5(
            candidate_id UNINDEXED,
            name,
            tags,
            raw_text
        );
        "#,
    )?;

    Ok(())
}

fn sync_candidate_search(conn: &Connection, candidate_id: i64) -> AppResult<()> {
    let mut stmt = conn.prepare(
        r#"
        SELECT c.name, c.tags_json, COALESCE(r.raw_text, '')
        FROM candidates c
        LEFT JOIN resumes r ON r.candidate_id = c.id
        WHERE c.id = ?1
        "#,
    )?;

    let (name, tags_json, raw_text): (String, String, String) = stmt.query_row([candidate_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;

    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let tags_text = tags.join(" ");

    conn.execute("DELETE FROM candidate_search WHERE candidate_id = ?1", [candidate_id])?;
    conn.execute(
        "INSERT INTO candidate_search(candidate_id, name, tags, raw_text) VALUES (?1, ?2, ?3, ?4)",
        params![candidate_id, name, tags_text, raw_text],
    )?;

    Ok(())
}

fn write_audit(
    conn: &Connection,
    action: &str,
    entity_type: &str,
    entity_id: Option<String>,
    payload: Value,
) -> AppResult<()> {
    let created_at = now_iso();
    conn.execute(
        "INSERT INTO audit_logs(action, entity_type, entity_id, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![action, entity_type, entity_id, payload.to_string(), created_at],
    )?;
    Ok(())
}

fn candidate_from_row(
    row: &rusqlite::Row<'_>,
    cipher: &FieldCipher,
) -> Result<Candidate, rusqlite::Error> {
    let stage_text: String = row.get("stage")?;
    let stage = PipelineStage::from_db(&stage_text)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err)))?;

    let tags_json: String = row.get("tags_json")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    let phone_masked = row
        .get::<_, Option<String>>("phone_enc")?
        .and_then(|value| cipher.decrypt(&value).ok())
        .map(|value| mask_phone(&value));

    let email_masked = row
        .get::<_, Option<String>>("email_enc")?
        .and_then(|value| cipher.decrypt(&value).ok())
        .map(|value| mask_email(&value));

    Ok(Candidate {
        id: row.get("id")?,
        external_id: row.get("external_id")?,
        source: row.get("source")?,
        name: row.get("name")?,
        current_company: row.get("current_company")?,
        years_of_experience: row.get("years_of_experience")?,
        stage,
        tags,
        phone_masked,
        email_masked,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[tauri::command]
fn create_job(state: State<'_, AppState>, input: NewJobInput) -> Result<Job, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let source = input.source.unwrap_or(SourceType::Manual).as_db().to_string();

    conn.execute(
        r#"
        INSERT INTO jobs(external_id, source, title, company, city, salary_k, description, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            input.external_id,
            source,
            input.title,
            input.company,
            input.city,
            input.salary_k,
            input.description,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let job = conn
        .query_row(
            "SELECT id, external_id, source, title, company, city, salary_k, description, created_at, updated_at FROM jobs WHERE id = ?1",
            [id],
            |row| {
                Ok(Job {
                    id: row.get(0)?,
                    external_id: row.get(1)?,
                    source: row.get(2)?,
                    title: row.get(3)?,
                    company: row.get(4)?,
                    city: row.get(5)?,
                    salary_k: row.get(6)?,
                    description: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "job.create",
        "job",
        Some(job.id.to_string()),
        serde_json::json!({"source": job.source, "title": job.title}),
    )
    .map_err(|error| error.to_string())?;

    Ok(job)
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> Result<Vec<Job>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, external_id, source, title, company, city, salary_k, description, created_at, updated_at FROM jobs ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let jobs = stmt
        .query_map([], |row| {
            Ok(Job {
                id: row.get(0)?,
                external_id: row.get(1)?,
                source: row.get(2)?,
                title: row.get(3)?,
                company: row.get(4)?,
                city: row.get(5)?,
                salary_k: row.get(6)?,
                description: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(jobs)
}

#[tauri::command]
fn create_candidate(state: State<'_, AppState>, input: NewCandidateInput) -> Result<Candidate, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();

    let phone_normalized = input.phone.as_deref().map(normalize_phone);
    let phone_hash = phone_normalized.as_deref().map(hash_value);
    let phone_encrypted = phone_normalized
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let email_hash = input.email.as_deref().map(hash_value);
    let email_encrypted = input
        .email
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let tags_json = serde_json::to_string(&input.tags).map_err(|error| error.to_string())?;

    conn.execute(
        r#"
        INSERT INTO candidates(
            external_id, source, name, current_company, years_of_experience, stage,
            phone_enc, phone_hash, email_enc, email_hash, tags_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'NEW', ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            input.external_id,
            input.source.unwrap_or(SourceType::Manual).as_db(),
            input.name,
            input.current_company,
            input.years_of_experience,
            phone_encrypted,
            phone_hash,
            email_encrypted,
            email_hash,
            tags_json,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let candidate_id = conn.last_insert_rowid();

    if let Some(job_id) = input.job_id {
        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, 'NEW', NULL, ?3, ?4)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET updated_at = excluded.updated_at
            "#,
            params![job_id, candidate_id, now, now],
        )
        .map_err(|error| error.to_string())?;
    }

    sync_candidate_search(&conn, candidate_id).map_err(|error| error.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, external_id, source, name, current_company, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE id = ?1",
        )
        .map_err(|error| error.to_string())?;

    let candidate = stmt
        .query_row([candidate_id], |row| candidate_from_row(row, &state.cipher))
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "candidate.create",
        "candidate",
        Some(candidate.id.to_string()),
        serde_json::json!({"source": candidate.source, "tags": candidate.tags}),
    )
    .map_err(|error| error.to_string())?;

    Ok(candidate)
}

#[tauri::command]
fn list_candidates(
    state: State<'_, AppState>,
    stage: Option<PipelineStage>,
) -> Result<Vec<Candidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    if let Some(filter_stage) = stage {
        let mut stmt = conn
            .prepare(
                "SELECT id, external_id, source, name, current_company, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE stage = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([filter_stage.as_db()], |row| {
                candidate_from_row(row, &state.cipher)
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, external_id, source, name, current_company, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates ORDER BY updated_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| candidate_from_row(row, &state.cipher))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn move_candidate_stage(state: State<'_, AppState>, input: MoveStageInput) -> Result<PipelineEvent, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let current_stage_text: String = conn
        .query_row(
            "SELECT stage FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    if !is_valid_transition(&current_stage_text, input.to_stage.as_db()) {
        return Err(
            AppError::InvalidTransition {
                from: current_stage_text,
                to: input.to_stage.as_db().to_string(),
            }
            .to_string(),
        );
    }

    let now = now_iso();
    conn.execute(
        "UPDATE candidates SET stage = ?1, updated_at = ?2 WHERE id = ?3",
        params![input.to_stage.as_db(), now, input.candidate_id],
    )
    .map_err(|error| error.to_string())?;

    if let Some(job_id) = input.job_id {
        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET stage = excluded.stage, notes = excluded.notes, updated_at = excluded.updated_at
            "#,
            params![
                job_id,
                input.candidate_id,
                input.to_stage.as_db(),
                input.note,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute(
        r#"
        INSERT INTO pipeline_events(candidate_id, job_id, from_stage, to_stage, note, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            input.candidate_id,
            input.job_id,
            current_stage_text,
            input.to_stage.as_db(),
            input.note,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let event_id = conn.last_insert_rowid();
    let event = conn
        .query_row(
            "SELECT id, candidate_id, job_id, from_stage, to_stage, note, created_at FROM pipeline_events WHERE id = ?1",
            [event_id],
            |row| {
                let from_stage_text: String = row.get(3)?;
                let to_stage_text: String = row.get(4)?;
                Ok(PipelineEvent {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    job_id: row.get(2)?,
                    from_stage: PipelineStage::from_db(&from_stage_text).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                    to_stage: PipelineStage::from_db(&to_stage_text).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                    note: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "candidate.stage.move",
        "candidate",
        Some(input.candidate_id.to_string()),
        serde_json::json!({"toStage": input.to_stage, "jobId": input.job_id}),
    )
    .map_err(|error| error.to_string())?;

    Ok(event)
}

#[tauri::command]
fn list_pipeline_events(state: State<'_, AppState>, candidate_id: i64) -> Result<Vec<PipelineEvent>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, candidate_id, job_id, from_stage, to_stage, note, created_at FROM pipeline_events WHERE candidate_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let from_stage_text: String = row.get(3)?;
            let to_stage_text: String = row.get(4)?;
            Ok(PipelineEvent {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                from_stage: PipelineStage::from_db(&from_stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                to_stage: PipelineStage::from_db(&to_stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                note: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn upsert_resume(state: State<'_, AppState>, input: UpsertResumeInput) -> Result<ResumeRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let source = input.source.unwrap_or(SourceType::Manual).as_db().to_string();
    let parsed_json = input.parsed.to_string();

    conn.execute(
        r#"
        INSERT INTO resumes(candidate_id, source, raw_text, parsed_json, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(candidate_id)
        DO UPDATE SET source = excluded.source, raw_text = excluded.raw_text, parsed_json = excluded.parsed_json, updated_at = excluded.updated_at
        "#,
        params![
            input.candidate_id,
            source,
            input.raw_text,
            parsed_json,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    sync_candidate_search(&conn, input.candidate_id).map_err(|error| error.to_string())?;

    let record = conn
        .query_row(
            "SELECT id, candidate_id, source, raw_text, parsed_json, created_at, updated_at FROM resumes WHERE candidate_id = ?1",
            [input.candidate_id],
            |row| {
                let parsed_text: String = row.get(4)?;
                let parsed = serde_json::from_str(&parsed_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
                Ok(ResumeRecord {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    source: row.get(2)?,
                    raw_text: row.get(3)?,
                    parsed,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "resume.upsert",
        "resume",
        Some(record.id.to_string()),
        serde_json::json!({"candidateId": record.candidate_id, "source": record.source}),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

fn parse_skills(parsed: &Value) -> Vec<String> {
    parsed
        .get("skills")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|value| value.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn clamp_score(value: i32) -> i32 {
    value.clamp(0, 100)
}

#[tauri::command]
fn run_candidate_analysis(
    state: State<'_, AppState>,
    input: RunAnalysisInput,
) -> Result<AnalysisRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let candidate = conn
        .query_row(
            "SELECT id, years_of_experience, stage, tags_json FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| {
                let tags_json: String = row.get(3)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    tags,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let resume_row: (String, Value) = conn
        .query_row(
            "SELECT raw_text, parsed_json FROM resumes WHERE candidate_id = ?1",
            [input.candidate_id],
            |row| {
                let parsed_json_text: String = row.get(1)?;
                let parsed_json = serde_json::from_str(&parsed_json_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
                Ok((row.get(0)?, parsed_json))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Resume required before analysis".to_string())?;

    let mut required_skills: Vec<String> = Vec::new();
    let mut max_salary: Option<f64> = None;
    let mut min_years: f64 = 0.0;

    if let Some(job_id) = input.job_id {
        if let Some((description, salary_k)) = conn
            .query_row(
                "SELECT description, salary_k FROM jobs WHERE id = ?1",
                [job_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            if let Some(description_text) = description {
                required_skills = description_text
                    .split(|char: char| !char.is_alphanumeric() && char != '+')
                    .filter(|token| token.len() >= 3)
                    .take(8)
                    .map(|token| token.to_lowercase())
                    .collect();
            }

            if let Some(salary_text) = salary_k {
                let numeric = salary_text
                    .split('-')
                    .last()
                    .and_then(|item| item.parse::<f64>().ok());
                max_salary = numeric;
            }
        }
    }

    let skills = parse_skills(&resume_row.1);
    let normalized_skills: Vec<String> = skills.iter().map(|skill| skill.to_lowercase()).collect();

    let matched = required_skills
        .iter()
        .filter(|required| normalized_skills.iter().any(|owned| owned.contains(*required)))
        .count() as i32;

    let skill_score = if required_skills.is_empty() {
        75
    } else {
        clamp_score((matched * 100) / required_skills.len() as i32)
    };

    let experience_score = clamp_score((candidate.1 * 12.0) as i32 + 20);
    min_years = min_years.max((required_skills.len() as f64 / 2.0).floor());

    let compensation_score = if let Some(max) = max_salary {
        let expected = resume_row
            .1
            .get("expectedSalaryK")
            .and_then(|value| value.as_f64())
            .unwrap_or(max - 5.0);
        clamp_score((80.0 + (max - expected) * 3.0) as i32)
    } else {
        75
    };

    let stability_score = clamp_score(60 + (candidate.1 / 2.0 * 10.0) as i32);

    let dimension_scores = vec![
        DimensionScore {
            key: "skill_match".to_string(),
            score: skill_score,
            reason: format!(
                "Matched {} out of {} extracted role keywords.",
                matched,
                required_skills.len()
            ),
        },
        DimensionScore {
            key: "experience".to_string(),
            score: experience_score,
            reason: format!(
                "Candidate experience {:.1} years, role baseline {:.1} years.",
                candidate.1, min_years
            ),
        },
        DimensionScore {
            key: "compensation".to_string(),
            score: compensation_score,
            reason: "Compensation fit estimated from available profile fields.".to_string(),
        },
        DimensionScore {
            key: "stability".to_string(),
            score: stability_score,
            reason: format!("Current stage {} with {} profile tags.", candidate.2, candidate.3.len()),
        },
    ];

    let overall_score = clamp_score(
        (dimension_scores[0].score as f64 * 0.4
            + dimension_scores[1].score as f64 * 0.25
            + dimension_scores[2].score as f64 * 0.15
            + dimension_scores[3].score as f64 * 0.2)
            .round() as i32,
    );

    let mut risks = Vec::<String>::new();
    if skill_score < 60 {
        risks.push("核心技能覆盖不足，建议补充技术验证。".to_string());
    }
    if compensation_score < 60 {
        risks.push("薪资期望与岗位预算可能存在偏差。".to_string());
    }

    let mut highlights = Vec::<String>::new();
    if skill_score >= 70 {
        highlights.push("技能匹配度较高，可进入技术面。".to_string());
    }
    if experience_score >= 75 {
        highlights.push("工作年限满足岗位要求。".to_string());
    }

    let suggestions = if risks.is_empty() {
        vec!["建议尽快安排首轮面试，验证业务场景适配度。".to_string()]
    } else {
        vec!["面试中重点核实风险项，并追加结构化评分。".to_string()]
    };

    let evidence = vec![
        EvidenceItem {
            dimension: "skill_match".to_string(),
            statement: format!("Skills extracted: {}", skills.join(", ")),
            source_snippet: resume_row.0.chars().take(140).collect(),
        },
        EvidenceItem {
            dimension: "experience".to_string(),
            statement: format!("Years of experience: {:.1}", candidate.1),
            source_snippet: resume_row.0.chars().take(140).collect(),
        },
    ];

    let model_info = serde_json::json!({
        "provider": "cloud-mock",
        "model": "gpt-style-compat",
        "generatedAt": now_iso(),
    });

    let created_at = now_iso();
    conn.execute(
        r#"
        INSERT INTO analysis_results(
            candidate_id, job_id, overall_score, dimension_scores_json,
            risks_json, highlights_json, suggestions_json, evidence_json,
            model_info_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            input.candidate_id,
            input.job_id,
            overall_score,
            serde_json::to_string(&dimension_scores).map_err(|error| error.to_string())?,
            serde_json::to_string(&risks).map_err(|error| error.to_string())?,
            serde_json::to_string(&highlights).map_err(|error| error.to_string())?,
            serde_json::to_string(&suggestions).map_err(|error| error.to_string())?,
            serde_json::to_string(&evidence).map_err(|error| error.to_string())?,
            model_info.to_string(),
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();

    let result = AnalysisRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: input.job_id,
        overall_score,
        dimension_scores,
        risks,
        highlights,
        suggestions,
        evidence,
        model_info,
        created_at,
    };

    write_audit(
        &conn,
        "analysis.run",
        "analysis_result",
        Some(result.id.to_string()),
        serde_json::json!({"candidateId": input.candidate_id, "jobId": input.job_id}),
    )
    .map_err(|error| error.to_string())?;

    Ok(result)
}

#[tauri::command]
fn list_analysis(state: State<'_, AppState>, candidate_id: i64) -> Result<Vec<AnalysisRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, overall_score, dimension_scores_json,
                   risks_json, highlights_json, suggestions_json, evidence_json,
                   model_info_json, created_at
            FROM analysis_results
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let parse_vec = |index: usize| -> Result<Vec<String>, rusqlite::Error> {
                let text: String = row.get(index)?;
                serde_json::from_str(&text).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        index,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            };

            let dimension_text: String = row.get(4)?;
            let evidence_text: String = row.get(8)?;
            let model_info_text: String = row.get(9)?;

            let dimension_scores = serde_json::from_str(&dimension_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let evidence = serde_json::from_str(&evidence_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let model_info = serde_json::from_str(&model_info_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;

            Ok(AnalysisRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                overall_score: row.get(3)?,
                dimension_scores,
                risks: parse_vec(5)?,
                highlights: parse_vec(6)?,
                suggestions: parse_vec(7)?,
                evidence,
                model_info,
                created_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_crawl_task(state: State<'_, AppState>, input: NewCrawlTaskInput) -> Result<CrawlTask, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO crawl_tasks(source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at)
        VALUES (?1, ?2, ?3, 'PENDING', 0, NULL, ?4, NULL, NULL, NULL, ?5, ?6)
        "#,
        params![
            input.source.as_db(),
            input.mode.as_db(),
            input.task_type,
            input.payload.to_string(),
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();

    let task = conn
        .query_row(
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at FROM crawl_tasks WHERE id = ?1",
            [id],
            |row| {
                let payload_text: String = row.get(7)?;
                let snapshot_text: Option<String> = row.get(8)?;
                Ok(CrawlTask {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    mode: row.get(2)?,
                    task_type: row.get(3)?,
                    status: row.get(4)?,
                    retry_count: row.get(5)?,
                    error_code: row.get(6)?,
                    payload: serde_json::from_str(&payload_text).unwrap_or(Value::Null),
                    snapshot: snapshot_text.and_then(|value| serde_json::from_str(&value).ok()),
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "crawl_task.create",
        "crawl_task",
        Some(task.id.to_string()),
        serde_json::json!({"source": task.source, "mode": task.mode, "taskType": task.task_type}),
    )
    .map_err(|error| error.to_string())?;

    Ok(task)
}

#[tauri::command]
fn update_crawl_task(state: State<'_, AppState>, input: UpdateCrawlTaskInput) -> Result<CrawlTask, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();

    let started_at: Option<String> = if matches!(input.status, CrawlTaskStatus::Running) {
        Some(now.clone())
    } else {
        conn.query_row(
            "SELECT started_at FROM crawl_tasks WHERE id = ?1",
            [input.task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten()
    };

    let finished_at = if matches!(input.status, CrawlTaskStatus::Failed | CrawlTaskStatus::Succeeded) {
        Some(now.clone())
    } else {
        None
    };

    conn.execute(
        r#"
        UPDATE crawl_tasks
        SET status = ?1,
            retry_count = COALESCE(?2, retry_count),
            error_code = ?3,
            snapshot_json = ?4,
            started_at = COALESCE(?5, started_at),
            finished_at = COALESCE(?6, finished_at),
            updated_at = ?7
        WHERE id = ?8
        "#,
        params![
            input.status.as_db(),
            input.retry_count,
            input.error_code,
            input.snapshot.map(|value| value.to_string()),
            started_at,
            finished_at,
            now,
            input.task_id,
        ],
    )
    .map_err(|error| error.to_string())?;

    let task = conn
        .query_row(
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at FROM crawl_tasks WHERE id = ?1",
            [input.task_id],
            |row| {
                let payload_text: String = row.get(7)?;
                let snapshot_text: Option<String> = row.get(8)?;
                Ok(CrawlTask {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    mode: row.get(2)?,
                    task_type: row.get(3)?,
                    status: row.get(4)?,
                    retry_count: row.get(5)?,
                    error_code: row.get(6)?,
                    payload: serde_json::from_str(&payload_text).unwrap_or(Value::Null),
                    snapshot: snapshot_text.and_then(|value| serde_json::from_str(&value).ok()),
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "crawl_task.update",
        "crawl_task",
        Some(task.id.to_string()),
        serde_json::json!({"status": task.status, "retryCount": task.retry_count, "errorCode": task.error_code}),
    )
    .map_err(|error| error.to_string())?;

    Ok(task)
}

#[tauri::command]
fn list_crawl_tasks(state: State<'_, AppState>) -> Result<Vec<CrawlTask>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at FROM crawl_tasks ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let payload_text: String = row.get(7)?;
            let snapshot_text: Option<String> = row.get(8)?;
            Ok(CrawlTask {
                id: row.get(0)?,
                source: row.get(1)?,
                mode: row.get(2)?,
                task_type: row.get(3)?,
                status: row.get(4)?,
                retry_count: row.get(5)?,
                error_code: row.get(6)?,
                payload: serde_json::from_str(&payload_text).unwrap_or(Value::Null),
                snapshot: snapshot_text.and_then(|value| serde_json::from_str(&value).ok()),
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn search_candidates(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let normalized_query = query.trim();
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            r#"
            SELECT c.id, c.name, c.stage, snippet(candidate_search, 3, '<b>', '</b>', '…', 10)
            FROM candidate_search
            JOIN candidates c ON c.id = candidate_search.candidate_id
            WHERE candidate_search MATCH ?1
            ORDER BY rank
            LIMIT 50
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([normalized_query], |row| {
            let stage_text: String = row.get(2)?;
            Ok(SearchHit {
                candidate_id: row.get(0)?,
                name: row.get(1)?,
                stage: PipelineStage::from_db(&stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                snippet: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn dashboard_metrics(state: State<'_, AppState>) -> Result<DashboardMetrics, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let total_jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let total_candidates: i64 = conn
        .query_row("SELECT COUNT(*) FROM candidates", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let total_resumes: i64 = conn
        .query_row("SELECT COUNT(*) FROM resumes", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let pending_tasks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM crawl_tasks WHERE status IN ('PENDING', 'RUNNING')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    let mut stage_stmt = conn
        .prepare("SELECT stage, COUNT(*) FROM candidates GROUP BY stage")
        .map_err(|error| error.to_string())?;
    let stage_rows = stage_stmt
        .query_map([], |row| {
            let stage_text: String = row.get(0)?;
            Ok(StageStat {
                stage: PipelineStage::from_db(&stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                count: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let stage_stats = stage_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(DashboardMetrics {
        total_jobs,
        total_candidates,
        total_resumes,
        pending_tasks,
        stage_stats,
    })
}

#[tauri::command]
fn app_health(state: State<'_, AppState>) -> Result<Value, String> {
    let db_exists = state.db_path.exists();
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let schema_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "dbPath": state.db_path,
        "dbExists": db_exists,
        "schemaVersion": schema_version,
    }))
}

fn resolve_db_path(app: &AppHandle) -> AppResult<PathBuf> {
    let data_dir = app.path().app_data_dir().map_err(|_| AppError::NotFound("app_data_dir".to_string()))?;
    fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("doss.sqlite3"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db_path = resolve_db_path(app.handle())?;
            migrate_db(&db_path)?;

            let seed = std::env::var("DOSS_LOCAL_KEY").unwrap_or_else(|_| {
                format!(
                    "{}:{}",
                    app.config().identifier,
                    app.package_info().version.to_string()
                )
            });

            app.manage(AppState::new(db_path, &seed));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_health,
            create_job,
            list_jobs,
            create_candidate,
            list_candidates,
            move_candidate_stage,
            list_pipeline_events,
            upsert_resume,
            run_candidate_analysis,
            list_analysis,
            create_crawl_task,
            update_crawl_task,
            list_crawl_tasks,
            search_candidates,
            dashboard_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_transition_rules_are_enforced() {
        assert!(is_valid_transition("NEW", "SCREENING"));
        assert!(!is_valid_transition("NEW", "OFFERED"));
    }

    #[test]
    fn field_cipher_roundtrip_works() {
        let cipher = FieldCipher::from_seed("unit-test-seed");
        let encrypted = cipher.encrypt("13800000000").expect("encrypt");
        let decrypted = cipher.decrypt(&encrypted).expect("decrypt");
        assert_eq!(decrypted, "13800000000");
    }
}
